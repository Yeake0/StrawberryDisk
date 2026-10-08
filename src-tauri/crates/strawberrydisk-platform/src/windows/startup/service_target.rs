use std::path::{Path, PathBuf};

use super::metadata::{filesystem_target_state, FilesystemTargetState};
use super::registry::{expand_environment_variables, split_command_line};

pub(super) struct ServiceTarget {
    pub path: Option<PathBuf>,
    pub arguments: Vec<String>,
    pub state: FilesystemTargetState,
    pub resolution: &'static str,
}

pub(super) fn resolve(command: &str) -> ServiceTarget {
    resolve_with(command, |path| {
        let state = filesystem_target_state(path);
        if state == FilesystemTargetState::Present
            && !std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
        {
            FilesystemTargetState::Unknown
        } else {
            state
        }
    })
}

/// SCM binary paths are executable command lines, not already-tokenized argv.
/// An unquoted executable may contain spaces. Probe prefixes in launch order
/// without executing anything; a missing target still retains its full .exe path.
fn resolve_with(command: &str, probe: impl Fn(&Path) -> FilesystemTargetState) -> ServiceTarget {
    let expanded = expand_environment_variables(command);
    let command = expanded.trim();
    if command.is_empty() || command.contains('\0') {
        return unresolved();
    }
    if let Some(quoted) = command.strip_prefix('"') {
        let Some(end) = quoted.find('"') else {
            return unresolved();
        };
        return target(&quoted[..end], &quoted[end + 1..], "quoted", &probe);
    }

    let mut uncertain_prefix = false;
    let mut missing_executable = None;
    for end in command
        .char_indices()
        .filter_map(|(index, character)| character.is_whitespace().then_some(index))
        .chain(Some(command.len()))
    {
        let prefix = command[..end].trim_end();
        if prefix.is_empty() {
            continue;
        }
        let explicit_executable = prefix.to_ascii_lowercase().ends_with(".exe");
        let candidate = if Path::new(prefix).extension().is_none() {
            format!("{prefix}.exe")
        } else {
            prefix.to_owned()
        };
        let mut resolved = target(&candidate, &command[end..], "unquoted", &probe);
        let present = resolved.state == FilesystemTargetState::Present;
        let uncertain = resolved.state == FilesystemTargetState::Unknown;
        if present || explicit_executable {
            // An inaccessible earlier prefix could be the actual executable.
            // Do not turn uncertainty into evidence for orphan cleanup.
            if uncertain_prefix {
                resolved.state = FilesystemTargetState::Unknown;
            }
            if present {
                return resolved;
            }
            // A directory may itself end in .exe before a space. Keep looking
            // for an existing longer path before using this missing candidate.
            if missing_executable.is_none() {
                missing_executable = Some(resolved);
            }
        }
        uncertain_prefix |= uncertain;
    }
    missing_executable.unwrap_or_else(unresolved)
}

fn target(
    path: &str,
    arguments: &str,
    resolution: &'static str,
    probe: &impl Fn(&Path) -> FilesystemTargetState,
) -> ServiceTarget {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return unresolved();
    }
    ServiceTarget {
        state: probe(&path),
        path: Some(path),
        // Prefix an argv[0] placeholder: CommandLineToArgvW applies special
        // parsing rules to the executable token, not to ordinary arguments.
        arguments: split_command_line(&format!("service {}", arguments.trim()))
            .into_iter()
            .skip(1)
            .collect(),
        resolution,
    }
}

fn unresolved() -> ServiceTarget {
    ServiceTarget {
        path: None,
        arguments: Vec::new(),
        state: FilesystemTargetState::Unknown,
        resolution: "unresolved",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_files_with_spaces_resolve_without_launching_the_executable() {
        let sequence = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("fixture clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "StrawberryDisk service fixture {} {sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&root).expect("create service fixture directory");
        let executable = root.join("fixture service.exe");
        std::fs::write(&executable, b"non-executable test fixture").expect("create inert fixture");
        let result = resolve(&format!("{} --service", executable.display()));
        std::fs::remove_dir_all(&root).expect("remove service fixture directory");
        assert_eq!(result.path.as_deref(), Some(executable.as_path()));
        assert_eq!(result.state, FilesystemTargetState::Present);
        assert_eq!(result.arguments, ["--service"]);
    }

    #[test]
    fn unquoted_service_paths_keep_spaces_and_arguments() {
        let expected = Path::new(r"C:\Program Files (x86)\LetsView\WXCastService.exe");
        let result = resolve_with(
            r#"C:\Program Files (x86)\LetsView\WXCastService.exe --name "two words""#,
            |path| {
                if path == expected {
                    FilesystemTargetState::Present
                } else {
                    FilesystemTargetState::Missing
                }
            },
        );
        assert_eq!(result.path.as_deref(), Some(expected));
        assert_eq!(result.arguments, ["--name", "two words"]);
        assert_eq!(result.state, FilesystemTargetState::Present);
    }

    #[test]
    fn missing_and_inaccessible_paths_are_not_truncated() {
        for state in [
            FilesystemTargetState::Missing,
            FilesystemTargetState::Unknown,
        ] {
            let result = resolve_with(r"C:\Program Files\Vendor\service.exe -service", |_| state);
            assert_eq!(
                result.path.as_deref(),
                Some(Path::new(r"C:\Program Files\Vendor\service.exe"))
            );
            assert_eq!(result.state, state);
        }
    }

    #[test]
    fn quoted_service_paths_do_not_probe_an_unquoted_prefix() {
        let result = resolve_with(r#""C:\Program Files\Vendor\service.exe" /start"#, |path| {
            assert_eq!(path, Path::new(r"C:\Program Files\Vendor\service.exe"));
            FilesystemTargetState::Present
        });
        assert_eq!(result.arguments, ["/start"]);
        assert_eq!(result.resolution, "quoted");
    }

    #[test]
    fn an_existing_earlier_executable_wins_over_a_longer_candidate() {
        let result = resolve_with(r"C:\Program Files\Vendor\service.exe", |path| {
            if path == Path::new(r"C:\Program.exe") {
                FilesystemTargetState::Present
            } else {
                FilesystemTargetState::Missing
            }
        });
        assert_eq!(result.path.as_deref(), Some(Path::new(r"C:\Program.exe")));
    }

    #[test]
    fn executable_suffix_in_a_directory_does_not_hide_an_existing_longer_path() {
        let expected = Path::new(r"C:\Vendor.exe Folder\service.exe");
        let result = resolve_with(r"C:\Vendor.exe Folder\service.exe", |path| {
            if path == expected {
                FilesystemTargetState::Present
            } else {
                FilesystemTargetState::Missing
            }
        });
        assert_eq!(result.path.as_deref(), Some(expected));
        assert_eq!(result.state, FilesystemTargetState::Present);
    }

    #[test]
    fn malformed_and_relative_commands_remain_unresolved() {
        for command in ["", "service.exe", "\"C:\\unterminated", "C:\\bad\0.exe"] {
            let result = resolve_with(command, |_| panic!("invalid path must not be probed"));
            assert!(result.path.is_none());
            assert_eq!(result.state, FilesystemTargetState::Unknown);
        }
    }
}
