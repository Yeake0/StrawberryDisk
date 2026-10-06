use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::Path,
};

use serde::Serialize;
use strawberrydisk_platform::{current_platform, Platform, PlatformErrorCode};

const MAX_ERROR_DETAILS: usize = 3;

#[derive(Debug, Default)]
struct DirectorySelectionDiagnostics {
    rejection_reasons: BTreeMap<&'static str, u64>,
    error_details: BTreeSet<String>,
}

impl DirectorySelectionDiagnostics {
    fn record(&mut self, reason: &'static str, diagnostic: Option<&[u8]>) {
        *self.rejection_reasons.entry(reason).or_default() += 1;
        if let Some(diagnostic) = diagnostic {
            if self.error_details.len() < MAX_ERROR_DETAILS {
                self.error_details
                    .insert(strawberrydisk_platform::diagnostics::text(
                        &String::from_utf8_lossy(diagnostic),
                    ));
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedDirectory {
    pub requested_path: String,
    pub path: String,
}

/// Versioned result preserves alias-to-target mappings for Known Folder labels
/// and saved selections. Rejections never expose native error text.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySelectionOutcome {
    pub schema_version: u32,
    pub directories: Vec<ResolvedDirectory>,
    pub rejected_count: u64,
    pub redirected_count: u64,
    #[serde(skip)]
    diagnostics: DirectorySelectionDiagnostics,
}

impl DirectorySelectionOutcome {
    pub fn rejection_reasons(&self) -> &BTreeMap<&'static str, u64> {
        &self.diagnostics.rejection_reasons
    }

    pub fn error_details(&self) -> &BTreeSet<String> {
        &self.diagnostics.error_details
    }
}

pub struct DirectorySelectionService;

impl DirectorySelectionService {
    /// Resolves selected entry points through the platform path boundary. Keep
    /// each input mapping even when aliases share a target; callers deduplicate
    /// targets only after attaching labels or migrating saved selections.
    pub fn resolve(paths: Vec<String>) -> DirectorySelectionOutcome {
        let mut targets = HashMap::new();
        let mut rejected_count = 0_u64;
        let mut redirected_count = 0_u64;
        let mut diagnostics = DirectorySelectionDiagnostics::default();
        let directories = paths
            .into_iter()
            .filter_map(|path| {
                let trimmed = path.trim();
                if trimmed.is_empty() {
                    rejected_count += 1;
                    diagnostics.record("emptyPath", None);
                    return None;
                }
                let requested = Path::new(trimmed);
                let canonical = match current_platform().resolve_directory_entry(requested) {
                    Ok(path) => path,
                    Err(error) => {
                        rejected_count += 1;
                        diagnostics.record(
                            platform_error_reason(error.code()),
                            Some(format!("path={} error={error}", requested.display()).as_bytes()),
                        );
                        return None;
                    }
                };
                let metadata = match fs::symlink_metadata(&canonical) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        rejected_count += 1;
                        diagnostics.record(
                            metadata_error_reason(error.kind()),
                            Some(format!("path={} error={error}", canonical.display()).as_bytes()),
                        );
                        return None;
                    }
                };
                if !metadata.is_dir() {
                    rejected_count += 1;
                    diagnostics.record("notDirectory", None);
                    return None;
                }
                if !current_platform().paths_equal(requested, &canonical) {
                    redirected_count += 1;
                }
                let key = current_platform().path_identity_key(&canonical);
                let target = targets
                    .entry(key)
                    .or_insert_with(|| current_platform().display_path(&canonical));
                Some(ResolvedDirectory {
                    requested_path: path,
                    path: target.clone(),
                })
            })
            .collect();
        DirectorySelectionOutcome {
            schema_version: 1,
            directories,
            rejected_count,
            redirected_count,
            diagnostics,
        }
    }
}

fn platform_error_reason(code: PlatformErrorCode) -> &'static str {
    match code {
        PlatformErrorCode::AccessDenied => "accessDenied",
        PlatformErrorCode::UserCancelled => "userCancelled",
        PlatformErrorCode::ItemChanged => "itemChanged",
        PlatformErrorCode::InvalidData => "invalidData",
        PlatformErrorCode::InvalidPath => "invalidPath",
        PlatformErrorCode::Io => "io",
        PlatformErrorCode::OperationFailed => "operationFailed",
        PlatformErrorCode::Unsupported => "unsupported",
    }
}

fn metadata_error_reason(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "metadataNotFound",
        std::io::ErrorKind::PermissionDenied => "metadataAccessDenied",
        _ => "metadataUnavailable",
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    struct DirectorySandbox(PathBuf);

    impl Drop for DirectorySandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn directory_filter_maps_windows_path_casing_to_one_target() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "StrawberryDisk-Directory-Identity-{}-{unique}",
            std::process::id()
        ));
        let _sandbox_cleanup = DirectorySandbox(root.clone());
        fs::create_dir_all(&root).expect("the directory identity fixture should be created");
        let original = root.to_string_lossy().into_owned();
        let different_case = original.to_uppercase();

        let outcome = DirectorySelectionService::resolve(vec![original, different_case]);

        assert_eq!(outcome.rejected_count, 0);
        assert_eq!(outcome.directories.len(), 2);
        assert_eq!(outcome.directories[0].path, outcome.directories[1].path);
        assert!(!outcome.directories[0].path.starts_with(r"\\?\"));
    }

    #[test]
    #[ignore = "reads only the explicitly supplied redirected Known Folder entries"]
    fn real_redirected_directory_entries_resolve() {
        let entries = std::env::var("STRAWBERRYDISK_TEST_DIRECTORY_ENTRIES")
            .expect("supply semicolon-separated redirected directory entries");
        let entries: Vec<String> = entries.split(';').map(str::to_owned).collect();
        let expected = entries.len();
        let started = std::time::Instant::now();
        let outcome = DirectorySelectionService::resolve(entries);
        assert_eq!(outcome.rejected_count, 0);
        assert_eq!(outcome.directories.len(), expected);
        for directory in &outcome.directories {
            assert!(current_platform()
                .canonicalize_no_links(Path::new(&directory.path))
                .is_ok());
        }
        println!(
            "resolved_count={} redirected_count={} elapsed_ms={}",
            outcome.directories.len(),
            outcome.redirected_count,
            started.elapsed().as_millis()
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejected_directory_retains_target_and_native_reason() {
        let path = std::env::temp_dir().join(format!(
            "strawberrydisk-missing-directory-{}",
            std::process::id()
        ));
        assert!(!path.exists());
        let outcome =
            super::DirectorySelectionService::resolve(vec![path.to_string_lossy().into_owned()]);
        assert_eq!(outcome.rejected_count, 1);
        let diagnostic = outcome.error_details().iter().next().unwrap();
        assert!(diagnostic.contains("strawberrydisk-missing-directory"));
        assert!(diagnostic.contains("error="));
        println!(
            "directory_fixture_rejected reasons={:?} error_details={:?}",
            outcome.rejection_reasons(),
            outcome.error_details()
        );
    }

    use super::*;

    #[test]
    fn regular_directories_remain_available_while_files_and_missing_paths_are_rejected() {
        let root = std::env::temp_dir().join(format!(
            "StrawberryDisk-Folder-Filter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("plain.txt"), b"fixture").unwrap();
        let requested = root.to_string_lossy().into_owned();
        let outcome = DirectorySelectionService::resolve(vec![
            requested.clone(),
            root.join("plain.txt").to_string_lossy().into_owned(),
            root.join("missing").to_string_lossy().into_owned(),
        ]);
        fs::remove_file(root.join("plain.txt")).unwrap();
        fs::remove_dir(&root).unwrap();
        assert_eq!(outcome.rejected_count, 2);
        assert_eq!(outcome.rejection_reasons().values().sum::<u64>(), 2);
        assert!(outcome.error_details().len() <= MAX_ERROR_DETAILS);
        assert_eq!(outcome.directories.len(), 1);
        assert_eq!(outcome.directories[0].requested_path, requested);
    }

    #[test]
    fn directory_entry_protocol_keeps_mappings_and_excludes_error_text() {
        let outcome = DirectorySelectionService::resolve(vec![String::new()]);
        assert_eq!(outcome.rejected_count, 1);
        let value = serde_json::to_value(outcome).unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["directories"], serde_json::json!([]));
        assert!(value.get("diagnostics").is_none());
        assert!(value.get("error").is_none());
    }

    #[test]
    fn rejection_diagnostics_keep_only_bounded_error_details() {
        let mut diagnostics = DirectorySelectionDiagnostics::default();
        for diagnostic in [b"one".as_slice(), b"two", b"three", b"four", b"five"] {
            diagnostics.record("io", Some(diagnostic));
        }
        assert_eq!(diagnostics.rejection_reasons["io"], 5);
        assert_eq!(diagnostics.error_details.len(), MAX_ERROR_DETAILS);
    }
}
