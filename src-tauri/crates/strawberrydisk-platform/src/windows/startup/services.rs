use std::mem::size_of;
use std::path::Path;
use std::slice;
use std::time::Instant;

use windows::Win32::System::Services::{
    CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW, QueryServiceConfig2W,
    QueryServiceConfigW, ENUM_SERVICE_STATUS_PROCESSW, QUERY_SERVICE_CONFIGW, SC_ENUM_PROCESS_INFO,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_AUTO_START, SERVICE_CONFIG_DESCRIPTION,
    SERVICE_DESCRIPTIONW, SERVICE_DISABLED, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS,
    SERVICE_RUNNING, SERVICE_START_TYPE, SERVICE_STATE_ALL, SERVICE_WIN32,
};
use windows_core::PCWSTR;

use crate::{
    PlatformCancellation, PlatformStartupArtifact, PlatformStartupConfiguredState,
    PlatformStartupControlCapability, PlatformStartupCoverageReason, PlatformStartupCoverageStatus,
    PlatformStartupDiagnosticCode, PlatformStartupIdentityConfidence, PlatformStartupOwner,
    PlatformStartupRuntimeState, PlatformStartupScope, PlatformStartupSourceKind,
    PlatformStartupSourceResult, PlatformStartupSummarySource, PlatformStartupTarget,
    PlatformStartupTargetKind, PlatformStartupTrigger,
};

use super::metadata::{file_version_metadata, startup_trust, FilesystemTargetState};
use super::registry::normalized_path;

pub(super) fn scan(cancellation: &PlatformCancellation) -> PlatformStartupSourceResult {
    let started = Instant::now();
    let manager = match unsafe {
        OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE)
    } {
        Ok(manager) => manager,
        Err(error) => {
            return result(
                Vec::new(),
                PlatformStartupCoverageStatus::Unavailable,
                Some(coverage_reason(&error)),
                started,
            );
        }
    };
    let enumeration = enumerate_services(manager);
    let (services, mut partial_reason) = match enumeration {
        Ok(services) => (services, None),
        Err(reason) => {
            unsafe {
                let _ = CloseServiceHandle(manager);
            }
            return result(
                Vec::new(),
                PlatformStartupCoverageStatus::Unavailable,
                Some(reason),
                started,
            );
        }
    };
    let mut items = Vec::new();
    for service in services {
        if cancellation.is_cancelled() {
            unsafe {
                let _ = CloseServiceHandle(manager);
            }
            return result(
                items,
                PlatformStartupCoverageStatus::Cancelled,
                Some(PlatformStartupCoverageReason::Cancelled),
                started,
            );
        }
        match inspect_service(manager, &service) {
            Ok(Some(item)) => items.push(item),
            Ok(None) => {}
            Err(reason) => {
                partial_reason.get_or_insert(reason);
            }
        }
    }
    unsafe {
        let _ = CloseServiceHandle(manager);
    }
    log::info!(
        "windows_startup_service_controls item_count={} diagnostic_count={} protected_count={} view_only_count={}",
        items.len(),
        items.iter().filter(|item| !item.diagnostics.is_empty()).count(),
        items
            .iter()
            .filter(
                |item| item.control_capability == PlatformStartupControlCapability::SystemManaged
            )
            .count(),
        items
            .iter()
            .filter(|item| item.control_capability == PlatformStartupControlCapability::ViewOnly)
            .count(),
    );
    result(
        items,
        if partial_reason.is_some() {
            PlatformStartupCoverageStatus::Partial
        } else {
            PlatformStartupCoverageStatus::Complete
        },
        partial_reason,
        started,
    )
}

#[derive(Clone)]
pub(super) struct EnumeratedService {
    pub(super) name: String,
    pub(super) display_name: String,
    pub(super) running: bool,
}

fn enumerate_services(
    manager: windows::Win32::System::Services::SC_HANDLE,
) -> Result<Vec<EnumeratedService>, PlatformStartupCoverageReason> {
    let mut needed = 0;
    let mut returned = 0;
    unsafe {
        let _ = EnumServicesStatusExW(
            manager,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            None,
            &mut needed,
            &mut returned,
            None,
            PCWSTR::null(),
        );
    }
    if needed == 0 {
        return Ok(Vec::new());
    }
    let capacity = needed as usize / size_of::<ENUM_SERVICE_STATUS_PROCESSW>() + 1;
    let mut buffer = vec![ENUM_SERVICE_STATUS_PROCESSW::default(); capacity];
    let bytes = unsafe {
        slice::from_raw_parts_mut(
            buffer.as_mut_ptr().cast::<u8>(),
            buffer.len() * size_of::<ENUM_SERVICE_STATUS_PROCESSW>(),
        )
    };
    unsafe {
        EnumServicesStatusExW(
            manager,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            Some(bytes),
            &mut needed,
            &mut returned,
            None,
            PCWSTR::null(),
        )
        .map_err(|error| coverage_reason(&error))?;
    }
    Ok(buffer
        .into_iter()
        .take(returned as usize)
        .filter_map(|service| {
            let name = pwstr_string(service.lpServiceName)?;
            let display_name = pwstr_string(service.lpDisplayName).unwrap_or_else(|| name.clone());
            Some(EnumeratedService {
                name,
                display_name,
                running: service.ServiceStatusProcess.dwCurrentState == SERVICE_RUNNING,
            })
        })
        .collect())
}

fn inspect_service(
    manager: windows::Win32::System::Services::SC_HANDLE,
    service: &EnumeratedService,
) -> Result<Option<PlatformStartupArtifact>, PlatformStartupCoverageReason> {
    let service_name = windows_core::HSTRING::from(&service.name);
    let handle = unsafe {
        OpenServiceW(
            manager,
            &service_name,
            SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS,
        )
        .map_err(|error| coverage_reason(&error))?
    };
    let config = query_service_config(handle, &service.name);
    let description = query_service_description(handle);
    unsafe {
        let _ = CloseServiceHandle(handle);
    }
    let config = config?;
    if config.start_type != SERVICE_AUTO_START && config.start_type != SERVICE_DISABLED {
        return Ok(None);
    }
    Ok(Some(artifact_from_config(service, &config, description)))
}

pub(super) fn artifact_from_config(
    service: &EnumeratedService,
    config: &ServiceConfig,
    description: Option<String>,
) -> PlatformStartupArtifact {
    let resolved = super::service_target::resolve(&config.binary_path);
    let target_path = resolved.path;
    let mut diagnostics = Vec::new();
    if target_path.is_none() {
        diagnostics.push(PlatformStartupDiagnosticCode::InvalidData);
    } else if let Some(diagnostic) = target_diagnostic(resolved.state, service.running) {
        diagnostics.push(diagnostic);
    }
    // Normal target resolution is scan detail; keep repeated preflight scans out of INFO.
    // Quoted names remain readable while escaping control characters in third-party metadata.
    log::debug!("windows_startup_service_target service_name={:?} resolution={} target_state={:?} running={} diagnostic={:?}", service.name, resolved.resolution, resolved.state, service.running, diagnostics.first());
    let system_root = super::super::directories::system_directory().ok();
    let system_item = target_path.as_deref().is_some_and(|path| {
        system_root.as_deref().is_some_and(|root| {
            normalized_path(path).starts_with(&format!(
                "{}\\",
                normalized_path(root).trim_end_matches('\\')
            ))
        })
    });
    // Service names are SCM identities. Executable paths are not ownership
    // evidence: unrelated services can share a host or an unresolved prefix.
    let target_identity = format!("service:{}", service.name.to_lowercase());
    let version_metadata = target_path
        .as_deref()
        .and_then(file_version_metadata)
        .unwrap_or_default();
    let service_description_available = description.is_some();
    let summary = description.or(version_metadata.description);
    PlatformStartupArtifact {
        provider_item_id: format!("service:{}", service.name.to_lowercase()),
        source_kind: PlatformStartupSourceKind::Service,
        scope: PlatformStartupScope::Machine,
        triggers: vec![PlatformStartupTrigger::Boot],
        display_name: service.display_name.clone(),
        configuration_path: None,
        target: PlatformStartupTarget {
            kind: PlatformStartupTargetKind::Service,
            identity_key: target_identity.clone(),
            path: target_path.clone(),
            executable_name: target_path
                .as_deref()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned),
            arguments: resolved.arguments,
        },
        owner: PlatformStartupOwner {
            identity_key: Some(target_identity),
            // SCM supplies the service's actual display name. PE product names
            // often identify only Windows or a shared host, obscuring distinct services.
            name: Some(service.display_name.clone()),
            publisher: version_metadata.company_name,
            summary: summary.clone(),
            summary_source: if service_description_available {
                PlatformStartupSummarySource::ServiceDescription
            } else if summary.is_some() {
                PlatformStartupSummarySource::VersionInfo
            } else {
                PlatformStartupSummarySource::SourceLabel
            },
            version: version_metadata.product_version,
            icon_path: target_path.clone().filter(|path| path.exists()),
            confidence: PlatformStartupIdentityConfidence::Strong,
        },
        configured_state: if config.start_type == SERVICE_AUTO_START {
            PlatformStartupConfiguredState::Enabled
        } else {
            PlatformStartupConfiguredState::Disabled
        },
        runtime_state: if service.running {
            PlatformStartupRuntimeState::Running
        } else {
            PlatformStartupRuntimeState::Stopped
        },
        control_capability: service_control_capability(
            config,
            system_item || system_root.is_none() || target_path.is_none(),
        ),
        trust: startup_trust(target_path.as_deref(), system_item),
        modified_at_ms: super::service_control::configuration_modified_at(&service.name),
        diagnostics,
    }
}

fn target_diagnostic(
    state: FilesystemTargetState,
    running: bool,
) -> Option<PlatformStartupDiagnosticCode> {
    match state {
        FilesystemTargetState::Present => None,
        FilesystemTargetState::Missing if !running => {
            Some(PlatformStartupDiagnosticCode::MissingTarget)
        }
        // A running service contradicts a simple leftover diagnosis. Retain
        // that uncertainty until its executable can be verified independently.
        _ => Some(PlatformStartupDiagnosticCode::StateUnavailable),
    }
}

#[derive(Clone)]
pub(super) struct ServiceConfig {
    pub(super) start_type: SERVICE_START_TYPE,
    pub(super) binary_path: String,
    pub(super) delayed: Option<u32>,
    pub(super) protection: Option<u32>,
}

pub(super) fn query_service_config(
    service: windows::Win32::System::Services::SC_HANDLE,
    name: &str,
) -> Result<ServiceConfig, PlatformStartupCoverageReason> {
    let mut needed = 0;
    unsafe {
        let _ = QueryServiceConfigW(service, None, 0, &mut needed);
    }
    if needed < size_of::<QUERY_SERVICE_CONFIGW>() as u32 {
        return Err(PlatformStartupCoverageReason::InvalidData);
    }
    let word_count = needed as usize / size_of::<usize>() + 1;
    let mut buffer = vec![0usize; word_count];
    unsafe {
        QueryServiceConfigW(
            service,
            Some(buffer.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>()),
            (buffer.len() * size_of::<usize>()) as u32,
            &mut needed,
        )
        .map_err(|error| coverage_reason(&error))?;
    }
    let config = unsafe { &*buffer.as_ptr().cast::<QUERY_SERVICE_CONFIGW>() };
    Ok(ServiceConfig {
        start_type: config.dwStartType,
        binary_path: pwstr_string(config.lpBinaryPathName).unwrap_or_default(),
        delayed: query_config_dword(
            service,
            name,
            windows::Win32::System::Services::SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
        ),
        protection: query_config_dword(
            service,
            name,
            windows::Win32::System::Services::SERVICE_CONFIG_LAUNCH_PROTECTED,
        ),
    })
}

fn service_control_capability(
    config: &ServiceConfig,
    system_item: bool,
) -> PlatformStartupControlCapability {
    // A confirmed launch-protection level is a system-managed boundary, not
    // missing elevation. Keep unknown protection read-only without claiming it is protected.
    if config.protection.is_some_and(|level| level != 0) {
        return PlatformStartupControlCapability::SystemManaged;
    }
    // Unknown protection/configuration must not become authority to mutate a service.
    if !system_item
        && config.protection == Some(0)
        && config.delayed.is_some()
        && !config.binary_path.trim().is_empty()
        && matches!(config.start_type, SERVICE_AUTO_START | SERVICE_DISABLED)
    {
        PlatformStartupControlCapability::ElevationRequired
    } else {
        PlatformStartupControlCapability::ViewOnly
    }
}

fn query_config_dword(
    service: windows::Win32::System::Services::SC_HANDLE,
    name: &str,
    level: windows::Win32::System::Services::SERVICE_CONFIG,
) -> Option<u32> {
    // Both queried structures contain one DWORD; preserve its native alignment.
    let mut value = 0u32;
    let bytes = unsafe {
        slice::from_raw_parts_mut((&mut value as *mut u32).cast::<u8>(), size_of::<u32>())
    };
    let mut needed = 0;
    if let Err(error) = unsafe { QueryServiceConfig2W(service, level, Some(bytes), &mut needed) } {
        // The service name identifies the failed query without exposing its executable path.
        log::warn!(
            "windows_startup_service_config_query_failed service_name={:?} config_level={} hresult={:08x}",
            name,
            level.0,
            error.code().0 as u32
        );
        return None;
    }
    Some(value)
}

fn query_service_description(
    service: windows::Win32::System::Services::SC_HANDLE,
) -> Option<String> {
    let mut needed = 0;
    unsafe {
        let _ = QueryServiceConfig2W(service, SERVICE_CONFIG_DESCRIPTION, None, &mut needed);
    }
    if needed < size_of::<SERVICE_DESCRIPTIONW>() as u32 {
        return None;
    }
    let word_count = needed as usize / size_of::<usize>() + 1;
    let mut buffer = vec![0usize; word_count];
    let bytes = unsafe {
        slice::from_raw_parts_mut(
            buffer.as_mut_ptr().cast::<u8>(),
            buffer.len() * size_of::<usize>(),
        )
    };
    unsafe {
        QueryServiceConfig2W(
            service,
            SERVICE_CONFIG_DESCRIPTION,
            Some(bytes),
            &mut needed,
        )
        .ok()?;
    }
    let description = unsafe { &*buffer.as_ptr().cast::<SERVICE_DESCRIPTIONW>() };
    pwstr_string(description.lpDescription).filter(|value| !value.trim().is_empty())
}

fn pwstr_string(value: windows_core::PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let mut length = 0;
    unsafe {
        while *value.0.add(length) != 0 {
            length += 1;
        }
        Some(String::from_utf16_lossy(slice::from_raw_parts(
            value.0, length,
        )))
    }
}

fn coverage_reason(error: &windows_core::Error) -> PlatformStartupCoverageReason {
    if error.code().0 as u32 == 0x8007_0005 {
        PlatformStartupCoverageReason::AccessDenied
    } else {
        PlatformStartupCoverageReason::ApiUnavailable
    }
}

fn result(
    items: Vec<PlatformStartupArtifact>,
    status: PlatformStartupCoverageStatus,
    reason: Option<PlatformStartupCoverageReason>,
    started: Instant,
) -> PlatformStartupSourceResult {
    PlatformStartupSourceResult {
        source_id: "windows.services".to_string(),
        required: true,
        status,
        reason,
        items,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_native_config_queries_keep_services_read_only() {
        // An invalid handle exercises the real error path without changing any
        // installed service. Neither failed query may become a writable default.
        let service = windows::Win32::System::Services::SC_HANDLE::default();
        for level in [
            windows::Win32::System::Services::SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            windows::Win32::System::Services::SERVICE_CONFIG_LAUNCH_PROTECTED,
        ] {
            assert_eq!(query_config_dword(service, "fixture-service", level), None);
        }
        for (delayed, protection) in [(None, Some(0)), (Some(0), None), (None, None)] {
            let config = ServiceConfig {
                start_type: SERVICE_AUTO_START,
                binary_path: r"C:\Fixture\service.exe".to_owned(),
                delayed,
                protection,
            };
            assert_eq!(
                service_control_capability(&config, false),
                PlatformStartupControlCapability::ViewOnly
            );
        }
    }

    #[test]
    fn running_or_inaccessible_services_are_not_reported_as_orphans() {
        assert_eq!(
            target_diagnostic(FilesystemTargetState::Missing, false),
            Some(PlatformStartupDiagnosticCode::MissingTarget)
        );
        assert_eq!(
            target_diagnostic(FilesystemTargetState::Missing, true),
            Some(PlatformStartupDiagnosticCode::StateUnavailable)
        );
        assert_eq!(
            target_diagnostic(FilesystemTargetState::Unknown, false),
            Some(PlatformStartupDiagnosticCode::StateUnavailable)
        );
        assert_eq!(
            target_diagnostic(FilesystemTargetState::Present, true),
            None
        );
    }

    #[test]
    fn services_with_shared_executables_retain_independent_owner_identities() {
        let config = ServiceConfig {
            start_type: SERVICE_AUTO_START,
            binary_path: r"C:\StrawberryDisk Test Fixture\missing.exe".to_string(),
            delayed: Some(0),
            protection: Some(0),
        };
        let artifacts = ["fixture-one", "fixture-two"].map(|name| {
            artifact_from_config(
                &EnumeratedService {
                    name: name.to_string(),
                    display_name: name.to_string(),
                    running: false,
                },
                &config,
                None,
            )
        });
        assert_ne!(
            artifacts[0].owner.identity_key,
            artifacts[1].owner.identity_key
        );
        assert_eq!(artifacts[0].target.path, artifacts[1].target.path);
        assert_eq!(artifacts[0].owner.name.as_deref(), Some("fixture-one"));
        assert_eq!(artifacts[1].owner.name.as_deref(), Some("fixture-two"));
        assert_eq!(
            artifacts[0].target.path.as_deref(),
            Some(Path::new(r"C:\StrawberryDisk Test Fixture\missing.exe"))
        );
    }

    #[test]
    fn service_display_name_takes_priority_over_shared_windows_product_metadata() {
        let target = super::super::super::directories::system_directory()
            .expect("Windows system directory must be available")
            .join(r"System32\kernel32.dll");
        let metadata =
            file_version_metadata(&target).expect("kernel32 must expose version metadata");
        assert!(metadata.product_name.is_some());
        let item = artifact_from_config(
            &EnumeratedService {
                name: "fixture-service".into(),
                display_name: "Distinct service name".into(),
                running: false,
            },
            &ServiceConfig {
                start_type: SERVICE_AUTO_START,
                binary_path: format!("\"{}\"", target.display()),
                delayed: Some(0),
                protection: Some(3),
            },
            None,
        );
        assert_eq!(item.owner.name.as_deref(), Some("Distinct service name"));
        assert_eq!(
            item.control_capability,
            PlatformStartupControlCapability::SystemManaged
        );
    }

    #[test]
    fn service_controls_require_known_unprotected_third_party_configuration() {
        let automatic = ServiceConfig {
            start_type: SERVICE_AUTO_START,
            binary_path: r"C:\Fixture\service.exe".to_owned(),
            delayed: Some(0),
            protection: Some(0),
        };

        assert_eq!(
            service_control_capability(&automatic, true),
            PlatformStartupControlCapability::ViewOnly
        );
        assert_eq!(
            service_control_capability(&automatic, false),
            PlatformStartupControlCapability::ElevationRequired
        );
        for protection in [None, Some(1), Some(2), Some(3)] {
            let protected = ServiceConfig {
                protection,
                ..automatic.clone()
            };
            assert_eq!(
                service_control_capability(&protected, false),
                if protection.is_some() {
                    PlatformStartupControlCapability::SystemManaged
                } else {
                    PlatformStartupControlCapability::ViewOnly
                }
            );
        }
    }
}
