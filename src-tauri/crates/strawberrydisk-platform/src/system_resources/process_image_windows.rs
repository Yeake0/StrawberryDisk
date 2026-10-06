//! Bounded image lookup cache; CPU counters do not depend on path permissions.
use super::process_cpu::{ProcessCpuCounter, ProcessLocationStatus};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};

struct Image {
    path: Option<PathBuf>,
    status: ProcessLocationStatus,
    attempts: u8,
    checked: Instant,
}
#[derive(Default)]
pub(super) struct ImageCache(HashMap<(u32, u64), Image>);

// Sampling owners retain metadata, not the process-wide registry. Lazily acquiring
// a strong reference allows dropping the last sampler to release the entire cache.
#[derive(Default)]
pub(super) struct ImageRegistry(Mutex<Weak<Mutex<ImageCache>>>);
impl ImageRegistry {
    pub(super) fn acquire(&self) -> Arc<Mutex<ImageCache>> {
        let mut reference = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(cache) = reference.upgrade() {
            return cache;
        }
        let cache = Arc::new(Mutex::new(ImageCache::default()));
        *reference = Arc::downgrade(&cache);
        cache
    }
}

pub(super) fn shared_cache() -> Arc<Mutex<ImageCache>> {
    static REGISTRY: OnceLock<ImageRegistry> = OnceLock::new();
    REGISTRY.get_or_init(Default::default).acquire()
}
impl ImageCache {
    pub(super) fn attach<'a>(&mut self, rows: impl IntoIterator<Item = &'a mut ProcessCpuCounter>) {
        self.attach_at(rows, Instant::now(), lookup);
    }
    fn attach_at<'a>(
        &mut self,
        rows: impl IntoIterator<Item = &'a mut ProcessCpuCounter>,
        now: Instant,
        mut lookup: impl FnMut(u32, u64) -> Result<PathBuf, ProcessLocationStatus>,
    ) {
        let mut previous = std::mem::take(&mut self.0);
        for row in rows {
            let key = (row.pid, row.started_at);
            let mut cached = previous.remove(&key);
            let retry = cached.as_ref().is_none_or(|image| {
                image.status == ProcessLocationStatus::Unavailable
                    && image.attempts < 3
                    && now.saturating_duration_since(image.checked) >= Duration::from_secs(30)
            });
            if retry {
                let attempts = cached.as_ref().map_or(1, |image| image.attempts + 1);
                let (path, status) = match lookup(row.pid, row.started_at) {
                    Ok(path) => (Some(path), ProcessLocationStatus::Available),
                    Err(status) => (None, status),
                };
                cached = Some(Image {
                    path,
                    status,
                    attempts,
                    checked: now,
                });
            }
            let image = cached.expect("new identities are always queried");
            row.executable.clone_from(&image.path);
            row.location_status = image.status;
            self.0.insert(key, image);
        }
        // Entries not present in this snapshot (including reused PIDs) are dropped.
    }
}

fn lookup(pid: u32, created: u64) -> Result<PathBuf, ProcessLocationStatus> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    let failure = |stage: &str| {
        let code = unsafe { GetLastError() };
        let status = match code {
            ERROR_ACCESS_DENIED => ProcessLocationStatus::Denied,
            ERROR_INVALID_PARAMETER => ProcessLocationStatus::Exited,
            _ => ProcessLocationStatus::Unavailable,
        };
        log::debug!(
            "process_image_lookup pid={pid} stage={stage} native_code={code} outcome={status:?}"
        );
        status
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Err(failure("open"));
    }
    let result = (|| {
        let (started, _) = super::process_cpu::windows_counters_for_handle(handle)
            .ok_or_else(|| failure("identity"))?;
        if started != created {
            return Err(ProcessLocationStatus::Exited);
        }
        let mut path = vec![0u16; 32_768];
        let mut length = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut length) } == 0 {
            return Err(failure("path"));
        }
        if length == 0 || length as usize > path.len() {
            return Err(ProcessLocationStatus::Unavailable);
        }
        Ok(PathBuf::from(OsString::from_wide(&path[..length as usize])))
    })();
    unsafe {
        CloseHandle(handle);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(pid: u32, started_at: u64) -> ProcessCpuCounter {
        ProcessCpuCounter {
            pid,
            started_at,
            name: "test".into(),
            executable: None,
            location_status: ProcessLocationStatus::Unavailable,
            cpu_time_ms: Some(0),
        }
    }
    #[test]
    fn transient_failures_retry_with_a_limit_while_denials_and_successes_are_cached() {
        let mut cache = ImageCache::default();
        let now = Instant::now();
        let mut rows = [row(1, 1), row(2, 1), row(3, 1)];
        cache.attach_at(&mut rows, now, |pid, _| match pid {
            1 => Err(ProcessLocationStatus::Unavailable),
            2 => Err(ProcessLocationStatus::Denied),
            _ => Ok("C:\\test.exe".into()),
        });
        cache.attach_at(&mut rows, now + Duration::from_secs(29), |_, _| {
            panic!("not due")
        });
        for seconds in [30, 60] {
            cache.attach_at(&mut rows, now + Duration::from_secs(seconds), |pid, _| {
                assert_eq!(pid, 1);
                Err(ProcessLocationStatus::Unavailable)
            });
        }
        cache.attach_at(&mut rows, now + Duration::from_secs(90), |_, _| {
            panic!("retry limit")
        });
        let mut reused = [row(2, 2)];
        cache.attach_at(&mut reused, now, |_, _| Ok("C:\\new.exe".into()));
        assert_eq!(cache.0.len(), 1);
        assert_eq!(reused[0].location_status, ProcessLocationStatus::Available);
    }
    #[test]
    fn transient_failure_can_recover_without_a_process_restart() {
        let mut cache = ImageCache::default();
        let now = Instant::now();
        let mut rows = [row(1, 1)];
        cache.attach_at(&mut rows, now, |_, _| {
            Err(ProcessLocationStatus::Unavailable)
        });
        cache.attach_at(&mut rows, now + Duration::from_secs(30), |_, _| {
            Ok("C:\\app.exe".into())
        });
        assert!(rows[0].executable.is_some());
        assert_eq!(rows[0].location_status, ProcessLocationStatus::Available);
    }

    #[test]
    fn sampling_owners_share_paths_and_release_metadata_with_the_last_owner() {
        let registry = ImageRegistry::default();
        let cpu = registry.acquire();
        let memory = registry.acquire();
        assert!(Arc::ptr_eq(&cpu, &memory));
        let now = Instant::now();
        let mut rows = [row(1, 10)];
        cpu.lock()
            .unwrap()
            .attach_at(&mut rows, now, |_, _| Ok("C:\\app.exe".into()));
        memory.lock().unwrap().attach_at(&mut rows, now, |_, _| {
            panic!("the other sampler must reuse the resolved path")
        });
        let weak = Arc::downgrade(&cpu);
        drop(cpu);
        assert!(weak.upgrade().is_some());
        drop(memory);
        assert!(weak.upgrade().is_none());
        assert!(registry.acquire().lock().unwrap().0.is_empty());
    }
}
