use strawberrydisk_core::{
    DuplicateFileService, DuplicateFilesResult, DuplicateGroupPage, DuplicateScanLocation,
    PermanentDeleteBatchResult, PermanentDeleteCandidate,
};
use strawberrydisk_core::{ScanExclusionOptions, ScanNameExclusion};

use crate::events;

use super::error::{run_blocking, CommandResult};

#[tauri::command]
pub async fn find_duplicate_files(
    app: tauri::AppHandle,
    locations: Vec<DuplicateScanLocation>,
    minimum_bytes: u64,
    excluded_paths: Vec<String>,
    excluded_names: Option<Vec<ScanNameExclusion>>,
) -> CommandResult<DuplicateFilesResult> {
    run_blocking("find_duplicate_files", move || {
        let progress_app = app.clone();
        DuplicateFileService::find_paged_with_locations_and_exclusions(
            locations,
            ScanExclusionOptions {
                paths: excluded_paths,
                names: excluded_names.unwrap_or_default(),
            },
            minimum_bytes,
            move |progress| {
                events::emit(&progress_app, events::DUPLICATE_FILES_PROGRESS, progress);
            },
            move |batch| events::emit(&app, events::DUPLICATE_FILE_GROUPS, batch),
        )
    })
    .await
}

#[tauri::command]
pub fn cancel_duplicate_files() {
    DuplicateFileService::cancel();
}

#[tauri::command]
pub async fn get_duplicate_file_groups(
    scan_id: u64,
    offset: u64,
    limit: u64,
) -> CommandResult<DuplicateGroupPage> {
    run_blocking("get_duplicate_file_groups", move || {
        DuplicateFileService::page(scan_id, offset, limit)
    })
    .await
}

#[tauri::command]
pub async fn delete_duplicate_files_permanently(
    scan_id: u64,
    candidates: Vec<PermanentDeleteCandidate>,
) -> CommandResult<PermanentDeleteBatchResult> {
    run_blocking("delete_duplicate_files_permanently", move || {
        DuplicateFileService::delete_files_permanently(scan_id, candidates)
    })
    .await
}
