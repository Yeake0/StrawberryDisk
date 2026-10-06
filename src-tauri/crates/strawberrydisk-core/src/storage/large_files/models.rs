use serde::{Deserialize, Serialize};

const LARGE_FILE_RESULT_LIMIT: usize = 2_000;

/// Every scan retains candidates from this fixed floor. UI threshold changes therefore remain a
/// deterministic view over the active scan and never need to touch the filesystem again.
pub(crate) const LARGE_FILE_CANDIDATE_FLOOR_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LargeFileScanMode {
    /// Uses the operating system's metadata index and may omit files that have not been indexed.
    Quick,
    /// Enumerates the selected filesystem scope and is authoritative for reachable files.
    Complete,
}

impl LargeFileScanMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFileEntry {
    pub name: String,
    pub path: String,
    pub parent_path: String,
    /// Physical storage charged to the volume and shown in cleanup estimates.
    pub bytes: u64,
    /// Logical content length retained for delete preflight.
    #[serde(skip)]
    pub(crate) logical_bytes: u64,
    pub modified_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFilesResult {
    pub scan_id: u64,
    pub roots: Vec<String>,
    /// Full candidate set retained by the Core session and never serialized to the WebView.
    ///
    /// Keeping this snapshot in the large-file domain avoids replacing the independent disk
    /// analysis cache when the user switches scan modes or thresholds.
    #[serde(skip)]
    pub(crate) retained_entries: Vec<LargeFileEntry>,
    pub scanned_at_ms: u64,
    pub scan_mode: LargeFileScanMode,
    /// Physical-size threshold used by large-file discovery.
    pub minimum_bytes: u64,
    /// Physical storage charged to all returned large files.
    pub total_bytes: u64,
    pub total_count: u64,
    pub returned_count: u64,
    pub truncated: bool,
    pub skipped_count: u64,
    pub entries: Vec<LargeFileEntry>,
}

impl LargeFilesResult {
    pub(crate) fn from_retained_entries(
        roots: Vec<String>,
        scanned_at_ms: u64,
        scan_mode: LargeFileScanMode,
        minimum_bytes: u64,
        skipped_count: u64,
        mut retained_entries: Vec<LargeFileEntry>,
    ) -> Self {
        // Apply the shared result limit only after globally ordering all selected roots.
        retained_entries.sort_by(|left, right| {
            right
                .bytes
                .cmp(&left.bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        let minimum_bytes = minimum_bytes.max(LARGE_FILE_CANDIDATE_FLOOR_BYTES);
        let mut entries = retained_entries
            .iter()
            .filter(|entry| entry.bytes >= minimum_bytes)
            .cloned()
            .collect::<Vec<_>>();
        let total_count = entries.len() as u64;
        let total_bytes = entries.iter().map(|entry| entry.bytes).sum();
        entries.truncate(LARGE_FILE_RESULT_LIMIT);
        let returned_count = entries.len() as u64;

        Self {
            scan_id: 0,
            roots,
            retained_entries,
            scanned_at_ms,
            scan_mode,
            minimum_bytes,
            total_bytes,
            total_count,
            returned_count,
            truncated: returned_count < total_count,
            skipped_count,
            entries,
        }
    }

    pub(crate) fn filtered(&self, minimum_bytes: u64) -> Self {
        Self::from_retained_entries(
            self.roots.clone(),
            self.scanned_at_ms,
            self.scan_mode,
            minimum_bytes,
            self.skipped_count,
            self.retained_entries.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_all_roots_before_applying_the_shared_result_limit() {
        let roots = vec!["/first".to_string(), "/second".to_string()];
        let mut entries = (0..LARGE_FILE_RESULT_LIMIT)
            .map(|index| LargeFileEntry {
                name: format!("{index}.bin"),
                path: format!("/first/{index}.bin"),
                parent_path: "/first".to_string(),
                bytes: LARGE_FILE_CANDIDATE_FLOOR_BYTES,
                logical_bytes: LARGE_FILE_CANDIDATE_FLOOR_BYTES,
                modified_at_ms: None,
            })
            .collect::<Vec<_>>();
        entries.push(LargeFileEntry {
            name: "largest.bin".to_string(),
            path: "/second/largest.bin".to_string(),
            parent_path: "/second".to_string(),
            bytes: LARGE_FILE_CANDIDATE_FLOOR_BYTES * 2,
            logical_bytes: LARGE_FILE_CANDIDATE_FLOOR_BYTES * 2,
            modified_at_ms: None,
        });
        let result = LargeFilesResult::from_retained_entries(
            roots.clone(),
            1,
            LargeFileScanMode::Complete,
            1,
            0,
            entries,
        );
        assert_eq!(result.total_count, LARGE_FILE_RESULT_LIMIT as u64 + 1);
        assert_eq!(result.entries.len(), LARGE_FILE_RESULT_LIMIT);
        assert!(result.truncated);
        assert_eq!(result.entries[0].path, "/second/largest.bin");
        assert_eq!(
            result
                .filtered(LARGE_FILE_CANDIDATE_FLOOR_BYTES * 2)
                .entries
                .len(),
            1
        );
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["roots"], serde_json::json!(roots));
        assert!(wire.get("root").is_none());
        assert!(wire.get("retainedEntries").is_none());
    }
}
