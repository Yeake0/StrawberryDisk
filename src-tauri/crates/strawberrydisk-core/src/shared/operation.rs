use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
    Arc, Condvar, Mutex, OnceLock,
};
use std::{
    fs::{self, File, OpenOptions},
    io::ErrorKind,
    time::Instant,
};

use crate::shared::application_paths;
use fs2::FileExt;

use super::{CoreError, CoreResult};

static COORDINATOR: OnceLock<OperationCoordinator> = OnceLock::new();
#[cfg(test)]
static TEST_OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const OPERATION_RUNNING: u8 = 0;
const OPERATION_COMPLETED: u8 = 1;
const OPERATION_DEFERRED: u8 = 2;
pub(crate) const OPERATION_CANCELLED_ERROR: &str = "operation cancelled";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoordinatedOperationKind {
    CleanupScan,
    Analysis,
    LargeFiles,
    DuplicateFiles,
    ApplicationScan,
    ApplicationLeftoverScan,
    ApplicationPreparation,
    Applications,
    ApplicationLeftoverCleanup,
    ApplicationClose,
    Cleanup,
    PermanentDelete,
    StartupScan,
    StartupChange,
    SystemSettingsScan,
    SystemSettingsChange,
    SystemMaintenanceScan,
    SystemMaintenanceExecution,
    PrivacyScan,
    PrivacyExecution,
}

impl CoordinatedOperationKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CleanupScan => "cleanup_scan",
            Self::Analysis => "analysis",
            Self::LargeFiles => "large_files",
            Self::DuplicateFiles => "duplicate_files",
            Self::ApplicationScan => "application_scan",
            Self::ApplicationLeftoverScan => "application_leftover_scan",
            Self::ApplicationPreparation => "application_preparation",
            Self::Applications => "applications",
            Self::ApplicationLeftoverCleanup => "application_leftover_cleanup",
            Self::ApplicationClose => "application_close",
            Self::Cleanup => "cleanup",
            Self::PermanentDelete => "permanent_delete",
            Self::StartupScan => "startup_scan",
            Self::StartupChange => "startup_change",
            Self::SystemSettingsScan => "system_settings_scan",
            Self::SystemSettingsChange => "system_settings_change",
            Self::SystemMaintenanceScan => "system_maintenance_scan",
            Self::SystemMaintenanceExecution => "system_maintenance_execution",
            Self::PrivacyScan => "privacy_scan",
            Self::PrivacyExecution => "privacy_execution",
        }
    }

    /// Declares only resources whose concurrent use can violate a domain invariant.
    ///
    /// Heavy read-only scans deliberately do not share a correctness lock. Their CPU and I/O
    /// parallelism is bounded inside the scan engines, while immutable result sessions and live
    /// mutation preflight keep a concurrently changing filesystem safe. Configuration domains use
    /// shared scan claims and exclusive mutation claims so unrelated pages remain responsive.
    fn resource_claims(self) -> Vec<ResourceClaim> {
        match self {
            Self::CleanupScan | Self::Analysis | Self::LargeFiles | Self::DuplicateFiles => vec![],
            Self::ApplicationScan | Self::ApplicationLeftoverScan => vec![ResourceClaim::shared(
                CoordinatedResource::ApplicationInventory,
            )],
            Self::ApplicationPreparation => vec![
                ResourceClaim::shared(CoordinatedResource::ApplicationInventory),
                ResourceClaim::shared(CoordinatedResource::ApplicationLifecycle),
            ],
            Self::Applications => vec![
                ResourceClaim::exclusive(CoordinatedResource::ApplicationInventory),
                ResourceClaim::exclusive(CoordinatedResource::ApplicationLifecycle),
                ResourceClaim::exclusive(CoordinatedResource::FilesystemMutation),
            ],
            Self::ApplicationLeftoverCleanup | Self::Cleanup | Self::PermanentDelete => {
                vec![ResourceClaim::exclusive(
                    CoordinatedResource::FilesystemMutation,
                )]
            }
            Self::ApplicationClose => vec![ResourceClaim::exclusive(
                CoordinatedResource::ApplicationLifecycle,
            )],
            Self::StartupScan => vec![ResourceClaim::shared(
                CoordinatedResource::StartupConfiguration,
            )],
            Self::StartupChange => vec![ResourceClaim::exclusive(
                CoordinatedResource::StartupConfiguration,
            )],
            Self::SystemSettingsScan => vec![ResourceClaim::shared(
                CoordinatedResource::SystemSettingsConfiguration,
            )],
            Self::SystemSettingsChange => vec![ResourceClaim::exclusive(
                CoordinatedResource::SystemSettingsConfiguration,
            )],
            Self::SystemMaintenanceScan => vec![ResourceClaim::shared(
                CoordinatedResource::SystemMaintenanceSession,
            )],
            Self::SystemMaintenanceExecution => vec![
                ResourceClaim::exclusive(CoordinatedResource::FilesystemMutation),
                ResourceClaim::exclusive(CoordinatedResource::SystemMaintenanceSession),
            ],
            // Privacy scans populate one opaque scan session. Serializing them prevents a second
            // adapter request from replacing the first result before its plan is prepared.
            Self::PrivacyScan => vec![ResourceClaim::exclusive(CoordinatedResource::PrivacyData)],
            Self::PrivacyExecution => vec![
                ResourceClaim::exclusive(CoordinatedResource::PrivacyData),
                ResourceClaim::exclusive(CoordinatedResource::FilesystemMutation),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourceAccess {
    Shared,
    Exclusive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CoordinatedResource {
    ApplicationInventory,
    ApplicationLifecycle,
    FilesystemMutation,
    StartupConfiguration,
    SystemMaintenanceSession,
    SystemSettingsConfiguration,
    PrivacyData,
}

impl CoordinatedResource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ApplicationInventory => "application_inventory",
            Self::ApplicationLifecycle => "application_lifecycle",
            Self::FilesystemMutation => "filesystem_mutation",
            Self::StartupConfiguration => "startup_configuration",
            Self::SystemMaintenanceSession => "system_maintenance_session",
            Self::SystemSettingsConfiguration => "system_settings_configuration",
            Self::PrivacyData => "privacy_data",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResourceClaim {
    resource: CoordinatedResource,
    access: ResourceAccess,
}

impl ResourceClaim {
    const fn shared(resource: CoordinatedResource) -> Self {
        Self {
            resource,
            access: ResourceAccess::Shared,
        }
    }

    const fn exclusive(resource: CoordinatedResource) -> Self {
        Self {
            resource,
            access: ResourceAccess::Exclusive,
        }
    }
}

/// Adapter-owned handle for cancelling one class of active Core operation.
///
/// The token contains no UI or terminal state. Desktop and CLI adapters can
/// therefore request cancellation through the same Core contract.
#[derive(Clone, Copy, Debug)]
pub struct OperationCancellationToken {
    kind: CoordinatedOperationKind,
}

impl OperationCancellationToken {
    pub const fn cleanup_scan() -> Self {
        Self {
            kind: CoordinatedOperationKind::CleanupScan,
        }
    }

    pub const fn analysis() -> Self {
        Self {
            kind: CoordinatedOperationKind::Analysis,
        }
    }

    pub const fn large_files() -> Self {
        Self {
            kind: CoordinatedOperationKind::LargeFiles,
        }
    }

    pub const fn duplicate_files() -> Self {
        Self {
            kind: CoordinatedOperationKind::DuplicateFiles,
        }
    }

    pub const fn cleanup() -> Self {
        Self {
            kind: CoordinatedOperationKind::Cleanup,
        }
    }

    pub const fn applications() -> Self {
        Self {
            kind: CoordinatedOperationKind::Applications,
        }
    }

    pub const fn application_leftover_cleanup() -> Self {
        Self {
            kind: CoordinatedOperationKind::ApplicationLeftoverCleanup,
        }
    }

    pub const fn application_scan() -> Self {
        Self {
            kind: CoordinatedOperationKind::ApplicationScan,
        }
    }

    pub const fn startup_scan() -> Self {
        Self {
            kind: CoordinatedOperationKind::StartupScan,
        }
    }

    pub const fn startup_change() -> Self {
        Self {
            kind: CoordinatedOperationKind::StartupChange,
        }
    }

    pub const fn system_settings_scan() -> Self {
        Self {
            kind: CoordinatedOperationKind::SystemSettingsScan,
        }
    }

    pub const fn system_maintenance_scan() -> Self {
        Self {
            kind: CoordinatedOperationKind::SystemMaintenanceScan,
        }
    }

    pub const fn privacy_scan() -> Self {
        Self {
            kind: CoordinatedOperationKind::PrivacyScan,
        }
    }

    pub const fn privacy_execution() -> Self {
        Self {
            kind: CoordinatedOperationKind::PrivacyExecution,
        }
    }

    pub fn cancel(self) {
        OperationGuard::cancel(self.kind);
    }
}

struct ActiveOperation {
    id: u64,
    kind: CoordinatedOperationKind,
    claims: Vec<ResourceClaim>,
    cancelled: Arc<AtomicBool>,
}

struct WaitingOperation {
    id: u64,
    kind: CoordinatedOperationKind,
    claims: Vec<ResourceClaim>,
    cancelled: Arc<AtomicBool>,
}

struct OperationCoordinator {
    next_id: AtomicU64,
    state: Mutex<CoordinatorState>,
    changed: Condvar,
}

#[derive(Default)]
struct CoordinatorState {
    active: Vec<ActiveOperation>,
    waiting: Vec<WaitingOperation>,
}

#[derive(Clone, Copy)]
enum ProcessLockMode {
    Shared,
    Exclusive,
}

struct ProcessOperationLock {
    file: File,
}

impl ProcessOperationLock {
    fn acquire_named(
        file_name: &str,
        kind: CoordinatedOperationKind,
        mode: ProcessLockMode,
    ) -> CoreResult<Self> {
        let directory = application_paths()?.runtime_directory();
        Self::acquire_in_directory(directory, file_name, kind, mode)
    }

    fn acquire_in_directory(
        directory: &std::path::Path,
        file_name: &str,
        kind: CoordinatedOperationKind,
        mode: ProcessLockMode,
    ) -> CoreResult<Self> {
        fs::create_dir_all(directory).map_err(|error| {
            CoreError::operation_failed(format!(
                "failed to create the operation lock directory: {error}"
            ))
        })?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join(file_name))
            .map_err(|error| {
                CoreError::operation_failed(format!("failed to open the operation lock: {error}"))
            })?;
        let result = match mode {
            ProcessLockMode::Shared => FileExt::try_lock_shared(&file),
            ProcessLockMode::Exclusive => FileExt::try_lock_exclusive(&file),
        };
        match result {
            Ok(()) => Ok(Self { file }),
            Err(error) if is_lock_contention(&error) => {
                // Cross-process callers cannot participate in the in-memory FIFO queue. Fail fast
                // with stable diagnostics so GUI and CLI adapters never block indefinitely while
                // another process owns a correctness resource.
                log::info!(
                    "operation_process_lock_contended operation_kind={} lock={file_name}",
                    kind.as_str()
                );
                Err(CoreError::operation_busy(format!(
                    "another StrawberryDisk operation is already running; requested={} lock={file_name}",
                    kind.as_str(),
                )))
            }
            Err(error) => Err(CoreError::operation_failed(format!(
                "failed to acquire the operation lock: {error}"
            ))),
        }
    }
}

fn is_lock_contention(error: &std::io::Error) -> bool {
    if error.kind() == ErrorKind::WouldBlock {
        return true;
    }

    // LockFileEx reports sharing and lock violations as native Windows errors
    // instead of ErrorKind::WouldBlock on some toolchain versions. Mapping only
    // those two codes preserves stable busy semantics without hiding genuine
    // permission or filesystem failures.
    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(32 | 33))
    }

    #[cfg(not(windows))]
    {
        false
    }
}

impl Drop for ProcessOperationLock {
    fn drop(&mut self) {
        if let Err(error) = FileExt::unlock(&self.file) {
            log::warn!("operation_process_lock_release_failed error={error}");
        }
    }
}

impl OperationCoordinator {
    fn global() -> &'static Self {
        COORDINATOR.get_or_init(|| Self {
            next_id: AtomicU64::new(1),
            state: Mutex::new(CoordinatorState::default()),
            changed: Condvar::new(),
        })
    }
}

/// Coordinates declared correctness resources across threads and processes.
/// Compatible scans keep independent private snapshots. Conflicts wait in FIFO order inside one
/// process, while process locks fail fast across GUI and CLI processes so no adapter can hang on an
/// unobservable external queue.
pub(crate) struct OperationGuard {
    id: u64,
    kind: CoordinatedOperationKind,
    cancelled: Arc<AtomicBool>,
    started: Instant,
    outcome: AtomicU8,
    _process_locks: Vec<ProcessOperationLock>,
}

impl OperationGuard {
    pub(crate) fn start(kind: CoordinatedOperationKind) -> CoreResult<Self> {
        let coordinator = OperationCoordinator::global();
        let claims = kind.resource_claims();
        let id = coordinator.next_id.fetch_add(1, Ordering::Relaxed);
        let cancelled = Arc::new(AtomicBool::new(false));
        let requested_at = Instant::now();
        let mut queued = false;
        let mut state = coordinator.state.lock().map_err(|_| {
            CoreError::operation_failed("the operation coordinator is temporarily unavailable")
        })?;

        // A second operation of the same kind is almost always a duplicate click or a repeated
        // adapter request. Reject it instead of queueing surprising work that starts only after the
        // visible first request has completed.
        if let Some((operation_id, operation_kind)) = state
            .active
            .iter()
            .map(|operation| (operation.id, operation.kind))
            .chain(
                state
                    .waiting
                    .iter()
                    .map(|operation| (operation.id, operation.kind)),
            )
            .find(|(_, operation_kind)| *operation_kind == kind)
        {
            return Err(CoreError::operation_busy(format!(
                "another StrawberryDisk operation is already running: {} ({})",
                operation_kind.as_str(),
                operation_id
            )));
        }

        loop {
            if cancelled.load(Ordering::Relaxed) {
                state.waiting.retain(|operation| operation.id != id);
                coordinator.changed.notify_all();
                return Err(CoreError::operation_cancelled());
            }

            let active_conflict = state.active.iter().find_map(|operation| {
                conflicting_resource(&claims, &operation.claims)
                    .map(|resource| (operation, resource))
            });
            let earlier_waiter = state
                .waiting
                .iter()
                .take_while(|operation| operation.id != id)
                .find_map(|operation| {
                    conflicting_resource(&claims, &operation.claims)
                        .map(|resource| (operation.kind, resource))
                });
            if active_conflict.is_none() && earlier_waiter.is_none() {
                state.waiting.retain(|operation| operation.id != id);
                state.active.push(ActiveOperation {
                    id,
                    kind,
                    claims: claims.clone(),
                    cancelled: Arc::clone(&cancelled),
                });
                break;
            }

            if !queued {
                let (blocking_kind, resource) = active_conflict
                    .map(|(operation, resource)| (operation.kind, resource))
                    .or(earlier_waiter)
                    .expect("a queued operation must have a conflicting resource");
                state.waiting.push(WaitingOperation {
                    id,
                    kind,
                    claims: claims.clone(),
                    cancelled: Arc::clone(&cancelled),
                });
                queued = true;
                log::info!(
                    "operation_queued operation_id={} operation_kind={} blocking_kind={} resource={} queue_depth={}",
                    id,
                    kind.as_str(),
                    blocking_kind.as_str(),
                    resource.as_str(),
                    state.waiting.len()
                );
            }
            state = coordinator.changed.wait(state).map_err(|_| {
                CoreError::operation_failed("the operation coordinator is temporarily unavailable")
            })?;
        }
        drop(state);

        let process_locks = match acquire_process_locks(kind, &claims) {
            Ok(locks) => locks,
            Err(error) => {
                release_active_operation(coordinator, id);
                return Err(error);
            }
        };
        log::info!(
            "operation_started operation_id={} operation_kind={} queued={} queue_wait_ms={} resource_count={}",
            id,
            kind.as_str(),
            queued,
            requested_at.elapsed().as_millis(),
            claims.len()
        );
        Ok(Self {
            id,
            kind,
            cancelled,
            started: Instant::now(),
            outcome: AtomicU8::new(OPERATION_RUNNING),
            _process_locks: process_locks,
        })
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn cancelled(&self) -> &AtomicBool {
        self.cancelled.as_ref()
    }

    /// Background validation receives only the flag so a worker cannot extend
    /// guard ownership and keep the process lock alive accidentally.
    pub(crate) fn cancellation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    pub(crate) fn ensure_not_cancelled(&self) -> CoreResult<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(CoreError::operation_cancelled())
        } else {
            Ok(())
        }
    }

    pub(crate) fn complete(&self) {
        self.outcome.store(OPERATION_COMPLETED, Ordering::Relaxed);
    }

    /// Records a retryable platform condition separately from a failed operation so diagnostics
    /// do not report an expected backpressure decision as a warning.
    pub(crate) fn defer(&self) {
        self.outcome.store(OPERATION_DEFERRED, Ordering::Relaxed);
    }

    pub(crate) fn cancel(kind: CoordinatedOperationKind) {
        let coordinator = OperationCoordinator::global();
        let Ok(state) = coordinator.state.lock() else {
            log::warn!(
                "operation_cancel_failed operation_kind={} reason=coordinator_poisoned",
                kind.as_str()
            );
            return;
        };
        let operation = state
            .active
            .iter()
            .find(|operation| operation.kind == kind)
            .map(|operation| (operation.id, operation.kind, &operation.cancelled))
            .or_else(|| {
                state
                    .waiting
                    .iter()
                    .find(|operation| operation.kind == kind)
                    .map(|operation| (operation.id, operation.kind, &operation.cancelled))
            });
        let Some((operation_id, operation_kind, cancellation)) = operation else {
            return;
        };
        cancellation.store(true, Ordering::Relaxed);
        coordinator.changed.notify_all();
        log::info!(
            "operation_cancel_requested operation_id={} operation_kind={}",
            operation_id,
            operation_kind.as_str()
        );
    }
}

fn conflicting_resource(
    requested: &[ResourceClaim],
    existing: &[ResourceClaim],
) -> Option<CoordinatedResource> {
    requested.iter().find_map(|left| {
        existing
            .iter()
            .find(|right| {
                left.resource == right.resource
                    && (left.access == ResourceAccess::Exclusive
                        || right.access == ResourceAccess::Exclusive)
            })
            .map(|_| left.resource)
    })
}

fn acquire_process_locks(
    kind: CoordinatedOperationKind,
    claims: &[ResourceClaim],
) -> CoreResult<Vec<ProcessOperationLock>> {
    let mut requested = vec![(
        format!("operation-kind-{}.lock", kind.as_str()),
        ProcessLockMode::Exclusive,
    )];
    requested.extend(claims.iter().map(|claim| {
        (
            format!("operation-resource-{}.lock", claim.resource.as_str()),
            match claim.access {
                ResourceAccess::Shared => ProcessLockMode::Shared,
                ResourceAccess::Exclusive => ProcessLockMode::Exclusive,
            },
        )
    }));
    requested.sort_by(|left, right| left.0.cmp(&right.0));
    requested
        .into_iter()
        .map(|(file_name, mode)| ProcessOperationLock::acquire_named(&file_name, kind, mode))
        .collect()
}

fn release_active_operation(coordinator: &OperationCoordinator, operation_id: u64) {
    let Ok(mut state) = coordinator.state.lock() else {
        log::warn!(
            "operation_release_failed operation_id={operation_id} reason=coordinator_poisoned"
        );
        return;
    };
    state
        .active
        .retain(|operation| operation.id != operation_id);
    coordinator.changed.notify_all();
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        // File locks must be released before waking a locally queued operation. Rust drops fields
        // only after this method returns; relying on implicit field drop would create a narrow race
        // where the awakened waiter reserves the local resource but still sees the old process lock.
        self._process_locks.clear();
        release_active_operation(OperationCoordinator::global(), self.id);
        let cancelled = self.cancelled.load(Ordering::Relaxed);
        let status = if cancelled {
            "cancelled"
        } else {
            match self.outcome.load(Ordering::Relaxed) {
                OPERATION_COMPLETED => "completed",
                OPERATION_DEFERRED => "deferred",
                _ => "failed",
            }
        };
        if status == "failed" {
            log::warn!(
                "operation_finished operation_id={} operation_kind={} status={} cancelled={} elapsed_ms={}",
                self.id,
                self.kind.as_str(),
                status,
                cancelled,
                self.started.elapsed().as_millis()
            );
        } else {
            log::info!(
                "operation_finished operation_id={} operation_kind={} status={} cancelled={} elapsed_ms={}",
                self.id,
                self.kind.as_str(),
                status,
                cancelled,
                self.started.elapsed().as_millis()
            );
        }
    }
}

#[cfg(test)]
pub(crate) fn test_operation_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        // The test-only mutex carries no business state. Recovering a poisoned
        // guard prevents one assertion failure from hiding later independent
        // disk test results.
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreErrorCode;

    fn wait_until_queued(kind: CoordinatedOperationKind) {
        for _ in 0..1_000 {
            if OperationCoordinator::global()
                .state
                .lock()
                .expect("the test coordinator should remain available")
                .waiting
                .iter()
                .any(|operation| operation.kind == kind)
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("the operation did not enter the coordinator queue");
    }

    #[test]
    fn cancellation_token_stops_the_matching_operation() {
        let _test_guard = test_operation_lock();
        let operation = OperationGuard::start(CoordinatedOperationKind::CleanupScan)
            .expect("the isolated operation should start");

        OperationCancellationToken::cleanup_scan().cancel();

        assert_eq!(
            operation
                .ensure_not_cancelled()
                .expect_err("the operation should be cancelled")
                .code(),
            CoreErrorCode::OperationCancelled
        );
    }

    #[test]
    fn retryable_backpressure_records_a_deferred_outcome() {
        let _test_guard = test_operation_lock();
        let operation = OperationGuard::start(CoordinatedOperationKind::Analysis)
            .expect("the isolated operation should start");

        operation.defer();

        assert_eq!(
            operation.outcome.load(Ordering::Relaxed),
            OPERATION_DEFERRED
        );
    }

    #[test]
    fn application_scan_cancellation_does_not_cancel_application_execution() {
        let _test_guard = test_operation_lock();
        let operation = OperationGuard::start(CoordinatedOperationKind::Applications)
            .expect("the isolated application operation should start");

        OperationCancellationToken::application_scan().cancel();

        operation
            .ensure_not_cancelled()
            .expect("scan cancellation must not affect an application mutation");
    }

    #[test]
    fn process_lock_is_released_when_the_guard_drops() {
        let _test_guard = test_operation_lock();
        let first = ProcessOperationLock::acquire_named(
            "operation-coordinator-test.lock",
            CoordinatedOperationKind::Analysis,
            ProcessLockMode::Exclusive,
        )
        .expect("the first process lock should succeed");
        let error = ProcessOperationLock::acquire_named(
            "operation-coordinator-test.lock",
            CoordinatedOperationKind::Analysis,
            ProcessLockMode::Exclusive,
        )
        .err()
        .expect("a second process lock should be rejected");
        assert_eq!(error.code(), CoreErrorCode::OperationBusy);

        drop(first);
        let second = ProcessOperationLock::acquire_named(
            "operation-coordinator-test.lock",
            CoordinatedOperationKind::Analysis,
            ProcessLockMode::Exclusive,
        )
        .expect("the process lock should be reusable after release");
        drop(second);
    }

    #[test]
    fn process_lock_contention_is_reported_across_processes() {
        let _test_guard = test_operation_lock();
        let lock_name = "operation-cross-process-test.lock";
        let lock_directory = std::env::temp_dir().join(format!(
            "strawberrydisk-operation-cross-process-test-{}",
            std::process::id()
        ));
        let first = ProcessOperationLock::acquire_in_directory(
            &lock_directory,
            lock_name,
            CoordinatedOperationKind::Analysis,
            ProcessLockMode::Exclusive,
        )
        .expect("the parent process should acquire the lock");
        let output = std::process::Command::new(
            std::env::current_exe().expect("the test executable should be available"),
        )
        .args([
            "--exact",
            "shared::operation::tests::cross_process_lock_probe",
            "--nocapture",
        ])
        .env(
            "STRAWBERRYDISK_CROSS_PROCESS_LOCK_PROBE",
            lock_directory.join(lock_name),
        )
        .output()
        .expect("the lock probe process should start");

        drop(first);
        let _ = fs::remove_file(lock_directory.join(lock_name));
        let _ = fs::remove_dir(lock_directory);
        assert!(
            output.status.success(),
            "the child process should observe contention\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn cross_process_lock_probe() {
        let Ok(lock_path) = std::env::var("STRAWBERRYDISK_CROSS_PROCESS_LOCK_PROBE") else {
            return;
        };
        let lock_path = std::path::PathBuf::from(lock_path);
        let error = ProcessOperationLock::acquire_in_directory(
            lock_path
                .parent()
                .expect("the probe lock path should have a parent"),
            lock_path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .expect("the probe lock name should be valid UTF-8"),
            CoordinatedOperationKind::Analysis,
            ProcessLockMode::Exclusive,
        )
        .err()
        .expect("the parent process should still own the lock");

        assert_eq!(error.code(), CoreErrorCode::OperationBusy);
    }

    #[test]
    fn shared_process_locks_allow_readers_and_reject_a_writer() {
        let _test_guard = test_operation_lock();
        let first = ProcessOperationLock::acquire_named(
            "operation-shared-test.lock",
            CoordinatedOperationKind::SystemSettingsScan,
            ProcessLockMode::Shared,
        )
        .expect("the first reader should acquire the resource lock");
        let second = ProcessOperationLock::acquire_named(
            "operation-shared-test.lock",
            CoordinatedOperationKind::SystemSettingsScan,
            ProcessLockMode::Shared,
        )
        .expect("a second reader should share the resource lock");
        let error = ProcessOperationLock::acquire_named(
            "operation-shared-test.lock",
            CoordinatedOperationKind::SystemSettingsChange,
            ProcessLockMode::Exclusive,
        )
        .err()
        .expect("a writer must be rejected while readers are active");
        assert_eq!(error.code(), CoreErrorCode::OperationBusy);

        drop(second);
        drop(first);
        let writer = ProcessOperationLock::acquire_named(
            "operation-shared-test.lock",
            CoordinatedOperationKind::SystemSettingsChange,
            ProcessLockMode::Exclusive,
        )
        .expect("the mutation writer should start after maintenance readers finish");
        drop(writer);
    }

    #[test]
    fn maintenance_execution_allows_a_read_only_foreground_scan() {
        let _test_guard = test_operation_lock();
        let maintenance =
            OperationGuard::start(CoordinatedOperationKind::SystemMaintenanceExecution)
                .expect("maintenance should start in isolation");
        let scan = OperationGuard::start(CoordinatedOperationKind::SystemSettingsScan)
            .expect("a read-only settings scan should remain available during maintenance");

        drop(scan);
        drop(maintenance);
    }

    #[test]
    fn independent_scan_and_mutation_domains_run_together() {
        let _test_guard = test_operation_lock();
        let maintenance =
            OperationGuard::start(CoordinatedOperationKind::SystemMaintenanceExecution)
                .expect("maintenance should start in isolation");
        let settings = OperationGuard::start(CoordinatedOperationKind::SystemSettingsChange)
            .expect("an unrelated settings mutation should start");
        let analysis = OperationGuard::start(CoordinatedOperationKind::Analysis)
            .expect("an unrelated disk analysis should start");

        drop(analysis);
        drop(settings);
        drop(maintenance);
    }

    #[test]
    fn duplicate_kind_is_rejected_instead_of_queued() {
        let _test_guard = test_operation_lock();
        let first = OperationGuard::start(CoordinatedOperationKind::Analysis)
            .expect("the first analysis should start");
        let error = OperationGuard::start(CoordinatedOperationKind::Analysis)
            .err()
            .expect("a duplicate analysis must be rejected");

        assert_eq!(error.code(), CoreErrorCode::OperationBusy);
        drop(first);
    }

    #[test]
    fn shared_application_inventory_scans_run_together() {
        let _test_guard = test_operation_lock();
        let catalog = OperationGuard::start(CoordinatedOperationKind::ApplicationScan)
            .expect("the application catalog should start");
        let leftovers = OperationGuard::start(CoordinatedOperationKind::ApplicationLeftoverScan)
            .expect("the leftover scan should share inventory access");
        let preparation = OperationGuard::start(CoordinatedOperationKind::ApplicationPreparation)
            .expect("application preparation should share inventory access");

        drop(preparation);
        drop(leftovers);
        drop(catalog);
    }

    #[test]
    fn application_close_waits_for_uninstall_preparation() {
        let _test_guard = test_operation_lock();
        let preparation = OperationGuard::start(CoordinatedOperationKind::ApplicationPreparation)
            .expect("application preparation should start");
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = OperationGuard::start(CoordinatedOperationKind::ApplicationClose).map(
                |operation| {
                    operation.complete();
                },
            );
            sender.send(result.map_err(|error| error.code())).unwrap();
        });

        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err(),
            "closing an application must not invalidate a running-state preparation snapshot"
        );
        drop(preparation);
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("the close operation should start after preparation finishes"),
            Ok(())
        );
        worker.join().expect("the close worker should finish");
    }

    #[test]
    fn conflicting_configuration_change_waits_for_scan() {
        let _test_guard = test_operation_lock();
        let scan = OperationGuard::start(CoordinatedOperationKind::SystemSettingsScan)
            .expect("the settings scan should start");
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = OperationGuard::start(CoordinatedOperationKind::SystemSettingsChange).map(
                |operation| {
                    operation.complete();
                },
            );
            sender.send(result.map_err(|error| error.code())).unwrap();
        });

        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err(),
            "the conflicting mutation must remain queued while the scan owns a shared claim"
        );
        drop(scan);
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("the queued mutation should start after the scan finishes"),
            Ok(())
        );
        worker.join().expect("the queued worker should finish");
    }

    #[test]
    fn queued_operation_can_be_cancelled_before_it_starts() {
        let _test_guard = test_operation_lock();
        let cleanup = OperationGuard::start(CoordinatedOperationKind::Cleanup)
            .expect("cleanup should own the filesystem mutation resource");
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = OperationGuard::start(CoordinatedOperationKind::PermanentDelete)
                .map(|_| ())
                .map_err(|error| error.code());
            sender.send(result).unwrap();
        });

        wait_until_queued(CoordinatedOperationKind::PermanentDelete);
        OperationGuard::cancel(CoordinatedOperationKind::PermanentDelete);
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("the cancelled waiter should return"),
            Err(CoreErrorCode::OperationCancelled)
        );
        worker.join().expect("the cancelled worker should finish");
        drop(cleanup);
    }

    #[test]
    fn conflicting_waiters_start_in_queue_order() {
        let _test_guard = test_operation_lock();
        let cleanup = OperationGuard::start(CoordinatedOperationKind::Cleanup)
            .expect("cleanup should own the filesystem mutation resource");
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let first_started_sender = started_sender.clone();
        let first = std::thread::spawn(move || {
            let operation = OperationGuard::start(CoordinatedOperationKind::PermanentDelete)
                .expect("the first waiter should eventually start");
            first_started_sender.send("permanent_delete").unwrap();
            release_receiver.recv().unwrap();
            operation.complete();
        });
        wait_until_queued(CoordinatedOperationKind::PermanentDelete);

        let second = std::thread::spawn(move || {
            let operation = OperationGuard::start(CoordinatedOperationKind::Applications)
                .expect("the second waiter should eventually start");
            started_sender.send("applications").unwrap();
            operation.complete();
        });
        wait_until_queued(CoordinatedOperationKind::Applications);
        drop(cleanup);

        assert_eq!(
            started_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("the first queued operation should start"),
            "permanent_delete"
        );
        assert!(
            started_receiver
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err(),
            "the second conflicting waiter must remain queued until the first finishes"
        );
        release_sender.send(()).unwrap();
        assert_eq!(
            started_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("the second queued operation should start after the first finishes"),
            "applications"
        );
        first.join().expect("the first queued worker should finish");
        second
            .join()
            .expect("the second queued worker should finish");
    }
}
