use std::sync::{atomic::Ordering, Arc};

use mangodisk_core::system_resources::metrics::MetricId;
use tauri::{Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use super::{runtime::ResidentState, PANEL_LABEL};

const WIDTH: f64 = 390.0;
const HEIGHT: f64 = 610.0;

pub const METRIC_EVENT: &str = "resident-panel-metric";
pub const VISIBILITY_EVENT: &str = "resident-panel-visibility";

pub fn select_metric(app: &tauri::AppHandle, metric: MetricId) {
    let state = app.state::<Arc<ResidentState>>();
    *state
        .panel_metric
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = metric;
    let _ = app.emit_to(PANEL_LABEL, METRIC_EVENT, metric);
    if metric == MetricId::Cpu && state.panel_open.load(Ordering::Relaxed) {
        state.refresh_cpu_processes();
    } else {
        state.wake();
    }
}

pub fn toggle_from(
    app: &tauri::AppHandle,
    source: &str,
    metric: Option<MetricId>,
) -> tauri::Result<()> {
    let state = app.state::<Arc<ResidentState>>();
    let _action = state
        .panel_action
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let same = state
        .panel_source
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_str()
        == source;
    if state.panel_open.load(Ordering::Relaxed) && same {
        log::info!("resident_panel_entry source={source} action=hide reason=repeated_click");
        hide(app);
        return Ok(());
    }
    *state
        .panel_source
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = source.into();
    let was_open = state.panel_open.load(Ordering::Relaxed);
    let previous = *state
        .panel_metric
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let selected = entry_selection(previous, metric, was_open);
    if selected != previous {
        select_metric(app, selected);
    }
    log::info!("resident_panel_entry source={source} switching={was_open} metric={selected:?}");
    open(app)
}

fn entry_selection(previous: MetricId, requested: Option<MetricId>, is_open: bool) -> MetricId {
    // Reopening resumes the user's tab. Explicit metric shortcuts can still
    // navigate an already open panel, without resetting the next visit to CPU.
    if is_open {
        requested.unwrap_or(previous)
    } else {
        previous
    }
}

pub fn open(app: &tauri::AppHandle) -> tauri::Result<()> {
    let state = app.state::<Arc<ResidentState>>();
    if !state.enabled() {
        return Ok(());
    }
    *state
        .panel_requested_at
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(std::time::Instant::now());
    log::info!(
        "resident_panel_open_requested ready={}",
        state.panel_ready.load(Ordering::Relaxed)
    );
    state.panel_open.store(true, Ordering::Relaxed);
    if *state.panel_metric.lock().unwrap_or_else(|e| e.into_inner()) == MetricId::Cpu {
        state.refresh_cpu_processes();
    }
    // Showing never waits for Vue, preference I/O, samples, or icon resolution.
    // Usually startup has already prepared the hidden WebView; an early click
    // still reveals its first frame while frontend initialization completes.
    if let Err(error) = ensure_created(app).and_then(|()| show(app)) {
        // Failed native creation/positioning must not retain open intent: it
        // would keep detailed sampling active and consume the next click as hide.
        hide(app);
        return Err(error);
    }
    state.wake();
    Ok(())
}

/// Prepare one hidden WebView without changing foreground or sampling intent.
/// Leave the native callback before creating it to keep WebView2 responsive.
pub fn prewarm(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if app.state::<Arc<ResidentState>>().enabled() {
            if let Err(error) = ensure_created(&app) {
                log::warn!("resident_panel_prewarm_failed error={error}");
            }
        }
    });
}

fn ensure_created(app: &tauri::AppHandle) -> tauri::Result<()> {
    let state = app.state::<Arc<ResidentState>>();
    // Startup preparation and an early click share one creation lock.
    let creation = state
        .panel_creation
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if app.get_webview_window(PANEL_LABEL).is_none() {
        let started = std::time::Instant::now();
        let builder =
            WebviewWindowBuilder::new(app, PANEL_LABEL, WebviewUrl::App("tray-panel.html".into()))
                .title("MangoDisk")
                .inner_size(WIDTH, HEIGHT)
                .resizable(false)
                .decorations(false)
                .visible(false)
                .focused(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .shadow(true);
        // A titled macOS window keeps the system's rounded frame and shadow.
        // Overlay lets the panel content fill it without a separate title bar.
        #[cfg(target_os = "macos")]
        let builder = builder
            // The tray panel may appear while another application is active.
            // Deliver that first click to its controls instead of consuming it
            // only for activation while the OS completes the focus transition.
            .accept_first_mouse(true)
            .decorations(true)
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
        let window = builder.build()?;
        #[cfg(target_os = "macos")]
        window.with_webview(|webview| {
            use objc2_app_kit::{NSWindow, NSWindowButton};

            // Tauri runs this callback on the main thread and owns the NSWindow
            // throughout the callback. Only public AppKit APIs are used here.
            let native = unsafe { &*webview.ns_window().cast::<NSWindow>() };
            for kind in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(button) = native.standardWindowButton(kind) {
                    button.setHidden(true);
                }
            }
        })?;
        let app = app.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::Focused(false) => {
                log::info!("resident_panel_focus_changed focused=false");
                // Windows activates the taskbar before delivering the tray click.
                // Preserve open intent until that click toggles it, otherwise the
                // same click would immediately reopen the panel we just hid.
                if tray_owns_focus(&app) {
                    log::info!("resident_panel_blur_deferred reason=tray_interaction");
                } else {
                    hide(&app);
                }
            }
            WindowEvent::Focused(true) => {
                log::info!("resident_panel_focus_changed focused=true");
                let state = app.state::<Arc<ResidentState>>();
                if let Some(started) = state
                    .panel_requested_at
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                {
                    log::info!(
                        "resident_panel_focused elapsed_ms={}",
                        started.elapsed().as_millis()
                    );
                };
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                hide(&app);
            }
            _ => {}
        });
        log::info!(
            "resident_panel_created elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    drop(creation);
    Ok(())
}

#[cfg(windows)]
fn tray_owns_focus(app: &tauri::AppHandle) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, WindowFromPoint, GA_ROOT,
    };
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return false;
    };
    let Ok(cursor) = window.cursor_position() else {
        return false;
    };
    let over_entry = super::tray_display::format::DisplayId::ALL
        .into_iter()
        .any(|id| {
            app.tray_by_id(id.tray_id())
                .and_then(|tray| tray.rect().ok().flatten())
                .is_some_and(|rect| {
                    let position = rect.position.to_physical::<f64>(1.0);
                    let size = rect.size.to_physical::<f64>(1.0);
                    contains(
                        (cursor.x, cursor.y),
                        (position.x, position.y, size.width, size.height),
                    )
                })
        });
    if !over_entry && !super::taskbar_display::contains_point(app, cursor.x, cursor.y) {
        return false;
    }
    // Cursor position alone is insufficient: Alt+Tab must still dismiss the
    // panel when the pointer happens to remain over the tray icon.
    unsafe {
        let surface = WindowFromPoint(windows_sys::Win32::Foundation::POINT {
            x: cursor.x as i32,
            y: cursor.y as i32,
        });
        // Embedded no-activate children can receive the press before Explorer
        // becomes foreground. Keep open intent until button-up consumes it.
        // A pointer merely parked here must not prevent keyboard dismissal.
        !surface.is_null()
            && defer_entry_blur(
                GetAncestor(surface, GA_ROOT) == GetForegroundWindow(),
                GetAsyncKeyState(VK_LBUTTON as i32) < 0,
            )
    }
}

#[cfg(any(windows, test))]
fn defer_entry_blur(entry_has_focus: bool, left_pressed: bool) -> bool {
    entry_has_focus || left_pressed
}

#[cfg(not(windows))]
fn tray_owns_focus(_app: &tauri::AppHandle) -> bool {
    false
}

#[cfg(windows)]
pub fn tray_pointer_left(app: &tauri::AppHandle) {
    // Dismiss a deferred blur if a tray press was abandoned or a context menu
    // took focus. A focused panel stays open when the pointer enters it.
    // A newly opened WebView may not have received its first focus event yet.
    if app
        .state::<Arc<ResidentState>>()
        .panel_requested_at
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_some()
    {
        return;
    }
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        if !window.is_focused().unwrap_or(false) {
            hide(app);
        }
    }
}

pub fn hide(app: &tauri::AppHandle) {
    let state = app.state::<Arc<ResidentState>>();
    state.panel_open.store(false, Ordering::Relaxed);
    *state
        .panel_requested_at
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        if let Err(error) = window.hide() {
            log::warn!("resident_panel_hide_failed error={error}");
        } else {
            let _ = app.emit_to(PANEL_LABEL, VISIBILITY_EVENT, false);
        }
    }
    state.wake();
}

pub fn ready(app: &tauri::AppHandle) {
    let state = app.state::<Arc<ResidentState>>();
    state.panel_ready.store(true, Ordering::Relaxed);
    // A prewarmed page becoming ready must not reveal itself or steal focus.
    log::info!("resident_panel_ready");
}

fn show(app: &tauri::AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let target = app.clone();
        // Linux monitor work-area queries access GDK directly. Dispatch the
        // entire positioning sequence to GTK's main thread to avoid concurrent
        // X11 requests from menu or command workers corrupting the connection.
        app.run_on_main_thread(move || {
            if let Err(error) = show_window(&target) {
                hide(&target);
                super::diagnostics::Failure::record("panel_show", &error);
            }
        })
    }
    #[cfg(not(target_os = "linux"))]
    show_window(app)
}

fn show_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let state = app.state::<Arc<ResidentState>>();
    if !state.panel_open.load(Ordering::Relaxed) || !state.enabled() {
        return Ok(());
    }
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return Ok(());
    };
    let source = state
        .panel_source
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let anchor = app
        .tray_by_id(&source)
        .and_then(|tray| tray.rect().ok().flatten());
    let anchor = anchor.map(|rect| {
        let position = rect.position.to_physical::<f64>(1.0);
        let size = rect.size.to_physical::<f64>(1.0);
        (
            position.x + size.width / 2.0,
            position.y + size.height / 2.0,
        )
    });
    #[cfg(windows)]
    let anchor = super::taskbar_display::anchor(app, &source).or(anchor);
    // Tray rectangles are physical pixels. On macOS monitor_from_point uses
    // logical screen points, so match against physical monitor bounds ourselves.
    let monitor = match anchor {
        Some(point) => app.available_monitors()?.into_iter().find(|monitor| {
            contains(
                point,
                (
                    monitor.position().x as f64,
                    monitor.position().y as f64,
                    monitor.size().width as f64,
                    monitor.size().height as f64,
                ),
            )
        }),
        None => None,
    };
    let monitor = match monitor {
        Some(monitor) => Some(monitor),
        None => app.primary_monitor()?,
    };
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let width = (WIDTH * scale).min(area.size.width as f64);
        let height = (HEIGHT * scale).min(area.size.height as f64);
        #[cfg(not(windows))]
        window.set_size(tauri::LogicalSize::new(width / scale, height / scale))?;
        let work = (
            area.position.x as f64,
            area.position.y as f64,
            area.size.width as f64,
            area.size.height as f64,
        );
        let anchor = anchor.unwrap_or((work.0 + work.2 - width / 2.0, work.1));
        let (x, y) = position(anchor, work, (width, height), 6.0 * scale);
        window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))?;
        // A hidden Windows panel can retain the previous display's DPI. Move
        // first, then apply the target monitor's physical size so logical-size
        // conversion cannot clip the WebView after a scale/display change.
        #[cfg(windows)]
        window.set_size(tauri::PhysicalSize::new(
            width.round() as u32,
            height.round() as u32,
        ))?;
        log::info!(
            "resident_panel_positioned scale={scale} x={x} y={y} width={width} height={height}"
        );
    }
    window.show()?;
    // Windows can reveal a taskbar popup before granting native focus. Its
    // visible content must resume independently of that later focus event.
    let _ = app.emit_to(PANEL_LABEL, VISIBILITY_EVENT, true);
    #[cfg(target_os = "macos")]
    window.with_webview(|webview| {
        use objc2_app_kit::NSWindow;

        // Activation restores the app's main/key windows. Select the panel first,
        // then let Tauri perform its native focus/activation sequence across macOS
        // versions; this prevents the product window from being brought forward.
        let native = unsafe { &*webview.ns_window().cast::<NSWindow>() };
        native.makeMainWindow();
        native.makeKeyAndOrderFront(None);
    })?;
    window.set_focus()?;
    if let Some(started) = state
        .panel_requested_at
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
    {
        log::info!(
            "resident_panel_show_dispatched elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    Ok(())
}

fn contains(point: (f64, f64), bounds: (f64, f64, f64, f64)) -> bool {
    point.0 >= bounds.0
        && point.0 < bounds.0 + bounds.2
        && point.1 >= bounds.1
        && point.1 < bounds.1 + bounds.3
}

/// Use physical coordinates consistently across Retina/non-Retina displays and taskbar edges.
fn position(
    anchor: (f64, f64),
    area: (f64, f64, f64, f64),
    size: (f64, f64),
    gap: f64,
) -> (f64, f64) {
    let (left, top, width, height) = area;
    // Side-taskbar anchors lie outside the work area horizontally. Center the
    // panel on the clicked metric while keeping the entire panel on the desktop.
    if (anchor.0 < left || anchor.0 >= left + width) && anchor.1 >= top && anchor.1 < top + height {
        let x = if anchor.0 < left {
            left + gap
        } else {
            left + width - size.0 - gap
        };
        let y = (anchor.1 - size.1 / 2.0).clamp(top, top + (height - size.1).max(0.0));
        return (x.clamp(left, left + (width - size.0).max(0.0)), y);
    }
    let x = (anchor.0 - size.0 / 2.0).clamp(left, left + (width - size.0).max(0.0));
    let y = if anchor.1 < top + height / 2.0 {
        top + gap
    } else {
        top + height - size.1 - gap
    };
    (x, y.clamp(top, top + (height - size.1).max(0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_press_preserves_toggle_intent_during_foreground_transition() {
        assert!(defer_entry_blur(false, true));
        assert!(defer_entry_blur(true, false));
        assert!(!defer_entry_blur(false, false));
    }

    #[test]
    fn reopening_remembers_the_tab_while_open_metric_shortcuts_still_navigate() {
        for previous in [MetricId::Cpu, MetricId::Memory] {
            for requested in [
                None,
                Some(MetricId::Cpu),
                Some(MetricId::Memory),
                Some(MetricId::Network),
            ] {
                assert_eq!(entry_selection(previous, requested, false), previous);
            }
        }
        assert_eq!(
            entry_selection(MetricId::Memory, None, true),
            MetricId::Memory
        );
        assert_eq!(
            entry_selection(MetricId::Memory, Some(MetricId::Cpu), true),
            MetricId::Cpu
        );
        assert_eq!(
            entry_selection(MetricId::Cpu, Some(MetricId::Memory), true),
            MetricId::Memory
        );
    }

    #[test]
    fn physical_monitor_selection_handles_retina_edges_and_negative_origins() {
        assert!(contains((2248.0, 24.0), (0.0, 0.0, 3840.0, 2160.0)));
        assert!(!contains((3840.0, 24.0), (0.0, 0.0, 3840.0, 2160.0)));
        assert!(contains((-100.0, 24.0), (-1920.0, 0.0, 1920.0, 1080.0)));
        assert!(!contains((0.0, -1.0), (0.0, 0.0, 3840.0, 2160.0)));
    }

    #[test]
    fn side_taskbar_panels_open_inside_the_work_area_at_the_selected_metric() {
        assert_eq!(
            position(
                (40.0, 500.0),
                (80.0, 0.0, 1840.0, 1080.0),
                (390.0, 610.0),
                6.0
            ),
            (86.0, 195.0)
        );
        assert_eq!(
            position(
                (1880.0, 500.0),
                (0.0, 0.0, 1840.0, 1080.0),
                (390.0, 610.0),
                6.0
            ),
            (1444.0, 195.0)
        );
    }

    #[test]
    fn panel_fits_top_bottom_and_negative_origin_monitors() {
        assert_eq!(
            position(
                (1900.0, 10.0),
                (0.0, 24.0, 1920.0, 1056.0),
                (390.0, 610.0),
                6.0
            ),
            (1530.0, 30.0)
        );
        assert_eq!(
            position(
                (20.0, 1080.0),
                (0.0, 0.0, 1920.0, 1040.0),
                (390.0, 610.0),
                6.0
            ),
            (0.0, 424.0)
        );
        assert_eq!(
            position(
                (-10.0, 0.0),
                (-1920.0, 24.0, 1920.0, 1056.0),
                (780.0, 900.0),
                12.0
            ),
            (-780.0, 36.0)
        );
        assert_eq!(
            position((0.0, 0.0), (0.0, 0.0, 300.0, 400.0), (300.0, 400.0), 6.0),
            (0.0, 0.0)
        );
    }
}
