//! Offline downloads owned by the macOS Tencent Video, Youku, and iQIYI apps.
//!
//! Each app keeps a separate download catalog. Removing media files alone
//! leaves a completed entry in the app, so this cleaner updates that catalog
//! only after moving the complete, verified media root out of the app's view.

use std::{
    collections::HashSet,
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
        metadata::{diagnostic_path, display_path, is_link_like, modified_ms},
        permanent_delete::{delete_path_permanently, prepare_path_for_permanent_delete},
    },
    shared::operation::OperationGuard,
};

pub(super) const CLEANER_REVISION: &str = "macos-offline-video-v3-finder-metadata";
pub(super) const QQLIVE_ID: &str = "special.qqlive-offline-videos";
pub(super) const YOUKU_ID: &str = "special.youku-offline-videos";
pub(super) const IQIYI_ID: &str = "special.iqiyi-offline-videos";

const MAX_CATALOG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MEDIA_FILES: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VideoApp {
    Qqlive,
    Youku,
    Iqiyi,
}

impl VideoApp {
    const ALL: [Self; 3] = [Self::Qqlive, Self::Youku, Self::Iqiyi];

    fn id(self) -> &'static str {
        match self {
            Self::Qqlive => QQLIVE_ID,
            Self::Youku => YOUKU_ID,
            Self::Iqiyi => IQIYI_ID,
        }
    }

    fn process(self) -> &'static str {
        match self {
            Self::Qqlive => "QQLive",
            Self::Youku => "\u{4f18}\u{9177}",
            Self::Iqiyi => "qiyimac",
        }
    }

    fn media_root(self, home: &Path) -> PathBuf {
        match self {
            Self::Qqlive => home.join("Library/Containers/com.tencent.tenvideo/Data/Library/Application Support/Download/videoNew"),
            Self::Youku => home.join("Library/Containers/com.youku.mac/Data/download"),
            Self::Iqiyi => home.join("Library/Containers/com.iqiyi.player/Data/Library/.download/qsv"),
        }
    }

    fn catalog(self, home: &Path) -> PathBuf {
        match self {
            Self::Qqlive => home.join("Library/Containers/com.tencent.tenvideo/Data/Library/Application Support/CoreData/Download/downloadTask.db"),
            Self::Youku => home.join("Library/Containers/com.youku.mac/Data/data.json"),
            Self::Iqiyi => home.join("Library/Containers/com.iqiyi.player/Data/Library/Application Support/LStyle/PPSDownLoad.db"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VideoCandidate {
    root: PathBuf,
    media_paths: Vec<PathBuf>,
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
    hasher.update(CLEANER_REVISION.as_bytes());
    for app in VideoApp::ALL {
        hasher.update(app.id().as_bytes());
        hasher.update(app.process().as_bytes());
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
        .map(|app| preview(app, is_cancelled, report_path, report_files))
        .collect()
}

pub(super) fn preview_limited_all() -> Vec<ScanRuleResult> {
    VideoApp::ALL
        .iter()
        .copied()
        .map(|app| unavailable_rule(app, ScanItemStatus::Limited, 0))
        .collect()
}

fn preview(
    app: VideoApp,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    report_path: &(dyn Fn(&Path) + Sync),
    report_files: &(dyn Fn(&Path, u64, u64) + Sync),
) -> ScanRuleResult {
    let started = Instant::now();
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return unavailable_rule(app, ScanItemStatus::Limited, 0);
    };
    let candidate = match discover(app, &home, is_cancelled) {
        Ok(Some(candidate)) => candidate,
        Ok(None) => {
            return unavailable_rule(
                app,
                ScanItemStatus::NotApplicable,
                started.elapsed().as_millis() as u64,
            );
        }
        Err(error) => {
            log::warn!(
                "offline_video_preview_limited app={} error={}",
                app.id(),
                strawberrydisk_platform::diagnostics::text(&error)
            );
            return unavailable_rule(
                app,
                ScanItemStatus::Limited,
                started.elapsed().as_millis() as u64,
            );
        }
    };
    let running_processes = match running_processes(app) {
        Ok(processes) => processes,
        Err(error) => {
            log::warn!(
                "offline_video_preview_limited app={} error={}",
                app.id(),
                strawberrydisk_platform::diagnostics::text(&error)
            );
            return unavailable_rule(
                app,
                ScanItemStatus::Limited,
                started.elapsed().as_millis() as u64,
            );
        }
    };
    report_path(&candidate.root);
    report_files(&candidate.root, candidate.file_count, candidate.bytes);
    candidate_rule(
        app,
        &candidate,
        running_processes,
        started.elapsed().as_millis() as u64,
    )
}

pub(super) fn execute(
    id: &str,
    scope: Option<&SourceScope>,
    dry_run: bool,
    operation: &OperationGuard,
) -> CleanupActionResult {
    let Some(app) = VideoApp::ALL.into_iter().find(|app| app.id() == id) else {
        return failed_action(id, 0, CleanupActionReason::PreflightFailed, Vec::new());
    };
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return failed_action(id, 0, CleanupActionReason::PreflightFailed, Vec::new());
    };
    let current = match discover(app, &home, &|| operation.ensure_not_cancelled().is_err()) {
        Ok(Some(candidate)) => candidate,
        _ => {
            log::warn!(
                "offline_video_preflight_failed app={} reason=discoveryUnavailable",
                app.id()
            );
            return failed_action(id, 0, CleanupActionReason::PreflightFailed, Vec::new());
        }
    };
    if scope.is_some_and(|selection| {
        selection
            .validate_known_paths([current.root.as_path()])
            .is_err()
    }) {
        return failed_action(
            id,
            current.bytes,
            CleanupActionReason::PreflightFailed,
            Vec::new(),
        );
    }
    if scope.is_some_and(|selection| !selection.selects(&current.root)) {
        return completed_action(id, dry_run, 0, 0, 0);
    }
    let running = match running_processes(app) {
        Ok(processes) => processes,
        Err(_) => {
            return failed_action(
                id,
                current.bytes,
                CleanupActionReason::PreflightFailed,
                Vec::new(),
            );
        }
    };
    if !running.is_empty() {
        return failed_action(
            id,
            current.bytes,
            CleanupActionReason::RunningProcesses,
            running,
        );
    }
    if dry_run {
        return completed_action(id, true, current.bytes, 0, current.file_count);
    }
    if operation.ensure_not_cancelled().is_err() {
        return failed_action(
            id,
            current.bytes,
            CleanupActionReason::Cancelled,
            Vec::new(),
        );
    }
    let stage = current.root.with_file_name(format!(
        ".strawberrydisk-offline-video-stage-{}",
        uuid::Uuid::new_v4()
    ));
    if prepare_path_for_permanent_delete(&current.root).is_err()
        || fs::rename(&current.root, &stage).is_err()
    {
        return failed_action(
            id,
            current.bytes,
            CleanupActionReason::PreflightFailed,
            Vec::new(),
        );
    }
    let catalog_result = match running_processes(app) {
        Ok(processes) if processes.is_empty() => {
            clear_catalog(app, &home, current.catalog_fingerprint)
        }
        Ok(_) => Err("video app restarted during cleanup".to_string()),
        Err(error) => Err(error),
    };
    if let Err(error) = catalog_result {
        log::warn!(
            "offline_video_catalog_update_failed app={} stage={} error={}",
            app.id(),
            diagnostic_path(&stage),
            strawberrydisk_platform::diagnostics::text(&error)
        );
        if fs::rename(&stage, &current.root).is_err() {
            log::error!(
                "offline_video_restore_failed app={} stage={}",
                app.id(),
                diagnostic_path(&stage)
            );
        }
        return failed_action(
            id,
            current.bytes,
            CleanupActionReason::PreflightFailed,
            Vec::new(),
        );
    }
    let action = match prepare_path_for_permanent_delete(&stage)
        .and_then(|prepared| delete_path_permanently(prepared, current.bytes, current.file_count))
    {
        Ok(()) => completed_action(id, false, current.bytes, current.bytes, current.file_count),
        Err(error) => {
            log::warn!(
                "offline_video_stage_delete_failed app={} stage={} released_bytes={} error={}",
                app.id(),
                diagnostic_path(&stage),
                error.released_bytes(),
                strawberrydisk_platform::diagnostics::text(&error)
            );
            CleanupActionResult {
                rule_id: id.to_string(),
                action_kind: CleanupActionKind::Delete,
                status: CleanupActionStatus::Partial,
                reason_code: Some(CleanupActionReason::ItemsSkipped),
                bytes_expected: current.bytes,
                released_bytes: error.released_bytes(),
                affected_item_count: error.affected_item_count(),
                failed_item_count: 1,
                running_processes: Vec::new(),
            }
        }
    };
    action
}

fn discover(
    app: VideoApp,
    home: &Path,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Option<VideoCandidate>, String> {
    let root = app.media_root(home);
    if !root.exists() {
        return Ok(None);
    }
    current_platform()
        .validate_path_no_links(&root)
        .map_err(|error| error.to_string())?;
    let catalog = app.catalog(home);
    current_platform()
        .validate_path_no_links(&catalog)
        .map_err(|error| error.to_string())?;
    let expected_catalog_fingerprint = catalog_fingerprint(app, home)?;
    let mut media_paths = match app {
        VideoApp::Qqlive => qqlive_paths(&root, &catalog)?,
        VideoApp::Youku => youku_paths(&root, &catalog)?,
        VideoApp::Iqiyi => iqiyi_paths(&root, &catalog)?,
    };
    if media_paths.is_empty() {
        return Ok(None);
    }
    media_paths.sort();
    media_paths.dedup();
    if catalog_fingerprint(app, home)? != expected_catalog_fingerprint {
        return Err("offline video catalog changed during discovery".to_string());
    }
    if is_cancelled() {
        return Err("cancelled while inspecting offline videos".to_string());
    }
    let measured = measure_path_filtered(&root, None, &|_, _| true);
    if measured.skipped_count != 0 || measured.file_count == 0 {
        return Err("offline video tree is incomplete or contains links".to_string());
    }
    let modified_at_ms = fs::metadata(&root)
        .ok()
        .and_then(|metadata| modified_ms(&metadata));
    Ok(Some(VideoCandidate {
        root,
        media_paths,
        catalog_fingerprint: expected_catalog_fingerprint,
        bytes: measured.bytes,
        file_count: measured.file_count,
        modified_at_ms,
    }))
}

fn qqlive_paths(root: &Path, catalog: &Path) -> Result<Vec<PathBuf>, String> {
    let connection = read_only_database(catalog)?;
    let mut statement = connection
        .prepare("SELECT state, file_size, down_size, video_info FROM download_record")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut paths = Vec::new();
    for row in rows {
        let (state, file_size, downloaded, info) = row.map_err(|error| error.to_string())?;
        if state != 3 || file_size <= 0 || downloaded != file_size {
            return Err("Tencent Video has an unfinished download".to_string());
        }
        let value: Value = serde_json::from_str(&info).map_err(|error| error.to_string())?;
        let key = value
            .get("vInfoKeyId")
            .and_then(Value::as_str)
            .ok_or("Tencent Video download has no media key")?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        {
            return Err("Tencent Video media key is not a safe directory name".to_string());
        }
        paths.push(root.join(format!("{key}.hls")));
    }
    validate_direct_children(root, &paths)?;
    Ok(paths)
}

fn youku_paths(root: &Path, catalog: &Path) -> Result<Vec<PathBuf>, String> {
    let content = read_bounded(catalog)?;
    let value: Value = serde_json::from_slice(&content).map_err(|error| error.to_string())?;
    let tasks = value
        .pointer("/download/taskList")
        .and_then(Value::as_array)
        .ok_or("Youku download task list is missing")?;
    let mut paths = Vec::new();
    for task in tasks {
        if task.get("status").and_then(Value::as_str) != Some("FINISH")
            || task.get("progress").and_then(Value::as_i64) != Some(100)
        {
            return Err("Youku has an unfinished download".to_string());
        }
        let path = task
            .get("filePath")
            .and_then(Value::as_str)
            .ok_or("Youku download has no file path")?;
        let path = PathBuf::from(path);
        if !safe_media_path(root, &path)
            || path.extension().is_none_or(|extension| extension != "ykv")
        {
            return Err("Youku download path is outside its verified sandbox".to_string());
        }
        paths.push(path);
    }
    validate_exact_files(root, &paths)?;
    Ok(paths)
}

fn iqiyi_paths(root: &Path, catalog: &Path) -> Result<Vec<PathBuf>, String> {
    let connection = read_only_database(catalog)?;
    let xml: Vec<u8> = connection
        .query_row(
            "SELECT Data FROM table_file WHERE Name='Downloaded.xml'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if xml.len() as u64 > MAX_CATALOG_BYTES {
        return Err("iQIYI download catalog is too large".to_string());
    }
    let mut reader = Reader::from_reader(xml.as_slice());
    let mut paths = Vec::new();
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(element) | Event::Empty(element) if element.name().as_ref() == b"Ch" => {
                let mut directory = None;
                let mut filename = None;
                let mut state = None;
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|error| error.to_string())?;
                    let value = attribute
                        .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())
                        .map_err(|error| error.to_string())?
                        .into_owned();
                    match attribute.key.as_ref() {
                        b"SaveDir" => directory = Some(value),
                        b"SaveFileName" => filename = Some(value),
                        b"State" => state = Some(value),
                        _ => {}
                    }
                }
                if state.as_deref() != Some("4") {
                    return Err("iQIYI has an unfinished download".to_string());
                }
                let directory =
                    PathBuf::from(directory.ok_or("iQIYI download has no save directory")?);
                let filename = filename.ok_or("iQIYI download has no filename")?;
                let path = directory.join(filename);
                if !safe_media_path(root, &path)
                    || path.extension().is_none_or(|extension| extension != "qsv")
                {
                    return Err("iQIYI download path is outside its verified sandbox".to_string());
                }
                paths.push(path);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    validate_exact_files(root, &paths)?;
    Ok(paths)
}

fn validate_direct_children(root: &Path, expected: &[PathBuf]) -> Result<(), String> {
    let unique = expected.iter().cloned().collect::<HashSet<_>>();
    if unique.len() != expected.len() {
        return Err("Tencent Video catalog has duplicate media directories".to_string());
    }
    let entries = fs::read_dir(root).map_err(|error| error.to_string())?;
    let mut actual = HashSet::new();
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        current_platform()
            .validate_path_no_links(&path)
            .map_err(|error| error.to_string())?;
        // Finder can add this root-level metadata file after the preview.
        // It is not a download, but its presence must not block catalog cleanup.
        if path == root.join(".DS_Store") && path.is_file() {
            continue;
        }
        if !path.is_dir() {
            return Err("Tencent Video download root contains an unknown item".to_string());
        }
        actual.insert(path);
    }
    if actual != unique {
        return Err("Tencent Video catalog and media directories differ".to_string());
    }
    Ok(())
}

fn validate_exact_files(root: &Path, expected: &[PathBuf]) -> Result<(), String> {
    let unique = expected.iter().cloned().collect::<HashSet<_>>();
    if unique.len() != expected.len() {
        return Err("offline video catalog has duplicate media files".to_string());
    }
    let mut actual = collect_files(root)?;
    // Finder may write this metadata file when the download directory is inspected.
    // Accept it only at the media root; every other file must match the catalog.
    actual.remove(&root.join(".DS_Store"));
    if actual != unique {
        return Err("offline video catalog and files differ".to_string());
    }
    Ok(())
}

fn safe_media_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|relative| {
        relative.components().next().is_some()
            && relative
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
    })
}

fn collect_files(root: &Path) -> Result<HashSet<PathBuf>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = HashSet::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if is_link_like(&metadata) {
                return Err("offline video tree contains a link".to_string());
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                files.insert(path);
                if files.len() > MAX_MEDIA_FILES {
                    return Err("offline video tree exceeds the scan limit".to_string());
                }
            } else {
                return Err("offline video tree contains an unsupported item".to_string());
            }
        }
    }
    Ok(files)
}

fn read_only_database(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    if fs::metadata(path).map_err(|error| error.to_string())?.len() > MAX_CATALOG_BYTES {
        return Err("offline video catalog exceeds the size limit".to_string());
    }
    let content = fs::read(path).map_err(|error| error.to_string())?;
    if content.len() as u64 > MAX_CATALOG_BYTES {
        return Err("offline video catalog grew past the size limit".to_string());
    }
    Ok(content)
}

fn catalog_fingerprint(app: VideoApp, home: &Path) -> Result<blake3::Hash, String> {
    let catalog = app.catalog(home);
    match app {
        VideoApp::Qqlive => qqlive_record_fingerprint(&read_only_database(&catalog)?),
        VideoApp::Youku => Ok(blake3::hash(&read_bounded(&catalog)?)),
        VideoApp::Iqiyi => {
            let connection = read_only_database(&catalog)?;
            let xml: Vec<u8> = connection
                .query_row(
                    "SELECT Data FROM table_file WHERE Name='Downloaded.xml'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if xml.len() as u64 > MAX_CATALOG_BYTES {
                return Err("iQIYI download catalog is too large".to_string());
            }
            Ok(blake3::hash(&xml))
        }
    }
}

fn qqlive_record_fingerprint(connection: &Connection) -> Result<blake3::Hash, String> {
    let mut statement = connection
        .prepare(
            "SELECT state, file_size, down_size, video_info FROM download_record ORDER BY rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut hasher = blake3::Hasher::new();
    for row in rows {
        let (state, size, downloaded, info) = row.map_err(|error| error.to_string())?;
        hasher.update(&state.to_le_bytes());
        hasher.update(&size.to_le_bytes());
        hasher.update(&downloaded.to_le_bytes());
        hasher.update(&(info.len() as u64).to_le_bytes());
        hasher.update(info.as_bytes());
    }
    Ok(hasher.finalize())
}

fn clear_catalog(
    app: VideoApp,
    home: &Path,
    expected_fingerprint: blake3::Hash,
) -> Result<(), String> {
    let catalog = app.catalog(home);
    match app {
        VideoApp::Qqlive => {
            let mut connection = Connection::open(&catalog).map_err(|error| error.to_string())?;
            let transaction = connection
                .transaction()
                .map_err(|error| error.to_string())?;
            if qqlive_record_fingerprint(&transaction)? != expected_fingerprint {
                return Err("Tencent Video catalog changed before cleanup".to_string());
            }
            transaction
                .execute("DELETE FROM download_record", [])
                .map_err(|error| error.to_string())?;
            transaction.commit().map_err(|error| error.to_string())?;
        }
        VideoApp::Youku => {
            let content = read_bounded(&catalog)?;
            if blake3::hash(&content) != expected_fingerprint {
                return Err("Youku catalog changed before cleanup".to_string());
            }
            let mut value: Value =
                serde_json::from_slice(&content).map_err(|error| error.to_string())?;
            let tasks = value
                .pointer_mut("/download/taskList")
                .and_then(Value::as_array_mut)
                .ok_or("Youku download task list is missing")?;
            tasks.clear();
            let parent = catalog.parent().ok_or("Youku catalog has no parent")?;
            let mut temporary =
                tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
            serde_json::to_writer(&mut temporary, &value).map_err(|error| error.to_string())?;
            temporary.flush().map_err(|error| error.to_string())?;
            temporary
                .as_file()
                .sync_all()
                .map_err(|error| error.to_string())?;
            temporary
                .persist(&catalog)
                .map_err(|error| error.to_string())?;
        }
        VideoApp::Iqiyi => {
            let mut connection = Connection::open(&catalog).map_err(|error| error.to_string())?;
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
            if original.len() as u64 > MAX_CATALOG_BYTES {
                return Err("iQIYI download catalog is too large".to_string());
            }
            if blake3::hash(&original) != expected_fingerprint {
                return Err("iQIYI catalog changed before cleanup".to_string());
            }
            let updated = remove_downloaded_xml_entries(&original)?;
            let affected = transaction
                .execute(
                    "UPDATE table_file SET Data=?1 WHERE Name='Downloaded.xml'",
                    params![updated],
                )
                .map_err(|error| error.to_string())?;
            if affected != 1 {
                return Err("iQIYI download catalog entry changed".to_string());
            }
            transaction.commit().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn remove_downloaded_xml_entries(xml: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut skipped_depth = 0usize;
    let mut removed = 0usize;
    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        match event {
            Event::Start(element) if skipped_depth == 0 && element.name().as_ref() == b"Ch" => {
                skipped_depth = 1;
                removed += 1;
            }
            Event::Empty(element) if skipped_depth == 0 && element.name().as_ref() == b"Ch" => {
                removed += 1;
            }
            Event::Start(_) if skipped_depth > 0 => skipped_depth += 1,
            Event::End(_) if skipped_depth > 0 => skipped_depth -= 1,
            Event::Eof if skipped_depth > 0 => {
                return Err("iQIYI download XML ends inside an entry".to_string());
            }
            Event::Eof => break,
            _ if skipped_depth > 0 => {}
            event => writer
                .write_event(event.into_owned())
                .map_err(|error| error.to_string())?,
        }
    }
    if removed == 0 {
        return Err("iQIYI download XML contains no removable entries".to_string());
    }
    Ok(writer.into_inner())
}

fn running_processes(app: VideoApp) -> Result<Vec<String>, String> {
    ProcessSnapshot::capture()
        .map(|snapshot| snapshot.matching_processes(&[app.process().to_string()]))
        .map_err(|error| error.to_string())
}

fn candidate_rule(
    app: VideoApp,
    candidate: &VideoCandidate,
    running_processes: Vec<String>,
    elapsed_ms: u64,
) -> ScanRuleResult {
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
        status: if running_processes.is_empty() {
            ScanItemStatus::Found
        } else {
            ScanItemStatus::RequiresClose
        },
        running_processes,
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

fn unavailable_rule(app: VideoApp, status: ScanItemStatus, elapsed_ms: u64) -> ScanRuleResult {
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

fn completed_action(
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

fn failed_action(
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
    #[ignore = "deletes real renderer caches from the three installed macOS video apps"]
    fn live_macos_video_rendering_cache_cleanup() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_MACOS_VIDEO_CACHE_TEST").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_MACOS_VIDEO_CACHE_TEST=1 after closing the three video apps"
        );
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let catalogs = VideoApp::ALL.map(|app| app.catalog(&home));
        let before = catalogs
            .iter()
            .map(|path| fs::read(path).expect("video download catalog must exist"))
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
        let result = CleanupService::execute(request(false)).unwrap();
        assert_eq!(result.actions.len(), 3);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        for (path, original) in catalogs.iter().zip(before) {
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

    #[test]
    #[ignore = "requires logged-in macOS apps and explicitly disposable downloads"]
    fn live_macos_video_preview_and_cleanup() {
        let mode = std::env::var("STRAWBERRYDISK_MACOS_VIDEO_TEST")
            .expect("set STRAWBERRYDISK_MACOS_VIDEO_TEST=preview or cleanup");
        assert!(mode == "preview" || mode == "cleanup");
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        for app in VideoApp::ALL {
            let candidate = match discover(app, &home, &|| false) {
                Ok(Some(candidate)) => candidate,
                Ok(None) => {
                    println!("app={} already_clean=true", app.id());
                    continue;
                }
                Err(error)
                    if app == VideoApp::Iqiyi
                        && mode == "preview"
                        && std::env::var("STRAWBERRYDISK_MACOS_PAUSED_IQIYI_TEST").as_deref()
                            == Ok("1") =>
                {
                    assert_eq!(error, "offline video catalog and files differ");
                    let result = preview(app, &|| false, &|_| {}, &|_, _, _| {});
                    assert_eq!(result.status, ScanItemStatus::Limited);
                    assert!(!result.selectable);
                    println!("app={} paused_download_limited=true", app.id());
                    continue;
                }
                Err(error) => panic!("app={} preview failed: {error}", app.id()),
            };
            println!(
                "app={} bytes={} files={} media_items={}",
                app.id(),
                candidate.bytes,
                candidate.file_count,
                candidate.media_paths.len()
            );
            assert!(candidate.bytes > 0);
            let scan = preview(app, &|| false, &|_| {}, &|_, _, _| {});
            assert!(
                matches!(
                    scan.status,
                    ScanItemStatus::Found | ScanItemStatus::RequiresClose
                ),
                "{scan:?}"
            );
            assert!(scan.selectable);
            assert_eq!(scan.bytes, candidate.bytes);
            assert!(!scan.default_selected);
            assert!(!scan.recommended_selected);
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
                assert!(discover(app, &home, &|| false).unwrap().is_none());
            }
        }
    }

    fn prepare(app: VideoApp, home: &Path) -> (PathBuf, PathBuf) {
        let root = app.media_root(home);
        let catalog = app.catalog(home);
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(catalog.parent().unwrap()).unwrap();
        (root, catalog)
    }

    #[test]
    fn tencent_catalog_requires_exact_completed_media_directories() {
        let fixture = tempfile::tempdir().unwrap();
        let (root, catalog) = prepare(VideoApp::Qqlive, fixture.path());
        let media = root.join("video_1.hls");
        fs::create_dir(&media).unwrap();
        fs::write(media.join("segment.ts"), b"fixture").unwrap();
        let connection = Connection::open(&catalog).unwrap();
        connection.execute_batch("CREATE TABLE download_record (state INTEGER, file_size INTEGER, down_size INTEGER, video_info TEXT)").unwrap();
        connection
            .execute(
                "INSERT INTO download_record VALUES (3, 7, 7, ?1)",
                [r#"{"vInfoKeyId":"video_1"}"#],
            )
            .unwrap();

        let candidate = discover(VideoApp::Qqlive, fixture.path(), &|| false)
            .unwrap()
            .unwrap();
        assert_eq!(candidate.bytes, 7);
        assert_eq!(candidate.media_paths, vec![media.clone()]);
        fs::write(root.join(".DS_Store"), b"finder metadata").unwrap();
        assert_eq!(
            discover(VideoApp::Qqlive, fixture.path(), &|| false)
                .unwrap()
                .unwrap()
                .bytes,
            22
        );
        fs::create_dir(root.join("unknown.hls")).unwrap();
        assert!(discover(VideoApp::Qqlive, fixture.path(), &|| false).is_err());
        fs::remove_dir(root.join("unknown.hls")).unwrap();
        clear_catalog(
            VideoApp::Qqlive,
            fixture.path(),
            candidate.catalog_fingerprint,
        )
        .unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM download_record", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(media.exists());
    }

    #[test]
    fn youku_catalog_keeps_unrelated_settings_and_rejects_path_traversal() {
        let fixture = tempfile::tempdir().unwrap();
        let (root, catalog) = prepare(VideoApp::Youku, fixture.path());
        let media = root.join("episode.ykv");
        fs::write(&media, b"video").unwrap();
        let content = serde_json::json!({"account": {"keep": true}, "download": {"taskList": [{"filePath": media, "status": "FINISH", "progress": 100}]}});
        fs::write(&catalog, serde_json::to_vec(&content).unwrap()).unwrap();
        assert_eq!(
            discover(VideoApp::Youku, fixture.path(), &|| false)
                .unwrap()
                .unwrap()
                .bytes,
            5
        );
        clear_catalog(
            VideoApp::Youku,
            fixture.path(),
            catalog_fingerprint(VideoApp::Youku, fixture.path()).unwrap(),
        )
        .unwrap();
        let after: Value = serde_json::from_slice(&fs::read(&catalog).unwrap()).unwrap();
        assert_eq!(after["account"], content["account"]);
        assert_eq!(after["download"]["taskList"].as_array().unwrap().len(), 0);
        assert!(media.exists());

        let escaped = root.join("..").join("outside.ykv");
        let bad = serde_json::json!({"download": {"taskList": [{"filePath": escaped, "status": "FINISH", "progress": 100}]}});
        fs::write(&catalog, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(discover(VideoApp::Youku, fixture.path(), &|| false).is_err());
    }

    #[test]
    fn youku_catalog_does_not_remove_a_replaced_download() {
        let fixture = tempfile::tempdir().unwrap();
        let (root, catalog) = prepare(VideoApp::Youku, fixture.path());
        let original = root.join("original.ykv");
        let replacement = root.join("replacement.ykv");
        fs::write(&original, b"video").unwrap();
        let task = |path: &Path| {
            serde_json::json!({
                "download": {"taskList": [{"filePath": path, "status": "FINISH", "progress": 100}]}
            })
        };
        fs::write(&catalog, serde_json::to_vec(&task(&original)).unwrap()).unwrap();
        let candidate = discover(VideoApp::Youku, fixture.path(), &|| false)
            .unwrap()
            .unwrap();
        fs::write(&catalog, serde_json::to_vec(&task(&replacement)).unwrap()).unwrap();

        assert!(clear_catalog(
            VideoApp::Youku,
            fixture.path(),
            candidate.catalog_fingerprint
        )
        .is_err());
        assert_eq!(candidate.media_paths, vec![original]);
        let after: Value = serde_json::from_slice(&fs::read(&catalog).unwrap()).unwrap();
        assert_eq!(
            after["download"]["taskList"][0]["filePath"].as_str(),
            replacement.to_str()
        );
    }

    #[test]
    fn iqiyi_catalog_clears_only_downloaded_xml() {
        let fixture = tempfile::tempdir().unwrap();
        let (root, catalog) = prepare(VideoApp::Iqiyi, fixture.path());
        let media = root.join("episode.qsv");
        fs::write(&media, b"media").unwrap();
        let connection = Connection::open(&catalog).unwrap();
        connection
            .execute_batch("CREATE TABLE table_file (Name TEXT PRIMARY KEY, Data BLOB)")
            .unwrap();
        let xml = format!("<Root Version=\"2\"><Chs><Ch SaveDir=\"{}\" SaveFileName=\"episode.qsv\" State=\"4\" /></Chs><Settings Keep=\"yes\" /></Root>", root.display());
        connection
            .execute(
                "INSERT INTO table_file VALUES ('Downloaded.xml', ?1)",
                [xml.as_bytes()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO table_file VALUES ('Other.xml', ?1)",
                [b"keep".as_slice()],
            )
            .unwrap();
        fs::write(root.join(".DS_Store"), b"finder metadata").unwrap();
        assert_eq!(
            discover(VideoApp::Iqiyi, fixture.path(), &|| false)
                .unwrap()
                .unwrap()
                .bytes,
            20
        );
        fs::write(root.join("unknown.txt"), b"keep").unwrap();
        assert!(discover(VideoApp::Iqiyi, fixture.path(), &|| false).is_err());
        fs::remove_file(root.join("unknown.txt")).unwrap();
        clear_catalog(
            VideoApp::Iqiyi,
            fixture.path(),
            catalog_fingerprint(VideoApp::Iqiyi, fixture.path()).unwrap(),
        )
        .unwrap();
        let downloaded: Vec<u8> = connection
            .query_row(
                "SELECT Data FROM table_file WHERE Name='Downloaded.xml'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let downloaded = String::from_utf8(downloaded).unwrap();
        assert!(!downloaded.contains("episode.qsv"));
        assert!(downloaded.contains("Version=\"2\""));
        assert!(downloaded.contains("Settings Keep=\"yes\""));
        let other: Vec<u8> = connection
            .query_row(
                "SELECT Data FROM table_file WHERE Name='Other.xml'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(other, b"keep");
        assert!(media.exists());
    }

    #[test]
    fn iqiyi_paused_download_is_not_exposed_as_completed_media() {
        let fixture = tempfile::tempdir().unwrap();
        let (root, catalog) = prepare(VideoApp::Iqiyi, fixture.path());
        let partial = root.join("episode/episode.qsv.tqs");
        fs::create_dir(partial.parent().unwrap()).unwrap();
        fs::write(&partial, b"partial").unwrap();
        let connection = Connection::open(&catalog).unwrap();
        connection
            .execute_batch("CREATE TABLE table_file (Name TEXT PRIMARY KEY, Data BLOB)")
            .unwrap();
        connection
            .execute(
                "INSERT INTO table_file VALUES ('Downloaded.xml', ?1)",
                [b"<Root><Chs /></Root>".as_slice()],
            )
            .unwrap();

        assert_eq!(
            discover(VideoApp::Iqiyi, fixture.path(), &|| false).unwrap_err(),
            "offline video catalog and files differ"
        );
        assert_eq!(fs::read(partial).unwrap(), b"partial");
    }
}
