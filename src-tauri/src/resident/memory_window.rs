//! A normal settings window survives focus changes; the transient monitor can close safely.
use super::memory_preferences::MemoryReleaseState;
use std::sync::Arc;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
pub const LABEL: &str = "memory-settings";

pub fn open(app: &tauri::AppHandle) -> tauri::Result<()> {
    let state = app.state::<Arc<MemoryReleaseState>>();
    let _creation = state
        .window_creation
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(window) = app.get_webview_window(LABEL) {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
    } else {
        // macOS only exposes the timer and threshold, without the application list.
        let (height, minimum_height) = if cfg!(windows) {
            (540.0, 400.0)
        } else {
            (320.0, 300.0)
        };
        let builder =
            WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("memory-settings.html".into()))
                .title("StrawberryDisk")
                .inner_size(520.0, height)
                .min_inner_size(420.0, minimum_height)
                .center()
                .visible(false);
        #[cfg(all(windows, debug_assertions))]
        let builder = builder.data_directory(super::debug_webview_data_directory()?);
        builder.build()?;
    }
    log::info!("memory_release_settings_open_requested");
    Ok(())
}
