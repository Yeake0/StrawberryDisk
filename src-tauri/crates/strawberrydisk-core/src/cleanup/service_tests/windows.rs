mod windows_cleanup_tests {
    use std::{
        ffi::OsString,
        fs,
        path::PathBuf,
        time::{Duration, Instant, SystemTime},
    };

    use super::*;
    use crate::cleanup::CleanupRequest;

    struct EnvironmentRestore(Vec<(&'static str, Option<OsString>)>);

    struct DirectoryCleanup(PathBuf);

    impl Drop for DirectoryCleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
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

    include!("windows/system_and_productivity.rs");
    include!("windows/communication_and_media.rs");
    include!("windows/developer_and_desktop.rs");
    include!("windows/collaboration_caches.rs");
    include!("windows/browser_caches.rs");
    include!("windows/isolated_rule_regressions.rs");
    include!("windows/notion_and_claude_code.rs");
}
