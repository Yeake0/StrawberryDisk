use std::{
    collections::BTreeSet,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use plist::{Dictionary, Value};

use crate::{
    PlatformCancellation, PlatformError, PlatformResult, PlatformStartupArtifact,
    PlatformStartupChangeRequest, PlatformStartupChangeResult, PlatformStartupConfiguredState,
    PlatformStartupControlCapability, PlatformStartupCoverageReason, PlatformStartupCoverageStatus,
    PlatformStartupDesiredState, PlatformStartupDiagnosticCode, PlatformStartupIdentityConfidence,
    PlatformStartupOwner, PlatformStartupRuntimeState, PlatformStartupScope,
    PlatformStartupSourceKind, PlatformStartupSourceResult, PlatformStartupSummarySource,
    PlatformStartupTarget, PlatformStartupTargetKind, PlatformStartupTrigger,
    PlatformStartupTrustState,
};

use super::{
    embedded::{bundle_name, read_bundle_metadata, string_value},
    login_items,
};

mod legacy;

const SOURCE_ID: &str = "macos.background_tasks";
const DATABASE_DIRECTORY: &str = "/var/db/com.apple.backgroundtaskmanagement";
const SUPPORTED_ARCHIVE_VERSIONS: &[u64] = &[13];
const MAX_DATABASE_BYTES: u64 = 8 * 1024 * 1024;
const APP_RECORD_TYPE: u64 = 2;
const DISPOSITION_ENABLED: u64 = 1;
const COCOA_REFERENCE_DATE_OFFSET_SECONDS: u64 = 978_307_200;

pub(super) fn scan(cancellation: &PlatformCancellation) -> PlatformStartupSourceResult {
    let started = Instant::now();
    if cancellation.is_cancelled() {
        return result(
            Vec::new(),
            PlatformStartupCoverageStatus::Cancelled,
            Some(PlatformStartupCoverageReason::Cancelled),
            started,
        );
    }
    let (records, modified_at_ms) = match read_database_records(cancellation) {
        Ok(value) => value,
        Err(ParseError::Cancelled) => {
            return result(
                Vec::new(),
                PlatformStartupCoverageStatus::Cancelled,
                Some(PlatformStartupCoverageReason::Cancelled),
                started,
            );
        }
        Err(ParseError::AccessDenied) => {
            return unavailable(started, PlatformStartupCoverageReason::AccessDenied);
        }
        Err(ParseError::Unsupported) => {
            return unavailable(
                started,
                PlatformStartupCoverageReason::UnsupportedOperatingSystem,
            );
        }
        Err(ParseError::InvalidData) => {
            return unavailable(started, PlatformStartupCoverageReason::InvalidData);
        }
    };
    let enabled_paths = login_items::enabled_paths().ok();
    let missing_items = login_items::missing_items().unwrap_or_else(|error| {
        log::warn!("startup_login_record_identity_unavailable source_id={} reason={:?} diagnostic_detail={}",
            SOURCE_ID, error.code(), crate::diagnostics::text(error.diagnostic()));
        Vec::new()
    });
    let removable = records
        .iter()
        .filter_map(|record| {
            match_missing_item(record, &records, &missing_items)
                .map(|item| (record.identifier.clone(), item))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let items = records
        .into_iter()
        .map(|record| {
            let native = removable.get(&record.identifier);
            let uuid = record.uuid;
            let mut artifact = artifact_from_record(record, modified_at_ms, enabled_paths.as_ref());
            if native.is_some() {
                artifact.control_capability = PlatformStartupControlCapability::RemoveOnly;
                // Bind preflight to all UUID bytes, not just the shared-list's 32-bit item ID.
                artifact.provider_item_id.push_str(&format!(
                    ":{}",
                    uuid.map(|value| value
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>())
                        .unwrap_or_default()
                ));
            }
            artifact
        })
        .collect();
    result(
        items,
        PlatformStartupCoverageStatus::Complete,
        None,
        started,
    )
}

#[derive(Debug, PartialEq, Eq)]
enum ParseError {
    AccessDenied,
    Cancelled,
    InvalidData,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq)]
struct BackgroundAppRecord {
    identifier: String,
    uuid: Option<[u8; 16]>,
    bundle_identifier: String,
    name: String,
    developer_name: Option<String>,
    path: PathBuf,
    disposition: u64,
    modified_at: Option<f64>,
}

/// BTM archive v13 represents legacy shared-list IDs with the first four UUID bytes in
/// little-endian order. This adapter only accepts a unique archive/native match, the native
/// display name, and explicit missing-file evidence from both sources. Unknown schemas and
/// collisions retain the system-managed fallback; names alone never authorize deletion.
fn match_missing_item(
    record: &BackgroundAppRecord,
    records: &[BackgroundAppRecord],
    items: &[login_items::MissingLoginItem],
) -> Option<login_items::MissingLoginItem> {
    let id = record.uuid.map(native_item_id)?;
    if !is_local_missing_target(&record.path) {
        return None;
    }
    if records
        .iter()
        .filter(|candidate| candidate.uuid.map(native_item_id) == Some(id))
        .count()
        != 1
    {
        return None;
    }
    let matching = items
        .iter()
        .filter(|item| item.id == id)
        .collect::<Vec<_>>();
    (matching.len() == 1 && matching[0].name == record.name).then(|| matching[0].clone())
}

fn is_local_missing_target(path: &Path) -> bool {
    // A missing mount must never turn an external application's login entry into cleanup data.
    // Resolve symlink ancestors too: a local-looking path may otherwise hide a dangling link
    // to an unplugged volume. Existing local aliases such as /var remain valid.
    if !path.is_absolute()
        || is_external_mount_path(path)
        || path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
        || !matches!(path.try_exists(), Ok(false))
    {
        return false;
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if !fs::canonicalize(ancestor)
                    .is_ok_and(|resolved| !is_external_mount_path(&resolved))
                {
                    return false;
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return false,
        }
    }
    true
}

fn is_external_mount_path(path: &Path) -> bool {
    path.starts_with("/Volumes")
        || path.starts_with("/Network")
        || path.starts_with("/System/Volumes/Data/Volumes")
        || path.starts_with("/System/Volumes/Data/Network")
}

fn native_item_id(uuid: [u8; 16]) -> u32 {
    u32::from_le_bytes([uuid[0], uuid[1], uuid[2], uuid[3]])
}

fn database_path() -> Option<(u64, PathBuf)> {
    SUPPORTED_ARCHIVE_VERSIONS.first().map(|version| {
        (
            *version,
            Path::new(DATABASE_DIRECTORY).join(format!("BackgroundItems-v{version}.btm")),
        )
    })
}

fn read_database_records(
    cancellation: &PlatformCancellation,
) -> Result<(Vec<BackgroundAppRecord>, Option<u64>), ParseError> {
    let (version, path) = database_path().ok_or(ParseError::Unsupported)?;
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return legacy::read_records(cancellation);
        }
        Err(error) => return Err(io_parse_error(error)),
    };
    if !metadata.is_file() || metadata.len() > MAX_DATABASE_BYTES {
        return Err(ParseError::InvalidData);
    }
    let bytes = fs::read(&path).map_err(io_parse_error)?;
    let archive = Value::from_reader(Cursor::new(bytes)).map_err(|_| ParseError::InvalidData)?;
    let records = parse_archive(&archive, version, cancellation)?;
    Ok((records, metadata.modified().ok().and_then(system_time_ms)))
}

fn io_parse_error(error: std::io::Error) -> ParseError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        ParseError::AccessDenied
    } else {
        ParseError::InvalidData
    }
}

fn parse_archive(
    archive: &Value,
    expected_version: u64,
    cancellation: &PlatformCancellation,
) -> Result<Vec<BackgroundAppRecord>, ParseError> {
    let root = archive.as_dictionary().ok_or(ParseError::InvalidData)?;
    if root.get("$archiver").and_then(Value::as_string) != Some("NSKeyedArchiver") {
        return Err(ParseError::InvalidData);
    }
    let top = root
        .get("$top")
        .and_then(Value::as_dictionary)
        .ok_or(ParseError::InvalidData)?;
    if top.get("version").and_then(Value::as_unsigned_integer) != Some(expected_version) {
        return Err(ParseError::Unsupported);
    }
    let objects = root
        .get("$objects")
        .and_then(Value::as_array)
        .ok_or(ParseError::InvalidData)?;
    let item_record_classes: BTreeSet<u64> = objects
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            (value.as_dictionary()?.get("$classname")?.as_string()? == "ItemRecord")
                .then_some(index as u64)
        })
        .collect();
    if item_record_classes.is_empty() {
        return Err(ParseError::InvalidData);
    }

    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for value in objects {
        if cancellation.is_cancelled() {
            return Err(ParseError::Cancelled);
        }
        let Some(dictionary) = value.as_dictionary() else {
            continue;
        };
        let Some(class) = dictionary.get("$class").and_then(Value::as_uid) else {
            continue;
        };
        if !item_record_classes.contains(&class.get())
            || dictionary.get("type").and_then(Value::as_unsigned_integer) != Some(APP_RECORD_TYPE)
        {
            continue;
        }
        let Some(record) = app_record(objects, dictionary) else {
            continue;
        };
        if seen.insert(record.identifier.clone()) {
            records.push(record);
        }
    }
    Ok(records)
}

fn app_record(objects: &[Value], dictionary: &Dictionary) -> Option<BackgroundAppRecord> {
    let identifier = referenced_string(objects, dictionary, "identifier")?;
    let bundle_identifier = referenced_string(objects, dictionary, "bundleIdentifier")?;
    let archived_name = referenced_string(objects, dictionary, "name")?;
    let path = referenced_url(objects, dictionary, "url")?;
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        return None;
    }
    let uuid = referenced_value(objects, dictionary, "uuid")
        .and_then(Value::as_dictionary)
        .and_then(|value| value.get("NS.uuidbytes"))
        .and_then(Value::as_data)
        .and_then(|bytes| bytes.try_into().ok());
    Some(BackgroundAppRecord {
        identifier,
        uuid,
        bundle_identifier,
        name: archived_name,
        developer_name: referenced_string(objects, dictionary, "developerName"),
        path,
        disposition: dictionary
            .get("disposition")
            .and_then(Value::as_unsigned_integer)?,
        modified_at: dictionary.get("modificationDate").and_then(number_value),
    })
}

fn referenced_string(objects: &[Value], dictionary: &Dictionary, key: &str) -> Option<String> {
    referenced_value(objects, dictionary, key)?
        .as_string()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn referenced_url(objects: &[Value], dictionary: &Dictionary, key: &str) -> Option<PathBuf> {
    let value = referenced_value(objects, dictionary, key)?.as_dictionary()?;
    let relative = referenced_value(objects, value, "NS.relative")?.as_string()?;
    file_url_path(relative)
}

fn referenced_value<'a>(
    objects: &'a [Value],
    dictionary: &Dictionary,
    key: &str,
) -> Option<&'a Value> {
    let index = dictionary.get(key)?.as_uid()?.get() as usize;
    (index != 0).then(|| objects.get(index)).flatten()
}

fn file_url_path(value: &str) -> Option<PathBuf> {
    let encoded = value.strip_prefix("file://")?;
    if !encoded.starts_with('/') {
        return None;
    }
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok().map(PathBuf::from)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn number_value(value: &Value) -> Option<f64> {
    value
        .as_real()
        .or_else(|| value.as_signed_integer().map(|number| number as f64))
        .or_else(|| value.as_unsigned_integer().map(|number| number as f64))
}

fn artifact_from_record(
    record: BackgroundAppRecord,
    database_modified_at_ms: Option<u64>,
    enabled_paths: Option<&BTreeSet<PathBuf>>,
) -> PlatformStartupArtifact {
    let metadata = read_bundle_metadata(&record.path);
    let name = metadata
        .as_ref()
        .and_then(bundle_name)
        .unwrap_or(record.name);
    let executable_name = metadata
        .as_ref()
        .and_then(|metadata| string_value(metadata, "CFBundleExecutable"));
    // Older archives retain bookmarks, but not bundle identifiers. Keep their path identity
    // distinct so unrelated orphan applications cannot collapse into an empty bundle group.
    let bundle_identifier = (!record.bundle_identifier.is_empty())
        .then_some(record.bundle_identifier)
        .or_else(|| {
            metadata
                .as_ref()
                .and_then(|value| string_value(value, "CFBundleIdentifier"))
        });
    let has_bundle_identity = bundle_identifier.is_some();
    let identity_key = bundle_identifier
        .map(|identifier| format!("bundle:{identifier}"))
        .unwrap_or_else(|| format!("path:{}", record.path.display()));
    // BTM already provides an exact bundle identity and a developer label for these
    // system-managed records. Synchronous signature validation can take seconds per
    // application on a cold macOS trust cache, so leave trust enrichment unknown
    // instead of blocking the complete startup scan for non-actionable metadata.
    let mut diagnostics = Vec::new();
    let target_exists = match record.path.try_exists() {
        Ok(exists) => {
            if !exists {
                diagnostics.push(PlatformStartupDiagnosticCode::MissingTarget);
            }
            exists
        }
        Err(error) => {
            diagnostics.push(if error.kind() == std::io::ErrorKind::PermissionDenied {
                PlatformStartupDiagnosticCode::AccessDenied
            } else {
                PlatformStartupDiagnosticCode::StateUnavailable
            });
            log::warn!(
                "startup_login_target_inspection_failed target_path={} native_code={:?} error={}",
                crate::diagnostics::text(&record.path.to_string_lossy()),
                error.raw_os_error(),
                crate::diagnostics::text(&error.to_string())
            );
            false
        }
    };
    let enabled = enabled_paths.is_some_and(|paths| paths.contains(&record.path))
        || (enabled_paths.is_none() && record.disposition & DISPOSITION_ENABLED != 0);
    PlatformStartupArtifact {
        provider_item_id: format!("background-task:{}", record.identifier),
        source_kind: PlatformStartupSourceKind::BackgroundTask,
        scope: PlatformStartupScope::CurrentUser,
        triggers: vec![PlatformStartupTrigger::UserLogon],
        display_name: name.clone(),
        configuration_path: None,
        target: PlatformStartupTarget {
            kind: PlatformStartupTargetKind::Application,
            identity_key: identity_key.clone(),
            path: Some(record.path.clone()),
            executable_name,
            arguments: Vec::new(),
        },
        owner: PlatformStartupOwner {
            identity_key: Some(identity_key),
            name: Some(name),
            publisher: record.developer_name,
            summary: None,
            summary_source: PlatformStartupSummarySource::BundleMetadata,
            version: metadata
                .as_ref()
                .and_then(|metadata| string_value(metadata, "CFBundleShortVersionString")),
            icon_path: Some(record.path),
            confidence: if has_bundle_identity {
                PlatformStartupIdentityConfidence::Exact
            } else {
                PlatformStartupIdentityConfidence::Strong
            },
        },
        configured_state: if enabled {
            PlatformStartupConfiguredState::Enabled
        } else {
            PlatformStartupConfiguredState::Disabled
        },
        runtime_state: PlatformStartupRuntimeState::Unknown,
        control_capability: if !target_exists {
            PlatformStartupControlCapability::SystemManaged
        } else if enabled_paths.is_some() {
            PlatformStartupControlCapability::Toggleable
        } else {
            PlatformStartupControlCapability::SystemManaged
        },
        trust: PlatformStartupTrustState::Unknown,
        modified_at_ms: record
            .modified_at
            .and_then(cocoa_time_ms)
            .or(database_modified_at_ms),
        diagnostics,
    }
}

pub(super) fn change(
    request: &PlatformStartupChangeRequest,
) -> PlatformResult<PlatformStartupChangeResult> {
    let cancellation = PlatformCancellation::new(|| false);
    let current = scan(&cancellation)
        .items
        .into_iter()
        .find(|artifact| artifact.provider_item_id == request.provider_item_id)
        .ok_or_else(|| PlatformError::item_changed("background login item no longer exists"))?;
    if current != request.expected_artifact {
        return Err(PlatformError::item_changed(
            "background login item changed after preflight",
        ));
    }
    if request.desired_state == PlatformStartupDesiredState::Removed {
        return remove_record(request, &current);
    }
    if current.control_capability != PlatformStartupControlCapability::Toggleable {
        return Err(PlatformError::new(
            crate::PlatformErrorCode::Unsupported,
            "background login item is not toggleable",
        ));
    }
    let path =
        current.target.path.as_deref().ok_or_else(|| {
            PlatformError::invalid_path("background login item path is unavailable")
        })?;
    let enabled = request.desired_state == PlatformStartupDesiredState::Enabled;
    login_items::set_enabled(path, enabled)?;
    let verified_enabled = login_items::enabled_paths()?.contains(path);
    let configured_state = if verified_enabled {
        PlatformStartupConfiguredState::Enabled
    } else {
        PlatformStartupConfiguredState::Disabled
    };
    let desired_state = if enabled {
        PlatformStartupConfiguredState::Enabled
    } else {
        PlatformStartupConfiguredState::Disabled
    };
    Ok(PlatformStartupChangeResult {
        previous_state: current.configured_state,
        configured_state,
        verified: configured_state == desired_state,
    })
}

/// Changes a login item retained by StrawberryDisk while BTM publishes its native record. The
/// validated bundle path remains the mutation target during both halves of this transition.
pub(super) fn change_managed_login_item(
    request: &PlatformStartupChangeRequest,
) -> PlatformResult<PlatformStartupChangeResult> {
    let path = validate_managed_change_request(request).inspect_err(|error| {
        log::warn!(
            "startup_managed_login_item_change_failed stage=preflight target_path={} desired_state={:?} reason={:?} detail={}",
            request.expected_artifact.target.path.as_ref().map_or_else(|| "none".to_owned(), |path| crate::diagnostics::text(&path.to_string_lossy())),
            request.desired_state,
            error.code(), crate::diagnostics::text(error.diagnostic())
        );
    })?;
    let enabled = request.desired_state == PlatformStartupDesiredState::Enabled;
    let currently_enabled = login_items::enabled_paths()?.contains(path);
    if currently_enabled
        != (request.expected_artifact.configured_state == PlatformStartupConfiguredState::Enabled)
    {
        return Err(PlatformError::item_changed(
            "managed login item changed before native mutation",
        ));
    }
    login_items::set_enabled(path, enabled).inspect_err(|error| {
        log::warn!(
            "startup_managed_login_item_change_failed stage=mutation target_path={} desired_state={:?} reason={:?} detail={}",
            crate::diagnostics::text(&path.to_string_lossy()), request.desired_state, error.code(),
            crate::diagnostics::text(error.diagnostic())
        );
    })?;
    let verified_enabled = login_items::enabled_paths()
        .map_err(|error| {
            log::warn!(
                "startup_managed_login_item_change_failed stage=verify target_path={} desired_state={:?} reason={:?} detail={}",
                crate::diagnostics::text(&path.to_string_lossy()), request.desired_state, error.code(),
                crate::diagnostics::text(error.diagnostic())
            );
            error.with_possible_side_effects()
        })?
        .contains(path);
    let verified = verified_enabled == enabled;
    log::info!(
        "startup_managed_login_item_change target_path={} desired_state={:?} verified={}",
        crate::diagnostics::text(&path.to_string_lossy()),
        request.desired_state,
        verified
    );
    Ok(PlatformStartupChangeResult {
        previous_state: request.expected_artifact.configured_state,
        configured_state: if verified_enabled {
            PlatformStartupConfiguredState::Enabled
        } else {
            PlatformStartupConfiguredState::Disabled
        },
        verified,
    })
}

fn validate_managed_change_request(
    request: &PlatformStartupChangeRequest,
) -> PlatformResult<&Path> {
    let expected = &request.expected_artifact;
    if !matches!(
        (request.desired_state, expected.configured_state),
        (
            PlatformStartupDesiredState::Enabled,
            PlatformStartupConfiguredState::Disabled
        ) | (
            PlatformStartupDesiredState::Disabled,
            PlatformStartupConfiguredState::Enabled
        )
    ) || request.source_id != "macos.managed_login_items"
        || !request.provider_item_id.starts_with("managed-login-item:")
        || expected.provider_item_id != request.provider_item_id
        || expected.source_kind != PlatformStartupSourceKind::BackgroundTask
        || expected.scope != PlatformStartupScope::CurrentUser
        || expected.target.kind != PlatformStartupTargetKind::Application
        || expected.control_capability != PlatformStartupControlCapability::Toggleable
    {
        return Err(PlatformError::item_changed(
            "managed login item change request changed after preflight",
        ));
    }
    let path = expected.target.path.as_deref().ok_or_else(|| {
        PlatformError::invalid_path("managed login item application path is unavailable")
    })?;
    if !path.is_absolute()
        || !path.is_dir()
        || !path
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("app"))
        || path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(PlatformError::item_changed(
            "managed login item application is no longer available",
        ));
    }
    let metadata = read_bundle_metadata(path).ok_or_else(|| {
        PlatformError::item_changed("managed login item bundle metadata is unavailable")
    })?;
    if let Some(bundle_id) = expected.target.identity_key.strip_prefix("bundle:") {
        if string_value(&metadata, "CFBundleIdentifier").as_deref() != Some(bundle_id) {
            return Err(PlatformError::item_changed(
                "managed login item bundle identity changed",
            ));
        }
    } else if expected.target.identity_key != format!("path:{}", path.display()) {
        return Err(PlatformError::item_changed(
            "managed login item path identity changed",
        ));
    }
    Ok(path)
}

fn remove_record(
    request: &PlatformStartupChangeRequest,
    current: &PlatformStartupArtifact,
) -> PlatformResult<PlatformStartupChangeResult> {
    if current.control_capability != PlatformStartupControlCapability::RemoveOnly {
        return Err(PlatformError::new(
            crate::PlatformErrorCode::Unsupported,
            "background login record has no verified native removal identity",
        ));
    }
    let cancellation = PlatformCancellation::new(|| false);
    let (records, _) = read_database_records(&cancellation).map_err(|error| {
        PlatformError::operation_failed(format!("login record preflight read failed: {error:?}"))
    })?;
    let items = login_items::missing_items()?;
    let record = records
        .iter()
        .find(|record| {
            record.uuid.is_some_and(|uuid| {
                let suffix = uuid
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                request.provider_item_id
                    == format!("background-task:{}:{suffix}", record.identifier)
            })
        })
        .ok_or_else(|| PlatformError::item_changed("background login record UUID changed"))?;
    if current.target.path.as_ref() != Some(&record.path) || current.display_name != record.name {
        return Err(PlatformError::item_changed(
            "background login record path or name changed before removal",
        ));
    }
    let native = match_missing_item(record, &records, &items).ok_or_else(|| {
        PlatformError::item_changed(
            "background login record no longer has a unique missing native target",
        )
    })?;
    log::info!("startup_login_record_remove_requested provider_item_id={} native_item_id={} name={} target_path={}",
        crate::diagnostics::text(&request.provider_item_id), native.id,
        crate::diagnostics::text(&record.name), crate::diagnostics::text(&record.path.to_string_lossy()));
    login_items::remove_missing_item(&native)?;
    // BTM persists asynchronously after the shared-list API returns. Bound readback retries so
    // a successful native deletion is not reported as a failure merely because disk state lags.
    let started = Instant::now();
    let mut attempts = 0;
    loop {
        attempts += 1;
        let (remaining, _) = read_database_records(&cancellation).map_err(|error| {
            PlatformError::operation_failed(format!(
                "login record verification read failed: {error:?}"
            ))
            .with_possible_side_effects()
        })?;
        let verified = !remaining.iter().any(|item| item.uuid == record.uuid);
        if verified || started.elapsed() >= std::time::Duration::from_secs(5) {
            log::info!("startup_login_record_remove_verified provider_item_id={} native_item_id={} verified={} attempts={} elapsed_ms={}",
                crate::diagnostics::text(&request.provider_item_id), native.id, verified, attempts, started.elapsed().as_millis());
            if !verified {
                return Err(PlatformError::operation_failed("native login item was removed but BTM archive still contains its UUID after the verification deadline")
                    .with_failure_reason(crate::PlatformFailureReason::VerificationFailed)
                    .with_possible_side_effects());
            }
            return Ok(PlatformStartupChangeResult {
                previous_state: current.configured_state,
                configured_state: PlatformStartupConfiguredState::NotApplicable,
                verified: true,
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

fn cocoa_time_ms(seconds: f64) -> Option<u64> {
    (seconds.is_finite() && seconds >= 0.0)
        .then(|| ((seconds + COCOA_REFERENCE_DATE_OFFSET_SECONDS as f64) * 1000.0).round() as u64)
}

fn system_time_ms(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn unavailable(
    started: Instant,
    reason: PlatformStartupCoverageReason,
) -> PlatformStartupSourceResult {
    result(
        Vec::new(),
        PlatformStartupCoverageStatus::Unavailable,
        Some(reason),
        started,
    )
}

fn result(
    items: Vec<PlatformStartupArtifact>,
    status: PlatformStartupCoverageStatus,
    reason: Option<PlatformStartupCoverageReason>,
    started: Instant,
) -> PlatformStartupSourceResult {
    PlatformStartupSourceResult {
        source_id: SOURCE_ID.to_owned(),
        required: false,
        status,
        reason,
        items,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use plist::{Uid, Value};

    use super::*;

    #[test]
    fn managed_login_change_validates_both_directions_and_bundle_identity() {
        let directory = tempfile::tempdir().unwrap();
        let app = directory.path().join("Example.app");
        let contents = app.join("Contents");
        fs::create_dir_all(&contents).unwrap();
        dictionary([(
            "CFBundleIdentifier",
            Value::String("com.example.App".to_owned()),
        )])
        .to_file_xml(contents.join("Info.plist"))
        .unwrap();
        let mut artifact = artifact_from_record(
            parse_archive(
                &fixture_archive(10),
                13,
                &PlatformCancellation::new(|| false),
            )
            .unwrap()
            .remove(0),
            None,
            Some(&BTreeSet::new()),
        );
        artifact.provider_item_id = "managed-login-item:test".to_owned();
        artifact.target.path = Some(app);
        artifact.target.identity_key = "bundle:com.example.App".to_owned();
        artifact.configured_state = PlatformStartupConfiguredState::Disabled;
        artifact.control_capability = PlatformStartupControlCapability::Toggleable;
        let mut request = PlatformStartupChangeRequest {
            provider_item_id: artifact.provider_item_id.clone(),
            source_id: "macos.managed_login_items".to_owned(),
            expected_artifact: artifact,
            desired_state: PlatformStartupDesiredState::Enabled,
        };

        assert!(validate_managed_change_request(&request).is_ok());
        request.desired_state = PlatformStartupDesiredState::Disabled;
        request.expected_artifact.configured_state = PlatformStartupConfiguredState::Enabled;
        assert!(validate_managed_change_request(&request).is_ok());
        request.expected_artifact.configured_state = PlatformStartupConfiguredState::Disabled;
        assert!(validate_managed_change_request(&request).is_err());
        request.expected_artifact.configured_state = PlatformStartupConfiguredState::Enabled;
        request.expected_artifact.target.identity_key = "bundle:com.example.Replaced".to_owned();
        assert!(validate_managed_change_request(&request).is_err());
    }

    fn dictionary(values: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Dictionary(
            values
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    fn uid(value: u64) -> Value {
        Value::Uid(Uid::new(value))
    }

    fn fixture_archive(disposition: u64) -> Value {
        let objects = vec![
            Value::String("$null".to_owned()),
            dictionary([("$classname", Value::String("ItemRecord".to_owned()))]),
            dictionary([
                ("$class", uid(1)),
                ("type", Value::Integer(APP_RECORD_TYPE.into())),
                ("identifier", uid(3)),
                ("uuid", uid(9)),
                ("bundleIdentifier", uid(4)),
                ("name", uid(5)),
                ("developerName", uid(6)),
                ("url", uid(7)),
                ("disposition", Value::Integer(disposition.into())),
                ("modificationDate", Value::Real(1.0)),
            ]),
            Value::String("2.com.example.Example".to_owned()),
            Value::String("com.example.Example".to_owned()),
            Value::String("Example".to_owned()),
            Value::String("Example Developer".to_owned()),
            dictionary([("NS.relative", uid(8))]),
            Value::String("file:///Applications/Example%20App.app/".to_owned()),
            dictionary([("NS.uuidbytes", Value::Data(vec![1; 16]))]),
        ];
        dictionary([
            ("$archiver", Value::String("NSKeyedArchiver".to_owned())),
            ("$top", dictionary([("version", Value::Integer(13.into()))])),
            ("$objects", Value::Array(objects)),
        ])
    }

    #[test]
    fn parses_supported_application_records() {
        let cancellation = PlatformCancellation::new(|| false);
        let records = parse_archive(&fixture_archive(11), 13, &cancellation).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "Example");
        assert_eq!(
            records[0].path,
            PathBuf::from("/Applications/Example App.app/")
        );
        assert_eq!(records[0].disposition, 11);
        assert_eq!(records[0].uuid, Some([1; 16]));

        let artifact = artifact_from_record(records.into_iter().next().unwrap(), None, None);
        assert_eq!(
            artifact.owner.publisher.as_deref(),
            Some("Example Developer")
        );
        assert_eq!(artifact.trust, PlatformStartupTrustState::Unknown);
    }

    #[test]
    fn inaccessible_target_is_not_reported_as_an_orphan() {
        let root = std::env::temp_dir().join(format!(
            "strawberrydisk-startup-inaccessible-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("Loop.app");
        std::os::unix::fs::symlink(&path, &path).unwrap();
        let mut record = parse_archive(
            &fixture_archive(11),
            13,
            &PlatformCancellation::new(|| false),
        )
        .unwrap()
        .remove(0);
        record.path = path.clone();
        let artifact = artifact_from_record(record, None, Some(&BTreeSet::new()));
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();

        assert!(!artifact
            .diagnostics
            .contains(&PlatformStartupDiagnosticCode::MissingTarget));
        assert!(artifact
            .diagnostics
            .contains(&PlatformStartupDiagnosticCode::StateUnavailable));
        assert_eq!(
            artifact.control_capability,
            PlatformStartupControlCapability::SystemManaged
        );
    }

    #[test]
    fn removal_rejects_traversal_and_symlink_paths_to_offline_volumes() {
        assert!(!is_local_missing_target(Path::new(
            "/Applications/../Volumes/Offline/App.app"
        )));
        assert!(!is_local_missing_target(Path::new(
            "/System/Volumes/Data/Volumes/Offline/App.app"
        )));
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "strawberrydisk-startup-offline-link-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let link = root.join("offline");
        std::os::unix::fs::symlink("/Volumes/StrawberryDiskMissingTestVolume", &link).unwrap();
        assert!(!is_local_missing_target(&link.join("App.app")));
        fs::remove_file(link).unwrap();
        let local_directory = root.join("local");
        fs::create_dir(&local_directory).unwrap();
        let local_link = root.join("local-alias");
        std::os::unix::fs::symlink(&local_directory, &local_link).unwrap();
        assert!(is_local_missing_target(&local_link.join("Missing.app")));
        fs::remove_file(local_link).unwrap();
        fs::remove_dir(local_directory).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn rejects_an_unexpected_archive_version() {
        let cancellation = PlatformCancellation::new(|| false);

        assert_eq!(
            parse_archive(&fixture_archive(11), 12, &cancellation),
            Err(ParseError::Unsupported)
        );
    }

    #[test]
    fn disposition_low_bit_controls_configured_state() {
        let enabled = artifact_from_record(
            parse_archive(
                &fixture_archive(11),
                13,
                &PlatformCancellation::new(|| false),
            )
            .unwrap()
            .remove(0),
            None,
            None,
        );
        let disabled = artifact_from_record(
            parse_archive(
                &fixture_archive(10),
                13,
                &PlatformCancellation::new(|| false),
            )
            .unwrap()
            .remove(0),
            None,
            None,
        );

        assert_eq!(
            enabled.configured_state,
            PlatformStartupConfiguredState::Enabled
        );
        assert_eq!(
            disabled.configured_state,
            PlatformStartupConfiguredState::Disabled
        );
    }

    #[test]
    fn missing_target_requires_native_identity_before_direct_management() {
        let record = parse_archive(
            &fixture_archive(10),
            13,
            &PlatformCancellation::new(|| false),
        )
        .unwrap()
        .remove(0);
        let path = record.path.clone();
        let enabled_paths = BTreeSet::from([path]);

        let artifact = artifact_from_record(record, None, Some(&enabled_paths));

        assert_eq!(
            artifact.configured_state,
            PlatformStartupConfiguredState::Enabled
        );
        assert_eq!(
            artifact.control_capability,
            PlatformStartupControlCapability::SystemManaged
        );
    }

    #[test]
    fn removal_requires_unique_uuid_id_name_and_missing_target() {
        let mut record = parse_archive(
            &fixture_archive(11),
            13,
            &PlatformCancellation::new(|| false),
        )
        .unwrap()
        .remove(0);
        record.path = std::env::temp_dir().join(format!(
            "strawberrydisk-absent-login-record-{}/Missing.app",
            std::process::id()
        ));
        let uuid = [
            0x78, 0x56, 0x34, 0x12, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
        ];
        record.uuid = Some(uuid);
        let native = login_items::MissingLoginItem {
            id: 0x1234_5678,
            name: record.name.clone(),
        };
        assert_eq!(
            match_missing_item(
                &record,
                std::slice::from_ref(&record),
                std::slice::from_ref(&native)
            ),
            Some(native.clone())
        );
        assert!(match_missing_item(
            &record,
            &[record.clone(), record.clone()],
            std::slice::from_ref(&native)
        )
        .is_none());
        assert!(match_missing_item(
            &record,
            std::slice::from_ref(&record),
            &[native.clone(), native.clone()]
        )
        .is_none());
        let mut renamed = native.clone();
        renamed.name.push_str(" other");
        assert!(match_missing_item(&record, std::slice::from_ref(&record), &[renamed]).is_none());
        let mut wrong_id = native.clone();
        wrong_id.id += 1;
        assert!(match_missing_item(&record, std::slice::from_ref(&record), &[wrong_id]).is_none());
        record.uuid = None;
        assert!(match_missing_item(
            &record,
            std::slice::from_ref(&record),
            std::slice::from_ref(&native)
        )
        .is_none());
        record.uuid = Some(uuid);
        for path in [
            "/Volumes/Unavailable/Example.app",
            "/Network/Unavailable/Example.app",
            "relative/Example.app",
        ] {
            record.path = PathBuf::from(path);
            assert!(match_missing_item(
                &record,
                std::slice::from_ref(&record),
                std::slice::from_ref(&native)
            )
            .is_none());
        }
        record.path = std::env::temp_dir();
        assert!(match_missing_item(&record, std::slice::from_ref(&record), &[native]).is_none());
    }

    #[test]
    #[ignore = "requires a supported macOS background item database"]
    fn parses_the_installed_background_item_database() {
        let (version, path) = database_path().expect("a supported background item database");
        let archive = Value::from_file(path).expect("a readable background item database");
        let records = parse_archive(&archive, version, &PlatformCancellation::new(|| false))
            .expect("a supported background item schema");

        assert!(!records.is_empty());
    }
}
