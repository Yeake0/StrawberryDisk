use std::{
    ffi::OsStr,
    fs, io,
    os::macos::fs::MetadataExt as MacOsMetadataExt,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc, Condvar, Mutex,
    },
    thread,
    time::Duration,
};

use crate::{
    ApplicationComponentAggregate, ApplicationComponentAggregateError, DirectoryAggregateProgress,
    DirectoryTreeAggregate, DirectoryTreeAggregateError, DirectoryTreeSourceAggregate,
};

use super::bulk_directory::{
    worker_count, AlignedBuffer, BulkDirectory, BulkDirectoryEntry, VNODE_TYPE_DIRECTORY,
    VNODE_TYPE_REGULAR_FILE, VNODE_TYPE_SYMBOLIC_LINK,
};
use super::is_dataless_flags;

const RESULT_POLL_INTERVAL: Duration = Duration::from_millis(40);
// Core already measures independent cleanup roots in parallel. Two inner readers overlap metadata
// I/O for one large root without turning four concurrent root tasks into an unbounded I/O storm.
const MAX_DIRECTORY_WORKERS: usize = 2;

/// Link and mounted-directory behavior differs between existing product domains. Encoding the
/// three established semantics as a private enum keeps callers explicit without exposing a broad,
/// configurable traversal API that would be easy to misuse at a safety boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AggregatePolicy {
    Cleanup,
    ProjectArtifact,
    ApplicationComponent,
}

#[derive(Debug)]
struct DirectoryTask {
    path: PathBuf,
    source_index: usize,
    is_root: bool,
}

#[derive(Debug)]
struct DirectoryReadResult {
    task: DirectoryTask,
    bytes: u64,
    file_count: u64,
    skipped_count: u64,
    read_failures: crate::FileReadFailures,
    unsupported_entry_count: u64,
    first_unsupported_entry: Option<(PathBuf, u32)>,
    modified_at_ms: Option<u64>,
    child_directories: Vec<PathBuf>,
    remote_file_count: u64,
    remote_directory_count: u64,
    first_flagged_entry: Option<String>,
}

#[derive(Default)]
struct DirectoryTaskQueue {
    state: Mutex<DirectoryTaskQueueState>,
    ready: Condvar,
}

#[derive(Default)]
struct DirectoryTaskQueueState {
    tasks: Vec<DirectoryTask>,
    stopped: bool,
}

impl DirectoryTaskQueue {
    fn push_many(&self, tasks: impl IntoIterator<Item = DirectoryTask>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.tasks.extend(tasks);
        self.ready.notify_all();
    }

    fn pop(&self) -> Option<DirectoryTask> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(task) = state.tasks.pop() {
                return Some(task);
            }
            if state.stopped {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    fn stop(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.stopped = true;
        state.tasks.clear();
        self.ready.notify_all();
    }
}

/// Measures a complete directory tree through a bounded parallel Darwin bulk traversal.
///
/// Deep-clean rules need one aggregate per direct child rather than one retained record per file.
/// `getattrlistbulk` removes per-entry `stat` calls, while the shared directory queue overlaps
/// independent subdirectory reads. Results are merged only on this coordinator thread so source
/// grouping, logical sizes, timestamps, and skipped-entry accounting remain deterministic.
fn measure(
    root: &Path,
    policy: AggregatePolicy,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    report_progress: &(dyn Fn(&Path, u64, u64) + Sync),
    flag_entry_name: fn(&OsStr) -> bool,
) -> Result<DirectoryTreeAggregate, DirectoryTreeAggregateError> {
    check_cancelled(is_cancelled)?;
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| platform_error("read directory aggregate root", root, &error))?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(DirectoryTreeAggregateError::Platform(
            "directory aggregate root is not a physical directory".to_string(),
        ));
    }
    if is_dataless_flags(MacOsMetadataExt::st_flags(&root_metadata)) {
        // A native aggregate opens its root immediately. Rejecting a dataless File Provider
        // directory here prevents that open from fetching remote directory contents.
        log::info!(
            "macos_directory_aggregate_blocked reason=remote_placeholder entry_kind=root_directory policy={}",
            aggregate_policy_code(policy)
        );
        return Err(DirectoryTreeAggregateError::Platform(
            "directory aggregate root is a remote placeholder".to_string(),
        ));
    }

    let task_queue = Arc::new(DirectoryTaskQueue::default());
    let abort = Arc::new(AtomicBool::new(false));
    let failure_sequence = Arc::new(AtomicU64::new(0));
    let (result_sender, result_receiver) = mpsc::channel();
    let worker_count = worker_count(MAX_DIRECTORY_WORKERS);
    let mut workers = Vec::with_capacity(worker_count);
    for worker_index in 0..worker_count {
        let worker_queue = Arc::clone(&task_queue);
        let worker_abort = Arc::clone(&abort);
        let failure_sequence = Arc::clone(&failure_sequence);
        let result_sender = result_sender.clone();
        let root_device = root_metadata.dev();
        let worker = thread::Builder::new()
            .name(format!("mangodisk-cleanup-aggregate-{worker_index}"))
            .spawn(move || {
                let mut buffer = AlignedBuffer::new();
                while let Some(task) = worker_queue.pop() {
                    if worker_abort.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = read_directory(
                        root_device,
                        task,
                        policy,
                        flag_entry_name,
                        &worker_abort,
                        &failure_sequence,
                        &mut buffer,
                    );
                    if result_sender.send(result).is_err() {
                        break;
                    }
                }
            });
        match worker {
            Ok(worker) => workers.push(worker),
            Err(error) => {
                abort.store(true, Ordering::Relaxed);
                task_queue.stop();
                for worker in workers {
                    let _ = worker.join();
                }
                return Err(platform_error(
                    "spawn directory aggregate worker",
                    root,
                    &error,
                ));
            }
        }
    }
    drop(result_sender);

    // Index zero represents files directly inside the rule root. Each direct child directory gets
    // one stable source index; every descendant task inherits it.
    let mut sources = vec![empty_source(root)];
    let mut skipped_count = 0_u64;
    let mut read_failures = crate::FileReadFailures::default();
    let mut unsupported_entry_count = 0_u64;
    let mut first_unsupported_entry = None;
    let mut remote_file_count = 0_u64;
    let mut remote_directory_count = 0_u64;
    let mut flagged_entry = None;
    let mut outstanding_tasks = 1_usize;
    let mut progress = DirectoryAggregateProgress::new(report_progress);
    task_queue.push_many([DirectoryTask {
        path: root.to_path_buf(),
        source_index: 0,
        is_root: true,
    }]);
    let mut scan_result = Ok(());

    while outstanding_tasks > 0 {
        if is_cancelled() {
            scan_result = Err(DirectoryTreeAggregateError::Cancelled);
            break;
        }
        let result = match result_receiver.recv_timeout(RESULT_POLL_INTERVAL) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                scan_result = Err(DirectoryTreeAggregateError::Platform(
                    "directory aggregate workers disconnected".to_string(),
                ));
                break;
            }
        };
        outstanding_tasks -= 1;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                scan_result = Err(error);
                break;
            }
        };
        let observed_entries = result
            .file_count
            .saturating_add(result.skipped_count)
            .saturating_add(result.child_directories.len() as u64);
        progress.observe(
            &result.task.path,
            observed_entries,
            result.file_count,
            result.bytes,
        );
        let source = &mut sources[result.task.source_index];
        source.bytes = source.bytes.saturating_add(result.bytes);
        source.file_count = source.file_count.saturating_add(result.file_count);
        source.modified_at_ms = latest_timestamp(source.modified_at_ms, result.modified_at_ms);
        skipped_count = skipped_count.saturating_add(result.skipped_count);
        read_failures.merge(&result.read_failures);
        unsupported_entry_count =
            unsupported_entry_count.saturating_add(result.unsupported_entry_count);
        if first_unsupported_entry.is_none() {
            first_unsupported_entry = result.first_unsupported_entry;
        }
        remote_file_count = remote_file_count.saturating_add(result.remote_file_count);
        remote_directory_count =
            remote_directory_count.saturating_add(result.remote_directory_count);
        // Worker results arrive in completion order, so a concurrent match may
        // win over an earlier directory's match. Any single flagged entry is
        // sufficient for the caller's fail-closed decision.
        if flagged_entry.is_none() {
            flagged_entry = result.first_flagged_entry;
        }

        let mut child_tasks = Vec::with_capacity(result.child_directories.len());
        for path in result.child_directories {
            let source_index = if result.task.is_root {
                sources.push(empty_source(&path));
                sources.len() - 1
            } else {
                result.task.source_index
            };
            child_tasks.push(DirectoryTask {
                path,
                source_index,
                is_root: false,
            });
        }
        outstanding_tasks = outstanding_tasks.saturating_add(child_tasks.len());
        task_queue.push_many(child_tasks);
    }

    abort.store(true, Ordering::Relaxed);
    task_queue.stop();
    for worker in workers {
        if worker.join().is_err() && scan_result.is_ok() {
            scan_result = Err(DirectoryTreeAggregateError::Platform(
                "directory aggregate worker panicked".to_string(),
            ));
        }
    }
    scan_result?;
    progress.finish(root);

    sources.retain(|source| source.bytes > 0 || source.file_count > 0);
    let bytes = sources
        .iter()
        .fold(0_u64, |total, source| total.saturating_add(source.bytes));
    let file_count = sources.iter().fold(0_u64, |total, source| {
        total.saturating_add(source.file_count)
    });
    if remote_file_count > 0 || remote_directory_count > 0 {
        log::info!(
            "macos_directory_aggregate_remote_placeholders_skipped policy={} file_count={} directory_count={} total_count={}",
            aggregate_policy_code(policy),
            remote_file_count,
            remote_directory_count,
            remote_file_count.saturating_add(remote_directory_count)
        );
    }
    // Sockets and FIFOs are expected in some application containers. Report one sample per root
    // so incomplete statistics can be distinguished from an I/O failure without per-entry noise.
    if let Some((path, object_type)) = first_unsupported_entry {
        log::info!(
            "macos_directory_aggregate_entries_skipped root={} policy={} reason=unsupported_file_type count={} sample_path={} vnode_type={}",
            crate::diagnostics::text(&root.display()),
            aggregate_policy_code(policy),
            unsupported_entry_count,
            crate::diagnostics::text(&path.display()),
            object_type,
        );
    }
    Ok(DirectoryTreeAggregate {
        bytes,
        file_count,
        skipped_count,
        read_failures,
        sources,
        strategy: "darwin-parallel-getattrlistbulk-resident-files-v4",
        flagged_entry,
    })
}

pub(super) fn measure_cleanup(
    root: &Path,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    report_progress: &(dyn Fn(&Path, u64, u64) + Sync),
) -> Result<DirectoryTreeAggregate, DirectoryTreeAggregateError> {
    measure(
        root,
        AggregatePolicy::Cleanup,
        is_cancelled,
        report_progress,
        |_| false,
    )
}

pub(super) fn measure_project_artifact(
    root: &Path,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    report_progress: &(dyn Fn(&Path, u64, u64) + Sync),
    flag_entry_name: fn(&OsStr) -> bool,
) -> Result<DirectoryTreeAggregate, DirectoryTreeAggregateError> {
    measure(
        root,
        AggregatePolicy::ProjectArtifact,
        is_cancelled,
        report_progress,
        flag_entry_name,
    )
}

/// Reuses the Darwin bulk reader while returning the narrower application catalog product.
/// Direct-child source groups are intentionally discarded: the uninstall domain associates the
/// whole verified component path and needs no cleanup-rule drill-down records.
pub(super) fn measure_application_component(
    root: &Path,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    report_progress: &(dyn Fn(&Path, u64, u64) + Sync),
) -> Result<ApplicationComponentAggregate, ApplicationComponentAggregateError> {
    measure(
        root,
        AggregatePolicy::ApplicationComponent,
        is_cancelled,
        report_progress,
        |_| false,
    )
    .map(|aggregate| ApplicationComponentAggregate {
        bytes: aggregate.bytes,
        file_count: aggregate.file_count,
        skipped_count: aggregate.skipped_count,
        strategy: "darwin-application-getattrlistbulk-resident-files-v2",
    })
    .map_err(|error| match error {
        DirectoryTreeAggregateError::Cancelled => ApplicationComponentAggregateError::Cancelled,
        DirectoryTreeAggregateError::Platform(error) => {
            ApplicationComponentAggregateError::Platform(error)
        }
    })
}

fn read_directory(
    root_device: u64,
    task: DirectoryTask,
    policy: AggregatePolicy,
    flag_entry_name: fn(&OsStr) -> bool,
    abort: &AtomicBool,
    failure_sequence: &AtomicU64,
    buffer: &mut AlignedBuffer,
) -> Result<DirectoryReadResult, DirectoryTreeAggregateError> {
    check_aborted(abort)?;
    let directory = match BulkDirectory::open(&task.path) {
        Ok(directory) => directory,
        Err(error) if !task.is_root => {
            log::debug!(
                "directory_aggregate_directory_skipped platform=macos error_kind={:?}",
                error.kind()
            );
            let mut read_failures = crate::FileReadFailures::default();
            read_failures.record_in_scope(
                &task.path,
                &error,
                crate::FileReadStage::OpenDirectory,
                failure_sequence,
            );
            return Ok(DirectoryReadResult {
                task,
                bytes: 0,
                file_count: 0,
                skipped_count: 1,
                read_failures,
                unsupported_entry_count: 0,
                first_unsupported_entry: None,
                modified_at_ms: None,
                child_directories: Vec::new(),
                remote_file_count: 0,
                remote_directory_count: 0,
                first_flagged_entry: None,
            });
        }
        Err(error) => {
            return Err(platform_error(
                "open directory aggregate root",
                &task.path,
                &error,
            ))
        }
    };
    let mut result = DirectoryReadResult {
        task,
        bytes: 0,
        file_count: 0,
        skipped_count: 0,
        read_failures: Default::default(),
        unsupported_entry_count: 0,
        first_unsupported_entry: None,
        modified_at_ms: None,
        child_directories: Vec::new(),
        remote_file_count: 0,
        remote_directory_count: 0,
        first_flagged_entry: None,
    };
    loop {
        check_aborted(abort)?;
        let entries = directory.read_page(buffer).map_err(|error| {
            platform_error("read directory aggregate", &result.task.path, &error)
        })?;
        if entries.is_empty() {
            break;
        }
        for entry in entries {
            check_aborted(abort)?;
            collect_entry(
                root_device,
                policy,
                flag_entry_name,
                entry,
                &mut result,
                failure_sequence,
            );
        }
    }
    Ok(result)
}

fn collect_entry(
    root_device: u64,
    policy: AggregatePolicy,
    flag_entry_name: fn(&OsStr) -> bool,
    entry: BulkDirectoryEntry,
    result: &mut DirectoryReadResult,
    failure_sequence: &AtomicU64,
) {
    if entry.name.as_encoded_bytes().is_empty() {
        result.skipped_count = result.skipped_count.saturating_add(1);
        return;
    }
    // The name check runs before the attribute-error return so an authored
    // entry with unreadable metadata is still flagged; the caller's fail-closed
    // decision only needs the name.
    if flag_entry_name(&entry.name) && result.first_flagged_entry.is_none() {
        result.first_flagged_entry = Some(entry.name.to_string_lossy().into_owned());
    }
    if entry.attribute_error != 0 {
        result.read_failures.record_in_scope(
            &result.task.path.join(&entry.name),
            &io::Error::from_raw_os_error(entry.attribute_error as i32),
            crate::FileReadStage::ReadMetadata,
            failure_sequence,
        );
        result.skipped_count = result.skipped_count.saturating_add(1);
        return;
    }
    // Ordinary complete-root cleanup rules stay on the validated source volume. Project artifact
    // measurement intentionally preserves the portable walker's semantics: a real mounted
    // directory inside the validated artifact root is traversed, while links are counted as
    // metadata only and never followed.
    if policy == AggregatePolicy::Cleanup
        && (entry.device != root_device || entry.mount_status & libc::DIR_MNTSTATUS_MNTPOINT != 0)
    {
        result.skipped_count = result.skipped_count.saturating_add(1);
        return;
    }
    let path = result.task.path.join(entry.name);
    match entry.object_type {
        VNODE_TYPE_DIRECTORY => {
            if is_dataless_flags(entry.flags) {
                // Enumerating a dataless directory can materialize its remote listing. Keep it out
                // of the worker queue and mark the aggregate incomplete.
                result.skipped_count = result.skipped_count.saturating_add(1);
                result.remote_directory_count = result.remote_directory_count.saturating_add(1);
                return;
            }
            result.child_directories.push(path);
        }
        VNODE_TYPE_REGULAR_FILE => {
            if is_dataless_flags(entry.flags) {
                // A cleanup preview must never claim remote-only bytes or make a cloud placeholder
                // eligible for deletion. Marking the aggregate incomplete preserves fail-closed
                // behavior for skipped entries.
                result.skipped_count = result.skipped_count.saturating_add(1);
                result.remote_file_count = result.remote_file_count.saturating_add(1);
                return;
            }
            result.bytes = result.bytes.saturating_add(entry.logical_bytes);
            result.file_count = result.file_count.saturating_add(1);
            result.modified_at_ms = latest_timestamp(result.modified_at_ms, entry.modified_at_ms);
        }
        VNODE_TYPE_SYMBOLIC_LINK if policy == AggregatePolicy::ProjectArtifact => {
            // Generated project trees intentionally count link metadata while never enqueueing or
            // following the target. This matches Core's portable artifact measurement without
            // weakening ordinary cleanup-root link handling.
            result.bytes = result.bytes.saturating_add(entry.logical_bytes);
            result.file_count = result.file_count.saturating_add(1);
            result.modified_at_ms = latest_timestamp(result.modified_at_ms, entry.modified_at_ms);
        }
        VNODE_TYPE_SYMBOLIC_LINK if policy == AggregatePolicy::ApplicationComponent => {
            // Application summaries ignore link metadata. Uninstall planning later hashes the link
            // and target text itself, so this cannot weaken preflight change detection.
        }
        _ => {
            result.skipped_count = result.skipped_count.saturating_add(1);
            result.unsupported_entry_count = result.unsupported_entry_count.saturating_add(1);
            if policy == AggregatePolicy::ApplicationComponent
                && result.first_unsupported_entry.is_none()
            {
                result.first_unsupported_entry = Some((path, entry.object_type));
            }
        }
    }
}

fn empty_source(path: &Path) -> DirectoryTreeSourceAggregate {
    DirectoryTreeSourceAggregate {
        path: path.to_path_buf(),
        bytes: 0,
        file_count: 0,
        modified_at_ms: None,
    }
}

fn aggregate_policy_code(policy: AggregatePolicy) -> &'static str {
    match policy {
        AggregatePolicy::Cleanup => "cleanup",
        AggregatePolicy::ProjectArtifact => "project_artifact",
        AggregatePolicy::ApplicationComponent => "application_component",
    }
}

fn check_cancelled(
    is_cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<(), DirectoryTreeAggregateError> {
    if is_cancelled() {
        Err(DirectoryTreeAggregateError::Cancelled)
    } else {
        Ok(())
    }
}

fn check_aborted(abort: &AtomicBool) -> Result<(), DirectoryTreeAggregateError> {
    if abort.load(Ordering::Relaxed) {
        Err(DirectoryTreeAggregateError::Cancelled)
    } else {
        Ok(())
    }
}

fn latest_timestamp(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn platform_error(
    operation: &'static str,
    path: &Path,
    error: &io::Error,
) -> DirectoryTreeAggregateError {
    DirectoryTreeAggregateError::Platform(format!(
        "{operation}: path={} error_kind={:?} os_error={:?} error={}",
        crate::diagnostics::text(&path.display()),
        error.kind(),
        error.raw_os_error(),
        crate::diagnostics::text(&error)
    ))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        os::unix::fs::symlink,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::reference_directory_tree_aggregate;

    #[test]
    fn aggregate_fallback_preserves_failed_path_and_native_error() {
        let path = Path::new("fixture/cache\nline");
        let native = io::Error::from_raw_os_error(libc::EIO);
        let error = platform_error("read directory aggregate", path, &native);
        let DirectoryTreeAggregateError::Platform(detail) = error else {
            panic!("an I/O failure must retain platform diagnostics");
        };
        assert!(detail.contains("read directory aggregate"));
        assert!(detail.contains("fixture/cache\\nline"));
        assert!(!detail.contains('\n'));
        assert!(detail.contains(&format!("os_error=Some({})", libc::EIO)));
        assert!(detail.contains(&native.to_string()));
    }

    struct DirectoryCleanup(PathBuf);

    impl Drop for DirectoryCleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture_root(name: &str) -> (PathBuf, DirectoryCleanup) {
        let root = std::env::temp_dir().join(format!(
            "mangodisk-directory-aggregate-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock must be after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("fixture root must be created");
        (root.clone(), DirectoryCleanup(root))
    }

    fn source_summary(aggregate: &DirectoryTreeAggregate) -> BTreeMap<PathBuf, (u64, u64)> {
        aggregate
            .sources
            .iter()
            .map(|source| (source.path.clone(), (source.bytes, source.file_count)))
            .collect()
    }

    #[test]
    fn dataless_directory_is_not_scheduled_for_aggregate_traversal() {
        let root = PathBuf::from("/fixture");
        let mut result = DirectoryReadResult {
            task: DirectoryTask {
                path: root,
                source_index: 0,
                is_root: true,
            },
            bytes: 0,
            file_count: 0,
            skipped_count: 0,
            read_failures: Default::default(),
            unsupported_entry_count: 0,
            first_unsupported_entry: None,
            modified_at_ms: None,
            child_directories: Vec::new(),
            remote_file_count: 0,
            remote_directory_count: 0,
            first_flagged_entry: None,
        };
        let entry = BulkDirectoryEntry {
            name: "remote-directory".into(),
            device: 7,
            file_id: 1,
            link_count: 1,
            object_type: VNODE_TYPE_DIRECTORY,
            mount_status: 0,
            flags: super::super::SF_DATALESS,
            logical_bytes: 0,
            allocated_bytes: 0,
            modified_at_ms: None,
            attribute_error: 0,
            record_length: 64,
        };

        collect_entry(
            7,
            AggregatePolicy::Cleanup,
            |_| false,
            entry,
            &mut result,
            &AtomicU64::new(0),
        );

        assert!(result.child_directories.is_empty());
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.remote_directory_count, 1);
        assert_eq!(result.read_failures.count, 0);
        assert_eq!(result.remote_file_count, 0);
    }

    #[test]
    fn native_aggregate_preserves_direct_child_sources_and_skips_links() {
        let (root, _cleanup) = fixture_root("sources");
        let nested = root.join("nested");
        fs::create_dir_all(nested.join("deep")).expect("nested fixture must be created");
        fs::create_dir(root.join("empty")).expect("empty fixture directory must be created");
        fs::write(root.join("direct.bin"), [0_u8; 4]).expect("direct fixture must be written");
        fs::write(nested.join("child.bin"), [0_u8; 5]).expect("child fixture must be written");
        fs::write(nested.join("deep/grandchild.bin"), [0_u8; 6])
            .expect("grandchild fixture must be written");
        symlink(root.join("direct.bin"), root.join("linked.bin"))
            .expect("link fixture must be created");

        let aggregate = measure_cleanup(&root, &|| false, &|_, _, _| {})
            .expect("native aggregate must succeed");

        assert_eq!(aggregate.bytes, 15);
        assert_eq!(aggregate.file_count, 3);
        assert_eq!(aggregate.skipped_count, 1);
        assert_eq!(aggregate.read_failures.count, 0);
        assert_eq!(
            aggregate.strategy,
            "darwin-parallel-getattrlistbulk-resident-files-v4"
        );
        assert_eq!(aggregate.sources.len(), 2);
        let direct = aggregate
            .sources
            .iter()
            .find(|source| source.path == root)
            .expect("direct files must use the root source");
        assert_eq!((direct.bytes, direct.file_count), (4, 1));
        let nested_source = aggregate
            .sources
            .iter()
            .find(|source| source.path == nested)
            .expect("descendants must use their first direct child");
        assert_eq!((nested_source.bytes, nested_source.file_count), (11, 2));
        assert!(nested_source.modified_at_ms.is_some());
    }

    #[test]
    fn project_artifact_aggregate_counts_link_metadata_without_following_targets() {
        let (root, _cleanup) = fixture_root("project-links");
        let target = root.join("target.bin");
        let linked = root.join("linked.bin");
        fs::write(&target, [0_u8; 7]).expect("target fixture must be written");
        symlink(&target, &linked).expect("link fixture must be created");
        let link_bytes = fs::symlink_metadata(&linked)
            .expect("link metadata must be readable")
            .len();

        let aggregate = measure_project_artifact(&root, &|| false, &|_, _, _| {}, |_| false)
            .expect("native aggregate must succeed");

        assert_eq!(aggregate.bytes, 7 + link_bytes);
        assert_eq!(aggregate.file_count, 2);
        assert_eq!(aggregate.skipped_count, 0);
    }

    #[test]
    fn project_artifact_aggregate_flags_authored_entries_during_traversal() {
        let (root, _cleanup) = fixture_root("project-flagged");
        fs::create_dir_all(root.join("debug/.git")).expect("nested git fixture must be created");
        fs::write(root.join("debug/.git/index"), [0_u8; 3])
            .expect("nested git content must be created");
        fs::write(root.join("debug.bin"), [0_u8; 4]).expect("fixture file must be written");

        let flag = |name: &std::ffi::OsStr| name.as_encoded_bytes() == b".git";
        let aggregate = measure_project_artifact(&root, &|| false, &|_, _, _| {}, flag)
            .expect("native aggregate must succeed");

        assert_eq!(aggregate.flagged_entry.as_deref(), Some(".git"));
        // Flagging must not prune the entry: limited preview still reports complete totals.
        assert_eq!(aggregate.file_count, 2);
        assert_eq!(aggregate.bytes, 7);
    }

    #[test]
    fn native_aggregate_matches_the_reference_walker() {
        let (root, _cleanup) = fixture_root("equivalence");
        fs::create_dir_all(root.join("alpha/deep")).expect("nested fixture must be created");
        fs::create_dir(root.join("empty")).expect("empty fixture directory must be created");
        fs::write(root.join("direct.bin"), [0_u8; 3]).expect("direct fixture must be written");
        fs::write(root.join("alpha/child.bin"), [0_u8; 7]).expect("child fixture must be written");
        fs::write(root.join("alpha/deep/grandchild.bin"), [0_u8; 11])
            .expect("grandchild fixture must be written");

        let native = measure_cleanup(&root, &|| false, &|_, _, _| {})
            .expect("native aggregate must succeed");
        let reference = reference_directory_tree_aggregate(&root);

        assert_eq!(
            (native.bytes, native.file_count, native.skipped_count),
            (
                reference.bytes,
                reference.file_count,
                reference.skipped_count
            )
        );
        assert_eq!(source_summary(&native), source_summary(&reference));
    }

    #[test]
    fn native_aggregate_honors_cancellation_before_opening_entries() {
        let (root, _cleanup) = fixture_root("cancel");

        assert!(matches!(
            measure_cleanup(&root, &|| true, &|_, _, _| {}),
            Err(DirectoryTreeAggregateError::Cancelled)
        ));
    }

    #[test]
    fn application_aggregate_reports_fifo_without_opening_its_contents() {
        let (root, _cleanup) = fixture_root("application-fifo");
        fs::write(root.join("ordinary.bin"), [0_u8; 7]).expect("fixture file must be written");
        let fifo = root.join("communication.pipe");
        let native_path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes())
            .expect("fixture path must not contain a nul byte");
        // Only create a disposable filesystem entry; opening a FIFO would wait for a peer and
        // must never be part of size measurement or diagnostic collection.
        assert_eq!(unsafe { libc::mkfifo(native_path.as_ptr(), 0o600) }, 0);
        let result = read_directory(
            fs::metadata(&root)
                .expect("fixture metadata must be readable")
                .dev(),
            DirectoryTask {
                path: root.clone(),
                source_index: 0,
                is_root: true,
            },
            AggregatePolicy::ApplicationComponent,
            |_| false,
            &AtomicBool::new(false),
            &AtomicU64::new(0),
            &mut AlignedBuffer::new(),
        )
        .expect("directory metadata must be readable");
        assert_eq!(result.unsupported_entry_count, 1);
        assert_eq!(result.first_unsupported_entry, Some((fifo, 7)));
        let aggregate = measure_application_component(&root, &|| false, &|_, _, _| {})
            .expect("application aggregate must succeed");
        assert_eq!(
            (
                aggregate.bytes,
                aggregate.file_count,
                aggregate.skipped_count
            ),
            (7, 1, 1)
        );
    }

    #[test]
    fn application_aggregate_ignores_links_without_marking_the_tree_incomplete() {
        let (root, _cleanup) = fixture_root("application-links");
        fs::write(root.join("inside.bin"), [0_u8; 7]).expect("fixture file must be written");
        symlink("inside.bin", root.join("linked.bin")).expect("fixture link must be created");

        let aggregate = measure_application_component(&root, &|| false, &|_, _, _| {})
            .expect("application aggregate must succeed");

        assert_eq!(aggregate.bytes, 7);
        assert_eq!(aggregate.file_count, 1);
        assert_eq!(aggregate.skipped_count, 0);
    }
}
