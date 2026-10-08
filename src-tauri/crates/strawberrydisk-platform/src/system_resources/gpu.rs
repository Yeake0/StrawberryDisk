//! System-wide GPU activity. Unavailable counters never imply an idle device.
use crate::{PlatformError, PlatformErrorCode, PlatformResult};
pub mod details;
#[cfg(any(target_os = "macos", windows))]
mod observation_diagnostics;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::GpuReader;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_counter;
#[cfg(windows)]
mod windows_details;
#[cfg(windows)]
mod windows_metadata;
#[cfg(windows)]
pub use windows::GpuReader;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuAdapter {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct GpuAdapterUsage {
    pub id: String,
    pub name: String,
    pub used_percent: f64,
    pub details: Option<details::GpuDetails>,
}

#[derive(Debug, Clone)]
pub enum GpuSample {
    Baseline,
    Usage(Vec<GpuAdapterUsage>),
}

#[cfg(not(any(target_os = "macos", windows)))]
#[derive(Default)]
pub struct GpuReader {}

#[cfg(not(any(target_os = "macos", windows)))]
impl GpuReader {
    pub fn read(&mut self) -> PlatformResult<GpuSample> {
        Err(unsupported())
    }
    pub fn read_detailed(&mut self) -> PlatformResult<GpuSample> {
        Err(unsupported())
    }
    pub fn reset(&mut self) {}
    pub fn catalogue(&mut self) -> PlatformResult<Vec<GpuAdapter>> {
        Err(unsupported())
    }
    pub fn adapters(&self) -> Vec<GpuAdapter> {
        Vec::new()
    }
}

fn unsupported() -> PlatformError {
    PlatformError::new(
        PlatformErrorCode::Unsupported,
        "GPU activity counters unavailable",
    )
}

#[cfg(any(target_os = "macos", windows))]
fn failed(stage: &str, code: impl std::fmt::Display) -> PlatformError {
    PlatformError::new(
        PlatformErrorCode::OperationFailed,
        format!("GPU sampling stage={stage} native_code={code}"),
    )
}
