//! A sleeping desktop worker schedules actions without blocking native rendering or sampling.
use super::memory_preferences::MemoryReleaseState;
use std::{
    sync::{atomic::Ordering, Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use strawberrydisk_core::system_resources::release_policy::ReleaseSchedule;
use strawberrydisk_platform::system_resources::memory::{MemorySampler, MemorySource};

pub fn start(app: tauri::AppHandle, state: Arc<MemoryReleaseState>) {
    std::thread::spawn(move || {
        let mut schedule = ReleaseSchedule::default();
        loop {
            let preferences = state
                .preferences
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            // Wall-clock gaps also detect suspend on systems whose monotonic clock pauses.
            // A clock adjustment resets the deadline instead of replaying missed intervals.
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            if schedule.due(now, &preferences, state.actions.load(Ordering::Relaxed)) {
                match MemorySampler::default().sample(false) {
                    Ok(sample) if sample.total_bytes > 0 => {
                        let percent = sample.used_bytes as f64 * 100.0 / sample.total_bytes as f64;
                        if percent < f64::from(preferences.threshold_percent) {
                            log::info!("memory_release_automatic_skipped reason=below_threshold used_percent={percent:.1} threshold={} revision={}", preferences.threshold_percent, preferences.revision);
                        } else {
                            // Re-read settings at execution, so disabling from another window wins.
                            super::memory_release::execute_configured(
                                &app,
                                true,
                                Some(preferences.revision),
                            );
                        }
                    }
                    result => log::warn!(
                        "memory_release_automatic_skipped reason=sample_unavailable error={}",
                        strawberrydisk_platform::diagnostics::text(&format!("{result:?}"))
                    ),
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}
