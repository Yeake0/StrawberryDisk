use std::{
    collections::{HashSet, VecDeque},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
};

use mangodisk_platform::{current_platform, Platform};

use super::{AnalysisEntryCandidate, AnalysisResult};

const ANALYSIS_RESULT_SESSION_LIMIT: usize = 80;

static NEXT_ANALYSIS_SCAN_ID: AtomicU64 = AtomicU64::new(1);
static ANALYSIS_RESULT_SESSIONS: OnceLock<Mutex<VecDeque<AnalysisSession>>> = OnceLock::new();

struct AnalysisSession {
    result: AnalysisResult,
    exclusions: crate::filesystem::ScanExclusionOptions,
}

fn sessions() -> &'static Mutex<VecDeque<AnalysisSession>> {
    ANALYSIS_RESULT_SESSIONS.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn lock_sessions() -> Result<MutexGuard<'static, VecDeque<AnalysisSession>>, String> {
    sessions()
        .lock()
        .map_err(|_| "the disk-analysis result session is unavailable".to_string())
}

/// Publishes an authoritative result snapshot used by trusted follow-up operations.
///
/// The UI keeps a bounded navigation cache, so Core retains the same number of recent snapshots.
/// A cached UI result therefore remains usable without trusting snapshots reconstructed by the
/// WebView.
pub(super) fn publish_result_session(result: AnalysisResult) -> Result<AnalysisResult, String> {
    publish_result_with_exclusions(result, Default::default())
}

pub(super) fn publish_result_with_exclusions(
    mut result: AnalysisResult,
    exclusions: crate::filesystem::ScanExclusionOptions,
) -> Result<AnalysisResult, String> {
    result.scan_id = NEXT_ANALYSIS_SCAN_ID.fetch_add(1, Ordering::Relaxed);
    let mut sessions = lock_sessions()?;
    sessions.retain(|session| {
        !current_platform().paths_equal(Path::new(&session.result.root), Path::new(&result.root))
    });
    sessions.push_front(AnalysisSession {
        result: result.clone(),
        exclusions,
    });
    sessions.truncate(ANALYSIS_RESULT_SESSION_LIMIT);
    Ok(result)
}

/// Resolves a UI selection back to the complete snapshot owned by Core.
pub(super) fn resolve_entry_candidate(
    scan_id: u64,
    selected_path: &str,
) -> Result<AnalysisEntryCandidate, String> {
    let sessions = lock_sessions()?;
    let result = sessions
        .iter()
        .find(|result| result.result.scan_id == scan_id)
        .ok_or_else(|| "the disk-analysis result session expired; scan again".to_string())?;
    let entry = result
        .result
        .entries
        .iter()
        .find(|entry| entry.path == selected_path)
        .ok_or_else(|| "the selected item is not part of the current disk analysis".to_string())?;
    Ok(AnalysisEntryCandidate {
        requires_rescan: result.result.requires_delete_rescan
            || sessions.iter().any(|session| {
                session.result.requires_delete_rescan
                    && current_platform().path_is_same_or_child(
                        Path::new(&result.result.root),
                        Path::new(&session.result.root),
                    )
            }),
        exclusions: result.exclusions.clone(),
        root: result.result.root.clone(),
        path: entry.path.clone(),
        expected_logical_bytes: entry.logical_bytes,
        expected_allocated_bytes: entry.bytes,
        expected_file_count: entry.file_count,
        is_directory: entry.is_directory,
    })
}

/// Expires authoritative snapshots whose contents may have changed after a failed delete.
pub(super) fn invalidate_changed_path(changed_path: &Path) -> Result<(), String> {
    let mut sessions = lock_sessions()?;
    sessions.retain(|session| {
        let root = Path::new(&session.result.root);
        !current_platform().path_is_same_or_child(root, changed_path)
            && !current_platform().path_is_same_or_child(changed_path, root)
    });
    Ok(())
}

/// Hard-link ownership can move between sibling snapshots, not just ancestors.
pub(super) fn invalidate_all() -> Result<(), String> {
    lock_sessions()?.clear();
    Ok(())
}

/// Removes the deleted item from its source session and expires overlapping snapshots.
///
/// Other cached roots may contain aggregate fingerprints that changed after this deletion.
/// Expiring them is safer than trying to synthesize a new fingerprint without rescanning.
pub(super) fn synchronize_removed_path(
    source_scan_id: u64,
    removed_path: &Path,
    released_bytes: u64,
) -> Result<(), String> {
    let mut sessions = lock_sessions()?;
    let source_index = sessions
        .iter()
        .position(|result| result.result.scan_id == source_scan_id)
        .ok_or_else(|| "the disk-analysis result session expired; scan again".to_string())?;
    let source_root = sessions[source_index].result.root.clone();
    let source = &mut sessions[source_index].result;
    let displayed_bytes = source
        .entries
        .iter()
        .find(|entry| current_platform().paths_equal(Path::new(&entry.path), removed_path))
        .map(|entry| entry.bytes)
        .unwrap_or(released_bytes);
    source
        .entries
        .retain(|entry| !current_platform().paths_equal(Path::new(&entry.path), removed_path));
    source.total_bytes = source.total_bytes.saturating_sub(displayed_bytes);

    let invalidated = sessions
        .iter()
        .filter(|session| {
            let result = &session.result;
            if result.scan_id == source_scan_id {
                return false;
            }
            let root = Path::new(&result.root);
            current_platform().path_is_same_or_child(root, removed_path)
                || current_platform().path_is_same_or_child(removed_path, root)
                || current_platform().paths_equal(Path::new(&result.root), Path::new(&source_root))
        })
        .map(|session| session.result.scan_id)
        .collect::<HashSet<_>>();
    sessions.retain(|session| !invalidated.contains(&session.result.scan_id));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::analysis::DirectoryEntryInfo;

    fn result(path: &str) -> AnalysisResult {
        AnalysisResult {
            scan_id: 0,
            root: "/fixture".to_string(),
            scanned_at_ms: 1,
            total_bytes: 4,
            skipped_count: 0,
            truncated: false,
            requires_delete_rescan: false,
            entries: vec![DirectoryEntryInfo {
                name: "sample.bin".to_string(),
                path: path.to_string(),
                bytes: 4,
                logical_bytes: 12,
                file_count: 1,
                is_directory: false,
                modified_at_ms: Some(7),
                content_fingerprint: None,
            }],
        }
    }

    #[test]
    fn entry_candidate_must_belong_to_the_authoritative_analysis_result() {
        let result = publish_result_session(result("/fixture/sample.bin"))
            .expect("publish the analysis fixture");

        let candidate = resolve_entry_candidate(result.scan_id, "/fixture/sample.bin")
            .expect("resolve the published entry");
        assert_eq!(candidate.expected_logical_bytes, 12);
        assert_eq!(candidate.expected_allocated_bytes, 4);
        assert!(
            resolve_entry_candidate(result.scan_id, "/fixture/not-scanned.bin").is_err(),
            "a fabricated path must not cross the analysis-result boundary"
        );
        assert!(
            resolve_entry_candidate(result.scan_id.saturating_add(10_000), &candidate.path)
                .is_err(),
            "an unknown scan identifier must be rejected"
        );
    }

    #[cfg(windows)]
    #[test]
    fn canonical_deleted_path_updates_display_path_session() {
        let root = std::env::temp_dir().join(format!(
            "mangodisk-analysis-session-{}-{}",
            std::process::id(),
            crate::filesystem::metadata::now_ms()
        ));
        std::fs::create_dir_all(&root).expect("the analysis session fixture should be created");
        let file = root.join("sample.bin");
        std::fs::write(&file, b"fixture").expect("the analysis session file should be written");
        let mut fixture = result(&file.to_string_lossy());
        fixture.root = root.to_string_lossy().into_owned();
        let published = publish_result_session(fixture).expect("publish the analysis session");
        let canonical =
            std::fs::canonicalize(&file).expect("the analysis session file should canonicalize");

        synchronize_removed_path(published.scan_id, &canonical, 12)
            .expect("the canonical deletion should update the display session");

        assert!(resolve_entry_candidate(published.scan_id, &file.to_string_lossy()).is_err());
        std::fs::remove_dir_all(root).expect("the analysis session fixture should be removed");
    }
}
