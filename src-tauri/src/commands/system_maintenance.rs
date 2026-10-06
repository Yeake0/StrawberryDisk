use std::sync::Arc;

use strawberrydisk_core::{
    SystemMaintenanceCatalog, SystemMaintenanceExecutionRequest, SystemMaintenanceJob,
    SystemMaintenanceRuntimeState, SystemMaintenanceService,
};

use super::error::{into_command_result, run_blocking, CommandResult};
use crate::events;

#[tauri::command]
pub async fn scan_system_maintenance() -> CommandResult<SystemMaintenanceCatalog> {
    run_blocking("scan_system_maintenance", SystemMaintenanceService::scan).await
}

#[tauri::command]
pub fn cancel_system_maintenance_scan() {
    SystemMaintenanceService::cancel_scan();
}

#[tauri::command]
pub fn execute_system_maintenance(
    app: tauri::AppHandle,
    request: SystemMaintenanceExecutionRequest,
) -> CommandResult<SystemMaintenanceJob> {
    let event_app = app.clone();
    let sink = Arc::new(move |job| {
        events::emit(&event_app, events::SYSTEM_MAINTENANCE_JOB_UPDATED, job);
    });
    into_command_result(
        "execute_system_maintenance",
        SystemMaintenanceService::start_execution(request, sink),
    )
}

#[tauri::command]
pub fn cancel_system_maintenance_execution(
    execution_id: String,
) -> CommandResult<SystemMaintenanceJob> {
    into_command_result(
        "cancel_system_maintenance_execution",
        SystemMaintenanceService::cancel_execution(&execution_id),
    )
}

#[tauri::command]
pub fn get_system_maintenance_runtime() -> CommandResult<SystemMaintenanceRuntimeState> {
    into_command_result(
        "get_system_maintenance_runtime",
        SystemMaintenanceService::runtime_state(),
    )
}
