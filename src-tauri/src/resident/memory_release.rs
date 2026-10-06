//! Desktop serialization for an explicit memory action; the Core owns execution semantics.
use std::sync::{Mutex, TryLockError};
use strawberrydisk_core::system_resources::release::{
    self, MemoryReleaseResult, MemoryReleaseStatus,
};

static ACTION: Mutex<()> = Mutex::new(());

pub fn execute_configured(
    app: &tauri::AppHandle,
    automatic: bool,
    revision: Option<u64>,
) -> MemoryReleaseResult {
    execute_serialized(&ACTION, || {
        let preferences = match super::memory_preferences::get(app) {
            Ok(preferences) => preferences,
            Err(_) => return MemoryReleaseResult::status(MemoryReleaseStatus::Failed),
        };
        if automatic && (!preferences.automatic || revision != Some(preferences.revision)) {
            log::info!("memory_release_automatic_skipped reason=settings_changed");
            return MemoryReleaseResult::status(MemoryReleaseStatus::Cancelled);
        }
        let options = strawberrydisk_platform::system_resources::release::ReleaseOptions {
            excluded_paths: preferences
                .exclusions
                .iter()
                .map(|item| item.path.clone())
                .collect(),
            skip_foreground: automatic && preferences.skip_foreground,
        };
        log::info!(
            "memory_release_policy source={} revision={} exclusions={} skip_foreground={}",
            if automatic { "automatic" } else { "manual" },
            preferences.revision,
            preferences.exclusions.len(),
            options.skip_foreground
        );
        let result = release::release_memory_with(&options);
        super::memory_preferences::record_action(app);
        result
    })
}

fn execute_serialized(
    lock: &Mutex<()>,
    action: impl FnOnce() -> MemoryReleaseResult,
) -> MemoryReleaseResult {
    let _guard = match lock.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            log::info!("memory_release_rejected reason=busy");
            return MemoryReleaseResult::status(MemoryReleaseStatus::Busy);
        }
        Err(TryLockError::Poisoned(error)) => error.into_inner(),
    };
    let started = std::time::Instant::now();
    log::info!("memory_release_started");
    let result = action();
    log::info!(
        "memory_release_finished status={:?} elapsed_ms={}",
        result.status,
        started.elapsed().as_millis()
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_panicking_native_action_does_not_permanently_block_retry() {
        let lock = Mutex::new(());
        assert!(
            std::panic::catch_unwind(|| execute_serialized(&lock, || panic!("native failure")))
                .is_err()
        );
        assert_eq!(
            execute_serialized(&lock, || MemoryReleaseResult::status(
                MemoryReleaseStatus::Completed
            ))
            .status,
            MemoryReleaseStatus::Completed
        );
    }

    #[test]
    fn duplicate_requests_are_rejected_and_completion_allows_retry() {
        let lock = Mutex::new(());
        let guard = lock.lock().unwrap();
        assert_eq!(
            execute_serialized(&lock, || panic!("a duplicate must not execute")).status,
            MemoryReleaseStatus::Busy
        );
        drop(guard);
        assert_eq!(
            execute_serialized(&lock, || MemoryReleaseResult::status(
                MemoryReleaseStatus::Cancelled
            ))
            .status,
            MemoryReleaseStatus::Cancelled
        );
        assert!(lock.try_lock().is_ok());
    }
}
