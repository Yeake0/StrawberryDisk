use std::{
    collections::{hash_map::Entry, HashMap},
    path::PathBuf,
};

use mangodisk_platform::{FilesystemChangeToken, PhysicalFileIdentity};

use crate::storage::index::cache::{DirectoryAggregate, IndexedFile};

/// Collects one completed scan in memory.
///
/// Scan results are rebuildable, session-scoped data. Keeping a single authoritative result avoids
/// duplicating millions of derived records before the UI can render them. The bounded cache that
/// owns the completed sink decides when an older root is released.
pub(super) struct IndexRecordSink {
    directories: HashMap<PathBuf, DirectoryAggregate>,
    files: HashMap<PathBuf, IndexedFile>,
    hard_links: HashMap<(u64, u64), Vec<(PathBuf, IndexedFile)>>,
    change_token: Option<FilesystemChangeToken>,
}

pub(super) struct CompletedIndexSink {
    pub(super) directories: HashMap<PathBuf, DirectoryAggregate>,
    pub(super) files: HashMap<PathBuf, IndexedFile>,
    pub(super) change_token: Option<FilesystemChangeToken>,
}

impl IndexRecordSink {
    pub(super) fn memory(change_token: Option<FilesystemChangeToken>) -> Self {
        Self {
            directories: HashMap::new(),
            files: HashMap::new(),
            hard_links: HashMap::new(),
            change_token,
        }
    }

    pub(super) fn push_directory(
        &mut self,
        path: PathBuf,
        aggregate: DirectoryAggregate,
    ) -> Result<(), String> {
        if self.directories.insert(path, aggregate).is_some() {
            return Err("the in-memory index received a duplicate directory record".to_string());
        }
        Ok(())
    }

    pub(super) fn push_large_file(
        &mut self,
        path: PathBuf,
        file: IndexedFile,
    ) -> Result<(), String> {
        if self.files.insert(path, file).is_some() {
            return Err("the in-memory index received a duplicate large-file record".to_string());
        }
        Ok(())
    }

    /// Inserts a validated record from an advisory candidate source.
    ///
    /// Native filesystem indexes may repeat a path, so candidate ingestion is idempotent while
    /// authoritative traversal streams keep their strict duplicate checks.
    pub(super) fn insert_large_file_candidate(&mut self, path: PathBuf, file: IndexedFile) -> bool {
        match self.files.entry(path) {
            Entry::Vacant(entry) => {
                entry.insert(file);
                true
            }
            Entry::Occupied(_) => false,
        }
    }

    pub(super) fn finish(self) -> Result<CompletedIndexSink, String> {
        Ok(CompletedIndexSink {
            directories: self.directories,
            files: self.files,
            change_token: self.change_token,
        })
    }

    pub(super) fn push_hard_link(
        &mut self,
        path: PathBuf,
        identity: PhysicalFileIdentity,
        file: IndexedFile,
    ) {
        self.hard_links
            .entry((identity.volume, identity.index))
            .or_default()
            .push((path, file));
    }

    /// Assign shared allocation to a stable path within this scan, regardless of worker ordering.
    /// Only multiply linked files are retained; ordinary files add no identity-map overhead.
    pub(super) fn finish_analysis(mut self) -> Result<CompletedIndexSink, String> {
        for mut links in self.hard_links.into_values() {
            links.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            for (path, mut file) in links.into_iter().skip(1) {
                for ancestor in path.ancestors().skip(1) {
                    if let Some(directory) = self.directories.get_mut(ancestor) {
                        directory.bytes =
                            directory.bytes.checked_sub(file.bytes).ok_or_else(|| {
                                "hard-link allocation exceeds the containing directory".to_string()
                            })?;
                        // Removing a link can change allocation ownership in another subtree.
                        directory.fingerprint = None;
                    }
                }
                if file.bytes > 0 {
                    if let Some(parent) = path
                        .parent()
                        .and_then(|parent| self.directories.get_mut(parent))
                    {
                        parent.direct_file_count =
                            parent.direct_file_count.checked_sub(1).ok_or_else(|| {
                                "hard-link count exceeds the containing directory".to_string()
                            })?;
                    }
                }
                file.bytes = 0;
                // Keep zero-charge aliases even below the candidate floor so live row assembly
                // cannot reintroduce their full allocation through its metadata fallback.
                self.files.insert(path, file);
            }
        }
        Ok(CompletedIndexSink {
            directories: self.directories,
            files: self.files,
            change_token: self.change_token,
        })
    }
}
