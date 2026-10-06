use std::{fs, io::Write, os::windows::fs::MetadataExt, path::PathBuf};

use windows_sys::Win32::{
    Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT,
    System::SystemInformation::GetWindowsDirectoryW,
};
use winreg::{enums::*, RegKey};

use crate::{
    PlatformError, PlatformErrorCode, PlatformResult, PlatformSystemSettingSnapshot,
    PlatformSystemSettingValue,
};

pub(super) const SETTING_ID: &str = "windows.explorer.hide-shortcut-arrows";
const REGISTRY_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Shell Icons";
const VALUE_NAME: &str = "29";
const ASSET_NAME: &str = "StrawberryDisk-shortcut-overlay-v2.ico";
const LEGACY_ASSET_NAME: &str = "StrawberryDisk-shortcut-overlay-v1.ico";

/// Shell icon overrides refer to a real, durable transparent icon. A missing file or an
/// undocumented blank index in a system DLL can produce black squares after cache rebuilding.
/// Keep this tiny resource in the protected Windows directory so all users can read it and an
/// application update/uninstall cannot invalidate the system-wide preference.
fn asset_path(name: &str) -> PlatformResult<PathBuf> {
    let mut buffer = vec![0_u16; 32_768];
    // SAFETY: the buffer is writable for the supplied number of UTF-16 code units.
    let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(failure("resolve_asset", std::io::Error::last_os_error()));
    }
    use std::os::windows::ffi::OsStringExt;
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length])).join(name))
}

fn expected_icon_value() -> PlatformResult<String> {
    Ok(format!("{},0", asset_path(ASSET_NAME)?.to_string_lossy()))
}

fn owned_icon_bytes(value: &str) -> PlatformResult<Option<&'static [u8]>> {
    if value == expected_icon_value()? {
        Ok(Some(icon::TRANSPARENT_ICON))
    } else if value == format!("{},0", asset_path(LEGACY_ASSET_NAME)?.to_string_lossy()) {
        Ok(Some(&icon::LEGACY_ICON))
    } else {
        Ok(None)
    }
}

pub(super) fn read() -> PlatformResult<PlatformSystemSettingValue> {
    // A user override can shadow the machine setting. Do not report a verified visual change
    // when another tool owns that override; keep both its value and shortcut type untouched.
    let user = RegKey::predef(HKEY_CURRENT_USER);
    match user.open_subkey_with_flags(REGISTRY_PATH, KEY_READ | KEY_WOW64_64KEY) {
        Ok(key) => match key.get_raw_value(VALUE_NAME) {
            Ok(_) => {
                log::warn!("windows_shortcut_overlay_failed stage=read_user_override reason=external_override");
                return Err(PlatformError::new(
                    PlatformErrorCode::Unsupported,
                    "a user shortcut overlay is already configured",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(failure("read_user_override", error)),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(failure("read_user_key", error)),
    }
    let root = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match root.open_subkey_with_flags(REGISTRY_PATH, KEY_READ | KEY_WOW64_64KEY) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PlatformSystemSettingValue::Missing)
        }
        Err(error) => return Err(failure("read_key", error)),
    };
    let raw = match key.get_raw_value(VALUE_NAME) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PlatformSystemSettingValue::Missing)
        }
        Err(error) => return Err(failure("read_value", error)),
    };
    // Recovery must preserve the exact native type. Refuse other types rather than converting a
    // third-party REG_EXPAND_SZ or binary value into REG_SZ during restore.
    if raw.vtype != REG_SZ {
        log::warn!(
            "windows_shortcut_overlay_failed stage=read_value reason=unsupported_registry_type"
        );
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "shortcut overlay value has an unsupported registry type",
        ));
    }
    let value: String = key
        .get_value(VALUE_NAME)
        .map_err(|error| failure("read_text", error))?;
    let Some(bytes) = owned_icon_bytes(&value)? else {
        log::warn!("windows_shortcut_overlay_failed stage=read_value reason=external_override");
        return Err(PlatformError::new(
            PlatformErrorCode::Unsupported,
            "a third-party shortcut overlay is already configured",
        ));
    };
    let legacy = value != expected_icon_value()?;
    let name = if legacy {
        LEGACY_ASSET_NAME
    } else {
        ASSET_NAME
    };
    if legacy {
        log::info!("windows_shortcut_overlay_legacy_detected resource_version=1 upgrade=disable_then_enable");
    }
    if !asset_is_valid(name, bytes).unwrap_or(false) {
        log::warn!(
            "windows_shortcut_overlay_failed stage=read_asset reason=asset_missing_or_invalid"
        );
    }
    Ok(PlatformSystemSettingValue::Snapshot(
        PlatformSystemSettingSnapshot::Text(value),
    ))
}

pub(super) fn effective(value: &PlatformSystemSettingValue) -> PlatformSystemSettingValue {
    let hidden = match value {
        PlatformSystemSettingValue::Snapshot(PlatformSystemSettingSnapshot::Text(value)) => {
            // State describes the configured override, even if its resource was externally
            // damaged or uses v1. Users can disable and re-enable to install v2.
            // Marking it disabled would invalidate Core's recovery baseline and hide
            // the restore action precisely when the user needs to remove a broken override.
            owned_icon_bytes(value).is_ok_and(|bytes| bytes.is_some())
        }
        _ => false,
    };
    PlatformSystemSettingValue::Boolean(hidden)
}

pub(super) fn valid_value(value: &PlatformSystemSettingValue) -> bool {
    match value {
        PlatformSystemSettingValue::Missing | PlatformSystemSettingValue::Boolean(_) => true,
        PlatformSystemSettingValue::Snapshot(PlatformSystemSettingSnapshot::Text(value)) => {
            owned_icon_bytes(value).is_ok_and(|bytes| bytes.is_some())
        }
        _ => false,
    }
}

pub(super) fn write(value: &PlatformSystemSettingValue) -> PlatformResult<()> {
    if !valid_value(value) {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "shortcut overlay request is invalid",
        ));
    }
    let desired = match value {
        PlatformSystemSettingValue::Boolean(true) => {
            ensure_asset()?;
            Some(expected_icon_value()?)
        }
        PlatformSystemSettingValue::Snapshot(PlatformSystemSettingSnapshot::Text(value)) => {
            Some(value.clone())
        }
        PlatformSystemSettingValue::Boolean(false) | PlatformSystemSettingValue::Missing => None,
        _ => unreachable!("validated shortcut overlay value"),
    };
    let root = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _) = root
        .create_subkey_with_flags(REGISTRY_PATH, KEY_SET_VALUE | KEY_WOW64_64KEY)
        .map_err(|error| failure("write_key", error))?;
    match desired {
        Some(value) => key
            .set_value(VALUE_NAME, &value)
            .map_err(|error| failure("write_value", error))?,
        None => match key.delete_value(VALUE_NAME) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(failure("delete_value", error)),
        },
    }
    // IsShortcut and other icon overrides are intentionally untouched. The Core recovery record
    // is saved before this write. Foreign overrides are refused instead of overwritten. Do not
    // delete the asset on restore: an older recovery snapshot may still reference it.
    log::info!(
        "windows_shortcut_overlay_written action={} restart_required=true",
        match value {
            PlatformSystemSettingValue::Boolean(true) => "hide",
            PlatformSystemSettingValue::Snapshot(_) => "restore_previous",
            _ => "show_default",
        }
    );
    Ok(())
}

fn asset_is_valid(name: &str, expected: &[u8]) -> PlatformResult<bool> {
    let path = asset_path(name)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(failure("inspect_asset", error)),
    };
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.len() != expected.len() as u64
    {
        return Ok(false);
    }
    Ok(fs::read(path).map_err(|error| failure("verify_asset", error))? == expected)
}

fn ensure_asset() -> PlatformResult<()> {
    if asset_is_valid(ASSET_NAME, icon::TRANSPARENT_ICON)? {
        return Ok(());
    }
    // create_new refuses collisions and reparse points. Never overwrite a foreign file under
    // elevation, even when its name happens to match our fixed resource name.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(asset_path(ASSET_NAME)?)
        .map_err(|error| failure("create_asset", error))?;
    file.write_all(icon::TRANSPARENT_ICON)
        .and_then(|_| file.sync_all())
        .map_err(|error| failure("write_asset", error))?;
    if !asset_is_valid(ASSET_NAME, icon::TRANSPARENT_ICON)? {
        return Err(PlatformError::operation_failed(
            "shortcut overlay asset verification failed",
        ));
    }
    log::info!(
        "windows_shortcut_overlay_asset_ready resource_version=2 bytes={}",
        icon::TRANSPARENT_ICON.len()
    );
    Ok(())
}

fn failure(stage: &'static str, error: std::io::Error) -> PlatformError {
    log::warn!(
        "windows_shortcut_overlay_failed stage={stage} native_code={:?} error_kind={:?}",
        error.raw_os_error(),
        error.kind()
    );
    PlatformError::new(
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            PlatformErrorCode::AccessDenied
        } else {
            PlatformErrorCode::OperationFailed
        },
        "shortcut overlay operation failed",
    )
}

#[path = "shortcut_overlay/icon.rs"]
mod icon;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_override_remains_restorable_without_reading_its_icon_file() {
        let value = PlatformSystemSettingValue::Snapshot(PlatformSystemSettingSnapshot::Text(
            expected_icon_value().expect("Windows directory should be available"),
        ));
        assert_eq!(effective(&value), PlatformSystemSettingValue::Boolean(true));
        assert_eq!(
            effective(&PlatformSystemSettingValue::Missing),
            PlatformSystemSettingValue::Boolean(false)
        );
    }

    #[test]
    fn legacy_override_is_owned_and_restorable_but_new_enable_uses_v2() {
        let legacy = format!(
            "{},0",
            asset_path(LEGACY_ASSET_NAME).unwrap().to_string_lossy()
        );
        let snapshot = PlatformSystemSettingValue::Snapshot(PlatformSystemSettingSnapshot::Text(
            legacy.clone(),
        ));
        assert!(valid_value(&snapshot));
        assert_eq!(
            effective(&snapshot),
            PlatformSystemSettingValue::Boolean(true)
        );
        assert_eq!(
            owned_icon_bytes(&legacy).unwrap(),
            Some(icon::LEGACY_ICON.as_slice())
        );
        assert_ne!(expected_icon_value().unwrap(), legacy);
        assert_eq!(
            owned_icon_bytes(&expected_icon_value().unwrap()).unwrap(),
            Some(icon::TRANSPARENT_ICON)
        );
    }

    #[test]
    fn privileged_requests_cannot_install_arbitrary_icon_or_dll_paths() {
        assert!(valid_value(&PlatformSystemSettingValue::Boolean(true)));
        assert!(valid_value(&PlatformSystemSettingValue::Boolean(false)));
        assert!(valid_value(&PlatformSystemSettingValue::Missing));
        for value in [
            r"C:\Users\fixture\payload.dll,0",
            r"\\server\share\icon.ico,0",
            "",
            "29",
        ] {
            assert!(!valid_value(&PlatformSystemSettingValue::Text(
                value.into()
            )));
            assert!(!valid_value(&PlatformSystemSettingValue::Snapshot(
                PlatformSystemSettingSnapshot::Text(value.into())
            )));
        }
        assert!(valid_value(&PlatformSystemSettingValue::Snapshot(
            PlatformSystemSettingSnapshot::Text(
                expected_icon_value().expect("Windows directory should be available")
            )
        )));
    }
}
