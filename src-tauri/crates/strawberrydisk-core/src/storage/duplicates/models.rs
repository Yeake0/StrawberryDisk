use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DuplicateScanLocationMode {
    Cleanable,
    Protected,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateScanLocation {
    pub path: String,
    pub mode: DuplicateScanLocationMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DuplicateEntryDeletePolicy {
    Cleanable,
    Protected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DuplicateGroupKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFileEntry {
    pub name: String,
    pub path: String,
    pub parent_path: String,
    /// Logical content length used for duplicate identity and delete preflight.
    pub bytes: u64,
    /// Physical storage charged to the volume and used for disk-space estimates.
    pub allocated_bytes: u64,
    pub modified_at_ms: Option<u64>,
    pub delete_policy: DuplicateEntryDeletePolicy,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub id: String,
    /// Stable proof token for this scan result. Ordinary file groups expose their BLAKE3 content
    /// digest; a group certified as entirely sparse uses a domain-separated layout proof token.
    /// Consumers must treat the value as opaque and must not reuse it as a file-content checksum.
    pub hash: String,
    pub kind: DuplicateGroupKind,
    /// Logical content length represented by each entry.
    pub bytes_per_file: u64,
    /// Number of regular files represented by one entry. File groups always use one.
    pub file_count_per_entry: u64,
    /// Maximum physical storage that can be released while preserving one entry.
    pub reclaimable_bytes: u64,
    pub entries: Vec<DuplicateFileEntry>,
}

impl DuplicateGroup {
    /// Returns the physical storage represented by every visible copy.
    pub(super) fn total_allocated_bytes(&self) -> u64 {
        self.entries
            .iter()
            .map(|entry| entry.allocated_bytes)
            .fold(0_u64, u64::saturating_add)
    }

    /// Returns the largest physical amount allowed by the scan's protection policy.
    /// Protected entries are always retained; without them, one fallback copy is retained.
    pub(super) fn maximum_reclaimable_bytes(&self) -> u64 {
        if self.entries.len() < 2 {
            return 0;
        }
        let protected_count = self
            .entries
            .iter()
            .filter(|entry| entry.delete_policy == DuplicateEntryDeletePolicy::Protected)
            .count();
        if protected_count > 0 {
            return self
                .entries
                .iter()
                .filter(|entry| entry.delete_policy == DuplicateEntryDeletePolicy::Cleanable)
                .map(|entry| entry.allocated_bytes)
                .fold(0_u64, u64::saturating_add);
        }
        self.total_allocated_bytes().saturating_sub(
            self.entries
                .iter()
                .map(|entry| entry.allocated_bytes)
                .min()
                .unwrap_or(0),
        )
    }

    pub(super) fn refresh_reclaimable_bytes(&mut self) {
        self.reclaimable_bytes = self.maximum_reclaimable_bytes();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFilesResult {
    pub scan_id: u64,
    pub roots: Vec<String>,
    pub protected_roots: Vec<String>,
    pub scanned_at_ms: u64,
    pub scanned_file_count: u64,
    pub skipped_count: u64,
    pub duplicate_file_count: u64,
    /// Physical storage charged to all duplicate entries in the result.
    pub total_duplicate_bytes: u64,
    /// Maximum physical storage that can be released across all groups.
    pub reclaimable_bytes: u64,
    pub total_group_count: u64,
    pub returned_group_count: u64,
    pub truncated: bool,
    pub groups: Vec<DuplicateGroup>,
}

/// Streams only groups that passed exact content verification by hashing or a platform proof.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroupBatch {
    pub operation_id: u64,
    pub sequence: u64,
    pub groups: Vec<DuplicateGroup>,
    pub found_group_count: u64,
    pub found_file_count: u64,
    /// Physical storage charged to duplicate entries streamed so far.
    pub found_total_bytes: u64,
    /// Maximum physical storage reclaimable from groups streamed so far.
    pub found_reclaimable_bytes: u64,
    pub elapsed_ms: u64,
}

/// Pages duplicate groups from the in-memory session created by the current scan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroupPage {
    pub scan_id: u64,
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub total_count: u64,
    pub groups: Vec<DuplicateGroup>,
}
