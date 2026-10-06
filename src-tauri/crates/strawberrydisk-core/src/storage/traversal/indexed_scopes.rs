//! Shares advisory index queries across overlapping roots without buffering unvalidated paths.
//! Every candidate is streamed into the most specific selected scope's existing safety checks.

use super::*;

pub(super) fn scan(
    roots: &[PathBuf],
    minimum_bytes: u64,
    excluded_paths: &crate::filesystem::ScanExclusionOptions,
    operation: &OperationGuard,
    callback: impl Fn(TraversalProgress) + Send + Sync + 'static,
) -> CoreResult<(LargeFilesResult, LargeFileScanDiagnostics)> {
    let started = Instant::now();
    let scanned_at_ms = now_ms();
    let exclusions = roots
        .iter()
        .map(|root| StorageScanExclusions::resolve_options(root, excluded_paths))
        .collect::<CoreResult<Vec<_>>>()?;
    // Total progress counts index queries, while the result retains all selected roots.
    let metadata = roots
        .iter()
        .map(fs::symlink_metadata)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CoreError::operation_failed(format!("failed to inspect index scopes: {error}"))
        })?;
    let sources = query_sources(roots, &metadata);
    let query_count = sources
        .iter()
        .enumerate()
        .filter(|(index, source)| *index == **source)
        .count();
    let progress = Arc::new(ProgressTracker::new(
        operation.id(),
        callback,
        query_count as u64,
    ));
    let mut validations = roots
        .iter()
        .zip(&exclusions)
        .map(|(root, exclusions)| {
            LargeFileStreamValidation::new(
                root,
                LARGE_FILE_CANDIDATE_FLOOR_BYTES,
                scanned_at_ms,
                &progress,
                operation.cancelled(),
                true,
                exclusions,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(traversal_core_error)?;
    let mut sinks = roots
        .iter()
        .map(|_| IndexRecordSink::memory(None))
        .collect::<Vec<_>>();
    let mut diagnostics = LargeFileScanDiagnostics::default();
    let mut skipped_count = 0_u64;
    log::info!(
        "large_file_index_scope_plan operation_id={} selected_roots={} query_roots={query_count}",
        operation.id(),
        roots.len()
    );
    for (query_index, query_root) in roots.iter().enumerate() {
        if sources[query_index] != query_index {
            continue;
        }
        if operation.cancelled().load(Ordering::Relaxed) {
            return Err(CoreError::operation_cancelled());
        }
        if exclusions[query_index].matches(query_root) {
            log::info!(
                "large_file_index_scope_excluded operation_id={} root={} outcome=pruned",
                operation.id(),
                diagnostic_path(query_root)
            );
            progress.complete_step(TraversalStage::Analyzing, query_root, 0);
            continue;
        }
        progress.emit(TraversalStage::Analyzing, query_root);
        let query_started = Instant::now();
        // Exclusions are applied per destination scope. The provider query may serve several
        // overlapping roots, so each owner must validate its own exclusion policy.
        let scan = current_platform().fast_large_file_candidates(
            query_root,
            LARGE_FILE_CANDIDATE_FLOOR_BYTES,
            &[],
            &|| operation.cancelled().load(Ordering::Relaxed),
            &mut |path| {
                if let Some(owner) = candidate_owner(roots, &path) {
                    // Mounted filesystems keep independent queries even if the index also returns
                    // their paths in the parent query. Never validate or retain a candidate twice.
                    if sources[owner] == query_index {
                        validations[owner].consume(path, &mut sinks[owner])?;
                    }
                }
                Ok(())
            },
        );
        let summary = match scan {
            Ok(Some(summary)) => summary,
            Err(LargeFileCandidateScanError::Cancelled) => {
                return Err(CoreError::operation_cancelled())
            }
            Err(LargeFileCandidateScanError::Consumer(error)) => {
                return Err(traversal_core_error(error))
            }
            other => {
                log::warn!(
                    "large_file_quick_scan_unavailable operation_id={} root={} detail={}",
                    operation.id(),
                    diagnostic_path(query_root),
                    strawberrydisk_platform::diagnostics::text(&format!("{other:?}"))
                );
                return Err(
                    CoreError::operation_failed("quick large-file scan unavailable")
                        .with_reason(CoreErrorReason::QuickScanUnavailable),
                );
            }
        };
        let query_ms = query_started.elapsed().as_millis() as u64;
        log::info!("large_file_index_query_finished operation_id={} root={} candidate_count={} elapsed_ms={query_ms}",
            operation.id(), diagnostic_path(query_root), summary.candidate_count);
        skipped_count = skipped_count.saturating_add(summary.skipped_count);
        diagnostics.accumulate(&LargeFileScanDiagnostics {
            native_directory_reads: summary.native_directory_reads,
            candidate_discovery_ms: query_ms,
            validation_or_traversal_ms: summary.consumer_elapsed_ms,
            candidate_count: summary.candidate_count,
            candidate_backpressure_ms: summary.producer_backpressure_ms,
            candidate_peak_in_flight: summary.peak_in_flight_candidates,
            candidate_strategy: summary.strategy,
            fast_path: "used",
            ..Default::default()
        });
        progress.complete_step(TraversalStage::Analyzing, query_root, 0);
    }
    if operation.cancelled().load(Ordering::Relaxed) {
        return Err(CoreError::operation_cancelled());
    }
    let result_started = Instant::now();
    let mut entries = Vec::new();
    for ((root, validation), sink) in roots.iter().zip(validations).zip(sinks) {
        skipped_count = skipped_count.saturating_add(validation.aggregate.skipped_count);
        entries.extend(cache::large_file_entries_from_snapshot(
            root,
            &sink.finish()?.files,
        ));
    }
    let result = LargeFilesResult::from_retained_entries(
        roots
            .iter()
            .map(|root| current_platform().display_path(root))
            .collect(),
        scanned_at_ms,
        LargeFileScanMode::Quick,
        minimum_bytes,
        skipped_count,
        entries,
    );
    diagnostics.result_build_ms = result_started.elapsed().as_millis() as u64;
    progress.finish(TraversalStage::Analyzing, &roots[0]);
    if operation.cancelled().load(Ordering::Relaxed) {
        return Err(CoreError::operation_cancelled());
    }
    log::info!("large_file_scan_finished operation_id={} root_count={} query_count={query_count} mode=quick total_count={} elapsed_ms={}",
        operation.id(), roots.len(), result.total_count, started.elapsed().as_millis());
    Ok((result, diagnostics))
}

/// Roots are canonical and sorted, so an ancestor precedes its descendants. Only advisory
/// queries share sources: physical traversal may prune descendants by platform policy.
fn query_sources(roots: &[PathBuf], metadata: &[fs::Metadata]) -> Vec<usize> {
    roots
        .iter()
        .enumerate()
        .map(|(index, root)| {
            (0..index)
                .find(|parent| {
                    current_platform().path_is_same_or_child(root, &roots[*parent])
                        && current_platform()
                            .is_same_filesystem(&metadata[index], &metadata[*parent])
                })
                .unwrap_or(index)
        })
        .collect()
}

fn candidate_owner(roots: &[PathBuf], path: &Path) -> Option<usize> {
    roots
        .iter()
        .rposition(|root| current_platform().path_is_same_or_child(path, root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_index_scopes_share_a_query_but_keep_the_most_specific_owner() {
        let parent = std::env::temp_dir().join("index-scope-fixture");
        let child = parent.join("child");
        let nested = child.join("nested");
        let sibling = std::env::temp_dir().join("index-scope-fixture-other");
        let roots = vec![
            parent.clone(),
            child.clone(),
            nested.clone(),
            sibling.clone(),
        ];
        let metadata = vec![fs::metadata(std::env::temp_dir()).unwrap(); roots.len()];
        assert_eq!(query_sources(&roots, &metadata), vec![0, 0, 0, 3]);
        assert_eq!(candidate_owner(&roots, &nested.join("file.bin")), Some(2));
        assert_eq!(candidate_owner(&roots, &child.join("file.bin")), Some(1));
        assert_eq!(candidate_owner(&roots, &sibling.join("file.bin")), Some(3));
        assert_eq!(candidate_owner(&roots, &parent.join("file.bin")), Some(0));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mounted_index_scopes_keep_independent_queries() {
        let roots = vec![PathBuf::from("/"), PathBuf::from("/dev")];
        let metadata = roots
            .iter()
            .map(|root| fs::metadata(root).unwrap())
            .collect::<Vec<_>>();
        assert!(!current_platform().is_same_filesystem(&metadata[0], &metadata[1]));
        assert_eq!(query_sources(&roots, &metadata), vec![0, 1]);
    }
}
