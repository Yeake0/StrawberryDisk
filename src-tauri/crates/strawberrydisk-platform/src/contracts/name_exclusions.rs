use std::{collections::HashSet, ffi::OsString, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExcludedNameKind {
    File,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanNameExclusion {
    pub name: String,
    pub kind: ExcludedNameKind,
}

/// Compiled exact-name pruning primitive shared by native and portable walkers.
/// Matching is case-sensitive on every platform and never follows links.
#[derive(Debug, Clone, Default)]
pub struct NameExclusions {
    files: HashSet<OsString>,
    folders: HashSet<OsString>,
}

impl NameExclusions {
    pub fn compile(rules: &[ScanNameExclusion]) -> Result<Self, String> {
        if rules.len() > 50 {
            return Err("at most 50 name exclusions are supported".to_string());
        }
        let mut compiled = Self::default();
        for rule in rules {
            let name = &rule.name;
            if name.is_empty()
                || name.trim() != name
                || name.len() > 255
                || matches!(name.as_str(), "." | "..")
                || name
                    .chars()
                    .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
            {
                return Err(
                    "name exclusions require a full name without paths or wildcards".to_string(),
                );
            }
            let names = match rule.kind {
                ExcludedNameKind::File => &mut compiled.files,
                ExcludedNameKind::Folder => &mut compiled.folders,
            };
            if !names.insert(OsString::from(name)) {
                return Err("duplicate name exclusion".to_string());
            }
        }
        Ok(compiled)
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.folders.is_empty()
    }

    /// Native traversal already knows the entry kind; no extra filesystem calls are needed.
    pub fn matches_entry(&self, path: &Path, is_directory: bool) -> bool {
        path.file_name().is_some_and(|name| {
            if is_directory {
                self.folders.contains(name)
            } else {
                self.files.contains(name)
            }
        })
    }

    /// Inspects atomic-delete candidates only when names are active. Unreadable trees fail closed.
    pub fn intersects_tree(
        &self,
        root: &Path,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, String> {
        use crate::Platform;
        if self.is_empty() {
            return Ok(false);
        }
        if self.matches_path(root) {
            return Ok(true);
        }
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            if is_cancelled() {
                return Err("exclusion inspection cancelled".into());
            }
            let metadata = std::fs::symlink_metadata(&directory).map_err(|e| {
                format!(
                    "exclusion inspection failed path={} error={}",
                    crate::diagnostics::text(&directory.display()),
                    crate::diagnostics::text(&e)
                )
            })?;
            if !metadata.is_dir() || crate::current_platform().is_link_like(&metadata) {
                continue;
            }
            for entry in std::fs::read_dir(&directory).map_err(|e| {
                format!(
                    "exclusion directory read failed path={} error={}",
                    crate::diagnostics::text(&directory.display()),
                    crate::diagnostics::text(&e)
                )
            })? {
                if is_cancelled() {
                    return Err("exclusion inspection cancelled".into());
                }
                let entry = entry.map_err(|e| e.to_string())?;
                let kind = entry.file_type().map_err(|e| e.to_string())?;
                if self.matches_entry(&entry.path(), kind.is_dir()) {
                    return Ok(true);
                }
                if kind.is_dir() && !kind.is_symlink() {
                    pending.push(entry.path());
                }
            }
        }
        Ok(false)
    }

    pub fn matches_ancestor(&self, path: &Path) -> bool {
        !self.folders.is_empty()
            && path
                .ancestors()
                .skip(1)
                .any(|parent| self.matches_entry(parent, true))
    }

    /// Only an exact leaf-name hit needs metadata when a caller has not read entry type yet.
    /// The empty-policy path and ordinary nonmatching entries perform no additional I/O.
    pub fn matches_path(&self, path: &Path) -> bool {
        if self.is_empty() {
            return false;
        }
        if self.matches_ancestor(path) {
            return true;
        }
        if !self.matches_entry(path, true) && !self.matches_entry(path, false) {
            return false;
        }
        std::fs::symlink_metadata(path)
            .map_or(true, |metadata| self.matches_entry(path, metadata.is_dir()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_names_preserve_kind_case_and_component_boundaries() {
        let names = NameExclusions::compile(&[
            ScanNameExclusion {
                name: "node_modules".into(),
                kind: ExcludedNameKind::Folder,
            },
            ScanNameExclusion {
                name: "keep.txt".into(),
                kind: ExcludedNameKind::File,
            },
        ])
        .unwrap();
        assert!(names.matches_entry(Path::new("project/node_modules"), true));
        assert!(!names.matches_entry(Path::new("project/node_modules"), false));
        assert!(!names.matches_entry(Path::new("project/NODE_MODULES"), true));
        assert!(!names.matches_entry(Path::new("project/my_node_modules"), true));
        assert!(names.matches_ancestor(Path::new("project/node_modules/lib/a.js")));
        assert!(names.matches_entry(Path::new("project/keep.txt"), false));
        assert!(!names.matches_entry(Path::new("project/keep.txt"), true));
    }

    #[test]
    fn rejects_patterns_paths_and_control_characters() {
        for name in ["", ".", "..", "../cache", "a\\b", "*.tmp", "a\nb", " cache"] {
            assert!(NameExclusions::compile(&[ScanNameExclusion {
                name: name.into(),
                kind: ExcludedNameKind::File
            }])
            .is_err());
        }
    }
}
