#![cfg(target_os = "macos")]

use std::{fs, os::unix::fs::PermissionsExt, sync::Mutex};
use strawberrydisk_platform::Platform;

struct FailureLogger(Mutex<Vec<String>>);

impl log::Log for FailureLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record<'_>) {
        let message = record.args().to_string();
        if record.level() == log::Level::Warn && message.starts_with("filesystem_read_failed ") {
            self.0.lock().unwrap().push(message);
        }
    }

    fn flush(&self) {}
}

static LOGGER: FailureLogger = FailureLogger(Mutex::new(Vec::new()));

#[test]
fn unreadable_siblings_share_one_warning_budget_per_aggregate() {
    log::set_logger(&LOGGER).expect("isolated test logger must install");
    log::set_max_level(log::LevelFilter::Warn);
    let fixture = tempfile::tempdir().expect("fixture must exist");
    let restricted = (0..75)
        .map(|index| {
            let path = fixture.path().join(format!("restricted-{index}"));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("hidden.bin"), [1_u8; 32]).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            path
        })
        .collect::<Vec<_>>();
    // A new traversal gets its own sample budget; neither workers nor sibling directories do.
    let mut scans = Vec::new();
    for _ in 0..2 {
        LOGGER.0.lock().unwrap().clear();
        let aggregate = strawberrydisk_platform::current_platform().fast_directory_tree_aggregate(
            fixture.path(),
            &|| false,
            &|_, _, _| {},
        );
        scans.push((aggregate, LOGGER.0.lock().unwrap().clone()));
    }
    for path in restricted {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for (aggregate, warnings) in scans {
        let aggregate = aggregate
            .unwrap()
            .expect("macOS aggregate must be supported");
        assert_eq!(aggregate.read_failures.count, 75);
        assert_eq!(aggregate.read_failures.permission_denied_count, 75);
        assert_eq!(aggregate.read_failures.details.len(), 50);
        assert!(aggregate
            .read_failures
            .details
            .iter()
            .all(|detail| { detail.path.contains("restricted-") && detail.os_error == Some(13) }));
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().all(
            |message| message.contains("os_error=Some(13)") && message.contains("restricted-")
        ));
    }
}
