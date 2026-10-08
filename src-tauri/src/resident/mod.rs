mod application_icons;
pub mod autostart;
mod diagnostics;
pub mod main_window;
mod memory_automatic;
pub mod memory_preferences;
pub mod memory_release;
pub mod memory_window;
pub mod panel;
mod preference_schema;
pub mod preferences;
mod presentation;
mod process_cpu_sampling;
pub mod runtime;
mod sampling_diagnostics;
mod sampling_schedule;
mod sampling_workers;

use std::sync::Arc;
use tauri::Manager;
pub mod taskbar_display;
pub mod tray_display;

pub const PANEL_LABEL: &str = "tray-panel";
pub const TRAY_ID: &str = "resident";

#[cfg(all(windows, debug_assertions))]
fn debug_webview_data_directory() -> tauri::Result<std::path::PathBuf> {
    // Keep development WebViews out of the installed app's profile. A stale
    // browser process in that profile can otherwise prevent the first window.
    Ok(std::env::current_exe()?.with_file_name("strawberrydisk-webview-dev"))
}

pub fn install(app: &tauri::AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    application_icons::install();
    #[cfg(windows)]
    taskbar_display::install(app);
    let tray_available = match tray_display::install(app) {
        Ok(()) => true,
        Err(error) => {
            log::warn!(
                "resident_tray_unavailable error={}",
                strawberrydisk_platform::diagnostics::text(&error)
            );
            false
        }
    };
    memory_preferences::install(app);
    let mut preferences = preferences::load(app);
    // Without a tray, a resident session would hide the only way to reopen
    // the main window. Keep the saved preference for a later successful launch.
    if !tray_available {
        preferences.enabled = false;
    }
    let state = runtime::start(app, preferences.clone());
    let reading = state
        .reading
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    if tray_available {
        if let Err(error) = tray_display::apply_preferences(app, &preferences, &reading) {
            log::warn!(
                "resident_tray_preferences_unavailable error={}",
                strawberrydisk_platform::diagnostics::text(&error)
            );
            state
                .preferences
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .enabled = false;
            state.wake();
        }
    }
    log::info!("resident_started enabled={}", state.enabled());
    Ok(())
}

/// macOS keeps its native close/reopen convention; Windows stays resident only
/// while the tray feature is enabled. Explicit Quit never enters this handler.
pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != crate::MAIN_WINDOW_LABEL {
        return;
    }
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let enabled = window
            .try_state::<Arc<runtime::ResidentState>>()
            .is_some_and(|state| state.enabled());
        if cfg!(target_os = "macos") || enabled {
            // A hidden tray cannot provide a reopening affordance on Windows.
            // macOS retains Dock reopening even when monitoring is disabled.
            api.prevent_close();
            match main_window::hide(window.app_handle(), enabled) {
                Ok(()) => log::info!("main_window_hidden reason=close_requested"),
                Err(error) => log::warn!("main_window_hide_failed error={error}"),
            }
        } else {
            // A previously opened, hidden panel is still a native window. Closing
            // the main window must therefore explicitly quit when residency is off.
            window.app_handle().exit(0);
        }
    }
}

mod disk_activity;
