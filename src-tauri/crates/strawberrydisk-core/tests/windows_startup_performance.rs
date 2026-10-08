#![cfg(target_os = "windows")]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use strawberrydisk_core::{
    configure_application_paths, ApplicationPaths, StartupCatalog, StartupChangeSelection,
    StartupDesiredState, StartupService,
};

const FIXTURE_PREFIX: &str = "StrawberryDiskStartupPerformanceFixture";
const FIXTURE_COUNT: usize = 6;
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVAL_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
static TEST_LOCK: Mutex<()> = Mutex::new(());
static STORAGE_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[test]
#[ignore = "creates isolated HKCU startup entries and measures the real Windows workflow"]
fn sequential_change_baseline() {
    let _test_lock = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _storage = TestStorage::create();
    let _fixture = WindowsStartupFixture::create();
    let mut catalog = StartupService::scan().expect("the startup catalog must be available");
    assert_eq!(fixture_item_ids(&catalog).len(), FIXTURE_COUNT);

    let disabled_started = Instant::now();
    for item_id in fixture_item_ids(&catalog) {
        catalog = change_items(catalog, vec![item_id], StartupDesiredState::Disabled, 1);
    }
    let disabled_elapsed = disabled_started.elapsed();

    let enabled_started = Instant::now();
    for item_id in fixture_item_ids(&catalog) {
        catalog = change_items(catalog, vec![item_id], StartupDesiredState::Enabled, 1);
    }

    println!(
        "startup_performance_baseline item_count={FIXTURE_COUNT} disable_elapsed_ms={} enable_elapsed_ms={}",
        disabled_elapsed.as_millis(),
        enabled_started.elapsed().as_millis()
    );
}

#[test]
#[ignore = "creates isolated HKCU startup entries and measures the batched Windows workflow"]
fn batched_change_performance() {
    let _test_lock = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _storage = TestStorage::create();
    let _fixture = WindowsStartupFixture::create();
    let catalog = StartupService::scan().expect("the startup catalog must be available");
    let fixture_ids = fixture_item_ids(&catalog);
    assert_eq!(fixture_ids.len(), FIXTURE_COUNT);

    let disabled_started = Instant::now();
    let catalog = change_items(
        catalog,
        fixture_ids,
        StartupDesiredState::Disabled,
        FIXTURE_COUNT,
    );
    let disabled_elapsed = disabled_started.elapsed();

    let enabled_started = Instant::now();
    let fixture_ids = fixture_item_ids(&catalog);
    let _catalog = change_items(
        catalog,
        fixture_ids,
        StartupDesiredState::Enabled,
        FIXTURE_COUNT,
    );

    println!(
        "startup_performance_optimized item_count={FIXTURE_COUNT} disable_elapsed_ms={} enable_elapsed_ms={}",
        disabled_elapsed.as_millis(),
        enabled_started.elapsed().as_millis()
    );
}

fn change_items(
    catalog: StartupCatalog,
    item_ids: Vec<String>,
    desired_state: StartupDesiredState,
    expected_count: usize,
) -> StartupCatalog {
    let plan = StartupService::prepare_change(StartupChangeSelection {
        scan_id: catalog.scan_id,
        item_ids,
        desired_state,
    })
    .expect("the isolated startup change must prepare");
    assert_eq!(plan.items.len(), expected_count);
    let result = StartupService::execute_change(plan.plan_id, None)
        .expect("the isolated startup change must execute");
    assert_eq!(result.changed_count, expected_count as u64);
    assert_eq!(result.failed_count, 0);
    result.catalog.expect("the catalog readback must succeed")
}

fn fixture_item_ids(catalog: &StartupCatalog) -> Vec<String> {
    let mut ids = catalog
        .artifacts
        .iter()
        .filter(|artifact| artifact.display_name.starts_with(FIXTURE_PREFIX))
        .map(|artifact| artifact.item_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

struct WindowsStartupFixture;

impl WindowsStartupFixture {
    fn create() -> Self {
        let fixture = Self;
        for index in 0..FIXTURE_COUNT {
            let name = format!("{FIXTURE_PREFIX}{index}");
            let output = Command::new("reg.exe")
                .args([
                    "add",
                    RUN_KEY,
                    "/v",
                    &name,
                    "/t",
                    "REG_SZ",
                    "/d",
                    r#"\"C:\Windows\System32\notepad.exe\" --strawberrydisk-performance-fixture"#,
                    "/f",
                ])
                .output()
                .expect("the Windows registry fixture command must start");
            assert!(
                output.status.success(),
                "the startup fixture must be created"
            );
            delete_registry_value(APPROVAL_KEY, &name);
        }
        fixture
    }
}

impl Drop for WindowsStartupFixture {
    fn drop(&mut self) {
        for index in 0..FIXTURE_COUNT {
            let name = format!("{FIXTURE_PREFIX}{index}");
            delete_registry_value(RUN_KEY, &name);
            delete_registry_value(APPROVAL_KEY, &name);
        }
    }
}

fn delete_registry_value(key: &str, value_name: &str) {
    let _ = Command::new("reg.exe")
        .args(["delete", key, "/v", value_name, "/f"])
        .output();
}

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn create() -> Self {
        let root = STORAGE_ROOT
            .get_or_init(|| {
                std::env::temp_dir().join(format!(
                    "strawberrydisk-startup-performance-storage-{}",
                    std::process::id()
                ))
            })
            .clone();
        let paths =
            ApplicationPaths::new(root.join("data"), root.join("cache"), root.join("runtime"))
                .expect("the performance storage paths must be valid");
        for path in [
            paths.data_directory(),
            paths.cache_directory(),
            paths.runtime_directory(),
        ] {
            fs::create_dir_all(path).expect("the performance storage directory must be available");
        }
        configure_application_paths(paths)
            .expect("the performance storage paths must be configured");
        Self { root }
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
