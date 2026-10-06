use std::mem::size_of;
use std::path::Path;

use windows::Win32::{
    Foundation::{HANDLE, HWND, TRUST_E_NOSIGNATURE},
    Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
        WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE,
        WTD_STATEACTION_IGNORE, WTD_UI_NONE,
    },
};
use windows_core::{HSTRING, PCWSTR, PWSTR};

use crate::PlatformStartupTrustState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FilesystemTargetState {
    Present,
    Missing,
    Unknown,
}

pub(super) use crate::windows::file_version::file_version_metadata;

pub(super) fn startup_trust(
    path: Option<&Path>,
    system_candidate: bool,
) -> PlatformStartupTrustState {
    let Some(path) = path.filter(|path| path.is_file()) else {
        return PlatformStartupTrustState::Unknown;
    };
    let path = HSTRING::from(path.as_os_str());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: std::ptr::null_mut(),
        pSIPClientData: std::ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_IGNORE,
        hWVTStateData: HANDLE::default(),
        pwszURLReference: PWSTR::null(),
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        dwUIContext: Default::default(),
        pSignatureSettings: std::ptr::null_mut(),
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    if status == 0 {
        if system_candidate {
            PlatformStartupTrustState::System
        } else {
            PlatformStartupTrustState::Verified
        }
    } else if status == TRUST_E_NOSIGNATURE.0 {
        PlatformStartupTrustState::Unsigned
    } else {
        PlatformStartupTrustState::Invalid
    }
}

/// Distinguishes a missing local target from an unavailable filesystem boundary.
///
/// Remote paths are intentionally never classified as missing because a disconnected share is
/// indistinguishable from a deleted target without performing network I/O. Local drive roots must
/// also be available before absence can become evidence for destructive orphan cleanup.
pub(super) fn filesystem_target_state(path: &Path) -> FilesystemTargetState {
    if !path.is_absolute() || path.to_string_lossy().starts_with(r"\\") {
        return FilesystemTargetState::Unknown;
    }
    let Some(root) = path
        .ancestors()
        .last()
        .filter(|root| !root.as_os_str().is_empty())
    else {
        return FilesystemTargetState::Unknown;
    };
    if !matches!(root.try_exists(), Ok(true)) {
        return FilesystemTargetState::Unknown;
    }
    match path.try_exists() {
        Ok(true) => FilesystemTargetState::Present,
        Ok(false) => FilesystemTargetState::Missing,
        Err(_) => FilesystemTargetState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn missing_file_has_no_version_metadata() {
        assert!(file_version_metadata(Path::new(r"C:\missing\fixture.exe")).is_none());
    }

    #[test]
    fn remote_target_is_never_claimed_as_missing() {
        assert_eq!(
            filesystem_target_state(Path::new(r"\\unavailable\share\agent.exe")),
            FilesystemTargetState::Unknown
        );
    }

    #[test]
    #[ignore = "requires a Windows system image"]
    fn system_binary_has_version_metadata_and_valid_trust() {
        let system_root = std::env::var_os("SystemRoot").expect("SystemRoot must be available");
        let target = PathBuf::from(system_root).join(r"System32\kernel32.dll");
        let metadata =
            file_version_metadata(&target).expect("kernel32 must expose version metadata");
        assert!(metadata.product_version.is_some());
        assert_eq!(
            startup_trust(Some(&target), true),
            PlatformStartupTrustState::System
        );
    }
}
