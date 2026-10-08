use serde::Deserialize;
use strawberrydisk_core::{
    ApplicationCloseBatchResult, CleanupApplicationCloseRequest, CleanupRequest, CleanupResult,
    CleanupScanResult, CleanupScanService, CleanupService, CustomCleanupRule, ScanRuleResult,
};
use strawberrydisk_core::{ScanExclusionOptions, ScanNameExclusion};

use crate::events;

use super::error::{run_blocking, CommandResult};

#[derive(Debug, Default, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CleanupScanScope {
    #[default]
    Standard,
    SelectedVolumes {
        volume_mount_points: Vec<String>,
    },
    Custom {
        rules: Vec<CustomCleanupRule>,
        #[serde(default)]
        include_standard_rules: bool,
        #[serde(default)]
        scan_id: Option<u64>,
    },
}

#[tauri::command]
pub async fn scan_cleanup_candidates(
    app: tauri::AppHandle,
    scan_scope: Option<CleanupScanScope>,
    excluded_paths: Option<Vec<String>>,
    excluded_names: Option<Vec<ScanNameExclusion>>,
) -> CommandResult<CleanupScanResult> {
    run_blocking("scan_cleanup_candidates", move || {
        let progress = move |value| events::emit(&app, events::CLEANUP_SCAN_PROGRESS, value);
        let excluded_paths = ScanExclusionOptions {
            paths: excluded_paths.unwrap_or_default(),
            names: excluded_names.unwrap_or_default(),
        };
        match scan_scope.unwrap_or_default() {
            CleanupScanScope::Standard => {
                CleanupScanService::scan_with_exclusions(excluded_paths, progress)
            }
            CleanupScanScope::SelectedVolumes {
                volume_mount_points,
            } => CleanupScanService::scan_with_selected_volumes_and_exclusions(
                volume_mount_points,
                excluded_paths,
                progress,
            ),
            CleanupScanScope::Custom {
                rules,
                include_standard_rules,
                ..
            } => CleanupScanService::scan_with_custom_rules_and_exclusions(
                rules,
                include_standard_rules,
                excluded_paths,
                progress,
            ),
        }
    })
    .await
}

#[tauri::command]
pub fn cancel_cleanup_scan() {
    CleanupScanService::cancel();
}

#[tauri::command]
pub async fn scan_windows_previous_installations_with_privileges() -> CommandResult<ScanRuleResult>
{
    run_blocking(
        "scan_windows_previous_installations_with_privileges",
        CleanupScanService::scan_previous_installations_with_privileges,
    )
    .await
}

#[tauri::command]
pub fn cancel_cleanup_execution() {
    CleanupService::cancel();
}

#[tauri::command]
pub async fn close_cleanup_applications(
    request: CleanupApplicationCloseRequest,
) -> CommandResult<ApplicationCloseBatchResult> {
    run_blocking("close_cleanup_applications", move || {
        CleanupService::close_applications(request)
    })
    .await
}

#[tauri::command]
pub async fn execute_cleanup(
    app: tauri::AppHandle,
    mut request: CleanupRequest,
    scan_scope: Option<CleanupScanScope>,
    excluded_paths: Option<Vec<String>>,
    excluded_names: Option<Vec<ScanNameExclusion>>,
    deep_cleanup_operation_id: String,
) -> CommandResult<CleanupResult> {
    // Arbitrary WebView paths remain untrusted. Execution replaces them with
    // mount points matched against the live platform inventory so the selected
    // volume scope can be reproduced without widening it to arbitrary folders.
    request.project_roots.clear();
    run_blocking("execute_cleanup", move || {
        let excluded_paths = ScanExclusionOptions { paths: excluded_paths.unwrap_or_default(), names: excluded_names.unwrap_or_default() };
        let progress = move |value| {
            events::emit(&app, events::CLEANUP_EXECUTION_PROGRESS, value);
        };
        match scan_scope.unwrap_or_default() {
            CleanupScanScope::Standard => CleanupService::execute_deep_cleanup_step_with_exclusions_and_progress(
                request,
                deep_cleanup_operation_id,
                excluded_paths,
                progress,
            ),
            CleanupScanScope::SelectedVolumes {
                volume_mount_points,
            } => {
                request.project_roots = volume_mount_points;
                CleanupService::execute_deep_cleanup_step_with_selected_volumes_and_exclusions_and_progress(
                    request,
                    deep_cleanup_operation_id,
                    excluded_paths,
                    progress,
                )
            }
            CleanupScanScope::Custom {
                rules,
                include_standard_rules,
                scan_id,
            } => {
                let scan_id = scan_id
                    .ok_or_else(|| "the custom cleanup result expired; scan again".to_string())?;
                CleanupService::execute_deep_cleanup_step_with_custom_rules_and_exclusions_and_progress(
                    request,
                    deep_cleanup_operation_id,
                    scan_id,
                    rules,
                    include_standard_rules,
                    excluded_paths,
                    progress,
                )
            }
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::CleanupScanScope;

    #[test]
    fn selected_volume_scope_accepts_the_frontend_payload_shape() {
        let scope = serde_json::from_value::<CleanupScanScope>(serde_json::json!({
            "mode": "selectedVolumes",
            "volumeMountPoints": ["C:\\"]
        }))
        .expect("deserialize the frontend scan scope");

        assert!(matches!(scope, CleanupScanScope::SelectedVolumes { .. }));
    }

    #[test]
    fn custom_scope_accepts_the_frontend_payload_shape() {
        let scope = serde_json::from_value::<CleanupScanScope>(serde_json::json!({
            "mode": "custom",
            "includeStandardRules": false,
            "rules": [{
                "schemaVersion": 1,
                "id": "temporary-files",
                "name": "Temporary files",
                "roots": ["/tmp/example"],
                "namePatterns": ["*.tmp"],
                "minimumBytes": null,
                "maximumBytes": null,
                "modifiedTime": { "mode": "any" },
                "recursive": true
            }]
        }))
        .expect("deserialize the custom scan scope");

        assert!(matches!(
            scope,
            CleanupScanScope::Custom {
                include_standard_rules: false,
                ..
            }
        ));
    }
}
