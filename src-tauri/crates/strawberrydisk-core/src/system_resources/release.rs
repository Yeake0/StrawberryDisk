//! Explicit memory reclamation, separate from passive sampling and its cadence.
use serde::Serialize;
use std::time::Duration;
use strawberrydisk_platform::system_resources::memory::NativeMemorySnapshot;
use strawberrydisk_platform::{
    system_resources::{
        memory::{MemorySampler, MemorySource},
        release,
    },
    PlatformErrorCode, PlatformResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryReleaseStatus {
    Completed,
    Cancelled,
    Unsupported,
    Failed,
    Busy,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReleaseResult {
    pub schema_version: u32,
    pub status: MemoryReleaseStatus,
    /// A system-wide observation, not a guarantee that this action freed these bytes.
    pub observed_reduction_bytes: Option<u64>,
}

impl MemoryReleaseResult {
    pub fn status(status: MemoryReleaseStatus) -> Self {
        Self {
            schema_version: 1,
            status,
            observed_reduction_bytes: None,
        }
    }
}

pub fn release_memory() -> MemoryReleaseResult {
    release_memory_with(&release::ReleaseOptions::default())
}

pub fn release_memory_with(options: &release::ReleaseOptions) -> MemoryReleaseResult {
    release_with(
        &mut MemorySampler::default(),
        || release::release_memory_with(options),
        std::thread::sleep,
    )
}

fn release_with(
    source: &mut impl MemorySource,
    action: impl FnOnce() -> PlatformResult<()>,
    mut wait: impl FnMut(Duration),
) -> MemoryReleaseResult {
    // A missing before/after reading must not turn a completed native action into
    // fabricated savings or invite a second destructive attempt to measure it.
    let before = observe(source, Duration::from_millis(100), false, &mut wait);
    match action() {
        Ok(()) => {
            let after = observe(source, Duration::from_millis(500), true, &mut wait);
            log::info!(
                "memory_release_observed before_used_bytes={:?} after_used_bytes={:?} before_free_bytes={:?} after_free_bytes={:?} before_swap_bytes={:?} after_swap_bytes={:?}",
                before.as_ref().map(|sample| sample.used_bytes),
                after.as_ref().map(|sample| sample.used_bytes),
                before.as_ref().map(|sample| sample.free_bytes),
                after.as_ref().map(|sample| sample.free_bytes),
                before.as_ref().map(|sample| sample.swap_used_bytes),
                after.as_ref().map(|sample| sample.swap_used_bytes),
            );
            log::info!(
                "memory_release_comparison used_delta_bytes={:?}",
                before
                    .as_ref()
                    .zip(after.as_ref())
                    .map(|(before, after)| i128::from(after.used_bytes)
                        - i128::from(before.used_bytes))
            );
            MemoryReleaseResult {
                schema_version: 1,
                status: MemoryReleaseStatus::Completed,
                observed_reduction_bytes: before
                    .zip(after)
                    .map(|(before, after)| before.used_bytes.saturating_sub(after.used_bytes)),
            }
        }
        Err(error) => {
            log::warn!(
                "memory_release_native_failed code={:?} error={}",
                error.code(),
                strawberrydisk_platform::diagnostics::text(&error)
            );
            MemoryReleaseResult::status(match error.code() {
                PlatformErrorCode::UserCancelled => MemoryReleaseStatus::Cancelled,
                PlatformErrorCode::Unsupported => MemoryReleaseStatus::Unsupported,
                _ => MemoryReleaseStatus::Failed,
            })
        }
    }
}

/// Observe a short window rather than interpreting one scheduling instant as savings.
/// The median avoids selecting an optimistic minimum; missing samples invalidate the
/// comparison. Waiting after the native call measures settling, not fake progress.
fn observe(
    source: &mut impl MemorySource,
    interval: Duration,
    wait_first: bool,
    wait: &mut impl FnMut(Duration),
) -> Option<NativeMemorySnapshot> {
    let mut samples = Vec::with_capacity(3);
    for index in 0..3 {
        if wait_first || index > 0 {
            wait(interval);
        }
        match source.sample(false) {
            Ok(sample) if sample.total_bytes > 0 => samples.push(sample),
            Ok(_) => log::warn!(
                "memory_release_measurement_failed reason=zero_capacity after={wait_first}"
            ),
            Err(error) => log::warn!(
                "memory_release_measurement_failed code={:?} after={wait_first}",
                error.code()
            ),
        }
    }
    if samples.len() != 3 {
        return None;
    }
    samples.sort_by_key(|sample| sample.used_bytes);
    log::info!("memory_release_sample_window after={wait_first} samples=3 min_used_bytes={} max_used_bytes={}", samples[0].used_bytes, samples[2].used_bytes);
    Some(samples.swap_remove(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use strawberrydisk_platform::{system_resources::memory::NativeMemorySnapshot, PlatformError};

    struct Source(VecDeque<Option<u64>>);
    impl MemorySource for Source {
        fn sample(&mut self, details: bool) -> PlatformResult<NativeMemorySnapshot> {
            assert!(!details);
            let used = self
                .0
                .pop_front()
                .flatten()
                .ok_or_else(|| PlatformError::operation_failed("unavailable"))?;
            Ok(NativeMemorySnapshot {
                total_bytes: 100,
                used_bytes: used,
                free_bytes: 100 - used,
                swap_used_bytes: 0,
                process_memory_kind:
                    strawberrydisk_platform::system_resources::memory::ProcessMemoryKind::native(),
                processes: None,
            })
        }
    }

    #[test]
    fn reports_only_observed_reduction_and_preserves_missing_measurements() {
        for (values, expected) in [
            ([Some(70), Some(40)], Some(30)),
            ([Some(40), Some(70)], Some(0)),
            ([None, Some(40)], None),
            ([Some(40), None], None),
        ] {
            let result = release_with(
                &mut Source(values.into_iter().flat_map(|value| [value; 3]).collect()),
                || Ok(()),
                |_| {},
            );
            assert_eq!(result.status, MemoryReleaseStatus::Completed);
            assert_eq!(result.observed_reduction_bytes, expected);
        }
    }

    #[test]
    fn cancellation_and_failure_never_claim_released_memory() {
        for (code, status) in [
            (
                PlatformErrorCode::UserCancelled,
                MemoryReleaseStatus::Cancelled,
            ),
            (
                PlatformErrorCode::Unsupported,
                MemoryReleaseStatus::Unsupported,
            ),
            (
                PlatformErrorCode::OperationFailed,
                MemoryReleaseStatus::Failed,
            ),
        ] {
            let mut source = Source([Some(70), Some(70), Some(70), Some(40)].into());
            let result = release_with(
                &mut source,
                || Err(PlatformError::new(code, "native action did not complete")),
                |_| {},
            );
            assert_eq!(result.status, status);
            assert_eq!(result.observed_reduction_bytes, None);
            assert_eq!(source.0.len(), 1);
        }
    }
    #[test]
    fn measurement_uses_medians_and_a_bounded_settling_window() {
        let mut waits = Vec::new();
        let mut source =
            Source([Some(70), Some(99), Some(72), Some(20), Some(60), Some(62)].into());
        let result = release_with(&mut source, || Ok(()), |duration| waits.push(duration));
        assert_eq!(result.observed_reduction_bytes, Some(12));
        assert_eq!(waits.iter().sum::<Duration>(), Duration::from_millis(1700));
        assert_eq!(waits.len(), 5);
    }
}
