use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntryInfo {
    pub name: String,
    pub path: String,
    /// Physical storage charged to the volume and shown by disk analysis.
    pub bytes: u64,
    /// Logical content length retained for delete preflight and cache updates.
    #[serde(skip)]
    pub(crate) logical_bytes: u64,
    pub file_count: u64,
    pub is_directory: bool,
    pub modified_at_ms: Option<u64>,
    pub content_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub scan_id: u64,
    pub root: String,
    pub scanned_at_ms: u64,
    /// Physical storage charged to all direct result entries.
    pub total_bytes: u64,
    pub skipped_count: u64,
    /// True when direct children were omitted from the displayed result.
    pub truncated: bool,
    pub entries: Vec<DirectoryEntryInfo>,
    /// Zero-charge aliases require allocation to be reassigned after deletion.
    /// Keep this with the authoritative session even when the index is evicted.
    #[serde(skip)]
    pub(crate) requires_delete_rescan: bool,
}

/// Captures an entry from an authoritative analysis snapshot.
#[derive(Debug, Clone)]
pub(crate) struct AnalysisEntryCandidate {
    pub(crate) requires_rescan: bool,
    pub(crate) exclusions: crate::filesystem::ScanExclusionOptions,
    pub(crate) root: String,
    pub(crate) path: String,
    pub(crate) expected_logical_bytes: u64,
    pub(crate) expected_allocated_bytes: u64,
    pub(crate) expected_file_count: u64,
    pub(crate) is_directory: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDeleteResult {
    pub removed_path: String,
    /// The original path changed or shared allocation must be reassigned.
    /// Clients must discard navigation snapshots before refreshing this result.
    pub requires_rescan: bool,
    /// Scan-time allocated bytes to remove from the displayed snapshot.
    /// This is not a live measurement of storage reclaimed by the filesystem.
    pub released_bytes: u64,
    /// Scan-time file count used only for snapshot reconciliation.
    pub removed_file_count: u64,
}
