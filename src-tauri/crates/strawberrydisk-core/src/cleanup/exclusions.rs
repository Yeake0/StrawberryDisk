use std::path::{Path, PathBuf};

use strawberrydisk_platform::{current_platform, Platform};

use crate::{
    filesystem::{exclusion_paths::resolve_exclusion_paths, metadata::diagnostic_path},
    shared::{CoreError, CoreResult},
};

/// Legacy path roots protect project artifacts; exact names protect every supported cleaner.
#[derive(Debug, Clone, Default)]
pub(super) struct CleanupExclusions {
    names: strawberrydisk_platform::NameExclusions,
    roots: Vec<PathBuf>,
    aliases: Vec<PathBuf>,
}

impl CleanupExclusions {
    pub(super) fn resolve_options(
        options: &crate::filesystem::ScanExclusionOptions,
    ) -> CoreResult<Self> {
        let mut policy = Self::resolve(&options.paths)?;
        policy.names = strawberrydisk_platform::NameExclusions::compile(&options.names)
            .map_err(CoreError::invalid_input)?;
        if !options.names.is_empty() {
            log::info!(
                "cleanup_name_exclusions_configured rule_count={} rules={} outcome=active",
                options.names.len(),
                strawberrydisk_platform::diagnostics::text(&format!("{:?}", options.names))
            );
        }
        Ok(policy)
    }

    pub(super) fn names(&self) -> &strawberrydisk_platform::NameExclusions {
        &self.names
    }

    pub(super) fn has_names(&self) -> bool {
        !self.names.is_empty()
    }

    /// Atomic artifact deletion cannot preserve descendants. Reject an artifact containing an
    /// excluded name, including unreadable trees whose safety cannot be established.
    pub(super) fn protects_tree(&self, path: &Path, is_cancelled: &dyn Fn() -> bool) -> bool {
        if self.intersects(path) {
            return true;
        }
        self.names
            .intersects_tree(path, is_cancelled)
            .unwrap_or_else(|error| {
                log::warn!(
                    "cleanup_exclusion_inspection_failed path={} error={} outcome=retained",
                    diagnostic_path(path),
                    strawberrydisk_platform::diagnostics::text(&error)
                );
                true
            })
    }

    pub(super) fn resolve(requested: &[String]) -> CoreResult<Self> {
        let resolved = resolve_exclusion_paths(requested)?;
        let unavailable = resolved.iter().filter(|root| root.unavailable).count();
        let mut roots = Vec::<PathBuf>::new();
        for resolved_path in resolved {
            let protected_root = resolved_path.path;
            if roots
                .iter()
                .any(|root| current_platform().path_is_same_or_child(&protected_root, root))
            {
                continue;
            }
            roots.retain(|root| !current_platform().path_is_same_or_child(root, &protected_root));
            roots.push(protected_root);
        }
        for root in &roots {
            log::info!(
                "cleanup_exclusion_root_resolved path={}",
                diagnostic_path(root)
            );
        }
        log::info!(
            "cleanup_exclusions_resolved requested_count={} protected_root_count={} unavailable_count={}",
            requested.len(),
            roots.len(),
            unavailable
        );
        let aliases = roots
            .iter()
            .flat_map(|root| current_platform().system_path_aliases(root))
            .collect();
        Ok(Self {
            roots,
            aliases,
            names: Default::default(),
        })
    }

    pub(super) fn matches(&self, path: &Path) -> bool {
        self.names.matches_path(path)
            || self
                .roots
                .iter()
                .chain(&self.aliases)
                .any(|root| current_platform().path_is_same_or_child(path, root))
    }

    pub(super) fn intersects(&self, path: &Path) -> bool {
        self.names.matches_path(path)
            || self.roots.iter().chain(&self.aliases).any(|root| {
                current_platform().path_is_same_or_child(path, root)
                    || current_platform().path_is_same_or_child(root, path)
            })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_temp_alias_matches_the_same_canonical_exclusion() {
        let canonical = PathBuf::from("/private/var");
        let alias = PathBuf::from("/var");
        let exclusions = CleanupExclusions::resolve(&[canonical.to_string_lossy().into_owned()])
            .expect("resolve the canonical exclusion");

        assert!(exclusions.matches(&alias.join("candidate.tmp")));
        assert!(exclusions.intersects(&alias));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn restored_missing_alias_exclusion_protects_canonical_artifact() {
        let fixture = tempfile::tempdir().unwrap();
        let root = current_platform()
            .canonicalize_no_links(fixture.path())
            .unwrap();
        let protected = root.join("artifact/keep");
        let alias = Path::new("/var").join(protected.strip_prefix("/private/var").unwrap());
        let exclusions =
            CleanupExclusions::resolve(&[alias.to_string_lossy().into_owned()]).unwrap();
        fs::create_dir_all(&protected).unwrap();
        assert!(exclusions.matches(&protected));
        assert!(exclusions.protects_tree(&root.join("artifact"), &|| false));
    }

    #[test]
    fn temporarily_unavailable_exclusion_still_prunes_when_it_reappears() {
        let fixture = tempfile::tempdir().expect("create exclusion fixture");
        let excluded = fixture.path().join("reappearing");
        let exclusions = CleanupExclusions::resolve(&[excluded.to_string_lossy().into_owned()])
            .expect("resolve a temporarily unavailable exclusion");

        fs::create_dir(&excluded).expect("restore the excluded directory");
        assert!(exclusions.matches(&excluded.join("candidate.tmp")));
    }
}
