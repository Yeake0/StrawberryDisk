use strawberrydisk_platform::system_resources::cpu::{CpuCounters, CpuSample};

use super::metrics::CpuUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuBaselineReason {
    FirstSample,
    InvalidInterval,
    CounterReset,
    NoProgress,
    InvalidValue,
}

#[derive(Default)]
pub struct CpuDelta {
    previous: Option<(CpuCounters, u64)>,
}

impl CpuDelta {
    pub fn observe(
        &mut self,
        sample: CpuSample,
        elapsed_ms: u64,
    ) -> Result<CpuUsage, CpuBaselineReason> {
        if let CpuSample::Counters(counters) = sample {
            return self.sample(counters, elapsed_ms);
        }
        // Crossing native percentage / fallback tick sources must never reuse
        // an unrelated baseline. macOS continues through the original tick path.
        self.reset();
        match sample {
            CpuSample::Baseline => Err(CpuBaselineReason::FirstSample),
            CpuSample::Percent { used, interval_ms } => {
                if !(100..=5_000).contains(&interval_ms) {
                    return Err(CpuBaselineReason::InvalidInterval);
                }
                if !used.is_finite() || !(0.0..=100.0).contains(&used) {
                    return Err(CpuBaselineReason::InvalidValue);
                }
                Ok(CpuUsage { used_percent: used })
            }
            CpuSample::Counters(_) => unreachable!(),
        }
    }

    /// elapsed_ms is monotonic time supplied by the worker. Sleep/resume and
    /// counter resets require a new baseline rather than a misleading spike.
    pub fn sample(
        &mut self,
        counters: CpuCounters,
        elapsed_ms: u64,
    ) -> Result<CpuUsage, CpuBaselineReason> {
        use CpuBaselineReason::*;
        let previous = self
            .previous
            .replace((counters, elapsed_ms))
            .ok_or(FirstSample)?;
        let elapsed = elapsed_ms.checked_sub(previous.1).ok_or(InvalidInterval)?;
        if !(100..=5_000).contains(&elapsed) {
            return Err(InvalidInterval);
        }
        let busy = counters
            .busy
            .checked_sub(previous.0.busy)
            .ok_or(CounterReset)?;
        let idle = counters
            .idle
            .checked_sub(previous.0.idle)
            .ok_or(CounterReset)?;
        let total = u128::from(busy) + u128::from(idle);
        if total == 0 {
            return Err(NoProgress);
        }
        Ok(CpuUsage {
            used_percent: busy as f64 / total as f64 * 100.0,
        })
    }

    pub fn reset(&mut self) {
        self.previous = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_percentages_reject_invalid_samples_and_reset_fallback_ticks() {
        let mut delta = CpuDelta::default();
        let ticks = CpuCounters { busy: 10, idle: 90 };
        assert_eq!(
            delta.observe(ticks.into(), 0).unwrap_err(),
            CpuBaselineReason::FirstSample
        );
        assert_eq!(
            delta
                .observe(
                    CpuSample::Percent {
                        used: 7.5,
                        interval_ms: 1000
                    },
                    1000
                )
                .unwrap()
                .used_percent,
            7.5
        );
        assert_eq!(
            delta.observe(ticks.into(), 2000).unwrap_err(),
            CpuBaselineReason::FirstSample
        );
        for used in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
            assert_eq!(
                delta
                    .observe(
                        CpuSample::Percent {
                            used,
                            interval_ms: 1000
                        },
                        3000
                    )
                    .unwrap_err(),
                CpuBaselineReason::InvalidValue
            );
        }
        for interval_ms in [0, 99, 5001, 60_000] {
            assert_eq!(
                delta
                    .observe(
                        CpuSample::Percent {
                            used: 7.5,
                            interval_ms
                        },
                        3000
                    )
                    .unwrap_err(),
                CpuBaselineReason::InvalidInterval
            );
        }
        assert_eq!(
            delta.observe(CpuSample::Baseline, 4000).unwrap_err(),
            CpuBaselineReason::FirstSample
        );
    }

    #[test]
    fn whole_machine_cpu_is_normalized_with_no_first_sample_estimate() {
        let mut delta = CpuDelta::default();
        assert!(delta
            .sample(
                CpuCounters {
                    busy: 100,
                    idle: 300
                },
                0
            )
            .is_err());
        let value = delta
            .sample(
                CpuCounters {
                    busy: 200,
                    idle: 600,
                },
                2_000,
            )
            .unwrap();
        assert_eq!(value.used_percent, 25.0);
        assert_eq!(
            delta
                .sample(
                    CpuCounters {
                        busy: 300,
                        idle: 600
                    },
                    4_000
                )
                .unwrap()
                .used_percent,
            100.0
        );
    }

    #[test]
    fn zero_ticks_reset_and_sleep_require_another_valid_interval() {
        let mut delta = CpuDelta::default();
        let counters = CpuCounters {
            busy: 100,
            idle: 300,
        };
        assert!(delta.sample(counters, 0).is_err());
        assert!(delta.sample(counters, 2_000).is_err());
        assert!(delta
            .sample(CpuCounters { busy: 1, idle: 1 }, 4_000)
            .is_err());
        assert!(delta.sample(counters, 60_000).is_err());
        assert!(delta
            .sample(
                CpuCounters {
                    busy: 200,
                    idle: 600
                },
                62_000
            )
            .is_ok());
        delta.reset();
        assert!(delta.sample(counters, 64_000).is_err());
    }
}
