//! Optional adapter facts. Unsupported memory is distinct from a failed observation.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuActivityKind {
    Graphics,
    Copy,
    VideoDecode,
    VideoEncode,
    VideoProcessing,
    Other,
    Renderer,
    Tiler,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuActivity {
    pub id: String,
    pub kind: GpuActivityKind,
    pub name: Option<String>,
    pub used_percent: f64,
    pub included_in_summary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuMemoryArchitecture {
    Unified,
    Dedicated,
    Shared,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuMemory {
    pub dedicated_used_bytes: Option<u64>,
    pub dedicated_total_bytes: Option<u64>,
    pub shared_used_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GpuMemoryStatus {
    Ready,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuTelemetry {
    pub core_count: Option<u32>,
    pub temperature_celsius: Option<f64>,
    pub core_clock_mhz: Option<f64>,
    pub memory_clock_mhz: Option<f64>,
    pub fan_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDetails {
    pub activities: Vec<GpuActivity>,
    pub telemetry: GpuTelemetry,
    pub memory_architecture: GpuMemoryArchitecture,
    pub memory_status: GpuMemoryStatus,
    pub memory: Option<GpuMemory>,
}
