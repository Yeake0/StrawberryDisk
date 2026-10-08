    #[test]
    #[ignore = "modifies process environment; run this test alone"]
    fn zoom_diagnostic_rule_preserves_recent_logs_and_recordings() {
        let _operation_lock = crate::shared::operation::test_operation_lock();
        let sandbox = std::env::current_dir()
            .expect("test process should have a working directory")
            .join("target")
            .join(format!(
                "strawberrydisk-zoom-cleanup-test-{}-{}",
                std::process::id(),
                now_ms()
            ));
        let _sandbox_cleanup = DirectoryCleanup(sandbox.clone());
        let profile = sandbox.join("UserProfile");
        let roaming = sandbox.join("RoamingAppData");
        let old_log = roaming.join("Zoom/logs/old-diagnostic.log");
        let recent_log = roaming.join("Zoom/logs/recent-diagnostic.log");
        let recording = profile.join("Documents/Zoom/meeting-recording.mp4");
        for fixture in [&old_log, &recent_log, &recording] {
            fs::create_dir_all(fixture.parent().expect("fixture must have a parent"))
                .expect("should create the isolated Zoom directory");
            fs::write(fixture, b"StrawberryDisk Zoom cleanup fixture")
                .expect("should write the isolated Zoom fixture");
        }
        let old_time = SystemTime::now()
            .checked_sub(Duration::from_secs(15 * 86_400))
            .expect("test time should move back by fifteen days");
        fs::File::options()
            .write(true)
            .open(&old_log)
            .expect("should open the old Zoom log fixture")
            .set_times(fs::FileTimes::new().set_modified(old_time))
            .expect("should set the old Zoom log modification time");

        let _restore = EnvironmentRestore(vec![
            ("USERPROFILE", std::env::var_os("USERPROFILE")),
            ("APPDATA", std::env::var_os("APPDATA")),
        ]);
        std::env::set_var("USERPROFILE", &profile);
        std::env::set_var("APPDATA", &roaming);

        let preview = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.zoom-diagnostic-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: true,
            project_roots: Vec::new(),
        })
        .expect("isolated Zoom diagnostic preview should succeed");
        assert_eq!(preview.failed_item_count, 0);
        assert!(old_log.exists());
        assert!(recent_log.exists());
        assert!(recording.exists());

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.zoom-diagnostic-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("isolated Zoom diagnostic cleanup should succeed");

        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert_eq!(result.affected_item_count, 1);
        assert!(
            !old_log.exists(),
            "Zoom diagnostic logs older than two weeks should be deleted"
        );
        assert!(
            recent_log.exists(),
            "recent Zoom diagnostic logs should remain available"
        );
        assert!(
            recording.exists(),
            "Zoom recordings must remain outside the cleanup boundary"
        );
    }

    #[test]
    #[ignore = "modifies process environment; run this test alone"]
    fn crash_dumps_and_windows_error_reports_are_actually_cleaned_in_isolated_roots() {
        const FIXTURE_CONTENT: &[u8] = b"StrawberryDisk safe cleanup fixture";

        let _operation_lock = crate::shared::operation::test_operation_lock();
        let sandbox = std::env::current_dir()
            .expect("test process should have a working directory")
            .join("target")
            .join(format!(
                "strawberrydisk-windows-cleanup-test-{}-{}",
                std::process::id(),
                now_ms()
            ));
        let _sandbox_cleanup = DirectoryCleanup(sandbox.clone());
        let local = sandbox.join("LocalAppData");
        let program_data = sandbox.join("ProgramData");
        let crash_dump = local.join("CrashDumps/fixture crash.dmp");
        let user_report =
            local.join("Microsoft/Windows/WER/ReportArchive/StrawberryDisk_User_Fixture/Report.wer");
        let recent_user_report = local
            .join("Microsoft/Windows/WER/ReportArchive/StrawberryDisk_Recent_User_Fixture/Report.wer");
        let system_report = program_data
            .join("Microsoft/Windows/WER/ReportQueue/StrawberryDisk_System_Fixture/Report.wer");
        let temporary_report = program_data.join("Microsoft/Windows/WER/Temp/fixture.tmp");
        for fixture in [
            &crash_dump,
            &user_report,
            &recent_user_report,
            &system_report,
            &temporary_report,
        ] {
            fs::create_dir_all(fixture.parent().expect("fixture must have a parent"))
                .expect("should create isolated diagnostic directory");
            fs::write(fixture, FIXTURE_CONTENT).expect("should write isolated diagnostic fixture");
        }
        let old_time = SystemTime::now()
            .checked_sub(Duration::from_secs(15 * 86_400))
            .expect("test time should move back by fifteen days");
        for fixture in [&user_report, &system_report, &temporary_report] {
            fs::File::options()
                .write(true)
                .open(fixture)
                .expect("should open the old WER fixture")
                .set_times(fs::FileTimes::new().set_modified(old_time))
                .expect("should set the old WER fixture modification time");
        }

        let _restore = EnvironmentRestore(vec![
            ("LOCALAPPDATA", std::env::var_os("LOCALAPPDATA")),
            ("PROGRAMDATA", std::env::var_os("PROGRAMDATA")),
        ]);
        std::env::set_var("LOCALAPPDATA", &local);
        std::env::set_var("PROGRAMDATA", &program_data);

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "system.crash-dumps".to_string(),
                "system.error-reports".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("isolated CrashDumps and WER cleanup should succeed");

        assert_eq!(result.failed_item_count, 0);
        assert_eq!(result.affected_item_count, 2);
        assert_eq!(
            result.released_bytes,
            2 * u64::try_from(FIXTURE_CONTENT.len()).expect("fixture length should fit in u64")
        );
        assert!(!crash_dump.exists());
        assert!(!user_report.exists());
        assert!(recent_user_report.exists());
        assert!(system_report.exists());
        assert!(temporary_report.exists());
    }

    #[test]
    #[ignore = "modifies process environment; run this test alone"]
    fn developer_cache_rules_preserve_windows_configuration_and_credentials() {
        let _operation_lock = crate::shared::operation::test_operation_lock();
        let sandbox = std::env::current_dir()
            .expect("test process should have a working directory")
            .join("target")
            .join(format!(
                "strawberrydisk-developer-cache-windows-cleanup-test-{}-{}",
                std::process::id(),
                now_ms()
            ));
        let _sandbox_cleanup = DirectoryCleanup(sandbox.clone());
        let profile = sandbox.join("UserProfile");
        let local = sandbox.join("LocalAppData");
        let roaming = sandbox.join("RoamingAppData");
        let cache_files = [
            roaming.join("ccache/a/result"),
            local.join("Mozilla/sccache/cache/0/compile-result"),
            profile.join(".hex/cache/registry.ets"),
            local.join("copilot/marketplace/index.json"),
            local.join("pypoetry/Cache/artifacts/aa/package.whl"),
            local.join("pypoetry/Cache/cache/repositories/PyPI/index.json"),
        ];
        let protected_files = [
            roaming.join("ccache/ccache.conf"),
            roaming.join("Mozilla/sccache/config/config"),
            profile.join(".hex/hex.config"),
            profile.join(".copilot/settings.json"),
            local.join("pypoetry/Cache/virtualenvs/project-py3.13/pyvenv.cfg"),
        ];
        for fixture in cache_files.iter().chain(&protected_files) {
            fs::create_dir_all(fixture.parent().expect("fixture must have a parent"))
                .expect("should create the isolated developer tool directory");
            fs::write(fixture, b"StrawberryDisk developer cache fixture")
                .expect("should write the isolated developer tool fixture");
        }

        let _restore = EnvironmentRestore(vec![
            ("USERPROFILE", std::env::var_os("USERPROFILE")),
            ("LOCALAPPDATA", std::env::var_os("LOCALAPPDATA")),
            ("APPDATA", std::env::var_os("APPDATA")),
        ]);
        std::env::set_var("USERPROFILE", &profile);
        std::env::set_var("LOCALAPPDATA", &local);
        std::env::set_var("APPDATA", &roaming);
        let rule_ids = [
            "dev.ccache-cache",
            "dev.sccache-cache",
            "dev.hex-cache",
            "dev.copilot-cli-cache",
            "dev.python-tooling-cache",
        ]
        .map(str::to_string)
        .to_vec();

        let preview = CleanupService::execute(CleanupRequest {
            rule_ids: rule_ids.clone(),
            source_selections: Vec::new(),
            dry_run: true,
            project_roots: Vec::new(),
        })
        .expect("isolated developer cache preview should succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(cache_files.iter().all(|fixture| fixture.exists()));
        assert!(protected_files.iter().all(|fixture| fixture.exists()));

        let result = CleanupService::execute(CleanupRequest {
            rule_ids,
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("isolated developer cache cleanup should succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert_eq!(result.affected_item_count, 6);
        assert!(cache_files.iter().all(|fixture| !fixture.exists()));
        assert!(protected_files.iter().all(|fixture| fixture.exists()));
    }

    #[test]
    #[ignore = "clears real Poetry caches in the Windows VM"]
    fn real_windows_poetry_cache_preserves_virtual_environments() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_WINDOWS_POETRY_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_WINDOWS_POETRY_CACHE=1 only in the isolated Windows VM"
        );
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let poetry_cache = local_app_data.join("pypoetry/Cache");
        let virtualenvs = poetry_cache.join("virtualenvs");
        assert!(virtualenvs.is_dir(), "Poetry must create a real virtualenv");
        let virtualenv = fs::read_dir(&virtualenvs)
            .expect("the Poetry virtualenv directory must remain readable")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.join("pyvenv.cfg").is_file()
                    && path.join("Lib/site-packages/idna/__init__.py").is_file()
            })
            .expect("Poetry must create a virtualenv containing the test dependency");
        let protected_files = [
            virtualenv.join("pyvenv.cfg"),
            virtualenv.join("Lib/site-packages/idna/__init__.py"),
        ];
        let protected_before = protected_files
            .iter()
            .map(|path| fs::read(path).expect("the Poetry virtualenv file must be readable"))
            .collect::<Vec<_>>();

        let target_roots = [poetry_cache.join("artifacts"), poetry_cache.join("cache")];
        assert!(
            target_roots.iter().all(|root| root.is_dir()),
            "Poetry must populate both rebuildable cache roots"
        );
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the Poetry cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["dev.python-tooling-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Poetry cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Poetry cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let protected_after = protected_files
            .iter()
            .map(|path| fs::read(path).expect("the Poetry virtualenv file must remain readable"))
            .collect::<Vec<_>>();
        assert_eq!(protected_after, protected_before);
        println!(
            "real_windows_poetry_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={}",
            preview.expected_bytes, result.released_bytes, result.affected_item_count
        );
    }

    #[test]
    #[ignore = "modifies process environment; run this test alone"]
    fn ai_cache_rules_clean_only_rebuildable_data_and_preserve_models() {
        let _operation_lock = crate::shared::operation::test_operation_lock();
        let sandbox = std::env::current_dir()
            .expect("test process should have a working directory")
            .join("target")
            .join(format!(
                "strawberrydisk-ai-cache-windows-cleanup-test-{}-{}",
                std::process::id(),
                now_ms()
            ));
        let _sandbox_cleanup = DirectoryCleanup(sandbox.clone());
        let profile = sandbox.join("UserProfile");
        let local = sandbox.join("LocalAppData");
        let roaming = sandbox.join("RoamingAppData");
        let downloads = profile.join("Downloads");
        let huggingface_hub = profile.join(".cache/huggingface/hub/models--fixture/blobs");
        let xet_environment = profile.join(".cache/huggingface/xet/environment");
        let xet_chunk_cache = xet_environment.join("chunk_cache");
        let xet_shard_cache = xet_environment.join("shard_cache");
        let xet_staging = xet_environment.join("staging");
        let project = profile.join("project");
        let adobe_local = local.join("Adobe/Common/Media Cache Files");
        let adobe_roaming = roaming.join("Adobe/Common/Media Cache Files");
        for directory in [
            &downloads,
            &huggingface_hub,
            &xet_chunk_cache,
            &xet_shard_cache,
            &xet_staging,
            &project,
            &adobe_local,
            &adobe_roaming,
        ] {
            fs::create_dir_all(directory).expect("should create isolated rule directory");
        }

        let stale_partial = downloads.join("old-model.crdownload");
        let recent_partial = downloads.join("active-model.crdownload");
        let completed_download = downloads.join("archive.zip");
        let downloaded_model = huggingface_hub.join("downloaded-model.bin");
        let xet_chunk = xet_chunk_cache.join("chunk.bin");
        let xet_shard = xet_shard_cache.join("shard.mdb");
        let resumable_upload = xet_staging.join("upload.mdb");
        let project_model = project.join("model.bin");
        let local_media_cache = adobe_local.join("local-cache.bin");
        let roaming_media_cache = adobe_roaming.join("roaming-cache.bin");
        for fixture in [
            &stale_partial,
            &recent_partial,
            &completed_download,
            &downloaded_model,
            &xet_chunk,
            &xet_shard,
            &resumable_upload,
            &project_model,
            &local_media_cache,
            &roaming_media_cache,
        ] {
            fs::write(fixture, b"StrawberryDisk round 04 fixture")
                .expect("should write isolated cleanup fixture");
        }
        let stale_time = SystemTime::now()
            .checked_sub(Duration::from_secs(8 * 86_400))
            .expect("test time should move back by eight days");
        fs::File::options()
            .write(true)
            .open(&stale_partial)
            .expect("should open stale download fixture")
            .set_times(fs::FileTimes::new().set_modified(stale_time))
            .expect("should set stale download modification time");

        let _restore = EnvironmentRestore(vec![
            ("USERPROFILE", std::env::var_os("USERPROFILE")),
            ("LOCALAPPDATA", std::env::var_os("LOCALAPPDATA")),
            ("APPDATA", std::env::var_os("APPDATA")),
        ]);
        std::env::set_var("USERPROFILE", &profile);
        std::env::set_var("LOCALAPPDATA", &local);
        std::env::set_var("APPDATA", &roaming);

        assert!(
            validate_rule_root(&downloads, &MatcherSpec::All).is_err(),
            "Downloads must never be authorized for full-root cleanup"
        );

        let retired_rule = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "system.stale-partial-downloads".to_string(),
                "ai.model-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        });
        assert!(retired_rule.is_err());
        assert!(stale_partial.exists());
        assert!(
            CleanupService::execute(CleanupRequest {
                rule_ids: vec!["ai.gemini-temp-files".to_string()],
                source_selections: Vec::new(),
                dry_run: false,
                project_roots: Vec::new(),
            })
            .is_err(),
            "retired Gemini session cleanup rule must remain unavailable"
        );

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "system.stale-partial-downloads".to_string(),
                "app.adobe-media-cache".to_string(),
                "ai.huggingface-xet-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("isolated AI cache cleanup should succeed");

        assert_eq!(
            result.failed_item_count, 0,
            "isolated cleanup should not fail: {:?}",
            result.actions
        );
        assert_eq!(result.affected_item_count, 5);
        assert!(!stale_partial.exists());
        assert!(!xet_chunk.exists());
        assert!(!xet_shard.exists());
        assert!(!local_media_cache.exists());
        assert!(!roaming_media_cache.exists());
        assert!(downloaded_model.exists());
        assert!(resumable_upload.exists());
        assert!(recent_partial.exists());
        assert!(completed_download.exists());
        assert!(project_model.exists());
    }
