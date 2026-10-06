use std::{
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use strawberrydisk_platform::{current_platform, Platform};

use crate::shared::{CoreError, CoreResult};

use super::metadata::diagnostic_path;

const MAX_EXCLUDED_PATHS: usize = 50;

pub(crate) struct ResolvedExclusionPath {
    pub(crate) path: PathBuf,
    pub(crate) unavailable: bool,
}

/// Resolves exclusion inputs once at the domain boundary. Storage and cleanup retain their own
/// scope policies, but must share validation and physical path spelling, including missing roots.
pub(crate) fn resolve_exclusion_paths(
    requested: &[String],
) -> CoreResult<Vec<ResolvedExclusionPath>> {
    if requested.len() > MAX_EXCLUDED_PATHS {
        log::warn!(
            "scan_exclusion_paths_rejected requested_count={} limit={} reason=tooManyPaths outcome=blocked",
            requested.len(), MAX_EXCLUDED_PATHS
        );
        return Err(CoreError::invalid_input("too many scan exclusion paths"));
    }
    requested
        .iter()
        .map(|value| {
            let path = Path::new(value.trim());
            let result = if value.trim().is_empty() || !path.is_absolute() {
                Err(CoreError::invalid_input(
                    "scan exclusion paths must be absolute directories",
                ))
            } else {
                resolve_exclusion_path(path)
            };
            result.inspect_err(|error| {
                log::warn!(
                    "scan_exclusion_path_rejected path={} error={} outcome=blocked",
                    diagnostic_path(Path::new(value)),
                    strawberrydisk_platform::diagnostics::text(error)
                );
            })
        })
        .collect()
}

fn resolve_exclusion_path(path: &Path) -> CoreResult<ResolvedExclusionPath> {
    let mut ancestor = path;
    let (canonical, unavailable) = loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                // Inspect links even when the leaf is missing. try_exists follows dangling links
                // and would let a missing child bypass the no-links boundary for live roots.
                let canonical = current_platform()
                    .canonicalize_no_links(ancestor)
                    .map_err(|error| CoreError::invalid_input(error.to_string()))?;
                if !canonical.is_dir() {
                    return Err(CoreError::invalid_input(
                        "scan exclusion paths must be directories",
                    ));
                }
                break (canonical, ancestor != path);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if let Some(parent) = ancestor.parent() {
                    ancestor = parent;
                } else {
                    // A disconnected Windows volume/share has no existing ancestor. Preserve
                    // its absolute root and suffix so ordinary scans do not lose saved rules.
                    break (ancestor.to_path_buf(), true);
                }
            }
            Err(error) => {
                return Err(CoreError::operation_failed(format!(
                    "failed to inspect scan exclusion path: {error}"
                )))
            }
        }
    };
    let suffix = path
        .strip_prefix(ancestor)
        .map_err(|error| CoreError::invalid_input(error.to_string()))?;
    // Resolving '..' across a missing component would guess how a future filesystem is laid out.
    // Existing parent components have already been resolved by the platform above.
    if suffix
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(CoreError::invalid_input(
            "missing scan exclusion paths must not contain unresolved parent components",
        ));
    }
    let resolved = canonical.join(suffix);
    if unavailable {
        log::info!(
            "scan_exclusion_path_resolved requested_path={} resolved_path={} outcome=protected_if_restored",
            diagnostic_path(path), diagnostic_path(&resolved)
        );
    }
    Ok(ResolvedExclusionPath {
        path: resolved,
        unavailable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_descendants_use_the_resolved_existing_parent() {
        let fixture = tempfile::tempdir().unwrap();
        fs::create_dir(fixture.path().join("existing")).unwrap();
        let requested = fixture.path().join("existing/../missing/nested");
        let result = resolve_exclusion_paths(&[requested.to_string_lossy().into_owned()]).unwrap();
        assert!(result[0].unavailable);
        assert_eq!(
            result[0].path,
            fs::canonicalize(fixture.path())
                .unwrap()
                .join("missing/nested")
        );
    }

    #[test]
    fn unresolved_parents_and_files_are_rejected() {
        let fixture = tempfile::tempdir().unwrap();
        fs::write(fixture.path().join("file"), b"fixture").unwrap();
        for suffix in ["missing/../keep", "file", "file/child"] {
            assert!(resolve_exclusion_paths(&[fixture
                .path()
                .join(suffix)
                .to_string_lossy()
                .into_owned()])
            .is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn disconnected_volume_roots_remain_in_the_policy() {
        let root = (b'D'..=b'Z')
            .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
            .find(|root| matches!(fs::symlink_metadata(root), Err(error) if error.kind() == ErrorKind::NotFound))
            .expect("the fixture needs an unused drive letter");
        let requested = root.join("excluded/nested");
        let result = resolve_exclusion_paths(&[requested.to_string_lossy().into_owned()]).unwrap();
        assert!(result[0].unavailable);
        assert_eq!(result[0].path, requested);
    }

    #[cfg(unix)]
    #[test]
    fn missing_children_cannot_hide_symbolic_links() {
        let fixture = tempfile::tempdir().unwrap();
        let target = fixture.path().join("target");
        fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, fixture.path().join("link")).unwrap();
        std::os::unix::fs::symlink(
            fixture.path().join("absent"),
            fixture.path().join("dangling"),
        )
        .unwrap();
        for suffix in ["link/missing", "dangling", "dangling/missing"] {
            assert!(resolve_exclusion_paths(&[fixture
                .path()
                .join(suffix)
                .to_string_lossy()
                .into_owned()])
            .is_err());
        }
    }
}
