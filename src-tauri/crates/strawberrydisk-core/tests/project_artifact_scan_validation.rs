use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

use strawberrydisk_core::{
    configure_application_paths, ApplicationPaths, CleanupScanService, ScanItemStatus,
};
use strawberrydisk_platform::{current_platform, Platform};

const EXPECTED_ARTIFACTS_ENV: &str = "STRAWBERRYDISK_TEST_PROJECT_ARTIFACT_PATHS";

#[test]
#[ignore = "scans real standard project roots named by STRAWBERRYDISK_TEST_PROJECT_ARTIFACT_PATHS"]
fn standard_scan_reports_each_real_project_artifact_once() {
    let expected_paths = env::var_os(EXPECTED_ARTIFACTS_ENV)
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .filter(|paths| !paths.is_empty())
        .expect("STRAWBERRYDISK_TEST_PROJECT_ARTIFACT_PATHS must name real artifact directories");
    for path in &expected_paths {
        assert!(
            path.is_absolute(),
            "expected artifact paths must be absolute"
        );
        assert!(path.is_dir(), "expected artifact paths must exist");
    }
    let _storage = TestStorage::create();

    let result = CleanupScanService::scan_with_progress(|_| {})
        .expect("the production standard cleanup scan must succeed");
    let project_rules = result
        .rules
        .iter()
        .filter(|rule| rule.rule_id.starts_with("project."))
        .collect::<Vec<_>>();
    let mut identities = HashSet::new();
    let mut project_source_count = 0_usize;
    let mut project_reclaimable_bytes = 0_u64;
    for rule in project_rules {
        let mut complete_bytes = 0_u64;
        for source in &rule.sources {
            project_source_count += 1;
            assert!(
                identities.insert(current_platform().path_identity_key(Path::new(&source.path))),
                "a physical project artifact path must appear only once in the scan result"
            );
            if source.block_reason.is_none() {
                complete_bytes = complete_bytes.saturating_add(source.bytes);
            }
        }
        if rule.status == ScanItemStatus::Found {
            assert_eq!(
                rule.bytes, complete_bytes,
                "a project rule must equal the sum of its selectable sources"
            );
        }
        project_reclaimable_bytes = project_reclaimable_bytes.saturating_add(rule.bytes);
    }
    for expected_path in expected_paths {
        let expected_identity = current_platform().path_identity_key(&expected_path);
        assert!(
            identities.contains(&expected_identity),
            "every requested real artifact must appear exactly once"
        );
    }

    println!(
        "project_artifact_standard_scan_validation source_count={project_source_count} reclaimable_bytes={project_reclaimable_bytes}"
    );
}

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn create() -> Self {
        let root = env::temp_dir().join(format!(
            "strawberrydisk-project-artifact-validation-storage-{}",
            std::process::id()
        ));
        let paths =
            ApplicationPaths::new(root.join("data"), root.join("cache"), root.join("runtime"))
                .expect("create isolated application paths");
        for path in [
            paths.data_directory(),
            paths.cache_directory(),
            paths.runtime_directory(),
        ] {
            fs::create_dir_all(path).expect("create isolated application storage");
        }
        configure_application_paths(paths).expect("configure isolated application paths");
        Self { root }
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
