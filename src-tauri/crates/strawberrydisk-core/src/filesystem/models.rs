use serde::{Deserialize, Serialize};
use strawberrydisk_platform::VolumeInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

impl From<VolumeInfo> for DiskInfo {
    fn from(value: VolumeInfo) -> Self {
        Self {
            name: value.name,
            mount_point: value.mount_point,
            total_bytes: value.total_bytes,
            available_bytes: value.available_bytes,
            used_bytes: value.used_bytes,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermanentDeleteCandidate {
    pub path: String,
    /// Logical content length used only to reject stale file selections.
    pub expected_bytes: u64,
    pub expected_modified_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermanentDeleteFailure {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermanentDeleteBatchResult {
    pub removed_paths: Vec<String>,
    pub failed: Vec<PermanentDeleteFailure>,
    /// Physical storage released by successfully removed selections.
    pub released_bytes: u64,
}
