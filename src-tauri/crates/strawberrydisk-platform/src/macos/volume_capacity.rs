//! Shared display capacity, including the space macOS expects to reclaim.
use std::{
    collections::HashMap,
    ffi::CString,
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use objc2::rc::autoreleasepool;
use objc2_foundation::{
    NSNumber, NSString, NSURLVolumeAvailableCapacityForImportantUsageKey, NSURL,
};

use crate::system_resources::disk::VolumeCapacity;

const CACHE_TTL: Duration = Duration::from_secs(5);
const MAX_CACHED_VOLUMES: usize = 64;
static CACHE: OnceLock<Mutex<CapacityCache>> = OnceLock::new();

#[derive(Default)]
struct CapacityCache {
    entries: HashMap<(i32, i32, u64), (Instant, VolumeCapacity)>,
}

impl CapacityCache {
    fn read(
        &mut self,
        key: (i32, i32, u64),
        now: Instant,
        query: impl FnOnce() -> VolumeCapacity,
    ) -> VolumeCapacity {
        self.entries
            .retain(|_, (sampled_at, _)| now.duration_since(*sampled_at) < CACHE_TTL);
        if let Some((_, capacity)) = self.entries.get(&key) {
            return *capacity;
        }
        let capacity = query();
        if self.entries.len() >= MAX_CACHED_VOLUMES {
            self.entries.clear();
        }
        self.entries.insert(key, (now, capacity));
        capacity
    }
}

pub(crate) fn invalidate() {
    if let Some(cache) = CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.entries.clear();
        }
    }
}

pub(crate) fn read(path: &Path) -> Result<VolumeCapacity, String> {
    // The read-only system snapshot and its writable Data volume share storage.
    // Use one query/cache entry so GUI, CLI and resident readings agree.
    let path = if path == Path::new("/") && Path::new("/System/Volumes/Data").is_dir() {
        Path::new("/System/Volumes/Data")
    } else {
        path
    };
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let mut stats: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stats) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if stats.f_flags & libc::MNT_LOCAL as u32 == 0 {
        return Err("display capacity requires a local volume".to_string());
    }
    let block_size = u64::from(stats.f_bsize);
    let total_bytes = stats.f_blocks.saturating_mul(block_size);
    let free_bytes = stats.f_bavail.saturating_mul(block_size).min(total_bytes);
    // Recheck native identity on every read. A replacement disk at the same
    // mount path must never inherit the previous disk's cached capacity.
    // Darwin's opaque fsid_t contains two int32 values; transmute checks the
    // ABI size without depending on libc's private field names.
    let fsid: [i32; 2] = unsafe { std::mem::transmute(stats.f_fsid) };
    let key = (fsid[0], fsid[1], total_bytes);
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(CapacityCache::default()))
        .lock()
        .map_err(|error| format!("volume capacity cache unavailable: {error}"))?;
    Ok(cache.read(key, Instant::now(), || {
        let started = Instant::now();
        let (available_bytes, fallback) = resolve_available(total_bytes, free_bytes, native_available(path));
        if let Some(error) = fallback {
                log::warn!(
                    "volume_capacity_fallback path={} stage=foundation_available error={} outcome=filesystem_free",
                    crate::diagnostics::text(&path.display()),
                    crate::diagnostics::text(&error)
                );
        }
        log::debug!(
            "volume_capacity_sampled path={} total_bytes={} free_bytes={} available_bytes={} reclaimable_bytes={} elapsed_us={}",
            crate::diagnostics::text(&path.display()),
            total_bytes,
            free_bytes,
            available_bytes,
            available_bytes.saturating_sub(free_bytes),
            started.elapsed().as_micros()
        );
        VolumeCapacity { total_bytes, available_bytes }
    }))
}

fn resolve_available(total: u64, free: u64, native: Result<u64, String>) -> (u64, Option<String>) {
    match native {
        Ok(bytes) if bytes <= total => (bytes.max(free), None),
        Ok(bytes) => (
            free,
            Some(format!(
                "native available capacity {bytes} exceeds total capacity {total}"
            )),
        ),
        Err(error) => (free, Some(error)),
    }
}

fn native_available(path: &Path) -> Result<u64, String> {
    let path = path.to_str().ok_or("volume path is not valid UTF-8")?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let mut value = None;
        // Foundation defines this resource key as NSNumber. Validate the
        // returned class before interpreting it; missing/invalid values fall back.
        unsafe {
            url.getResourceValue_forKey_error(
                &mut value,
                NSURLVolumeAvailableCapacityForImportantUsageKey,
            )
        }
        .map_err(|error| error.to_string())?;
        let bytes = value
            .as_ref()
            .and_then(|value| value.downcast_ref::<NSNumber>())
            .ok_or("Foundation did not return a capacity number")?
            .longLongValue();
        u64::try_from(bytes).map_err(|_| "Foundation returned a negative capacity".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_native_estimates_preserve_filesystem_free_space_and_error_details() {
        assert_eq!(resolve_available(100, 20, Ok(80)), (80, None));
        assert_eq!(resolve_available(100, 20, Ok(10)), (20, None));
        assert!(resolve_available(100, 20, Ok(101)).1.is_some());
        assert_eq!(
            resolve_available(100, 20, Err("native error 5".into())),
            (20, Some("native error 5".into()))
        );
    }

    #[test]
    fn display_queries_share_a_snapshot_until_expiry_and_never_share_volume_identity() {
        let mut cache = CapacityCache::default();
        let now = Instant::now();
        let first = VolumeCapacity {
            total_bytes: 100,
            available_bytes: 80,
        };
        let changed = VolumeCapacity {
            total_bytes: 100,
            available_bytes: 60,
        };
        assert_eq!(cache.read((1, 2, 100), now, || first).available_bytes, 80);
        assert_eq!(
            cache
                .read((1, 2, 100), now + Duration::from_secs(4), || panic!(
                    "cached query must not call Foundation"
                ))
                .available_bytes,
            80
        );
        assert_eq!(cache.read((2, 2, 100), now, || changed).available_bytes, 60);
        assert_eq!(
            cache
                .read((1, 2, 100), now + CACHE_TTL, || changed)
                .available_bytes,
            60
        );
        cache.entries.clear();
        assert_eq!(
            cache
                .read((1, 2, 100), now + CACHE_TTL, || first)
                .available_bytes,
            80
        );
    }

    #[test]
    fn cached_volume_count_is_bounded() {
        let mut cache = CapacityCache::default();
        let now = Instant::now();
        for id in 0..100 {
            cache.read((id, 0, 100), now, || VolumeCapacity {
                total_bytes: 100,
                available_bytes: 10,
            });
            assert!(cache.entries.len() <= MAX_CACHED_VOLUMES);
        }
    }

    #[test]
    fn system_capacity_paths_share_the_same_display_snapshot() {
        let root = read(Path::new("/")).expect("system capacity must be readable");
        if Path::new("/System/Volumes/Data").is_dir() {
            let data =
                read(Path::new("/System/Volumes/Data")).expect("Data capacity must be readable");
            assert_eq!(root.total_bytes, data.total_bytes);
            assert_eq!(root.available_bytes, data.available_bytes);
        }
        assert!(root.total_bytes > 0);
        assert!(root.available_bytes <= root.total_bytes);
    }
}
