use crate::filesystem::ScanExclusionOptions;
use std::time::Instant;

use crate::{
    filesystem::{
        metadata::diagnostic_path, permanent_delete::delete_analysis_candidate_permanently,
    },
    shared::{
        operation::{CoordinatedOperationKind, OperationGuard},
        CoreResult, TraversalProgress,
    },
    storage::traversal::{AnalysisScanDiagnostics, StorageTraversal},
    storage::{
        analysis::{AnalysisDeleteResult, AnalysisResult},
        index::cache,
    },
    ProgressSink,
};

use super::session::{
    invalidate_changed_path, publish_result_session, resolve_entry_candidate,
    synchronize_removed_path,
};

pub struct AnalysisService;

impl AnalysisService {
    pub fn analyze_with_progress(
        path: Option<String>,
        refresh: bool,
        callback: impl ProgressSink,
    ) -> CoreResult<AnalysisResult> {
        let result =
            StorageTraversal::analyze_path_with_progress(path, refresh, move |progress| {
                callback.report(progress);
            })?;
        Ok(publish_result_session(result)?)
    }

    pub fn analyze_with_exclusions_progress(
        path: Option<String>,
        refresh: bool,
        excluded_paths: impl Into<ScanExclusionOptions>,
        callback: impl ProgressSink,
    ) -> CoreResult<AnalysisResult> {
        let excluded_paths = excluded_paths.into();
        let snapshot = StorageTraversal::analyze_path_with_exclusions_snapshot(
            path,
            refresh,
            excluded_paths,
            move |progress| callback.report(progress),
        )?;
        Ok(super::session::publish_result_with_exclusions(
            snapshot.result,
            snapshot.exclusions,
        )?)
    }

    pub(crate) fn analyze_with_diagnostics(
        path: Option<String>,
        refresh: bool,
        callback: impl Fn(TraversalProgress) + Send + Sync + 'static,
    ) -> CoreResult<(AnalysisResult, AnalysisScanDiagnostics)> {
        StorageTraversal::analyze_path_with_diagnostics(path, refresh, callback)
    }

    pub fn cancel() {
        StorageTraversal::cancel_analysis();
    }

    /// Resolves an external-open request against the authoritative scan snapshot.
    ///
    /// The platform adapter owns launching the system handler. Core only proves
    /// that the requested path was published to the current UI by a real scan.
    pub fn resolve_open_target(scan_id: u64, selected_path: String) -> CoreResult<String> {
        Ok(resolve_entry_candidate(scan_id, &selected_path)?.path)
    }

    pub fn delete_entry_permanently(
        scan_id: u64,
        selected_path: String,
    ) -> CoreResult<AnalysisDeleteResult> {
        let candidate = resolve_entry_candidate(scan_id, &selected_path)?;
        let operation = OperationGuard::start(CoordinatedOperationKind::PermanentDelete)?;
        let started = Instant::now();
        let is_directory = candidate.is_directory;
        let requires_allocation_rescan = candidate.requires_rescan;
        let selected_path = std::path::PathBuf::from(&candidate.path);
        log::info!(
            "analysis_permanent_delete_started operation_id={} scan_id={} path={} entry_kind={}",
            operation.id(),
            scan_id,
            diagnostic_path(std::path::Path::new(&candidate.path)),
            if is_directory { "directory" } else { "file" }
        );
        let mut outcome = match delete_analysis_candidate_permanently(candidate) {
            Ok(outcome) => outcome,
            Err(error) => {
                if error.is_partial() {
                    if let Err(session_error) = invalidate_changed_path(&selected_path) {
                        log::error!("analysis_partial_delete_session_invalidation_failed operation_id={} path={} error={}",
                            operation.id(), diagnostic_path(&selected_path), mangodisk_platform::diagnostics::text(&session_error));
                    }
                    // A partially changed directory no longer matches any derived index snapshot.
                    // Clearing the rebuildable cache prevents stale sizes from surviving the
                    // irreversible boundary.
                    let cache_started = Instant::now();
                    let cache_result = cache::clear_all();
                    log::info!(
                        "analysis_delete_stage_finished operation_id={} stage=invalidate_cache outcome={} elapsed_ms={}",
                        operation.id(), if cache_result.is_ok() { "completed" } else { "failed" },
                        cache_started.elapsed().as_millis()
                    );
                    if let Err(cache_error) = cache_result {
                        log::error!(
                            "analysis_partial_delete_cache_clear_failed operation_id={} scan_id={} error={}",
                            operation.id(),
                            scan_id,
                            mangodisk_platform::diagnostics::text(&cache_error)
                        );
                    }
                }
                log::warn!(
                    "analysis_permanent_delete_failed operation_id={} scan_id={} partial={} released_logical_bytes={:?} removed_files={:?} remaining_restored={} elapsed_ms={} error={}",
                    operation.id(),
                    scan_id,
                    error.is_partial(),
                    error.observed_or_estimated_bytes(),
                    error.observed_or_estimated_files(),
                    error.remaining_was_restored(),
                    started.elapsed().as_millis(),
                    mangodisk_platform::diagnostics::text(&error)
                );
                let mut core_error = crate::shared::CoreError::operation_failed(error.to_string());
                if let Some(reason) = error.reason() {
                    core_error = core_error.with_reason(reason);
                }
                if error.is_partial() {
                    core_error = core_error.with_possible_side_effects();
                    if !error.remaining_was_restored() {
                        core_error = core_error
                            .with_reason(crate::shared::CoreErrorReason::DeleteRecoveryFailed);
                    } else if error.reason()
                        != Some(crate::shared::CoreErrorReason::DirectoryNotEmpty)
                    {
                        core_error = core_error
                            .with_reason(crate::shared::CoreErrorReason::DeleteIncomplete);
                    }
                }
                return Err(core_error);
            }
        };
        let cache_started = Instant::now();
        // Staging frees the original name before recursive deletion completes.
        // A concurrently recreated entry belongs to another operation and must
        // remain visible; uncertainty also requires a fresh authoritative scan.
        outcome.result.requires_rescan =
            requires_allocation_rescan || original_path_requires_rescan(&outcome.target);
        if outcome.result.requires_rescan {
            let invalidation = if requires_allocation_rescan {
                super::session::invalidate_all()
            } else {
                invalidate_changed_path(&outcome.target)
            };
            invalidation.map_err(|error| {
                crate::shared::CoreError::operation_failed(error).with_possible_side_effects()
            })?;
            cache::clear_all().map_err(|error| {
                crate::shared::CoreError::operation_failed(error).with_possible_side_effects()
            })?;
            log::info!(
                "analysis_delete_rescan_required operation_id={} path={} shared_allocation={} action=rescan",
                operation.id(),
                diagnostic_path(&outcome.target),
                requires_allocation_rescan
            );
        } else {
            cache::remove_entry(
                &outcome.target,
                outcome.removed_usage,
                outcome.result.removed_file_count,
                is_directory,
            );
            synchronize_removed_path(scan_id, &outcome.target, outcome.result.released_bytes)
                .map_err(|error| {
                    crate::shared::CoreError::operation_failed(error).with_possible_side_effects()
                })?;
        }
        log::info!(
            "analysis_delete_stage_finished operation_id={} stage=synchronize_cache outcome=completed elapsed_ms={}",
            operation.id(), cache_started.elapsed().as_millis()
        );
        log::info!(
            "analysis_permanent_delete_finished operation_id={} scan_id={} path={} entry_kind={} snapshot_logical_bytes={} snapshot_allocated_bytes={} snapshot_file_count={} count_source=scan_snapshot elapsed_ms={}",
            operation.id(),
            scan_id,
            diagnostic_path(&outcome.target),
            if is_directory { "directory" } else { "file" },
            outcome.removed_usage.logical_bytes,
            outcome.result.released_bytes,
            outcome.result.removed_file_count,
            started.elapsed().as_millis()
        );
        operation.complete();
        Ok(outcome.result)
    }
}

fn original_path_requires_rescan(path: &std::path::Path) -> bool {
    !matches!(std::fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use super::*;

    struct AnalysisFixture {
        root: PathBuf,
    }

    impl AnalysisFixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "mangodisk-analysis-service-{}-{}",
                std::process::id(),
                crate::filesystem::metadata::now_ms()
            ));
            fs::create_dir_all(&root).expect("the analysis service fixture should be created");
            Self { root }
        }

        fn file(&self) -> PathBuf {
            self.root.join("candidate.bin")
        }
    }

    impl Drop for AnalysisFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(unix)]
    #[test]
    fn deleting_charged_hard_link_requires_reconciliation_of_surviving_alias() {
        let _operation_lock = crate::shared::operation::test_operation_lock();
        cache::clear_all().unwrap();
        let fixture = AnalysisFixture::new();
        let owner = fixture.root.join("a.bin");
        let alias = fixture.root.join("b.bin");
        fs::write(&owner, vec![7; 8192]).unwrap();
        fs::hard_link(&owner, &alias).unwrap();
        let result = AnalysisService::analyze_with_progress(
            Some(fixture.root.to_string_lossy().into_owned()),
            true,
            |_| {},
        )
        .unwrap();
        let owner = result
            .entries
            .iter()
            .find(|entry| entry.name == "a.bin")
            .unwrap();
        assert_eq!(owner.bytes, result.total_bytes);
        assert_eq!(
            result
                .entries
                .iter()
                .find(|entry| entry.name == "b.bin")
                .unwrap()
                .bytes,
            0
        );
        // The session must retain this fact even if another scan evicts its index.
        cache::clear_all().unwrap();
        let deleted =
            AnalysisService::delete_entry_permanently(result.scan_id, owner.path.clone()).unwrap();
        assert!(alias.exists());
        assert!(
            deleted.requires_rescan,
            "the surviving link still owns the same allocation"
        );
        let refreshed =
            AnalysisService::analyze_with_progress(Some(result.root), true, |_| {}).unwrap();
        assert_eq!(refreshed.total_bytes, result.total_bytes);
        assert_eq!(refreshed.entries.len(), 1);
        assert_eq!(refreshed.entries[0].name, "b.bin");
    }

    #[cfg(unix)]
    #[test]
    fn deleting_from_cached_child_invalidates_shared_sibling_sessions() {
        let _operation_lock = crate::shared::operation::test_operation_lock();
        for refresh in [false, true] {
            cache::clear_all().unwrap();
            let fixture = AnalysisFixture::new();
            let a = fixture.root.join("a");
            let b = fixture.root.join("b");
            fs::create_dir(&a).unwrap();
            fs::create_dir(&b).unwrap();
            fs::write(a.join("owner.bin"), vec![7; 8192]).unwrap();
            fs::hard_link(a.join("owner.bin"), b.join("alias.bin")).unwrap();
            AnalysisService::analyze_with_progress(
                Some(fixture.root.to_string_lossy().into_owned()),
                true,
                |_| {},
            )
            .unwrap();
            let sibling = AnalysisService::analyze_with_progress(
                Some(b.to_string_lossy().into_owned()),
                false,
                |_| {},
            )
            .unwrap();
            let owner = AnalysisService::analyze_with_progress(
                Some(a.to_string_lossy().into_owned()),
                refresh,
                |_| {},
            )
            .unwrap();
            assert_eq!(sibling.total_bytes, 0);
            cache::clear_all().unwrap();
            let deleted = AnalysisService::delete_entry_permanently(
                owner.scan_id,
                owner.entries[0].path.clone(),
            )
            .unwrap();
            assert!(deleted.requires_rescan);
            assert!(
                resolve_entry_candidate(sibling.scan_id, &sibling.entries[0].path).is_err(),
                "the sibling's zero-charge snapshot must expire"
            );
        }
    }

    #[test]
    fn recreated_original_path_requires_rescan_including_dangling_links() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("selected");
        assert!(!super::original_path_requires_rescan(&target));
        fs::create_dir(&target).unwrap();
        assert!(super::original_path_requires_rescan(&target));
        fs::remove_dir(&target).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.path().join("absent"), &target).unwrap();
            assert!(super::original_path_requires_rescan(&target));
        }
    }

    #[test]
    fn analysis_service_deletes_the_current_direct_child_and_synchronizes_its_session() {
        let _operation_lock = crate::shared::operation::test_operation_lock();
        cache::clear_all().expect("the analysis cache should be clear before the service test");
        let fixture = AnalysisFixture::new();
        let path = fixture.file();
        fs::write(&path, vec![1_u8; 16 * 1024]).expect("the analysis candidate should be written");
        let progress_events = Arc::new(Mutex::new(Vec::new()));
        let captured_events = Arc::clone(&progress_events);

        let initial = AnalysisService::analyze_with_progress(
            Some(fixture.root.to_string_lossy().into_owned()),
            true,
            move |progress| {
                captured_events
                    .lock()
                    .expect("the analysis progress fixture should remain available")
                    .push(progress)
            },
        )
        .expect("the analysis service should scan the isolated fixture");
        let selected_path = initial
            .entries
            .iter()
            .find(|entry| entry.name == "candidate.bin")
            .expect("the analysis result should contain the fixture file")
            .path
            .clone();
        assert_eq!(
            AnalysisService::resolve_open_target(initial.scan_id, selected_path.clone())
                .expect("the published analysis entry should resolve"),
            selected_path
        );
        assert!(
            !progress_events
                .lock()
                .expect("the analysis progress fixture should remain readable")
                .is_empty(),
            "the service adapter must forward traversal progress"
        );

        assert!(
            AnalysisService::delete_entry_permanently(
                initial.scan_id,
                fixture
                    .root
                    .join("fabricated.bin")
                    .to_string_lossy()
                    .into_owned(),
            )
            .is_err(),
            "the service must reject a path that was not published by the scan"
        );
        // Analysis deletion intentionally authorizes the current regular direct child even when
        // it changed after measurement. The permanent-delete boundary pins its physical identity
        // during execution; stale scan sizes are accounting facts rather than preflight gates.
        fs::write(&path, vec![2_u8; 32 * 1024])
            .expect("the analysis candidate should change after the scan");
        let deleted =
            AnalysisService::delete_entry_permanently(initial.scan_id, selected_path.clone())
                .expect("the current direct child should be deleted safely");

        assert_eq!(deleted.removed_path, selected_path);
        assert_eq!(deleted.removed_file_count, 1);
        assert!(!path.exists());
        assert!(
            AnalysisService::resolve_open_target(initial.scan_id, deleted.removed_path).is_err(),
            "a deleted entry must disappear from the authoritative result session"
        );
        cache::clear_all().expect("the analysis cache should be clear after the service test");
    }
    #[cfg(unix)]
    #[test]
    fn failed_native_directory_delete_invalidates_sessions_even_without_known_counts() {
        use std::os::unix::fs::PermissionsExt;
        let _operation_lock = crate::shared::operation::test_operation_lock();
        cache::clear_all().unwrap();
        let fixture = AnalysisFixture::new();
        let directory = fixture.root.join("directory");
        let locked = directory.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("retained"), b"keep").unwrap();
        let initial = AnalysisService::analyze_with_progress(
            Some(fixture.root.to_string_lossy().into_owned()),
            true,
            |_| {},
        )
        .unwrap();
        let selected = initial
            .entries
            .iter()
            .find(|entry| entry.name == "directory")
            .unwrap()
            .path
            .clone();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o0)).unwrap();
        let result = AnalysisService::delete_entry_permanently(initial.scan_id, selected.clone());
        // Restore fixture access before assertions so a failed assertion cannot
        // leave an unreadable temporary tree behind.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        let error = result.expect_err("unreadable contents must stop deletion");
        assert_eq!(
            error.mutation_state(),
            mangodisk_platform::PlatformMutationState::MayHaveChanged
        );
        assert_eq!(
            error.reason(),
            Some(crate::shared::CoreErrorReason::DeleteIncomplete)
        );
        assert!(locked.join("retained").exists());
        assert!(AnalysisService::resolve_open_target(initial.scan_id, selected).is_err());
        cache::clear_all().unwrap();
    }
}
