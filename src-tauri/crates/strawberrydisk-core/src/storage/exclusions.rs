use std::path::{Path, PathBuf};

use strawberrydisk_platform::{current_platform, Platform};

use crate::{
    filesystem::{exclusion_paths::resolve_exclusion_paths, metadata::diagnostic_path},
    shared::{CoreError, CoreResult},
};

/// Owns the shared user exclusion policy for storage discovery scans.
///
/// Validation happens once per scan root so every native and generic traversal observes identical
/// path semantics. Missing folders remain in both the configuration fingerprint and the prune
/// policy so a temporarily disconnected folder cannot reappear during a scan and be included.
#[derive(Debug)]
pub(crate) struct StorageScanExclusions {
    roots: Vec<PathBuf>,
    names: strawberrydisk_platform::NameExclusions,
    /// Selected descendant roots join the prune set later, but are not user exclusions.
    configured_root_count: usize,
    configuration_fingerprint: [u8; 32],
    requested_count: usize,
    unavailable_count: usize,
    out_of_scope_count: usize,
}

impl StorageScanExclusions {
    pub(crate) fn resolve_options(
        scan_root: &Path,
        options: &crate::filesystem::ScanExclusionOptions,
    ) -> CoreResult<Self> {
        let mut resolved = Self::resolve(scan_root, &options.paths)?;
        resolved.names = strawberrydisk_platform::NameExclusions::compile(&options.names)
            .map_err(CoreError::invalid_input)?;
        if !options.names.is_empty() {
            let mut names = options.names.clone();
            names.sort_by(|a, b| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| format!("{:?}", a.kind).cmp(&format!("{:?}", b.kind)))
            });
            let mut hasher = blake3::Hasher::new();
            hasher.update(&resolved.configuration_fingerprint);
            hasher.update(b"scan-name-exclusions-v1");
            hasher.update(
                &serde_json::to_vec(&names)
                    .map_err(|error| CoreError::invalid_input(error.to_string()))?,
            );
            resolved.configuration_fingerprint = *hasher.finalize().as_bytes();
            log::info!("storage_scan_name_exclusions_configured root={} rule_count={} rules={} outcome=active", diagnostic_path(scan_root), names.len(), strawberrydisk_platform::diagnostics::text(&format!("{names:?}")));
        }
        Ok(resolved)
    }

    pub(crate) fn names(&self) -> &strawberrydisk_platform::NameExclusions {
        &self.names
    }

    /// Callers supply the canonical scan root and canonical traversal candidates.
    pub(crate) fn resolve(scan_root: &Path, requested: &[String]) -> CoreResult<Self> {
        let resolved = resolve_exclusion_paths(requested)?;
        let requested_count = requested.len();
        let unavailable_count = resolved.iter().filter(|root| root.unavailable).count();
        let mut out_of_scope_count = 0;
        let mut configured_paths = Vec::<PathBuf>::new();
        let mut roots = Vec::<PathBuf>::new();
        for resolved_path in resolved {
            let path = resolved_path.path;
            configured_paths.push(path.clone());
            if !current_platform().path_is_same_or_child(&path, scan_root)
                && !current_platform().path_is_same_or_child(scan_root, &path)
            {
                // A saved exclusion also applies when the selected scan root is equal to or
                // nested inside it. Only disjoint paths belong to another scan scope.
                out_of_scope_count += 1;
                continue;
            }
            if roots
                .iter()
                .any(|root| current_platform().path_is_same_or_child(&path, root))
            {
                continue;
            }
            roots.retain(|root| !current_platform().path_is_same_or_child(root, &path));
            roots.push(path);
        }
        for root in &roots {
            log::info!(
                "storage_scan_exclusion_resolved scan_root={} path={} outcome=protected",
                diagnostic_path(scan_root),
                diagnostic_path(root)
            );
        }

        configured_paths.sort_by_key(|path| current_platform().display_path(path));
        let configuration_fingerprint = if configured_paths.is_empty() {
            [0; 32]
        } else {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"mangodisk-storage-scan-exclusions-v1\0");
            for path in configured_paths {
                hasher.update(current_platform().display_path(&path).as_bytes());
                hasher.update(&[0]);
            }
            *hasher.finalize().as_bytes()
        };

        let configured_root_count = roots.len();
        Ok(Self {
            roots,
            names: strawberrydisk_platform::NameExclusions::default(),
            configured_root_count,
            configuration_fingerprint,
            requested_count,
            unavailable_count,
            out_of_scope_count,
        })
    }

    /// Adds selected descendant roots to the parent traversal's prune set. Each selected child is
    /// scanned separately, avoiding duplicate work without treating it as a user exclusion.
    pub(crate) fn delegate_selected_descendants(
        &mut self,
        scan_root: &Path,
        selected: &[PathBuf],
    ) -> usize {
        let mut delegated = 0;
        for child in selected {
            if current_platform().paths_equal(child, scan_root)
                || !current_platform().path_is_same_or_child(child, scan_root)
                || self.matches(child)
            {
                continue;
            }
            self.roots
                .retain(|root| !current_platform().path_is_same_or_child(root, child));
            self.roots.push(child.clone());
            delegated += 1;
        }
        delegated
    }

    pub(crate) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub(crate) fn configuration_fingerprint(&self) -> [u8; 32] {
        self.configuration_fingerprint
    }

    pub(crate) fn matches(&self, path: &Path) -> bool {
        self.names.matches_path(path)
            || self
                .roots
                .iter()
                .any(|root| current_platform().path_is_same_or_child(path, root))
    }

    pub(crate) fn active_count(&self) -> usize {
        self.configured_root_count
    }

    pub(crate) fn requested_count(&self) -> usize {
        self.requested_count
    }

    pub(crate) fn unavailable_count(&self) -> usize {
        self.unavailable_count
    }

    pub(crate) fn out_of_scope_count(&self) -> usize {
        self.out_of_scope_count
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn selected_descendants_are_scanned_separately_without_excluding_sibling_roots() {
        let parent = std::env::temp_dir().join("strawberrydisk-storage-exclusion-parent");
        let child = parent.join("child");
        let nested = child.join("nested");
        let sibling = parent.join("sibling");
        let selected = vec![
            parent.clone(),
            child.clone(),
            nested.clone(),
            sibling.clone(),
        ];

        let mut parent_exclusions = StorageScanExclusions::resolve(&parent, &[]).unwrap();
        parent_exclusions.delegate_selected_descendants(&parent, &selected);
        assert_eq!(parent_exclusions.active_count(), 0);
        assert!(parent_exclusions.matches(&child.join("candidate.bin")));
        assert!(parent_exclusions.matches(&sibling));
        assert!(!parent_exclusions.matches(&parent.join("included.bin")));

        let mut child_exclusions = StorageScanExclusions::resolve(&child, &[]).unwrap();
        child_exclusions.delegate_selected_descendants(&child, &selected);
        assert!(!child_exclusions.matches(&child.join("candidate.bin")));
        assert!(child_exclusions.matches(&nested));
        assert!(!child_exclusions.matches(&sibling));
    }

    #[test]
    fn collapses_nested_exclusions_and_ignores_other_scopes() {
        let root = std::env::temp_dir().join(format!(
            "strawberrydisk-storage-scan-exclusions-{}",
            std::process::id()
        ));
        let excluded = root.join("excluded");
        let nested = excluded.join("nested");
        let other = std::env::temp_dir().join(format!(
            "strawberrydisk-storage-scan-exclusions-other-{}",
            std::process::id()
        ));
        fs::create_dir_all(&nested).expect("the nested exclusion fixture should be created");
        fs::create_dir_all(&other).expect("the other-scope fixture should be created");
        let canonical_root = current_platform()
            .canonicalize_no_links(&root)
            .expect("the fixture root should resolve");
        let canonical_nested = current_platform()
            .canonicalize_no_links(&nested)
            .expect("the nested fixture should resolve");

        let exclusions = StorageScanExclusions::resolve(
            &canonical_root,
            &[
                nested.to_string_lossy().into_owned(),
                excluded.to_string_lossy().into_owned(),
                other.to_string_lossy().into_owned(),
            ],
        )
        .expect("valid exclusions should resolve");

        assert_eq!(exclusions.active_count(), 1);
        assert_eq!(exclusions.out_of_scope_count(), 1);
        assert!(exclusions.matches(&canonical_nested.join("candidate.bin")));
        assert!(!exclusions.matches(&canonical_root.join("included.bin")));

        fs::remove_dir_all(&root).expect("the fixture root should be removed");
        fs::remove_dir_all(&other).expect("the other-scope fixture should be removed");
    }

    #[test]
    fn excludes_an_explicit_scan_root_when_it_is_inside_a_saved_exclusion() {
        let root = std::env::temp_dir().join(format!(
            "strawberrydisk-storage-root-exclusion-{}",
            std::process::id()
        ));
        let child = root.join("child");
        fs::create_dir_all(&child).expect("create the scan-root fixture");
        let canonical_root = current_platform()
            .canonicalize_no_links(&root)
            .expect("canonicalize the exclusion fixture");
        let canonical_child = current_platform()
            .canonicalize_no_links(&child)
            .expect("canonicalize the scan-root fixture");

        let exclusions = StorageScanExclusions::resolve(
            &canonical_child,
            &[current_platform().display_path(&canonical_root)],
        )
        .expect("resolve the parent exclusion");
        assert!(exclusions.matches(&canonical_child));
        assert_eq!(exclusions.out_of_scope_count(), 0);

        fs::remove_dir_all(&root).expect("remove the scan-root fixture");
    }

    #[test]
    fn temporarily_unavailable_folder_is_pruned_if_it_reappears_during_a_scan() {
        let fixture = tempfile::tempdir().expect("create storage exclusion fixture");
        // Production traversal canonicalizes its root before resolving exclusions. Keep the
        // requested spelling separate so this also covers temporary-directory aliases on macOS.
        let root = current_platform()
            .canonicalize_no_links(fixture.path())
            .unwrap();
        let excluded = fixture.path().join("reappearing");
        let exclusions =
            StorageScanExclusions::resolve(&root, &[excluded.to_string_lossy().into_owned()])
                .expect("resolve a temporarily unavailable folder");

        fs::create_dir(&excluded).expect("restore the excluded folder");
        assert!(exclusions.matches(&root.join("reappearing/candidate.bin")));
    }

    #[test]
    fn configuration_fingerprint_is_root_independent_and_changes_with_preferences() {
        let root = std::env::temp_dir();
        let missing_a = root.join("strawberrydisk-storage-missing-a");
        let missing_b = root.join("strawberrydisk-storage-missing-b");
        let first =
            StorageScanExclusions::resolve(&root, &[missing_a.to_string_lossy().into_owned()])
                .unwrap();
        let second = StorageScanExclusions::resolve(
            &root.join("other"),
            &[missing_a.to_string_lossy().into_owned()],
        )
        .unwrap();
        let changed =
            StorageScanExclusions::resolve(&root, &[missing_b.to_string_lossy().into_owned()])
                .unwrap();

        assert_eq!(
            first.configuration_fingerprint(),
            second.configuration_fingerprint()
        );
        assert_ne!(
            first.configuration_fingerprint(),
            changed.configuration_fingerprint()
        );
        assert_eq!(first.unavailable_count(), 1);
    }

    #[test]
    fn rejects_relative_paths_and_oversized_requests() {
        let error = StorageScanExclusions::resolve(
            &std::env::temp_dir(),
            &["relative/excluded-folder".to_string()],
        )
        .expect_err("relative exclusions must be rejected");
        assert!(error.to_string().contains("must be absolute"));

        let requested = (0..=50)
            .map(|index| {
                std::env::temp_dir()
                    .join(format!("strawberrydisk-missing-exclusion-{index}"))
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        let error = StorageScanExclusions::resolve(&std::env::temp_dir(), &requested)
            .expect_err("oversized exclusion requests must be rejected");
        assert!(error.to_string().contains("too many"));
    }
}
