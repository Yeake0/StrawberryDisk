use std::{path::Path, time::Duration};

use crate::{
    run_controlled_command, ControlledCommandError, ControlledCommandLimits,
    ControlledEnvironmentPolicy, ControlledExecutable, PlatformCancellation, PlatformError,
    PlatformErrorCode, PlatformResult, PlatformSystemMaintenanceCompletion,
    PlatformSystemMaintenanceDiagnosticCode, PlatformSystemMaintenanceExecution,
    PlatformSystemMaintenancePhase, PlatformSystemMaintenanceProgress,
    PlatformSystemMaintenanceProgressSink, PlatformSystemMaintenanceState,
    PlatformSystemMaintenanceStatus,
};

const SPOTLIGHT: &str = "macos.maintenance.spotlight-index";
const LAUNCH_SERVICES: &str = "macos.maintenance.launch-services";
const QUICKLOOK_CACHE: &str = "macos.maintenance.quicklook-cache";
const ICON_CACHE: &str = "macos.maintenance.icon-cache";
const FINDER_SERVICE: &str = "macos.maintenance.finder-service";
const AUDIO_SERVICE: &str = "macos.maintenance.audio-service";
const USER_PERMISSIONS: &str = "macos.maintenance.user-permissions";
const LEGACY_OVERRIDES: &str = "macos.maintenance.legacy-overrides";
const STARTUP_DISK: &str = "macos.maintenance.startup-disk";
const DNS_CACHE: &str = "macos.maintenance.dns-cache";
const ADMIN_SUCCESS_RESPONSE: &str = "mangodisk-maintenance-v1:success";
const ADMIN_ERROR_RESPONSE_PREFIX: &str = "mangodisk-maintenance-v1:error:";
const USER_CANCELLED_APPLESCRIPT_ERROR: i32 = -128;

const SUPPORTED_TASKS: &[&str] = &[
    SPOTLIGHT,
    LAUNCH_SERVICES,
    QUICKLOOK_CACHE,
    ICON_CACHE,
    FINDER_SERVICE,
    AUDIO_SERVICE,
    USER_PERMISSIONS,
    LEGACY_OVERRIDES,
    STARTUP_DISK,
    DNS_CACHE,
];

const DEFAULT_LIMITS: ControlledCommandLimits = ControlledCommandLimits {
    timeout: Duration::from_secs(30),
    stdout_bytes: 64 * 1024,
    stderr_bytes: 64 * 1024,
};
const LONG_LIMITS: ControlledCommandLimits = ControlledCommandLimits {
    timeout: Duration::from_secs(180),
    stdout_bytes: 64 * 1024,
    stderr_bytes: 64 * 1024,
};
const DISK_CHECK_LIMITS: ControlledCommandLimits = ControlledCommandLimits {
    timeout: Duration::from_secs(600),
    stdout_bytes: 256 * 1024,
    stderr_bytes: 64 * 1024,
};

const ADMIN_SCRIPT: &str = r#"on run argv
set maintenanceCommand to item 1 of argv
set promptText to item 2 of argv
try
    if promptText is "" then
        do shell script maintenanceCommand with administrator privileges
    else
        do shell script maintenanceCommand with prompt promptText with administrator privileges
    end if
    return "mangodisk-maintenance-v1:success"
on error errorMessage number errorNumber
    return "mangodisk-maintenance-v1:error:" & errorNumber
end try
end run"#;

pub(crate) fn scan(
    task_ids: &[&str],
    cancellation: &PlatformCancellation,
) -> PlatformResult<Vec<PlatformSystemMaintenanceState>> {
    validate_ids(task_ids)?;
    let mut states = Vec::with_capacity(task_ids.len());
    for task_id in task_ids {
        if cancellation.is_cancelled() {
            return Err(PlatformError::operation_failed(
                "system maintenance scan was cancelled",
            ));
        }
        states.push(match *task_id {
            SPOTLIGHT => scan_spotlight(cancellation),
            USER_PERMISSIONS => availability_state(
                USER_PERMISSIONS,
                Path::new("/usr/sbin/diskutil").is_file(),
                true,
            ),
            LEGACY_OVERRIDES => scan_legacy_overrides(cancellation),
            LAUNCH_SERVICES => {
                availability_state(LAUNCH_SERVICES, launch_services_path().is_some(), false)
            }
            QUICKLOOK_CACHE => availability_state(
                QUICKLOOK_CACHE,
                Path::new("/usr/bin/qlmanage").is_file(),
                false,
            ),
            ICON_CACHE => {
                availability_state(ICON_CACHE, Path::new("/usr/bin/qlmanage").is_file(), false)
            }
            FINDER_SERVICE => availability_state(
                FINDER_SERVICE,
                Path::new("/usr/bin/killall").is_file() && Path::new("/usr/bin/pgrep").is_file(),
                false,
            ),
            AUDIO_SERVICE => match process_running("coreaudiod", cancellation) {
                Ok(true) => available(AUDIO_SERVICE, true),
                Ok(false) => unavailable(
                    AUDIO_SERVICE,
                    true,
                    PlatformSystemMaintenanceDiagnosticCode::ComponentUnavailable,
                ),
                Err(_) => unavailable(
                    AUDIO_SERVICE,
                    true,
                    PlatformSystemMaintenanceDiagnosticCode::CheckFailed,
                ),
            },
            STARTUP_DISK => availability_state(
                STARTUP_DISK,
                Path::new("/usr/sbin/diskutil").is_file(),
                false,
            ),
            DNS_CACHE => availability_state(
                DNS_CACHE,
                Path::new("/usr/bin/dscacheutil").is_file()
                    && Path::new("/usr/bin/killall").is_file(),
                true,
            ),
            _ => unreachable!("validated maintenance identifier"),
        });
    }
    Ok(states)
}

pub(crate) fn execute(
    task_id: &str,
    cancellation: &PlatformCancellation,
    authorization_prompt: Option<&str>,
    progress: &PlatformSystemMaintenanceProgressSink,
) -> PlatformResult<PlatformSystemMaintenanceExecution> {
    validate_ids(&[task_id])?;
    match task_id {
        SPOTLIGHT => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RebuildingSearchIndex,
            ));
            run_administrator_command(
                "/usr/bin/mdutil -E /",
                authorization_prompt,
                cancellation,
                LONG_LIMITS,
            )?;
            Ok(execution(task_id, true, true, false, true))
        }
        LAUNCH_SERVICES => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RebuildingAppAssociations,
            ));
            let path = launch_services_path().ok_or_else(tool_unavailable)?;
            run_mutating(path, &["-gc"], cancellation, DEFAULT_LIMITS)?;
            run_mutating(
                path,
                &["-r", "-f", "-domain", "local", "-domain", "user"],
                cancellation,
                LONG_LIMITS,
            )
            .map_err(PlatformError::with_possible_side_effects)?;
            Ok(execution(task_id, true, true, false, false))
        }
        QUICKLOOK_CACHE => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RefreshingShellCaches,
            ));
            run_mutating(
                "/usr/bin/qlmanage",
                &["-r", "cache"],
                cancellation,
                DEFAULT_LIMITS,
            )?;
            run_mutating("/usr/bin/qlmanage", &["-r"], cancellation, DEFAULT_LIMITS)
                .map_err(PlatformError::with_possible_side_effects)?;
            Ok(execution(task_id, true, true, false, false))
        }
        ICON_CACHE => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RefreshingShellCaches,
            ));
            run_mutating(
                "/usr/bin/qlmanage",
                &["-r", "cache"],
                cancellation,
                DEFAULT_LIMITS,
            )?;
            // iconservicesagent is demand-launched. Skip it only when a readback confirms that no
            // process exists; swallowing every killall failure would otherwise report success for
            // permission, timeout, or executable-integrity errors after qlmanage already changed
            // cache state.
            let icon_service_running = process_running("iconservicesagent", cancellation)
                .map_err(PlatformError::with_possible_side_effects)?;
            if icon_service_running {
                run_mutating(
                    "/usr/bin/killall",
                    &["iconservicesagent"],
                    cancellation,
                    DEFAULT_LIMITS,
                )
                .map_err(PlatformError::with_possible_side_effects)?;
            }
            Ok(execution(task_id, true, true, false, false))
        }
        FINDER_SERVICE => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RestartingFinder,
            ));
            restart_user_process("Finder", cancellation)?;
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::Verifying,
            ));
            let verified = wait_for_process("Finder", cancellation)
                .map_err(PlatformError::with_possible_side_effects)?;
            Ok(execution(task_id, true, verified, false, false))
        }
        AUDIO_SERVICE => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RestartingAudioService,
            ));
            run_administrator_command(
                "/usr/bin/killall coreaudiod",
                authorization_prompt,
                cancellation,
                LONG_LIMITS,
            )?;
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::Verifying,
            ));
            let verified = wait_for_process("coreaudiod", cancellation)
                .map_err(PlatformError::with_possible_side_effects)?;
            Ok(execution(task_id, true, verified, false, false))
        }
        USER_PERMISSIONS => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RepairingPermissions,
            ));
            let uid = unsafe { libc::getuid() };
            run_administrator_command(
                &format!("/usr/sbin/diskutil resetUserPermissions / {uid}"),
                authorization_prompt,
                cancellation,
                LONG_LIMITS,
            )?;
            // `diskutil resetUserPermissions` performs the authoritative recursive repair. A
            // shallow metadata check cannot verify ACLs throughout the user tree and previously
            // hid this task when only a nested folder was broken, so a successful native command
            // is the stable completion signal.
            Ok(execution(task_id, true, true, false, false))
        }
        LEGACY_OVERRIDES => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RestoringDefaults,
            ));
            let mut changed = delete_default("NSGlobalDomain", "NSAppSleepDisabled", cancellation)?;
            for key in ["skip-verify", "skip-verify-locked", "skip-verify-remote"] {
                match delete_default("com.apple.frameworks.diskimages", key, cancellation) {
                    Ok(deleted) => changed |= deleted,
                    Err(error) if changed => return Err(error.with_possible_side_effects()),
                    Err(error) => return Err(error),
                }
            }
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::Verifying,
            ));
            let verified = !legacy_overrides_present(cancellation)
                .map_err(PlatformError::with_possible_side_effects)?;
            Ok(execution(task_id, true, verified, false, false))
        }
        STARTUP_DISK => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::CheckingStartupDisk,
            ));
            run(
                "/usr/sbin/diskutil",
                &["verifyVolume", "/"],
                cancellation,
                DISK_CHECK_LIMITS,
            )?;
            Ok(execution(task_id, false, true, false, false))
        }
        DNS_CACHE => {
            progress(PlatformSystemMaintenanceProgress::phase(
                PlatformSystemMaintenancePhase::RefreshingNetwork,
            ));
            run_administrator_command(
                "/usr/bin/dscacheutil -flushcache; /usr/bin/killall -HUP mDNSResponder",
                authorization_prompt,
                cancellation,
                LONG_LIMITS,
            )?;
            Ok(execution(task_id, true, true, false, false))
        }
        _ => unreachable!("validated maintenance identifier"),
    }
}

fn restart_user_process(
    process_name: &str,
    cancellation: &PlatformCancellation,
) -> PlatformResult<()> {
    if !process_running(process_name, cancellation)? {
        return Ok(());
    }
    run_mutating(
        "/usr/bin/killall",
        &[process_name],
        cancellation,
        DEFAULT_LIMITS,
    )?;
    Ok(())
}

fn wait_for_process(
    process_name: &str,
    cancellation: &PlatformCancellation,
) -> PlatformResult<bool> {
    for _ in 0..20 {
        if cancellation.is_cancelled() {
            return Err(PlatformError::new(
                PlatformErrorCode::UserCancelled,
                "system maintenance verification was cancelled",
            ));
        }
        if process_running(process_name, cancellation)? {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(false)
}

fn process_running(
    process_name: &str,
    cancellation: &PlatformCancellation,
) -> PlatformResult<bool> {
    let executable =
        ControlledExecutable::capture(Path::new("/usr/bin/pgrep")).map_err(command_error)?;
    let output = run_controlled_command(
        "macos_maintenance_process_query",
        &executable,
        &["-x", process_name],
        ControlledEnvironmentPolicy::Inherit,
        DEFAULT_LIMITS,
        &|| cancellation.is_cancelled(),
    )
    .map_err(command_error)?;
    match output.status.code() {
        Some(0) => Ok(true),
        // pgrep documents exit status 1 as a successful query with no matching processes. Keep
        // every other status distinct so a timeout, query failure, or invalid invocation cannot
        // masquerade as a verified absence after a maintenance task changed system state.
        Some(1) => Ok(false),
        _ => Err(PlatformError::operation_failed(
            "system maintenance process query failed",
        )),
    }
}

fn scan_spotlight(cancellation: &PlatformCancellation) -> PlatformSystemMaintenanceState {
    if !Path::new("/usr/bin/mdutil").is_file() {
        return unavailable(
            SPOTLIGHT,
            false,
            PlatformSystemMaintenanceDiagnosticCode::ToolUnavailable,
        );
    }
    match run(
        "/usr/bin/mdutil",
        &["-s", "/"],
        cancellation,
        DEFAULT_LIMITS,
    ) {
        Ok(output) if String::from_utf8_lossy(&output).contains("Indexing enabled") => {
            available(SPOTLIGHT, true)
        }
        Ok(_) => unavailable(
            SPOTLIGHT,
            true,
            PlatformSystemMaintenanceDiagnosticCode::ComponentUnavailable,
        ),
        Err(_) => unavailable(
            SPOTLIGHT,
            true,
            PlatformSystemMaintenanceDiagnosticCode::CheckFailed,
        ),
    }
}

fn scan_legacy_overrides(cancellation: &PlatformCancellation) -> PlatformSystemMaintenanceState {
    match legacy_overrides_present(cancellation) {
        Ok(true) => recommended(LEGACY_OVERRIDES, false),
        Ok(false) => healthy(LEGACY_OVERRIDES, false),
        Err(_) => unavailable(
            LEGACY_OVERRIDES,
            false,
            PlatformSystemMaintenanceDiagnosticCode::CheckFailed,
        ),
    }
}

fn legacy_overrides_present(cancellation: &PlatformCancellation) -> PlatformResult<bool> {
    if default_exists("NSGlobalDomain", "NSAppSleepDisabled", cancellation)? {
        return Ok(true);
    }
    for key in ["skip-verify", "skip-verify-locked", "skip-verify-remote"] {
        if default_exists("com.apple.frameworks.diskimages", key, cancellation)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn default_exists(
    domain: &str,
    key: &str,
    cancellation: &PlatformCancellation,
) -> PlatformResult<bool> {
    match run(
        "/usr/bin/defaults",
        &["read", domain, key],
        cancellation,
        DEFAULT_LIMITS,
    ) {
        Ok(_) => Ok(true),
        Err(error) if error.code() == PlatformErrorCode::OperationFailed => Ok(false),
        Err(error) => Err(error),
    }
}

fn delete_default(
    domain: &str,
    key: &str,
    cancellation: &PlatformCancellation,
) -> PlatformResult<bool> {
    if !default_exists(domain, key, cancellation)? {
        return Ok(false);
    }
    run_mutating(
        "/usr/bin/defaults",
        &["delete", domain, key],
        cancellation,
        DEFAULT_LIMITS,
    )?;
    Ok(true)
}

fn run_administrator_command(
    command: &str,
    authorization_prompt: Option<&str>,
    cancellation: &PlatformCancellation,
    limits: ControlledCommandLimits,
) -> PlatformResult<()> {
    let prompt = authorization_prompt.unwrap_or(
        "StrawberryDisk needs administrator approval to perform the selected system maintenance.",
    );
    if prompt.contains(['\n', '\r']) || prompt.len() > 240 {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "system maintenance authorization prompt is invalid",
        ));
    }
    let output = run_mutating(
        "/usr/bin/osascript",
        &["-e", ADMIN_SCRIPT, command, prompt],
        cancellation,
        limits,
    )?;
    match parse_administrator_response(&String::from_utf8_lossy(&output)) {
        AdministratorResponse::Completed => Ok(()),
        AdministratorResponse::UserCancelled => Err(PlatformError::new(
            PlatformErrorCode::UserCancelled,
            "system maintenance authorization was cancelled",
        )),
        AdministratorResponse::Failed(error_number) => {
            log::warn!(
                "macos_system_maintenance_authorization_failed error_number={}",
                error_number
                    .map(|number| number.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
            Err(
                PlatformError::operation_failed("system maintenance administrator command failed")
                    .with_possible_side_effects(),
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdministratorResponse {
    Completed,
    UserCancelled,
    Failed(Option<i32>),
}

fn parse_administrator_response(output: &str) -> AdministratorResponse {
    let response = output.trim();
    if response == ADMIN_SUCCESS_RESPONSE {
        return AdministratorResponse::Completed;
    }
    let error_number = response
        .strip_prefix(ADMIN_ERROR_RESPONSE_PREFIX)
        .and_then(|value| value.parse::<i32>().ok());
    match error_number {
        Some(USER_CANCELLED_APPLESCRIPT_ERROR) => AdministratorResponse::UserCancelled,
        value => AdministratorResponse::Failed(value),
    }
}

fn run_mutating(
    path: &str,
    arguments: &[&str],
    cancellation: &PlatformCancellation,
    limits: ControlledCommandLimits,
) -> PlatformResult<Vec<u8>> {
    run_with_effect(
        path,
        arguments,
        cancellation,
        limits,
        CommandEffect::MayMutate,
    )
}

fn run(
    path: &str,
    arguments: &[&str],
    cancellation: &PlatformCancellation,
    limits: ControlledCommandLimits,
) -> PlatformResult<Vec<u8>> {
    run_with_effect(
        path,
        arguments,
        cancellation,
        limits,
        CommandEffect::ReadOnly,
    )
}

fn run_with_effect(
    path: &str,
    arguments: &[&str],
    cancellation: &PlatformCancellation,
    limits: ControlledCommandLimits,
    effect: CommandEffect,
) -> PlatformResult<Vec<u8>> {
    let executable = ControlledExecutable::capture(Path::new(path)).map_err(command_error)?;
    let output = run_controlled_command(
        maintenance_command_id(path),
        &executable,
        arguments,
        ControlledEnvironmentPolicy::Inherit,
        limits,
        &|| cancellation.is_cancelled(),
    )
    .map_err(|error| command_execution_error(error, effect))?;
    if !output.status.success() {
        let error = PlatformError::operation_failed("system maintenance command failed");
        return Err(match effect {
            CommandEffect::ReadOnly => error,
            CommandEffect::MayMutate => error.with_possible_side_effects(),
        });
    }
    Ok(output.stdout)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandEffect {
    ReadOnly,
    MayMutate,
}

fn command_execution_error(error: ControlledCommandError, effect: CommandEffect) -> PlatformError {
    // Capture and spawn failures prove that the native tool never ran. Later failures are
    // uncertain for mutating commands because useful work may precede a timeout, cancellation, or
    // output-reader failure.
    let process_never_started = matches!(
        error,
        ControlledCommandError::InvalidExecutable
            | ControlledCommandError::ExecutableChanged
            | ControlledCommandError::SpawnFailed
    );
    let error = command_error(error);
    if effect == CommandEffect::MayMutate && !process_never_started {
        error.with_possible_side_effects()
    } else {
        error
    }
}

fn maintenance_command_id(path: &str) -> &'static str {
    match path {
        "/usr/bin/mdutil" => "macos_maintenance_mdutil",
        "/usr/bin/qlmanage" => "macos_maintenance_qlmanage",
        "/usr/bin/killall" => "macos_maintenance_killall",
        "/usr/bin/pgrep" => "macos_maintenance_process_query",
        "/usr/sbin/diskutil" => "macos_maintenance_disk_check",
        "/usr/bin/defaults" => "macos_maintenance_defaults",
        "/usr/bin/osascript" => "macos_maintenance_authorization",
        _ => "macos_maintenance_launch_services",
    }
}

fn command_error(error: ControlledCommandError) -> PlatformError {
    PlatformError::new(
        match error {
            ControlledCommandError::Cancelled => PlatformErrorCode::UserCancelled,
            ControlledCommandError::InvalidExecutable
            | ControlledCommandError::ExecutableChanged => PlatformErrorCode::Unsupported,
            ControlledCommandError::SpawnFailed
            | ControlledCommandError::ReaderFailed
            | ControlledCommandError::WaitFailed
            | ControlledCommandError::TimedOut
            | ControlledCommandError::OutputLimitExceeded => PlatformErrorCode::OperationFailed,
        },
        "system maintenance command could not complete",
    )
}

fn launch_services_path() -> Option<&'static str> {
    [
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
        "/System/Library/Frameworks/ApplicationServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
    ]
    .into_iter()
    .find(|path| Path::new(path).is_file())
}

fn validate_ids(task_ids: &[&str]) -> PlatformResult<()> {
    if task_ids.is_empty()
        || task_ids
            .iter()
            .any(|task_id| !SUPPORTED_TASKS.contains(task_id))
    {
        return Err(PlatformError::new(
            PlatformErrorCode::Unsupported,
            "system maintenance task identifier is unsupported",
        ));
    }
    Ok(())
}

fn availability_state(
    task_id: &str,
    is_available: bool,
    requires_elevation: bool,
) -> PlatformSystemMaintenanceState {
    if is_available {
        available(task_id, requires_elevation)
    } else {
        unavailable(
            task_id,
            requires_elevation,
            PlatformSystemMaintenanceDiagnosticCode::ToolUnavailable,
        )
    }
}

fn healthy(task_id: &str, requires_elevation: bool) -> PlatformSystemMaintenanceState {
    state(
        task_id,
        PlatformSystemMaintenanceStatus::Healthy,
        requires_elevation,
        None,
    )
}

fn recommended(task_id: &str, requires_elevation: bool) -> PlatformSystemMaintenanceState {
    state(
        task_id,
        PlatformSystemMaintenanceStatus::Recommended,
        requires_elevation,
        None,
    )
}

fn available(task_id: &str, requires_elevation: bool) -> PlatformSystemMaintenanceState {
    state(
        task_id,
        PlatformSystemMaintenanceStatus::Available,
        requires_elevation,
        None,
    )
}

fn unavailable(
    task_id: &str,
    requires_elevation: bool,
    diagnostic: PlatformSystemMaintenanceDiagnosticCode,
) -> PlatformSystemMaintenanceState {
    state(
        task_id,
        PlatformSystemMaintenanceStatus::Unavailable,
        requires_elevation,
        Some(diagnostic),
    )
}

fn state(
    task_id: &str,
    status: PlatformSystemMaintenanceStatus,
    requires_elevation: bool,
    diagnostic: Option<PlatformSystemMaintenanceDiagnosticCode>,
) -> PlatformSystemMaintenanceState {
    PlatformSystemMaintenanceState {
        task_id: task_id.to_string(),
        status,
        requires_elevation,
        diagnostic,
    }
}

fn execution(
    task_id: &str,
    changed: bool,
    verified: bool,
    requires_restart: bool,
    started: bool,
) -> PlatformSystemMaintenanceExecution {
    PlatformSystemMaintenanceExecution {
        task_id: task_id.to_string(),
        changed,
        verified,
        requires_restart,
        completion: if started {
            PlatformSystemMaintenanceCompletion::Started
        } else {
            PlatformSystemMaintenanceCompletion::Completed
        },
    }
}

fn tool_unavailable() -> PlatformError {
    PlatformError::new(
        PlatformErrorCode::Unsupported,
        "system maintenance tool is unavailable",
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::*;

    #[test]
    fn unknown_task_identifiers_are_rejected() {
        assert!(validate_ids(&["macos.maintenance.unknown"]).is_err());
    }

    #[test]
    fn administrator_prompt_rejects_multiline_text() {
        let cancellation = PlatformCancellation::new(|| false);
        let error = run_administrator_command(
            "/usr/bin/true",
            Some("line one\nline two"),
            &cancellation,
            DEFAULT_LIMITS,
        )
        .expect_err("multiline authorization text must be rejected");
        assert_eq!(error.code(), PlatformErrorCode::InvalidData);
    }

    #[test]
    fn administrator_response_distinguishes_cancellation_and_failure() {
        assert_eq!(
            parse_administrator_response(ADMIN_SUCCESS_RESPONSE),
            AdministratorResponse::Completed
        );
        assert_eq!(
            parse_administrator_response("mangodisk-maintenance-v1:error:-128\n"),
            AdministratorResponse::UserCancelled
        );
        assert_eq!(
            parse_administrator_response("mangodisk-maintenance-v1:error:1"),
            AdministratorResponse::Failed(Some(1))
        );
        assert_eq!(
            parse_administrator_response("unexpected response"),
            AdministratorResponse::Failed(None)
        );
    }

    #[test]
    fn mutating_command_spawn_failure_is_known_to_leave_system_unchanged() {
        let spawn_error = command_execution_error(
            ControlledCommandError::SpawnFailed,
            CommandEffect::MayMutate,
        );
        assert_eq!(
            spawn_error.mutation_state(),
            crate::PlatformMutationState::NotAttempted
        );

        let timeout_error =
            command_execution_error(ControlledCommandError::TimedOut, CommandEffect::MayMutate);
        assert_eq!(
            timeout_error.mutation_state(),
            crate::PlatformMutationState::MayHaveChanged
        );
    }

    #[test]
    fn process_query_distinguishes_a_match_from_a_verified_absence() {
        let cancellation = PlatformCancellation::new(|| false);
        let mut child = Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .expect("the process-query fixture must start");
        let running = process_running("sleep", &cancellation);
        let _ = child.kill();
        let _ = child.wait();

        assert!(running.expect("the spawned process must remain queryable"));
        assert!(!process_running("md-no-such-proc", &cancellation)
            .expect("an absent process must remain a successful read-only query"));
    }

    #[test]
    fn administrator_script_compiles_with_macos_tool() {
        let compiled_script = std::env::temp_dir().join(format!(
            "strawberrydisk-maintenance-authorization-{}.scpt",
            std::process::id()
        ));
        let output = Command::new("/usr/bin/osacompile")
            .arg("-e")
            .arg(ADMIN_SCRIPT)
            .arg("-o")
            .arg(&compiled_script)
            .output()
            .expect("launch the macOS AppleScript compiler");
        let diagnostic = String::from_utf8_lossy(&output.stderr).into_owned();
        let _ = fs::remove_file(compiled_script);

        assert!(
            output.status.success(),
            "administrator script should compile: {diagnostic}"
        );
    }

    /// Runs one real maintenance task on an explicitly authorized macOS host.
    ///
    /// Each invocation is limited to the stable catalog identifier supplied by the test runner.
    /// Keeping the test ignored prevents ordinary builds from rebuilding caches, indexes, or
    /// system associations without an operator choosing the exact task first.
    #[test]
    #[ignore = "changes macOS system state; run one authorized task at a time"]
    fn actual_maintenance_task_executes_and_verifies() {
        let task_id = std::env::var("STRAWBERRYDISK_MAINTENANCE_TASK")
            .expect("STRAWBERRYDISK_MAINTENANCE_TASK must name one supported maintenance task");
        assert!(SUPPORTED_TASKS.contains(&task_id.as_str()));

        let cancellation = PlatformCancellation::new(|| false);
        let state = scan(&[task_id.as_str()], &cancellation)
            .expect("the maintenance task must be scannable on this macOS host")
            .into_iter()
            .next()
            .expect("the platform must return the requested maintenance task");
        assert_ne!(state.status, PlatformSystemMaintenanceStatus::Unavailable);

        let observed_progress = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let progress_output = std::sync::Arc::clone(&observed_progress);
        let progress_sink = move |progress: PlatformSystemMaintenanceProgress| {
            progress_output
                .lock()
                .expect("progress test mutex must remain available")
                .push(progress);
        };
        let outcome = execute(&task_id, &cancellation, None, &progress_sink)
            .expect("the authorized maintenance task must execute successfully");
        assert_eq!(outcome.task_id, task_id);
        assert_eq!(outcome.changed, task_id != STARTUP_DISK);
        assert!(outcome.verified);
        assert!(
            !observed_progress
                .lock()
                .expect("progress test mutex must remain available")
                .is_empty(),
            "every maintenance task must report at least one useful phase"
        );
    }
}
