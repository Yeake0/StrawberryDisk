use std::{
    collections::BTreeSet,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use objc2::{rc::autoreleasepool, runtime::AnyObject};
use objc2_foundation::{
    NSArray, NSData, NSString, NSURLBookmarkResolutionOptions, NSURLNameKey, NSURLResourceKey,
    NSURL,
};
use plist::{Dictionary, Value};

use crate::{PlatformError, PlatformErrorCode, PlatformPrivacyDetailEntry, PlatformResult};

const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
struct Entry {
    uuid: String,
    bookmark: Vec<u8>,
}

#[derive(Debug)]
pub(super) struct Snapshot {
    pub(super) item_count: u64,
    pub(super) revision: String,
}

fn paths(home: &Path, bundle_identifier: &str) -> [PathBuf; 2] {
    let root = home.join(
        "Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.ApplicationRecentDocuments",
    );
    [
        root.join(format!("{bundle_identifier}.sfl3")),
        root.join(format!("{bundle_identifier}.sfl2")),
    ]
}

fn system_paths(home: &Path, list_identifier: &str) -> [PathBuf; 2] {
    let root = home.join("Library/Application Support/com.apple.sharedfilelist");
    [
        root.join(format!("{list_identifier}.sfl3")),
        root.join(format!("{list_identifier}.sfl2")),
    ]
}

fn system_group_paths(home: &Path, list_identifiers: &[&str]) -> Vec<PathBuf> {
    list_identifiers
        .iter()
        .flat_map(|list_identifier| system_paths(home, list_identifier))
        .collect()
}

fn archive_object<'a>(objects: &'a [Value], value: &Value) -> Option<&'a Value> {
    let index = value.as_uid()?.get() as usize;
    objects.get(index)
}

fn archive_dictionary_value<'a>(
    objects: &'a [Value],
    dictionary: &'a Dictionary,
    expected_key: &str,
) -> Option<&'a Value> {
    let keys = dictionary.get("NS.keys")?.as_array()?;
    let values = dictionary.get("NS.objects")?.as_array()?;
    if keys.len() != values.len() {
        return None;
    }
    keys.iter().zip(values).find_map(|(key, value)| {
        if archive_object(objects, key)?.as_string()? != expected_key {
            return None;
        }
        archive_object(objects, value)
    })
}

fn parse_archive(bytes: &[u8]) -> PlatformResult<(Vec<Entry>, u64)> {
    let archive = Value::from_reader(Cursor::new(bytes)).map_err(|_| {
        PlatformError::new(
            PlatformErrorCode::InvalidData,
            "the shared file list is not a valid property-list archive",
        )
    })?;
    let root = archive.as_dictionary().ok_or_else(|| {
        PlatformError::new(
            PlatformErrorCode::InvalidData,
            "the shared file list archive root is invalid",
        )
    })?;
    if root.get("$archiver").and_then(Value::as_string) != Some("NSKeyedArchiver") {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "the shared file list archive format is unsupported",
        ));
    }
    let objects = root
        .get("$objects")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PlatformError::new(
                PlatformErrorCode::InvalidData,
                "the shared file list archive has no object table",
            )
        })?;
    let top = root
        .get("$top")
        .and_then(Value::as_dictionary)
        .and_then(|top| top.get("root"))
        .and_then(|value| archive_object(objects, value))
        .and_then(Value::as_dictionary)
        .ok_or_else(|| {
            PlatformError::new(
                PlatformErrorCode::InvalidData,
                "the shared file list archive has no root dictionary",
            )
        })?;
    let item_array = archive_dictionary_value(objects, top, "items")
        .and_then(Value::as_dictionary)
        .and_then(|array| array.get("NS.objects"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PlatformError::new(
                PlatformErrorCode::InvalidData,
                "the shared file list archive has no item array",
            )
        })?;

    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    let mut skipped_item_count = 0_u64;
    for item_reference in item_array {
        let Some(item) = archive_object(objects, item_reference).and_then(Value::as_dictionary)
        else {
            skipped_item_count = skipped_item_count.saturating_add(1);
            continue;
        };
        let Some(visibility) = archive_dictionary_value(objects, item, "visibility")
            .and_then(Value::as_signed_integer)
        else {
            skipped_item_count = skipped_item_count.saturating_add(1);
            continue;
        };
        if visibility != 0 {
            continue;
        }
        let Some(uuid) = archive_dictionary_value(objects, item, "uuid")
            .and_then(Value::as_string)
            .filter(|uuid| !uuid.is_empty())
        else {
            skipped_item_count = skipped_item_count.saturating_add(1);
            continue;
        };
        let Some(bookmark) = archive_dictionary_value(objects, item, "Bookmark")
            .and_then(Value::as_data)
            .filter(|bookmark| !bookmark.is_empty())
        else {
            // Some Apple applications retain transient UUID-only placeholders beside real
            // records. They cannot be resolved to a user-visible item and must not invalidate the
            // remaining list or appear as opaque identifiers in privacy details.
            skipped_item_count = skipped_item_count.saturating_add(1);
            continue;
        };
        if seen.insert(uuid.to_owned()) {
            entries.push(Entry {
                uuid: uuid.to_owned(),
                bookmark: bookmark.to_vec(),
            });
        }
    }
    Ok((entries, skipped_item_count))
}

fn entries_and_revision(
    home: &Path,
    bundle_identifier: &str,
) -> PlatformResult<(Vec<Entry>, String)> {
    entries_and_revision_from_paths(paths(home, bundle_identifier), bundle_identifier)
}

fn entries_and_revision_from_paths(
    source_paths: impl IntoIterator<Item = PathBuf>,
    source_identifier: &str,
) -> PlatformResult<(Vec<Entry>, String)> {
    let mut revision = blake3::Hasher::new();
    revision.update(b"mangodisk-macos-shared-file-list-v1\0");
    revision.update(source_identifier.as_bytes());
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    let mut file_count = 0_u64;
    let mut skipped_item_count = 0_u64;
    for path in source_paths {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(PlatformError::io("inspect the shared file list", &error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PlatformError::invalid_path(
                "the shared file list is not a safe regular file",
            ));
        }
        if metadata.len() > MAX_ARCHIVE_BYTES {
            return Err(PlatformError::new(
                PlatformErrorCode::InvalidData,
                "the shared file list exceeds the supported size",
            ));
        }
        let bytes = fs::read(&path)
            .map_err(|error| PlatformError::io("read the shared file list", &error))?;
        file_count = file_count.saturating_add(1);
        revision.update(blake3::hash(&bytes).as_bytes());
        let (parsed_entries, parsed_skipped_item_count) = parse_archive(&bytes)?;
        skipped_item_count = skipped_item_count.saturating_add(parsed_skipped_item_count);
        for entry in parsed_entries {
            if seen.insert(entry.uuid.clone()) {
                entries.push(entry);
            }
        }
    }
    log::debug!(
        "macos_shared_file_list_scanned source_identifier={} file_count={} item_count={} skipped_item_count={}",
        source_identifier,
        file_count,
        entries.len(),
        skipped_item_count
    );
    Ok((entries, revision.finalize().to_hex().to_string()))
}

pub(super) fn snapshot(home: &Path, bundle_identifier: &str) -> PlatformResult<Snapshot> {
    let (entries, revision) = entries_and_revision(home, bundle_identifier)?;
    Ok(Snapshot {
        item_count: entries.len() as u64,
        revision,
    })
}

pub(super) fn system_snapshot(home: &Path, list_identifier: &str) -> PlatformResult<Snapshot> {
    let (entries, revision) =
        entries_and_revision_from_paths(system_paths(home, list_identifier), list_identifier)?;
    Ok(Snapshot {
        item_count: entries.len() as u64,
        revision,
    })
}

pub(super) fn system_group_snapshot(
    home: &Path,
    list_identifiers: &[&str],
) -> PlatformResult<Snapshot> {
    let source_identifier = list_identifiers.join(",");
    let (entries, revision) = entries_and_revision_from_paths(
        system_group_paths(home, list_identifiers),
        &source_identifier,
    )?;
    Ok(Snapshot {
        item_count: entries.len() as u64,
        revision,
    })
}

fn bookmark_display_name(bookmark: &[u8], fallback: &str) -> String {
    autoreleasepool(|_| {
        // SAFETY: `dataWithBytes:length:` copies the provided slice before this call returns, so
        // the retained NSData cannot outlive or alias Rust-owned memory.
        let data =
            unsafe { NSData::dataWithBytes_length(bookmark.as_ptr().cast(), bookmark.len()) };
        // SAFETY: Foundation exports this immutable resource-key constant for the process lifetime.
        let name_key = unsafe { NSURLNameKey };
        let keys = NSArray::<NSURLResourceKey>::from_slice(&[name_key]);
        let resource_name = NSURL::resourceValuesForKeys_fromBookmarkData(&keys, &data)
            .and_then(|values| values.objectForKey(name_key))
            .and_then(|value: objc2::rc::Retained<AnyObject>| {
                value.downcast_ref::<NSString>().map(ToString::to_string)
            })
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
        // Resolve only bookmark metadata and explicitly forbid UI and remote mounting. A local
        // file path is more useful than NSURLNameKey's leaf name, while network entries keep their
        // complete URL instead of an ambiguous path component.
        let options = NSURLBookmarkResolutionOptions::WithoutUI
            | NSURLBookmarkResolutionOptions::WithoutMounting;
        // SAFETY: Passing a null stale-state pointer is supported by Foundation. The NSData owns
        // its copied bytes, and the resolution options forbid UI and implicit network mounting.
        let resolved_url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data,
                options,
                None,
                std::ptr::null_mut(),
            )
        }
        .ok()
        .and_then(|url| {
            if url.isFileURL() {
                url.path().map(|path| path.to_string())
            } else {
                url.absoluteString().map(|url| url.to_string())
            }
        })
        .map(|value| value.trim().to_owned())
        .filter(|url| !url.is_empty());

        resolved_url
            .or(resource_name)
            .unwrap_or_else(|| fallback.to_owned())
    })
}

pub(super) fn details(
    home: &Path,
    bundle_identifier: &str,
    offset: u64,
    limit: u32,
) -> PlatformResult<Vec<PlatformPrivacyDetailEntry>> {
    let (entries, _) = entries_and_revision(home, bundle_identifier)?;
    Ok(detail_entries(entries, offset, limit))
}

pub(super) fn system_details(
    home: &Path,
    list_identifier: &str,
    offset: u64,
    limit: u32,
) -> PlatformResult<Vec<PlatformPrivacyDetailEntry>> {
    let (entries, _) =
        entries_and_revision_from_paths(system_paths(home, list_identifier), list_identifier)?;
    Ok(detail_entries(entries, offset, limit))
}

pub(super) fn system_group_details(
    home: &Path,
    list_identifiers: &[&str],
    offset: u64,
    limit: u32,
) -> PlatformResult<Vec<PlatformPrivacyDetailEntry>> {
    let source_identifier = list_identifiers.join(",");
    let (entries, _) = entries_and_revision_from_paths(
        system_group_paths(home, list_identifiers),
        &source_identifier,
    )?;
    Ok(detail_entries(entries, offset, limit))
}

fn detail_entries(entries: Vec<Entry>, offset: u64, limit: u32) -> Vec<PlatformPrivacyDetailEntry> {
    entries
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|entry| PlatformPrivacyDetailEntry {
            label: bookmark_display_name(&entry.bookmark, &entry.uuid),
            item_count: 1,
        })
        .collect()
}

pub(super) fn clear(home: &Path, bundle_identifier: &str) -> PlatformResult<bool> {
    clear_paths(paths(home, bundle_identifier), bundle_identifier, || {
        snapshot(home, bundle_identifier)
    })
}

pub(super) fn clear_system(home: &Path, list_identifier: &str) -> PlatformResult<bool> {
    clear_paths(system_paths(home, list_identifier), list_identifier, || {
        system_snapshot(home, list_identifier)
    })
}

pub(super) fn clear_system_group(home: &Path, list_identifiers: &[&str]) -> PlatformResult<bool> {
    let source_identifier = list_identifiers.join(",");
    clear_paths(
        system_group_paths(home, list_identifiers),
        &source_identifier,
        || system_group_snapshot(home, list_identifiers),
    )
}

fn clear_paths(
    source_paths: impl IntoIterator<Item = PathBuf>,
    source_identifier: &str,
    remaining_snapshot: impl FnOnce() -> PlatformResult<Snapshot>,
) -> PlatformResult<bool> {
    let mut removed_file_count = 0_u64;
    for path in source_paths {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(PlatformError::io("inspect the shared file list", &error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PlatformError::invalid_path(
                "the shared file list is not a safe regular file",
            ));
        }
        fs::remove_file(&path).map_err(|error| {
            PlatformError::io("remove the shared file list", &error).with_possible_side_effects()
        })?;
        removed_file_count = removed_file_count.saturating_add(1);
    }
    let remaining = remaining_snapshot()?.item_count;
    log::info!(
        "macos_shared_file_list_cleared source_identifier={source_identifier} removed_file_count={removed_file_count} remaining_count={remaining}"
    );
    Ok(remaining == 0)
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use plist::Uid;

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    fn fixture_home(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "strawberrydisk-shared-file-list-{label}-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn dictionary(values: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Dictionary(
            values
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    fn uid(index: usize) -> Value {
        Value::Uid(Uid::new(index as u64))
    }

    pub(crate) fn write_fixture(
        home: &Path,
        bundle_identifier: &str,
        records: &[(&str, i64, &[u8])],
    ) -> PathBuf {
        write_fixture_at_path(paths(home, bundle_identifier)[0].clone(), records)
    }

    pub(crate) fn write_system_fixture(
        home: &Path,
        list_identifier: &str,
        records: &[(&str, i64, &[u8])],
    ) -> PathBuf {
        write_fixture_at_path(system_paths(home, list_identifier)[0].clone(), records)
    }

    fn write_fixture_at_path(path: PathBuf, records: &[(&str, i64, &[u8])]) -> PathBuf {
        let mut objects = vec![
            Value::String("$null".into()),
            Value::String("root-placeholder".into()),
            Value::String("items".into()),
            Value::String("properties".into()),
            Value::String("items-placeholder".into()),
            Value::String("visibility".into()),
            Value::String("Bookmark".into()),
            Value::String("uuid".into()),
        ];
        let mut item_references = Vec::new();
        for (uuid_value, visibility, bookmark) in records {
            let visibility_index = objects.len();
            objects.push(Value::Integer((*visibility).into()));
            let bookmark_index = objects.len();
            objects.push(Value::Data(bookmark.to_vec()));
            let uuid_index = objects.len();
            objects.push(Value::String((*uuid_value).into()));
            let item_index = objects.len();
            objects.push(dictionary([
                ("NS.keys", Value::Array(vec![uid(5), uid(6), uid(7)])),
                (
                    "NS.objects",
                    Value::Array(vec![
                        uid(visibility_index),
                        uid(bookmark_index),
                        uid(uuid_index),
                    ]),
                ),
            ]));
            item_references.push(uid(item_index));
        }
        objects[4] = dictionary([("NS.objects", Value::Array(item_references))]);
        objects[1] = dictionary([
            ("NS.keys", Value::Array(vec![uid(2), uid(3)])),
            ("NS.objects", Value::Array(vec![uid(4), uid(0)])),
        ]);
        let archive = dictionary([
            ("$archiver", Value::String("NSKeyedArchiver".into())),
            ("$version", Value::Integer(100_000.into())),
            ("$objects", Value::Array(objects)),
            ("$top", dictionary([("root", uid(1))])),
        ]);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        archive.to_file_binary(&path).unwrap();
        path
    }

    #[test]
    fn counts_visible_items_lists_details_and_verifies_cleanup() {
        let home = fixture_home("visible-items");
        write_fixture(
            &home,
            "com.apple.iwork.pages",
            &[
                ("pages-visible-one", 0, b"bookmark-one"),
                ("pages-visible-two", 0, b"bookmark-two"),
                ("pages-hidden", 1, b"bookmark-hidden"),
                ("pages-incomplete", 0, b""),
            ],
        );

        let before = snapshot(&home, "com.apple.iwork.pages").unwrap();
        let details = details(&home, "com.apple.iwork.pages", 0, 10).unwrap();

        assert_eq!(before.item_count, 2);
        assert_eq!(details.len(), 2);
        assert_eq!(details[0].label, "pages-visible-one");
        assert_eq!(details[1].label, "pages-visible-two");
        assert!(clear(&home, "com.apple.iwork.pages").unwrap());
        assert_eq!(
            snapshot(&home, "com.apple.iwork.pages").unwrap().item_count,
            0
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn rejects_an_ordinary_file() {
        let error = parse_archive(b"not-an-archive").unwrap_err();

        assert_eq!(error.code(), PlatformErrorCode::InvalidData);
    }

    #[test]
    fn system_list_counts_logical_items_and_verifies_cleanup() {
        let home = fixture_home("system-list");
        write_system_fixture(
            &home,
            "com.apple.LSSharedFileList.RecentApplications",
            &[
                ("application-one", 0, b"bookmark-one"),
                ("application-two", 0, b"bookmark-two"),
            ],
        );

        let before =
            system_snapshot(&home, "com.apple.LSSharedFileList.RecentApplications").unwrap();
        let details = system_details(
            &home,
            "com.apple.LSSharedFileList.RecentApplications",
            0,
            10,
        )
        .unwrap();

        assert_eq!(before.item_count, 2);
        assert_eq!(details.len(), 2);
        assert!(clear_system(&home, "com.apple.LSSharedFileList.RecentApplications").unwrap());
        assert_eq!(
            system_snapshot(&home, "com.apple.LSSharedFileList.RecentApplications")
                .unwrap()
                .item_count,
            0
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn system_group_combines_and_clears_logical_items_from_each_list() {
        let home = fixture_home("system-list-group");
        let identifiers = [
            "com.apple.LSSharedFileList.RecentHosts",
            "com.apple.LSSharedFileList.RecentServers",
        ];
        write_system_fixture(&home, identifiers[0], &[("host-one", 0, b"bookmark-host")]);
        write_system_fixture(
            &home,
            identifiers[1],
            &[
                ("server-one", 0, b"bookmark-server-one"),
                ("server-two", 0, b"bookmark-server-two"),
            ],
        );

        let before = system_group_snapshot(&home, &identifiers).unwrap();
        let details = system_group_details(&home, &identifiers, 0, 10).unwrap();

        assert_eq!(before.item_count, 3);
        assert_eq!(details.len(), 3);
        assert!(clear_system_group(&home, &identifiers).unwrap());
        assert_eq!(
            system_group_snapshot(&home, &identifiers)
                .unwrap()
                .item_count,
            0
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    #[ignore = "copies explicitly selected real application shared-file lists"]
    fn actual_application_copies_parse_and_clear_without_mutating_sources() {
        let sources = [
            ("com.apple.iwork.pages", "STRAWBERRYDISK_TEST_PAGES_SFL"),
            ("com.apple.iwork.numbers", "STRAWBERRYDISK_TEST_NUMBERS_SFL"),
            ("com.apple.iwork.keynote", "STRAWBERRYDISK_TEST_KEYNOTE_SFL"),
            ("com.microsoft.vscode", "STRAWBERRYDISK_TEST_VSCODE_SFL"),
            ("com.apple.dt.xcode", "STRAWBERRYDISK_TEST_XCODE_SFL"),
            ("org.videolan.vlc", "STRAWBERRYDISK_TEST_VLC_SFL"),
            ("com.apple.preview", "STRAWBERRYDISK_TEST_PREVIEW_SFL"),
            (
                "com.readdle.pdfexpert-mac",
                "STRAWBERRYDISK_TEST_PDF_EXPERT_SFL",
            ),
            ("com.apple.textedit", "STRAWBERRYDISK_TEST_TEXTEDIT_SFL"),
            (
                "net.sourceforge.skim-app.skim",
                "STRAWBERRYDISK_TEST_SKIM_SFL",
            ),
        ]
        .into_iter()
        .filter_map(|(bundle_identifier, variable)| {
            std::env::var_os(variable)
                .map(PathBuf::from)
                .map(|source| (bundle_identifier, source))
        })
        .collect::<Vec<_>>();
        assert!(
            !sources.is_empty(),
            "at least one shared-file list is required"
        );
        let source_hashes = sources
            .iter()
            .map(|(_, source)| blake3::hash(&fs::read(source).unwrap()))
            .collect::<Vec<_>>();
        let home = fixture_home("actual-iwork-copies");
        for (bundle_identifier, source) in &sources {
            let destination = paths(&home, bundle_identifier)[0].clone();
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source, destination).unwrap();
        }

        for (bundle_identifier, _) in &sources {
            let before = entries_and_revision(&home, bundle_identifier).unwrap().0;
            assert!(!before.is_empty());
            println!(
                "validated shared-file list bundle_identifier={bundle_identifier} item_count={}",
                before.len()
            );
            let details = details(&home, bundle_identifier, 0, before.len() as u32).unwrap();
            assert_eq!(details.len(), before.len());
            assert!(details
                .iter()
                .zip(&before)
                .all(|(detail, entry)| !detail.label.is_empty() && detail.label != entry.uuid));
            assert!(details
                .iter()
                .any(|detail| detail.label.starts_with('/') || detail.label.contains("://")));
            assert!(clear(&home, bundle_identifier).unwrap());
        }
        for ((_, source), before_hash) in sources.iter().zip(source_hashes) {
            assert_eq!(before_hash, blake3::hash(&fs::read(source).unwrap()));
        }
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    #[ignore = "copies explicitly selected real system shared-file lists"]
    fn actual_system_copies_parse_and_clear_without_mutating_sources() {
        let sources = [
            (
                "com.apple.LSSharedFileList.RecentApplications",
                "STRAWBERRYDISK_TEST_RECENT_APPLICATIONS_SFL",
            ),
            (
                "com.apple.LSSharedFileList.RecentDocuments",
                "STRAWBERRYDISK_TEST_RECENT_DOCUMENTS_SFL",
            ),
            (
                "com.apple.LSSharedFileList.RecentHosts",
                "STRAWBERRYDISK_TEST_RECENT_HOSTS_SFL",
            ),
            (
                "com.apple.LSSharedFileList.RecentServers",
                "STRAWBERRYDISK_TEST_RECENT_SERVERS_SFL",
            ),
        ]
        .into_iter()
        .filter_map(|(list_identifier, variable)| {
            std::env::var_os(variable)
                .map(PathBuf::from)
                .map(|source| (list_identifier, source))
        })
        .collect::<Vec<_>>();
        assert!(
            !sources.is_empty(),
            "at least one system shared-file list is required"
        );
        let source_hashes = sources
            .iter()
            .map(|(_, source)| blake3::hash(&fs::read(source).unwrap()))
            .collect::<Vec<_>>();
        let home = fixture_home("actual-system-copies");
        for (list_identifier, source) in &sources {
            let destination = system_paths(&home, list_identifier)[0].clone();
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source, destination).unwrap();
        }

        for (list_identifier, _) in &sources {
            let before = entries_and_revision_from_paths(
                system_paths(&home, list_identifier),
                list_identifier,
            )
            .unwrap()
            .0;
            assert!(!before.is_empty());
            println!(
                "validated system shared-file list source={list_identifier} item_count={}",
                before.len()
            );
            let details = system_details(&home, list_identifier, 0, before.len() as u32).unwrap();
            assert_eq!(details.len(), before.len());
            assert!(details
                .iter()
                .zip(&before)
                .all(|(detail, entry)| !detail.label.is_empty() && detail.label != entry.uuid));
            assert!(clear_system(&home, list_identifier).unwrap());
        }
        for ((_, source), before_hash) in sources.iter().zip(source_hashes) {
            assert_eq!(before_hash, blake3::hash(&fs::read(source).unwrap()));
        }
        fs::remove_dir_all(home).unwrap();
    }
}
