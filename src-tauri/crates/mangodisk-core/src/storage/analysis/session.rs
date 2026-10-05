use std::{
    collections::{HashSet, VecDeque},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
};

use mangodisk_platform::{current_platform, Platform};

use super::{
    AnalysisDirectoryNode, AnalysisEntryCandidate, AnalysisRemainderParent, AnalysisResult,
    DirectoryEntryInfo,
};

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
        scan_mode: result.result.scan_mode,
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
        expected_displayed_bytes: entry.bytes,
        expected_file_count: entry.file_count,
        is_directory: entry.is_directory,
    })
}

fn find_directory<'a>(
    nodes: &'a [AnalysisDirectoryNode],
    path: &str,
) -> Option<&'a AnalysisDirectoryNode> {
    for node in nodes {
        if current_platform().paths_equal(Path::new(&node.path), Path::new(path)) {
            return Some(node);
        }
        if let Some(found) = find_directory(&node.children, path) {
            return Some(found);
        }
    }
    None
}

fn find_projected_file<'a>(
    nodes: &'a [AnalysisDirectoryNode],
    path: &str,
) -> Option<&'a DirectoryEntryInfo> {
    for node in nodes {
        if let Some(file) = node
            .files
            .iter()
            .find(|file| current_platform().paths_equal(Path::new(&file.path), Path::new(path)))
        {
            return Some(file);
        }
        if let Some(file) = find_projected_file(&node.children, path) {
            return Some(file);
        }
    }
    None
}

/// Read-only open actions can target published hierarchy files and directories. The
/// complete entry resolver remains the only authority for destructive actions.
pub(super) fn resolve_open_path(scan_id: u64, selected_path: &str) -> Result<String, String> {
    let sessions = lock_sessions()?;
    let result = &sessions
        .iter()
        .find(|session| session.result.scan_id == scan_id)
        .ok_or_else(|| "the disk-analysis result session expired; scan again".to_string())?
        .result;
    result
        .entries
        .iter()
        .find(|entry| entry.path == selected_path)
        .map(|entry| entry.path.clone())
        .or_else(|| {
            find_directory(&result.directory_hierarchy, selected_path).map(|node| node.path.clone())
        })
        .or_else(|| {
            find_projected_file(&result.directory_hierarchy, selected_path)
                .map(|file| file.path.clone())
        })
        .ok_or_else(|| "the selected item is not part of the current disk analysis".to_string())
}

/// Only published parents can supply read-only remainder details.
pub(super) fn resolve_remainder_parent(
    scan_id: u64,
    path: &str,
) -> Result<AnalysisRemainderParent, String> {
    let sessions = lock_sessions()?;
    let session = sessions
        .iter()
        .find(|session| session.result.scan_id == scan_id)
        .ok_or_else(|| "the disk-analysis result session expired; scan again".to_string())?;
    let result = &session.result;
    let (path, bytes) = if current_platform().paths_equal(Path::new(&result.root), Path::new(path))
    {
        (result.root.clone(), result.total_bytes)
    } else {
        find_directory(&result.directory_hierarchy, path)
            .map(|node| (node.path.clone(), node.bytes))
            .ok_or_else(|| {
                "the remainder parent is not part of the current disk analysis".to_string()
            })?
    };
    Ok(AnalysisRemainderParent {
        scan_mode: result.scan_mode,
        path,
        bytes,
        exclusions: session.exclusions.clone(),
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
    if displayed_bytes > 0 {
        source.total_entry_count = source.total_entry_count.saturating_sub(1);
    }
    source
        .directory_hierarchy
        .retain(|node| !current_platform().paths_equal(Path::new(&node.path), removed_path));

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
            scan_mode: Default::default(),
            scan_id: 0,
            root: "/fixture".to_string(),
            scanned_at_ms: 1,
            total_bytes: 4,
            total_entry_count: 1,
            skipped_count: 0,
            truncated: false,
            directory_hierarchy: Vec::new(),
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
        let _operation_lock = crate::shared::operation::test_operation_lock();
        let result = publish_result_session(result("/fixture/sample.bin"))
            .expect("publish the analysis fixture");

        let candidate = resolve_entry_candidate(result.scan_id, "/fixture/sample.bin")
            .expect("resolve the published entry");
        assert_eq!(candidate.expected_logical_bytes, 12);
        assert_eq!(candidate.expected_displayed_bytes, 4);
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

    #[test]
    fn hierarchy_open_targets_do_not_authorize_deletion_or_unpublished_descendants() {
        let _operation_lock = crate::shared::operation::test_operation_lock();
        let mut fixture = result("/context-fixture/sample.bin");
        fixture.root = "/context-fixture".into();
        fixture.directory_hierarchy = vec![AnalysisDirectoryNode {
            name: "A".into(),
            path: "/context-fixture/A".into(),
            bytes: 4,
            file_count: 1,
            total_entry_count: 1,
            children: vec![AnalysisDirectoryNode {
                name: "B".into(),
                path: "/context-fixture/A/B".into(),
                bytes: 4,
                file_count: 1,
                total_entry_count: 0,
                children: vec![],
                files: vec![],
            }],
            files: vec![],
        }];
        let published = publish_result_session(fixture).unwrap();
        assert_eq!(
            resolve_open_path(published.scan_id, "/context-fixture/sample.bin").unwrap(),
            "/context-fixture/sample.bin"
        );
        assert_eq!(
            resolve_open_path(published.scan_id, "/context-fixture/A/B").unwrap(),
            "/context-fixture/A/B"
        );
        assert!(resolve_entry_candidate(published.scan_id, "/context-fixture/A/B").is_err());
        for path in [
            "/context-fixture/A/B/unpublished.bin",
            "/context-fixture/A/B/../unknown",
            "/outside",
            "/context-fixture",
        ] {
            assert!(
                resolve_open_path(published.scan_id, path).is_err(),
                "unpublished target must fail: {path}"
            );
        }
        assert!(resolve_open_path(published.scan_id + 10000, "/context-fixture/A/B").is_err());
        invalidate_changed_path(Path::new("/context-fixture/A")).unwrap();
        assert!(resolve_open_path(published.scan_id, "/context-fixture/A/B").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn canonical_deleted_path_updates_display_path_session() {
        let _operation_lock = crate::shared::operation::test_operation_lock();
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
