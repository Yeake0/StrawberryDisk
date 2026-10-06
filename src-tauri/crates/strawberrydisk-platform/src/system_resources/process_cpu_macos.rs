//! Native counters with a bounded system-tool fallback for other users' processes.
use super::process_cpu::{ProcessCpuCounter, ProcessCpuSnapshot};
use crate::{
    run_controlled_command_with_log_policy, ControlledCommandLimits, ControlledCommandLogPolicy,
    ControlledEnvironmentPolicy, ControlledExecutable,
};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn fill_unreadable(mut snapshot: ProcessCpuSnapshot) -> ProcessCpuSnapshot {
    if snapshot.processes.iter().any(|p| p.cpu_time_ms.is_none()) {
        recover_unreadable(&mut snapshot.processes);
    }
    // ps omits the kernel task. Keep its absence explicit rather than fabricate a CPU value.
    if !snapshot.processes.iter().any(|p| p.pid == 0) {
        snapshot.processes.push(ProcessCpuCounter {
            pid: 0,
            started_at: 0,
            name: "kernel_task".into(),
            executable: None,
            location_status: super::process_cpu::ProcessLocationStatus::Unavailable,
            cpu_time_ms: None,
        });
    }
    let applications = super::application_metadata_macos::read();
    for process in &mut snapshot.processes {
        if let Some(name) = applications
            .get(&process.pid)
            .and_then(|application| application.name.as_ref())
        {
            process.name.clone_from(name);
        }
    }
    snapshot
}

fn recover_unreadable(processes: &mut [ProcessCpuCounter]) {
    // Apple's setuid ps reads task counters for other users without a new helper or prompt.
    // Read cumulative time rather than ps %cpu, which is a lifetime average, not an interval.
    let pids = processes
        .iter()
        .filter(|p| p.cpu_time_ms.is_none() && p.pid != 0)
        .map(|p| p.pid.to_string())
        .collect::<Vec<_>>()
        .join(",");
    if pids.is_empty() {
        return;
    }
    let output = ControlledExecutable::capture(Path::new("/bin/ps")).and_then(|executable| {
        run_controlled_command_with_log_policy(
            "macos_process_cpu_fallback",
            &executable,
            &["-p", &pids, "-o", "pid=,lstart=,time=,comm="],
            ControlledEnvironmentPolicy::Isolated,
            ControlledCommandLimits {
                timeout: Duration::from_millis(500),
                stdout_bytes: 1024 * 1024,
                stderr_bytes: 4096,
            },
            ControlledCommandLogPolicy::ExceptionalOnly,
            &|| false,
        )
    });
    if let Ok(output) = output {
        if !output.status.success() {
            return;
        }
        let times = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(parse_row)
            .collect::<HashMap<_, _>>();
        for p in processes {
            if p.cpu_time_ms.is_some() {
                continue;
            }
            if let Some(row) = times.get(&p.pid) {
                p.started_at = row.started_at;
                p.cpu_time_ms = Some(row.cpu_time_ms);
                if p.executable.is_none() {
                    p.executable.clone_from(&row.executable);
                }
                if p.name.is_empty() {
                    p.name = row
                        .executable
                        .as_deref()
                        .and_then(Path::file_name)
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                }
            }
        }
    }
}

struct PsCounter {
    started_at: u64,
    cpu_time_ms: u64,
    executable: Option<PathBuf>,
}
fn parse_row(line: &str) -> Option<(u32, PsCounter)> {
    let mut remaining = line;
    let pid = take_token(&mut remaining)?.parse::<u32>().ok()?;
    // lstart is stable across samples, including inaccessible BSD start times. Its one-second
    // resolution is the system tool's limit; the native path retains subsecond process identity.
    let start = (0..5)
        .map(|_| take_token(&mut remaining))
        .collect::<Option<Vec<_>>>()?;
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    start.hash(&mut identity);
    let cpu_time_ms = parse_cpu_time(take_token(&mut remaining)?)?;
    let executable = PathBuf::from(remaining.trim());
    if remaining.trim().is_empty() {
        return None;
    }
    let executable = executable.is_absolute().then_some(executable);
    Some((
        pid,
        PsCounter {
            started_at: identity.finish(),
            cpu_time_ms,
            executable,
        },
    ))
}
fn take_token<'a>(remaining: &mut &'a str) -> Option<&'a str> {
    let trimmed = remaining.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    *remaining = &trimmed[end..];
    Some(&trimmed[..end])
}

fn parse_cpu_time(value: &str) -> Option<u64> {
    let (minutes, tail) = value.split_once(':')?;
    let (seconds, centiseconds) = tail.split_once('.')?;
    if centiseconds.len() != 2 {
        return None;
    }
    let seconds = seconds.parse::<u64>().ok()?;
    let fraction = centiseconds.parse::<u64>().ok()?;
    if seconds >= 60 || fraction >= 100 {
        return None;
    }
    minutes
        .parse::<u64>()
        .ok()?
        .checked_mul(60_000)?
        .checked_add(seconds * 1000 + fraction * 10)
}

pub(super) fn read_usage(pid: u32) -> Option<libc::rusage_info_v2> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // rusage CPU times use Mach ticks, including on Apple silicon; convert with the native timebase.
    // Do not turn a permission failure or an exited process into an idle reading.
    let result = unsafe {
        libc::proc_pid_rusage(pid as i32, libc::RUSAGE_INFO_V2, usage.as_mut_ptr().cast())
    };
    if result != 0 {
        return None;
    }
    Some(unsafe { usage.assume_init() })
}

pub(super) fn cpu_time_ms(usage: &libc::rusage_info_v2) -> Option<u64> {
    static TIMEBASE: std::sync::OnceLock<Option<(u32, u32)>> = std::sync::OnceLock::new();
    let (numer, denom) = (*TIMEBASE.get_or_init(|| {
        #[repr(C)]
        struct Timebase {
            numer: u32,
            denom: u32,
        }
        unsafe extern "C" {
            fn mach_timebase_info(info: *mut Timebase) -> i32;
        }
        let mut info = std::mem::MaybeUninit::<Timebase>::zeroed();
        if unsafe { mach_timebase_info(info.as_mut_ptr()) } != 0 {
            return None;
        }
        let info = unsafe { info.assume_init() };
        (info.denom > 0).then_some((info.numer, info.denom))
    }))?;
    let cpu_time_ms = ((u128::from(usage.ri_user_time) + u128::from(usage.ri_system_time))
        * u128::from(numer)
        / u128::from(denom)
        / 1_000_000)
        .min(u128::from(u64::MAX)) as u64;
    Some(cpu_time_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ps_time_uses_cumulative_centiseconds_and_preserves_large_totals() {
        assert_eq!(parse_cpu_time("1418:21.60"), Some(85_101_600));
        assert_eq!(parse_cpu_time("0:00.01"), Some(10));
        for invalid in [
            "5.5",
            "0:60.00",
            "0:00.000",
            "x:00.00",
            "18446744073709551615:01.00",
        ] {
            assert_eq!(parse_cpu_time(invalid), None);
        }
    }
    #[test]
    fn ps_identity_is_stable_and_changes_when_a_pid_is_reused() {
        let (pid, row) =
            parse_row("164 Wed Sep 30 16:48:51 2026 228:35.58 /System/Window  Server").unwrap();
        assert_eq!(pid, 164);
        assert_eq!(row.cpu_time_ms, 13_715_580);
        assert_eq!(
            row.executable.as_deref(),
            Some(Path::new("/System/Window  Server"))
        );
        let (_, again) =
            parse_row("164 Wed Sep 30 16:48:51 2026 228:36.58 /System/Window  Server").unwrap();
        assert_eq!(row.started_at, again.started_at);
        let (_, reused) =
            parse_row("164 Wed Sep 30 16:48:52 2026 0:00.00 /System/Window  Server").unwrap();
        assert_ne!(row.started_at, reused.started_at);
        assert!(parse_row("164 Wed Sep 30 16:48:51 2026 invalid /System/WindowServer").is_none());
        let (_, counter) =
            parse_row("164 Wed Sep 30 16:48:51 2026 0:01.00 system-process").unwrap();
        assert_eq!(counter.cpu_time_ms, 1000);
        assert!(counter.executable.is_none());
    }
    #[test]
    fn system_tool_fallback_recovers_other_user_process_counters() {
        let mut sampler = super::super::process_cpu::ProcessCpuSampler::default();
        use super::super::process_cpu::ProcessCpuSource;
        let value = sampler.sample().unwrap();
        assert!(
            value
                .processes
                .iter()
                .any(|p| p.pid == 1 && p.cpu_time_ms.is_some()),
            "launchd cumulative time should be readable through the system tool"
        );
        assert!(
            value
                .processes
                .iter()
                .any(|p| p.pid == 0 && p.cpu_time_ms.is_none()),
            "kernel task must remain explicitly unavailable"
        );
    }
}
