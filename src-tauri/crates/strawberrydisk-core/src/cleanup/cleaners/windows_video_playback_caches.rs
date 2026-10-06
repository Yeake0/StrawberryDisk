//! Explicitly opted-in playback caches discovered from the Windows clients' settings.
//!
//! Playback fragments are not offline downloads. The three clients put these
//! caches outside their roaming profiles, so fixed TOML roots would miss custom
//! locations or accidentally include downloaded videos.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use base64::Engine;
use serde_json::Value;
use strawberrydisk_platform::{current_platform, Platform};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

use super::windows_video_offline_downloads::{
    read_bounded, roaming_dir, running_processes, tencent_paths, VideoApp, TENCENT_SHORTCUT_DIR,
};
use crate::{
    cleanup::{
        measurement::measure_path_filtered, source_selection::SourceScope, CleanupActionKind,
        CleanupActionReason, CleanupActionResult, CleanupActionStatus, CleanupCategory,
        CleanupGroup, CleanupSourceDetail, RiskLevel, ScanItemStatus, ScanRuleResult,
    },
    filesystem::{
        metadata::{diagnostic_path, display_path, modified_ms},
        permanent_delete::{delete_path_permanently, prepare_path_for_permanent_delete},
    },
    shared::operation::OperationGuard,
};

const REVISION: &str = "windows-video-playback-v1-configured-roots";

#[derive(Debug)]
struct CacheCandidate {
    paths: Vec<PathBuf>,
    bytes: u64,
    file_count: u64,
}

fn id(app: VideoApp) -> &'static str {
    match app {
        VideoApp::Qqlive => "special.qqlive-playback-cache",
        VideoApp::Youku => "special.youku-playback-cache",
        VideoApp::Iqiyi => "special.iqiyi-playback-cache",
    }
}

pub(super) fn contains(rule_id: &str) -> bool {
    VideoApp::ALL.iter().any(|app| id(*app) == rule_id)
}

pub(super) fn count() -> usize {
    VideoApp::ALL.len()
}

pub(super) fn catalog_digest() -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(REVISION.as_bytes());
    for app in VideoApp::ALL {
        hasher.update(id(app).as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

pub(super) fn preview_all(
    is_cancelled: &(dyn Fn() -> bool + Sync),
    report_path: &(dyn Fn(&Path) + Sync),
    report_files: &(dyn Fn(&Path, u64, u64) + Sync),
) -> Vec<ScanRuleResult> {
    VideoApp::ALL
        .into_iter()
        .map(|app| {
            let started = Instant::now();
            match discover(app, is_cancelled) {
                Ok(Some(candidate)) => match running_processes(app) {
                    Ok(running) => {
                        for path in &candidate.paths {
                            report_path(path);
                        }
                        report_files(
                            candidate.paths.first().expect("candidate has paths"),
                            candidate.file_count,
                            candidate.bytes,
                        );
                        scan_rule(
                            app,
                            &candidate,
                            running,
                            started.elapsed().as_millis() as u64,
                        )
                    }
                    Err(error) => {
                        log::warn!(
                            "video_playback_preview_limited rule={} error={}",
                            id(app),
                            strawberrydisk_platform::diagnostics::text(&error)
                        );
                        unavailable(
                            app,
                            ScanItemStatus::Limited,
                            started.elapsed().as_millis() as u64,
                        )
                    }
                },
                Ok(None) => unavailable(
                    app,
                    ScanItemStatus::NotApplicable,
                    started.elapsed().as_millis() as u64,
                ),
                Err(error) => {
                    log::warn!(
                        "video_playback_preview_limited rule={} error={}",
                        id(app),
                        strawberrydisk_platform::diagnostics::text(&error)
                    );
                    unavailable(
                        app,
                        ScanItemStatus::Limited,
                        started.elapsed().as_millis() as u64,
                    )
                }
            }
        })
        .collect()
}

pub(super) fn preview_limited_all() -> Vec<ScanRuleResult> {
    VideoApp::ALL
        .into_iter()
        .map(|app| unavailable(app, ScanItemStatus::Limited, 0))
        .collect()
}

pub(super) fn execute(
    rule_id: &str,
    scope: Option<&SourceScope>,
    dry_run: bool,
    operation: &OperationGuard,
) -> CleanupActionResult {
    let Some(app) = VideoApp::ALL.into_iter().find(|app| id(*app) == rule_id) else {
        return failed(rule_id, 0, CleanupActionReason::PreflightFailed, Vec::new());
    };
    let candidate = match discover(app, &|| operation.ensure_not_cancelled().is_err()) {
        Ok(Some(candidate)) => candidate,
        Ok(None) | Err(_) => {
            return failed(rule_id, 0, CleanupActionReason::PreflightFailed, Vec::new())
        }
    };
    if scope.is_some_and(|selection| {
        selection
            .validate_known_paths(candidate.paths.iter().map(PathBuf::as_path))
            .is_err()
    }) {
        return failed(
            rule_id,
            candidate.bytes,
            CleanupActionReason::PreflightFailed,
            Vec::new(),
        );
    }
    let selected = candidate
        .paths
        .iter()
        .filter(|path| scope.is_none_or(|selection| selection.selects(path)))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return completed(rule_id, dry_run, 0, 0, 0, 0);
    }
    let running = match running_processes(app) {
        Ok(running) => running,
        Err(_) => {
            return failed(
                rule_id,
                candidate.bytes,
                CleanupActionReason::PreflightFailed,
                Vec::new(),
            )
        }
    };
    if !running.is_empty() {
        return failed(
            rule_id,
            candidate.bytes,
            CleanupActionReason::RunningProcesses,
            running,
        );
    }
    let expected = selected
        .iter()
        .map(|path| measure_path_filtered(path, None, &|_, _| true).bytes)
        .sum();
    if dry_run {
        return completed(rule_id, true, expected, 0, 0, 0);
    }
    let mut released = 0;
    let mut removed = 0;
    let mut failures = 0;
    for path in selected {
        if operation.ensure_not_cancelled().is_err() {
            return CleanupActionResult {
                rule_id: rule_id.to_string(),
                action_kind: CleanupActionKind::Delete,
                status: CleanupActionStatus::Partial,
                reason_code: Some(CleanupActionReason::Cancelled),
                bytes_expected: expected,
                released_bytes: released,
                affected_item_count: removed,
                failed_item_count: failures,
                running_processes: Vec::new(),
            };
        }
        let measured = measure_path_filtered(path, None, &|_, _| true);
        match prepare_path_for_permanent_delete(path).and_then(|prepared| {
            delete_path_permanently(prepared, measured.bytes, measured.file_count)
        }) {
            Ok(()) => {
                released += measured.bytes;
                removed += measured.file_count;
            }
            Err(error) => {
                released += error.released_bytes();
                removed += error.affected_item_count();
                failures += 1;
                log::warn!(
                    "video_playback_delete_failed rule={} path={} error={}",
                    rule_id,
                    diagnostic_path(path),
                    strawberrydisk_platform::diagnostics::text(&error)
                );
            }
        }
    }
    completed(rule_id, false, expected, released, removed, failures)
}

fn discover(
    app: VideoApp,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Option<CacheCandidate>, String> {
    let roaming = roaming_dir()?;
    if !profile_present(app, &roaming) {
        return Ok(None);
    }
    let mut paths = match app {
        VideoApp::Qqlive => tencent_cache_paths(&roaming)?,
        VideoApp::Youku => youku_cache_paths(&roaming)?,
        VideoApp::Iqiyi => iqiyi_cache_paths(&roaming)?,
    };
    if paths.is_empty() {
        return Ok(None);
    }
    paths.sort();
    paths.dedup();
    let mut nonempty = Vec::new();
    let mut bytes = 0u64;
    let mut file_count = 0u64;
    for path in &paths {
        if is_cancelled() {
            return Err("cancelled while inspecting video cache".to_string());
        }
        current_platform()
            .validate_path_no_links(path)
            .map_err(|error| error.to_string())?;
        let measured = measure_path_filtered(path, None, &|_, _| true);
        if measured.skipped_count != 0 {
            return Err("video cache contains a link or changed during scanning".to_string());
        }
        if measured.file_count == 0 {
            continue;
        }
        bytes = bytes
            .checked_add(measured.bytes)
            .ok_or("video cache size overflow")?;
        file_count = file_count
            .checked_add(measured.file_count)
            .ok_or("video cache file count overflow")?;
        nonempty.push(path.clone());
    }
    if nonempty.is_empty() {
        return Ok(None);
    }
    Ok(Some(CacheCandidate {
        paths: nonempty,
        bytes,
        file_count,
    }))
}

fn profile_present(app: VideoApp, roaming: &Path) -> bool {
    match app {
        VideoApp::Iqiyi => roaming.join("IQIYI Video/LStyle/QySetting.ini").exists(),
        _ => app.catalog(roaming).exists(),
    }
}

fn tencent_cache_paths(roaming: &Path) -> Result<Vec<PathBuf>, String> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Tencent\\qqlive\\Download")
        .map_err(|error| error.to_string())?;
    let shortcut_directory: String = key.get_value("path").map_err(|error| error.to_string())?;
    let shortcut_directory = PathBuf::from(shortcut_directory);
    if shortcut_directory
        .file_name()
        .is_none_or(|name| name != TENCENT_SHORTCUT_DIR)
    {
        return Err("Tencent Video registry has an unexpected shortcut directory".to_string());
    }
    let root = shortcut_directory
        .parent()
        .ok_or("Tencent Video cache root is missing")?;
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Tencent Video cache root is invalid")?;
    if root_name.len() != 32
        || !root_name.bytes().all(|byte| byte.is_ascii_hexdigit())
        || root.components().count() != 3
    {
        return Err("Tencent Video cache root is not its verified directory".to_string());
    }
    current_platform()
        .validate_path_no_links(root)
        .map_err(|error| error.to_string())?;
    let catalog = VideoApp::Qqlive.catalog(roaming);
    let offline = match tencent_paths(&catalog)? {
        Some((download_root, paths)) if download_root == root => paths,
        Some(_) => return Err("Tencent Video cache and download roots disagree".to_string()),
        None => Vec::new(),
    };
    let mut paths = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Tencent Video cache name is invalid")?;
        if path.extension().is_some_and(|extension| extension == "hls")
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            && !offline.iter().any(|media| media == &path)
            && entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
        {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn youku_cache_paths(roaming: &Path) -> Result<Vec<PathBuf>, String> {
    let catalog = roaming.join("youku-app-arm/data.json");
    let value: Value =
        serde_json::from_slice(&read_bounded(&catalog)?).map_err(|error| error.to_string())?;
    let configured = value
        .get("accCacheDir")
        .and_then(Value::as_str)
        .ok_or("Youku cache location is missing")?;
    let root = PathBuf::from(configured);
    if root
        .file_name()
        .is_none_or(|name| !name.eq_ignore_ascii_case("youkudisk"))
    {
        return Err("Youku cache location has an unexpected layout".to_string());
    }
    let data = root.join("ngxcdn/data");
    if !data.is_dir() {
        return Ok(Vec::new());
    }
    current_platform()
        .validate_path_no_links(&data)
        .map_err(|error| error.to_string())?;
    fs::read_dir(data)
        .map_err(|error| error.to_string())?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| error.to_string())
        })
        .collect()
}

fn iqiyi_cache_paths(roaming: &Path) -> Result<Vec<PathBuf>, String> {
    let settings = roaming.join("IQIYI Video/LStyle/QySetting.ini");
    let settings =
        String::from_utf8(read_bounded(&settings)?).map_err(|error| error.to_string())?;
    let encoded = settings
        .lines()
        .find_map(|line| line.strip_prefix("download_path_new="))
        .ok_or("iQIYI download location is missing")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|error| error.to_string())?;
    let download = PathBuf::from(String::from_utf8(decoded).map_err(|error| error.to_string())?);
    if download
        .file_name()
        .is_none_or(|name| !name.eq_ignore_ascii_case("download"))
    {
        return Err("iQIYI download location has an unexpected layout".to_string());
    }
    let root = iqiyi_cache_root(&download)?;
    let mut paths = Vec::new();
    for name in ["temp_cache", "ad_cache"] {
        let cache = root.join(name);
        if cache.is_dir() {
            current_platform()
                .validate_path_no_links(&cache)
                .map_err(|error| error.to_string())?;
            for entry in fs::read_dir(cache).map_err(|error| error.to_string())? {
                paths.push(entry.map_err(|error| error.to_string())?.path());
            }
        }
    }
    Ok(paths)
}

fn iqiyi_cache_root(download: &Path) -> Result<&Path, String> {
    let root = download
        .parent()
        .ok_or("iQIYI cache root is missing".to_string())?;
    if root
        .file_name()
        .is_none_or(|name| !name.eq_ignore_ascii_case("qycache"))
    {
        return Err("iQIYI cache root has an unexpected layout".to_string());
    }
    Ok(root)
}

fn scan_rule(
    app: VideoApp,
    candidate: &CacheCandidate,
    running: Vec<String>,
    elapsed_ms: u64,
) -> ScanRuleResult {
    let sources = candidate
        .paths
        .iter()
        .map(|path| {
            let measured = measure_path_filtered(path, None, &|_, _| true);
            CleanupSourceDetail {
                path: display_path(path),
                bytes: measured.bytes,
                file_count: measured.file_count,
                modified_at_ms: fs::metadata(path)
                    .ok()
                    .and_then(|metadata| modified_ms(&metadata)),
                block_reason: None,
            }
        })
        .collect::<Vec<_>>();
    ScanRuleResult {
        rule_id: id(app).to_string(),
        category: CleanupCategory::Application,
        group: CleanupGroup::Application,
        risk: RiskLevel::Recoverable,
        default_selected: false,
        recommended_selected: false,
        bytes: candidate.bytes,
        file_count: candidate.file_count,
        available: true,
        selectable: true,
        status: if running.is_empty() {
            ScanItemStatus::Found
        } else {
            ScanItemStatus::RequiresClose
        },
        running_processes: running,
        requires_app_close: true,
        source_count: sources.len() as u64,
        sources_truncated: false,
        sources,
        scan_elapsed_ms: elapsed_ms,
    }
}

fn unavailable(app: VideoApp, status: ScanItemStatus, elapsed_ms: u64) -> ScanRuleResult {
    ScanRuleResult {
        rule_id: id(app).to_string(),
        category: CleanupCategory::Application,
        group: CleanupGroup::Application,
        risk: RiskLevel::Recoverable,
        default_selected: false,
        recommended_selected: false,
        bytes: 0,
        file_count: 0,
        available: status != ScanItemStatus::NotApplicable,
        selectable: false,
        status,
        running_processes: Vec::new(),
        requires_app_close: true,
        sources: Vec::new(),
        source_count: 0,
        sources_truncated: false,
        scan_elapsed_ms: elapsed_ms,
    }
}

fn completed(
    rule_id: &str,
    dry_run: bool,
    expected: u64,
    released: u64,
    affected: u64,
    failures: u64,
) -> CleanupActionResult {
    CleanupActionResult {
        rule_id: rule_id.to_string(),
        action_kind: CleanupActionKind::Delete,
        status: if dry_run {
            CleanupActionStatus::Previewed
        } else if failures == 0 {
            CleanupActionStatus::Completed
        } else {
            CleanupActionStatus::Partial
        },
        reason_code: if failures == 0 {
            None
        } else {
            Some(CleanupActionReason::ItemsSkipped)
        },
        bytes_expected: expected,
        released_bytes: released,
        affected_item_count: affected,
        failed_item_count: failures,
        running_processes: Vec::new(),
    }
}

fn failed(
    rule_id: &str,
    expected: u64,
    reason: CleanupActionReason,
    running: Vec<String>,
) -> CleanupActionResult {
    CleanupActionResult {
        rule_id: rule_id.to_string(),
        action_kind: CleanupActionKind::Delete,
        status: if matches!(
            reason,
            CleanupActionReason::RunningProcesses | CleanupActionReason::Cancelled
        ) {
            CleanupActionStatus::Blocked
        } else {
            CleanupActionStatus::Failed
        },
        reason_code: Some(reason),
        bytes_expected: expected,
        released_bytes: 0,
        affected_item_count: 0,
        failed_item_count: 1,
        running_processes: running,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::operation::CoordinatedOperationKind;

    #[test]
    fn playback_cache_ids_are_distinct_from_offline_downloads() {
        for app in VideoApp::ALL {
            assert!(contains(id(app)));
            assert_ne!(
                id(app),
                match app {
                    VideoApp::Qqlive => "special.qqlive-offline-videos",
                    VideoApp::Youku => "special.youku-offline-videos",
                    VideoApp::Iqiyi => "special.iqiyi-offline-videos",
                }
            );
        }
    }

    #[test]
    fn iqiyi_cache_root_rejects_unrelated_siblings_of_custom_downloads() {
        let download = Path::new(r"C:\PersonalData\download");
        assert!(iqiyi_cache_root(download).is_err());
        let owned = Path::new(r"C:\QyCache\download");
        assert_eq!(iqiyi_cache_root(owned).unwrap(), Path::new(r"C:\QyCache"));
    }

    #[test]
    fn iqiyi_playback_cache_does_not_require_offline_downloads() {
        let fixture = tempfile::tempdir().unwrap();
        let settings = fixture.path().join("IQIYI Video/LStyle/QySetting.ini");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(settings, b"download_path_new=placeholder").unwrap();
        assert!(profile_present(VideoApp::Iqiyi, fixture.path()));
    }

    #[test]
    #[ignore = "deletes disposable playback caches in a logged-in Windows test VM"]
    fn live_windows_video_playback_cache_cleanup() {
        let mode = std::env::var("STRAWBERRYDISK_WINDOWS_VIDEO_PLAYBACK_TEST")
            .expect("set STRAWBERRYDISK_WINDOWS_VIDEO_PLAYBACK_TEST=preview or cleanup");
        assert!(mode == "preview" || mode == "cleanup");
        for app in VideoApp::ALL {
            let Some(candidate) = discover(app, &|| false).unwrap() else {
                println!("rule={} already_clean=true", id(app));
                continue;
            };
            println!(
                "rule={} bytes={} files={} paths={}",
                id(app),
                candidate.bytes,
                candidate.file_count,
                candidate.paths.len()
            );
            assert!(candidate.bytes > 0);
            let operation = OperationGuard::start(CoordinatedOperationKind::Cleanup).unwrap();
            let preview = execute(id(app), None, true, &operation);
            assert!(matches!(
                preview.status,
                CleanupActionStatus::Previewed | CleanupActionStatus::Blocked
            ));
            if mode == "preview" {
                continue;
            }
            let result = execute(id(app), None, false, &operation);
            println!(
                "rule={} status={:?} released={}",
                id(app),
                result.status,
                result.released_bytes
            );
            assert_eq!(result.status, CleanupActionStatus::Completed, "{result:?}");
            assert!(discover(app, &|| false).unwrap().is_none());
        }
    }
}
