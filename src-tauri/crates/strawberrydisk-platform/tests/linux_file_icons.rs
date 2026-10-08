#![cfg(all(target_os = "linux", feature = "linux-desktop-icons"))]

use gio::prelude::AppInfoExt;
use std::{sync::mpsc, time::Instant};
use strawberrydisk_platform::{
    NativeFileIconItemKind, NativeFileIconMode, NativeFileIconRequest, NativeFileIconService,
};

#[test]
#[ignore = "requires a Linux desktop session for executable-alias regression validation"]
fn shared_runtime_alias_retains_fallback() {
    if std::env::var_os("STRAWBERRYDISK_ICON_ALIAS_CHILD").is_none() {
        // A separate process keeps GIO's desktop registry and GTK initialization
        // isolated from the real desktop workload and from other tests.
        let fixture = tempfile::tempdir().unwrap();
        let applications = fixture.path().join("applications");
        std::fs::create_dir(&applications).unwrap();
        let alias = fixture.path().join("application-launcher");
        std::os::unix::fs::symlink("/usr/bin/bash", &alias).unwrap();
        std::fs::write(
            applications.join("fixture.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Alias fixture\nExec={} --noprofile\nIcon=utilities-terminal\n",
                alias.display()
            ),
        )
        .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "shared_runtime_alias_retains_fallback",
                "--ignored",
                "--nocapture",
            ])
            .env("STRAWBERRYDISK_ICON_ALIAS_CHILD", "1")
            .env("XDG_DATA_HOME", fixture.path())
            .status()
            .unwrap();
        assert!(
            status.success(),
            "isolated executable-alias regression must pass"
        );
        return;
    }
    gtk::init().expect("GTK must connect to the test desktop");
    assert!(
        gio::AppInfo::all()
            .iter()
            .any(|app| app.id().as_deref() == Some("fixture.desktop")),
        "fixture must be registered before testing its executable identity"
    );
    let worker = std::thread::spawn(|| {
        NativeFileIconService::load(
            vec![NativeFileIconRequest {
                path: "/usr/bin/bash".into(),
                kind: NativeFileIconItemKind::File,
                mode: NativeFileIconMode::Path,
            }],
            None,
        )
    });
    let context = glib::MainContext::default();
    let started = Instant::now();
    while !worker.is_finished() {
        while context.pending() {
            context.iteration(false);
        }
        assert!(started.elapsed().as_secs() < 30, "alias lookup must finish");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let result = worker.join().unwrap();
    assert!(
        result.assignments.is_empty(),
        "a launcher alias must not label every shell process as its application"
    );
}

/// Run against the real desktop with DISPLAY set; reports aggregate timing only.
#[test]
#[ignore = "requires a Linux desktop session for native icon and performance validation"]
fn native_icon_workload() {
    gtk::init().expect("GTK must connect to the test desktop");
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let cache = tempfile::tempdir().expect("isolated icon cache must be writable");
        let paths = [
            "/usr/bin/nautilus",
            "/usr/bin/gnome-shell",
            "/usr/bin/gnome-terminal",
            "/usr/lib/xorg/Xorg",
            "/usr/sbin/xrdp",
            "/usr/bin/bash",
        ];
        let requests = paths
            .into_iter()
            .map(|path| NativeFileIconRequest {
                path: path.into(),
                kind: NativeFileIconItemKind::File,
                mode: NativeFileIconMode::Path,
            })
            .collect::<Vec<_>>();
        let mut timings = Vec::new();
        let mut cold_assets = 0;
        let mut repeated_assets = 0;
        let mut repeated_lookups = 0;
        for run in 0..101 {
            let started = Instant::now();
            let result = NativeFileIconService::load(requests.clone(), Some(cache.path().into()));
            let elapsed_us = started.elapsed().as_micros();
            if run == 0 {
                cold_assets = result.assets.len();
                println!(
                    "linux_icon_workload cold_us={elapsed_us} assets={cold_assets} requests={}",
                    requests.len()
                );
                if std::env::var_os("STRAWBERRYDISK_EXPECT_NATIVE_ICONS").is_some() {
                    // Ubuntu 24 registers Terminal and Terminal Preferences against
                    // the same executable with different artwork, so it safely falls
                    // back. Nautilus is unambiguous on both supported test desktops.
                    assert!(
                        result
                            .assignments
                            .iter()
                            .any(|item| item.path == "/usr/bin/nautilus"),
                        "registered Nautilus application must expose its icon"
                    );
                    assert!(
                        !result
                            .assignments
                            .iter()
                            .any(|item| item.path == "/usr/lib/xorg/Xorg"),
                        "a daemon without a desktop icon must retain its fallback"
                    );
                }
            } else {
                timings.push(elapsed_us);
                repeated_assets += result.assets.len();
                repeated_lookups += result.system_lookups;
            }
        }
        timings.sort_unstable();
        println!("linux_icon_workload repeated_runs=100 p50_us={} p95_us={} max_us={} assets={} system_lookups={repeated_lookups}", timings[49], timings[94], timings[99], repeated_assets);
        if std::env::var_os("STRAWBERRYDISK_EXPECT_NATIVE_ICONS").is_some() {
            let path = std::env::current_exe().expect("test executable must be known");
            NativeFileIconService::register_process_icon(
                path.clone(),
                include_bytes!("../../../icons/32x32.png"),
            );
            let own = NativeFileIconService::load(
                vec![NativeFileIconRequest {
                    path: path.to_string_lossy().into_owned(),
                    kind: NativeFileIconItemKind::File,
                    mode: NativeFileIconMode::Path,
                }],
                Some(cache.path().into()),
            );
            assert_eq!(
                own.assets.len(),
                1,
                "verified current executable must use bundled artwork"
            );
        }
        sender
            .send(cold_assets)
            .expect("test receiver must remain alive");
    });
    let context = glib::MainContext::default();
    let started = Instant::now();
    loop {
        while context.pending() {
            context.iteration(false);
        }
        if receiver.try_recv().is_ok() {
            break;
        }
        if worker.is_finished() {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "native icon workload must finish without a main-loop deadlock"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    worker.join().expect("native icon worker must succeed");
}
