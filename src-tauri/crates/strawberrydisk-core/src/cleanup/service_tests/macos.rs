mod macos_cleanup_tests {
    use std::{
        ffi::OsString,
        os::unix::fs::symlink,
        path::PathBuf,
        time::{Duration, SystemTime},
    };

    use super::*;
    use crate::cleanup::CleanupRequest;

    struct DirectoryCleanup(PathBuf);

    struct FileCleanup(Vec<PathBuf>);

    struct EnvironmentRestore(Vec<(&'static str, Option<OsString>)>);

    impl Drop for DirectoryCleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    impl Drop for FileCleanup {
        fn drop(&mut self) {
            for path in &self.0 {
                let _ = fs::remove_file(path);
            }
        }
    }

    impl Drop for EnvironmentRestore {
        fn drop(&mut self) {
            for (name, value) in self.0.drain(..) {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    include!("macos/foundation_and_developer.rs");
    include!("macos/application_caches.rs");
    include!("macos/collaboration_caches.rs");
    include!("macos/productivity_caches.rs");
    include!("macos/browser_caches.rs");
    include!("macos/tree_helpers.rs");
    include!("macos/reference_expansion.rs");
    include!("macos/notion_and_claude_code.rs");
}
