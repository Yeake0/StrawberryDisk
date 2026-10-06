use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use plist::Value;

use crate::{PlatformError, PlatformErrorCode, PlatformPrivacyDetailEntry, PlatformResult};

const MAX_PREFERENCES_BYTES: u64 = 32 * 1024 * 1024;
const RECENT_KEY_MARKER: &str = ".openfilelist.";

#[derive(Debug)]
pub(super) struct Snapshot {
    pub(super) item_count: u64,
    pub(super) revision: String,
}

fn paths(home: &Path) -> [PathBuf; 2] {
    [
        "com.kingsoft.wpsoffice.mac.global",
        "com.kingsoft.wpsoffice.mac",
    ]
    .map(|bundle_identifier| {
        home.join("Library/Containers")
            .join(bundle_identifier)
            .join("Data/Library/Preferences/com.kingsoft.Office.plist")
    })
}

fn is_recent_key(key: &str) -> bool {
    key.contains(RECENT_KEY_MARKER)
}

fn read_preferences(path: &Path) -> PlatformResult<Option<(Value, Vec<u8>)>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(PlatformError::io("inspect the WPS preferences", &error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PlatformError::invalid_path(
            "the WPS preferences are not a safe regular file",
        ));
    }
    if metadata.len() > MAX_PREFERENCES_BYTES {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "the WPS preferences exceed the supported size",
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| PlatformError::io("read the WPS preferences", &error))?;
    let value = Value::from_reader(Cursor::new(&bytes)).map_err(|_| {
        PlatformError::new(
            PlatformErrorCode::InvalidData,
            "the WPS preferences are not a valid property list",
        )
    })?;
    if value.as_dictionary().is_none() {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "the WPS preferences root is not a dictionary",
        ));
    }
    Ok(Some((value, bytes)))
}

fn entries_and_revision(home: &Path) -> PlatformResult<(Vec<String>, String)> {
    let mut revision = blake3::Hasher::new();
    revision.update(b"mangodisk-macos-wps-recent-documents-v1\0");
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    let mut file_count = 0_u64;

    for path in paths(home) {
        let Some((preferences, bytes)) = read_preferences(&path)? else {
            continue;
        };
        file_count = file_count.saturating_add(1);
        revision.update(blake3::hash(&bytes).as_bytes());
        for (key, value) in preferences.as_dictionary().expect("validated dictionary") {
            if !is_recent_key(key) || !key.ends_with(".path") {
                continue;
            }
            let Some(path) = value
                .as_string()
                .map(str::trim)
                .filter(|path| !path.is_empty())
            else {
                continue;
            };
            if seen.insert(path.to_owned()) {
                entries.push(path.to_owned());
            }
        }
    }

    log::debug!(
        "macos_wps_recent_documents_scanned file_count={} item_count={}",
        file_count,
        entries.len()
    );
    Ok((entries, revision.finalize().to_hex().to_string()))
}

pub(super) fn snapshot(home: &Path) -> PlatformResult<Snapshot> {
    let (entries, revision) = entries_and_revision(home)?;
    Ok(Snapshot {
        item_count: entries.len() as u64,
        revision,
    })
}

pub(super) fn details(
    home: &Path,
    offset: u64,
    limit: u32,
) -> PlatformResult<Vec<PlatformPrivacyDetailEntry>> {
    let (entries, _) = entries_and_revision(home)?;
    Ok(entries
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|label| PlatformPrivacyDetailEntry {
            label,
            item_count: 1,
        })
        .collect())
}

fn replace_preferences(path: &Path, value: &Value) -> PlatformResult<()> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| PlatformError::io("inspect the WPS preferences", &error))?;
    let mut encoded = Vec::new();
    plist::to_writer_binary(&mut encoded, value).map_err(|_| {
        PlatformError::new(
            PlatformErrorCode::InvalidData,
            "the updated WPS preferences could not be encoded",
        )
    })?;
    let parent = path.parent().ok_or_else(|| {
        PlatformError::invalid_path("the WPS preferences have no parent directory")
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PlatformError::invalid_path("the WPS preferences file name is invalid"))?;

    // Write beside the source and rename only after the new property list is durable. This keeps
    // unrelated WPS settings intact if encoding or disk I/O fails partway through cleanup.
    let mut temporary = None;
    for attempt in 0..16_u8 {
        let candidate = parent.join(format!(
            ".{file_name}.strawberrydisk-{}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(PlatformError::io(
                    "create the updated WPS preferences",
                    &error,
                ));
            }
        }
    }
    let (temporary_path, mut temporary_file) = temporary.ok_or_else(|| {
        PlatformError::new(
            PlatformErrorCode::Io,
            "a temporary WPS preferences file could not be created",
        )
    })?;

    let write_result = (|| -> std::io::Result<()> {
        temporary_file.set_permissions(metadata.permissions())?;
        temporary_file.write_all(&encoded)?;
        temporary_file.sync_all()?;
        drop(temporary_file);
        fs::rename(&temporary_path, path)
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(
            PlatformError::io("replace the WPS preferences", &error).with_possible_side_effects()
        );
    }
    Ok(())
}

pub(super) fn clear(home: &Path) -> PlatformResult<bool> {
    let mut updated_file_count = 0_u64;
    let mut removed_key_count = 0_u64;
    for path in paths(home) {
        let Some((mut preferences, _)) = read_preferences(&path)? else {
            continue;
        };
        let dictionary = preferences
            .as_dictionary_mut()
            .expect("validated dictionary");
        let original_count = dictionary.len();
        dictionary.retain(|key, _| !is_recent_key(key));
        let removed = original_count.saturating_sub(dictionary.len()) as u64;
        if removed == 0 {
            continue;
        }
        replace_preferences(&path, &preferences)?;
        updated_file_count = updated_file_count.saturating_add(1);
        removed_key_count = removed_key_count.saturating_add(removed);
    }

    let remaining = snapshot(home)?.item_count;
    log::info!(
        "macos_wps_recent_documents_cleared updated_file_count={updated_file_count} removed_key_count={removed_key_count} remaining_count={remaining}"
    );
    Ok(remaining == 0)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use plist::Dictionary;

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    fn fixture_home() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "strawberrydisk-wps-recents-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_fixture(home: &Path) -> PathBuf {
        let path = paths(home)[0].clone();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut dictionary = Dictionary::new();
        dictionary.insert(
            "6·0.plugins.officespace.usercenter.Test.openfilelist.1.path".into(),
            Value::String("/Users/test/Documents/report.docx".into()),
        );
        dictionary.insert(
            "6·0.plugins.officespace.usercenter.Test.openfilelist.2.path".into(),
            Value::String("/Users/test/Documents/slides.pptx".into()),
        );
        dictionary.insert(
            "6·0.plugins.officespace.usercenter.Test.openfilelist.recordtime".into(),
            Value::String("2026-09-03".into()),
        );
        dictionary.insert("unrelated.setting".into(), Value::Boolean(true));
        Value::Dictionary(dictionary).to_file_binary(&path).unwrap();
        path
    }

    #[test]
    fn recent_documents_expose_paths_and_clear_only_the_recent_key_family() {
        let home = fixture_home();
        let path = write_fixture(&home);

        let snapshot = snapshot(&home).unwrap();
        let details = details(&home, 0, 10).unwrap();
        assert_eq!(snapshot.item_count, 2);
        assert_eq!(details[0].label, "/Users/test/Documents/report.docx");
        assert_eq!(details[1].label, "/Users/test/Documents/slides.pptx");

        assert!(clear(&home).unwrap());
        let preferences = Value::from_file(&path).unwrap();
        let dictionary = preferences.as_dictionary().unwrap();
        assert_eq!(
            dictionary.get("unrelated.setting"),
            Some(&Value::Boolean(true))
        );
        assert!(dictionary.keys().all(|key| !is_recent_key(key)));
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    #[ignore = "copies an explicitly selected real WPS preferences file"]
    fn actual_preferences_copy_lists_paths_and_clears_without_mutating_the_source() {
        let source = std::env::var_os("STRAWBERRYDISK_TEST_WPS_PREFERENCES")
            .map(PathBuf::from)
            .expect("STRAWBERRYDISK_TEST_WPS_PREFERENCES must name the WPS preferences file");
        let source_before = blake3::hash(&fs::read(&source).unwrap());
        let home = fixture_home();
        let destination = paths(&home)[0].clone();
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(&source, &destination).unwrap();

        let before = snapshot(&home).unwrap();
        let details = details(&home, 0, before.item_count as u32).unwrap();
        assert!(before.item_count > 0);
        assert_eq!(details.len() as u64, before.item_count);
        assert!(details
            .iter()
            .all(|entry| Path::new(&entry.label).is_absolute()));
        assert!(clear(&home).unwrap());
        assert_eq!(snapshot(&home).unwrap().item_count, 0);
        assert_eq!(source_before, blake3::hash(&fs::read(&source).unwrap()));

        fs::remove_dir_all(home).unwrap();
    }
}
