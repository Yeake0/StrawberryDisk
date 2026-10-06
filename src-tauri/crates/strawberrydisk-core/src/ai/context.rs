use serde::{Deserialize, Serialize};

use super::AiError;
use crate::applications::uninstall::{
    ApplicationSystemKind, ApplicationUninstallCapability, ApplicationUninstallComponentKind,
    ApplicationUninstallExecutionMode, ApplicationUninstallInstallerKind,
    ApplicationUninstallInventorySource, ApplicationUninstallPlatform,
    ApplicationUninstallRecordState, ApplicationUninstallRisk,
};
use crate::cleanup::{CleanupSourceDetail, RiskLevel, ScanItemStatus};
use crate::privacy::{
    PrivacyCapabilityState, PrivacyDataKind, PrivacyImpact, PrivacyRecommendation, PrivacyTimeRange,
};
use crate::startup::{
    StartupConfiguredState, StartupControlCapability, StartupDiagnosticCode, StartupRuntimeState,
    StartupSourceKind, StartupTrigger, StartupTrustState,
};
use crate::storage::duplicates::{DuplicateEntryDeletePolicy, DuplicateGroupKind};
use crate::system_maintenance::{SystemMaintenanceRiskLevel, SystemMaintenanceStatus};
use crate::system_settings::{
    SystemSettingRiskLevel, SystemSettingSelectionKind, SystemSettingStatus,
    SystemSettingTargetState,
};
use strawberrydisk_platform::{
    PlatformSystemMaintenanceDiagnosticCode, PlatformSystemSettingDiagnosticCode,
};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AiPlatform {
    Macos,
    Windows,
    Linux,
    Unknown,
}

/// Transient IPC schema. Older preview contexts are rejected, not migrated;
/// provider configuration has its own independent persisted schema.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiContext {
    pub schema_version: u8,
    pub platform: AiPlatform,
    pub title: String,
    pub description: String,
    pub subject: AiSubject,
}

/// Domain enum types are reused, but operational objects and their private fields
/// never cross this allowlist. The subject tag also selects the module prompt.
#[derive(Clone, Deserialize, Serialize)]
#[serde(
    tag = "module",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AiSubject {
    ApplicationUninstall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity: Option<strawberrydisk_platform::ApplicationIdentityMetadata>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        installation_sources: Vec<ApplicationUninstallInventorySource>,
        platform: ApplicationUninstallPlatform,
        publisher: Option<String>,
        version: Option<String>,
        application_path: Option<String>,
        capability: ApplicationUninstallCapability,
        record_state: ApplicationUninstallRecordState,
        system_kind: ApplicationSystemKind,
        installer_kind: Option<ApplicationUninstallInstallerKind>,
        execution_mode: Option<ApplicationUninstallExecutionMode>,
        associated_data_complete: bool,
        execution_supported: bool,
        catalog_actionable: bool,
        record_removal_available: bool,
        selection_kind: AiUninstallSelectionKind,
        components: Vec<AiUninstallComponent>,
    },
    LargeFiles {
        file: AiFileMetadata,
    },
    DuplicateFiles {
        kind: DuplicateGroupKind,
        target: AiDuplicateEntry,
        other_copies: Vec<AiDuplicateEntry>,
        omitted_count: u64,
    },
    Cleanup {
        impact: String,
        bytes: u64,
        item_count: u64,
        requires_app_close: bool,
        scan: AiCleanupScan,
    },
    Privacy {
        kind: PrivacyDataKind,
        impact: PrivacyImpact,
        capability: PrivacyCapabilityState,
        recommendation: PrivacyRecommendation,
        time_range: PrivacyTimeRange,
        item_count: u64,
        estimated_bytes: u64,
        requires_browser_close: bool,
        synchronization_may_propagate: bool,
    },
    Startup {
        entries: Vec<AiStartupEntry>,
        omitted_count: u64,
    },
    SystemOptimization {
        status: SystemSettingStatus,
        selection_kind: SystemSettingSelectionKind,
        diagnostic: Option<PlatformSystemSettingDiagnosticCode>,
        risk_level: SystemSettingRiskLevel,
        has_recorded_original_value: bool,
        requires_restart: bool,
        requires_elevation: bool,
        pending_target: Option<SystemSettingTargetState>,
    },
    SystemMaintenance {
        task_id: String,
        status: SystemMaintenanceStatus,
        risk_level: SystemMaintenanceRiskLevel,
        requires_restart: bool,
        requires_elevation: bool,
        estimated_duration_seconds: u64,
        diagnostic: Option<PlatformSystemMaintenanceDiagnosticCode>,
    },
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AiUninstallSelectionKind {
    Default,
    Current,
}

/// Aggregated descriptive scope; selection is an unapplied UI draft, not an action permit.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiUninstallComponent {
    pub kind: ApplicationUninstallComponentKind,
    pub risk: ApplicationUninstallRisk,
    pub bytes: u64,
    pub selected: bool,
}

/// Only descriptive metadata enters a provider request, never file contents,
/// duplicate proof tokens, scan handles, or executable actions.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiFileMetadata {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub modified_at_ms: Option<u64>,
}

impl AiFileMetadata {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 32768
            && !self.path.trim().is_empty()
            && self.path.len() <= 32768
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiDuplicateEntry {
    pub file: AiFileMetadata,
    pub delete_policy: DuplicateEntryDeletePolicy,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiCleanupScan {
    pub rule_id: String,
    pub risk: RiskLevel,
    pub status: ScanItemStatus,
    pub available: bool,
    pub selectable: bool,
    pub running_processes: Vec<String>,
    pub sources: Vec<CleanupSourceDetail>,
    pub source_count: u64,
    pub sources_truncated: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiStartupEntry {
    pub name: String,
    pub identity: AiStartupIdentity,
    pub source_kind: StartupSourceKind,
    pub triggers: Vec<StartupTrigger>,
    pub configured_state: StartupConfiguredState,
    pub runtime_state: StartupRuntimeState,
    pub control_capability: StartupControlCapability,
    pub diagnostics: Vec<StartupDiagnosticCode>,
    pub removal_supported: bool,
}

/// Preserve original software metadata for attribution. These are untrusted
/// descriptions, never instructions, and must not enter diagnostic logs.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiStartupIdentity {
    pub application_name: String,
    pub publisher: String,
    pub executable_name: String,
    pub executable_path: String,
    pub configuration_path: String,
    pub description: String,
    pub version: String,
    pub trust: StartupTrustState,
}

impl AiStartupIdentity {
    fn valid(&self) -> bool {
        [
            &self.application_name,
            &self.publisher,
            &self.executable_name,
            &self.executable_path,
            &self.configuration_path,
            &self.description,
            &self.version,
        ]
        .into_iter()
        .all(|value| value.len() <= 32768)
    }
}

impl AiSubject {
    pub fn module_name(&self) -> &'static str {
        match self {
            Self::ApplicationUninstall { .. } => "applicationUninstall",
            Self::LargeFiles { .. } => "largeFiles",
            Self::DuplicateFiles { .. } => "duplicateFiles",
            Self::Cleanup { .. } => "cleanup",
            Self::Privacy { .. } => "privacy",
            Self::Startup { .. } => "startup",
            Self::SystemOptimization { .. } => "systemOptimization",
            Self::SystemMaintenance { .. } => "systemMaintenance",
        }
    }
}

impl AiContext {
    /// Provider text is not the versioned IPC document. Omit only absent descriptive
    /// metadata; false/zero values, empty inventories and null drafts remain facts.
    pub(super) fn provider_json(&self) -> Result<String, AiError> {
        fn omit_absent_metadata(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(fields) => {
                    fields.retain(|key, value| {
                        value.as_str() != Some("")
                            && !(value.is_null()
                                && matches!(
                                    key.as_str(),
                                    "diagnostic" | "modifiedAtMs" | "blockReason"
                                ))
                            && !(key == "diagnostics"
                                && value.as_array().is_some_and(Vec::is_empty))
                    });
                    for value in fields.values_mut() {
                        omit_absent_metadata(value);
                    }
                }
                serde_json::Value::Array(values) => {
                    for value in values {
                        omit_absent_metadata(value);
                    }
                }
                _ => {}
            }
        }
        let mut value = serde_json::to_value(self).map_err(|_| AiError::InvalidContext)?;
        value.as_object_mut().unwrap().remove("schemaVersion");
        omit_absent_metadata(&mut value);
        serde_json::to_string(&value).map_err(|_| AiError::InvalidContext)
    }

    pub(super) fn validate(&self) -> Result<(), AiError> {
        let invalid_subject =
            match &self.subject {
                AiSubject::ApplicationUninstall {
                    identity,
                    installation_sources,
                    platform,
                    publisher,
                    version,
                    application_path,
                    components,
                    ..
                } => [publisher, version, application_path]
                    .into_iter()
                    .flatten()
                    .any(|value| value.len() > 32768)
                    || components.len() > 54
                    || installation_sources.len() > 12
                    || identity.as_ref().is_some_and(|identity| {
                        let platform_matches = matches!(
                            (platform, identity),
                            (
                                ApplicationUninstallPlatform::MacosBundle,
                                strawberrydisk_platform::ApplicationIdentityMetadata::Macos { .. }
                            ) | (
                                ApplicationUninstallPlatform::WindowsRegistry,
                                strawberrydisk_platform::ApplicationIdentityMetadata::Windows { .. }
                            )
                        );
                        !platform_matches
                            || serde_json::to_vec(identity).map_or(true, |json| json.len() > 8192)
                    }),
                AiSubject::LargeFiles { file } => !file.valid(),
                AiSubject::DuplicateFiles {
                    target,
                    other_copies,
                    ..
                } => {
                    !target.file.valid()
                        || other_copies.is_empty()
                        || other_copies.len() > 31
                        || other_copies.iter().any(|copy| !copy.file.valid())
                        || {
                            let mut paths = std::collections::HashSet::from([&target.file.path]);
                            other_copies
                                .iter()
                                .any(|copy| !paths.insert(&copy.file.path))
                        }
                }
                AiSubject::Cleanup { impact, scan, .. } => {
                    impact.len() > 32768
                        || scan.rule_id.len() > 1024
                        || scan.sources.len() > 256
                        || scan.running_processes.len() > 256
                        || scan.running_processes.iter().any(|name| name.len() > 32768)
                        || scan.sources.iter().any(|source| source.path.len() > 32768)
                }
                AiSubject::Startup { entries, .. } => {
                    entries.is_empty()
                        || entries.len() > 256
                        || entries.iter().any(|entry| {
                            entry.name.trim().is_empty()
                                || entry.name.len() > 32768
                                || entry.triggers.len() > 8
                                || entry.diagnostics.len() > 6
                                || !entry.identity.valid()
                        })
                }
                AiSubject::SystemMaintenance { task_id, .. } => {
                    task_id.is_empty() || task_id.len() > 256
                }
                _ => false,
            };
        if self.schema_version != 2
            || self.title.trim().is_empty()
            || self.title.len() > 32768
            || self.description.len() > 2048
            || invalid_subject
        {
            return Err(AiError::InvalidContext);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_metadata_is_optional_bounded_and_platform_specific() {
        let mut fixture = fixtures().remove(7);
        let identity = serde_json::json!({"platform":"macos", "bundleIdentifier":"com.example.game", "category":"public.app-category.games", "signing":{"kind":"unavailable"}});
        fixture["subject"]["identity"] = identity.clone();
        let context: AiContext = serde_json::from_value(fixture.clone()).unwrap();
        context.validate().unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&context.provider_json().unwrap()).unwrap();
        assert_eq!(
            json["subject"]["identity"]["category"],
            "public.app-category.games"
        );
        assert!(json["subject"]["identity"].get("productName").is_none());
        fixture["subject"]["platform"] = serde_json::json!("windowsRegistry");
        assert!(serde_json::from_value::<AiContext>(fixture.clone())
            .unwrap()
            .validate()
            .is_err());
        fixture["subject"]["identity"] =
            serde_json::json!({"platform":"windows", "productName":"Example"});
        serde_json::from_value::<AiContext>(fixture.clone())
            .unwrap()
            .validate()
            .unwrap();
        fixture["subject"]["identity"]["bundleIdentifier"] =
            serde_json::json!("not a Windows field");
        assert!(serde_json::from_value::<AiContext>(fixture.clone()).is_err());
        fixture["subject"]["identity"] =
            serde_json::json!({"platform":"windows", "fileDescription":"x".repeat(8193)});
        assert!(serde_json::from_value::<AiContext>(fixture)
            .unwrap()
            .validate()
            .is_err());
    }

    #[test]
    fn uninstall_context_preserves_scope_and_rejects_operational_metadata() {
        let fixture = fixtures().remove(7);
        let context: AiContext = serde_json::from_value(fixture.clone()).unwrap();
        context.validate().unwrap();
        let provider: serde_json::Value =
            serde_json::from_str(&context.provider_json().unwrap()).unwrap();
        assert_eq!(
            provider["subject"]["components"],
            fixture["subject"]["components"]
        );
        assert_eq!(provider["subject"]["associatedDataComplete"], false);
        assert_eq!(provider["subject"]["recordRemovalAvailable"], false);
        for field in ["uninstallCommand", "applicationId", "sourceIdentities"] {
            let mut invalid = fixture.clone();
            invalid["subject"][field] = serde_json::json!("private");
            assert!(serde_json::from_value::<AiContext>(invalid).is_err());
        }
        let mut invalid = fixture.clone();
        invalid["subject"]["components"][0]["path"] = serde_json::json!("private data path");
        assert!(serde_json::from_value::<AiContext>(invalid).is_err());
        let mut invalid = fixture.clone();
        invalid["subject"]["capability"] = serde_json::json!("invented");
        assert!(serde_json::from_value::<AiContext>(invalid).is_err());
        let mut oversized = fixture;
        oversized["subject"]["components"] =
            serde_json::json!(vec![oversized["subject"]["components"][0].clone(); 55]);
        assert!(serde_json::from_value::<AiContext>(oversized)
            .unwrap()
            .validate()
            .is_err());
    }
    pub(super) fn fixtures() -> Vec<serde_json::Value> {
        serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/ai-context-v2.json"
        ))
        .unwrap()
    }

    #[test]
    fn provider_context_preserves_decision_facts_and_original_metadata() {
        let mut fixture = fixtures().remove(3);
        fixture["subject"]["pendingTarget"] = serde_json::Value::Null;
        let context: AiContext = serde_json::from_value(fixture).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&context.provider_json().unwrap()).unwrap();
        assert_eq!(value["subject"]["pendingTarget"], serde_json::Value::Null);
        assert!(value["subject"].get("pendingTarget").is_some());
        assert_eq!(value["subject"]["hasRecordedOriginalValue"], false);
        assert_eq!(value["subject"]["requiresRestart"], true);
        assert!(value.get("schemaVersion").is_none());
        assert!(value["subject"].get("diagnostic").is_none());

        let context: AiContext = serde_json::from_value(fixtures().remove(0)).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&context.provider_json().unwrap()).unwrap();
        assert_eq!(value["subject"]["scan"]["sourceCount"], 0);
        assert_eq!(value["subject"]["scan"]["sources"], serde_json::json!([]));
        assert_eq!(
            value["subject"]["scan"]["runningProcesses"],
            serde_json::json!([])
        );
        assert_eq!(value["subject"]["scan"]["sourcesTruncated"], false);

        let mut fixture = fixtures().remove(6);
        fixture["subject"]["target"]["file"]["modifiedAtMs"] = serde_json::json!(123);
        let context: AiContext = serde_json::from_value(fixture.clone()).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&context.provider_json().unwrap()).unwrap();
        assert_eq!(value["subject"]["target"], fixture["subject"]["target"]);
        assert_eq!(
            value["subject"]["otherCopies"][0]["deletePolicy"],
            "protected"
        );
        assert_eq!(
            value["subject"]["otherCopies"][0]["file"]["path"],
            fixture["subject"]["otherCopies"][0]["file"]["path"]
        );
        assert_eq!(value["subject"]["omittedCount"], 0);

        let fixture = fixtures().remove(2);
        let context: AiContext = serde_json::from_value(fixture.clone()).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&context.provider_json().unwrap()).unwrap();
        let identity = &value["subject"]["entries"][0]["identity"];
        assert!(identity.get("publisher").is_none());
        assert_eq!(identity["trust"], "unknown");
        assert_eq!(
            identity["executablePath"],
            fixture["subject"]["entries"][0]["identity"]["executablePath"]
        );
        assert_eq!(
            value["subject"]["entries"][0]["configuredState"],
            "disabled"
        );
        assert_eq!(value["subject"]["entries"][0]["runtimeState"], "running");
        assert_eq!(serde_json::to_value(context).unwrap(), fixture);
    }

    #[test]
    fn frontend_contexts_round_trip_with_explicit_metadata_fields() {
        for fixture in fixtures() {
            let context: AiContext = serde_json::from_value(fixture.clone()).unwrap();
            context.validate().unwrap();
            assert_eq!(serde_json::to_value(&context).unwrap(), fixture);
            let mut invalid = fixture.clone();
            invalid["subject"]["command"] = serde_json::json!("/private/command");
            assert!(serde_json::from_value::<AiContext>(invalid).is_err());
            let mut invalid = fixture;
            invalid["path"] = serde_json::json!("/private/path");
            assert!(serde_json::from_value::<AiContext>(invalid).is_err());
        }
    }

    #[test]
    fn file_contexts_reject_private_payloads_invalid_targets_and_unbounded_copy_lists() {
        let fixtures = fixtures();
        for index in [5, 6] {
            let mut invalid = fixtures[index].clone();
            let file = if index == 5 {
                &mut invalid["subject"]["file"]
            } else {
                &mut invalid["subject"]["target"]["file"]
            };
            file["content"] = serde_json::json!("private document content");
            assert!(serde_json::from_value::<AiContext>(invalid).is_err());
        }
        let mut invalid = fixtures[5].clone();
        invalid["subject"]["file"]["path"] = serde_json::json!("");
        assert!(serde_json::from_value::<AiContext>(invalid)
            .unwrap()
            .validate()
            .is_err());
        let duplicate = &fixtures[6];
        let mut invalid = duplicate.clone();
        invalid["subject"]["otherCopies"] = serde_json::json!([]);
        assert!(serde_json::from_value::<AiContext>(invalid)
            .unwrap()
            .validate()
            .is_err());
        let mut invalid = duplicate.clone();
        invalid["subject"]["otherCopies"][0]["file"]["path"] =
            invalid["subject"]["target"]["file"]["path"].clone();
        assert!(serde_json::from_value::<AiContext>(invalid)
            .unwrap()
            .validate()
            .is_err());
        let mut invalid = duplicate.clone();
        invalid["subject"]["target"]["deletePolicy"] = serde_json::json!("allowed-by-ai");
        assert!(serde_json::from_value::<AiContext>(invalid).is_err());
        let mut invalid = duplicate.clone();
        let copy = &duplicate["subject"]["otherCopies"][0];
        invalid["subject"]["otherCopies"] = serde_json::json!((0..32)
            .map(|index| {
                let mut copy = copy.clone();
                copy["file"]["path"] = serde_json::json!(format!("/copy-{index}/report.pdf"));
                copy
            })
            .collect::<Vec<_>>());
        assert!(serde_json::from_value::<AiContext>(invalid)
            .unwrap()
            .validate()
            .is_err());
    }

    #[test]
    fn startup_identity_preserves_paths_but_rejects_unbounded_or_execution_fields() {
        let mut fixture = fixtures().remove(2);
        fixture["subject"]["entries"][0]["identity"]["executablePath"] =
            serde_json::json!("C:\\Users\\Example\\Applications\\Agent.exe");
        let context: AiContext = serde_json::from_value(fixture.clone()).unwrap();
        context.validate().unwrap();
        assert_eq!(serde_json::to_value(context).unwrap(), fixture);
        let mut oversized = fixture.clone();
        oversized["subject"]["entries"][0]["identity"]["description"] =
            serde_json::json!("x".repeat(32769));
        assert_eq!(
            serde_json::from_value::<AiContext>(oversized)
                .unwrap()
                .validate(),
            Err(AiError::InvalidContext)
        );
        fixture["subject"]["entries"][0]["identity"]["arguments"] =
            serde_json::json!(["--secret=value"]);
        assert!(serde_json::from_value::<AiContext>(fixture).is_err());
    }

    #[test]
    fn cleanup_sources_round_trip_without_silent_redaction_or_truncation() {
        let mut fixture = fixtures().remove(0);
        fixture["subject"]["scan"]["sources"] = serde_json::json!([{
            "path": "C:\\Users\\Example\\Application Data\\Vendor\\cache.bin",
            "bytes": 128,
            "fileCount": 1,
            "modifiedAtMs": null,
            "blockReason": "requiresClose"
        }]);
        fixture["subject"]["scan"]["sourceCount"] = serde_json::json!(300);
        fixture["subject"]["scan"]["sourcesTruncated"] = serde_json::json!(true);
        let mut context: AiContext = serde_json::from_value(fixture.clone()).unwrap();
        context.validate().unwrap();
        assert_eq!(serde_json::to_value(&context).unwrap(), fixture);
        if let AiSubject::Cleanup { scan, .. } = &mut context.subject {
            scan.sources = vec![scan.sources[0].clone(); 257];
        }
        assert_eq!(context.validate(), Err(AiError::InvalidContext));
    }

    #[test]
    fn unsupported_schema_empty_titles_invalid_enums_and_unbounded_groups_are_rejected() {
        let fixture = fixtures().remove(2);
        let mut context: AiContext = serde_json::from_value(fixture.clone()).unwrap();
        context.schema_version = 1;
        assert_eq!(context.validate(), Err(AiError::InvalidContext));
        context.schema_version = 2;
        context.title = " ".into();
        assert_eq!(context.validate(), Err(AiError::InvalidContext));
        context.title = "Fixture".into();
        if let AiSubject::Startup { entries, .. } = &mut context.subject {
            *entries = vec![entries[0].clone(); 257];
        }
        assert_eq!(context.validate(), Err(AiError::InvalidContext));
        let mut invalid = fixture;
        invalid["subject"]["entries"][0]["controlCapability"] = serde_json::json!("freelyMutable");
        assert!(serde_json::from_value::<AiContext>(invalid).is_err());
    }
}
