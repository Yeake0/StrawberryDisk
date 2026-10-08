    /// Executes the production Dart cache rule in a snapshot-backed Windows VM.
    ///
    /// The test deletes the current account's real `.dartServer` cache and
    /// therefore requires an explicit environment gate. It validates dry-run,
    /// Known Folder resolution, whole-root deletion, live accounting, and final
    /// root absence while printing only aggregate counts and timings.
    #[test]
    #[ignore = "deletes the real Dart analysis cache in an isolated Windows VM"]
    fn real_dart_analysis_cache_uses_whole_root_cleanup() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_DART_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_DART_CACHE=1 only in a snapshot-backed Windows VM"
        );
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let cache_root = local_app_data.join(".dartServer");
        assert!(
            cache_root.is_dir(),
            "the real Dart cache fixture must exist"
        );
        let request = || CleanupRequest {
            rule_ids: vec!["dev.dart-analysis-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        };

        let preview_started = Instant::now();
        let mut preview_request = request();
        preview_request.dry_run = true;
        let preview = CleanupService::execute(preview_request)
            .expect("the real Dart cache preview must succeed");
        let preview_ms = preview_started.elapsed().as_secs_f64() * 1_000.0;
        assert_eq!(preview.actions.len(), 1);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes > 0);
        assert!(cache_root.exists(), "dry-run must preserve the Dart cache");

        let cleanup_started = Instant::now();
        let result =
            CleanupService::execute(request()).expect("the real Dart cache cleanup must succeed");
        let cleanup_ms = cleanup_started.elapsed().as_secs_f64() * 1_000.0;
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes > 0);
        assert!(result.affected_item_count > 0);
        assert!(
            !cache_root.exists(),
            "the complete Dart cache root must be removed"
        );
        println!(
            "real_dart_analysis_cleanup preview_ms={preview_ms:.2} cleanup_ms={cleanup_ms:.2} expected_bytes={} released_bytes={} affected_item_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count
        );
    }

    /// Confirms that both the desktop client and its crash handler own the
    /// Tencent Meeting profile. Cleanup must stop before traversing the mixed
    /// tree containing downloaded models, resources, databases, and settings.
    #[test]
    #[ignore = "requires the real Tencent Meeting application to be running"]
    fn real_tencent_meeting_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_WINDOWS_TENCENT_MEETING_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_WINDOWS_TENCENT_MEETING_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&[
                "WeMeetApp.exe".to_string(),
                "WeMeetCrashHandler.exe".to_string(),
            ]);
        assert!(!running.is_empty(), "Tencent Meeting must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.tencent-meeting-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Tencent Meeting cleanup must return a structured result");
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
            "real_windows_tencent_meeting_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Clears fixed cache suffixes from every timestamped WebView profile and
    /// both log roots. Digests protect downloaded models and dynamic resources,
    /// account databases and preferences, per-user meeting state, and browser
    /// preferences, network state, and Local/Session Storage.
    #[test]
    #[ignore = "clears real Tencent Meeting caches in an isolated Windows VM"]
    fn real_tencent_meeting_cache_preserves_account_and_meeting_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_WINDOWS_TENCENT_MEETING_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_WINDOWS_TENCENT_MEETING_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let meeting_root = roaming_app_data.join("Tencent/WeMeet");
        let webkit_root = meeting_root.join("Global/Data/WebkitCacheData");
        assert!(
            webkit_root.is_dir(),
            "Tencent Meeting must complete a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&[
                "WeMeetApp.exe".to_string(),
                "WeMeetCrashHandler.exe".to_string(),
            ]);
        assert!(
            running.is_empty(),
            "Tencent Meeting must be completely stopped before cleanup"
        );

        let profiles = fs::read_dir(&webkit_root)
            .expect("the WebView profile root must be readable")
            .map(|entry| {
                entry
                    .expect("the WebView profile entry must be readable")
                    .path()
            })
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        assert!(
            !profiles.is_empty(),
            "the real client must expose a WebView profile"
        );

        let mut preserved_paths = [
            "Global/Data/AudioModel",
            "Global/Data/DynamicResource",
            "Global/Data/DynamicResourcePackage",
            "Global/Data/StartUp",
            "Global/Data/Timeline",
            "Global/Data/Timezone",
            "Global/Data/VirtualBkg",
            "Global/Data/XCast",
            "Global/Database",
            "Global/Preferences",
            "Global/Upgrade",
            "Global/voiceprint_record",
            "Users",
        ]
        .map(|relative| meeting_root.join(relative))
        .to_vec();
        for profile in &profiles {
            for relative in [
                "Default/Local Storage",
                "Default/Session Storage",
                "Default/Network",
                "Default/Preferences",
            ] {
                let path = profile.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the initialized client must expose durable account, meeting, and browser state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let cache_suffixes = [
            "BrowserMetrics",
            "Default/Cache",
            "Default/Code Cache",
            "Default/GPUCache",
            "Default/DawnCache",
            "Default/DawnGraphiteCache",
            "Default/DawnWebGPUCache",
            "Default/GrShaderCache",
            "Default/GraphiteDawnCache",
            "Default/Shared Dictionary/cache",
            "GraphiteDawnCache",
            "GrShaderCache",
            "ShaderCache",
        ];
        let mut target_roots = vec![
            meeting_root.join("Global/Logs"),
            local_app_data.join("Tencent/WeMeet/Logs"),
        ];
        for profile in &profiles {
            for suffix in cache_suffixes {
                let path = profile.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 8,
            "the initialized client must expose verified cache and log roots"
        );

        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::create_dir_all(marker.parent().expect("the marker must have a parent"))
                .expect("the Tencent Meeting target root must be writable");
            fs::write(marker, b"payload")
                .expect("the Tencent Meeting cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.tencent-meeting-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Tencent Meeting cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Tencent Meeting cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_windows_tencent_meeting_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} profile_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            profiles.len(),
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Confirms that Sogou Input's broker, cloud, tool, and smart-assistant
    /// processes block cleanup before the shared CEF profile is traversed.
    #[test]
    #[ignore = "requires real Sogou Input processes to be running"]
    fn real_sogou_input_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_SOGOU_INPUT_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_SOGOU_INPUT_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_names = [
            "SGMyInput.exe",
            "SGTool.exe",
            "SGWebRender.exe",
            "SogouCloud.exe",
            "SogouImeBroker.exe",
            "SOGOUSmartAssistant.exe",
        ]
        .map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&process_names);
        assert!(!running.is_empty(), "Sogou Input must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.sogou-input-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Sogou Input cleanup must return a structured result");
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
            "real_windows_sogou_input_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Clears only Sogou Input's fixed CEF response and code-cache leaves.
    /// Full digests protect dictionaries, personalization, backups, settings,
    /// models, components, updates, and every adjacent browser-storage family.
    #[test]
    #[ignore = "clears real Sogou Input CEF caches in an isolated Windows VM"]
    fn real_sogou_input_cache_preserves_dictionary_and_personalization_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_SOGOU_INPUT_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_SOGOU_INPUT_CACHE=1 only in an isolated Windows VM"
        );
        let process_names = [
            "SGMyInput.exe",
            "SGTool.exe",
            "SGWebRender.exe",
            "SogouCloud.exe",
            "SogouImeBroker.exe",
            "SOGOUSmartAssistant.exe",
        ]
        .map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&process_names);
        assert!(
            running.is_empty(),
            "all Sogou Input owner processes must be stopped before cleanup"
        );

        let program_data = std::env::var_os("PROGRAMDATA")
            .map(PathBuf::from)
            .expect("PROGRAMDATA must be available");
        let user_profile = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .expect("USERPROFILE must be available");
        let sogou_root = program_data.join("SogouInput");
        let cef_root = sogou_root.join("SGCefCache/SGMyInput/CefLocalStorage");
        assert!(
            cef_root.is_dir(),
            "Sogou Input must complete a first launch"
        );

        let preserved_candidates = [
            user_profile.join("AppData/LocalLow/SogouPY.users"),
            user_profile.join("AppData/LocalLow/SogouPY/Backup"),
            user_profile.join("AppData/LocalLow/SogouPY/Indv"),
            user_profile.join("AppData/LocalLow/SogouPY/mmkv"),
            user_profile.join("AppData/LocalLow/SogouPY/scd"),
            sogou_root.join("Components"),
            sogou_root.join("SGBizConfig"),
            sogou_root.join("SGSmartAssistant"),
            sogou_root.join("ShiplyUpdate"),
            sogou_root.join("skinrootdir"),
            cef_root.join("blob_storage"),
            cef_root.join("databases"),
            cef_root.join("IndexedDB"),
            cef_root.join("Local Storage"),
            cef_root.join("Session Storage"),
        ];
        let preserved_paths = preserved_candidates
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        assert!(
            preserved_paths.len() >= 12,
            "the initialized input method must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let target_roots = [cef_root.join("Cache"), cef_root.join("Code Cache")]
            .into_iter()
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        assert_eq!(
            target_roots.len(),
            2,
            "the initialized CEF profile must expose response and code caches"
        );
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the Sogou Input cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.sogou-input-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Sogou Input cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Sogou Input cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_windows_sogou_input_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Confirms that every Windows WPS cache boundary fails closed while the
    /// editor, cloud service, renderer, or updater still owns shared profile
    /// files. All three rules use the same exhaustive process list so one
    /// overlooked helper cannot leave only part of the cleanup plan writable.
    #[test]
    #[ignore = "requires real WPS Office processes to be running"]
    fn real_wps_caches_block_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_WPS_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_WPS_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_names = [
            "wpsoffice.exe",
            "wps.exe",
            "et.exe",
            "wpp.exe",
            "wpspdf.exe",
            "wpscloudsvr.exe",
            "wpscenter.exe",
            "promecefpluginhost.exe",
            "ksomisc.exe",
            "wpsupdate.exe",
        ]
        .map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&process_names);
        assert!(!running.is_empty(), "WPS Office must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "app.wps-cache".to_string(),
                "app.wps-rendering-cache".to_string(),
                "app.wps-diagnostic-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked WPS cleanup must return a structured result");
        assert_eq!(result.actions.len(), 3);
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
            "real_windows_wps_cache_block running_process_count={} blocked_action_count={}",
            running.len(),
            result.actions.len()
        );
    }

    /// Clears only WPS's dedicated application cache, fixed CEF response/code
    /// leaves, and diagnostic log/dump directories. A full digest of every
    /// adjacent subtree protects the temporary validation document, recovery
    /// and backup state, add-ons, settings, account databases, cloud-file state,
    /// and persistent browser storage from accidental matcher expansion.
    #[test]
    #[ignore = "clears real WPS caches in an isolated Windows VM"]
    fn real_wps_caches_preserve_documents_account_and_recovery_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_WPS_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_WPS_CACHE=1 only in an isolated Windows VM"
        );
        let process_names = [
            "wpsoffice.exe",
            "wps.exe",
            "et.exe",
            "wpp.exe",
            "wpspdf.exe",
            "wpscloudsvr.exe",
            "wpscenter.exe",
            "promecefpluginhost.exe",
            "ksomisc.exe",
            "wpsupdate.exe",
        ]
        .map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&process_names);
        assert!(
            running.is_empty(),
            "all WPS Office owner processes must be stopped before cleanup"
        );

        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let kingsoft = roaming.join("Kingsoft");
        let office6 = kingsoft.join("office6");
        let wps = kingsoft.join("wps");
        let office_cache = office6.join("cache");
        let cef_root = wps.join("addons/data/win-i386/cef/2");
        let validation_document =
            local.join("Temp/StrawberryDisk-WPS-Rule-20260811/wps-validation-document.rtf");
        assert!(
            office_cache.is_dir(),
            "WPS must reproduce its application cache"
        );
        assert!(cef_root.is_dir(), "WPS must reproduce its CEF profile root");
        assert!(
            validation_document.is_file(),
            "the temporary validation document must remain available"
        );

        let preserved_roots = [
            kingsoft.join("PDF"),
            kingsoft.join("kaccountsdk"),
            kingsoft.join("qing"),
            kingsoft.join("WPS Cloud Files"),
            local.join("Kingsoft/WPS Cloud Files"),
            local.join("CEF/User Data"),
            validation_document.clone(),
        ]
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
        assert!(
            preserved_roots.len() >= 5,
            "the initialized WPS profile must expose representative durable state"
        );
        let preserved_snapshot = || {
            let mut digests = preserved_roots
                .iter()
                .map(|path| digest_tree(path))
                .collect::<Vec<_>>();
            // The mixed-purpose office and add-on trees are hashed as a whole
            // except for the exact path segments owned by these three rules.
            digests.push(digest_tree_excluding_segments(
                &office6,
                &["cache", "log", "dump"],
            ));
            digests.push(digest_tree_excluding_segments(
                &wps,
                &["Cache", "Code Cache"],
            ));
            digests
        };
        let preserved_before = preserved_snapshot();

        let mut target_roots = vec![office_cache, office6.join("log")];
        for candidate in [
            office6.join("OfficeSpace/log"),
            office6.join("OfficeSpace/dump"),
        ] {
            if candidate.is_dir() {
                target_roots.push(candidate);
            }
        }
        let cef_targets = directories_with_leaf_names(&cef_root, &["Cache", "Code Cache"]);
        let cef_partition_count = cef_targets
            .iter()
            .filter_map(|path| path.parent())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        target_roots.extend(cef_targets);
        target_roots.retain(|path| path.is_dir());
        assert!(
            target_roots.len() >= 5,
            "the initialized WPS profile must expose all three cleanup families"
        );

        let markers = target_roots
            .iter()
            .enumerate()
            .map(|(index, root)| root.join(format!("strawberrydisk-rule-validation-{index}.bin")))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the WPS cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.wps-cache".to_string(),
                "app.wps-rendering-cache".to_string(),
                "app.wps-diagnostic-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real WPS cache dry run must succeed");
        assert_eq!(preview.actions.len(), 3);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real WPS cache cleanup must succeed");
        assert_eq!(result.actions.len(), 3);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_snapshot();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_windows_wps_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} cef_partition_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            cef_partition_count,
            target_roots.len(),
            preserved_roots.len() + 2
        );
    }

    /// Exercises the NetEase Cloud Music owner boundary while the real client
    /// is running. Its renderer cache shares one application root with music,
    /// account, library, and browser state, so traversal must not start until
    /// every cloudmusic process has stopped.
    #[test]
    #[ignore = "requires the real NetEase Cloud Music application to be running"]
    fn real_netease_cloud_music_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_NETEASE_CLOUD_MUSIC_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_NETEASE_CLOUD_MUSIC_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["cloudmusic.exe".to_string()]);
        assert!(!running.is_empty(), "NetEase Cloud Music must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.netease-cloud-music-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked NetEase Cloud Music cleanup must return a structured result");
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
            "real_windows_netease_cloud_music_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Exercises only the fixed Chromium and diagnostic-log leaves produced by
    /// the official NetEase client. The top-level Cache is intentionally hashed
    /// rather than selected because it may contain music data. Library,
    /// downloaded web data, preferences, cookies, IndexedDB, Local/Session
    /// Storage, quota state, and crash dumps are also preserved explicitly.
    #[test]
    #[ignore = "clears real NetEase Cloud Music caches in an isolated Windows VM"]
    fn real_netease_cloud_music_cache_preserves_library_and_account_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_NETEASE_CLOUD_MUSIC_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_NETEASE_CLOUD_MUSIC_CACHE=1 only in an isolated Windows VM"
        );
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let cloud_music_root = local_app_data.join("NetEase/CloudMusic");
        let renderer_root = cloud_music_root.join("webapp91x64");
        assert!(
            renderer_root.is_dir(),
            "NetEase Cloud Music must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["cloudmusic.exe".to_string()]);
        assert!(
            running.is_empty(),
            "NetEase Cloud Music must be completely stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            "Cache",
            "Library",
            "Statics",
            "webdata",
            "dumps",
            "localdata",
            "localware",
            "webapp91x64/Cookies",
            "webapp91x64/IndexedDB",
            "webapp91x64/Local Storage",
            "webapp91x64/Session Storage",
            "webapp91x64/LocalPrefs.json",
            "webapp91x64/Network Persistent State",
            "webapp91x64/QuotaManager",
        ]
        .map(|relative| cloud_music_root.join(relative));
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the real profile must expose representative music, account, and browser state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let target_roots = [renderer_root.join("Cache"), cloud_music_root.join("Log")];
        for root in &target_roots {
            fs::create_dir_all(root).expect("the NetEase Cloud Music cache root must be writable");
        }
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload")
                .expect("the isolated NetEase Cloud Music cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.netease-cloud-music-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real NetEase Cloud Music cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real NetEase Cloud Music cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_netease_cloud_music_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }
