use std::{ffi::OsStr, fs, path::Path, time::Duration};

use strawberrydisk_platform::{
    detect_git_executable, run_controlled_command_with_log_policy, ControlledCommandLimits,
    ControlledCommandLogPolicy, ControlledEnvironmentPolicy,
};

use crate::filesystem::metadata::is_link_like;

/// Authored-entry names establish location, not ownership: Anchor keys and tracked source can
/// live inside build output. This predicate is evaluated for every entry during the same
/// measurement traversal, so detecting authored content adds no extra walk. `eq_ignore_ascii_case`
/// matches the previous inspector's semantics on both platforms.
pub(super) fn is_authored_entry_name(name: &OsStr) -> bool {
    const KEYPAIR_SUFFIX: &[u8] = b"-keypair.json";
    let bytes = name.as_encoded_bytes();
    bytes.eq_ignore_ascii_case(b".git")
        || bytes.len() >= KEYPAIR_SUFFIX.len()
            && bytes[bytes.len() - KEYPAIR_SUFFIX.len()..].eq_ignore_ascii_case(KEYPAIR_SUFFIX)
}

/// Confirms that a measured artifact contains no tracked authored content. Entry-name inspection
/// happens inside the measurement traversal; this ownership probe only resolves the enclosing
/// repository and asks Git for tracked files. A missing tool or malformed worktree is unknown
/// ownership, never permission to delete.
pub(super) fn validate_ownership(
    root: &Path,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<(), String> {
    if let Some(name) = root.file_name() {
        if is_authored_entry_name(name) {
            return Err("artifact contains repository metadata or a program keypair".into());
        }
    }
    // Do not ask Git to discover unrelated repositories through inherited GIT_* variables.
    for ancestor in root.ancestors().skip(1) {
        match fs::symlink_metadata(ancestor.join(".git")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
            Ok(metadata) if is_link_like(&metadata) => {
                return Err("repository metadata is a link".into());
            }
            Ok(_) => {}
        }
        let executable =
            detect_git_executable().ok_or("Git unavailable for artifact inspection")?;
        let root = root.to_str().ok_or("artifact path is not valid Unicode")?;
        let output = run_controlled_command_with_log_policy(
            "project-artifact-tracked-files",
            &executable,
            &[
                "--no-optional-locks",
                "--literal-pathspecs",
                "-c",
                "core.fsmonitor=false",
                "-C",
                root,
                "ls-files",
                "--cached",
                "-z",
                "--",
                ".",
            ],
            ControlledEnvironmentPolicy::Isolated,
            ControlledCommandLimits {
                timeout: Duration::from_secs(5),
                stdout_bytes: 1024 * 1024,
                stderr_bytes: 8192,
            },
            ControlledCommandLogPolicy::ExceptionalOnly,
            cancelled,
        )
        .map_err(|error| format!("Git artifact inspection failed: {}", error.as_str()))?;
        if !output.status.success() {
            return Err(format!(
                "Git artifact inspection exited with {}",
                output.status
            ));
        }
        return if output.stdout.is_empty() {
            Ok(())
        } else {
            Err("artifact contains Git-tracked files".into())
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn git(path: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Git fixture failed: {:?}",
            output.status
        );
    }

    #[test]
    fn authored_entry_names_are_matched_case_insensitively() {
        for name in [
            ".git",
            ".GIT",
            "program-keypair.json",
            "Program-Keypair.JSON",
        ] {
            assert!(is_authored_entry_name(&OsString::from(name)), "{name}");
        }
        for name in [
            "git",
            ".github",
            "gitignore",
            "keypair.json",
            "program-keypair.json.bak",
            "target",
        ] {
            assert!(!is_authored_entry_name(&OsString::from(name)), "{name}");
        }
    }

    #[test]
    fn ownership_probe_preserves_tracked_files_nested_links_and_cancellation() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("target");
        fs::create_dir_all(&root).unwrap();
        assert!(validate_ownership(&root, &|| false).is_ok());

        git(fixture.path(), &["init", "--quiet"]);
        fs::write(root.join("source.txt"), b"authored source").unwrap();
        git(fixture.path(), &["add", "target/source.txt"]);
        assert!(validate_ownership(&root, &|| false)
            .unwrap_err()
            .contains("Git-tracked"));
        git(fixture.path(), &["rm", "--cached", "target/source.txt"]);
        assert!(validate_ownership(&root, &|| false).is_ok());
        assert!(validate_ownership(&root, &|| true).is_err());
    }
}
