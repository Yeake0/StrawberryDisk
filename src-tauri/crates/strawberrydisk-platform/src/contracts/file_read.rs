use std::{io, path::Path};

use serde::Serialize;

/// Bounded per-aggregate diagnostics; merges retain this same upper bound.
pub const MAX_FILE_READ_FAILURE_DETAILS: usize = 50;

/// The filesystem operation that failed, independent of product/UI guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileReadStage {
    OpenDirectory,
    ReadDirectory,
    ReadMetadata,
}

impl FileReadStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::OpenDirectory => "open_directory",
            Self::ReadDirectory => "read_directory",
            Self::ReadMetadata => "read_metadata",
        }
    }
}

/// Stable classification for presentation; native messages remain diagnostic evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileReadFailureReason {
    PermissionDenied,
    IoError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReadFailureDetail {
    pub path: String,
    pub stage: FileReadStage,
    pub reason: FileReadFailureReason,
    pub os_error: Option<i32>,
    pub error: String,
    pub privacy_restriction_possible: bool,
}

/// Read failures are separate from intentional link, mount, and placeholder skips.
/// Counts describe failed read operations, not the number of files hidden below them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileReadFailures {
    pub count: u64,
    pub details: Vec<FileReadFailureDetail>,
    /// Native permission denials, including possible privacy restrictions.
    pub permission_denied_count: u64,
    /// macOS protected app-data reads for which privacy settings may help.
    /// This is an observation, not proof that Full Disk Access is disabled.
    pub privacy_restricted_count: u64,
}

impl FileReadFailures {
    pub fn record(&mut self, path: &Path, error: &io::Error, stage: FileReadStage) {
        self.record_with_sequence(path, error, stage, |count| count);
    }

    /// Parallel directory readers share a sequence so each child cannot reset the warning budget.
    #[cfg(target_os = "macos")]
    pub(crate) fn record_in_scope(
        &mut self,
        path: &Path,
        error: &io::Error,
        stage: FileReadStage,
        sequence: &std::sync::atomic::AtomicU64,
    ) {
        self.record_with_sequence(path, error, stage, |_| {
            sequence
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .saturating_add(1)
        });
    }

    fn record_with_sequence(
        &mut self,
        path: &Path,
        error: &io::Error,
        stage: FileReadStage,
        next_sequence: impl FnOnce(u64) -> u64,
    ) {
        // Applications routinely remove cache entries during a scan.
        if error.kind() == io::ErrorKind::NotFound {
            return;
        }
        self.count = self.count.saturating_add(1);
        let permission_denied = error.kind() == io::ErrorKind::PermissionDenied;
        if permission_denied {
            self.permission_denied_count = self.permission_denied_count.saturating_add(1);
        }
        #[cfg(target_os = "macos")]
        let privacy_restriction_possible =
            crate::macos::file_read::is_privacy_restricted(path, error);
        #[cfg(not(target_os = "macos"))]
        let privacy_restriction_possible = false;
        if privacy_restriction_possible {
            self.privacy_restricted_count = self.privacy_restricted_count.saturating_add(1);
        }
        if self.details.len() < MAX_FILE_READ_FAILURE_DETAILS {
            self.details.push(FileReadFailureDetail {
                path: path.to_string_lossy().into_owned(),
                stage,
                reason: if permission_denied {
                    FileReadFailureReason::PermissionDenied
                } else {
                    FileReadFailureReason::IoError
                },
                os_error: error.raw_os_error(),
                error: crate::diagnostics::bounded_message(error, 512),
                privacy_restriction_possible,
            });
        }
        // Keep representative failures in default logs. Remaining failures stay
        // available at Debug; merged counters retain every failure in this scope.
        let scope_failure_count = next_sequence(self.count);
        let level = if scope_failure_count <= 3 {
            log::Level::Warn
        } else {
            log::Level::Debug
        };
        log::log!(
            level,
            "filesystem_read_failed stage={} path={} error_kind={:?} os_error={:?} privacy_restriction_possible={} error={} outcome=skipped scope_failure_count={}",
            stage.as_str(),
            crate::diagnostics::text(&path.display()),
            error.kind(),
            error.raw_os_error(),
            privacy_restriction_possible,
            crate::diagnostics::text(error),
            scope_failure_count
        );
    }

    pub fn merge(&mut self, other: &Self) {
        let remaining = MAX_FILE_READ_FAILURE_DETAILS.saturating_sub(self.details.len());
        self.details
            .extend(other.details.iter().take(remaining).cloned());
        self.count = self.count.saturating_add(other.count);
        self.permission_denied_count = self
            .permission_denied_count
            .saturating_add(other.permission_denied_count);
        self.privacy_restricted_count = self
            .privacy_restricted_count
            .saturating_add(other.privacy_restricted_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_failures_ignore_disappeared_entries_and_keep_other_errors() {
        let mut failures = FileReadFailures::default();
        let path = Path::new("fixture/cache");
        failures.record(
            path,
            &io::Error::from(io::ErrorKind::NotFound),
            FileReadStage::ReadMetadata,
        );
        assert_eq!(failures.count, 0);
        failures.record(
            path,
            &io::Error::from(io::ErrorKind::PermissionDenied),
            FileReadStage::OpenDirectory,
        );
        failures.record(
            path,
            &io::Error::from(io::ErrorKind::Other),
            FileReadStage::ReadDirectory,
        );
        assert_eq!(failures.count, 2);
        assert_eq!(failures.permission_denied_count, 1);
        assert_eq!(failures.privacy_restricted_count, 0);
    }

    #[test]
    fn failure_details_are_bounded_without_losing_counts_when_merged() {
        let mut discovery = FileReadFailures::default();
        let mut traversal = FileReadFailures::default();
        for index in 0..25 {
            discovery.record(
                Path::new(&format!("fixture/discovery-{index}")),
                &io::Error::from(io::ErrorKind::PermissionDenied),
                FileReadStage::OpenDirectory,
            );
        }
        for index in 0..90 {
            traversal.record(
                Path::new(&format!("fixture/traversal-{index}")),
                &io::Error::from(io::ErrorKind::Other),
                FileReadStage::ReadMetadata,
            );
        }
        assert_eq!(traversal.details.len(), MAX_FILE_READ_FAILURE_DETAILS);
        discovery.merge(&traversal);
        assert_eq!(discovery.count, 115);
        assert_eq!(discovery.permission_denied_count, 25);
        assert_eq!(discovery.details.len(), MAX_FILE_READ_FAILURE_DETAILS);
        assert_eq!(discovery.details[24].path, "fixture/discovery-24");
        assert_eq!(discovery.details[25].path, "fixture/traversal-0");
        assert_eq!(discovery.details[49].path, "fixture/traversal-24");
        assert_eq!(traversal.count, 90);
    }

    #[test]
    fn failure_details_keep_typed_diagnostics_and_bound_native_messages() {
        let mut failures = FileReadFailures::default();
        failures.record(
            Path::new("fixture/disappeared"),
            &io::Error::from(io::ErrorKind::NotFound),
            FileReadStage::ReadMetadata,
        );
        assert!(failures.details.is_empty());
        failures.record(
            Path::new("fixture/cache"),
            &io::Error::other("native error ".repeat(100)),
            FileReadStage::ReadDirectory,
        );
        let detail = &failures.details[0];
        assert_eq!(detail.reason, FileReadFailureReason::IoError);
        assert_eq!(detail.stage, FileReadStage::ReadDirectory);
        assert!(detail.error.ends_with("…[truncated]"));
        assert!(detail.error.chars().count() < 530);
        let payload = serde_json::to_value(detail).expect("diagnostics must serialize");
        assert_eq!(payload["path"], "fixture/cache");
        assert_eq!(payload["stage"], "readDirectory");
        assert_eq!(payload["reason"], "ioError");
        assert_eq!(payload["osError"], serde_json::Value::Null);
        assert_eq!(payload["privacyRestrictionPossible"], false);
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_failed_paths_do_not_break_scan_serialization() {
        use std::os::unix::ffi::OsStringExt;
        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 0xff]));
        let mut failures = FileReadFailures::default();
        failures.record(
            &path,
            &io::Error::from(io::ErrorKind::Other),
            FileReadStage::ReadMetadata,
        );
        let payload =
            serde_json::to_value(&failures.details).expect("native paths must remain serializable");
        assert!(payload[0]["path"].as_str().unwrap().contains('\u{fffd}'));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn merged_failures_distinguish_native_denials_from_possible_privacy_restrictions() {
        let protected = dirs::home_dir()
            .expect("home must be available")
            .join("Library/Containers/fixture/Data/Library/Caches");
        let ordinary = Path::new("fixture/cache");
        let mut discovery = FileReadFailures::default();
        discovery.record(
            &protected,
            &io::Error::from_raw_os_error(libc::EPERM),
            FileReadStage::OpenDirectory,
        );
        discovery.record(
            &protected,
            &io::Error::from_raw_os_error(libc::EACCES),
            FileReadStage::ReadMetadata,
        );
        let mut traversal = FileReadFailures::default();
        traversal.record(
            ordinary,
            &io::Error::from_raw_os_error(libc::EPERM),
            FileReadStage::OpenDirectory,
        );
        traversal.record(
            ordinary,
            &io::Error::from_raw_os_error(libc::EIO),
            FileReadStage::ReadDirectory,
        );
        traversal.record(
            ordinary,
            &io::Error::from_raw_os_error(libc::ENOENT),
            FileReadStage::ReadMetadata,
        );
        discovery.merge(&traversal);
        assert_eq!(discovery.count, 4);
        assert_eq!(discovery.permission_denied_count, 3);
        assert_eq!(discovery.privacy_restricted_count, 1);
    }
}
