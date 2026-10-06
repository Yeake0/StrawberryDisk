#![cfg(target_os = "macos")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Instant;

use strawberrydisk_core::{
    configure_application_paths, ApplicationPaths, StartupCatalog, StartupChangeSelection,
    StartupConfiguredState, StartupDesiredState, StartupService,
};

static STORAGE_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[test]
#[ignore = "temporarily changes an explicitly authorized user LaunchAgent and restores its state"]
fn actual_user_launch_agent_round_trips_through_core() {
    let fixture_path = std::env::var_os("STRAWBERRYDISK_TEST_MACOS_STARTUP_PLIST")
        .map(PathBuf::from)
        .expect("STRAWBERRYDISK_TEST_MACOS_STARTUP_PLIST must name an authorized LaunchAgent");
    assert!(fixture_path.is_absolute());
    let expected_parent = std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("HOME must be available")
        .join("Library/LaunchAgents");
    assert_eq!(
        fixture_path.parent(),
        Some(expected_parent.as_path()),
        "the real startup item must be a current-user LaunchAgent"
    );

    let _storage = TestStorage::create();
    let catalog = StartupService::scan().expect("the startup catalog must be available");
    let artifact = fixture_artifact(&catalog, &fixture_path);
    let initial_state = artifact.configured_state;
    let item_id = artifact.item_id.clone();
    assert!(matches!(
        initial_state,
        StartupConfiguredState::Enabled | StartupConfiguredState::Disabled
    ));
    let _restore = LaunchAgentStateRestore::new(artifact_label(&fixture_path), initial_state);

    let disabled_started = Instant::now();
    let catalog = change_item(catalog, item_id, StartupDesiredState::Disabled);
    let disabled_elapsed = disabled_started.elapsed();
    assert_eq!(
        fixture_artifact(&catalog, &fixture_path).configured_state,
        StartupConfiguredState::Disabled
    );

    let enabled_started = Instant::now();
    let item_id = fixture_artifact(&catalog, &fixture_path).item_id.clone();
    let catalog = change_item(catalog, item_id, StartupDesiredState::Enabled);
    assert_eq!(
        fixture_artifact(&catalog, &fixture_path).configured_state,
        StartupConfiguredState::Enabled
    );

    println!(
        "macos_startup_validation disable_elapsed_ms={} enable_elapsed_ms={}",
        disabled_elapsed.as_millis(),
        enabled_started.elapsed().as_millis()
    );
}

fn change_item(
    catalog: StartupCatalog,
    item_id: String,
    desired_state: StartupDesiredState,
) -> StartupCatalog {
    let plan = StartupService::prepare_change(StartupChangeSelection {
        scan_id: catalog.scan_id,
        item_ids: vec![item_id],
        desired_state,
    })
    .expect("the isolated startup change must prepare");
    assert_eq!(plan.items.len(), 1);
    let result = StartupService::execute_change(plan.plan_id, None)
        .expect("the isolated startup change must execute");
    assert_eq!(
        result.changed_count, 1,
        "change outcomes: {:?}",
        result.items
    );
    assert_eq!(result.failed_count, 0);
    result.catalog.expect("the catalog readback must succeed")
}

fn fixture_artifact<'a>(
    catalog: &'a StartupCatalog,
    fixture_path: &Path,
) -> &'a strawberrydisk_core::StartupArtifact {
    catalog
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.source_id == "macos.launchd.user_agents"
                && artifact.configuration_path.as_deref() == fixture_path.to_str()
        })
        .expect("the authorized LaunchAgent must be present in the catalog")
}

fn artifact_label(fixture_path: &Path) -> String {
    plist::Value::from_file(fixture_path)
        .expect("the authorized LaunchAgent property list must be readable")
        .as_dictionary()
        .and_then(|dictionary| dictionary.get("Label"))
        .and_then(plist::Value::as_string)
        .filter(|label| !label.trim().is_empty())
        .expect("the authorized LaunchAgent must define a non-empty Label")
        .to_owned()
}

struct LaunchAgentStateRestore {
    label: String,
    state: StartupConfiguredState,
}

impl LaunchAgentStateRestore {
    fn new(label: String, state: StartupConfiguredState) -> Self {
        Self { label, state }
    }
}

impl Drop for LaunchAgentStateRestore {
    fn drop(&mut self) {
        let action = match self.state {
            StartupConfiguredState::Disabled => "disable",
            _ => "enable",
        };
        let target = format!("gui/{}/{}", unsafe { libc::geteuid() }, self.label);
        let _ = Command::new("/bin/launchctl")
            .args([action, &target])
            .output();
    }
}

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn create() -> Self {
        let root = STORAGE_ROOT
            .get_or_init(|| {
                std::env::temp_dir().join(format!(
                    "strawberrydisk-macos-startup-validation-storage-{}",
                    std::process::id()
                ))
            })
            .clone();
        let paths =
            ApplicationPaths::new(root.join("data"), root.join("cache"), root.join("runtime"))
                .expect("the validation storage paths must be valid");
        for path in [
            paths.data_directory(),
            paths.cache_directory(),
            paths.runtime_directory(),
        ] {
            fs::create_dir_all(path).expect("the validation storage directory must be available");
        }
        configure_application_paths(paths)
            .expect("the validation storage paths must be configured");
        Self { root }
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore = "removes only the explicitly authorized StrawberryDisk Startup Demo orphan login record"]
fn actual_orphan_login_record_is_removed_through_core() {
    struct StartupValidationLogger;
    impl log::Log for StartupValidationLogger {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            if record.args().to_string().starts_with("startup_") {
                println!("{}", record.args());
            }
        }
        fn flush(&self) {}
    }
    let _ = log::set_logger(&StartupValidationLogger);
    log::set_max_level(log::LevelFilter::Info);
    let path = std::env::var_os("STRAWBERRYDISK_TEST_ORPHAN_APP")
        .map(PathBuf::from)
        .expect("STRAWBERRYDISK_TEST_ORPHAN_APP must identify the disposable test application");
    assert!(path.is_absolute());
    assert_eq!(
        path.file_name().and_then(|value| value.to_str()),
        Some("StrawberryDisk Startup Demo.app")
    );
    assert!(!path.try_exists().expect("the target must be accessible"));
    let _storage = TestStorage::create();
    let catalog = StartupService::scan().expect("startup scan must succeed");
    let candidates = catalog
        .artifacts
        .iter()
        .filter(|item| {
            item.source_id == "macos.background_tasks"
                && item.target.path.as_deref().map(Path::new) == Some(path.as_path())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        candidates.len(),
        1,
        "the test record must be uniquely identified by its full path"
    );
    let item = candidates[0];
    assert!(item.removal_supported && item.removable_orphan);
    let item_id = item.item_id.clone();
    let retained_ids = catalog
        .artifacts
        .iter()
        .filter(|item| item.item_id != item_id)
        .map(|item| item.item_id.clone())
        .collect::<Vec<_>>();
    let catalog = change_item(catalog, item_id.clone(), StartupDesiredState::Removed);
    assert!(!catalog.artifacts.iter().any(|item| item.item_id == item_id
        || item.target.path.as_deref().map(Path::new) == Some(path.as_path())));
    for id in retained_ids {
        assert!(
            catalog.artifacts.iter().any(|item| item.item_id == id),
            "unrelated startup records must remain"
        );
    }
    assert!(
        !path.exists(),
        "record deletion must not recreate or touch the application target"
    );
    println!("orphan_login_record_removed verified=true unrelated_records_preserved=true target_recreated=false");
}
