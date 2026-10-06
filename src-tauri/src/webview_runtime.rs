//! Startup compatibility checks must work before any WebView can render UI.

// Tauri's generated runtime context omits minimumWebview2Version. Keep an
// explicit startup threshold; the configuration test prevents installer drift.
pub const MINIMUM_VERSION: &str = "111.0.1661.62";

/// Compare four numeric components instead of treating browser versions as
/// decimal numbers or lexical strings. Preview-channel suffixes are metadata.
fn version_parts(version: &str) -> Option<[u32; 4]> {
    let mut parts = version.split_ascii_whitespace().next()?.split('.');
    let mut result = [0; 4];
    for part in &mut result {
        let text = parts.next()?;
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *part = text.parse().ok()?;
    }
    parts.next().is_none().then_some(result)
}

pub fn requires_update(current: Option<&str>, minimum: Option<&str>) -> bool {
    let (Some(current), Some(minimum)) = (current, minimum) else {
        // A failed query is already logged by startup. Missing diagnostics do
        // not prove that the installed runtime is too old.
        return false;
    };
    match (version_parts(current), version_parts(minimum)) {
        (Some(current), Some(minimum)) => current < minimum,
        // Startup records the queried version after logging is initialized.
        // An unfamiliar format is diagnostic evidence, not proof of old age.
        _ => false,
    }
}

#[cfg(target_os = "windows")]
pub fn show_update_prompt(app: &tauri::AppHandle) {
    use crate::services::native_labels::NativeLabels;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    use tauri_plugin_opener::OpenerExt;

    const DOWNLOAD_URL: &str = "https://developer.microsoft.com/en-us/microsoft-edge/webview2/";

    let labels = NativeLabels::load(app);
    let app = app.clone();
    // The native dialog stays readable even when the runtime cannot evaluate
    // the application's CSS. Its callback leaves the main event loop free.
    app.dialog()
        .message(labels.text("/webviewRuntime/updateRequired"))
        .title("StrawberryDisk")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            labels.text("/webviewRuntime/update").into(),
            labels.text("/webviewRuntime/exit").into(),
        ))
        .show(move |update| {
            if update {
                if let Err(error) = app.opener().open_url(DOWNLOAD_URL, None::<&str>) {
                    log::warn!(
                        "webview_runtime_update_link_failed error={}",
                        strawberrydisk_platform::diagnostics::text(&error)
                    );
                    app.dialog()
                        .message(
                            labels
                                .text("/webviewRuntime/openFailed")
                                .replace("{url}", DOWNLOAD_URL),
                        )
                        .title("StrawberryDisk")
                        .buttons(MessageDialogButtons::OkCustom(
                            labels.text("/webviewRuntime/exit").into(),
                        ))
                        .show(move |_| app.exit(0));
                    return;
                }
                log::info!("webview_runtime_update_link_opened");
            } else {
                log::info!("webview_runtime_update_dismissed");
            }
            app.exit(0);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_version_rejects_older_components_and_accepts_the_boundary() {
        let minimum = Some("111.0.1661.62");
        for version in ["110.0.1587.69", "111.0.1660.99", "111.0.1661.61"] {
            assert!(requires_update(Some(version), minimum), "{version}");
        }
        for version in [
            "111.0.1661.62",
            "111.0.1661.63",
            "111.0.1662.0",
            "153.0.4234.32",
            "111.0.1661.62 beta",
        ] {
            assert!(!requires_update(Some(version), minimum), "{version}");
        }
    }

    #[test]
    fn unavailable_or_malformed_versions_are_not_reported_as_outdated() {
        for current in [
            None,
            Some(""),
            Some("unknown"),
            Some("110.0"),
            Some("110.0.1.2.3"),
            Some("+110.0.1.2"),
            Some("4294967296.0.0.0"),
        ] {
            assert!(!requires_update(current, Some("111.0.1661.62")));
        }
        assert!(!requires_update(Some("110.0.1587.69"), None));
        assert!(!requires_update(Some("110.0.1587.69"), Some("invalid")));
    }

    #[test]
    fn installer_and_startup_share_a_valid_minimum_version() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let minimum = config["bundle"]["windows"]["minimumWebview2Version"]
            .as_str()
            .expect("the installer must declare the runtime baseline");
        assert_eq!(minimum, MINIMUM_VERSION);
        assert_eq!(version_parts(minimum), Some([111, 0, 1661, 62]));
        assert!(requires_update(Some("110.0.1587.69"), Some(minimum)));
    }
}
