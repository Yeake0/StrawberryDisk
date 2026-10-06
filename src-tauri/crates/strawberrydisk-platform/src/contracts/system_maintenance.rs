use serde::{Deserialize, Serialize};

use super::{PlatformCancellation, PlatformResult};

/// Stable task state reported by an operating-system adapter.
///
/// `Available` is intentionally distinct from `Recommended`: some repairs, such as refreshing a
/// DNS cache, cannot be diagnosed reliably and should run only when the user recognizes the
/// corresponding symptom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformSystemMaintenanceStatus {
    Healthy,
    Recommended,
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformSystemMaintenanceDiagnosticCode {
    AccessDenied,
    ApplicationRunning,
    CheckFailed,
    ComponentUnavailable,
    ToolUnavailable,
    UnsupportedVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformSystemMaintenanceState {
    pub task_id: String,
    pub status: PlatformSystemMaintenanceStatus,
    pub requires_elevation: bool,
    pub diagnostic: Option<PlatformSystemMaintenanceDiagnosticCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformSystemMaintenanceCompletion {
    Completed,
    Started,
}

/// A stable, user-presentable phase emitted while a native maintenance task is running.
///
/// Platform adapters report typed phases instead of localized command output so the Core and UI
/// never need to infer behavior from free-form DISM, SFC, PowerShell, or shell messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformSystemMaintenancePhase {
    Preparing,
    WaitingForAuthorization,
    RepairingComponentImage,
    CheckingSystemFiles,
    CheckingStartupDisk,
    CheckingSystemDisk,
    RebuildingSearchIndex,
    RefreshingShellCaches,
    RestartingFinder,
    RestartingAudioService,
    RestartingServices,
    RepairingPrintQueue,
    SynchronizingTime,
    RebuildingPerformanceCounters,
    ResettingStoreCache,
    RefreshingNetwork,
    RebuildingAppAssociations,
    RepairingPermissions,
    RestoringDefaults,
    Verifying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformSystemMaintenanceProgress {
    pub phase: PlatformSystemMaintenancePhase,
    pub current_step: Option<u8>,
    pub total_steps: Option<u8>,
    pub percent: Option<u8>,
}

impl PlatformSystemMaintenanceProgress {
    pub const fn phase(phase: PlatformSystemMaintenancePhase) -> Self {
        Self {
            phase,
            current_step: None,
            total_steps: None,
            percent: None,
        }
    }

    pub const fn step(
        phase: PlatformSystemMaintenancePhase,
        current_step: u8,
        total_steps: u8,
        percent: Option<u8>,
    ) -> Self {
        Self {
            phase,
            current_step: Some(current_step),
            total_steps: Some(total_steps),
            percent,
        }
    }
}

pub type PlatformSystemMaintenanceProgressSink =
    dyn Fn(PlatformSystemMaintenanceProgress) + Send + Sync;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformSystemMaintenanceExecution {
    pub task_id: String,
    pub changed: bool,
    pub verified: bool,
    pub requires_restart: bool,
    pub completion: PlatformSystemMaintenanceCompletion,
}

/// Platform boundary for finite, one-shot maintenance tasks.
///
/// Implementations must reject identifiers outside their compiled catalog and must never accept
/// executable paths, command arguments, registry paths, or preference domains from an adapter.
pub trait SystemMaintenancePlatform: Send + Sync {
    fn scan_system_maintenance(
        &self,
        task_ids: &[&str],
        cancellation: &PlatformCancellation,
    ) -> PlatformResult<Vec<PlatformSystemMaintenanceState>>;

    fn execute_system_maintenance(
        &self,
        task_id: &str,
        cancellation: &PlatformCancellation,
        authorization_prompt: Option<&str>,
        progress: &PlatformSystemMaintenanceProgressSink,
    ) -> PlatformResult<PlatformSystemMaintenanceExecution>;
}
