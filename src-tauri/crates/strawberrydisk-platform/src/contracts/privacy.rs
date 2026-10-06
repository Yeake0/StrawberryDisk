use std::path::PathBuf;

use super::{PlatformCancellation, PlatformResult};

/// Discovers operating-system owned privacy sources without applying product policy.
///
/// Privacy data paths and provider keys remain inside Core. The serialized protocol exposes only
/// opaque tokens, aggregate counts, typed capability states, and an installed application path
/// used by the shared icon resolver.
pub trait PrivacyPlatform: Send + Sync {
    fn discover_privacy_sources(
        &self,
        cancellation: &PlatformCancellation,
    ) -> PlatformResult<PlatformPrivacyDiscovery>;

    /// Clears a non-file system trace through its native API. File-backed traces are executed by
    /// Core after path identity and symlink checks have been repeated immediately before mutation.
    fn clear_system_privacy_trace(
        &self,
        trace: PlatformPrivacySystemTraceKind,
    ) -> PlatformResult<bool>;

    /// Returns the current content-free revision for a native source that is cleared together
    /// with Core-owned files. Most file-backed sources have no native companion and use the
    /// default. Keeping this lookup narrow avoids repeating full privacy discovery during
    /// destructive preflight.
    fn system_privacy_trace_revision(
        &self,
        _trace: PlatformPrivacySystemTraceKind,
    ) -> PlatformResult<Option<String>> {
        Ok(None)
    }

    /// Clears an application-owned native trace whose registry or operating-system representation
    /// cannot be handled by Core's file safety boundary. The enum is a closed allowlist: neither
    /// the WebView nor Core can supply a registry path or application identifier to this method.
    fn clear_application_privacy_trace(
        &self,
        trace: PlatformPrivacyApplicationNativeTraceKind,
    ) -> PlatformResult<bool>;

    /// Returns private, read-only evidence for a native system trace on explicit user request.
    /// Implementations must not log or persist returned labels.
    fn system_privacy_trace_details(
        &self,
        trace: PlatformPrivacySystemTraceKind,
        offset: u64,
        limit: u32,
    ) -> PlatformResult<Vec<PlatformPrivacyDetailEntry>>;

    /// Returns private, read-only evidence for a native application trace on explicit user
    /// request. Metadata used only for drift verification must remain excluded.
    fn application_privacy_trace_details(
        &self,
        trace: PlatformPrivacyApplicationNativeTraceKind,
        offset: u64,
        limit: u32,
    ) -> PlatformResult<Vec<PlatformPrivacyDetailEntry>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPrivacyDetailEntry {
    pub label: String,
    pub item_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPrivacyDiscovery {
    pub browsers: Vec<PlatformPrivacyBrowser>,
    pub applications: Vec<PlatformPrivacyApplication>,
    pub system_traces: Vec<PlatformPrivacySystemTrace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformPrivacyBrowserKind {
    Chromium,
    Firefox,
    Safari,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPrivacyBrowser {
    pub provider_key: String,
    pub display_name: String,
    /// Installed application or executable used only for the shared icon resolver.
    pub application_path: Option<PathBuf>,
    pub kind: PlatformPrivacyBrowserKind,
    pub process_names: Vec<String>,
    pub profiles: Vec<PlatformPrivacyProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPrivacyProfile {
    pub provider_key: String,
    pub display_name: String,
    pub root: PathBuf,
    pub history_database: Option<PathBuf>,
    pub cookie_database: Option<PathBuf>,
    /// Browser-owned credential database or structured file. Core currently exposes aggregate
    /// coverage only because direct deletion can conflict with sync and credential-store invariants.
    pub saved_password_source: Option<PathBuf>,
    /// Browser-owned form-history database. Core currently exposes aggregate coverage only.
    pub autofill_database: Option<PathBuf>,
    pub permission_database: Option<PathBuf>,
    /// Chromium's frequently visited-site database. Other engines leave this absent until a
    /// versioned schema contract is available.
    pub top_sites_database: Option<PathBuf>,
    /// Chromium's address-bar shortcut database, which contains learned navigation suggestions.
    pub shortcut_database: Option<PathBuf>,
    /// Chromium's page-to-icon mapping database. Clearing it removes regenerable visual browsing
    /// traces without touching bookmarks or the main history database.
    pub favicon_database: Option<PathBuf>,
    pub session_directories: Vec<PathBuf>,
    pub site_storage_directories: Vec<PathBuf>,
    /// Regenerable browser caches that can retain page content or activity-derived artifacts.
    pub cache_directories: Vec<PathBuf>,
}

/// One installed application and its independently selectable privacy traces.
///
/// Platform adapters resolve only stable application identity, process identity, and fixed native
/// locations. Core owns product policy, aggregate filesystem inspection, preflight, and mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPrivacyApplication {
    pub provider_key: String,
    pub display_name: String,
    pub application_path: Option<PathBuf>,
    pub process_names: Vec<String>,
    pub traces: Vec<PlatformPrivacyApplicationTrace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformPrivacyApplicationTraceKind {
    Cache,
    Logs,
    Sessions,
    EditorLocalHistory,
    RecentDocuments,
    RecentProjects,
    RecentConnections,
    PlaybackHistory,
    RecentPaths,
    RecentSearches,
}

/// Closed native actions used by application traces. Each variant maps to a compile-time allowlist
/// of registry keys or application-owned records; arbitrary paths never cross this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformPrivacyApplicationNativeTraceKind {
    MacPreviewRecentDocuments,
    MacPdfExpertRecentDocuments,
    MacTextEditRecentDocuments,
    MacSkimRecentDocuments,
    MacWpsOfficeRecentDocuments,
    MacPagesRecentDocuments,
    MacNumbersRecentDocuments,
    MacKeynoteRecentDocuments,
    MacVisualStudioCodeRecentDocuments,
    MacVisualStudioCodeEditorHistory,
    MacXcodeRecentProjects,
    MacVlcRecentMedia,
    MacMovistRecentMedia,
    MacQuickTimeRecentMedia,
    MacMicrosoftWordRecentDocuments,
    MacMicrosoftExcelRecentDocuments,
    MacMicrosoftPowerPointRecentDocuments,
    RemoteDesktopConnections,
    SevenZipRecentPaths,
    MicrosoftWordRecentDocuments,
    MicrosoftWordRecentPaths,
    MicrosoftWordRecentSearches,
    MicrosoftExcelRecentDocuments,
    MicrosoftExcelRecentPaths,
    MicrosoftExcelRecentSearches,
    MicrosoftPowerPointRecentDocuments,
    MicrosoftPowerPointRecentPaths,
    MicrosoftPowerPointRecentSearches,
    WindowsVisualStudioCodeEditorHistory,
    WpsOfficeRecentDocuments,
    WpsOfficeRecentFolders,
    WindowsModernMediaPlayerRecentMedia,
    WindowsVlcRecentMedia,
    WindowsPotPlayerRecentMedia,
    WindowsNotepadSessionHistory,
    PaintRecentDocuments,
    WordPadRecentDocuments,
    AdobeReaderRecentDocuments,
    TeamViewerRecentConnections,
    TortoiseSvnHistory,
    WinRarHistory,
    WinZipRecentArchives,
}

/// Reports whether a fixed application privacy source can be inspected on the current machine.
/// Keeping permission denial separate from general failures prevents product adapters from
/// presenting a supported but protected source as an unsupported cleanup capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformPrivacyApplicationTraceAvailability {
    Available,
    PermissionRequired,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPrivacyApplicationTrace {
    pub provider_key: String,
    pub display_name: String,
    pub kind: PlatformPrivacyApplicationTraceKind,
    /// Existing or permission-blocked fixed roots. An empty list remains a valid scanned result for
    /// an installed application, allowing the UI to show that the category was checked and empty.
    pub roots: Vec<PathBuf>,
    pub native_kind: Option<PlatformPrivacyApplicationNativeTraceKind>,
    pub all_time_only: bool,
    pub availability: PlatformPrivacyApplicationTraceAvailability,
    /// Aggregate native count and content-free revision. File-backed traces are recounted and
    /// fingerprinted by Core, so their adapter values remain zero and empty respectively.
    pub item_count: u64,
    pub revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformPrivacySystemTraceKind {
    CurrentClipboard,
    ClipboardHistory,
    RecentItems,
    RecentApplications,
    RecentDocumentHistory,
    ApplicationUsageHistory,
    NetworkConnectionHistory,
    FolderViewHistory,
    PrinterHistory,
    ShellHistory,
    JumpLists,
    RunDialogHistory,
    FileDialogHistory,
    ExplorerSearchHistory,
    ExplorerPathHistory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPrivacySystemTrace {
    pub provider_key: String,
    pub display_name: String,
    pub kind: PlatformPrivacySystemTraceKind,
    /// Fixed platform-owned roots when the source is file-backed. Core never serializes them and
    /// repeats identity and link checks before every mutation. Multiple roots let one product item
    /// represent a cohesive trace such as both automatic and custom Windows jump lists.
    pub roots: Vec<PathBuf>,
    /// Whether the adapter can constrain this source to a time range. Sources backed by opaque
    /// operating-system formats remain available only for an all-time scan instead of presenting
    /// a misleading partial count.
    pub all_time_only: bool,
    pub available: bool,
    pub item_count: u64,
    /// Content-free native revision used to prevent clearing data copied after the scan.
    pub revision: String,
}
