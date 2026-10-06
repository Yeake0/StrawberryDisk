//! Minimal native process enumeration with shared, lifetime-scoped executable metadata.
use super::process_cpu::{ProcessCpuCounter, ProcessLocationStatus};
use crate::{PlatformError, PlatformErrorCode, PlatformResult};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};

const MAX_PROCESSES: usize = 65_536;
const PID_MARGIN: usize = 256;

pub(super) struct ProcessReading {
    pub counter: ProcessCpuCounter,
    pub footprint: Option<u64>,
}

#[derive(Default)]
pub(super) struct ProcessSnapshotReader {
    pids: Vec<libc::pid_t>,
    metadata: Option<Arc<Mutex<MetadataCache>>>,
}

impl ProcessSnapshotReader {
    pub(super) fn read(&mut self) -> PlatformResult<Vec<ProcessReading>> {
        let count = self.enumerate()?;
        let mut rows = Vec::with_capacity(count);
        let mut identities = Vec::with_capacity(count);
        for &pid in &self.pids[..count] {
            if pid <= 0 {
                continue;
            }
            let usage = super::process_cpu_macos::read_usage(pid as u32);
            let identity = usage.as_ref().map(ProcessIdentity::from_usage);
            identities.push((pid as u32, identity));
            rows.push(ProcessReading {
                footprint: usage.as_ref().map(|usage| usage.ri_phys_footprint),
                counter: ProcessCpuCounter {
                    pid: pid as u32,
                    started_at: identity.map_or(0, |identity| identity.started_at),
                    name: String::new(),
                    executable: None,
                    location_status: ProcessLocationStatus::Unavailable,
                    cpu_time_ms: usage
                        .as_ref()
                        .and_then(super::process_cpu_macos::cpu_time_ms),
                },
            });
        }
        let cache = self.metadata.get_or_insert_with(|| {
            static REGISTRY: OnceLock<MetadataRegistry> = OnceLock::new();
            REGISTRY.get_or_init(Default::default).acquire()
        });
        cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .attach(&mut rows, &identities, Instant::now(), read_metadata);
        Ok(rows)
    }

    fn enumerate(&mut self) -> PlatformResult<usize> {
        if self.pids.is_empty() {
            let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
            if count <= 0 {
                return Err(enumeration_error());
            }
            self.grow(count as usize + PID_MARGIN)?;
        }
        for _ in 0..3 {
            let count = unsafe {
                libc::proc_listallpids(
                    self.pids.as_mut_ptr().cast(),
                    (self.pids.len() * std::mem::size_of::<libc::pid_t>()) as i32,
                )
            };
            if count <= 0 {
                return Err(enumeration_error());
            }
            if (count as usize) < self.pids.len() {
                return Ok(count as usize);
            }
            self.grow(self.pids.len().saturating_mul(2))?;
        }
        Err(PlatformError::operation_failed(
            "macOS process enumeration exceeded retry limit",
        ))
    }

    fn grow(&mut self, requested: usize) -> PlatformResult<()> {
        if requested > MAX_PROCESSES || requested <= self.pids.len() {
            return Err(PlatformError::operation_failed(
                "macOS process enumeration exceeded capacity limit",
            ));
        }
        self.pids
            .try_reserve_exact(requested - self.pids.len())
            .map_err(|error| {
                PlatformError::operation_failed(format!(
                    "macOS process enumeration allocation failed: {error}"
                ))
            })?;
        self.pids.resize(requested, 0);
        Ok(())
    }
}

fn enumeration_error() -> PlatformError {
    PlatformError::new(
        PlatformErrorCode::OperationFailed,
        format!(
            "macOS process enumeration failed: {}",
            std::io::Error::last_os_error()
        ),
    )
}

#[derive(Clone, Default)]
struct Metadata {
    name: String,
    executable: Option<PathBuf>,
    identity_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ProcessIdentity {
    started_at: u64,
    image_uuid: [u8; 16],
}
impl ProcessIdentity {
    fn from_usage(usage: &libc::rusage_info_v2) -> Self {
        Self {
            started_at: usage.ri_proc_start_abstime,
            image_uuid: usage.ri_uuid,
        }
    }
}

struct CachedMetadata {
    value: Metadata,
    checked: Instant,
}
// The registry must not retain process metadata after sampling has been disabled.
// Readers acquire lazily so constructing a reset sampler cannot keep the old cache alive.
#[derive(Default)]
struct MetadataRegistry(Mutex<Weak<Mutex<MetadataCache>>>);
impl MetadataRegistry {
    fn acquire(&self) -> Arc<Mutex<MetadataCache>> {
        let mut reference = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(cache) = reference.upgrade() {
            return cache;
        }
        let cache = Arc::new(Mutex::new(MetadataCache::default()));
        *reference = Arc::downgrade(&cache);
        cache
    }
}

#[derive(Default)]
struct MetadataCache(HashMap<(u32, ProcessIdentity), CachedMetadata>);
impl MetadataCache {
    fn attach(
        &mut self,
        rows: &mut [ProcessReading],
        identities: &[(u32, Option<ProcessIdentity>)],
        now: Instant,
        mut lookup: impl FnMut(u32, Option<ProcessIdentity>) -> Metadata,
    ) {
        let live = identities
            .iter()
            .filter_map(|&(pid, identity)| identity.map(|identity| (pid, identity)))
            .collect::<HashSet<_>>();
        self.0.retain(|identity, _| live.contains(identity));
        for (row, &(pid, identity)) in rows.iter_mut().zip(identities) {
            // PID reuse and an executable-image change both invalidate cached paths.
            // Processes without a readable creation identity never enter the cache.
            let fresh;
            let meta = if let Some(identity) = identity {
                let cached = self
                    .0
                    .entry((pid, identity))
                    .or_insert_with(|| CachedMetadata {
                        value: lookup(pid, Some(identity)),
                        checked: now,
                    });
                // Recover transient path failures promptly; periodically notice moved executables.
                let interval = if cached.value.executable.is_none() {
                    2
                } else {
                    30
                };
                if now.saturating_duration_since(cached.checked) >= Duration::from_secs(interval) {
                    cached.value = lookup(pid, Some(identity));
                    cached.checked = now;
                }
                &cached.value
            } else {
                fresh = lookup(pid, None);
                &fresh
            };
            row.counter.name.clone_from(&meta.name);
            row.counter.executable.clone_from(&meta.executable);
            row.counter.location_status = if meta.identity_changed {
                row.footprint = None;
                row.counter.cpu_time_ms = None;
                ProcessLocationStatus::Exited
            } else if meta.executable.is_some() {
                ProcessLocationStatus::Available
            } else {
                ProcessLocationStatus::Unavailable
            };
        }
    }
}

fn read_metadata(pid: u32, expected: Option<ProcessIdentity>) -> Metadata {
    let mut buffer = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let bytes =
        unsafe { libc::proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    let executable = (bytes > 0)
        .then(|| {
            let length = buffer
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(buffer.len());
            PathBuf::from(OsString::from_vec(buffer[..length].to_vec()))
        })
        .filter(|path| path.is_absolute());
    let name = executable
        .as_deref()
        .and_then(std::path::Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            let mut buffer = [0u8; 256];
            let bytes = unsafe {
                libc::proc_name(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32)
            };
            if bytes <= 0 {
                return String::new();
            }
            let length = buffer
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(buffer.len());
            String::from_utf8_lossy(&buffer[..length]).into_owned()
        });
    if let Some(expected) = expected {
        if super::process_cpu_macos::read_usage(pid)
            .as_ref()
            .map(ProcessIdentity::from_usage)
            != Some(expected)
        {
            return Metadata {
                identity_changed: true,
                ..Default::default()
            };
        }
    }
    Metadata {
        name,
        executable,
        identity_changed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(pid: u32) -> ProcessReading {
        ProcessReading {
            footprint: None,
            counter: ProcessCpuCounter {
                pid,
                started_at: 0,
                name: String::new(),
                executable: None,
                location_status: ProcessLocationStatus::Unavailable,
                cpu_time_ms: None,
            },
        }
    }
    #[test]
    fn shared_metadata_is_released_when_the_last_sampling_owner_drops() {
        let registry = MetadataRegistry::default();
        let first = registry.acquire();
        let second = registry.acquire();
        assert!(Arc::ptr_eq(&first, &second));
        let lifetime = Arc::downgrade(&first);
        drop(first);
        assert!(lifetime.upgrade().is_some());
        drop(second);
        assert!(lifetime.upgrade().is_none());
        let replacement = registry.acquire();
        assert!(replacement.lock().unwrap().0.is_empty());
    }

    #[test]
    fn metadata_is_shared_for_a_lifetime_and_requeried_after_pid_reuse() {
        let mut cache = MetadataCache::default();
        let mut reads = 0;
        let mut lookup = |pid, _| {
            reads += 1;
            Metadata {
                name: format!("process-{pid}"),
                executable: None,
                ..Default::default()
            }
        };
        let mut rows = vec![row(1), row(2)];
        let now = Instant::now();
        let identity = |started_at| {
            Some(ProcessIdentity {
                started_at,
                image_uuid: [0; 16],
            })
        };
        cache.attach(&mut rows, &[(1, identity(10)), (2, None)], now, &mut lookup);
        cache.attach(&mut rows, &[(1, identity(10)), (2, None)], now, &mut lookup);
        assert_eq!(reads, 3);
        cache.attach(&mut rows, &[(1, identity(20)), (2, None)], now, |_, _| {
            Metadata {
                name: "replacement".into(),
                executable: None,
                ..Default::default()
            }
        });
        assert_eq!(rows[0].counter.name, "replacement");
        assert_eq!(cache.0.len(), 1);
        cache.attach(&mut [], &[], now, |_, _| unreachable!());
        assert!(cache.0.is_empty());
    }
    #[test]
    fn native_snapshot_preserves_own_footprint_and_identity() {
        let rows = ProcessSnapshotReader::default().read().unwrap();
        let own = rows
            .iter()
            .find(|row| row.counter.pid == std::process::id())
            .unwrap();
        assert!(own.footprint.is_some_and(|bytes| bytes > 0));
        assert!(own.counter.started_at > 0);
        assert!(own.counter.cpu_time_ms.is_some());
        assert!(own.counter.executable.is_some());
    }

    #[test]
    fn transient_failures_retry_and_image_changes_do_not_reuse_metadata() {
        let mut cache = MetadataCache::default();
        let now = Instant::now();
        let identity = ProcessIdentity {
            started_at: 10,
            image_uuid: [1; 16],
        };
        let mut rows = vec![row(1)];
        cache.attach(&mut rows, &[(1, Some(identity))], now, |_, _| Metadata {
            name: "first".into(),
            executable: None,
            ..Default::default()
        });
        cache.attach(
            &mut rows,
            &[(1, Some(identity))],
            now + Duration::from_secs(1),
            |_, _| unreachable!(),
        );
        cache.attach(
            &mut rows,
            &[(1, Some(identity))],
            now + Duration::from_secs(2),
            |_, _| Metadata {
                name: "recovered".into(),
                executable: Some("/bin/first".into()),
                ..Default::default()
            },
        );
        assert_eq!(rows[0].counter.name, "recovered");
        let replacement = ProcessIdentity {
            image_uuid: [2; 16],
            ..identity
        };
        cache.attach(
            &mut rows,
            &[(1, Some(replacement))],
            now + Duration::from_secs(3),
            |_, _| Metadata {
                name: "replacement".into(),
                executable: Some("/bin/second".into()),
                ..Default::default()
            },
        );
        assert_eq!(rows[0].counter.name, "replacement");
        assert_eq!(cache.0.len(), 1);
    }

    #[test]
    fn identity_change_during_metadata_lookup_discards_mismatched_counters() {
        let mut rows = vec![row(1)];
        rows[0].footprint = Some(1024);
        rows[0].counter.cpu_time_ms = Some(12);
        MetadataCache::default().attach(
            &mut rows,
            &[(
                1,
                Some(ProcessIdentity {
                    started_at: 10,
                    image_uuid: [1; 16],
                }),
            )],
            Instant::now(),
            |_, _| Metadata {
                identity_changed: true,
                ..Default::default()
            },
        );
        assert_eq!(rows[0].footprint, None);
        assert_eq!(rows[0].counter.cpu_time_ms, None);
        assert_eq!(
            rows[0].counter.location_status,
            ProcessLocationStatus::Exited
        );
    }

    #[test]
    fn process_buffer_rejects_growth_beyond_its_limit() {
        let mut reader = ProcessSnapshotReader::default();
        assert!(reader.grow(MAX_PROCESSES + 1).is_err());
        assert!(reader.pids.is_empty());
    }

    #[test]
    fn native_exec_refreshes_the_image_without_waiting_for_cache_expiry() {
        use std::{
            io::{BufRead, BufReader, Write},
            process::{Child, Command, Stdio},
        };
        struct Fixture(Child);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut fixture = Fixture(
            Command::new("/bin/bash")
                .args(["-c", "printf 'ready\\n'; read ignored; exec /bin/sleep 20"])
                .stdout(Stdio::piped())
                .stdin(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        // Wait for the first exec to complete before checking the later image transition.
        let mut ready = String::new();
        BufReader::new(fixture.0.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "ready");
        let pid = fixture.0.id();
        let mut reader = ProcessSnapshotReader::default();
        let before = reader
            .read()
            .unwrap()
            .into_iter()
            .find(|row| row.counter.pid == pid)
            .unwrap();
        assert_eq!(
            before.counter.executable.as_deref(),
            Some(std::path::Path::new("/bin/bash"))
        );
        fixture
            .0
            .stdin
            .take()
            .unwrap()
            .write_all(b"continue\n")
            .unwrap();
        let started = Instant::now();
        loop {
            let rows = reader.read().unwrap();
            if rows.iter().any(|row| {
                row.counter.pid == pid
                    && row.counter.executable.as_deref() == Some(std::path::Path::new("/bin/sleep"))
            }) {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the new image must be visible before the 30-second metadata expiry"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
