//! Catalog-aware removal of completed Windows video downloads.
//!
//! These apps keep their download records beside account state, while Tencent
//! also mixes playback fragments with downloads in one media directory. Only
//! catalog-identified files are staged and deleted after the catalog changes.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

use quick_xml::{events::Event, Reader, Writer, XmlVersion};
use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use strawberrydisk_platform::{current_platform, Platform};

use crate::{
    applications::catalog::ProcessSnapshot,
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

const REVISION: &str = "windows-offline-video-v3-catalog-fingerprint";
pub(super) const TENCENT_SHORTCUT_DIR: &str =
    "\u{4e0b}\u{8f7d}\u{5267}\u{96c6}\u{5feb}\u{6377}\u{65b9}\u{5f0f}";
const MAX_CATALOG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MEDIA_ITEMS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VideoApp {
    Qqlive,
    Youku,
    Iqiyi,
}

impl VideoApp {
    pub(super) const ALL: [Self; 3] = [Self::Qqlive, Self::Youku, Self::Iqiyi];

    fn id(self) -> &'static str {
        match self {
            Self::Qqlive => "special.qqlive-offline-videos",
            Self::Youku => "special.youku-offline-videos",
            Self::Iqiyi => "special.iqiyi-offline-videos",
        }
    }

    pub(super) fn processes(self) -> &'static [&'static str] {
        match self {
            Self::Qqlive => &["QQLive.exe", "QQLiveService.exe", "QQLiveLCService.exe"],
            Self::Youku => &["YOUKU.exe", "YoukuNplayer.exe"],
            Self::Iqiyi => &[
                "QyClient.exe",
                "QyPlayer.exe",
                "QyFragment.exe",
                "QyKernel.exe",
            ],
        }
    }

    pub(super) fn catalog(self, roaming: &Path) -> PathBuf {
        match self {
            Self::Qqlive => roaming.join("Tencent/QQLive/xml/0/QLDownload.xml"),
            Self::Youku => roaming.join("youku-app-arm/data.json"),
            Self::Iqiyi => roaming.join("IQIYI Video/LStyle/PPSDownLoad.db"),
        }
    }
}

#[derive(Debug)]
struct Candidate {
    root: PathBuf,
    paths: Vec<PathBuf>,
    catalog_record_count: usize,
    catalog_fingerprint: blake3::Hash,
    bytes: u64,
    file_count: u64,
    modified_at_ms: Option<u64>,
}

pub(super) fn contains(id: &str) -> bool {
    VideoApp::ALL.iter().any(|app| app.id() == id)
}
pub(super) fn count() -> usize {
    VideoApp::ALL.len()
}

pub(super) fn catalog_digest() -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(REVISION.as_bytes());
    for app in VideoApp::ALL {
        hasher.update(app.id().as_bytes());
        for process in app.processes() {
            hasher.update(process.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

pub(super) fn preview_all(
    is_cancelled: &(dyn Fn() -> bool + Sync),
    report_path: &(dyn Fn(&Path) + Sync),
    report_files: &(dyn Fn(&Path, u64, u64) + Sync),
) -> Vec<ScanRuleResult> {
    VideoApp::ALL
        .iter()
        .copied()
        .map(|app| {
            let started = Instant::now();
            match roaming_dir().and_then(|roaming| discover(app, &roaming, is_cancelled)) {
                Ok(Some(candidate)) => match running_processes(app) {
                    Ok(running) => {
                        report_path(&candidate.root);
                        report_files(&candidate.root, candidate.file_count, candidate.bytes);
                        scan_rule(
                            app,
                            Some(&candidate),
                            running,
                            started.elapsed().as_millis() as u64,
                        )
                    }
                    Err(error) => {
                        log::warn!(
                            "windows_video_preview_limited app={} error={}",
                            app.id(),
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
                        "windows_video_preview_limited app={} error={}",
                        app.id(),
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
        .iter()
        .copied()
        .map(|app| unavailable(app, ScanItemStatus::Limited, 0))
        .collect()
}

pub(super) fn execute(
    id: &str,
    scope: Option<&SourceScope>,
    dry_run: bool,
    operation: &OperationGuard,
) -> CleanupActionResult {
    let Some(app) = VideoApp::ALL.into_iter().find(|app| app.id() == id) else {
        return failed(id, 0, CleanupActionReason::PreflightFailed, Vec::new());
    };
    let candidate = match roaming_dir()
        .and_then(|roaming| discover(app, &roaming, &|| operation.ensure_not_cancelled().is_err()))
    {
        Ok(Some(candidate)) => candidate,
        Err(error) => {
            log::warn!(
                "windows_video_preflight_failed app={} error={}",
                id,
                strawberrydisk_platform::diagnostics::text(&error)
            );
            return failed(id, 0, CleanupActionReason::PreflightFailed, Vec::new());
        }
        Ok(None) => return failed(id, 0, CleanupActionReason::PreflightFailed, Vec::new()),
    };
    if scope.is_some_and(|selection| {
        selection
            .validate_known_paths([candidate.root.as_path()])
            .is_err()
    }) {
        return failed(
            id,
            candidate.bytes,
            CleanupActionReason::PreflightFailed,
            Vec::new(),
        );
    }
    if scope.is_some_and(|selection| !selection.selects(&candidate.root)) {
        return completed(id, dry_run, 0, 0, 0);
    }
    let running = match running_processes(app) {
        Ok(running) => running,
        Err(_) => {
            return failed(
                id,
                candidate.bytes,
                CleanupActionReason::PreflightFailed,
                Vec::new(),
            )
        }
    };
    if !running.is_empty() {
        return failed(
            id,
            candidate.bytes,
            CleanupActionReason::RunningProcesses,
            running,
        );
    }
    if dry_run {
        return completed(id, true, candidate.bytes, 0, candidate.file_count);
    }
    if operation.ensure_not_cancelled().is_err() {
        return failed(
            id,
            candidate.bytes,
            CleanupActionReason::Cancelled,
            Vec::new(),
        );
    }

    let mut staged = Vec::new();
    for path in &candidate.paths {
        let stage = path.with_file_name(format!(
            ".strawberrydisk-video-stage-{}",
            uuid::Uuid::new_v4()
        ));
        if prepare_path_for_permanent_delete(path).is_err() || fs::rename(path, &stage).is_err() {
            restore_staged(app, &staged);
            return failed(
                id,
                candidate.bytes,
                CleanupActionReason::PreflightFailed,
                Vec::new(),
            );
        }
        staged.push((path.clone(), stage));
    }
    let catalog_result = match running_processes(app) {
        Ok(running) if running.is_empty() => roaming_dir().and_then(|roaming| {
            clear_catalog(
                app,
                &app.catalog(&roaming),
                candidate.catalog_record_count,
                candidate.catalog_fingerprint,
            )
        }),
        Ok(_) => Err("video app restarted during cleanup".to_string()),
        Err(error) => Err(error),
    };
    if let Err(error) = catalog_result {
        log::warn!(
            "windows_video_catalog_update_failed app={} error={}",
            id,
            strawberrydisk_platform::diagnostics::text(&error)
        );
        restore_staged(app, &staged);
        return failed(
            id,
            candidate.bytes,
            CleanupActionReason::PreflightFailed,
            Vec::new(),
        );
    }
    let mut released = 0;
    let mut removed = 0;
    let mut failures = 0;
    for (_, stage) in staged {
        let measured = measure_path_filtered(&stage, None, &|_, _| true);
        match prepare_path_for_permanent_delete(&stage).and_then(|prepared| {
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
                    "windows_video_stage_delete_failed app={} stage={} error={}",
                    id,
                    diagnostic_path(&stage),
                    strawberrydisk_platform::diagnostics::text(&error)
                );
            }
        }
    }
    if failures == 0 {
        completed(id, false, candidate.bytes, released, removed)
    } else {
        CleanupActionResult {
            rule_id: id.to_string(),
            action_kind: CleanupActionKind::Delete,
            status: CleanupActionStatus::Partial,
            reason_code: Some(CleanupActionReason::ItemsSkipped),
            bytes_expected: candidate.bytes,
            released_bytes: released,
            affected_item_count: removed,
            failed_item_count: failures,
            running_processes: Vec::new(),
        }
    }
}

fn restore_staged(app: VideoApp, staged: &[(PathBuf, PathBuf)]) {
    for (original, stage) in staged.iter().rev() {
        if let Err(error) = fs::rename(stage, original) {
            log::error!(
                "windows_video_restore_failed app={} stage={} destination={} error={}",
                app.id(),
                diagnostic_path(stage),
                diagnostic_path(original),
                strawberrydisk_platform::diagnostics::text(&error)
            );
        }
    }
}

pub(super) fn roaming_dir() -> Result<PathBuf, String> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("APPDATA is unavailable".to_string())
}

fn discover(
    app: VideoApp,
    roaming: &Path,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Option<Candidate>, String> {
    let catalog = app.catalog(roaming);
    if !catalog.exists() {
        return Ok(None);
    }
    current_platform()
        .validate_path_no_links(&catalog)
        .map_err(|error| error.to_string())?;
    let expected_catalog_fingerprint = catalog_fingerprint(app, &catalog)?;
    let Some((root, mut paths)) = (match app {
        VideoApp::Qqlive => tencent_paths(&catalog)?,
        VideoApp::Youku => youku_paths(&catalog)?,
        VideoApp::Iqiyi => iqiyi_paths(&catalog)?,
    }) else {
        return Ok(None);
    };
    if paths.is_empty() {
        return Ok(None);
    }
    if paths.len() > MAX_MEDIA_ITEMS {
        return Err("offline download count exceeds the limit".to_string());
    }
    current_platform()
        .validate_path_no_links(&root)
        .map_err(|error| error.to_string())?;
    paths.sort();
    let original_count = paths.len();
    paths.dedup();
    if paths.len() != original_count {
        return Err("offline video catalog repeats a media path".to_string());
    }
    if catalog_fingerprint(app, &catalog)? != expected_catalog_fingerprint {
        return Err("offline video catalog changed during discovery".to_string());
    }
    let catalog_record_count = if app == VideoApp::Qqlive {
        remove_tencent_download_items(&read_bounded(&catalog)?)?.1
    } else {
        paths.len()
    };
    let mut bytes = 0u64;
    let mut file_count = 0u64;
    for path in &paths {
        if is_cancelled() {
            return Err("cancelled while inspecting offline videos".to_string());
        }
        if !path.starts_with(&root) {
            return Err("offline video is outside its download root".to_string());
        }
        current_platform()
            .validate_path_no_links(path)
            .map_err(|error| error.to_string())?;
        let measured = measure_path_filtered(path, None, &|_, _| true);
        if measured.skipped_count != 0 || measured.file_count == 0 {
            return Err("offline video is incomplete or contains a link".to_string());
        }
        bytes = bytes
            .checked_add(measured.bytes)
            .ok_or("offline video size overflow")?;
        file_count = file_count
            .checked_add(measured.file_count)
            .ok_or("offline video file count overflow")?;
    }
    let modified_at_ms = fs::metadata(&root)
        .ok()
        .and_then(|metadata| modified_ms(&metadata));
    Ok(Some(Candidate {
        root,
        paths,
        catalog_record_count,
        catalog_fingerprint: expected_catalog_fingerprint,
        bytes,
        file_count,
        modified_at_ms,
    }))
}

pub(super) fn tencent_paths(catalog: &Path) -> Result<Option<(PathBuf, Vec<PathBuf>)>, String> {
    let xml = read_bounded(catalog)?;
    let mut reader = Reader::from_reader(xml.as_slice());
    let mut root = None;
    let mut paths = Vec::new();
    let mut pending_fid = None;
    let mut depth = 0usize;
    let mut download_list_depth = None;
    let mut download_item_depth = None;
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(element) => {
                if element.name().as_ref() == b"list"
                    && attribute(&element, b"parent", &reader)?.as_deref() == Some("4")
                {
                    if download_list_depth.is_some() {
                        return Err("Tencent Video has nested download lists".to_string());
                    }
                    download_list_depth = Some(depth + 1);
                } else if element.name().as_ref() == b"item" && download_list_depth == Some(depth) {
                    download_item_depth = Some(depth + 1);
                } else if element.name().as_ref() == b"l" && download_item_depth.is_some() {
                    pending_fid = attribute(&element, b"fid", &reader)?;
                    if attribute(&element, b"cache_complete", &reader)?.as_deref() != Some("true") {
                        return Err("Tencent Video has an unfinished download".to_string());
                    }
                }
                depth += 1;
            }
            Event::Empty(element)
                if element.name().as_ref() == b"FormatSegment" && download_item_depth.is_some() =>
            {
                let fid = pending_fid
                    .take()
                    .ok_or("Tencent Video download has no media key")?;
                if fid.is_empty()
                    || !fid.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                    })
                    || !fid.ends_with(".hls")
                {
                    return Err("Tencent Video media key is invalid".to_string());
                }
                let shortcut = PathBuf::from(
                    attribute(&element, b"ShortCutFile", &reader)?
                        .ok_or("Tencent Video shortcut is missing")?,
                );
                let shortcut_dir = shortcut
                    .parent()
                    .and_then(Path::parent)
                    .ok_or("Tencent Video shortcut path is invalid")?;
                if shortcut_dir
                    .file_name()
                    .is_none_or(|name| name != TENCENT_SHORTCUT_DIR)
                    || shortcut.extension().is_none_or(|ext| ext != "url")
                {
                    return Err("Tencent Video shortcut layout changed".to_string());
                }
                let media_root = shortcut_dir
                    .parent()
                    .ok_or("Tencent Video media root is missing")?
                    .to_path_buf();
                let root_name = media_root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("Tencent Video media root is invalid")?;
                if root_name.len() != 32
                    || !root_name.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || media_root.components().count() != 3
                {
                    return Err(
                        "Tencent Video media root is not its verified cache directory".to_string(),
                    );
                }
                if root
                    .as_ref()
                    .is_some_and(|existing| existing != &media_root)
                {
                    return Err("Tencent Video downloads use different roots".to_string());
                }
                root = Some(media_root.clone());
                paths.push(media_root.join(fid));
                if shortcut.exists() {
                    paths.push(shortcut);
                }
            }
            Event::End(element) => {
                if element.name().as_ref() == b"item" && download_item_depth == Some(depth) {
                    download_item_depth = None;
                }
                if element.name().as_ref() == b"list" && download_list_depth == Some(depth) {
                    download_list_depth = None;
                }
                depth = depth
                    .checked_sub(1)
                    .ok_or("Tencent Video XML has an unmatched end tag")?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if pending_fid.is_some() || depth != 0 || download_list_depth.is_some() {
        return Err("Tencent Video catalog contains an incomplete entry".to_string());
    }
    Ok(root.map(|root| (root, paths)))
}

fn youku_paths(catalog: &Path) -> Result<Option<(PathBuf, Vec<PathBuf>)>, String> {
    let value: Value =
        serde_json::from_slice(&read_bounded(catalog)?).map_err(|error| error.to_string())?;
    let tasks = value
        .pointer("/download/taskList")
        .and_then(Value::as_array)
        .ok_or("Youku task list is missing")?;
    let mut root = None;
    let mut paths = Vec::new();
    for task in tasks {
        if task.get("status").and_then(Value::as_str) != Some("FINISH")
            || task.get("progress").and_then(Value::as_i64) != Some(100)
        {
            return Err("Youku has an unfinished download".to_string());
        }
        let path = PathBuf::from(
            task.get("filePath")
                .and_then(Value::as_str)
                .ok_or("Youku task has no file path")?,
        );
        if path.extension().is_none_or(|ext| ext != "ykv") {
            return Err("Youku task is not a .ykv download".to_string());
        }
        let download_root = named_ancestor(&path, "download")?;
        if root
            .as_ref()
            .is_some_and(|existing| existing != &download_root)
        {
            return Err("Youku downloads use different roots".to_string());
        }
        root = Some(download_root);
        paths.push(path);
    }
    Ok(root.map(|root| (root, paths)))
}

fn iqiyi_paths(catalog: &Path) -> Result<Option<(PathBuf, Vec<PathBuf>)>, String> {
    let connection = Connection::open_with_flags(
        catalog,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    let xml: Vec<u8> = connection
        .query_row(
            "SELECT Data FROM table_file WHERE Name='Downloaded.xml'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if xml.len() as u64 > MAX_CATALOG_BYTES {
        return Err("iQIYI catalog exceeds the size limit".to_string());
    }
    let mut reader = Reader::from_reader(xml.as_slice());
    let mut root = None;
    let mut paths = Vec::new();
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(element) | Event::Empty(element) if element.name().as_ref() == b"Ch" => {
                if legacy_iqiyi_attribute(&element, b"State")?.as_deref() != Some("4") {
                    return Err("iQIYI has an unfinished download".to_string());
                }
                let directory = PathBuf::from(
                    legacy_iqiyi_attribute(&element, b"SaveDir")?
                        .ok_or("iQIYI download directory is missing")?,
                );
                let name = legacy_iqiyi_attribute(&element, b"SaveFileName")?
                    .ok_or("iQIYI download filename is missing")?;
                if Path::new(&name).components().count() != 1 {
                    return Err("iQIYI download filename is invalid".to_string());
                }
                let path = directory.join(name);
                if path.extension().is_none_or(|ext| ext != "qsv") {
                    return Err("iQIYI download is not a .qsv file".to_string());
                }
                let download_root = named_ancestor(&path, "download")?;
                if root
                    .as_ref()
                    .is_some_and(|existing| existing != &download_root)
                {
                    return Err("iQIYI downloads use different roots".to_string());
                }
                root = Some(download_root);
                paths.push(path);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(root.map(|root| (root, paths)))
}

fn named_ancestor(path: &Path, name: &str) -> Result<PathBuf, String> {
    path.ancestors()
        .find(|ancestor| {
            ancestor
                .file_name()
                .and_then(|part| part.to_str())
                .is_some_and(|part| part.eq_ignore_ascii_case(name))
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("offline video path has no {name} directory"))
}

fn attribute(
    element: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
    reader: &Reader<&[u8]>,
) -> Result<Option<String>, String> {
    for entry in element.attributes() {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.key.as_ref() == key {
            return entry
                .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())
                .map(|value| Some(value.into_owned()))
                .map_err(|error| error.to_string());
        }
    }
    Ok(None)
}

fn legacy_iqiyi_attribute(
    element: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
) -> Result<Option<String>, String> {
    for entry in element.attributes() {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.key.as_ref() == key {
            // The Windows client writes GBK bytes despite an UTF-8 XML header.
            // Prefer valid UTF-8 so newer clients do not change interpretation.
            let value = match std::str::from_utf8(entry.value.as_ref()) {
                Ok(value) => value.to_string(),
                Err(_) => encoding_rs::GBK
                    .decode_without_bom_handling_and_without_replacement(entry.value.as_ref())
                    .ok_or("iQIYI XML attribute is not valid UTF-8 or GBK")?
                    .into_owned(),
            };
            return quick_xml::escape::unescape(&value)
                .map(|value| Some(value.into_owned()))
                .map_err(|error| error.to_string());
        }
    }
    Ok(None)
}

pub(super) fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    if fs::metadata(path).map_err(|error| error.to_string())?.len() > MAX_CATALOG_BYTES {
        return Err("video catalog exceeds the size limit".to_string());
    }
    let content = fs::read(path).map_err(|error| error.to_string())?;
    if content.len() as u64 > MAX_CATALOG_BYTES {
        return Err("video catalog grew past the size limit".to_string());
    }
    Ok(content)
}

fn catalog_fingerprint(app: VideoApp, catalog: &Path) -> Result<blake3::Hash, String> {
    match app {
        VideoApp::Qqlive | VideoApp::Youku => Ok(blake3::hash(&read_bounded(catalog)?)),
        VideoApp::Iqiyi => {
            let connection = Connection::open_with_flags(
                catalog,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|error| error.to_string())?;
            let xml: Vec<u8> = connection
                .query_row(
                    "SELECT Data FROM table_file WHERE Name='Downloaded.xml'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if xml.len() as u64 > MAX_CATALOG_BYTES {
                return Err("iQIYI catalog exceeds the size limit".to_string());
            }
            Ok(blake3::hash(&xml))
        }
    }
}

fn clear_catalog(
    app: VideoApp,
    catalog: &Path,
    expected_records: usize,
    expected_fingerprint: blake3::Hash,
) -> Result<(), String> {
    match app {
        VideoApp::Qqlive => {
            let original = read_bounded(catalog)?;
            if blake3::hash(&original) != expected_fingerprint {
                return Err("Tencent Video catalog changed before cleanup".to_string());
            }
            let (updated, removed) = remove_tencent_download_items(&original)?;
            if removed == 0 || removed != expected_records {
                return Err("Tencent Video catalog changed before cleanup".to_string());
            }
            atomic_replace(catalog, &updated)
        }
        VideoApp::Youku => {
            let original = read_bounded(catalog)?;
            if blake3::hash(&original) != expected_fingerprint {
                return Err("Youku catalog changed before cleanup".to_string());
            }
            let mut value: Value =
                serde_json::from_slice(&original).map_err(|error| error.to_string())?;
            let tasks = value
                .pointer_mut("/download/taskList")
                .and_then(Value::as_array_mut)
                .ok_or("Youku task list is missing")?;
            if tasks.len() != expected_records {
                return Err("Youku catalog changed before cleanup".to_string());
            }
            let ids = tasks
                .iter()
                .filter_map(|task| task.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect::<Vec<_>>();
            tasks.clear();
            if let Some(configs) = value
                .pointer_mut("/download/configs")
                .and_then(Value::as_object_mut)
            {
                for id in ids {
                    configs.remove(&id);
                }
            }
            atomic_replace(
                catalog,
                &serde_json::to_vec(&value).map_err(|error| error.to_string())?,
            )
        }
        VideoApp::Iqiyi => {
            let mut connection = Connection::open(catalog).map_err(|error| error.to_string())?;
            let transaction = connection
                .transaction()
                .map_err(|error| error.to_string())?;
            let original: Vec<u8> = transaction
                .query_row(
                    "SELECT Data FROM table_file WHERE Name='Downloaded.xml'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if blake3::hash(&original) != expected_fingerprint {
                return Err("iQIYI catalog changed before cleanup".to_string());
            }
            let (updated, removed) = remove_xml_entries(&original, b"Ch")?;
            if removed != expected_records {
                return Err("iQIYI catalog changed before cleanup".to_string());
            }
            let affected = transaction
                .execute(
                    "UPDATE table_file SET Data=?1 WHERE Name='Downloaded.xml'",
                    params![updated],
                )
                .map_err(|error| error.to_string())?;
            if affected != 1 {
                return Err("iQIYI catalog entry changed".to_string());
            }
            transaction.commit().map_err(|error| error.to_string())
        }
    }
}

fn remove_xml_entries(xml: &[u8], name: &[u8]) -> Result<(Vec<u8>, usize), String> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut skipped_depth = 0usize;
    let mut removed = 0;
    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        match event {
            Event::Start(element) if skipped_depth == 0 && element.name().as_ref() == name => {
                skipped_depth = 1;
                removed += 1;
            }
            Event::Empty(element) if skipped_depth == 0 && element.name().as_ref() == name => {
                removed += 1;
            }
            Event::Start(_) if skipped_depth > 0 => skipped_depth += 1,
            Event::End(_) if skipped_depth > 0 => skipped_depth -= 1,
            Event::Eof if skipped_depth > 0 => {
                return Err("video catalog ends inside an entry".to_string())
            }
            Event::Eof => break,
            _ if skipped_depth > 0 => {}
            event => writer
                .write_event(event.into_owned())
                .map_err(|error| error.to_string())?,
        }
    }
    Ok((writer.into_inner(), removed))
}

fn remove_tencent_download_items(xml: &[u8]) -> Result<(Vec<u8>, usize), String> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut depth = 0usize;
    let mut download_list_depth = None;
    let mut skipped_depth = 0usize;
    let mut removed = 0usize;
    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        match event {
            Event::Start(_) if skipped_depth > 0 => skipped_depth += 1,
            Event::End(_) if skipped_depth > 0 => skipped_depth -= 1,
            Event::Eof if skipped_depth > 0 => {
                return Err("Tencent Video XML ends inside a download".to_string());
            }
            _ if skipped_depth > 0 => {}
            Event::Start(element) => {
                if element.name().as_ref() == b"item" && download_list_depth == Some(depth) {
                    skipped_depth = 1;
                    removed += 1;
                    continue;
                }
                if element.name().as_ref() == b"list"
                    && attribute(&element, b"parent", &reader)?.as_deref() == Some("4")
                {
                    if download_list_depth.is_some() {
                        return Err("Tencent Video XML has nested download lists".to_string());
                    }
                    download_list_depth = Some(depth + 1);
                }
                depth += 1;
                writer
                    .write_event(Event::Start(element.into_owned()))
                    .map_err(|error| error.to_string())?;
            }
            Event::Empty(element)
                if element.name().as_ref() == b"item" && download_list_depth == Some(depth) =>
            {
                removed += 1;
            }
            Event::End(element) => {
                if download_list_depth == Some(depth) && element.name().as_ref() == b"list" {
                    download_list_depth = None;
                }
                depth = depth
                    .checked_sub(1)
                    .ok_or("Tencent Video XML has an unmatched end tag")?;
                writer
                    .write_event(Event::End(element.into_owned()))
                    .map_err(|error| error.to_string())?;
            }
            Event::Eof => break,
            event => writer
                .write_event(event.into_owned())
                .map_err(|error| error.to_string())?,
        }
    }
    if download_list_depth.is_some() || depth != 0 {
        return Err("Tencent Video XML has unclosed elements".to_string());
    }
    Ok((writer.into_inner(), removed))
}

fn atomic_replace(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("video catalog has no parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temp.write_all(content).map_err(|error| error.to_string())?;
    temp.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temp.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn running_processes(app: VideoApp) -> Result<Vec<String>, String> {
    let names = app
        .processes()
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    ProcessSnapshot::capture()
        .map(|snapshot| snapshot.matching_processes(&names))
        .map_err(|error| error.to_string())
}

fn scan_rule(
    app: VideoApp,
    candidate: Option<&Candidate>,
    running: Vec<String>,
    elapsed_ms: u64,
) -> ScanRuleResult {
    let candidate = candidate.expect("found scan rule needs a candidate");
    ScanRuleResult {
        rule_id: app.id().to_string(),
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
        sources: vec![CleanupSourceDetail {
            path: display_path(&candidate.root),
            bytes: candidate.bytes,
            file_count: candidate.file_count,
            modified_at_ms: candidate.modified_at_ms,
            block_reason: None,
        }],
        source_count: 1,
        sources_truncated: false,
        scan_elapsed_ms: elapsed_ms,
    }
}

fn unavailable(app: VideoApp, status: ScanItemStatus, elapsed_ms: u64) -> ScanRuleResult {
    ScanRuleResult {
        rule_id: app.id().to_string(),
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
    id: &str,
    dry_run: bool,
    expected: u64,
    released: u64,
    count: u64,
) -> CleanupActionResult {
    CleanupActionResult {
        rule_id: id.to_string(),
        action_kind: CleanupActionKind::Delete,
        status: if dry_run {
            CleanupActionStatus::Previewed
        } else {
            CleanupActionStatus::Completed
        },
        reason_code: None,
        bytes_expected: expected,
        released_bytes: released,
        affected_item_count: count,
        failed_item_count: 0,
        running_processes: Vec::new(),
    }
}

fn failed(
    id: &str,
    expected: u64,
    reason: CleanupActionReason,
    running: Vec<String>,
) -> CleanupActionResult {
    CleanupActionResult {
        rule_id: id.to_string(),
        action_kind: CleanupActionKind::Delete,
        status: if matches!(
            reason,
            CleanupActionReason::Cancelled | CleanupActionReason::RunningProcesses
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
    use crate::cleanup::{CleanupRequest, CleanupService};
    use crate::shared::operation::CoordinatedOperationKind;

    #[test]
    fn tencent_catalog_identifies_only_completed_media_and_shortcuts() {
        let fixture = tempfile::tempdir().unwrap();
        let catalog = fixture.path().join("QLDownload.xml");
        let root = r"C:\0123456789abcdef0123456789abcdef";
        let shortcut_dir = TENCENT_SHORTCUT_DIR;
        let xml = format!(
            r#"<qqlive><list parent="4"><item><l fid="video_1.hls" cache_complete="true"><FormatSegment ShortCutFile="{root}\{shortcut_dir}\series\episode.url" /></l></item></list><other keep="yes" /></qqlive>"#
        );
        fs::write(&catalog, &xml).unwrap();
        let (parsed_root, paths) = tencent_paths(&catalog).unwrap().unwrap();
        assert_eq!(parsed_root, PathBuf::from(root));
        assert_eq!(paths, vec![PathBuf::from(root).join("video_1.hls")]);
        let (updated, removed) = remove_tencent_download_items(xml.as_bytes()).unwrap();
        assert_eq!(removed, 1);
        assert!(String::from_utf8(updated).unwrap().contains("keep=\"yes\""));
        fs::write(
            &catalog,
            remove_tencent_download_items(xml.as_bytes()).unwrap().0,
        )
        .unwrap();
        assert!(tencent_paths(&catalog).unwrap().is_none());

        fs::write(
            &catalog,
            xml.replace("cache_complete=\"true\"", "cache_complete=\"false\""),
        )
        .unwrap();
        assert!(tencent_paths(&catalog).is_err());
    }

    #[test]
    fn tencent_catalog_preserves_items_outside_download_list() {
        let xml = br#"<qqlive><list parent="4"><item id="download" /></list><recommendations><item id="keep" /></recommendations></qqlive>"#;
        let (updated, removed) = remove_tencent_download_items(xml).unwrap();
        assert_eq!(removed, 1);
        assert!(String::from_utf8(updated).unwrap().contains("id=\"keep\""));
    }

    #[test]
    fn tencent_catalog_ignores_media_outside_download_list() {
        let fixture = tempfile::tempdir().unwrap();
        let catalog = fixture.path().join("QLDownload.xml");
        let root = r"C:\0123456789abcdef0123456789abcdef";
        let shortcut_dir = TENCENT_SHORTCUT_DIR;
        let xml = format!(
            r#"<qqlive><list parent="4" /><history><l fid="video_1.hls" cache_complete="true"><FormatSegment ShortCutFile="{root}\{shortcut_dir}\series\episode.url" /></l></history></qqlive>"#
        );
        fs::write(&catalog, xml).unwrap();
        assert!(tencent_paths(&catalog).unwrap().is_none());
    }

    #[test]
    fn tencent_catalog_rejects_more_records_than_staged_videos() {
        let fixture = tempfile::tempdir().unwrap();
        let catalog = fixture.path().join("QLDownload.xml");
        fs::write(
            &catalog,
            br#"<qqlive><list parent="4"><item id="staged" /><item id="new" /></list></qqlive>"#,
        )
        .unwrap();
        // One staged video may also have a staged shortcut, but only one
        // catalog record is authorized for removal.
        assert!(clear_catalog(
            VideoApp::Qqlive,
            &catalog,
            1,
            catalog_fingerprint(VideoApp::Qqlive, &catalog).unwrap(),
        )
        .is_err());
    }

    #[test]
    fn youku_catalog_rejects_unfinished_and_non_media_paths() {
        let fixture = tempfile::tempdir().unwrap();
        let catalog = fixture.path().join("data.json");
        let root = fixture.path().join("download");
        let media = root.join("series/episode.ykv");
        let value = serde_json::json!({"download": {"taskList": [{"status": "FINISH", "progress": 100, "filePath": media}]}});
        fs::write(&catalog, serde_json::to_vec(&value).unwrap()).unwrap();
        let (parsed_root, paths) = youku_paths(&catalog).unwrap().unwrap();
        assert_eq!(parsed_root, root);
        assert_eq!(paths, vec![media]);

        let invalid = serde_json::json!({"download": {"taskList": [{"status": "FINISH", "progress": 100, "filePath": fixture.path().join("notes.txt")}]}});
        fs::write(&catalog, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(youku_paths(&catalog).is_err());
    }

    #[test]
    fn youku_catalog_does_not_remove_a_replaced_download() {
        let fixture = tempfile::tempdir().unwrap();
        let catalog = fixture.path().join("data.json");
        let root = fixture.path().join("download");
        let original = root.join("original.ykv");
        let replacement = root.join("replacement.ykv");
        let task = |path: &Path| {
            serde_json::json!({
                "download": {"taskList": [{"filePath": path, "status": "FINISH", "progress": 100}]}
            })
        };
        fs::write(&catalog, serde_json::to_vec(&task(&original)).unwrap()).unwrap();
        let original_fingerprint = catalog_fingerprint(VideoApp::Youku, &catalog).unwrap();
        fs::write(&catalog, serde_json::to_vec(&task(&replacement)).unwrap()).unwrap();

        assert!(clear_catalog(VideoApp::Youku, &catalog, 1, original_fingerprint).is_err());
        let after: Value = serde_json::from_slice(&fs::read(&catalog).unwrap()).unwrap();
        assert_eq!(
            after["download"]["taskList"][0]["filePath"].as_str(),
            replacement.to_str()
        );
    }

    #[test]
    fn iqiyi_catalog_keeps_unrelated_xml_when_downloads_are_removed() {
        let fixture = tempfile::tempdir().unwrap();
        let catalog = fixture.path().join("PPSDownLoad.db");
        let download = fixture.path().join("download");
        let media = download.join("series/episode.qsv");
        let connection = Connection::open(&catalog).unwrap();
        connection
            .execute_batch("CREATE TABLE table_file (Name TEXT PRIMARY KEY, Data BLOB)")
            .unwrap();
        let xml = format!(
            r#"<Root><Ch SaveDir="{}" SaveFileName="episode.qsv" State="4" /><Setting keep="yes" /></Root>"#,
            media.parent().unwrap().display()
        );
        connection
            .execute(
                "INSERT INTO table_file VALUES ('Downloaded.xml', ?1)",
                [xml.as_bytes()],
            )
            .unwrap();
        let (parsed_root, paths) = iqiyi_paths(&catalog).unwrap().unwrap();
        assert_eq!(parsed_root, download);
        assert_eq!(paths, vec![media]);
        let (updated, removed) = remove_xml_entries(xml.as_bytes(), b"Ch").unwrap();
        assert_eq!(removed, 1);
        assert!(String::from_utf8(updated).unwrap().contains("keep=\"yes\""));
    }

    #[test]
    fn iqiyi_accepts_gbk_paths_with_a_misleading_utf8_declaration() {
        let fixture = tempfile::tempdir().unwrap();
        let catalog = fixture.path().join("PPSDownLoad.db");
        let download = fixture.path().join("download");
        let directory = download.join("\u{6d4b}\u{8bd5}");
        let connection = Connection::open(&catalog).unwrap();
        connection
            .execute_batch("CREATE TABLE table_file (Name TEXT PRIMARY KEY, Data BLOB)")
            .unwrap();
        let mut xml = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Root><Ch SaveDir=\"".to_vec();
        let directory_text = directory.display().to_string();
        let (encoded, _, errors) = encoding_rs::GBK.encode(&directory_text);
        assert!(!errors);
        xml.extend_from_slice(&encoded);
        xml.extend_from_slice(b"\" SaveFileName=\"episode.qsv\" State=\"4\" /></Root>");
        connection
            .execute(
                "INSERT INTO table_file VALUES ('Downloaded.xml', ?1)",
                [xml],
            )
            .unwrap();
        let (parsed_root, paths) = iqiyi_paths(&catalog).unwrap().unwrap();
        assert_eq!(parsed_root, download);
        assert_eq!(paths, vec![directory.join("episode.qsv")]);
    }

    #[test]
    #[ignore = "requires logged-in Windows apps and explicitly disposable downloads"]
    fn live_windows_video_preview_and_cleanup() {
        let mode = std::env::var("STRAWBERRYDISK_WINDOWS_VIDEO_TEST")
            .expect("set STRAWBERRYDISK_WINDOWS_VIDEO_TEST=preview or cleanup");
        assert!(mode == "preview" || mode == "cleanup");
        for app in VideoApp::ALL {
            let roaming = roaming_dir().unwrap();
            let Some(candidate) = discover(app, &roaming, &|| false).unwrap() else {
                println!("app={} already_clean=true", app.id());
                continue;
            };
            println!(
                "app={} bytes={} files={} media_items={}",
                app.id(),
                candidate.bytes,
                candidate.file_count,
                candidate.paths.len()
            );
            assert!(candidate.bytes > 0);
            let operation = OperationGuard::start(CoordinatedOperationKind::Cleanup).unwrap();
            let preview = execute(app.id(), None, true, &operation);
            assert!(
                matches!(
                    preview.status,
                    CleanupActionStatus::Previewed | CleanupActionStatus::Blocked
                ),
                "{preview:?}"
            );
            if mode == "cleanup" {
                let result = execute(app.id(), None, false, &operation);
                println!(
                    "app={} cleanup={:?} released={}",
                    app.id(),
                    result.status,
                    result.released_bytes
                );
                assert_eq!(result.status, CleanupActionStatus::Completed, "{result:?}");
                assert!(discover(app, &roaming, &|| false).unwrap().is_none());
            }
        }
    }

    #[test]
    #[ignore = "deletes real renderer caches in a Windows test VM"]
    fn live_windows_video_rendering_cache_cleanup() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_WINDOWS_VIDEO_CACHE_TEST").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_WINDOWS_VIDEO_CACHE_TEST=1 after closing the three video apps"
        );
        let roaming = roaming_dir().unwrap();
        let preserved_paths = [
            roaming.join("Tencent/QQLive/user.ini"),
            roaming.join("Tencent/QQLive/xml/0/QLDownload.xml"),
            roaming.join("youku-app-arm/data.json"),
            roaming.join("IQIYI Video/LStyle/QySetting.ini"),
            roaming.join("IQIYI Video/LStyle/PPSDownLoad.db"),
        ];
        let before = preserved_paths
            .iter()
            .map(|path| fs::read(path).expect("account, settings, and catalog must exist"))
            .collect::<Vec<_>>();
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.qqlive-rendering-cache".to_string(),
                "app.youku-rendering-cache".to_string(),
                "app.iqiyi-rendering-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };
        let preview = CleanupService::execute(request(true)).unwrap();
        assert_eq!(preview.actions.len(), 3);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes > 0);
        let result = CleanupService::execute(request(false)).unwrap();
        assert_eq!(result.actions.len(), 3);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes > 0);
        for (path, original) in preserved_paths.iter().zip(before) {
            assert_eq!(
                fs::read(path).unwrap(),
                original,
                "{} changed",
                path.display()
            );
        }
        println!(
            "video_rendering_cache expected={} released={} affected={}",
            preview.expected_bytes, result.released_bytes, result.affected_item_count
        );
    }
}
