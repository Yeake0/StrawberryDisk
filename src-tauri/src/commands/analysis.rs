use mangodisk_core::{
    AnalysisRemainderPage, AnalysisRemainderRequest, AnalysisResult, AnalysisService,
};
use mangodisk_core::{ScanExclusionOptions, ScanNameExclusion};

use crate::events;

use super::error::{run_blocking, CommandResult};

#[tauri::command]
pub async fn analyze_path(
    app: tauri::AppHandle,
    path: Option<String>,
    refresh: bool,
    excluded_paths: Vec<String>,
    excluded_names: Option<Vec<ScanNameExclusion>>,
) -> CommandResult<AnalysisResult> {
    run_blocking("analyze_path", move || {
        AnalysisService::analyze_with_exclusions_progress(
            path,
            refresh,
            ScanExclusionOptions {
                paths: excluded_paths,
                names: excluded_names.unwrap_or_default(),
            },
            move |progress| {
                events::emit(&app, events::ANALYSIS_PROGRESS, progress);
            },
        )
    })
    .await
}

#[tauri::command]
pub fn cancel_analysis() {
    AnalysisService::cancel();
}

#[tauri::command]
pub async fn list_analysis_remainder(
    request: AnalysisRemainderRequest,
) -> CommandResult<AnalysisRemainderPage> {
    run_blocking("list_analysis_remainder", move || {
        AnalysisService::list_remainder(request)
    })
    .await
}

#[tauri::command]
pub async fn release_analysis_remainder(snapshot_id: u64) -> CommandResult<()> {
    run_blocking("release_analysis_remainder", move || {
        AnalysisService::release_remainder(snapshot_id)
    })
    .await
}
