    /// Submits production cleanup while the signed ZenAion process is running.
    /// The assertion proves that the process gate blocks the WebView2 rule
    /// before scanning or deletion and never reports released bytes.
    #[test]
    #[ignore = "requires the real ZenAion application to be running"]
    fn real_zenaion_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_ZENAION_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_ZENAION_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["ZenAI.exe".to_string(), "zenai-host.exe".to_string()]);
        assert!(
            !running.is_empty(),
            "ZenAion or its agent host must be running"
        );

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.zenaion-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked ZenAion cleanup must return a structured result");
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
            "real_windows_zenaion_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }
    /// Clears fixed cache leaves from a real ZenAion WebView2 profile and hashes
    /// account, cookie, history, browser-setting, and site-storage state. This
    /// proves that the rule never treats the complete WebView2 user data folder
    /// as disposable while exercising both dry-run and production execution.
    #[test]
    #[ignore = "clears real ZenAion WebView2 caches in an isolated Windows VM"]
    fn real_zenaion_cache_preserves_account_and_browser_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_ZENAION_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_ZENAION_CACHE=1 only in an isolated Windows VM"
        );
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let zen_root = local_app_data.join("bot.zenai");
        let webview_root = zen_root.join("EBWebView");
        let default_root = webview_root.join("Default");
        assert!(
            default_root.is_dir(),
            "ZenAion must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["ZenAI.exe".to_string(), "zenai-host.exe".to_string()]);
        assert!(
            running.is_empty(),
            "ZenAion and its agent host must be stopped before cleanup"
        );

        let preserved_paths = [
            zen_root.join(".cookies"),
            webview_root.join("Local State"),
            default_root.join("History"),
            default_root.join("Preferences"),
            default_root.join("Web Data"),
            default_root.join("Local Storage"),
            default_root.join("Network"),
            default_root.join("IndexedDB"),
        ]
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
        assert!(
            preserved_paths.len() >= 6,
            "the real profile must expose representative durable browser state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let target_roots = [
            default_root.join("Cache"),
            default_root.join("Code Cache"),
            default_root.join("GPUCache"),
            webview_root.join("GraphiteDawnCache"),
        ];
        assert!(
            target_roots.iter().all(|path| path.is_dir()),
            "the real profile must expose the verified WebView2 cache roots"
        );
        let markers = target_roots.map(|root| root.join("strawberrydisk-rule-validation.bin"));
        for marker in &markers {
            fs::write(marker, b"payload").expect("the ZenAion cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.zenaion-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real ZenAion cache dry run must succeed");
        assert_eq!(preview.actions.len(), 1);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real ZenAion cache cleanup must succeed");
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_windows_zenaion_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }

    /// Sends a real cleanup request while the signed Weixin client owns its
    /// renderer profiles. The blocked result proves that dynamic profile
    /// expansion never begins while Chromium files can still be open.
    #[test]
    #[ignore = "requires the real Weixin application to be running"]
    fn real_wechat_rendering_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_WECHAT_RENDERING_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_WECHAT_RENDERING_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Weixin.exe".to_string(), "WeChat.exe".to_string()]);
        assert!(!running.is_empty(), "Weixin or WeChat must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.wechat-rendering-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Weixin cleanup must return a structured result");
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
            "real_windows_wechat_rendering_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Clears fixed renderer-cache and mini-program compiled-code leaves below
    /// real Weixin profiles. Per-profile browser state and every user-root file
    /// except the exact `codecache` segment are hashed around production cleanup,
    /// proving that applet data, history, cookies, storage, service workers,
    /// messages, and downloaded files stay outside the rule.
    #[test]
    #[ignore = "clears real Weixin renderer caches in an isolated Windows VM"]
    fn real_wechat_rendering_cache_preserves_account_and_profile_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_WECHAT_RENDERING_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_WECHAT_RENDERING_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let tencent_root = roaming_app_data.join("Tencent");
        let current_root = tencent_root.join("xwechat/radium");
        let profiles_root = current_root.join("web/profiles");
        let users_root = current_root.join("users");
        let legacy_radium_root = tencent_root.join("WeChat/radium");
        let legacy_wmpf_cache = legacy_radium_root.join("WmpfCache");
        assert!(
            profiles_root.is_dir(),
            "Weixin must have completed a first launch"
        );
        assert!(
            users_root.is_dir(),
            "the real account state root must exist"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Weixin.exe".to_string(), "WeChat.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Weixin and WeChat must be stopped before cleanup"
        );

        let profile_roots = fs::read_dir(&profiles_root)
            .expect("the renderer profile root must remain readable")
            .map(|entry| entry.expect("the profile entry must be readable").path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        assert!(
            profile_roots.len() >= 7,
            "the initialized client must expose the observed renderer profiles"
        );

        let preserved_user_state_before =
            digest_tree_excluding_segments(&users_root, &["codecache"]);
        let legacy_applet = legacy_radium_root.join("Applet");
        let preserved_legacy_applet_before =
            legacy_applet.is_dir().then(|| digest_tree(&legacy_applet));
        let mut preserved_paths = [
            tencent_root.join("xwechat/login"),
            tencent_root.join("xwechat/config"),
            tencent_root.join("xwechat/All Users/config"),
            tencent_root.join("WeChat/All Users/config"),
        ]
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
        for profile in &profile_roots {
            for relative in [
                "History",
                "History_encrypted",
                "History.wxbak",
                "Preferences",
                "Local Storage",
                "Network",
                "IndexedDB",
                "Session Storage",
                "WebStorage",
                "Service Worker",
            ] {
                let path = profile.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        assert!(
            preserved_paths.len() >= 30,
            "the real profiles must expose representative persistent state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let mut markers = Vec::new();
        for profile in &profile_roots {
            let cache = profile.join("Cache");
            if cache.is_dir() {
                let marker = cache.join("strawberrydisk-rule-validation.bin");
                fs::write(&marker, b"payload")
                    .expect("the Weixin profile cache marker must be created");
                markers.push(marker);
            }
        }
        let renderer_marker_count = markers.len();
        let user_roots = fs::read_dir(&users_root)
            .expect("the Weixin account root must remain readable")
            .map(|entry| {
                entry
                    .expect("the Weixin account entry must be readable")
                    .path()
            })
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        let applet_code_caches = user_roots
            .iter()
            .map(|user| user.join("applet/codecache"))
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        assert!(
            !applet_code_caches.is_empty(),
            "the initialized account must expose a mini-program code cache"
        );
        for cache in &applet_code_caches {
            let marker = cache.join("strawberrydisk-rule-validation.bin");
            fs::write(&marker, b"payload")
                .expect("the Weixin applet code-cache marker must be created");
            markers.push(marker);
        }
        assert!(
            legacy_wmpf_cache.is_dir(),
            "the migrated profile must expose the legacy WMPF cache"
        );
        let legacy_marker = legacy_wmpf_cache.join("strawberrydisk-rule-validation.bin");
        fs::write(&legacy_marker, b"payload")
            .expect("the legacy WMPF cache marker must be created");
        markers.push(legacy_marker);
        assert_eq!(
            renderer_marker_count,
            profile_roots.len(),
            "every observed renderer profile must expose a Cache root"
        );
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.wechat-rendering-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Weixin cache dry run must succeed");
        assert_eq!(preview.actions.len(), 1);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Weixin rendering cache cleanup must succeed");
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        assert_eq!(
            digest_tree_excluding_segments(&users_root, &["codecache"]),
            preserved_user_state_before
        );
        assert_eq!(
            legacy_applet.is_dir().then(|| digest_tree(&legacy_applet)),
            preserved_legacy_applet_before
        );
        println!(
            "real_windows_wechat_rendering_cleanup expected_bytes={} released_bytes={} affected_item_count={} profile_count={} applet_code_cache_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            profile_roots.len(),
            applet_code_caches.len(),
            preserved_paths.len()
        );
    }

    /// Sends production cleanup requests while the real Windows clients are
    /// running. Both executable names must trigger process preflight, and each
    /// blocked request must report zero deleted bytes and items.
    #[test]
    #[ignore = "requires the real QQ and WeCom applications to be running"]
    fn real_qq_wecom_caches_block_running_owners() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_QQ_WECOM_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_QQ_WECOM_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let cases = [
            ("app.qq-rendering-cache", "QQ.exe"),
            ("app.wecom-diagnostic-cache", "WXWork.exe"),
        ];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the Windows process inventory must be available");

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
            "real_windows_qq_wecom_cache_block owner_count={}",
            cases.len()
        );
    }

    /// Runs dry-run and real cleanup against QQ and WeCom in the isolated VM.
    /// Cache markers must be deleted while hashes prove that account/message
    /// roots, browser persistence, dictionary databases, and Crashpad metadata
    /// remain unchanged.
    #[test]
    #[ignore = "clears real QQ and WeCom caches in an isolated Windows VM"]
    fn real_qq_wecom_caches_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_QQ_WECOM_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_QQ_WECOM_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the Windows process inventory must be available");
        for process_name in ["QQ.exe", "WXWork.exe"] {
            assert!(
                process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "both cache owners must be stopped before cleanup"
            );
        }

        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let qq_root = roaming.join("QQ");
        let wecom_root = roaming.join("Tencent/WXWork");
        assert!(qq_root.is_dir() && wecom_root.is_dir());

        let mut preserved_paths = Vec::new();
        for relative in [
            "blob_storage",
            "Dictionaries",
            "Local Storage",
            "Network",
            "Shared Dictionary/db",
            "Shared Dictionary/db-journal",
            "Crashpad/attachments",
            "Crashpad/metadata",
            "Crashpad/settings.dat",
        ] {
            let path = qq_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for relative in ["Data", "Applet"] {
            let path = wecom_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        assert!(
            preserved_paths.len() >= 8,
            "the initialized clients must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

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
            "log",
            "Crashpad/reports",
        ] {
            let path = qq_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        let wecom_log = wecom_root.join("Log");
        if wecom_log.is_dir() {
            target_roots.push(wecom_log);
        }
        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 6,
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
                "app.qq-rendering-cache".to_string(),
                "app.wecom-diagnostic-cache".to_string(),
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
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_windows_qq_wecom_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Proves that the signed Windows executable reaches process preflight
    /// before any WebView2 profile or diagnostic root is traversed.
    #[test]
    #[ignore = "requires the real FlashVoice application to be running"]
    fn real_flashvoice_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_FLASHVOICE_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_FLASHVOICE_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the Windows process inventory must be available");
        assert!(
            !process_snapshot
                .matching_processes(&["FlashVoice.exe".to_string()])
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
        println!("real_windows_flashvoice_cache_block owner_count=1");
    }

    /// Clears fixed WebView2 cache leaves and diagnostics in the isolated VM.
    /// Full hashes cover voice models, recordings, transcription indexes,
    /// settings, browser history, storage, sessions, and credentials.
    #[test]
    #[ignore = "clears real FlashVoice caches in an isolated Windows VM"]
    fn real_flashvoice_cache_preserves_models_and_recordings() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_FLASHVOICE_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_FLASHVOICE_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the Windows process inventory must be available");
        assert!(
            process_snapshot
                .matching_processes(&["FlashVoice.exe".to_string()])
                .is_empty(),
            "FlashVoice must be stopped before cleanup"
        );

        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let support = roaming.join("com.flashvoices");
        let webview = local.join("com.flashvoices/EBWebView");
        assert!(support.is_dir() && webview.is_dir());

        let mut preserved_paths = Vec::new();
        for relative in [
            "models",
            "recordings",
            "transcriptions",
            "config.json",
            "fv_config.json",
            "fv_onboarding.json",
            "fv_recordings.json",
            "installation.json",
            "onboarding.json",
            "recordings.json",
            "transcriptions.json",
        ] {
            let path = support.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for relative in [
            "Local State",
            "Last Version",
            "Default/Preferences",
            "Default/Secure Preferences",
            "Default/History",
            "Default/Login Data",
            "Default/Login Data For Account",
            "Default/Local Storage",
            "Default/IndexedDB",
            "Default/Network",
            "Default/Session Storage",
            "Default/WebStorage",
            "Default/Extension Cookies",
            "Default/Web Data",
        ] {
            let path = webview.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        assert!(
            preserved_paths.len() >= 20,
            "the initialized application must expose durable voice and browser state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let mut target_roots = Vec::new();
        for relative in [
            "component_crx_cache",
            "extensions_crx_cache",
            "GPUPersistentCache",
            "GraphiteDawnCache",
            "GrShaderCache",
            "ShaderCache",
            "Crashpad/reports",
            "Default/Cache",
            "Default/Code Cache",
            "Default/GPUCache",
            "Default/DawnCache",
            "Default/DawnGraphiteCache",
            "Default/DawnWebGPUCache",
            "Default/GrShaderCache",
            "Default/GraphiteDawnCache",
            "Default/Shared Dictionary/cache",
            "Default/Service Worker/CacheStorage",
        ] {
            let path = webview.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        let logs = support.join("logs");
        if logs.is_dir() {
            target_roots.push(logs);
        }
        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 12,
            "the real WebView2 profile must expose verified cache leaves"
        );

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
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_windows_flashvoice_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Confirms that the Microsoft Store GitMind process blocks both renderer
    /// cache cleanup and shared updater-staging cleanup before traversal.
    #[test]
    #[ignore = "requires the real GitMind application to be running"]
    fn real_gitmind_caches_block_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_GITMIND_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_GITMIND_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the Windows process inventory must be available");
        assert!(
            !process_snapshot
                .matching_processes(&["GitMind.exe".to_string()])
                .is_empty(),
            "the real GitMind application must be running"
        );

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "app.gitmind-rendering-cache".to_string(),
                "app.electron-updater-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked cleanup must return structured results");
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
        println!("real_windows_gitmind_cache_block owner_count=1");
    }

    /// Clears fixed GitMind renderer leaves and three dedicated updater roots.
    /// Full hashes cover installed binaries, product configuration, mind-map
    /// state, credentials, history, cookies, network data, and browser storage.
    #[test]
    #[ignore = "clears real GitMind rendering and Electron updater caches in an isolated Windows VM"]
    fn real_gitmind_and_electron_updater_caches_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_GITMIND_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_GITMIND_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the Windows process inventory must be available");
        assert!(
            process_snapshot
                .matching_processes(&["GitMind.exe".to_string()])
                .is_empty(),
            "GitMind must be stopped before cleanup"
        );

        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let program_files_x86 = std::env::var_os("ProgramFiles(x86)")
            .map(PathBuf::from)
            .expect("ProgramFiles(x86) must be available");
        let gitmind_root = roaming.join("GitMind");
        let webview = gitmind_root.join("webview/EBWebView");
        assert!(gitmind_root.is_dir() && webview.is_dir());

        let mut preserved_paths = Vec::new();
        for relative in [
            "Local Storage",
            "Session Storage",
            "IndexedDB",
            "shared_proto_db",
            "blob_storage",
            "Dictionaries",
            "GitMind",
            "Service Worker/Database",
            "Service Worker/ScriptCache",
        ] {
            let path = gitmind_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for relative in [
            "Local State",
            "Last Version",
            "Default/Preferences",
            "Default/Secure Preferences",
            "Default/History",
            "Default/Login Data",
            "Default/Login Data For Account",
            "Default/Local Storage",
            "Default/IndexedDB",
            "Default/Network",
            "Default/Session Storage",
            "Default/WebStorage",
            "Default/Extension Cookies",
            "Default/Web Data",
            "Default/Service Worker/Database",
            "Default/Service Worker/ScriptCache",
        ] {
            let path = webview.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for path in [
            roaming.join("com.wangxutech.gitmind.desktop"),
            roaming.join("weflow"),
            program_files_x86.join("Apowersoft/GitMind/GitMind.exe"),
        ] {
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        assert!(
            preserved_paths.len() >= 18,
            "the initialized application must expose durable product and browser state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let mut target_roots = Vec::new();
        for relative in [
            "Code Cache",
            "GPUCache",
            "Shared Dictionary/cache",
            "Service Worker/CacheStorage",
            "logs",
            "Crashpad/reports",
        ] {
            let path = gitmind_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        for relative in [
            "component_crx_cache",
            "extensions_crx_cache",
            "GPUPersistentCache",
            "GraphiteDawnCache",
            "GrShaderCache",
            "ShaderCache",
            "Crashpad/reports",
            "Default/Cache",
            "Default/Code Cache",
            "Default/GPUCache",
            "Default/DawnCache",
            "Default/DawnGraphiteCache",
            "Default/DawnWebGPUCache",
            "Default/GrShaderCache",
            "Default/GraphiteDawnCache",
            "Default/Shared Dictionary/cache",
            "Default/Service Worker/CacheStorage",
        ] {
            let path = webview.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        let updater_roots = [
            local.join("gowhisper-updater"),
            local.join("weflow-updater"),
            local.join("gitmind-updater"),
        ];
        assert!(updater_roots.iter().all(|root| root.is_dir()));
        target_roots.extend(updater_roots.iter().cloned());
        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 17,
            "GitMind and the updater fixtures must expose verified cache roots"
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
                "app.gitmind-rendering-cache".to_string(),
                "app.electron-updater-cache".to_string(),
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
        assert!(updater_roots.iter().all(|root| !root.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_windows_gitmind_updater_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }
