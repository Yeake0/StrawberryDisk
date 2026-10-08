//! Machine-scoped uninstall capabilities for the shared authorization session.
//! The broker receives registration evidence, never a vendor command supplied over IPC.

use super::*;
use crate::elevation::{LaunchRequest, UninstallRequest};

pub(crate) fn launch_privileged_uninstaller(
    request: UninstallRequest,
) -> Result<Option<HANDLE>, ApplicationUninstallPlatformError> {
    let changed = ApplicationUninstallPlatformError::RegistrationChanged;
    let (executable, arguments) = match request {
        UninstallRequest::Msi { product_code } => {
            if !msi::valid_product_code(&product_code)
                || msi::registration_state(&product_code, ApplicationInstallScope::Machine)?
                    != ApplicationUninstallRegistrationState::Installed
            {
                return Err(changed);
            }
            (
                system_directory_path()?.join("msiexec.exe"),
                format!("/x {product_code} /qn /norestart"),
            )
        }
        UninstallRequest::Registered {
            key_name,
            registry32,
            digest,
        } => {
            let scope = ApplicationInstallScope::Machine;
            let view = if registry32 {
                WindowsRegistryView::Registry32
            } else {
                WindowsRegistryView::Registry64
            };
            let values =
                read_registered_uninstall_values(&key_name, scope, view)?.ok_or(changed)?;
            let (_, actual_digest) =
                registered_uninstall_command_evidence(&values.command, &key_name, scope)
                    .ok_or(changed)?;
            if actual_digest != digest {
                return Err(changed);
            }
            match validated_registered_command(&values.command, &key_name, scope).ok_or(changed)? {
                ValidatedRegisteredCommand::Executable {
                    executable,
                    arguments,
                }
                | ValidatedRegisteredCommand::BatchScript {
                    executable,
                    arguments,
                } => (executable, arguments),
                ValidatedRegisteredCommand::Rundll32(command) => {
                    (command.executable, command.arguments)
                }
                // Per-user package tools and scripts retain their original user token.
                _ => return Err(changed),
            }
        }
        UninstallRequest::Chocolatey {
            package_name,
            install_root,
            marker_digest,
            executable_digest,
        } => {
            let executable = [
                install_root.join("bin").join("choco.exe"),
                install_root.join("choco.exe"),
            ]
            .iter()
            .find_map(|path| {
                crate::command::ControlledExecutable::capture(path)
                    .ok()
                    .filter(|captured| identity_digest(captured) == executable_digest)
            })
            .ok_or(changed)?;
            if chocolatey_package_state(&package_name, &install_root, &marker_digest, &executable)?
                != ApplicationUninstallRegistrationState::Installed
            {
                return Err(changed);
            }
            (
                executable.validated_path().map_err(|_| changed)?.to_owned(),
                format!("uninstall {package_name} --yes --no-progress --limit-output"),
            )
        }
    };
    let executable = wide_path(&executable);
    let arguments = wide_string(&arguments);
    let mut execution = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpFile: executable.as_ptr(),
        lpParameters: arguments.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // This process is already elevated. The default verb preserves native batch handlers
    // and vendor UI without a second authorization request for the launched process.
    if unsafe { ShellExecuteExW(&mut execution) } == 0 {
        return Err(elevation_request_error(unsafe { GetLastError() }));
    }
    // Shell can delegate successfully without returning a process handle. That is
    // an uncertain dispatch, not a pre-launch rejection which is safe to retry.
    Ok((!execution.hProcess.is_null()).then_some(execution.hProcess))
}

pub(super) fn identity_digest(executable: &crate::command::ControlledExecutable) -> String {
    // The private capture contains the canonical path, volume/file ID, size and timestamps.
    // Only its digest crosses this protocol; ControlledExecutable remains non-deserializable.
    blake3::hash(format!("{executable:?}").as_bytes())
        .to_hex()
        .to_string()
}

pub(super) fn execute(
    request: UninstallRequest,
    executor_kind: &'static str,
) -> Result<u32, ApplicationUninstallPlatformError> {
    let started = Instant::now();
    let handle = crate::elevation::launch(LaunchRequest::Uninstall(request)).map_err(|error| {
        if error.code == ERROR_CANCELLED && !error.may_have_started {
            log::info!("application_uninstall_authorized_launch_cancelled executor_kind={executor_kind} native_code={}", error.code);
        } else {
            log::warn!("application_uninstall_authorized_launch_failed executor_kind={executor_kind} native_code={} may_have_started={}", error.code, error.may_have_started);
        }
        map_launch_error(error)
    })?;
    let handle = OwnedHandle(handle);
    let result = wait_for_native_process_tree(handle.0, unsafe { GetProcessId(handle.0) });
    log::info!("application_uninstall_authorized_process_finished executor_kind={executor_kind} exit_code={:?} platform_error={} elapsed_ms={}",
        result.as_ref().ok(), result.as_ref().err().map_or("none", |e| e.stable_code()), started.elapsed().as_millis());
    result
}

fn map_launch_error(error: crate::elevation::LaunchError) -> ApplicationUninstallPlatformError {
    // Transport loss cannot prove whether the vendor finished, failed, or is still
    // running. Preserve that distinction through Core, history, and the UI.
    if error.may_have_started {
        ApplicationUninstallPlatformError::LaunchUnconfirmed(error.code)
    } else if error.reason == crate::elevation::RejectionReason::ItemChanged {
        ApplicationUninstallPlatformError::RegistrationChanged
    } else {
        elevation_request_error(error.code)
    }
}

pub(super) fn registered_request(
    key_name: &str,
    view: WindowsRegistryView,
    digest: &str,
) -> UninstallRequest {
    UninstallRequest::Registered {
        key_name: key_name.to_owned(),
        registry32: view == WindowsRegistryView::Registry32,
        digest: digest.to_owned(),
    }
}

/// CreateProcess honors the manifest and installer-detection policy but returns 740 instead
/// of presenting UAC itself. Only that pre-launch error enters the shared elevated session;
/// a started vendor process is never retried. Ordinary EXEs keep the caller's user context.
pub(super) fn execute_machine_executable(
    executable: &Path,
    arguments: &str,
    request: UninstallRequest,
) -> Result<u32, ApplicationUninstallPlatformError> {
    let mut command_line = wide_string(&format!(
        "{} {arguments}",
        quote_windows_argument(&executable.to_string_lossy())
    ));
    let executable = wide_path(executable);
    // Vendor EXEs may be interactive console programs. Do not apply our background
    // command policy or redirect their standard handles: preserve Windows' normal
    // console/UI initialization while preventing automatic Shell elevation.
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    if unsafe {
        windows_sys::Win32::System::Threading::CreateProcessW(
            executable.as_ptr(),
            command_line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            0,
            ptr::null(),
            ptr::null(),
            &startup,
            &mut process,
        )
    } == 0
    {
        let code = unsafe { GetLastError() };
        return if code == 740 {
            log::info!("application_uninstall_launch_policy executor_kind=windows_registered authorization=required reason=windows_elevation_required");
            execute(request, EXECUTOR_REGISTERED)
        } else {
            Err(ApplicationUninstallPlatformError::NativeFailure(code))
        };
    }
    let handle = OwnedHandle(process.hProcess);
    let _thread = OwnedHandle(process.hThread);
    log::info!("application_uninstall_launch_policy executor_kind=windows_registered authorization=not_required");
    wait_for_native_process_tree(handle.0, process.dwProcessId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lost_launch_response_does_not_claim_the_uninstaller_failed() {
        // Inject the exact error emitted after a broker loses a launch response.
        // The vendor process may still be running; this is not a vendor failure.
        let error = map_launch_error(crate::elevation::LaunchError {
            code: windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE,
            may_have_started: true,
            reason: crate::elevation::RejectionReason::Native,
        });
        assert_eq!(error.stable_code(), "launch_unconfirmed");
        assert_eq!(error.native_code(), Some(109));
    }

    #[test]
    fn confirmed_launch_rejections_preserve_cancellation_and_changed_evidence() {
        for (code, reason, expected) in [
            (
                ERROR_CANCELLED,
                crate::elevation::RejectionReason::Native,
                ApplicationUninstallPlatformError::UserCancelled,
            ),
            (
                13,
                crate::elevation::RejectionReason::ItemChanged,
                ApplicationUninstallPlatformError::RegistrationChanged,
            ),
            (
                5,
                crate::elevation::RejectionReason::Native,
                ApplicationUninstallPlatformError::NativeFailure(5),
            ),
        ] {
            assert_eq!(
                map_launch_error(crate::elevation::LaunchError {
                    code,
                    may_have_started: false,
                    reason
                }),
                expected
            );
        }
    }

    #[test]
    fn privileged_uninstall_rejects_unregistered_or_injected_targets() {
        for request in [
            UninstallRequest::Msi {
                product_code: "{9627E855-337D-45EC-A2D9-CBB92B447399} /i arbitrary.msi".into(),
            },
            UninstallRequest::Registered {
                key_name: "StrawberryDisk-Missing-Uac-Boundary-Fixture".into(),
                registry32: false,
                digest: "a".repeat(64),
            },
        ] {
            assert_eq!(
                launch_privileged_uninstaller(request).unwrap_err(),
                ApplicationUninstallPlatformError::RegistrationChanged
            );
        }
    }
}
