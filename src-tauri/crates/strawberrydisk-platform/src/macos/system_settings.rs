use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{
    preflight_system_setting_change, run_controlled_command_with_log_policy,
    ControlledCommandError, ControlledCommandLimits, ControlledCommandLogPolicy,
    ControlledCommandOutput, ControlledEnvironmentPolicy, ControlledExecutable,
    PlatformCancellation, PlatformError, PlatformErrorCode, PlatformResult,
    PlatformSystemSettingChangeRequest, PlatformSystemSettingChangeResult,
    PlatformSystemSettingDiagnosticCode, PlatformSystemSettingState, PlatformSystemSettingValue,
};

const DEFAULTS_LIMITS: ControlledCommandLimits = ControlledCommandLimits {
    timeout: Duration::from_secs(2),
    stdout_bytes: 64 * 1024,
    stderr_bytes: 16 * 1024,
};
const SETTINGS_SCAN_DEADLINE: Duration = Duration::from_secs(6);

#[derive(Clone, Copy)]
enum ValueKind {
    Boolean,
    Integer,
    Text,
}

#[derive(Clone, Copy)]
struct SettingDefinition {
    id: &'static str,
    domain: &'static str,
    key: &'static str,
    kind: ValueKind,
}

const SETTINGS: &[SettingDefinition] = &[
    setting(
        "macos.finder.show-file-extensions",
        "NSGlobalDomain",
        "AppleShowAllExtensions",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.show-path-bar",
        "com.apple.finder",
        "ShowPathbar",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.show-status-bar",
        "com.apple.finder",
        "ShowStatusBar",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.show-hidden-files",
        "com.apple.finder",
        "AppleShowAllFiles",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.show-posix-path",
        "com.apple.finder",
        "_FXShowPosixPathInTitle",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.disable-animations",
        "com.apple.finder",
        "DisableAllAnimations",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.keep-folders-on-top",
        "com.apple.finder",
        "_FXSortFoldersFirst",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.search-current-folder",
        "com.apple.finder",
        "FXDefaultSearchScope",
        ValueKind::Text,
    ),
    setting(
        "macos.finder.disable-extension-warning",
        "com.apple.finder",
        "FXEnableExtensionChangeWarning",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.show-hard-drives-on-desktop",
        "com.apple.finder",
        "ShowHardDrivesOnDesktop",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.show-external-drives-on-desktop",
        "com.apple.finder",
        "ShowExternalHardDrivesOnDesktop",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.show-removable-media-on-desktop",
        "com.apple.finder",
        "ShowRemovableMediaOnDesktop",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.default-list-view",
        "com.apple.finder",
        "FXPreferredViewStyle",
        ValueKind::Text,
    ),
    setting(
        "macos.finder.folders-first-on-desktop",
        "com.apple.finder",
        "_FXSortFoldersFirstOnDesktop",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.enable-quit-menu",
        "com.apple.finder",
        "QuitMenuItem",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.remove-old-trash-items",
        "com.apple.finder",
        "FXRemoveOldTrashItems",
        ValueKind::Boolean,
    ),
    setting(
        "macos.panels.expand-save",
        "NSGlobalDomain",
        "NSNavPanelExpandedStateForSaveMode",
        ValueKind::Boolean,
    ),
    setting(
        "macos.panels.expand-print",
        "NSGlobalDomain",
        "PMPrintingExpandedStateForPrint",
        ValueKind::Boolean,
    ),
    setting(
        "macos.desktop.prevent-network-ds-store",
        "com.apple.desktopservices",
        "DSDontWriteNetworkStores",
        ValueKind::Boolean,
    ),
    setting(
        "macos.desktop.prevent-usb-ds-store",
        "com.apple.desktopservices",
        "DSDontWriteUSBStores",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.auto-hide",
        "com.apple.dock",
        "autohide",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.minimize-to-application",
        "com.apple.dock",
        "minimize-to-application",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.use-scale-effect",
        "com.apple.dock",
        "mineffect",
        ValueKind::Text,
    ),
    setting(
        "macos.mission-control.keep-space-order",
        "com.apple.dock",
        "mru-spaces",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.hide-recent-apps",
        "com.apple.dock",
        "show-recents",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.disable-launch-animation",
        "com.apple.dock",
        "launchanim",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.show-only-open-apps",
        "com.apple.dock",
        "static-only",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.dim-hidden-apps",
        "com.apple.dock",
        "showhidden",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.remove-auto-hide-delay",
        "com.apple.dock",
        "autohide-delay",
        ValueKind::Integer,
    ),
    setting(
        "macos.dock.enable-magnification",
        "com.apple.dock",
        "magnification",
        ValueKind::Boolean,
    ),
    setting(
        "macos.mission-control.group-windows-by-app",
        "com.apple.dock",
        "expose-group-apps",
        ValueKind::Boolean,
    ),
    setting(
        "macos.window.disable-animations",
        "NSGlobalDomain",
        "NSAutomaticWindowAnimationsEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.window.close-app-windows",
        "NSGlobalDomain",
        "NSQuitAlwaysKeepsWindows",
        ValueKind::Boolean,
    ),
    setting(
        "macos.window.double-click-titlebar-minimize",
        "NSGlobalDomain",
        "AppleActionOnDoubleClick",
        ValueKind::Text,
    ),
    setting(
        "macos.screenshots.disable-shadow",
        "com.apple.screencapture",
        "disable-shadow",
        ValueKind::Boolean,
    ),
    setting(
        "macos.screenshots.use-png",
        "com.apple.screencapture",
        "type",
        ValueKind::Text,
    ),
    setting(
        "macos.screenshots.disable-thumbnail",
        "com.apple.screencapture",
        "show-thumbnail",
        ValueKind::Boolean,
    ),
    setting(
        "macos.keyboard.full-navigation",
        "NSGlobalDomain",
        "AppleKeyboardUIMode",
        ValueKind::Integer,
    ),
    setting(
        "macos.keyboard.fast-key-repeat",
        "NSGlobalDomain",
        "KeyRepeat",
        ValueKind::Integer,
    ),
    setting(
        "macos.keyboard.short-repeat-delay",
        "NSGlobalDomain",
        "InitialKeyRepeat",
        ValueKind::Integer,
    ),
    setting(
        "macos.keyboard.disable-press-and-hold",
        "NSGlobalDomain",
        "ApplePressAndHoldEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.keyboard.use-standard-function-keys",
        "NSGlobalDomain",
        "com.apple.keyboard.fnState",
        ValueKind::Boolean,
    ),
    setting(
        "macos.documents.save-locally",
        "NSGlobalDomain",
        "NSDocumentSaveNewDocumentsToCloud",
        ValueKind::Boolean,
    ),
    setting(
        "macos.printing.quit-after-finish",
        "com.apple.print.PrintingPrefs",
        "Quit When Finished",
        ValueKind::Boolean,
    ),
    setting(
        "macos.safari.show-full-url",
        "com.apple.Safari",
        "ShowFullURLInSmartSearchField",
        ValueKind::Boolean,
    ),
    setting(
        "macos.safari.disable-safe-download-auto-open",
        "com.apple.Safari",
        "AutoOpenSafeDownloads",
        ValueKind::Boolean,
    ),
    setting(
        "macos.safari.show-status-bar",
        "com.apple.Safari",
        "ShowOverlayStatusBar",
        ValueKind::Boolean,
    ),
    setting(
        "macos.safari.enable-develop-menu",
        "com.apple.Safari",
        "IncludeDevelopMenu",
        ValueKind::Boolean,
    ),
    setting(
        "macos.textedit.plain-text-default",
        "com.apple.TextEdit",
        "RichText",
        ValueKind::Boolean,
    ),
    setting(
        "macos.photos.disable-auto-open",
        "com.apple.ImageCapture",
        "disableHotPlug",
        ValueKind::Boolean,
    ),
    setting(
        "macos.privacy.disable-personalized-ads",
        "com.apple.AdLib",
        "allowApplePersonalizedAdvertising",
        ValueKind::Boolean,
    ),
    setting(
        "macos.sharing.disable-airdrop",
        "com.apple.NetworkBrowser",
        "DisableAirDrop",
        ValueKind::Boolean,
    ),
    setting(
        "macos.activity-monitor.show-all-processes",
        "com.apple.ActivityMonitor",
        "ShowCategory",
        ValueKind::Integer,
    ),
    setting(
        "macos.app-store.enable-auto-updates",
        "com.apple.commerce",
        "AutoUpdate",
        ValueKind::Boolean,
    ),
    setting(
        "macos.text.disable-auto-correct",
        "NSGlobalDomain",
        "NSAutomaticSpellingCorrectionEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.text.disable-smart-quotes",
        "NSGlobalDomain",
        "NSAutomaticQuoteSubstitutionEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.text.disable-smart-dashes",
        "NSGlobalDomain",
        "NSAutomaticDashSubstitutionEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.text.disable-auto-capitalization",
        "NSGlobalDomain",
        "NSAutomaticCapitalizationEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.text.disable-period-substitution",
        "NSGlobalDomain",
        "NSAutomaticPeriodSubstitutionEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.finder.warn-before-empty-trash",
        "com.apple.finder",
        "WarnOnEmptyTrash",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.enable-spring-loading",
        "com.apple.dock",
        "enable-spring-load-actions-on-all-items",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.scroll-to-expose",
        "com.apple.dock",
        "scroll-to-open",
        ValueKind::Boolean,
    ),
    setting(
        "macos.dock.fast-auto-hide-animation",
        "com.apple.dock",
        "autohide-time-modifier",
        ValueKind::Integer,
    ),
    setting(
        "macos.safari.disable-search-suggestions",
        "com.apple.Safari",
        "SuppressSearchSuggestions",
        ValueKind::Boolean,
    ),
    setting(
        "macos.safari.disable-top-hit-preload",
        "com.apple.Safari",
        "PreloadTopHit",
        ValueKind::Boolean,
    ),
    setting(
        "macos.text.disable-text-completion",
        "NSGlobalDomain",
        "NSAutomaticTextCompletionEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.text.disable-inline-predictions",
        "NSGlobalDomain",
        "NSAutomaticInlinePredictionEnabled",
        ValueKind::Boolean,
    ),
    setting(
        "macos.sound.disable-volume-feedback",
        "NSGlobalDomain",
        "com.apple.sound.beep.feedback",
        ValueKind::Boolean,
    ),
    setting(
        "macos.time-machine.hide-new-disk-prompts",
        "com.apple.TimeMachine",
        "DoNotOfferNewDisksForBackup",
        ValueKind::Boolean,
    ),
    setting(
        "macos.security.require-password-after-sleep",
        "com.apple.screensaver",
        "askForPassword",
        ValueKind::Boolean,
    ),
    setting(
        "macos.security.lock-immediately-after-sleep",
        "com.apple.screensaver",
        "askForPasswordDelay",
        ValueKind::Integer,
    ),
];

const fn setting(
    id: &'static str,
    domain: &'static str,
    key: &'static str,
    kind: ValueKind,
) -> SettingDefinition {
    SettingDefinition {
        id,
        domain,
        key,
        kind,
    }
}

pub(crate) fn scan(
    setting_ids: &[&str],
    cancellation: &PlatformCancellation,
) -> PlatformResult<Vec<PlatformSystemSettingState>> {
    let started = Instant::now();
    let result = scan_with_reader(
        setting_ids,
        cancellation,
        SETTINGS_SCAN_DEADLINE,
        || started.elapsed(),
        read_state,
    );
    if let Some(elapsed) = result.deadline_elapsed {
        log::warn!(
            "macos_system_settings_scan_deadline_exceeded completed_count={} requested_count={} elapsed_ms={}",
            result.states.len(),
            setting_ids.len(),
            elapsed.as_millis()
        );
    }
    Ok(result.states)
}

struct SettingsScanResult {
    states: Vec<PlatformSystemSettingState>,
    deadline_elapsed: Option<Duration>,
}

fn scan_with_reader<E, R>(
    setting_ids: &[&str],
    cancellation: &PlatformCancellation,
    deadline: Duration,
    mut elapsed: E,
    mut read: R,
) -> SettingsScanResult
where
    E: FnMut() -> Duration,
    R: FnMut(SettingDefinition, &PlatformCancellation) -> PlatformSystemSettingState,
{
    let mut states = Vec::with_capacity(setting_ids.len());
    for setting_id in setting_ids {
        if cancellation.is_cancelled() {
            // Core owns the public cancellation protocol. Returning the partial read allows it to
            // convert the shared cancellation flag into the stable operation-cancelled error.
            break;
        }
        let current_elapsed = elapsed();
        if current_elapsed >= deadline {
            // Native preference reads normally finish in milliseconds. Stop after a bounded
            // deadline when cfprefsd or a container filesystem stalls so the page can render the
            // successfully observed settings and mark the remaining definitions unavailable.
            return SettingsScanResult {
                states,
                deadline_elapsed: Some(current_elapsed),
            };
        }
        let Some(definition) = definition(setting_id) else {
            states.push(unsupported_state(setting_id));
            continue;
        };
        states.push(read(definition, cancellation));
    }
    SettingsScanResult {
        states,
        deadline_elapsed: None,
    }
}

pub(crate) fn change(
    request: &PlatformSystemSettingChangeRequest,
) -> PlatformResult<PlatformSystemSettingChangeResult> {
    let result = change_without_refresh(request)?;
    if result.changed && requires_finder_refresh(&request.setting_id) {
        refresh_finder().map_err(PlatformError::with_possible_side_effects)?;
        log::info!("macos_system_settings_finder_refreshed setting_count=1");
    }
    Ok(result)
}

pub(crate) fn change_many(
    requests: &[PlatformSystemSettingChangeRequest],
) -> PlatformResult<Vec<PlatformResult<PlatformSystemSettingChangeResult>>> {
    let mut results = requests
        .iter()
        .map(change_without_refresh)
        .collect::<Vec<_>>();
    let refresh_indexes = requests
        .iter()
        .zip(&results)
        .enumerate()
        .filter_map(|(index, (request, result))| {
            result
                .as_ref()
                .is_ok_and(|result| result.changed && requires_finder_refresh(&request.setting_id))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if refresh_indexes.is_empty() {
        return Ok(results);
    }

    match refresh_finder() {
        Ok(()) => log::info!(
            "macos_system_settings_finder_refreshed setting_count={}",
            refresh_indexes.len()
        ),
        Err(error) => {
            log::warn!(
                "macos_system_settings_finder_refresh_failed setting_count={} code={:?}",
                refresh_indexes.len(),
                error.code()
            );
            for index in refresh_indexes {
                results[index] = Err(PlatformError::with_possible_side_effects(error.clone()));
            }
        }
    }
    Ok(results)
}

fn change_without_refresh(
    request: &PlatformSystemSettingChangeRequest,
) -> PlatformResult<PlatformSystemSettingChangeResult> {
    let definition = definition(&request.setting_id).ok_or_else(|| {
        PlatformError::new(
            PlatformErrorCode::Unsupported,
            "system setting identifier is unsupported",
        )
    })?;
    validate_value(definition.kind, &request.desired_value)?;
    let before = read_value(definition)?;
    if let Some(result) = preflight_system_setting_change(&before, request)? {
        return Ok(result);
    }

    write_value(definition, &request.desired_value)
        .map_err(PlatformError::with_possible_side_effects)?;
    let after = read_value(definition).map_err(PlatformError::with_possible_side_effects)?;
    Ok(PlatformSystemSettingChangeResult {
        changed: after != before,
        verified: after == request.desired_value,
        value: after,
    })
}

fn requires_finder_refresh(setting_id: &str) -> bool {
    // This preference is safe to apply immediately. Other Finder settings remain restart-gated
    // because relaunching Finder can trigger behavior such as deleting old Trash items.
    setting_id == "macos.finder.show-hidden-files"
}

fn refresh_finder() -> PlatformResult<()> {
    for attempt in 0..3 {
        let status = Command::new(Path::new("/usr/bin/killall"))
            .arg("Finder")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| PlatformError::io("refresh Finder", &error))?;
        if status.success() || !process_is_running("Finder")? {
            return Ok(());
        }
        if attempt < 2 {
            // Finder is demand-launched. A rapid second update can race with launchd between the
            // old process exiting and the replacement becoming signalable.
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    Err(PlatformError::operation_failed("failed to refresh Finder"))
}

fn process_is_running(process_name: &str) -> PlatformResult<bool> {
    Command::new(Path::new("/usr/bin/pgrep"))
        .args(["-x", process_name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|error| PlatformError::io("inspect process state", &error))
}

fn definition(setting_id: &str) -> Option<SettingDefinition> {
    SETTINGS.iter().copied().find(|item| item.id == setting_id)
}

fn unsupported_state(setting_id: &str) -> PlatformSystemSettingState {
    PlatformSystemSettingState {
        setting_id: setting_id.to_string(),
        value: PlatformSystemSettingValue::Missing,
        effective_value: PlatformSystemSettingValue::Missing,
        requires_elevation: false,
        diagnostic: Some(PlatformSystemSettingDiagnosticCode::Unsupported),
    }
}

fn read_state(
    definition: SettingDefinition,
    cancellation: &PlatformCancellation,
) -> PlatformSystemSettingState {
    match read_value_with_cancellation(definition, &|| cancellation.is_cancelled()) {
        Ok(value) => PlatformSystemSettingState {
            setting_id: definition.id.to_string(),
            effective_value: value.clone(),
            value,
            requires_elevation: false,
            diagnostic: None,
        },
        Err(error) => {
            log::warn!(
                "macos_system_setting_read_failed setting_id={} code={:?}",
                definition.id,
                error.code()
            );
            PlatformSystemSettingState {
                setting_id: definition.id.to_string(),
                value: PlatformSystemSettingValue::Missing,
                effective_value: PlatformSystemSettingValue::Missing,
                requires_elevation: false,
                diagnostic: Some(diagnostic_for_error(error.code())),
            }
        }
    }
}

fn diagnostic_for_error(code: PlatformErrorCode) -> PlatformSystemSettingDiagnosticCode {
    match code {
        PlatformErrorCode::AccessDenied => PlatformSystemSettingDiagnosticCode::AccessDenied,
        PlatformErrorCode::Unsupported => PlatformSystemSettingDiagnosticCode::Unsupported,
        _ => PlatformSystemSettingDiagnosticCode::StateUnavailable,
    }
}

fn read_value(definition: SettingDefinition) -> PlatformResult<PlatformSystemSettingValue> {
    read_value_with_cancellation(definition, &|| false)
}

fn read_value_with_cancellation(
    definition: SettingDefinition,
    is_cancelled: &(dyn Fn() -> bool + Sync),
) -> PlatformResult<PlatformSystemSettingValue> {
    let output = run_defaults(&["read", definition.domain, definition.key], is_cancelled)?;
    if !output.status.success() {
        // A missing key delegates to the operating-system default and must be
        // preserved as a distinct value so an optimization can be reverted.
        return Ok(PlatformSystemSettingValue::Missing);
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| PlatformError::operation_failed("system setting returned invalid UTF-8"))?;
    let text = text.trim();
    match definition.kind {
        ValueKind::Boolean => match text.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(PlatformSystemSettingValue::Boolean(true)),
            "0" | "false" | "no" => Ok(PlatformSystemSettingValue::Boolean(false)),
            _ => Err(PlatformError::operation_failed(
                "system setting returned an invalid boolean",
            )),
        },
        ValueKind::Integer => text
            .parse::<i64>()
            .map(PlatformSystemSettingValue::Integer)
            .map_err(|_| {
                PlatformError::operation_failed("system setting returned an invalid integer")
            }),
        ValueKind::Text => Ok(PlatformSystemSettingValue::Text(text.to_string())),
    }
}

fn write_value(
    definition: SettingDefinition,
    value: &PlatformSystemSettingValue,
) -> PlatformResult<()> {
    let output = match value {
        PlatformSystemSettingValue::Missing => {
            run_defaults(&["delete", definition.domain, definition.key], &|| false)?
        }
        PlatformSystemSettingValue::Boolean(value) => run_defaults(
            &[
                "write",
                definition.domain,
                definition.key,
                "-bool",
                if *value { "true" } else { "false" },
            ],
            &|| false,
        )?,
        PlatformSystemSettingValue::Integer(value) => {
            let value = value.to_string();
            run_defaults(
                &[
                    "write",
                    definition.domain,
                    definition.key,
                    "-int",
                    value.as_str(),
                ],
                &|| false,
            )?
        }
        PlatformSystemSettingValue::Text(value) => run_defaults(
            &["write", definition.domain, definition.key, "-string", value],
            &|| false,
        )?,
        PlatformSystemSettingValue::Snapshot(_) => {
            return Err(PlatformError::new(
                PlatformErrorCode::InvalidData,
                "native snapshots are unsupported for macOS settings",
            ))
        }
    };
    let deleted = matches!(value, PlatformSystemSettingValue::Missing)
        && read_value(definition)? == PlatformSystemSettingValue::Missing;
    if output.status.success() || deleted {
        Ok(())
    } else {
        Err(PlatformError::operation_failed(
            "failed to write the system setting",
        ))
    }
}

fn validate_value(kind: ValueKind, value: &PlatformSystemSettingValue) -> PlatformResult<()> {
    let valid = matches!(value, PlatformSystemSettingValue::Missing)
        || matches!(
            (kind, value),
            (ValueKind::Boolean, PlatformSystemSettingValue::Boolean(_))
                | (ValueKind::Integer, PlatformSystemSettingValue::Integer(_))
                | (ValueKind::Text, PlatformSystemSettingValue::Text(_))
        );
    if valid {
        Ok(())
    } else {
        Err(PlatformError::operation_failed(
            "system setting value has an invalid type",
        ))
    }
}

fn run_defaults(
    args: &[&str],
    is_cancelled: &(dyn Fn() -> bool + Sync),
) -> PlatformResult<ControlledCommandOutput> {
    let executable = ControlledExecutable::capture(Path::new("/usr/bin/defaults"))
        .map_err(system_settings_command_error)?;
    run_controlled_command_with_log_policy(
        "macos_system_settings_defaults",
        &executable,
        args,
        ControlledEnvironmentPolicy::Inherit,
        DEFAULTS_LIMITS,
        ControlledCommandLogPolicy::ExceptionalOnly,
        is_cancelled,
    )
    .map_err(system_settings_command_error)
}

fn system_settings_command_error(error: ControlledCommandError) -> PlatformError {
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
        "system settings command could not complete",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use super::*;

    fn observed_state(definition: SettingDefinition) -> PlatformSystemSettingState {
        PlatformSystemSettingState {
            setting_id: definition.id.to_string(),
            value: PlatformSystemSettingValue::Boolean(true),
            effective_value: PlatformSystemSettingValue::Boolean(true),
            requires_elevation: false,
            diagnostic: None,
        }
    }

    #[test]
    fn setting_identifiers_are_unique_and_namespaced() {
        let mut ids = SETTINGS.iter().map(|item| item.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SETTINGS.len());
        assert!(ids.iter().all(|id| id.starts_with("macos.")));
    }

    #[test]
    fn only_safe_finder_preferences_refresh_immediately() {
        assert!(requires_finder_refresh("macos.finder.show-hidden-files"));
        assert!(!requires_finder_refresh(
            "macos.finder.remove-old-trash-items"
        ));
        assert!(!requires_finder_refresh("macos.dock.auto-hide"));
    }

    #[test]
    fn expanded_catalog_uses_expected_preference_types() {
        let cases = [
            (
                "macos.finder.show-hidden-files",
                "com.apple.finder",
                "AppleShowAllFiles",
                ValueKind::Boolean,
            ),
            (
                "macos.finder.default-list-view",
                "com.apple.finder",
                "FXPreferredViewStyle",
                ValueKind::Text,
            ),
            (
                "macos.dock.remove-auto-hide-delay",
                "com.apple.dock",
                "autohide-delay",
                ValueKind::Integer,
            ),
            (
                "macos.privacy.disable-personalized-ads",
                "com.apple.AdLib",
                "allowApplePersonalizedAdvertising",
                ValueKind::Boolean,
            ),
            (
                "macos.finder.warn-before-empty-trash",
                "com.apple.finder",
                "WarnOnEmptyTrash",
                ValueKind::Boolean,
            ),
            (
                "macos.dock.fast-auto-hide-animation",
                "com.apple.dock",
                "autohide-time-modifier",
                ValueKind::Integer,
            ),
            (
                "macos.security.lock-immediately-after-sleep",
                "com.apple.screensaver",
                "askForPasswordDelay",
                ValueKind::Integer,
            ),
        ];

        for (id, domain, key, kind) in cases {
            let definition = definition(id).expect("the expanded setting should exist");
            assert_eq!(definition.domain, domain);
            assert_eq!(definition.key, key);
            assert!(matches!(
                (definition.kind, kind),
                (ValueKind::Boolean, ValueKind::Boolean)
                    | (ValueKind::Integer, ValueKind::Integer)
                    | (ValueKind::Text, ValueKind::Text)
            ));
        }
    }

    #[test]
    fn scan_stops_before_reading_when_already_cancelled() {
        let result = scan_with_reader(
            &["macos.finder.show-hidden-files"],
            &PlatformCancellation::new(|| true),
            SETTINGS_SCAN_DEADLINE,
            || Duration::ZERO,
            |definition, _| observed_state(definition),
        );

        assert!(result.states.is_empty());
        assert!(result.deadline_elapsed.is_none());
    }

    #[test]
    fn scan_preserves_partial_results_after_cancellation() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let cancellation =
            PlatformCancellation::new(move || cancellation_flag.load(Ordering::SeqCst));
        let result = scan_with_reader(
            &["macos.finder.show-hidden-files", "macos.dock.auto-hide"],
            &cancellation,
            SETTINGS_SCAN_DEADLINE,
            || Duration::ZERO,
            |definition, _| {
                cancelled.store(true, Ordering::SeqCst);
                observed_state(definition)
            },
        );

        assert_eq!(result.states.len(), 1);
        assert_eq!(
            result.states[0].setting_id,
            "macos.finder.show-hidden-files"
        );
        assert!(result.deadline_elapsed.is_none());
    }

    #[test]
    fn scan_reports_deadline_without_discarding_completed_states() {
        let elapsed_values = [Duration::ZERO, SETTINGS_SCAN_DEADLINE];
        let mut elapsed_values = elapsed_values.into_iter();
        let result = scan_with_reader(
            &["macos.finder.show-hidden-files", "macos.dock.auto-hide"],
            &PlatformCancellation::new(|| false),
            SETTINGS_SCAN_DEADLINE,
            || {
                elapsed_values
                    .next()
                    .expect("each requested setting checks the deadline")
            },
            |definition, _| observed_state(definition),
        );

        assert_eq!(result.states.len(), 1);
        assert_eq!(result.deadline_elapsed, Some(SETTINGS_SCAN_DEADLINE));
    }

    #[test]
    fn scan_keeps_unknown_settings_as_partial_diagnostics() {
        let result = scan_with_reader(
            &["macos.unknown.setting", "macos.dock.auto-hide"],
            &PlatformCancellation::new(|| false),
            SETTINGS_SCAN_DEADLINE,
            || Duration::ZERO,
            |definition, _| observed_state(definition),
        );

        assert_eq!(result.states.len(), 2);
        assert_eq!(
            result.states[0].diagnostic,
            Some(PlatformSystemSettingDiagnosticCode::Unsupported)
        );
        assert_eq!(result.states[1].setting_id, "macos.dock.auto-hide");
    }

    #[test]
    #[ignore = "reads the current macOS preferences database"]
    fn actual_catalog_reads_every_known_setting() {
        let ids = SETTINGS.iter().map(|item| item.id).collect::<Vec<_>>();
        let states = scan(&ids, &PlatformCancellation::new(|| false))
            .expect("the macOS preferences catalog should be readable");

        assert_eq!(states.len(), SETTINGS.len());
        assert!(states.iter().all(|state| state.diagnostic.is_none()));
    }

    #[test]
    #[ignore = "temporarily changes and restores the current Activity Monitor preference"]
    fn actual_setting_round_trip_restores_original_value() {
        let definition = definition("macos.activity-monitor.show-all-processes")
            .expect("the Activity Monitor setting should exist");
        let before = read_value(definition).expect("the original preference should be readable");
        let temporary = if before == PlatformSystemSettingValue::Integer(100) {
            PlatformSystemSettingValue::Integer(102)
        } else {
            PlatformSystemSettingValue::Integer(100)
        };

        let round_trip = (|| -> PlatformResult<()> {
            write_value(definition, &temporary)?;
            let after = read_value(definition)?;
            if after != temporary {
                return Err(PlatformError::operation_failed(
                    "temporary system setting value was not persisted",
                ));
            }
            Ok(())
        })();
        let restore = write_value(definition, &before);

        restore.expect("the original preference should always be restored");
        round_trip.expect("the expanded setting should support a verified round trip");
        assert_eq!(
            read_value(definition).expect("the restored preference should be readable"),
            before
        );
    }

    #[test]
    #[ignore = "temporarily changes the hidden-file preference and refreshes Finder"]
    fn actual_hidden_files_round_trip_refreshes_and_restores() {
        let definition = definition("macos.finder.show-hidden-files")
            .expect("the hidden-file setting should exist");
        let before = read_value(definition).expect("the original preference should be readable");
        let temporary = match before {
            PlatformSystemSettingValue::Boolean(value) => {
                PlatformSystemSettingValue::Boolean(!value)
            }
            _ => PlatformSystemSettingValue::Boolean(true),
        };

        let round_trip = (|| -> PlatformResult<()> {
            let mut results = change_many(&[PlatformSystemSettingChangeRequest {
                setting_id: definition.id.to_string(),
                expected_value: before.clone(),
                desired_value: temporary.clone(),
            }])?;
            let result = results
                .pop()
                .ok_or_else(|| PlatformError::operation_failed("batch result was missing"))??;
            if !result.changed || !result.verified || read_value(definition)? != temporary {
                return Err(PlatformError::operation_failed(
                    "temporary hidden-file preference was not applied",
                ));
            }
            Ok(())
        })();
        let restore = (|| -> PlatformResult<()> {
            let mut results = change_many(&[PlatformSystemSettingChangeRequest {
                setting_id: definition.id.to_string(),
                expected_value: temporary,
                desired_value: before.clone(),
            }])?;
            results
                .pop()
                .ok_or_else(|| PlatformError::operation_failed("batch result was missing"))??;
            Ok(())
        })();

        restore.expect("the original hidden-file preference should always be restored");
        round_trip.expect("the hidden-file preference should support a verified round trip");
        assert_eq!(
            read_value(definition).expect("the restored preference should be readable"),
            before
        );
    }

    #[test]
    #[ignore = "temporarily changes and restores the current keyboard repeat preferences"]
    fn actual_key_repeat_settings_round_trip_and_restore() {
        let cases = [
            (
                "macos.keyboard.fast-key-repeat",
                PlatformSystemSettingValue::Integer(2),
            ),
            (
                "macos.keyboard.short-repeat-delay",
                PlatformSystemSettingValue::Integer(15),
            ),
        ];

        for (setting_id, candidate) in cases {
            let definition = definition(setting_id).expect("the key repeat setting should exist");
            let before =
                read_value(definition).expect("the original preference should be readable");
            let temporary = if before == candidate {
                let PlatformSystemSettingValue::Integer(value) = candidate else {
                    unreachable!("keyboard repeat test values must be integers");
                };
                PlatformSystemSettingValue::Integer(value + 1)
            } else {
                candidate
            };

            let round_trip = (|| -> PlatformResult<()> {
                write_value(definition, &temporary)?;
                if read_value(definition)? != temporary {
                    return Err(PlatformError::operation_failed(
                        "temporary keyboard repeat preference was not persisted",
                    ));
                }
                Ok(())
            })();
            let restore = write_value(definition, &before);

            restore.expect("the original keyboard repeat preference should always be restored");
            round_trip
                .expect("the keyboard repeat preference should support a verified round trip");
            assert_eq!(
                read_value(definition).expect("the restored preference should be readable"),
                before
            );
        }
    }

    #[test]
    #[ignore = "temporarily changes and restores newly added macOS preferences"]
    fn actual_expanded_settings_round_trip_and_restore() {
        let cases = [
            (
                "macos.finder.warn-before-empty-trash",
                PlatformSystemSettingValue::Boolean(false),
            ),
            (
                "macos.dock.fast-auto-hide-animation",
                PlatformSystemSettingValue::Integer(1),
            ),
            (
                "macos.sound.disable-volume-feedback",
                PlatformSystemSettingValue::Boolean(false),
            ),
        ];

        for (setting_id, candidate) in cases {
            let definition = definition(setting_id).expect("the expanded setting should exist");
            let before =
                read_value(definition).expect("the original preference should be readable");
            let temporary = if before == candidate {
                match candidate {
                    PlatformSystemSettingValue::Boolean(value) => {
                        PlatformSystemSettingValue::Boolean(!value)
                    }
                    PlatformSystemSettingValue::Integer(value) => {
                        PlatformSystemSettingValue::Integer(value + 1)
                    }
                    _ => panic!("the test case must use a scalar preference value"),
                }
            } else {
                candidate
            };

            let round_trip = (|| -> PlatformResult<()> {
                write_value(definition, &temporary)?;
                if read_value(definition)? != temporary {
                    return Err(PlatformError::operation_failed(
                        "temporary expanded preference value was not persisted",
                    ));
                }
                Ok(())
            })();
            let restore = write_value(definition, &before);

            restore.expect("the original expanded preference should always be restored");
            round_trip.expect("the expanded preference should support a verified round trip");
            assert_eq!(
                read_value(definition).expect("the restored preference should be readable"),
                before
            );
        }
    }
}
