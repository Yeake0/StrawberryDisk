use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use strawberrydisk_platform::{current_platform, Platform};

use crate::cleanup::{CleanupSourceSelection, CleanupSourceSelectionMode};

const MAX_RULE_SOURCE_SELECTIONS: usize = 256;
const MAX_SELECTED_SOURCE_PATHS: usize = 4_096;

/// Maps a matched file to the stable source row shown by the cleanup scan.
///
/// Files directly under a rule root share the root row; deeper files are
/// grouped by the first child. Scan and execution must use the same mapping so
/// a source checkbox cannot expand or silently change its deletion scope.
pub(crate) fn cleanup_source_path(rule_root: &Path, matched_file: &Path) -> PathBuf {
    let Some(relative) = current_platform().relative_path(matched_file, rule_root) else {
        return rule_root.to_path_buf();
    };
    let mut components = relative.components();
    let Some(first) = components.next() else {
        return rule_root.to_path_buf();
    };
    if components.next().is_none() {
        return rule_root.to_path_buf();
    }
    rule_root.join(first.as_os_str())
}

#[derive(Debug)]
pub(crate) struct SourceSelectionPolicy {
    scopes: HashMap<String, SourceScope>,
}

#[derive(Debug)]
pub(crate) struct SourceScope {
    mode: CleanupSourceSelectionMode,
    path_identity_keys: HashSet<String>,
}

impl SourceSelectionPolicy {
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            scopes: HashMap::new(),
        }
    }

    pub(crate) fn from_request(
        selected_rule_ids: &HashSet<String>,
        selections: &[CleanupSourceSelection],
    ) -> Result<Self, String> {
        if selections.len() > MAX_RULE_SOURCE_SELECTIONS {
            return Err("the cleanup request contains too many source selections".to_string());
        }
        let mut scopes = HashMap::with_capacity(selections.len());
        let mut total_path_count = 0_usize;
        for selection in selections {
            if !selected_rule_ids.contains(&selection.rule_id) {
                return Err("a cleanup source selection references an unselected rule".to_string());
            }
            if selection.paths.is_empty() {
                return Err("a cleanup source selection must contain at least one path".to_string());
            }
            total_path_count = total_path_count.saturating_add(selection.paths.len());
            if total_path_count > MAX_SELECTED_SOURCE_PATHS {
                return Err("the cleanup request contains too many source paths".to_string());
            }
            let paths = selection
                .paths
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            if paths.iter().any(|path| !path.is_absolute()) {
                return Err("cleanup source paths must be unique absolute paths".to_string());
            }
            let path_identity_keys = paths
                .iter()
                .map(|path| current_platform().path_identity_key(path))
                .collect::<HashSet<_>>();
            if path_identity_keys.len() != selection.paths.len() {
                return Err("cleanup source paths must be unique absolute paths".to_string());
            }
            if scopes
                .insert(
                    selection.rule_id.clone(),
                    SourceScope {
                        mode: selection.mode,
                        path_identity_keys,
                    },
                )
                .is_some()
            {
                return Err("the cleanup request contains duplicate source selections".to_string());
            }
        }
        Ok(Self { scopes })
    }

    pub(crate) fn scope(&self, rule_id: &str) -> Option<&SourceScope> {
        self.scopes.get(rule_id)
    }
}

impl SourceScope {
    pub(crate) fn selects(&self, source_path: &Path) -> bool {
        let contains = self
            .path_identity_keys
            .contains(&current_platform().path_identity_key(source_path));
        match self.mode {
            CleanupSourceSelectionMode::Include => contains,
            CleanupSourceSelectionMode::Exclude => !contains,
        }
    }

    /// Rejects stale or fabricated source paths before a cleaner uses the
    /// selection. The scan summary is only presentation data; live discovery
    /// remains the authority for every destructive operation.
    pub(crate) fn validate_known_paths<'a>(
        &self,
        known_paths: impl IntoIterator<Item = &'a Path>,
    ) -> Result<(), String> {
        let known = known_paths
            .into_iter()
            .map(|path| current_platform().path_identity_key(path))
            .collect::<HashSet<_>>();
        if self
            .path_identity_keys
            .iter()
            .all(|path| known.contains(path))
        {
            Ok(())
        } else {
            Err("a selected cleanup source is no longer available".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected_rules() -> HashSet<String> {
        HashSet::from(["app.cache".to_string()])
    }

    fn source_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("strawberrydisk-source-selection")
            .join(name)
    }

    fn path_key(path: &Path) -> String {
        current_platform().path_identity_key(path)
    }

    #[test]
    fn request_rejects_relative_duplicate_and_unselected_sources() {
        let relative = CleanupSourceSelection {
            rule_id: "app.cache".to_string(),
            mode: CleanupSourceSelectionMode::Include,
            paths: vec!["relative".to_string()],
        };
        assert!(SourceSelectionPolicy::from_request(&selected_rules(), &[relative]).is_err());

        let duplicate = CleanupSourceSelection {
            rule_id: "app.cache".to_string(),
            mode: CleanupSourceSelectionMode::Include,
            paths: vec![
                source_path("a").to_string_lossy().into_owned(),
                source_path("a").to_string_lossy().into_owned(),
            ],
        };
        assert!(SourceSelectionPolicy::from_request(&selected_rules(), &[duplicate]).is_err());

        let unselected = CleanupSourceSelection {
            rule_id: "app.other".to_string(),
            mode: CleanupSourceSelectionMode::Include,
            paths: vec![source_path("a").to_string_lossy().into_owned()],
        };
        assert!(SourceSelectionPolicy::from_request(&selected_rules(), &[unselected]).is_err());
    }

    #[test]
    fn include_and_exclude_scopes_validate_live_sources() {
        let first = source_path("a");
        let second = source_path("b");
        let include = SourceScope {
            mode: CleanupSourceSelectionMode::Include,
            path_identity_keys: HashSet::from([path_key(&first)]),
        };
        assert!(include.selects(&first));
        assert!(!include.selects(&second));
        assert!(include
            .validate_known_paths([first.as_path(), second.as_path()])
            .is_ok());

        let exclude = SourceScope {
            mode: CleanupSourceSelectionMode::Exclude,
            path_identity_keys: HashSet::from([path_key(&first)]),
        };
        assert!(!exclude.selects(&first));
        assert!(exclude.selects(&second));
        assert!(exclude.validate_known_paths([second.as_path()]).is_err());
    }

    #[test]
    fn request_enforces_rule_and_path_count_limits() {
        let excessive_rule_count = MAX_RULE_SOURCE_SELECTIONS + 1;
        let selected_rule_ids = (0..excessive_rule_count)
            .map(|index| format!("app.cache-{index}"))
            .collect::<HashSet<_>>();
        let selections = (0..excessive_rule_count)
            .map(|index| CleanupSourceSelection {
                rule_id: format!("app.cache-{index}"),
                mode: CleanupSourceSelectionMode::Include,
                paths: vec![source_path(&format!("rule-{index}"))
                    .to_string_lossy()
                    .into_owned()],
            })
            .collect::<Vec<_>>();
        assert!(SourceSelectionPolicy::from_request(&selected_rule_ids, &selections).is_err());

        let excessive_paths = (0..=MAX_SELECTED_SOURCE_PATHS)
            .map(|index| {
                source_path(&format!("path-{index}"))
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        let selection = CleanupSourceSelection {
            rule_id: "app.cache".to_string(),
            mode: CleanupSourceSelectionMode::Include,
            paths: excessive_paths,
        };
        assert!(SourceSelectionPolicy::from_request(&selected_rules(), &[selection]).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn source_identity_accepts_windows_display_and_verbatim_forms() {
        let selection = CleanupSourceSelection {
            rule_id: "app.cache".to_string(),
            mode: CleanupSourceSelectionMode::Include,
            paths: vec![r"C:\Users\Example\Cache".to_string()],
        };
        let policy = SourceSelectionPolicy::from_request(&selected_rules(), &[selection])
            .expect("the display path should be accepted");
        let scope = policy
            .scope("app.cache")
            .expect("the source scope must exist");
        let canonical = Path::new(r"\\?\c:\users\example\cache");

        assert!(scope.selects(canonical));
        assert!(scope.validate_known_paths([canonical]).is_ok());
    }
}
