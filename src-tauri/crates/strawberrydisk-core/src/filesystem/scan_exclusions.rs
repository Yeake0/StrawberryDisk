use serde::{Deserialize, Serialize};
use strawberrydisk_platform::ScanNameExclusion;

/// A scan's immutable exclusion input. Adapters choose module scopes before entering Core.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanExclusionOptions {
    pub paths: Vec<String>,
    pub names: Vec<ScanNameExclusion>,
}

impl From<Vec<String>> for ScanExclusionOptions {
    fn from(paths: Vec<String>) -> Self {
        Self {
            paths,
            names: Vec::new(),
        }
    }
}

impl ScanExclusionOptions {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.names.is_empty()
    }
    pub fn len(&self) -> usize {
        self.paths.len() + self.names.len()
    }
}
