    /// Exercises the dedicated Notion process boundary while the signed client
    /// is running. The production gate must stop before traversing the mixed
    /// profile that also contains databases, offline pages, and browser state.
    #[test]
    #[ignore = "requires the real Notion application to be running"]
    fn real_notion_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_NOTION_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_NOTION_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Notion.exe".to_string()]);
        assert!(!running.is_empty(), "Notion must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.notion-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Notion cleanup must return a structured result");
        assert_eq!(result.actions.len(), 1);
        let action = &result.actions[0];
        assert_eq!(action.status, crate::cleanup::CleanupActionStatus::Blocked);
        assert_eq!(
            action.reason_code,
            Some(crate::cleanup::CleanupActionReason::RunningProcesses)
        );
        assert_eq!(action.released_bytes, 0);
        assert_eq!(action.affected_item_count, 0);
        assert!(!action.running_processes.is_empty());
        println!(
            "real_windows_notion_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }
    /// Exercises the dedicated Notion rule against a real signed installation.
    /// Notion puts disposable renderer caches beside its local database and
    /// offline-capable browser state. Representative top-level state and every
    /// storage family present in each real renderer partition are therefore
    /// hashed before and after production cleanup.
    #[test]
    #[ignore = "clears real Notion rendering caches in an isolated Windows VM"]
    fn real_notion_partition_cache_preserves_database_and_offline_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_NOTION_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_NOTION_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let notion_root = roaming_app_data.join("Notion");
        let partitions_root = notion_root.join("Partitions");
        assert!(
            partitions_root.is_dir(),
            "Notion must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Notion.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Notion must be completely stopped before the real cleanup diagnostic"
        );

        let mut partition_roots = fs::read_dir(&partitions_root)
            .expect("the Notion partitions root must be readable")
            .map(|entry| entry.expect("the partition entry must be readable").path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        partition_roots.sort();
        assert!(
            !partition_roots.is_empty(),
            "the real Notion profile must expose at least one renderer partition"
        );

        let mut preserved_paths = [
            "notion.db",
            "state.json",
            "Preferences",
            "Local State",
            "Local Storage",
            "Network",
        ]
        .map(|relative| notion_root.join(relative))
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
        for partition in &partition_roots {
            for relative in [
                "IndexedDB",
                "Local Storage",
                "Network",
                "Session Storage",
                "WebStorage",
                "Preferences",
                "Service Worker",
            ] {
                let path = partition.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        assert!(
            preserved_paths.len() >= 10,
            "the real Notion profile must expose representative database and offline state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let mut markers = Vec::with_capacity(partition_roots.len() + 1);
        let top_level_cache = notion_root.join("Cache");
        fs::create_dir_all(&top_level_cache).expect("the Notion top-level cache must be writable");
        markers.push(top_level_cache.join("strawberrydisk-rule-validation.bin"));
        for partition in &partition_roots {
            let cache = partition.join("Cache");
            fs::create_dir_all(&cache).expect("the Notion partition cache must be writable");
            markers.push(cache.join("strawberrydisk-rule-validation.bin"));
        }
        for marker in &markers {
            fs::write(marker, b"payload")
                .expect("the isolated Notion cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.notion-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Notion cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Notion cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_notion_partition_cleanup expected_bytes={} released_bytes={} affected_item_count={} partition_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            partition_roots.len(),
            preserved_paths.len()
        );
    }

    /// Exercises the dedicated Signal process boundary while the signed client
    /// is running. Real execution must stop before reading account, database,
    /// optional-resource, or browser-persistence roots.
    #[test]
    #[ignore = "requires the real Signal application to be running"]
    fn real_signal_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_SIGNAL_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_SIGNAL_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Signal.exe".to_string()]);
        assert!(!running.is_empty(), "Signal must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.signal-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Signal cleanup must return a structured result");
        assert_eq!(result.actions.len(), 1);
        let action = &result.actions[0];
        assert_eq!(action.status, crate::cleanup::CleanupActionStatus::Blocked);
        assert_eq!(
            action.reason_code,
            Some(crate::cleanup::CleanupActionReason::RunningProcesses)
        );
        assert_eq!(action.released_bytes, 0);
        assert_eq!(action.affected_item_count, 0);
        assert!(!action.running_processes.is_empty());
        println!(
            "real_windows_signal_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Exercises the production Signal rule against a real signed Signal
    /// installation after an interactive first launch. The rule intentionally
    /// clears the disposable Chromium leaves, so the environment gate prevents
    /// accidental use on a developer's daily profile. Digests of representative
    /// account, database, storage, and optional-resource paths prove that the
    /// broad Signal user-data directory never becomes the deletion boundary.
    #[test]
    #[ignore = "clears real Signal rendering caches in an isolated Windows VM"]
    fn real_signal_cache_preserves_account_and_message_state_roots() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_SIGNAL_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_SIGNAL_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let signal_root = roaming_app_data.join("Signal");
        assert!(
            signal_root.is_dir(),
            "Signal must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Signal.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Signal must be completely stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            "config.json",
            "optionalResources",
            "sql",
            "IndexedDB",
            "Local Storage",
        ]
        .map(|relative| signal_root.join(relative));
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the real profile must expose representative non-cache state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let cache_root = signal_root.join("Cache");
        fs::create_dir_all(&cache_root).expect("the Signal HTTP cache root must be writable");
        let marker = cache_root.join("strawberrydisk-rule-validation.bin");
        fs::write(&marker, b"payload").expect("the isolated Signal cache marker must be created");
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.signal-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Signal cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 7);
        assert!(marker.exists(), "dry-run must preserve the cache marker");

        let result = CleanupService::execute(request(false))
            .expect("the real Signal cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 7);
        assert!(
            !marker.exists(),
            "the selected cache marker must be deleted"
        );
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_signal_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }

    /// Validates Telegram's two fixed temporary roots without assigning cleanup
    /// meaning to neighboring hashed tdata entries. Telegram deliberately mixes
    /// account keys, settings, local storage, downloaded emoji resources, and
    /// disposable files in one directory, so every non-target direct child is
    /// hashed before and after the production cleanup.
    #[test]
    #[ignore = "clears real Telegram temporary data in an isolated Windows VM"]
    fn real_telegram_temporary_cache_preserves_all_other_tdata_entries() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_TELEGRAM_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_TELEGRAM_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let tdata_root = roaming_app_data.join("Telegram Desktop/tdata");
        assert!(
            tdata_root.is_dir(),
            "Telegram must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Telegram.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Telegram must be completely stopped before the real cleanup diagnostic"
        );

        let preserved_snapshot = || {
            let mut entries = fs::read_dir(&tdata_root)
                .expect("the Telegram tdata root must remain readable")
                .map(|entry| entry.expect("the tdata entry must be readable").path())
                .filter(|path| {
                    !matches!(
                        path.file_name().and_then(|name| name.to_str()),
                        Some("temp" | "dumps")
                    )
                })
                .map(|path| {
                    let name = path
                        .file_name()
                        .expect("the tdata entry must have a name")
                        .to_os_string();
                    (name, digest_tree(&path))
                })
                .collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            entries
        };
        let preserved_before = preserved_snapshot();
        assert!(
            !preserved_before.is_empty(),
            "the real profile must expose non-cache tdata state"
        );

        let temp_root = tdata_root.join("temp");
        let dumps_root = tdata_root.join("dumps");
        fs::create_dir_all(&temp_root).expect("the Telegram temp root must be writable");
        fs::create_dir_all(&dumps_root).expect("the Telegram dumps root must be writable");
        let temp_marker = temp_root.join("strawberrydisk-rule-validation.tmp");
        let dump_marker = dumps_root.join("strawberrydisk-rule-validation.dmp");
        fs::write(&temp_marker, b"payload").expect("the Telegram temp marker must be created");
        fs::write(&dump_marker, b"payload").expect("the Telegram dump marker must be created");
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.telegram-temporary-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Telegram temporary-data dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 14);
        assert!(temp_marker.exists() && dump_marker.exists());

        let result = CleanupService::execute(request(false))
            .expect("the real Telegram temporary-data cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 14);
        assert!(!temp_marker.exists() && !dump_marker.exists());
        assert_eq!(preserved_snapshot(), preserved_before);
        println!(
            "real_telegram_temporary_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_entry_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_before.len()
        );
    }

    /// Runs the production VLC rule against artwork reproduced from an embedded
    /// cover. A marker beside the two owned roots proves that the shared vlc
    /// parent remains configuration space rather than becoming a broad cache
    /// root. Extension filtering is exercised in both artwork and crashdump.
    #[test]
    #[ignore = "clears real VLC artwork and dump files in an isolated Windows VM"]
    fn real_vlc_cache_preserves_configuration_and_playlist_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_VLC_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_VLC_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let vlc_root = roaming_app_data.join("vlc");
        let art_root = vlc_root.join("art");
        assert!(
            art_root.is_dir(),
            "VLC must have cached a real embedded cover before this diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["vlc.exe".to_string()]);
        assert!(
            running.is_empty(),
            "VLC must be completely stopped before the real cleanup diagnostic"
        );

        fs::create_dir_all(&vlc_root).expect("the VLC data root must be writable");
        let preserved_marker = vlc_root.join("strawberrydisk-rule-validation.cfg");
        fs::write(&preserved_marker, b"preserve")
            .expect("the VLC non-cache marker must be created");
        let preserved_before = digest_tree(&preserved_marker);
        let art_marker = art_root.join("strawberrydisk-rule-validation.png");
        fs::write(&art_marker, b"payload").expect("the VLC artwork marker must be created");
        let dump_root = vlc_root.join("crashdump");
        fs::create_dir_all(&dump_root).expect("the VLC crashdump root must be writable");
        let dump_marker = dump_root.join("strawberrydisk-rule-validation.dmp");
        fs::write(&dump_marker, b"payload").expect("the VLC dump marker must be created");
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.vlc-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real VLC cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 14);
        assert!(art_marker.exists() && dump_marker.exists());

        let result = CleanupService::execute(request(false))
            .expect("the real VLC cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 14);
        assert!(!art_marker.exists() && !dump_marker.exists());
        assert_eq!(digest_tree(&preserved_marker), preserved_before);
        fs::remove_file(&preserved_marker).expect("the VLC preserved marker must be removed");
        println!(
            "real_vlc_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={}",
            preview.expected_bytes, result.released_bytes, result.affected_item_count
        );
    }

    /// Verifies EA's versioned CEF discovery and its independent QML cache.
    /// Browser storage and offline state are hashed because their neighboring
    /// names resemble Chromium cache data but can carry login or product state.
    /// The persistent background service may remain active; all processes that
    /// own the per-user interface are required to be stopped.
    #[test]
    #[ignore = "clears real EA interface caches in an isolated Windows VM"]
    fn real_ea_rendering_cache_preserves_browser_and_offline_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_EA_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_EA_CACHE=1 only in an isolated Windows VM"
        );
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let ea_root = local_app_data.join("Electronic Arts/EA Desktop");
        let cef_root = ea_root.join("CEF");
        let version_root = fs::read_dir(&cef_root)
            .expect("the EA CEF generation root must be readable")
            .map(|entry| entry.expect("the CEF entry must be readable").path())
            .find(|path| path.join("EADesktop/BrowserCache").is_dir())
            .expect("a real EA CEF generation must contain BrowserCache");
        let browser_root = version_root.join("EADesktop/BrowserCache");
        let qml_cache = local_app_data.join("EADesktop/cache/qmlcache");
        assert!(qml_cache.is_dir(), "the real EA QML cache must exist");
        let required_processes =
            ["EADesktop.exe", "EACefSubProcess.exe", "EALocalHostSvc.exe"].map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&required_processes);
        assert!(
            running.is_empty(),
            "EA interface processes must be stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            browser_root.join("Local Storage"),
            browser_root.join("Network"),
            browser_root.join("Session Storage"),
            ea_root.join("OfflineCache"),
        ];
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the real EA profile must expose representative non-cache state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let http_cache = browser_root.join("Cache");
        fs::create_dir_all(&http_cache).expect("the EA HTTP cache root must be writable");
        let cef_marker = http_cache.join("strawberrydisk-rule-validation.bin");
        let qml_marker = qml_cache.join("strawberrydisk-rule-validation.qmlc");
        fs::write(&cef_marker, b"payload").expect("the EA CEF marker must be created");
        fs::write(&qml_marker, b"payload").expect("the EA QML marker must be created");
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.ea-rendering-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview =
            CleanupService::execute(request(true)).expect("the real EA cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 14);
        assert!(cef_marker.exists() && qml_marker.exists());

        let result = CleanupService::execute(request(false))
            .expect("the real EA cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 14);
        assert!(!cef_marker.exists() && !qml_marker.exists());
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_ea_rendering_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }

    /// Steam's `htmlcache` is a Chromium profile that mixes disposable renderer
    /// data with account-adjacent browser state. Hashing the representative
    /// databases, preferences, and storage directories proves that the rule
    /// selects only cache leaves. Game downloads and per-game shader caches are
    /// deliberately outside this test and outside the production rule.
    #[test]
    #[ignore = "clears real Steam interface caches in an isolated Windows VM"]
    fn real_steam_rendering_cache_preserves_browser_and_game_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_STEAM_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_STEAM_CACHE=1 only in an isolated Windows VM"
        );
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let html_cache = local_app_data.join("Steam/htmlcache");
        let profile = html_cache.join("Default");
        assert!(profile.is_dir(), "Steam must have completed a first launch");
        let required_processes = ["steam.exe", "steamwebhelper.exe"].map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&required_processes);
        assert!(
            running.is_empty(),
            "Steam and its web helpers must be stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            profile.join("Login Data"),
            profile.join("History"),
            profile.join("Preferences"),
            profile.join("Local Storage"),
            profile.join("Network"),
            profile.join("Session Storage"),
        ];
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the real Steam profile must expose representative non-cache state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let http_cache = profile.join("Cache");
        let gpu_cache = profile.join("GPUCache");
        fs::create_dir_all(&http_cache).expect("the Steam HTTP cache root must be writable");
        fs::create_dir_all(&gpu_cache).expect("the Steam GPU cache root must be writable");
        let http_marker = http_cache.join("strawberrydisk-rule-validation.bin");
        let gpu_marker = gpu_cache.join("strawberrydisk-rule-validation.bin");
        fs::write(&http_marker, b"payload").expect("the Steam HTTP marker must be created");
        fs::write(&gpu_marker, b"payload").expect("the Steam GPU marker must be created");
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.steam-rendering-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Steam cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 14);
        assert!(http_marker.exists() && gpu_marker.exists());

        let result = CleanupService::execute(request(false))
            .expect("the real Steam cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 14);
        assert!(!http_marker.exists() && !gpu_marker.exists());
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_steam_rendering_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }
