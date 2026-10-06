use serde::Deserialize;
use strawberrydisk_core::{
    SystemSettingsCatalog, SystemSettingsChangePlan, SystemSettingsChangeResult,
    SystemSettingsChangeSelection, SystemSettingsService,
};

use super::error::{run_blocking, CommandResult};

#[tauri::command]
pub async fn scan_system_settings() -> CommandResult<SystemSettingsCatalog> {
    run_blocking("scan_system_settings", SystemSettingsService::scan).await
}

#[tauri::command]
pub fn cancel_system_settings_scan() {
    SystemSettingsService::cancel_scan();
}

#[tauri::command]
pub async fn prepare_system_settings_change(
    selection: SystemSettingsChangeSelection,
) -> CommandResult<SystemSettingsChangePlan> {
    run_blocking("prepare_system_settings_change", move || {
        SystemSettingsService::prepare_change(selection)
    })
    .await
}

#[tauri::command]
pub async fn execute_system_settings_change(
    plan_id: String,
) -> CommandResult<SystemSettingsChangeResult> {
    run_blocking("execute_system_settings_change", move || {
        SystemSettingsService::execute_change(plan_id)
    })
    .await
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MacOsPrivacyDestination {
    ApplicationData,
    FilesAndFolders,
    FullDiskAccess,
}

impl MacOsPrivacyDestination {
    fn settings_uri(self, macos_major_version: Option<u32>) -> &'static str {
        // macOS 13 replaced the Security & Privacy preference pane with the
        // Privacy & Security Settings extension. Opening a Ventura-style URI
        // on Monterey merely raises System Preferences at its current pane,
        // which leaves the user without actionable permission guidance.
        if macos_major_version.is_some_and(|major| major < 13) {
            return match self {
                Self::ApplicationData => {
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_AppData"
                }
                Self::FilesAndFolders => {
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders"
                }
                Self::FullDiskAccess => {
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
                }
            };
        }

        match self {
            Self::ApplicationData => {
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AppData"
            }
            Self::FilesAndFolders => {
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_FilesAndFolders"
            }
            Self::FullDiskAccess => {
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles"
            }
        }
    }

    fn diagnostic_name(self) -> &'static str {
        match self {
            Self::ApplicationData => "application_data",
            Self::FilesAndFolders => "files_and_folders",
            Self::FullDiskAccess => "full_disk_access",
        }
    }
}

/// Opens only a known macOS privacy destination. Keeping the URI mapping in
/// the Tauri adapter avoids granting the webview access to arbitrary custom
/// schemes and keeps operating-system navigation out of the Core domains.
#[tauri::command]
pub async fn open_privacy_settings(destination: MacOsPrivacyDestination) -> CommandResult<()> {
    run_blocking("open_macos_privacy_settings", move || {
        let macos_major_version = current_macos_major_version();
        let settings_generation = if macos_major_version.is_some_and(|major| major < 13) {
            "legacy"
        } else {
            "modern"
        };
        log::info!(
            "macos_privacy_settings_open_requested destination={} settings_generation={} os_major={:?}",
            destination.diagnostic_name(),
            settings_generation,
            macos_major_version
        );
        open_settings_uri(destination.settings_uri(macos_major_version))
    })
    .await
}

#[cfg(target_os = "macos")]
fn current_macos_major_version() -> Option<u32> {
    tauri_plugin_os::version()
        .to_string()
        .split('.')
        .next()
        .and_then(|major| major.parse().ok())
}

#[cfg(not(target_os = "macos"))]
fn current_macos_major_version() -> Option<u32> {
    None
}

fn login_items_settings_uri(macos_major_version: Option<u32>) -> &'static str {
    // Monterey does not reliably handle Login Items deep links. Open its fixed
    // Users & Groups pane; the inline help identifies the Login Items tab.
    if macos_major_version.is_some_and(|major| major < 13) {
        "file:///System/Library/PreferencePanes/Accounts.prefPane"
    } else {
        "x-apple.systempreferences:com.apple.LoginItems-Settings.extension"
    }
}

/// Opens the version-appropriate Login Items pane without exposing an
/// arbitrary URL opener to the webview.
#[tauri::command]
pub async fn open_macos_login_items_settings() -> CommandResult<()> {
    run_blocking("open_macos_login_items_settings", || {
        let macos_major_version = current_macos_major_version();
        let uri = login_items_settings_uri(macos_major_version);
        log::info!(
            "macos_login_items_settings_open_requested os_major={macos_major_version:?} uri={uri}"
        );
        open_settings_uri(uri)
    })
    .await
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowsStartupTool {
    Services,
    TaskScheduler,
}

impl WindowsStartupTool {
    fn snap_in(self) -> &'static str {
        match self {
            Self::Services => "services.msc",
            Self::TaskScheduler => "taskschd.msc",
        }
    }

    fn diagnostic_name(self) -> &'static str {
        match self {
            Self::Services => "services",
            Self::TaskScheduler => "task_scheduler",
        }
    }
}

/// Opens one fixed Windows management console snap-in for startup entries that StrawberryDisk cannot
/// safely remove. The enum prevents the webview from passing arbitrary programs or arguments.
#[tauri::command]
pub async fn open_windows_startup_tool(tool: WindowsStartupTool) -> CommandResult<()> {
    run_blocking("open_windows_startup_tool", move || {
        log::info!(
            "windows_startup_tool_open_requested tool={}",
            tool.diagnostic_name()
        );
        open_windows_management_console(tool)
    })
    .await
}

#[cfg(target_os = "macos")]
fn open_settings_uri(uri: &str) -> Result<(), String> {
    tauri_plugin_opener::open_url(uri, None::<&str>)
        .map_err(|error| format!("failed to open macOS System Settings: {error}"))
}

#[cfg(not(target_os = "macos"))]
fn open_settings_uri(_uri: &str) -> Result<(), String> {
    Err("macOS privacy settings are unavailable on this platform".to_string())
}

#[cfg(target_os = "windows")]
fn open_windows_management_console(
    tool: WindowsStartupTool,
) -> Result<(), strawberrydisk_core::CoreError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::GetLastError,
        System::{
            Com::{
                CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
            SystemInformation::GetSystemDirectoryW,
        },
        UI::{
            Shell::{ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    };

    // Resolve the trusted console from Windows, never from PATH or the working
    // directory. Only the fixed enum-selected snap-in crosses the UAC boundary.
    let mut directory = vec![0u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) };
    if length == 0 || length as usize >= directory.len() {
        let code = unsafe { GetLastError() };
        log::warn!(
            "windows_startup_tool_open_failed tool={} stage=system_directory os_error={code}",
            tool.diagnostic_name()
        );
        return Err(management_console_error(code));
    }
    directory.truncate(length as usize);
    let directory = String::from_utf16_lossy(&directory);
    let wide = |value: &str| {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let executable = wide(&format!("{directory}\\mmc.exe"));
    let arguments = wide(&format!("\"{directory}\\{}\"", tool.snap_in()));
    let verb = wide("runas");
    let com = unsafe {
        CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        )
    };
    if com < 0 {
        log::warn!(
            "windows_startup_tool_open_failed tool={} stage=com hresult={com}",
            tool.diagnostic_name()
        );
        return Err(strawberrydisk_core::CoreError::operation_failed(
            "initialize management console launcher failed",
        ));
    }
    let mut execution = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: executable.as_ptr(),
        lpParameters: arguments.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..unsafe { std::mem::zeroed() }
    };
    // ShellExecute can display UAC; CreateProcess (Command::spawn) instead
    // returns error 740. Do not wait for the user to close the console.
    let launched = unsafe { ShellExecuteExW(&mut execution) } != 0;
    let code = if launched {
        0
    } else {
        unsafe { GetLastError() }
    };
    unsafe {
        CoUninitialize();
    }
    if !launched {
        let status = if code == 1223 { "cancelled" } else { "failed" };
        log::info!("windows_startup_tool_open_finished tool={} status={status} os_error={code} elevation_requested=true", tool.diagnostic_name());
        return Err(management_console_error(code));
    }
    log::info!(
        "windows_startup_tool_open_finished tool={} status=launched elevation_requested=true",
        tool.diagnostic_name()
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn open_windows_management_console(
    tool: WindowsStartupTool,
) -> Result<(), strawberrydisk_core::CoreError> {
    Err(strawberrydisk_core::CoreError::operation_failed(format!(
        "Windows startup tool {} is unavailable on this platform",
        tool.snap_in()
    )))
}

#[cfg(any(target_os = "windows", test))]
fn management_console_error(code: u32) -> strawberrydisk_core::CoreError {
    match code {
        1223 => strawberrydisk_core::CoreError::operation_cancelled(),
        5 => strawberrydisk_core::CoreError::new(
            strawberrydisk_core::CoreErrorCode::PermissionDenied,
            "management console elevation was denied",
        ),
        _ => strawberrydisk_core::CoreError::operation_failed(format!(
            "management console launch failed: os_error={code}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn management_console_errors_distinguish_cancel_permission_and_failure() {
        use strawberrydisk_core::CoreErrorCode;
        assert_eq!(
            management_console_error(1223).code(),
            CoreErrorCode::OperationCancelled
        );
        assert_eq!(
            management_console_error(5).code(),
            CoreErrorCode::PermissionDenied
        );
        assert_eq!(
            management_console_error(740).code(),
            CoreErrorCode::OperationFailed
        );
    }

    #[test]
    fn every_destination_maps_to_a_fixed_privacy_uri() {
        for destination in [
            MacOsPrivacyDestination::ApplicationData,
            MacOsPrivacyDestination::FilesAndFolders,
            MacOsPrivacyDestination::FullDiskAccess,
        ] {
            let uri = destination.settings_uri(Some(13));
            assert!(uri.starts_with(
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_"
            ));
            assert!(!uri.contains([' ', '\n', '\r']));
        }
    }

    #[test]
    fn monterey_uses_the_legacy_security_preference_pane() {
        for destination in [
            MacOsPrivacyDestination::ApplicationData,
            MacOsPrivacyDestination::FilesAndFolders,
            MacOsPrivacyDestination::FullDiskAccess,
        ] {
            let uri = destination.settings_uri(Some(12));
            assert!(
                uri.starts_with("x-apple.systempreferences:com.apple.preference.security?Privacy_")
            );
            assert!(!uri.contains([' ', '\n', '\r']));
        }
    }

    #[test]
    fn startup_tools_map_only_to_fixed_management_console_snap_ins() {
        assert_eq!(WindowsStartupTool::Services.snap_in(), "services.msc");
        assert_eq!(WindowsStartupTool::TaskScheduler.snap_in(), "taskschd.msc");
    }

    #[test]
    fn login_items_destination_is_a_fixed_settings_uri() {
        assert_eq!(
            login_items_settings_uri(Some(12)),
            "file:///System/Library/PreferencePanes/Accounts.prefPane"
        );
        for version in [Some(13), Some(26), None] {
            assert_eq!(
                login_items_settings_uri(version),
                "x-apple.systempreferences:com.apple.LoginItems-Settings.extension"
            );
        }
    }
}
