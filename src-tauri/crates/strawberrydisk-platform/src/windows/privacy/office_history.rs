use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

use crate::{
    vscode_history, PlatformError, PlatformErrorCode, PlatformPrivacyDetailEntry, PlatformResult,
};

const MAX_ACCOUNT_COUNT: usize = 64;
const MAX_CACHE_FILE_COUNT: usize = 256;
const MAX_CACHE_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_HISTORY_COUNT: usize = 50_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Application {
    Word,
    Excel,
    PowerPoint,
}

impl Application {
    fn directory_name(self) -> &'static str {
        match self {
            Self::Word => "Word",
            Self::Excel => "Excel",
            Self::PowerPoint => "PowerPoint",
        }
    }

    fn aggregate_prefix(self) -> &'static str {
        match self {
            Self::Word => "w-mru4-",
            Self::Excel => "x-mru4-",
            Self::PowerPoint => "p-mru4-",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Documents,
    Places,
}

impl Kind {
    fn file_prefix(self) -> &'static str {
        match self {
            Self::Documents => "Documents_",
            Self::Places => "Places_",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CacheRecord {
    #[serde(default)]
    application: String,
    #[serde(default)]
    document_url: String,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    folder_name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    place_url: String,
}

#[derive(Debug)]
struct HistoryEntry {
    label: String,
}

#[derive(Debug)]
pub(super) struct Snapshot {
    pub(super) item_count: u64,
    pub(super) revision: String,
}

fn safe_directory(path: &Path) -> PlatformResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(PlatformError::invalid_path(
                    "Office recent-item cache contains a reparse point",
                ));
            }
            Ok(metadata.is_dir())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(PlatformError::io(
            "inspect Office recent-item cache directory",
            &error,
        )),
    }
}

fn service_cache_files(
    office_root: &Path,
    application: Application,
    kind: Kind,
) -> PlatformResult<Vec<PathBuf>> {
    let cache_root = office_root.join("MruServiceCache");
    if !safe_directory(&cache_root)? {
        return Ok(Vec::new());
    }
    let mut account_directories = fs::read_dir(&cache_root)
        .map_err(|error| PlatformError::io("list Office recent-item accounts", &error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| PlatformError::io("read Office recent-item account", &error))
        })
        .collect::<PlatformResult<Vec<_>>>()?;
    account_directories.sort();
    if account_directories.len() > MAX_ACCOUNT_COUNT {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "Office recent-item cache exceeds the supported account count",
        ));
    }

    let mut files = Vec::new();
    for account_directory in account_directories {
        if !safe_directory(&account_directory)? {
            continue;
        }
        let application_directory = account_directory.join(application.directory_name());
        if !safe_directory(&application_directory)? {
            continue;
        }
        for entry in fs::read_dir(&application_directory)
            .map_err(|error| PlatformError::io("list Office recent-item cache", &error))?
        {
            let path = entry
                .map_err(|error| PlatformError::io("read Office recent-item cache", &error))?
                .path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with(kind.file_prefix()) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| PlatformError::io("inspect Office recent-item cache", &error))?;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_file()
            {
                return Err(PlatformError::invalid_path(
                    "Office recent-item cache is not a safe regular file",
                ));
            }
            if metadata.len() > MAX_CACHE_FILE_BYTES {
                return Err(PlatformError::new(
                    PlatformErrorCode::InvalidData,
                    "Office recent-item cache exceeds the supported size",
                ));
            }
            files.push(path);
            if files.len() > MAX_CACHE_FILE_COUNT {
                return Err(PlatformError::new(
                    PlatformErrorCode::InvalidData,
                    "Office recent-item cache exceeds the supported file count",
                ));
            }
        }
    }
    files.sort();
    Ok(files)
}

fn entries_and_revision(
    office_root: &Path,
    application: Application,
    kind: Kind,
) -> PlatformResult<(Vec<HistoryEntry>, String)> {
    let mut revision = blake3::Hasher::new();
    revision.update(b"mangodisk-windows-office-service-mru-v1\0");
    revision.update(application.directory_name().as_bytes());
    revision.update(kind.file_prefix().as_bytes());
    let mut entries = BTreeMap::<String, HistoryEntry>::new();
    let files = service_cache_files(office_root, application, kind)?;
    for path in &files {
        let file = fs::File::open(path)
            .map_err(|error| PlatformError::io("open Office recent-item cache", &error))?;
        let mut bytes = Vec::new();
        file.take(MAX_CACHE_FILE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| PlatformError::io("read Office recent-item cache", &error))?;
        if bytes.len() as u64 > MAX_CACHE_FILE_BYTES {
            return Err(PlatformError::new(
                PlatformErrorCode::InvalidData,
                "Office recent-item cache changed beyond the supported size",
            ));
        }
        revision.update(blake3::hash(&bytes).as_bytes());
        let records = serde_json::from_slice::<Vec<CacheRecord>>(&bytes).map_err(|_| {
            PlatformError::new(
                PlatformErrorCode::InvalidData,
                "Office recent-item cache has an unsupported format",
            )
        })?;
        for record in records {
            if !record.application.is_empty()
                && !record
                    .application
                    .eq_ignore_ascii_case(application.directory_name())
            {
                continue;
            }
            let raw_label = match kind {
                Kind::Documents => [record.document_url, record.path, record.file_name]
                    .into_iter()
                    .find(|value| !value.trim().is_empty()),
                Kind::Places => [record.place_url, record.path, record.folder_name]
                    .into_iter()
                    .find(|value| !value.trim().is_empty()),
            };
            let Some(raw_label) = raw_label else {
                continue;
            };
            let label = vscode_history::resource_label(&raw_label);
            if label.is_empty() {
                continue;
            }
            entries
                .entry(label.clone())
                .or_insert(HistoryEntry { label });
            if entries.len() > MAX_HISTORY_COUNT {
                return Err(PlatformError::new(
                    PlatformErrorCode::InvalidData,
                    "Office recent-item cache exceeds the supported history count",
                ));
            }
        }
    }
    log::debug!(
        "windows_office_service_mru_scanned application={:?} kind={:?} file_count={} item_count={}",
        application,
        kind,
        files.len(),
        entries.len()
    );
    Ok((
        entries.into_values().collect(),
        revision.finalize().to_hex().to_string(),
    ))
}

pub(super) fn snapshot(
    office_root: &Path,
    application: Application,
    kind: Kind,
) -> PlatformResult<Snapshot> {
    let (entries, revision) = entries_and_revision(office_root, application, kind)?;
    Ok(Snapshot {
        item_count: entries.len() as u64,
        revision,
    })
}

pub(super) fn details(
    office_root: &Path,
    application: Application,
    kind: Kind,
    offset: u64,
    limit: u32,
) -> PlatformResult<Vec<PlatformPrivacyDetailEntry>> {
    let (entries, _) = entries_and_revision(office_root, application, kind)?;
    Ok(entries
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|entry| PlatformPrivacyDetailEntry {
            label: entry.label,
            item_count: 1,
        })
        .collect())
}

fn aggregate_cache_files(
    office_root: &Path,
    application: Application,
) -> PlatformResult<Vec<PathBuf>> {
    let aggregate_root = office_root.join("aggmru");
    if !safe_directory(&aggregate_root)? {
        return Ok(Vec::new());
    }
    let mut accounts = fs::read_dir(&aggregate_root)
        .map_err(|error| PlatformError::io("list Office aggregate MRU accounts", &error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| PlatformError::io("read Office aggregate MRU account", &error))
        })
        .collect::<PlatformResult<Vec<_>>>()?;
    accounts.sort();
    if accounts.len() > MAX_ACCOUNT_COUNT {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "Office aggregate MRU cache exceeds the supported account count",
        ));
    }
    let mut files = Vec::new();
    for account in accounts {
        if !safe_directory(&account)? {
            continue;
        }
        for entry in fs::read_dir(account)
            .map_err(|error| PlatformError::io("list Office aggregate MRU cache", &error))?
        {
            let path = entry
                .map_err(|error| PlatformError::io("read Office aggregate MRU cache", &error))?
                .path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with(application.aggregate_prefix()) || !name.ends_with(".json") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| PlatformError::io("inspect Office aggregate MRU cache", &error))?;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_file()
            {
                return Err(PlatformError::invalid_path(
                    "Office aggregate MRU cache is not a safe regular file",
                ));
            }
            files.push(path);
            if files.len() > MAX_CACHE_FILE_COUNT {
                return Err(PlatformError::new(
                    PlatformErrorCode::InvalidData,
                    "Office aggregate MRU cache exceeds the supported file count",
                ));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Clears both the rendered service cache and its aggregate response cache. Office can otherwise
/// rebuild the visible list from the stale aggregate file immediately after the application opens.
pub(super) fn clear(
    office_root: &Path,
    application: Application,
    kind: Kind,
) -> PlatformResult<bool> {
    let mut files = service_cache_files(office_root, application, kind)?;
    files.extend(aggregate_cache_files(office_root, application)?);
    let mut removed_count = 0_u64;
    for path in files {
        fs::remove_file(&path).map_err(|error| {
            PlatformError::io("remove Office recent-item cache", &error)
                .with_possible_side_effects()
        })?;
        removed_count = removed_count.saturating_add(1);
    }
    let remaining = snapshot(office_root, application, kind)?.item_count;
    log::info!(
        "windows_office_service_mru_cleared application={application:?} kind={kind:?} removed_file_count={removed_count} remaining_count={remaining}"
    );
    Ok(remaining == 0 && aggregate_cache_files(office_root, application)?.is_empty())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    fn fixture_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "strawberrydisk-office-history-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_cache(
        root: &Path,
        account: &str,
        application: Application,
        kind: Kind,
        records: serde_json::Value,
    ) {
        let directory = root
            .join("MruServiceCache")
            .join(account)
            .join(application.directory_name());
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("{}zh-CN", kind.file_prefix())),
            serde_json::to_vec(&records).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn reads_cloud_documents_as_full_urls_and_deduplicates_locales() {
        let root = fixture_root();
        let records = serde_json::json!([{
            "Application": "Word",
            "DocumentUrl": "https://d.docs.live.net/account/Documents/My%20File.docx",
            "FileName": "My File.docx",
            "Path": "OneDrive / Documents"
        }]);
        write_cache(
            &root,
            "account-one",
            Application::Word,
            Kind::Documents,
            records.clone(),
        );
        let second_directory = root.join("MruServiceCache/account-one/Word/Documents_en-US");
        fs::write(second_directory, serde_json::to_vec(&records).unwrap()).unwrap();

        let snapshot = snapshot(&root, Application::Word, Kind::Documents).unwrap();
        let details = details(&root, Application::Word, Kind::Documents, 0, 10).unwrap();

        assert_eq!(snapshot.item_count, 1);
        assert_eq!(details.len(), 1);
        assert_eq!(
            details[0].label,
            "https://d.docs.live.net/account/Documents/My File.docx"
        );
        assert!(!snapshot.revision.contains("My File.docx"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clearing_one_application_leaves_other_office_caches_untouched() {
        let root = fixture_root();
        for application in [Application::Word, Application::PowerPoint] {
            write_cache(
                &root,
                "account-one",
                application,
                Kind::Documents,
                serde_json::json!([{
                    "Application": application.directory_name(),
                    "DocumentUrl": format!("file:///C:/Fixture/{}.docx", application.directory_name())
                }]),
            );
            let aggregate = root.join("aggmru/account-one");
            fs::create_dir_all(&aggregate).unwrap();
            fs::write(
                aggregate.join(format!("{}zh-CN-sr.json", application.aggregate_prefix())),
                b"{}",
            )
            .unwrap();
        }

        assert!(clear(&root, Application::Word, Kind::Documents).unwrap());
        assert_eq!(
            snapshot(&root, Application::Word, Kind::Documents)
                .unwrap()
                .item_count,
            0
        );
        assert_eq!(
            snapshot(&root, Application::PowerPoint, Kind::Documents)
                .unwrap()
                .item_count,
            1
        );
        assert_eq!(
            aggregate_cache_files(&root, Application::PowerPoint)
                .unwrap()
                .len(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "copies installed Office MRU caches and clears only the isolated copies"]
    fn actual_office_cache_copies_preserve_counts_and_clear_independently() {
        let source_root = dirs::data_local_dir()
            .expect("local application data must be available")
            .join("Microsoft/Office/16.0");
        let fixture = fixture_root();

        for application in [Application::Word, Application::PowerPoint] {
            let source = snapshot(&source_root, application, Kind::Documents).unwrap();
            assert!(
                source.item_count > 0,
                "installed Office application has no cached recent documents: {application:?}"
            );
            for path in service_cache_files(&source_root, application, Kind::Documents)
                .unwrap()
                .into_iter()
                .chain(aggregate_cache_files(&source_root, application).unwrap())
            {
                let relative = path.strip_prefix(&source_root).unwrap();
                let target = fixture.join(relative);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::copy(path, target).unwrap();
            }

            let copied = snapshot(&fixture, application, Kind::Documents).unwrap();
            assert_eq!(copied.item_count, source.item_count);
            assert!(clear(&fixture, application, Kind::Documents).unwrap());
            assert_eq!(
                snapshot(&fixture, application, Kind::Documents)
                    .unwrap()
                    .item_count,
                0
            );
        }
        fs::remove_dir_all(fixture).unwrap();
    }
}
