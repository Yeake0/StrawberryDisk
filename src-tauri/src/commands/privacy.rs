use strawberrydisk_core::{
    ApplicationCloseBatchResult, PrivacyBrowserCloseRequest, PrivacyBrowserStatusRequest,
    PrivacyBrowserStatusResult, PrivacyDetailsPage, PrivacyDetailsRequest, PrivacyExecutionPlan,
    PrivacyExecutionRequest, PrivacyExecutionResult, PrivacyExecutionRunRequest,
    PrivacyScanRequest, PrivacyScanResult, PrivacyService,
};

use super::error::{run_blocking, CommandResult};

#[tauri::command]
pub async fn scan_privacy(
    app: tauri::AppHandle,
    request: PrivacyScanRequest,
) -> CommandResult<PrivacyScanResult> {
    run_blocking("scan_privacy", move || {
        PrivacyService::scan_with_progress(request, move |progress| {
            crate::events::emit(&app, crate::events::PRIVACY_SCAN_PROGRESS, progress);
        })
    })
    .await
}

#[tauri::command]
pub fn cancel_privacy_scan() {
    PrivacyService::cancel_scan();
}

#[tauri::command]
pub async fn get_privacy_details(
    request: PrivacyDetailsRequest,
) -> CommandResult<PrivacyDetailsPage> {
    run_blocking("get_privacy_details", move || {
        PrivacyService::details(request)
    })
    .await
}

#[tauri::command]
pub async fn prepare_privacy_execution(
    request: PrivacyExecutionRequest,
) -> CommandResult<PrivacyExecutionPlan> {
    run_blocking("prepare_privacy_execution", move || {
        PrivacyService::prepare(request)
    })
    .await
}

#[tauri::command]
pub async fn close_privacy_browsers(
    request: PrivacyBrowserCloseRequest,
) -> CommandResult<ApplicationCloseBatchResult> {
    run_blocking("close_privacy_browsers", move || {
        PrivacyService::close_browsers(request)
    })
    .await
}

#[tauri::command]
pub async fn refresh_privacy_browser_status(
    request: PrivacyBrowserStatusRequest,
) -> CommandResult<PrivacyBrowserStatusResult> {
    run_blocking("refresh_privacy_browser_status", move || {
        PrivacyService::refresh_browser_status(request)
    })
    .await
}

#[tauri::command]
pub async fn execute_privacy(
    app: tauri::AppHandle,
    request: PrivacyExecutionRunRequest,
) -> CommandResult<PrivacyExecutionResult> {
    run_blocking("execute_privacy", move || {
        PrivacyService::execute_with_progress(request, move |progress| {
            crate::events::emit(&app, crate::events::PRIVACY_EXECUTION_PROGRESS, progress);
        })
    })
    .await
}

#[tauri::command]
pub fn cancel_privacy_execution() {
    PrivacyService::cancel_execution();
}
