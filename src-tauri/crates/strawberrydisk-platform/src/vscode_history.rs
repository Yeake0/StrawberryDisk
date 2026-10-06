use std::{fs, path::Path};

use serde::Deserialize;

use crate::{PlatformError, PlatformErrorCode, PlatformPrivacyDetailEntry, PlatformResult};

const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RESOURCE_COUNT: usize = 50_000;

#[derive(Debug, Deserialize)]
struct HistoryManifest {
    resource: String,
    entries: Vec<serde_json::Value>,
}

#[derive(Debug)]
struct ResourceHistory {
    label: String,
    entry_count: u64,
}

#[derive(Debug)]
pub(crate) struct Snapshot {
    pub(crate) item_count: u64,
    pub(crate) revision: String,
}

fn histories_and_revision(root: &Path) -> PlatformResult<(Vec<ResourceHistory>, String)> {
    let mut revision = blake3::Hasher::new();
    revision.update(b"mangodisk-vscode-local-history-v2\0");
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), revision.finalize().to_hex().to_string()));
        }
        Err(error) => return Err(PlatformError::io("inspect VS Code local history", &error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PlatformError::invalid_path(
            "VS Code local history is not a safe directory",
        ));
    }

    let mut resource_directories = fs::read_dir(root)
        .map_err(|error| PlatformError::io("list VS Code local history", &error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| PlatformError::io("read VS Code history resource", &error))
        })
        .collect::<PlatformResult<Vec<_>>>()?;
    resource_directories.sort();
    if resource_directories.len() > MAX_RESOURCE_COUNT {
        return Err(PlatformError::new(
            PlatformErrorCode::InvalidData,
            "VS Code local history exceeds the supported resource count",
        ));
    }

    let mut histories = Vec::new();
    for directory in resource_directories {
        let metadata = fs::symlink_metadata(&directory)
            .map_err(|error| PlatformError::io("inspect VS Code history resource", &error))?;
        if metadata.file_type().is_symlink() {
            return Err(PlatformError::invalid_path(
                "VS Code history resource is link-like",
            ));
        }
        if !metadata.is_dir() {
            continue;
        }
        let manifest_path = directory.join("entries.json");
        let metadata = match fs::symlink_metadata(&manifest_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(PlatformError::io(
                    "inspect VS Code history manifest",
                    &error,
                ));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PlatformError::invalid_path(
                "VS Code history manifest is not a safe regular file",
            ));
        }
        if metadata.len() > MAX_MANIFEST_BYTES {
            return Err(PlatformError::new(
                PlatformErrorCode::InvalidData,
                "VS Code history manifest exceeds the supported size",
            ));
        }
        let bytes = fs::read(&manifest_path)
            .map_err(|error| PlatformError::io("read VS Code history manifest", &error))?;
        revision.update(
            directory
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_bytes(),
        );
        revision.update(blake3::hash(&bytes).as_bytes());
        let manifest = serde_json::from_slice::<HistoryManifest>(&bytes).map_err(|_| {
            PlatformError::new(
                PlatformErrorCode::InvalidData,
                "VS Code history manifest has an unsupported format",
            )
        })?;
        if manifest.resource.trim().is_empty() || manifest.entries.is_empty() {
            continue;
        }
        histories.push(ResourceHistory {
            label: resource_label(&manifest.resource),
            entry_count: manifest.entries.len() as u64,
        });
    }
    let item_count = histories
        .iter()
        .map(|history| history.entry_count)
        .sum::<u64>();
    log::debug!(
        "vscode_history_scanned resource_count={} item_count={}",
        histories.len(),
        item_count
    );
    Ok((histories, revision.finalize().to_hex().to_string()))
}

/// Converts the URI stored by VS Code and Office into a readable local path when possible.
/// Non-file schemes are preserved because values such as `vscode-userdata:` identify real
/// resources but do not have a filesystem path.
pub(crate) fn resource_label(resource: &str) -> String {
    let (value, file_uri) = resource
        .strip_prefix("file://")
        .map_or((resource, false), |value| (value, true));
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    let mut label = String::from_utf8_lossy(&decoded).into_owned();
    if file_uri && cfg!(windows) {
        if label
            .as_bytes()
            .get(1)
            .is_some_and(|value| value.is_ascii_alphabetic())
            && label.as_bytes().get(2) == Some(&b':')
        {
            label.remove(0);
        } else if !label.starts_with('/') {
            label.insert_str(0, "//");
        }
        label = label.replace('/', "\\");
    }
    sanitize_label(&label)
}

fn sanitize_label(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(2_048)
        .collect()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn snapshot(root: &Path) -> PlatformResult<Snapshot> {
    let (histories, revision) = histories_and_revision(root)?;
    Ok(Snapshot {
        item_count: histories.iter().map(|history| history.entry_count).sum(),
        revision,
    })
}

pub(crate) fn details(
    root: &Path,
    offset: u64,
    limit: u32,
) -> PlatformResult<Vec<PlatformPrivacyDetailEntry>> {
    let (histories, _) = histories_and_revision(root)?;
    Ok(histories
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|history| PlatformPrivacyDetailEntry {
            label: history.label,
            item_count: history.entry_count,
        })
        .collect())
}

pub(crate) fn clear(root: &Path) -> PlatformResult<bool> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(PlatformError::io("inspect VS Code local history", &error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PlatformError::invalid_path(
            "VS Code local history is not a safe directory",
        ));
    }
    fs::remove_dir_all(root).map_err(|error| {
        PlatformError::io("remove VS Code local history", &error).with_possible_side_effects()
    })?;
    let remaining = snapshot(root)?.item_count;
    log::info!("vscode_history_cleared remaining_count={remaining}");
    Ok(remaining == 0)
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    fn fixture_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "strawberrydisk-vscode-history-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_manifest(root: &Path, directory: &str, resource: &str, entry_count: usize) {
        let resource_root = root.join(directory);
        fs::create_dir_all(&resource_root).unwrap();
        let entries = (0..entry_count)
            .map(|index| serde_json::json!({ "id": format!("entry-{index}") }))
            .collect::<Vec<_>>();
        fs::write(
            resource_root.join("entries.json"),
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "resource": resource,
                "entries": entries,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn counts_records_and_displays_decoded_resource_paths() {
        let root = fixture_root();
        write_manifest(&root, "-hash-one", "file:///Users/test/My%20File.txt", 2);
        write_manifest(&root, "-hash-two", "vscode-userdata:/settings.json", 1);

        let scan = snapshot(&root).unwrap();
        let details = details(&root, 0, 10).unwrap();

        assert_eq!(scan.item_count, 3);
        assert_eq!(details.len(), 2);
        assert!(!details.iter().any(|entry| entry.label.starts_with("-hash")));
        #[cfg(target_os = "macos")]
        assert_eq!(details[0].label, "/Users/test/My File.txt");
        #[cfg(windows)]
        assert_eq!(details[0].label, "\\Users\\test\\My File.txt");
        assert_eq!(details[0].item_count, 2);
        assert_eq!(details[1].label, "vscode-userdata:/settings.json");
        assert!(clear(&root).unwrap());
        assert_eq!(snapshot(&root).unwrap().item_count, 0);
    }
}
