//! Persistent, language-neutral Windows CPU counters. A failed PDH provider
//! falls back to busy time with explicit diagnostics, never to a fabricated zero.
use super::{CpuSample, PlatformResult};
use std::{
    ptr,
    time::{Duration, Instant},
};
use windows_sys::{core::w, Win32::System::Performance::*};
use winreg::{
    enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY},
    RegKey,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    ProcessorTime,
    ProcessorUtility,
}
impl Source {
    fn path(self) -> *const u16 {
        match self {
            Self::ProcessorTime => w!("\\Processor Information(_Total)\\% Processor Time"),
            Self::ProcessorUtility => w!("\\Processor Information(_Total)\\% Processor Utility"),
        }
    }
}

fn preferred_source(version: Option<(u32, u32)>) -> Source {
    // KB5053656 introduced standard busy-time metrics in Windows 11 24H2.
    // Older Task Manager versions use frequency-weighted utility. The rollout
    // was gradual: this is a documented build policy, not detection of a private
    // feature flag, and it cannot promise pixel-identical Task Manager readings.
    if version
        .is_some_and(|(build, revision)| build > 26100 || (build == 26100 && revision >= 3624))
    {
        Source::ProcessorTime
    } else {
        Source::ProcessorUtility
    }
}
fn version() -> Option<(u32, u32)> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .ok()?;
    Some((
        key.get_value::<String, _>("CurrentBuildNumber")
            .ok()?
            .parse()
            .ok()?,
        key.get_value("UBR").ok()?,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Failure {
    stage: &'static str,
    code: u32,
    status: u32,
}
fn check(stage: &'static str, code: u32) -> Result<(), Failure> {
    if code == 0 {
        Ok(())
    } else {
        Err(Failure {
            stage,
            code,
            status: 0,
        })
    }
}

struct Query {
    handle: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    previous: Option<Instant>,
}
impl Drop for Query {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.handle);
        }
    }
}
impl Query {
    fn open(source: Source) -> Result<Self, Failure> {
        let mut handle = ptr::null_mut();
        check("open", unsafe {
            PdhOpenQueryW(ptr::null(), 0, &mut handle)
        })?;
        let mut query = Self {
            handle,
            counter: ptr::null_mut(),
            previous: None,
        };
        // Do not use localized counter names or spawn PowerShell/WMI per sample.
        check("add", unsafe {
            PdhAddEnglishCounterW(handle, source.path(), 0, &mut query.counter)
        })?;
        Ok(query)
    }
    fn read(&mut self) -> Result<CpuSample, Failure> {
        check("collect", unsafe { PdhCollectQueryData(self.handle) })?;
        let now = Instant::now();
        let interval = self
            .previous
            .replace(now)
            .map(|previous| now.duration_since(previous).as_millis() as u64);
        let Some(interval_ms) = interval.filter(|ms| (100..=5_000).contains(ms)) else {
            // PDH needs two samples. Long pauses (sleep/disabled monitoring)
            // replace the baseline instead of averaging activity over the gap.
            return Ok(CpuSample::Baseline);
        };
        let mut value = PDH_FMT_COUNTERVALUE::default();
        let code = unsafe {
            PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, ptr::null_mut(), &mut value)
        };
        if code != 0 || !matches!(value.CStatus, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA) {
            return Err(Failure {
                stage: "format",
                code,
                status: value.CStatus,
            });
        }
        let raw = unsafe { value.Anonymous.doubleValue };
        if !raw.is_finite() || raw < 0.0 {
            return Err(Failure {
                stage: "value",
                code: PDH_INVALID_DATA,
                status: value.CStatus,
            });
        }
        // Turbo can legitimately exceed nominal capacity. Match a percentage
        // gauge's 0..100 range without treating an invalid sample as idle.
        Ok(CpuSample::Percent {
            used: raw.min(100.0),
            interval_ms,
        })
    }
}

pub struct CpuReader {
    preferred: Source,
    query: Option<Query>,
    retry_at: Option<Instant>,
    failure: Option<Failure>,
    active: Option<Source>,
    summary_at: Instant,
    samples: u64,
    sum: f64,
    peak: f64,
}
impl Default for CpuReader {
    fn default() -> Self {
        let version = version();
        let preferred = preferred_source(version);
        log::info!("cpu_source_selected source={preferred:?} windows_version={version:?} policy=task_manager_build fallback=GetSystemTimes");
        Self {
            preferred,
            query: None,
            retry_at: None,
            failure: None,
            active: None,
            summary_at: Instant::now(),
            samples: 0,
            sum: 0.0,
            peak: 0.0,
        }
    }
}
impl CpuReader {
    pub fn reset(&mut self) {
        if let Some(query) = self.query.as_mut() {
            query.previous = None;
        }
        self.reset_summary();
    }
    fn reset_summary(&mut self) {
        // A summary describes one uninterrupted native sampling window. Do not
        // carry old peaks across disabled monitoring, sleep or provider failure.
        self.summary_at = Instant::now();
        self.samples = 0;
        self.sum = 0.0;
        self.peak = 0.0;
    }
    pub fn read(&mut self) -> PlatformResult<CpuSample> {
        let now = Instant::now();
        if self.query.is_none() && self.retry_at.is_none_or(|due| now >= due) {
            match Query::open(self.preferred) {
                Ok(query) => {
                    self.query = Some(query);
                    self.retry_at = None;
                }
                Err(failure) => self.fallback(failure),
            }
        }
        if let Some(query) = self.query.as_mut() {
            match query.read() {
                Ok(sample) => {
                    if let CpuSample::Percent { used, .. } = sample {
                        if self.active != Some(self.preferred) {
                            log::info!(
                                "cpu_source_active source={:?} recovered={}",
                                self.preferred,
                                self.failure.is_some()
                            );
                            self.active = Some(self.preferred);
                        }
                        self.failure = None;
                        self.samples += 1;
                        self.sum += used;
                        self.peak = self.peak.max(used);
                        log::debug!("cpu_sample source={:?} percent={used:.2}", self.preferred);
                        let window = self.summary_at.elapsed();
                        if window >= Duration::from_secs(60) {
                            log::info!("cpu_sample_summary source={:?} window_ms={} samples={} mean_percent={:.2} peak_percent={:.2}", self.preferred, window.as_millis(), self.samples, self.sum / self.samples as f64, self.peak);
                            self.reset_summary();
                        }
                    } else {
                        self.reset_summary();
                    }
                    return Ok(sample);
                }
                Err(failure) => self.fallback(failure),
            }
        }
        super::read().map(CpuSample::Counters)
    }
    fn fallback(&mut self, failure: Failure) {
        // Provider failures must not spawn/initialize a query every UI tick.
        // Log transitions, preserve the native code and retry at a bounded rate.
        if self.failure != Some(failure) {
            log::warn!("cpu_source_fallback requested={:?} source=GetSystemTimes stage={} code=0x{:08x} status=0x{:08x} retry_ms=30000", self.preferred, failure.stage, failure.code, failure.status);
        }
        self.failure = Some(failure);
        self.active = None;
        self.query = None;
        self.retry_at = Some(Instant::now() + Duration::from_secs(30));
        self.reset_summary();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_sampling_discards_previous_summary() {
        let mut reader = CpuReader::default();
        for fallback in [false, true] {
            reader.summary_at = Instant::now() - Duration::from_secs(120);
            reader.samples = 10;
            reader.sum = 900.0;
            reader.peak = 100.0;
            if fallback {
                reader.fallback(Failure {
                    stage: "test",
                    code: PDH_CSTATUS_NO_COUNTER,
                    status: 0,
                });
            } else {
                reader.reset();
            }
            assert_eq!(
                reader.samples, 0,
                "a new session must not reuse old samples"
            );
            assert_eq!(reader.sum, 0.0);
            assert_eq!(reader.peak, 0.0);
            assert!(reader.summary_at.elapsed() < Duration::from_secs(60));
        }
    }

    #[test]
    fn task_manager_policy_preserves_legacy_utility_and_modern_time() {
        for version in [
            None,
            Some((19045, 6466)),
            Some((22631, 6000)),
            Some((26100, 3623)),
        ] {
            assert_eq!(preferred_source(version), Source::ProcessorUtility);
        }
        for version in [Some((26100, 3624)), Some((26200, 8875))] {
            assert_eq!(preferred_source(version), Source::ProcessorTime);
        }
    }
    #[test]
    fn native_query_primes_and_restarts_after_reset_and_sleep() {
        let mut reader = CpuReader::default();
        assert!(matches!(reader.read().unwrap(), CpuSample::Baseline));
        std::thread::sleep(Duration::from_millis(1100));
        assert!(
            matches!(reader.read().unwrap(), CpuSample::Percent { used, .. } if used.is_finite() && (0.0..=100.0).contains(&used))
        );
        reader.reset();
        assert!(matches!(reader.read().unwrap(), CpuSample::Baseline));
        reader.query.as_mut().unwrap().previous = Some(Instant::now() - Duration::from_secs(60));
        reader.samples = 10;
        reader.sum = 900.0;
        reader.peak = 100.0;
        assert!(matches!(reader.read().unwrap(), CpuSample::Baseline));
        assert_eq!(reader.samples, 0);
        assert_eq!(reader.sum, 0.0);
        assert_eq!(reader.peak, 0.0);
    }
    #[test]
    fn native_provider_failure_falls_back_and_can_recover() {
        let mut reader = CpuReader::default();
        reader.fallback(Failure {
            stage: "test",
            code: PDH_CSTATUS_NO_COUNTER,
            status: 0,
        });
        if unsafe { windows_sys::Win32::System::Threading::GetActiveProcessorGroupCount() } > 1 {
            // The fallback deliberately refuses partial processor-group totals.
            assert_eq!(
                reader.read().unwrap_err().code(),
                crate::PlatformErrorCode::Unsupported
            );
        } else {
            assert!(matches!(reader.read().unwrap(), CpuSample::Counters(_)));
        }
        assert!(reader.query.is_none());
        reader.retry_at = Some(Instant::now());
        assert!(matches!(reader.read().unwrap(), CpuSample::Baseline));
        std::thread::sleep(Duration::from_millis(1100));
        assert!(matches!(reader.read().unwrap(), CpuSample::Percent { .. }));
        assert!(reader.failure.is_none());
    }
    #[test]
    #[ignore = "manual Windows CPU comparison under controlled load"]
    fn native_cpu_comparison_probe() {
        let mut reader = CpuReader::default();
        for _ in 0..65 {
            let start = Instant::now();
            let sample = reader.read().unwrap();
            eprintln!(
                "CPU_PROBE timestamp_ms={} source={:?} sample={sample:?} query_us={}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
                reader.preferred,
                start.elapsed().as_micros()
            );
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}
