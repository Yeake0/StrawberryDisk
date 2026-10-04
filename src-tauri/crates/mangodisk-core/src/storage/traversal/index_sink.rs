use std::{
    cmp::Reverse,
    collections::{hash_map::Entry, BinaryHeap, HashMap},
    path::PathBuf,
};

use mangodisk_platform::{
    current_platform, FastAnalysisFile, FilesystemChangeToken, PhysicalFileIdentity, Platform,
    ScanPurpose,
};

use crate::storage::large_files::LARGE_FILE_CANDIDATE_FLOOR_BYTES;

use crate::storage::index::cache::{DirectoryAggregate, IndexedFile};

const ANALYSIS_FILES_PER_DIRECTORY: usize = 64;
const ANALYSIS_FILE_BUDGET: usize = 8192;

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
    analysis_files: AnalysisCandidates,
}

pub(super) struct CompletedIndexSink {
    pub(super) directories: HashMap<PathBuf, DirectoryAggregate>,
    pub(super) files: HashMap<PathBuf, IndexedFile>,
    pub(super) change_token: Option<FilesystemChangeToken>,
}

/// Keep a bounded allocation-ranked subset, with deterministic path ties.
/// The scan-wide budget caps extra retained metadata independently of file count.
pub(super) struct AnalysisCandidates {
    files: BinaryHeap<Reverse<FastAnalysisFile>>,
    limit: usize,
}
impl AnalysisCandidates {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            files: BinaryHeap::new(),
            limit,
        }
    }
    pub(super) fn would_retain(&self, bytes: u64, path: &std::path::Path) -> bool {
        bytes > 0
            && self.limit > 0
            && (self.files.len() < self.limit
                || self.files.peek().is_some_and(|smallest| {
                    bytes > smallest.0.allocated_bytes
                        || (bytes == smallest.0.allocated_bytes && path < smallest.0.path.as_path())
                }))
    }
    pub(super) fn push(&mut self, file: FastAnalysisFile) {
        if file.allocated_bytes == 0 || self.limit == 0 {
            return;
        }
        if self.files.len() < self.limit {
            self.files.push(Reverse(file));
        } else if self.files.peek().is_some_and(|smallest| file > smallest.0) {
            *self
                .files
                .peek_mut()
                .expect("a full candidate heap is nonempty") = Reverse(file);
        }
    }
    pub(super) fn into_files(self) -> impl Iterator<Item = FastAnalysisFile> {
        self.files.into_iter().map(|file| file.0)
    }
}

impl IndexRecordSink {
    pub(super) fn memory(change_token: Option<FilesystemChangeToken>) -> Self {
        Self {
            directories: HashMap::new(),
            files: HashMap::new(),
            hard_links: HashMap::new(),
            change_token,
            analysis_files: AnalysisCandidates::new(ANALYSIS_FILE_BUDGET),
        }
    }

    pub(super) fn push_analysis_file(&mut self, file: FastAnalysisFile) {
        self.analysis_files.push(file);
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
        // Ownership is resolved after native enumeration. Bound its per-directory
        // candidate buffers too, prioritizing large branches when the budget is full.
        let parent_limit = ANALYSIS_FILE_BUDGET / ANALYSIS_FILES_PER_DIRECTORY;
        let mut parents = BinaryHeap::new();
        for (path, directory) in &self.directories {
            if directory.direct_file_count == 0 {
                continue;
            }
            let rank = (directory.bytes, path.as_path());
            if parents.len() < parent_limit {
                parents.push(Reverse(rank));
            } else if parents.peek().is_some_and(|smallest| rank > smallest.0) {
                *parents.peek_mut().expect("a full parent heap is nonempty") = Reverse(rank);
            }
        }
        let mut owner_files: HashMap<PathBuf, AnalysisCandidates> = parents
            .into_iter()
            .map(|Reverse((_, path))| {
                (
                    path.to_path_buf(),
                    AnalysisCandidates::new(ANALYSIS_FILES_PER_DIRECTORY),
                )
            })
            .collect();
        for mut links in self.hard_links.into_values() {
            links.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            // Select charged owners only after all aliases are known. Ordinary files
            // have already been bounded by their native directory reader.
            if let Some((path, file)) = links.first() {
                if let Some(candidates) =
                    path.parent().and_then(|parent| owner_files.get_mut(parent))
                {
                    if file.bytes > 0
                        && file.bytes < LARGE_FILE_CANDIDATE_FLOOR_BYTES
                        && candidates.would_retain(file.bytes, path)
                        && current_platform()
                            .should_skip(
                                path,
                                path.parent().unwrap_or(path),
                                ScanPurpose::LargeFiles,
                            )
                            .is_none()
                    {
                        candidates.push(FastAnalysisFile {
                            path: path.clone(),
                            allocated_bytes: file.bytes,
                            logical_bytes: file.logical_bytes,
                            modified_at_ms: file.modified_at_ms,
                        });
                    }
                }
            }
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
        for candidates in owner_files.into_values() {
            for file in candidates.into_files() {
                self.analysis_files.push(file);
            }
        }
        let mut candidates = self.analysis_files.into_files().collect::<Vec<_>>();
        candidates.sort_unstable_by(|left, right| right.cmp(left));
        let mut parent_counts = HashMap::new();
        for file in candidates {
            if self
                .files
                .get(&file.path)
                .is_some_and(|indexed| indexed.bytes == 0)
            {
                continue;
            }
            let Some(parent) = file.path.parent() else {
                continue;
            };
            let count = parent_counts.entry(parent.to_path_buf()).or_insert(0);
            if *count >= ANALYSIS_FILES_PER_DIRECTORY {
                continue;
            }
            *count += 1;
            // Zero-charge aliases remain authoritative even if a candidate was stale.
            self.files.entry(file.path).or_insert(IndexedFile {
                bytes: file.allocated_bytes,
                logical_bytes: file.logical_bytes,
                modified_at_ms: file.modified_at_ms,
            });
        }
        Ok(CompletedIndexSink {
            directories: self.directories,
            files: self.files,
            change_token: self.change_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplemental_analysis_files_stay_bounded_and_never_replace_zero_charge_aliases() {
        let mut sink = IndexRecordSink::memory(None);
        for index in (0..12_000).rev() {
            sink.push_analysis_file(FastAnalysisFile {
                path: format!("/fixture/parent-{}/file-{index:05}", index / 64).into(),
                allocated_bytes: index + 1,
                logical_bytes: index + 1,
                modified_at_ms: None,
            });
        }
        assert_eq!(sink.analysis_files.files.len(), 8192);
        let alias = PathBuf::from("/fixture/parent-187/file-11999");
        sink.push_large_file(
            alias.clone(),
            IndexedFile {
                bytes: 0,
                logical_bytes: 12000,
                modified_at_ms: None,
            },
        )
        .unwrap();
        let snapshot = sink.finish_analysis().unwrap();
        assert_eq!(snapshot.files.len(), 8192);
        assert_eq!(snapshot.files[&alias].bytes, 0);
        assert!(!snapshot
            .files
            .contains_key(std::path::Path::new("/fixture/parent-0/file-00000")));
    }
}
