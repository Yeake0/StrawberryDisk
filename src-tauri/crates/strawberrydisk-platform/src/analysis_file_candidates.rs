use std::{cmp::Reverse, collections::BinaryHeap};

use crate::FastAnalysisFile;

/// Caps supplemental candidates while a directory is enumerated. Native metadata
/// is reused and only the largest files below the large-file floor are retained.
#[derive(Debug, Default)]
pub(crate) struct AnalysisFileCandidates {
    files: BinaryHeap<Reverse<FastAnalysisFile>>,
}

impl AnalysisFileCandidates {
    pub(crate) fn would_retain(&self, bytes: u64, path: &std::path::Path) -> bool {
        bytes > 0
            && (self.files.len() < 64
                || self.files.peek().is_some_and(|smallest| {
                    bytes > smallest.0.allocated_bytes
                        || (bytes == smallest.0.allocated_bytes && path < smallest.0.path.as_path())
                }))
    }

    pub(crate) fn push(&mut self, file: FastAnalysisFile) {
        const LIMIT: usize = 64;
        if file.allocated_bytes == 0 {
            return;
        }
        if self.files.len() < LIMIT {
            self.files.push(Reverse(file));
        } else if self.files.peek().is_some_and(|smallest| file > smallest.0) {
            *self
                .files
                .peek_mut()
                .expect("a full candidate heap is nonempty") = Reverse(file);
        }
    }

    pub(crate) fn into_files(self) -> impl Iterator<Item = FastAnalysisFile> {
        self.files.into_iter().map(|file| file.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_largest_files_with_stable_path_ties_and_ignores_zero_allocation() {
        let mut candidates = AnalysisFileCandidates::default();
        for index in (0..1024).rev() {
            candidates.push(FastAnalysisFile {
                path: format!("file-{index:04}").into(),
                allocated_bytes: index / 2,
                logical_bytes: index,
                modified_at_ms: None,
            });
        }
        let mut files = candidates.into_files().collect::<Vec<_>>();
        files.sort_by(|left, right| right.cmp(left));
        assert_eq!(files.len(), 64);
        assert_eq!(files[0].path.to_str(), Some("file-1022"));
        assert_eq!(files[63].path.to_str(), Some("file-0961"));
    }
}
