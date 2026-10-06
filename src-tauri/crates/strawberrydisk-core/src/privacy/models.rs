use serde::{Deserialize, Serialize};

use crate::ApplicationCloseMode;

pub const PRIVACY_SCAN_SCHEMA_VERSION: u32 = 6;
pub const PRIVACY_PLAN_SCHEMA_VERSION: u32 = 2;
pub const PRIVACY_DETAILS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyScanStage {
    Discovering,
    Browser,
    Application,
    System,
    Finalizing,
}

/// Identifies the actual source currently inspected without exposing any private record content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyScanProgress {
    pub stage: PrivacyScanStage,
    pub source_name: Option<String>,
    pub completed_sources: u64,
    pub total_sources: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyTimeRange {
    LastHour,
    Today,
    LastSevenDays,
    AllTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyCategory {
    BrowserActivity,
    BrowserAccountState,
    ApplicationActivity,
    SystemActivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacySensitivity {
    Activity,
    ContentDerived,
    AccountState,
    PersonalContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyImpact {
    Low,
    Workflow,
    SignOut,
    CrossDevice,
    DataLoss,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyRecommendation {
    Recommended,
    Manual,
    ReviewOnly,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyCapabilityState {
    Ready,
    Empty,
    BrowserRunning,
    ApplicationRunning,
    PermissionRequired,
    Unsupported,
    SchemaUnsupported,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyDataKind {
    BrowsingHistory,
    DownloadHistory,
    Cookies,
    SiteStorage,
    SitePermissions,
    Sessions,
    BrowserCache,
    SearchHistory,
    WebsiteIcons,
    FrequentlyVisitedSites,
    AddressBarShortcuts,
    SavedPasswords,
    AutofillData,
    CurrentClipboard,
    ClipboardHistory,
    RecentItems,
    RecentApplications,
    ApplicationUsageHistory,
    NetworkConnectionHistory,
    FolderViewHistory,
    PrinterHistory,
    ShellHistory,
    JumpLists,
    RunDialogHistory,
    FileDialogHistory,
    SystemSearchHistory,
    ExplorerPathHistory,
    ApplicationCache,
    ApplicationLogs,
    ApplicationSessions,
    EditorLocalHistory,
    RecentDocuments,
    RecentProjects,
    RecentConnections,
    PlaybackHistory,
    RecentPaths,
    RecentSearches,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyScanRequest {
    pub time_range: PrivacyTimeRange,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyItem {
    pub token: String,
    pub source_id: String,
    pub source_name: String,
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub category: PrivacyCategory,
    pub kind: PrivacyDataKind,
    pub sensitivity: PrivacySensitivity,
    pub impact: PrivacyImpact,
    pub recommendation: PrivacyRecommendation,
    pub capability: PrivacyCapabilityState,
    pub item_count: u64,
    pub estimated_bytes: u64,
    pub selected_by_default: bool,
    pub requires_browser_close: bool,
    pub synchronization_may_propagate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacySourceCoverage {
    pub source_id: String,
    pub source_name: String,
    /// Installed application location for deferred icon resolution. This is never a privacy data
    /// source and must not be included in logs or persisted operation records.
    pub icon_path: Option<String>,
    pub capability: PrivacyCapabilityState,
    pub item_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyScanResult {
    pub schema_version: u32,
    pub scan_id: String,
    pub revision: String,
    pub time_range: PrivacyTimeRange,
    pub scanned_at_ms: u64,
    pub elapsed_ms: u64,
    pub items: Vec<PrivacyItem>,
    pub coverage: Vec<PrivacySourceCoverage>,
}

/// Requests one bounded, read-only page of evidence for an aggregate privacy item.
///
/// Detail labels can contain private browsing or application data. They exist only in the active
/// scan session and must never be persisted or included in diagnostic logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyDetailsRequest {
    pub scan_id: String,
    pub token: String,
    pub offset: u64,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyDetailEntry {
    pub label: String,
    pub item_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyDetailsPresentation {
    List,
    AggregateOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyDetailsPage {
    pub schema_version: u32,
    pub scan_id: String,
    pub token: String,
    pub total_item_count: u64,
    pub presentation: PrivacyDetailsPresentation,
    pub entries: Vec<PrivacyDetailEntry>,
    pub next_offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyExecutionRequest {
    pub scan_id: String,
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyExecutionPlanItem {
    pub token: String,
    pub source_id: String,
    pub source_name: String,
    pub profile_name: Option<String>,
    pub kind: PrivacyDataKind,
    pub impact: PrivacyImpact,
    pub item_count: u64,
    pub estimated_bytes: u64,
    pub requires_browser_close: bool,
    pub synchronization_may_propagate: bool,
}

/// A running browser group that the confirmation dialog may close before
/// privacy execution. Core resolves these entries from trusted scan evidence;
/// the WebView receives process names for explanation but cannot supply them
/// back as executable identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyBrowserCloseRequirement {
    pub source_id: String,
    pub source_name: String,
    pub processes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyExecutionPlan {
    pub schema_version: u32,
    pub plan_id: String,
    pub scan_id: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub items: Vec<PrivacyExecutionPlanItem>,
    pub browser_close_requirements: Vec<PrivacyBrowserCloseRequirement>,
    pub requires_confirmation: bool,
    pub requires_browser_close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyBrowserCloseRequest {
    pub plan_id: String,
    pub source_ids: Vec<String>,
    pub mode: ApplicationCloseMode,
}

/// Requests a read-only process refresh for applications that remained after a graceful close.
/// Source IDs are resolved through the pending privacy plan, so adapters cannot inspect arbitrary
/// process names supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyBrowserStatusRequest {
    pub plan_id: String,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyBrowserStatusTarget {
    pub source_id: String,
    pub running_processes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyBrowserStatusResult {
    pub running_process_count: u64,
    pub targets: Vec<PrivacyBrowserStatusTarget>,
    pub elapsed_ms: u64,
}

/// Executes a previously confirmed privacy plan while allowing the adapter to omit only browser
/// sources that the user explicitly chose not to close. Exclusions reduce the authorized action;
/// they can never add a source or privacy kind that was absent from the prepared plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivacyExecutionRunRequest {
    pub plan_id: String,
    pub excluded_source_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyExecutionStage {
    Validating,
    Cleaning,
    Finalizing,
}

/// Reports aggregate execution state without exposing paths or the private content being removed.
/// Source and kind identify the visible confirmation item, while counts let adapters present real
/// progress instead of an indeterminate button spinner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyExecutionProgress {
    pub stage: PrivacyExecutionStage,
    pub current_token: Option<String>,
    pub current_source_name: Option<String>,
    pub current_kind: Option<PrivacyDataKind>,
    pub completed_item_count: u64,
    pub total_item_count: u64,
    pub affected_item_count: u64,
    pub elapsed_ms: u64,
    pub completed_items: Vec<PrivacyExecutionProgressItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyExecutionItemStatus {
    Cleared,
    Unchanged,
    Failed,
    Cancelled,
}

/// Identifies the real terminal state of one processed plan item. Tokens are opaque and already
/// present in the confirmed plan, so progress can stay path-free while adapters avoid inferring
/// success from an aggregate processed count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyExecutionProgressItem {
    pub token: String,
    pub status: PrivacyExecutionItemStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyExecutionItemResult {
    pub token: String,
    pub status: PrivacyExecutionItemStatus,
    pub affected_item_count: u64,
    pub verified: bool,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyExecutionResult {
    pub plan_id: String,
    pub affected_item_count: u64,
    pub failed_item_count: u64,
    pub items: Vec<PrivacyExecutionItemResult>,
    pub scan: Option<PrivacyScanResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_scan_item_exposes_only_aggregate_privacy_evidence() {
        let item = PrivacyItem {
            token: "opaque-token".into(),
            source_id: "chrome".into(),
            source_name: "Google Chrome".into(),
            profile_id: Some("chrome:Default".into()),
            profile_name: Some("Default".into()),
            category: PrivacyCategory::BrowserActivity,
            kind: PrivacyDataKind::BrowsingHistory,
            sensitivity: PrivacySensitivity::Activity,
            impact: PrivacyImpact::CrossDevice,
            recommendation: PrivacyRecommendation::Manual,
            capability: PrivacyCapabilityState::Ready,
            item_count: 4,
            estimated_bytes: 128,
            selected_by_default: false,
            requires_browser_close: true,
            synchronization_may_propagate: true,
        };
        let value = serde_json::to_value(item).expect("privacy item must serialize");
        let serialized = value.to_string();
        for forbidden in [
            "url",
            "cookieValue",
            "clipboardContent",
            "databasePath",
            "nativePath",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn scan_request_rejects_unknown_protocol_fields() {
        let value = r#"{"timeRange":"allTime","databasePath":"private"}"#;
        assert!(serde_json::from_str::<PrivacyScanRequest>(value).is_err());
    }
}
