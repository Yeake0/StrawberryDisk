    /// Proves that the current signed WeChat, WeCom, and QQ clients reach the
    /// production process gate before any sandbox profile is traversed. These
    /// applications keep messages and account databases beside Chromium cache
    /// leaves, so real executable-name matching is required safety evidence.
    #[test]
    #[ignore = "requires the real WeChat, WeCom, and QQ applications to be running"]
    fn real_tencent_application_caches_block_running_owners() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_TENCENT_MACOS_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_TENCENT_MACOS_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let cases = [
            ("app.wechat-cache", vec!["WeChat".to_string()]),
            (
                "app.wecom-cache",
                vec![
                    "WeCom".to_string(),
                    "WXWork".to_string(),
                    "\u{4f01}\u{4e1a}\u{5fae}\u{4fe1}".to_string(),
                ],
            ),
            ("app.qq-cache", vec!["QQ".to_string()]),
        ];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");

        for (rule_id, process_names) in &cases {
            assert!(
                !process_snapshot
                    .matching_processes(process_names)
                    .is_empty(),
                "every Tencent cache owner must be running"
            );
            let result = CleanupService::execute(CleanupRequest {
                rule_ids: vec![(*rule_id).to_string()],
                source_selections: Vec::new(),
                dry_run: false,
                project_roots: Vec::new(),
            })
            .expect("the blocked Tencent cleanup must return a structured result");
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
        }
        println!("real_macos_tencent_cache_block owner_count={}", cases.len());
    }
    /// Clears fixed renderer-cache leaves in real WeChat, WeCom, and QQ
    /// sandboxes. Large message, account, mail, document, download, package,
    /// and browser-state roots are hashed before and after cleanup. Markers in
    /// every discovered target prove dry-run behavior and dynamic ownership.
    #[test]
    #[ignore = "permanently clears real WeChat, WeCom, and QQ renderer caches"]
    fn real_tencent_application_caches_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_TENCENT_MACOS_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_TENCENT_MACOS_CACHE=1 to authorize this real cache diagnostic"
        );
        let required_processes = [
            "WeChat",
            "WeCom",
            "WXWork",
            "\u{4f01}\u{4e1a}\u{5fae}\u{4fe1}",
            "QQ",
        ];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in required_processes {
            assert!(
                process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "every Tencent cache owner must be stopped before cleanup"
            );
        }

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let wechat_container = home.join("Library/Containers/com.tencent.xinWeChat/Data");
        let wecom_container = home.join("Library/Containers/com.tencent.WeWorkMac/Data");
        let qq_container = home.join("Library/Containers/com.tencent.qq/Data");
        for root in [&wechat_container, &wecom_container, &qq_container] {
            assert!(root.is_dir(), "every Tencent client must have a sandbox");
        }

        let wechat_web_profiles_root =
            wechat_container.join("Documents/app_data/radium/web/profiles");
        let wechat_cache_profiles_root = wechat_container.join("Library/Caches/profiles");
        let wechat_web_profiles = direct_directory_children(&wechat_web_profiles_root);
        let wechat_cache_profiles = direct_directory_children(&wechat_cache_profiles_root);
        assert!(
            wechat_web_profiles.len() >= 6 && wechat_cache_profiles.len() >= 6,
            "the real WeChat sandbox must expose both profile trees"
        );

        let wecom_cef_root = wecom_container.join("Documents/cefcache");
        // WeCom stores account-scoped CEF profiles under `wew_*`. Filtering
        // siblings prevents global cache directories from entering the durable
        // state digest and keeps this test aligned with the declarative rule.
        let wecom_child_profiles = direct_directory_children(&wecom_cef_root)
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("wew_"))
            })
            .collect::<Vec<_>>();
        assert!(
            !wecom_child_profiles.is_empty(),
            "the real WeCom sandbox must expose CEF profile children"
        );

        let qq_root = qq_container.join("Library/Application Support/QQ");
        let qq_partitions = direct_directory_children(&qq_root.join("Partitions"));
        let qq_account_roots = direct_directory_children(&qq_root)
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("nt_qq_"))
            })
            .collect::<Vec<_>>();
        assert!(
            qq_partitions.len() >= 4 && qq_account_roots.len() >= 2,
            "the real QQ sandbox must expose renderer partitions and account roots"
        );

        let mut preserved_paths = Vec::new();
        for relative in [
            "Documents/xwechat_files",
            "Documents/app_data/users",
            "Documents/app_data/xplugin",
            "Documents/app_data/net",
            "Documents/app_data/login",
            "Documents/app_data/config",
        ] {
            let path = wechat_container.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for profile in &wechat_web_profiles {
            for relative in [
                "History",
                "Cookies",
                "Preferences",
                "Network Persistent State",
                "Local Storage",
                "IndexedDB",
                "Session Storage",
                "WebStorage",
                "Web Data",
                "Share Data",
                "Service Worker/Database",
                "Service Worker/ScriptCache",
            ] {
                let path = profile.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }

        for relative in [
            "Documents/Profiles",
            "Documents/Network",
            "Documents/local_storage_index.db",
            "Documents/GYLog",
            "Library/Application Support/WXWork",
            "Library/Application Support/WeMail",
            "Library/Application Support/Wedoc",
            "Library/Application Support/WXDrive",
            "Library/Application Support/setting.json",
            "Library/WebKit",
        ] {
            let path = wecom_container.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for profile in &wecom_child_profiles {
            for relative in [
                "History",
                "Cookies",
                "Preferences",
                "Secure Preferences",
                "Network Persistent State",
                "Local Storage",
                "IndexedDB",
                "Session Storage",
                "WebStorage",
                "Web Data",
                "Service Worker/Database",
                "Service Worker/ScriptCache",
            ] {
                let path = profile.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }

        for account in &qq_account_roots {
            preserved_paths.push(account.clone());
        }
        for relative in [
            "global",
            "dynamic_package",
            "dynamic_module",
            "arks",
            "Preferences",
            "Network Persistent State",
            "Local Storage",
            "Cookies",
        ] {
            let path = qq_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for partition in &qq_partitions {
            for relative in [
                "Cookies",
                "Preferences",
                "Network Persistent State",
                "Local Storage",
                "IndexedDB",
                "Session Storage",
                "WebStorage",
                "Service Worker/Database",
                "Service Worker/ScriptCache",
            ] {
                let path = partition.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        assert!(
            preserved_paths.len() >= 90,
            "the real sandboxes must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();

        let dynamic_suffixes = [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "Shared Dictionary/cache",
            "Service Worker/CacheStorage",
        ];
        let mut target_roots = Vec::new();
        for profile in wechat_web_profiles.iter().chain(&wechat_cache_profiles) {
            for suffix in dynamic_suffixes {
                let path = profile.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        let wechat_bundle_cache = home.join("Library/Caches/com.tencent.xinWeChat");
        if wechat_bundle_cache.is_dir() {
            target_roots.push(wechat_bundle_cache);
        }

        for relative in [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "ShaderCache",
            "Shared Dictionary/cache",
            "Service Worker/CacheStorage",
            "component_crx_cache",
        ] {
            let path = wecom_cef_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        for profile in &wecom_child_profiles {
            for suffix in dynamic_suffixes {
                let path = profile.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        let wecom_bundle_cache = home.join("Library/Caches/com.tencent.WeWorkMac");
        if wecom_bundle_cache.is_dir() {
            target_roots.push(wecom_bundle_cache);
        }

        for relative in [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "Shared Dictionary/cache",
        ] {
            let path = qq_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        for partition in &qq_partitions {
            for suffix in dynamic_suffixes {
                let path = partition.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        let qq_bundle_cache = home.join("Library/Caches/com.tencent.qq");
        if qq_bundle_cache.is_dir() {
            target_roots.push(qq_bundle_cache);
        }

        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 55,
            "the real sandboxes must expose verified Tencent cache roots"
        );
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the Tencent cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.wechat-cache".to_string(),
                "app.wecom-cache".to_string(),
                "app.qq-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Tencent cache dry run must succeed");
        assert_eq!(preview.actions.len(), 3);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Tencent cache cleanup must succeed");
        assert_eq!(result.actions.len(), 3);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_tencent_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} wechat_profile_count={} wecom_child_count={} qq_partition_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            wechat_web_profiles.len() + wechat_cache_profiles.len(),
            wecom_child_profiles.len(),
            qq_partitions.len(),
            preserved_paths.len()
        );
    }

    /// Sends production cleanup requests while the signed applications are
    /// running. This proves that process preflight blocks both rules before
    /// any notes, offline content, or persistent browser state is traversed.
    #[test]
    #[ignore = "requires the real YNote and QQLive applications to be running"]
    fn real_ynote_qqlive_caches_block_running_owners() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_YNOTE_QQLIVE_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_YNOTE_QQLIVE_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let cases = [
            (
                "app.ynote-cache",
                "\u{6709}\u{9053}\u{4e91}\u{7b14}\u{8bb0}",
            ),
            ("app.qqlive-rendering-cache", "QQLive"),
        ];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");

        for (rule_id, process_name) in cases {
            assert!(
                !process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "both real cache owners must be running"
            );
            let result = CleanupService::execute(CleanupRequest {
                rule_ids: vec![rule_id.to_string()],
                source_selections: Vec::new(),
                dry_run: false,
                project_roots: Vec::new(),
            })
            .expect("the blocked cleanup must return a structured result");
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
        }
        println!(
            "real_macos_ynote_qqlive_cache_block owner_count={}",
            cases.len()
        );
    }

    /// Runs dry-run and real cleanup against initialized YNote and QQLive
    /// profiles. Account and partition roots are discovered without logging
    /// private identifiers. Hashes prove that notes, backups, offline packs,
    /// runtime components, settings, and browser persistence remain unchanged.
    #[test]
    #[ignore = "permanently clears real YNote and QQLive caches"]
    fn real_ynote_qqlive_caches_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_YNOTE_QQLIVE_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_YNOTE_QQLIVE_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in ["\u{6709}\u{9053}\u{4e91}\u{7b14}\u{8bb0}", "QQLive"] {
            assert!(
                process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "both cache owners must be stopped before cleanup"
            );
        }

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let application_support = home.join("Library/Application Support");
        let ynote_root = application_support.join("ynote-desktop");
        let qqlive_root = application_support.join("com.tencent.mac.marvis");
        assert!(ynote_root.is_dir() && qqlive_root.is_dir());

        // The account directory name is private. Locate it only through the
        // stable ynote-data child and never include its actual name in output.
        let ynote_account_root = direct_directory_children(&ynote_root)
            .into_iter()
            .find(|path| path.join("ynote-data").is_dir())
            .expect("the initialized YNote profile must expose durable note data");
        let ynote_data = ynote_account_root.join("ynote-data");
        let ynote_partitions = direct_directory_children(&ynote_root.join("Partitions"));
        let qqlive_partitions = direct_directory_children(&qqlive_root.join("Partitions"));
        assert!(
            !ynote_partitions.is_empty() && qqlive_partitions.len() >= 2,
            "both initialized applications must expose renderer partitions"
        );

        let mut preserved_paths = vec![ynote_data];
        for relative in [
            "setting.json",
            "browser-settings.json",
            "Cookies",
            "Preferences",
            "Local Storage",
            "IndexedDB",
            "databases",
            "storage",
            "Session Storage",
            "Network Persistent State",
        ] {
            let path = ynote_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for partition in &ynote_partitions {
            for relative in [
                "Cookies",
                "Preferences",
                "Local Storage",
                "IndexedDB",
                "Session Storage",
                "Network Persistent State",
            ] {
                let path = partition.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }

        for relative in [
            "components",
            "OfflinePack",
            "Knowledgebase",
            "services",
            "MarvisData",
            "Cookies",
            "Preferences",
            "Local Storage",
            "IndexedDB",
            "Session Storage",
            "WebStorage",
            "marvis-login-state.json",
            "marvis-settings.json",
            "installed.json",
        ] {
            let path = qqlive_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for partition in &qqlive_partitions {
            for relative in [
                "Cookies",
                "Preferences",
                "Local Storage",
                "Session Storage",
                "Network Persistent State",
                "blob_storage",
            ] {
                let path = partition.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        assert!(
            preserved_paths.len() >= 25,
            "the real profiles must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();

        let cache_suffixes = [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "Shared Dictionary/cache",
        ];
        let mut target_roots = Vec::new();
        for relative in [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "Shared Dictionary/cache",
            "Crashpad/reports",
            "myLogs",
        ] {
            let path = ynote_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        let ynote_updater = application_support.join("Caches/ynote-desktop-updater");
        if ynote_updater.is_dir() {
            target_roots.push(ynote_updater);
        }
        for partition in &ynote_partitions {
            for suffix in cache_suffixes {
                let path = partition.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }

        for relative in [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "Shared Dictionary/cache",
            "Crashpad/reports",
            "icon_cache",
        ] {
            let path = qqlive_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        for path in [
            home.join("Library/Caches/com.tencent.tenvideo"),
            application_support.join("Caches/com.tencent.tenvideo"),
        ] {
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        for partition in &qqlive_partitions {
            for suffix in cache_suffixes {
                let path = partition.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 20,
            "the real profiles must expose verified cache leaves"
        );

        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.ynote-cache".to_string(),
                "app.qqlive-rendering-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview =
            CleanupService::execute(request(true)).expect("the real cache dry run must succeed");
        assert_eq!(preview.actions.len(), 2);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result =
            CleanupService::execute(request(false)).expect("the real cache cleanup must succeed");
        assert_eq!(result.actions.len(), 2);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_ynote_qqlive_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} ynote_partition_count={} qqlive_partition_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            ynote_partitions.len(),
            qqlive_partitions.len(),
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Proves that the production process snapshot recognizes the executable
    /// from the notarized FlashVoice bundle before its WebKit cache is touched.
    #[test]
    #[ignore = "requires the real FlashVoice application to be running"]
    fn real_flashvoice_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_FLASHVOICE_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_FLASHVOICE_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        assert!(
            !process_snapshot
                .matching_processes(&["FlashVoice".to_string()])
                .is_empty(),
            "the real FlashVoice application must be running"
        );

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.flashvoice-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked cleanup must return a structured result");
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
        println!("real_macos_flashvoice_cache_block owner_count=1");
    }

    /// Clears the real FlashVoice WebKit cache and diagnostics only after the
    /// application stops. Full hashes cover downloaded speech models, audio
    /// recordings, transcription indexes, configuration, and window state.
    #[test]
    #[ignore = "permanently clears real FlashVoice caches"]
    fn real_flashvoice_cache_preserves_models_and_recordings() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_FLASHVOICE_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_FLASHVOICE_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        assert!(
            process_snapshot
                .matching_processes(&["FlashVoice".to_string()])
                .is_empty(),
            "FlashVoice must be stopped before cleanup"
        );

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let support = home.join("Library/Application Support/com.flashvoices");
        let cache = home.join("Library/Caches/FlashVoice");
        assert!(support.is_dir() && cache.is_dir());

        let mut preserved_paths = Vec::new();
        for relative in [
            "models",
            "recordings",
            "transcriptions",
            "fv_config.json",
            "fv_onboarding.json",
            "fv_recordings.json",
            "fv_transcriptions.json",
            "installation.json",
            ".persisted-scope",
            ".persisted-scope-asset",
            ".window-state.json",
        ] {
            let path = support.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        assert!(
            preserved_paths.len() >= 10,
            "the initialized application must expose durable voice state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();

        let logs = support.join("logs");
        assert!(logs.is_dir());
        let target_roots = [cache, logs];
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.flashvoice-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real FlashVoice cache dry run must succeed");
        assert_eq!(preview.actions.len(), 1);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real FlashVoice cache cleanup must succeed");
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_flashvoice_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Verifies that every newly covered macOS client is recognized by the
    /// production process snapshot before any mixed-purpose data root is read.
    #[test]
    #[ignore = "requires uTools, Clash Verge, and Youdao Dictionary to be running"]
    fn real_utools_clash_youdao_caches_block_running_owners() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_UTILITY_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_UTILITY_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in [
            "uTools",
            "clash-verge",
            "\u{7f51}\u{6613}\u{6709}\u{9053}\u{7ffb}\u{8bd1}",
        ] {
            assert!(
                !process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "the real {process_name} application must be running"
            );
        }

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "app.utools-rendering-cache".to_string(),
                "app.clash-verge-diagnostic-cache".to_string(),
                "app.youdao-translation-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked cleanup must return structured results");
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
        println!("real_macos_utility_cache_block application_count=3");
    }

    /// Clears only renderer caches, diagnostics, and the application sandbox
    /// cache directory. Digests deliberately cover clipboard history, plugins,
    /// databases, proxy profiles and configuration, dictionaries, preferences,
    /// documents, and durable browser storage before and after production cleanup.
    #[test]
    #[ignore = "permanently clears real uTools, Clash Verge, and Youdao caches"]
    fn real_utools_clash_youdao_caches_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_UTILITY_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_UTILITY_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in [
            "uTools",
            "clash-verge",
            "\u{7f51}\u{6613}\u{6709}\u{9053}\u{7ffb}\u{8bd1}",
        ] {
            assert!(
                process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "{process_name} must be stopped before cleanup"
            );
        }

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let application_support = home.join("Library/Application Support");
        let utools_root = application_support.join("uTools");
        let clash_root = application_support.join("io.github.clash-verge-rev.clash-verge-rev");
        let youdao_data = home.join("Library/Containers/com.youdao.YoudaoDict/Data");
        let youdao_library = youdao_data.join("Library");
        let youdao_cache = youdao_library.join("Caches");
        assert!(utools_root.is_dir() && clash_root.is_dir() && youdao_cache.is_dir());

        let mut preserved_paths = vec![
            (utools_root.join("clipboard-data"), Vec::new()),
            (utools_root.join("plugins"), Vec::new()),
            (utools_root.join("database"), Vec::new()),
            (utools_root.join("Local Storage"), Vec::new()),
            (clash_root.clone(), vec!["logs"]),
            (youdao_library, vec!["Caches"]),
        ];
        let youdao_documents = youdao_data.join("Documents");
        if youdao_documents.exists() {
            preserved_paths.push((youdao_documents, Vec::new()));
        }

        let utools_partitions = direct_directory_children(&utools_root.join("Partitions"));
        for partition in &utools_partitions {
            for relative in [
                "Local Storage",
                "IndexedDB",
                "Session Storage",
                "WebStorage",
                "Network",
                "Cookies",
                "Preferences",
                "History",
                "Service Worker/Database",
                "Service Worker/ScriptCache",
            ] {
                let path = partition.join(relative);
                if path.exists() {
                    preserved_paths.push((path, Vec::new()));
                }
            }
        }
        preserved_paths.retain(|(path, _)| path.exists());
        assert!(
            preserved_paths.len() >= 7,
            "the initialized clients must expose durable application state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|(path, excluded)| {
                digest_macos_tree_with_exclusions_without_following_links(path, excluded)
            })
            .collect::<Vec<_>>();

        let cache_suffixes = [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "Shared Dictionary/cache",
            "Service Worker/CacheStorage",
        ];
        let mut target_roots = Vec::new();
        for relative in [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "Shared Dictionary/cache",
            "Service Worker/CacheStorage",
            "logs",
            "Crashpad/reports",
        ] {
            let path = utools_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        for partition in &utools_partitions {
            for suffix in cache_suffixes {
                let path = partition.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        let clash_logs = clash_root.join("logs");
        if clash_logs.is_dir() {
            target_roots.push(clash_logs);
        }
        target_roots.push(youdao_cache);
        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 8,
            "the initialized clients must expose verified cache roots"
        );

        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.utools-rendering-cache".to_string(),
                "app.clash-verge-diagnostic-cache".to_string(),
                "app.youdao-translation-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview =
            CleanupService::execute(request(true)).expect("the real cache dry run must succeed");
        assert_eq!(preview.actions.len(), 3);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result =
            CleanupService::execute(request(false)).expect("the real cache cleanup must succeed");
        assert_eq!(result.actions.len(), 3);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|(path, excluded)| {
                digest_macos_tree_with_exclusions_without_following_links(path, excluded)
            })
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_utility_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} utools_partition_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            utools_partitions.len(),
            target_roots.len(),
            preserved_paths.len()
        );
    }
