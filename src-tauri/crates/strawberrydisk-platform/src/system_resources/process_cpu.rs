//! Minimal cumulative process counters; cadence and application grouping belong to callers.
use crate::{PlatformError, PlatformErrorCode, PlatformResult};
use std::path::PathBuf;
use sysinfo::{CpuRefreshKind, RefreshKind, System};
#[cfg(not(target_os = "macos"))]
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

#[derive(Debug, Clone)]
pub struct ProcessCpuCounter {
    pub location_status: ProcessLocationStatus,
    pub pid: u32,
    pub started_at: u64,
    pub name: String,
    pub executable: Option<PathBuf>,
    pub cpu_time_ms: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessLocationStatus {
    Available,
    Denied,
    Exited,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CpuUsageScale {
    SingleCore,
    TotalCapacity,
}

#[derive(Debug, Clone)]
pub struct ProcessCpuSnapshot {
    pub logical_cpu_count: usize,
    pub usage_scale: CpuUsageScale,
    pub processes: Vec<ProcessCpuCounter>,
}
pub trait ProcessCpuSource: Send {
    fn sample(&mut self) -> PlatformResult<ProcessCpuSnapshot>;
}
pub struct ProcessCpuSampler {
    system: System,
    #[cfg(target_os = "macos")]
    macos: super::process_snapshot_macos::ProcessSnapshotReader,
    #[cfg(windows)]
    windows: super::process_snapshot_windows::ProcessSnapshotReader,
}
impl Default for ProcessCpuSampler {
    fn default() -> Self {
        Self {
            system: System::new_with_specifics(
                RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()),
            ),
            #[cfg(windows)]
            windows: Default::default(),
            #[cfg(target_os = "macos")]
            macos: Default::default(),
        }
    }
}
impl ProcessCpuSource for ProcessCpuSampler {
    fn sample(&mut self) -> PlatformResult<ProcessCpuSnapshot> {
        let logical_cpu_count = self.system.cpus().len();
        if logical_cpu_count == 0 {
            return Err(PlatformError::new(
                PlatformErrorCode::OperationFailed,
                "logical CPU count is unavailable",
            ));
        }
        #[cfg(target_os = "macos")]
        return Ok(super::process_cpu_macos::fill_unreadable(
            ProcessCpuSnapshot {
                logical_cpu_count,
                usage_scale: CpuUsageScale::SingleCore,
                processes: self
                    .macos
                    .read()?
                    .into_iter()
                    .map(|row| row.counter)
                    .collect(),
            },
        ));
        #[cfg(not(target_os = "macos"))]
        self.sample_other(logical_cpu_count)
    }
}

impl ProcessCpuSampler {
    #[cfg(not(target_os = "macos"))]
    fn sample_other(&mut self, logical_cpu_count: usize) -> PlatformResult<ProcessCpuSnapshot> {
        #[cfg(windows)]
        if let Some(processes) = self.windows.read() {
            return Ok(ProcessCpuSnapshot {
                logical_cpu_count,
                usage_scale: CpuUsageScale::TotalCapacity,
                processes: processes.into_iter().map(|row| row.counter).collect(),
            });
        }
        // Never collect memory, command lines, environment, or disk I/O for the CPU list.
        let refresh = ProcessRefreshKind::nothing()
            // Process counters already include their threads; avoid duplicate task rows.
            .without_tasks()
            .with_exe(UpdateKind::OnlyIfNotSet);
        #[cfg(not(any(target_os = "macos", windows)))]
        let refresh = refresh.with_cpu();
        let updated =
            self.system
                .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
        if updated == 0 {
            return Err(PlatformError::new(
                PlatformErrorCode::OperationFailed,
                "process CPU counters are unavailable",
            ));
        }
        let snapshot = ProcessCpuSnapshot {
            logical_cpu_count,
            usage_scale: if cfg!(target_os = "macos") {
                CpuUsageScale::SingleCore
            } else {
                CpuUsageScale::TotalCapacity
            },
            processes: self
                .system
                .processes()
                .iter()
                // Windows PID 0 measures idle capacity, not application work.
                .filter(|(pid, _)| !cfg!(windows) || pid.as_u32() != 0)
                .map(|(pid, p)| {
                    let counter = counters(pid.as_u32(), p);
                    ProcessCpuCounter {
                        pid: pid.as_u32(),
                        started_at: counter
                            .map(|(started, _)| started)
                            .unwrap_or(p.start_time()),
                        name: p.name().to_string_lossy().into_owned(),
                        executable: p.exe().map(PathBuf::from),
                        location_status: if p.exe().is_some() {
                            ProcessLocationStatus::Available
                        } else {
                            ProcessLocationStatus::Unavailable
                        },
                        cpu_time_ms: counter.map(|(_, time)| time),
                    }
                })
                .collect(),
        };
        Ok(snapshot)
    }
}

#[cfg(windows)]
fn counters(pid: u32, _process: &sysinfo::Process) -> Option<(u64, u64)> {
    public_windows_counters(pid)
}

#[cfg(windows)]
pub(super) fn public_windows_counters(pid: u32) -> Option<(u64, u64)> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    // Limited query access is enough; never request termination or elevation rights.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let result = windows_counters_for_handle(handle);
    unsafe {
        CloseHandle(handle);
    }
    result
}

#[cfg(windows)]
pub(super) fn windows_counters_for_handle(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Option<(u64, u64)> {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetProcessTimes};
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    let read =
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    if read == 0 {
        return None;
    }
    let ticks =
        |value: FILETIME| u64::from(value.dwHighDateTime) << 32 | u64::from(value.dwLowDateTime);
    Some((
        ticks(created),
        ticks(kernel).saturating_add(ticks(user)) / 10_000,
    ))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn counters(_pid: u32, process: &sysinfo::Process) -> Option<(u64, u64)> {
    Some((process.start_time(), process.accumulated_cpu_time()))
}
