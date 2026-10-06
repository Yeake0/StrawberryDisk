    /// Confirms that the signed WPS application, cloud service, and CEF hosts
    /// block both cache and diagnostic cleanup before profile traversal.
    #[test]
    #[ignore = "requires the real WPS Office application to be running"]
    fn real_wps_caches_block_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_WPS_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_WPS_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_names = [
            "wpsoffice",
            "WPS Office",
            "wpscloudsvr",
            "promecefpluginhost",
            "promecefpluginhost (GPU)",
            "promecefpluginhost (Renderer)",
        ]
        .map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&process_names);
        assert!(!running.is_empty(), "WPS Office must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "app.wps-cache".to_string(),
                "app.wps-diagnostic-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked WPS cleanup must return structured results");
        assert_eq!(result.actions.len(), 2);
        for action in &result.actions {
            assert_eq!(action.status, crate::cleanup::CleanupActionStatus::Blocked);
            assert_eq!(
                action.reason_code,
                Some(crate::cleanup::CleanupActionReason::RunningProcesses)
            );
            assert_eq!(action.released_bytes, 0);
            assert_eq!(action.affected_item_count, 0);
            assert!(!action.running_processes.is_empty());
        }
        println!(
            "real_macos_wps_cache_block running_process_count={}",
            running.len()
        );
    }
    /// Clears WPS's sandbox cache and fixed diagnostic leaves. Opaque digests
    /// protect the signed-in cloud profile, cloud file cache, recovery/import
    /// state, preferences, HTTP storage, group container, and CEF website data.
    #[test]
    #[ignore = "permanently clears real WPS Office cache and diagnostic data"]
    fn real_wps_caches_preserve_documents_account_and_recovery_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_WPS_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_WPS_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_names = [
            "wpsoffice",
            "WPS Office",
            "wpscloudsvr",
            "promecefpluginhost",
            "promecefpluginhost (GPU)",
            "promecefpluginhost (Renderer)",
        ]
        .map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&process_names);
        assert!(running.is_empty(), "WPS Office must be completely stopped");

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let container = home.join("Library/Containers/com.kingsoft.wpsoffice.mac.global/Data");
        let library = container.join("Library");
        let app_support = library.join("Application Support");
        let kingsoft = app_support.join("Kingsoft");
        let office6 = kingsoft.join("office6");
        let office_space = office6.join("OfficeSpace");
        let cache_root = library.join("Caches/com.kingsoft.wpsoffice.mac.global");
        let target_roots = [
            cache_root,
            office6.join("log"),
            office_space.join("log"),
            office_space.join("dump"),
        ]
        .into_iter()
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
        assert_eq!(
            target_roots.len(),
            4,
            "the initialized profile must expose all verified WPS cache and diagnostic roots"
        );

        let preserved_candidates = [
            container.join("Documents"),
            app_support.join("CEF/User Data"),
            app_support.join("Google"),
            kingsoft.join("qing"),
            kingsoft.join("WPS Cloud Files"),
            library.join("HTTPStorages/com.kingsoft.wpsoffice.mac.global"),
            library.join("Preferences"),
            home.join("Library/Group Containers/2G98R5QYU5.wpsoffice"),
        ];
        let preserved_paths = preserved_candidates
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        assert!(
            preserved_paths.len() >= 7,
            "the signed-in WPS profile must expose representative durable state"
        );
        let preserved_snapshot = || {
            let mut digests = preserved_paths
                .iter()
                .map(|path| digest_macos_tree_without_following_links(path))
                .collect::<Vec<_>>();
            digests.push(digest_macos_tree_with_exclusions_without_following_links(
                &office6,
                &["log", "OfficeSpace"],
            ));
            digests.push(digest_macos_tree_with_exclusions_without_following_links(
                &office_space,
                &["log", "dump"],
            ));
            digests
        };
        let preserved_before = preserved_snapshot();

        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the WPS cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.wps-cache".to_string(),
                "app.wps-diagnostic-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real WPS cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real WPS cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_snapshot();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_wps_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len() + 2
        );
    }

    /// Confirms that the signed UC browser and its Chromium helper processes
    /// block cleanup before any real profile or component cache is traversed.
    #[test]
    #[ignore = "requires the real UC browser to be running"]
    fn real_uc_browser_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_UC_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_UC_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_names =
            ["UC", "UC Helper", "UC Helper (GPU)", "UC Helper (Renderer)"].map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&process_names);
        assert!(!running.is_empty(), "UC browser must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["browser.uc-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked UC cleanup must return a structured result");
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
            "real_macos_uc_cache_block running_process_count={}",
            running.len()
        );
    }

    /// Clears only UC's dedicated HTTP, code, GPU, shader, and downloaded
    /// component-package cache roots. Full digests protect representative
    /// credentials, cookies, history, bookmarks, extensions, sessions, local
    /// storage, Service Worker state, downloads, and browser preferences.
    #[test]
    #[ignore = "permanently clears real UC browser caches"]
    fn real_uc_browser_cache_preserves_profile_and_download_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_UC_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_UC_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_names =
            ["UC", "UC Helper", "UC Helper (GPU)", "UC Helper (Renderer)"].map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&process_names);
        assert!(running.is_empty(), "UC browser must be completely stopped");

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let uc_root = home.join("Library/Application Support/UC");
        let profile = uc_root.join("Default");
        assert!(profile.is_dir(), "UC must complete a first launch");

        let preserved_candidates = [
            uc_root.join("Local State"),
            uc_root.join("NativeMessagingHosts"),
            uc_root.join("user_info"),
            profile.join("Bookmarks"),
            profile.join("Cookies"),
            profile.join("History"),
            profile.join("Login Data"),
            profile.join("Network"),
            profile.join("Extensions"),
            profile.join("Local Storage"),
            profile.join("Preferences"),
            profile.join("Service Worker"),
            profile.join("Session Storage"),
            profile.join("Sessions"),
            profile.join("WebStorage"),
        ];
        let preserved_paths = preserved_candidates
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        assert!(
            preserved_paths.len() >= 12,
            "the initialized UC profile must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();

        let target_candidates = [
            home.join("Library/Caches/UC"),
            home.join("Library/Caches/org.uc.UC"),
            uc_root.join("ShaderCache"),
            uc_root.join("GrShaderCache"),
            uc_root.join("GraphiteDawnCache"),
            uc_root.join("component_crx_cache"),
            profile.join("Cache"),
            profile.join("Code Cache"),
            profile.join("GPUCache"),
            profile.join("DawnGraphiteCache"),
            profile.join("DawnWebGPUCache"),
        ];
        let target_roots = target_candidates
            .into_iter()
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        assert!(
            target_roots.len() >= 9,
            "the initialized UC profile must expose verified cache roots"
        );
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the UC cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["browser.uc-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview =
            CleanupService::execute(request(true)).expect("the real UC cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real UC cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_uc_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Confirms that 360 Extreme Browser and its Chromium helper processes
    /// block cleanup before any real profile or component cache is traversed.
    #[test]
    #[ignore = "requires the real 360 Extreme Browser to be running"]
    fn real_360_speed_browser_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_360_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_360_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_names = [
            "360Chrome",
            "360Chrome Helper",
            "360Chrome Helper (GPU)",
            "360Chrome Helper (Renderer)",
        ]
        .map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&process_names);
        assert!(!running.is_empty(), "360 Extreme Browser must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["browser.360-speed-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked 360 cleanup must return a structured result");
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
            "real_macos_360_cache_block running_process_count={}",
            running.len()
        );
    }

    /// Clears only 360 Extreme Browser's dedicated HTTP, code, GPU, shader,
    /// and downloaded component-package caches. Full digests protect profile
    /// credentials, cookies, history, extensions, sessions, local storage, and
    /// preferences from accidental overlap with the cache boundary.
    #[test]
    #[ignore = "permanently clears real 360 Extreme Browser caches"]
    fn real_360_speed_browser_cache_preserves_profile_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_360_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_360_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_names = [
            "360Chrome",
            "360Chrome Helper",
            "360Chrome Helper (GPU)",
            "360Chrome Helper (Renderer)",
        ]
        .map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&process_names);
        assert!(
            running.is_empty(),
            "360 Extreme Browser must be completely stopped"
        );

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let browser_root = home.join("Library/Application Support/360Chrome");
        let profile = browser_root.join("Default");
        assert!(
            profile.is_dir(),
            "360 Extreme Browser must complete a first launch"
        );

        let preserved_candidates = [
            browser_root.join("Local State"),
            browser_root.join("NativeMessagingHosts"),
            profile.join("Bookmarks"),
            profile.join("Cookies"),
            profile.join("History"),
            profile.join("Login Data"),
            profile.join("Network"),
            profile.join("Extensions"),
            profile.join("Local Storage"),
            profile.join("Preferences"),
            profile.join("Service Worker"),
            profile.join("Session Storage"),
            profile.join("Sessions"),
            profile.join("WebStorage"),
        ];
        let preserved_paths = preserved_candidates
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        assert!(
            preserved_paths.len() >= 9,
            "the initialized 360 profile must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();

        let target_candidates = [
            home.join("Library/Caches/360Chrome"),
            browser_root.join("ShaderCache64"),
            browser_root.join("GrShaderCache64"),
            browser_root.join("GraphiteDawnCache"),
            browser_root.join("component_crx_cache"),
            profile.join("GPUCache64"),
            profile.join("DawnGraphiteCache"),
            profile.join("DawnWebGPUCache"),
        ];
        let target_roots = target_candidates
            .into_iter()
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        assert_eq!(
            target_roots.len(),
            8,
            "the initialized 360 profile must expose every verified cache root"
        );
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the 360 cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["browser.360-speed-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real 360 cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real 360 cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_360_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }
