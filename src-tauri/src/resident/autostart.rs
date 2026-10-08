//! Login registration belongs to the OS; do not maintain a second persisted boolean.
use super::diagnostics::Failure;
use tauri_plugin_autostart::ManagerExt;

pub fn enabled(app: &tauri::AppHandle) -> Result<bool, Failure> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| Failure::record("autostart_read", &error))
}

pub fn set(app: &tauri::AppHandle, enabled: bool) -> Result<(), Failure> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|error| {
        Failure::record(
            if enabled {
                "autostart_enable"
            } else {
                "autostart_disable"
            },
            &error,
        )
    })?;
    log::info!("resident_autostart_updated enabled={enabled}");
    Ok(())
}
