/// Bounds snapshot transport and follow-up authority independently of virtual row rendering.
pub(crate) const ANALYSIS_VISIBLE_ENTRY_LIMIT: usize = 500;

use serde::{Deserialize, Serialize};

/// Selects the byte metric without changing traversal or deletion safety boundaries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisScanMode {
    #[default]
    Standard,
    Fast,
}

impl AnalysisScanMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Fast => "fast",
        }
    }

    pub(crate) fn file_usage(
        self,
        path: &std::path::Path,
        metadata: &std::fs::Metadata,
    ) -> mangodisk_platform::FileSpaceUsage {
        match self {
            Self::Standard => mangodisk_platform::Platform::file_space_usage(
                &mangodisk_platform::current_platform(),
                path,
                metadata,
            ),
            Self::Fast => mangodisk_platform::FileSpaceUsage::logical_only(metadata.len()),
        }
    }

    pub(crate) const fn displayed_bytes(self, usage: mangodisk_platform::FileSpaceUsage) -> u64 {
        match self {
            Self::Standard => usage.allocated_bytes,
            Self::Fast => usage.logical_bytes,
        }
    }
}

/// A bounded hierarchy projection of the same index used by the flat result.
/// Omitted files and directories remain part of the parent's byte total.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDirectoryNode {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    /// Positive-byte direct children before hierarchy and viewport filtering.
    pub total_entry_count: u64,
    pub children: Vec<AnalysisDirectoryNode>,
    /// Read-only file projections share the child and node budgets with directories.
    pub files: Vec<DirectoryEntryInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntryInfo {
    pub name: String,
    pub path: String,
    /// Bytes in the owning result's scan metric.
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
    pub scan_mode: AnalysisScanMode,
    pub scan_id: u64,
    pub root: String,
    pub scanned_at_ms: u64,
    /// Sum in the declared scan metric, not a live measurement of reclaimed space.
    pub total_bytes: u64,
    /// Positive-byte direct children, counted before the bounded projection.
    pub total_entry_count: usize,
    pub skipped_count: u64,
    /// True when direct children were omitted from the displayed result.
    pub truncated: bool,
    pub entries: Vec<DirectoryEntryInfo>,
    /// Up to six directory levels, bounded independently of the full scan index.
    pub directory_hierarchy: Vec<AnalysisDirectoryNode>,
    /// Zero-charge aliases require allocation to be reassigned after deletion.
    /// Keep this with the authoritative session even when the index is evicted.
    #[serde(skip)]
    pub(crate) requires_delete_rescan: bool,
}

/// Reads omitted direct children without extending destructive-operation authority.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRemainderRequest {
    /// Unsupported wire schemas are rejected; details are never persisted.
    pub schema_version: u8,
    pub scan_id: u64,
    pub parent_path: String,
    pub visible_paths: Vec<String>,
    /// Displayed scan total for diagnostics only; live details may have changed.
    pub expected_bytes: u64,
    pub offset: usize,
    pub snapshot_id: Option<u64>,
}

impl AnalysisRemainderRequest {
    pub const SCHEMA_VERSION: u8 = 2;
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRemainderPage {
    pub snapshot_id: u64,
    pub schema_version: u8,
    pub parent_path: String,
    pub total_bytes: u64,
    pub total_count: usize,
    pub entries: Vec<DirectoryEntryInfo>,
    pub next_offset: Option<usize>,
}

pub(crate) struct AnalysisRemainderParent {
    pub(crate) scan_mode: AnalysisScanMode,
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) exclusions: crate::filesystem::ScanExclusionOptions,
}

/// Captures an entry from an authoritative analysis snapshot.
#[derive(Debug, Clone)]
pub(crate) struct AnalysisEntryCandidate {
    pub(crate) scan_mode: AnalysisScanMode,
    pub(crate) requires_rescan: bool,
    pub(crate) exclusions: crate::filesystem::ScanExclusionOptions,
    pub(crate) root: String,
    pub(crate) path: String,
    pub(crate) expected_logical_bytes: u64,
    pub(crate) expected_displayed_bytes: u64,
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
    /// Scan-time bytes in the source result's metric to remove from the snapshot.
    /// This is not a live measurement of storage reclaimed by the filesystem.
    pub released_bytes: u64,
    /// Scan-time file count used only for snapshot reconciliation.
    pub removed_file_count: u64,
}
