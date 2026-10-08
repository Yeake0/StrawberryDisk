use serde::Serialize;
use tauri::Emitter;

pub const CLEANUP_SCAN_PROGRESS: &str = "cleanup-scan-progress";
pub const CLEANUP_EXECUTION_PROGRESS: &str = "cleanup-execution-progress";
pub const ANALYSIS_PROGRESS: &str = "analysis-progress";
pub const LARGE_FILES_PROGRESS: &str = "large-files-progress";
pub const DUPLICATE_FILES_PROGRESS: &str = "duplicate-files-progress";
pub const DUPLICATE_FILE_GROUPS: &str = "duplicate-files-groups";
pub const APPLICATION_UNINSTALL_PROGRESS: &str = "application-uninstall-progress";
pub const APPLICATION_UNINSTALL_EXECUTION_PROGRESS: &str =
    "application-uninstall-execution-progress";
pub const PRIVACY_SCAN_PROGRESS: &str = "privacy-scan-progress";
pub const PRIVACY_EXECUTION_PROGRESS: &str = "privacy-execution-progress";
pub const SYSTEM_MAINTENANCE_JOB_UPDATED: &str = "system-maintenance-job-updated";
// Resident navigation can request About on either desktop platform.
pub const OPEN_ABOUT: &str = "application-menu-open-about";

/// Emits one typed desktop event and keeps delivery failures in native logs.
/// A closed window must not turn a completed domain operation into a failure.
pub fn emit<S>(app: &tauri::AppHandle, event: &'static str, payload: S)
where
    S: Serialize + Clone,
{
    if let Err(error) = app.emit(event, payload) {
        log::warn!("desktop_event_emit_failed event={event} error={error}");
    }
}
