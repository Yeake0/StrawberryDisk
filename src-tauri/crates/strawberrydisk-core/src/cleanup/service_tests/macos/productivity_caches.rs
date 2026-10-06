    /// Confirms that each newly covered client reaches the production process
    /// gate before StrawberryDisk traverses a mixed-purpose application data root.
    /// WorkBuddy's main executable is generically named Electron, so its unique
    /// helper process is used to avoid blocking unrelated Electron clients.
    #[test]
    #[ignore = "requires BaiduNetdisk, Manus, and WorkBuddy to be running"]
    fn real_baidu_manus_workbuddy_caches_block_running_owners() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_AI_WORKSPACE_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_AI_WORKSPACE_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in ["BaiduNetdisk", "Manus", "WorkBuddy Helper"] {
            assert!(
                !process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "the real {process_name} process must be running"
            );
        }

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "app.baidu-netdisk-rendering-cache".to_string(),
                "app.manus-rendering-cache".to_string(),
                "app.workbuddy-rendering-cache".to_string(),
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
        println!("real_macos_ai_workspace_cache_block application_count=3");
    }
    /// Clears only fixed Electron renderer caches after all three clients stop.
    /// Opaque-link digests protect Baidu sync/account state, Manus browser and
    /// task state, and WorkBuddy tasks, projects, databases, connectors, and
    /// partition storage without following sandbox or singleton links.
    #[test]
    #[ignore = "permanently clears real BaiduNetdisk, Manus, and WorkBuddy caches"]
    fn real_baidu_manus_workbuddy_caches_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_AI_WORKSPACE_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_AI_WORKSPACE_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in [
            "BaiduNetdisk",
            "Manus",
            "WorkBuddy Helper",
            "WorkBuddy Helper (GPU)",
            "WorkBuddy Helper (Renderer)",
            "WorkBuddyRepair",
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
        let baidu_data = home.join("Library/Containers/com.baidu.netdisk/Data");
        let baidu_support = baidu_data.join("Library/Application Support");
        let baidu_renderer = baidu_support.join("baidunetdisk");
        let manus_root = home.join("Library/Application Support/Manus");
        let workbuddy_root = home.join(".workbuddy");
        let workbuddy_session = workbuddy_root.join("app/session");
        assert!(baidu_renderer.is_dir() && manus_root.is_dir() && workbuddy_session.is_dir());

        let mut preserved_paths = vec![
            baidu_support.join("com.baidu.netdisk"),
            home.join("Library/Group Containers/group.com.baidu.BaiduNetdisk-Mac"),
            home.join("Library/Group Containers/group.com.baidu.netdisk"),
            home.join("Library/Group Containers/LKD5676Y5W.com.baidu.netdisk"),
        ];
        for relative in [
            "Preferences",
            "Cookies",
            "IndexedDB",
            "Local Storage",
            "Session Storage",
            "Storage",
            "WebStorage",
            "databases",
        ] {
            preserved_paths.push(baidu_renderer.join(relative));
        }
        for relative in [
            "Preferences",
            "Cookies",
            "IndexedDB",
            "Local Storage",
            "Session Storage",
            "WebStorage",
            "Network Persistent State",
            "localStorage.json",
            "window-state.json",
            ".updaterId",
        ] {
            preserved_paths.push(manus_root.join(relative));
        }
        for relative in [
            "workbuddy.db",
            "settings.json",
            "user-state.json",
            "workspace-state.json",
            ".mcp.json",
            "projects",
            "tasks",
            "automation-backups",
            "local_storage",
            "connectors",
            "memory",
        ] {
            preserved_paths.push(workbuddy_root.join(relative));
        }
        for relative in [
            "Preferences",
            "Cookies",
            "DIPS",
            "IndexedDB",
            "Local Storage",
            "Session Storage",
            "WebStorage",
        ] {
            preserved_paths.push(workbuddy_session.join(relative));
        }
        let workbuddy_partitions = direct_directory_children(&workbuddy_session.join("Partitions"));
        for partition in &workbuddy_partitions {
            for relative in [
                "Preferences",
                "Cookies",
                "DIPS",
                "IndexedDB",
                "Local Storage",
                "Session Storage",
                "WebStorage",
            ] {
                preserved_paths.push(partition.join(relative));
            }
        }
        preserved_paths.retain(|path| path.exists());
        preserved_paths.sort();
        preserved_paths.dedup();
        assert!(
            preserved_paths.len() >= 30,
            "the initialized clients must expose durable account, task, project, and browser state"
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
            "Service Worker/CacheStorage",
        ];
        let mut target_roots = Vec::new();
        for root in [&baidu_renderer, &manus_root, &workbuddy_session] {
            for suffix in cache_suffixes {
                let path = root.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        for partition in &workbuddy_partitions {
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
            target_roots.len() >= 25,
            "the initialized clients must expose verified fixed renderer-cache roots"
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
                "app.baidu-netdisk-rendering-cache".to_string(),
                "app.manus-rendering-cache".to_string(),
                "app.workbuddy-rendering-cache".to_string(),
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
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_ai_workspace_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} workbuddy_partition_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            workbuddy_partitions.len(),
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Confirms that update and renderer cleanup never starts while the two
    /// owning desktop clients can still mutate their cache directories.
    #[test]
    #[ignore = "requires the real Manus and Claude applications to be running"]
    fn real_manus_update_and_claude_cache_block_running_owners() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_MANUS_CLAUDE_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_MANUS_CLAUDE_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in ["Manus", "Claude"] {
            assert!(
                !process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "the real {process_name} process must be running"
            );
        }

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec![
                "app.manus-update-cache".to_string(),
                "app.claude-cache".to_string(),
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
        println!("real_macos_manus_claude_cache_block application_count=2");
    }

    /// Executes the production rules against a stale downloaded Manus update
    /// and Claude's fixed Electron cache leaves. The digest set covers all
    /// Manus application state and Claude's sessions, credentials, local-agent
    /// work, configuration, and partition storage without following links.
    #[test]
    #[ignore = "permanently clears real Manus update and Claude renderer caches"]
    fn real_manus_update_and_claude_cache_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_MANUS_CLAUDE_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_MANUS_CLAUDE_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in ["Manus", "Claude"] {
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
        let user_caches = home.join("Library/Caches");
        let manus_root = application_support.join("Manus");
        let claude_root = application_support.join("Claude");
        assert!(manus_root.is_dir() && claude_root.is_dir());

        let mut preserved_paths = vec![(manus_root, Vec::new())];
        for candidate in [
            home.join("Library/Preferences/im.manus.desktop.plist"),
            home.join("Library/Preferences/com.anthropic.claudefordesktop.plist"),
            claude_root.join("Preferences"),
            claude_root.join("Cookies"),
            claude_root.join("IndexedDB"),
            claude_root.join("Local Storage"),
            claude_root.join("Session Storage"),
            claude_root.join("WebStorage"),
            claude_root.join("Network Persistent State"),
            claude_root.join("local-agent-mode-sessions"),
            claude_root.join("claude_desktop_config.json"),
        ] {
            preserved_paths.push((candidate, Vec::new()));
        }
        let shared_dictionary = claude_root.join("Shared Dictionary");
        if shared_dictionary.exists() {
            preserved_paths.push((shared_dictionary, vec!["cache"]));
        }

        let claude_partitions = direct_directory_children(&claude_root.join("Partitions"));
        for partition in &claude_partitions {
            for relative in [
                "Preferences",
                "Cookies",
                "DIPS",
                "IndexedDB",
                "Local Storage",
                "Session Storage",
                "WebStorage",
            ] {
                let candidate = partition.join(relative);
                if candidate.exists() {
                    preserved_paths.push((candidate, Vec::new()));
                }
            }
            let partition_dictionary = partition.join("Shared Dictionary");
            if partition_dictionary.exists() {
                preserved_paths.push((partition_dictionary, vec!["cache"]));
            }
        }
        preserved_paths.retain(|(path, _)| path.exists());
        assert!(
            preserved_paths.len() >= 12,
            "the initialized clients must expose durable application and browser state"
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
            "Crashpad/reports",
        ];
        let mut target_roots = Vec::new();
        for candidate in [
            user_caches.join("manus-updater"),
            user_caches.join("im.manus.desktop"),
            user_caches.join("com.anthropic.claudefordesktop"),
        ] {
            if candidate.is_dir() {
                target_roots.push(candidate);
            }
        }
        for cache_suffix in cache_suffixes {
            let candidate = claude_root.join(cache_suffix);
            if candidate.is_dir() {
                target_roots.push(candidate);
            }
        }
        for partition in &claude_partitions {
            for cache_suffix in cache_suffixes {
                let candidate = partition.join(cache_suffix);
                if candidate.is_dir() {
                    target_roots.push(candidate);
                }
            }
        }
        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 14,
            "the initialized clients must expose downloaded update and renderer-cache roots"
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
                "app.manus-update-cache".to_string(),
                "app.claude-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Manus and Claude cache dry run must succeed");
        assert_eq!(preview.actions.len(), 2);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Manus and Claude cache cleanup must succeed");
        assert_eq!(result.actions.len(), 2);
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
            "real_macos_manus_claude_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} claude_partition_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            claude_partitions.len(),
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Confirms that the production rule reaches the process gate before it
    /// can traverse Tencent Meeting's cache. The client keeps authenticated
    /// account and meeting state in a separate sandbox container, but it can
    /// still mutate the selected WebKit cache while running.
    #[test]
    #[ignore = "requires the real Tencent Meeting application to be running"]
    fn real_tencent_meeting_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_TENCENT_MEETING_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_TENCENT_MEETING_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&["TencentMeeting".to_string()]);
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
            "real_macos_tencent_meeting_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Clears only WebKit's fixed NetworkCache. Separate digests cover the
    /// HSTS, CacheStorage, and transport databases plus representative account,
    /// meeting, cookie, browser-storage, preference, and download state inside
    /// the sandbox container.
    #[test]
    #[ignore = "permanently clears the real Tencent Meeting WebKit network cache"]
    fn real_tencent_meeting_cache_preserves_account_and_meeting_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_TENCENT_MEETING_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_TENCENT_MEETING_CACHE=1 to authorize this real cache diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&["TencentMeeting".to_string()]);
        assert!(
            running.is_empty(),
            "Tencent Meeting must be completely stopped before cleanup"
        );

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let webkit_cache = home.join("Library/Caches/com.tencent.meeting/WebKit");
        let target_root = webkit_cache.join("NetworkCache");
        let container_library = home.join("Library/Containers/com.tencent.meeting/Data/Library");
        let preserved_paths = [
            webkit_cache.join("AlternativeServices"),
            webkit_cache.join("CacheStorage"),
            webkit_cache.join("HSTS"),
            container_library.join("Preferences"),
            container_library.join("Cookies"),
            container_library.join("WebKit/WebsiteData"),
            container_library.join("Users"),
            container_library.join("Global/Database"),
            container_library.join("Global/Preferences"),
            container_library.join("Global/Data/DynamicResource"),
            container_library.join("Global/Data/DynamicResourcePackage"),
            home.join("Library/Containers/com.tencent.meeting/Data/Documents"),
        ];
        assert!(target_root.is_dir());
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the real profile must expose representative account, meeting, and browser state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();

        let marker = target_root.join("strawberrydisk-rule-validation.bin");
        fs::write(&marker, b"payload").expect("the Tencent Meeting cache marker must be created");
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.tencent-meeting-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Tencent Meeting cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 7);
        assert!(marker.exists());

        let result = CleanupService::execute(request(false))
            .expect("the real Tencent Meeting cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 7);
        assert!(!marker.exists());
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_tencent_meeting_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count=1 preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }

    /// Confirms that Tencent Lemon's foreground client and persistent status
    /// monitor block all three cache containers before traversal. The system
    /// daemon is intentionally absent: live-handle inspection confirms that it
    /// does not own files below these per-user cache roots.
    #[test]
    #[ignore = "requires the real Tencent Lemon application to be running"]
    fn real_tencent_lemon_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_TENCENT_LEMON_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_TENCENT_LEMON_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_names = ["Tencent Lemon", "LemonMonitor", "LemonUpdate"].map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&process_names);
        assert!(
            running.len() >= 2,
            "Tencent Lemon and its status monitor must be running"
        );

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.tencent-lemon-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Tencent Lemon cleanup must return a structured result");
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
            "real_macos_tencent_lemon_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Clears only Tencent Lemon's three standard cache containers. Opaque
    /// digests protect cleanup history, scan databases, preferences, monitor
    /// state, HTTP storage, logs, launch configuration, and sandbox data.
    #[test]
    #[ignore = "permanently clears real Tencent Lemon caches"]
    fn real_tencent_lemon_cache_preserves_history_and_monitor_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_TENCENT_LEMON_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_TENCENT_LEMON_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_names = ["Tencent Lemon", "LemonMonitor", "LemonUpdate"].map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&process_names);
        assert!(
            running.is_empty(),
            "Tencent Lemon and its monitor must be completely stopped before cleanup"
        );

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let library = home.join("Library");
        let target_roots = [
            library.join("Caches/com.tencent.Lemon"),
            library.join("Caches/com.tencent.LemonMonitor"),
            library.join("Caches/com.tencent.LemonUpdate"),
        ];
        assert!(
            target_roots.iter().all(|root| root.is_dir()),
            "the initialized Lemon components must expose all three cache roots"
        );
        let preserved_candidates = [
            library.join("Application Support/com.tencent.Lemon"),
            library.join("Application Support/com.tencent.LemonMonitor"),
            library.join("Containers/com.tencent.LemonLite"),
            library.join("Application Scripts/com.tencent.LemonLite"),
            library.join("HTTPStorages/com.tencent.Lemon"),
            library.join("HTTPStorages/com.tencent.LemonMonitor"),
            library.join("HTTPStorages/com.tencent.LemonUpdate"),
            library.join("Preferences/com.tencent.Lemon.plist"),
            library.join("Preferences/com.tencent.LemonMonitor.plist"),
            library.join("Preferences/com.tencent.LemonUpdate.plist"),
            library.join("Logs/Tencent Lemon.log"),
            library.join("LaunchAgents/com.tencent.Lemon.trash.plist"),
        ];
        let preserved_paths = preserved_candidates
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        assert!(
            preserved_paths.len() >= 10,
            "the initialized client must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();

        let markers = target_roots
            .iter()
            .enumerate()
            .map(|(index, root)| root.join(format!("strawberrydisk-rule-validation-{index}.bin")))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the Tencent Lemon marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.tencent-lemon-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Tencent Lemon cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Tencent Lemon cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_tencent_lemon_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            target_roots.len(),
            preserved_paths.len()
        );
    }

    /// Confirms that Thunder and its download helper cannot mutate the cache
    /// while the production cleanup path is measuring or deleting it.
    #[test]
    #[ignore = "requires the real Thunder application to be running"]
    fn real_thunder_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_THUNDER_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_THUNDER_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&["Thunder".to_string(), "DownloadService".to_string()]);
        assert!(!running.is_empty(), "Thunder or its helper must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.thunder-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Thunder cleanup must return a structured result");
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
            "real_macos_thunder_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Clears Thunder's dedicated macOS cache container while opaque digests
    /// protect download tasks, cloud-drive databases, accounts, uploads,
    /// preferences, HTTP storage, and WebKit website state outside that root.
    #[test]
    #[ignore = "permanently clears the real Thunder application cache"]
    fn real_thunder_cache_preserves_download_and_account_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_THUNDER_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_THUNDER_CACHE=1 to authorize this real cache diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the macOS process inventory must be available")
            .matching_processes(&["Thunder".to_string(), "DownloadService".to_string()]);
        assert!(
            running.is_empty(),
            "Thunder and its download helper must be completely stopped before cleanup"
        );

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let target_root = home.join("Library/Caches/com.xunlei.Thunder");
        let preserved_candidates = [
            home.join("Library/Application Support/com.xunlei.Thunder"),
            home.join("Library/Application Support/Thunder"),
            home.join("Library/WebKit/com.xunlei.Thunder"),
            home.join("Library/HTTPStorages/com.xunlei.Thunder"),
            home.join("Library/Preferences/com.xunlei.Thunder.plist"),
            home.join("Library/Containers/com.xunlei.Thunder.Thunder-Extension"),
        ];
        let preserved_paths = preserved_candidates
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        assert!(target_root.is_dir());
        assert!(
            preserved_paths.len() >= 5,
            "the initialized client must expose representative durable state outside its cache"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();

        let marker = target_root.join("strawberrydisk-rule-validation.bin");
        fs::write(&marker, b"payload").expect("the Thunder cache marker must be created");
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.thunder-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Thunder cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 7);
        assert!(marker.exists());

        let result = CleanupService::execute(request(false))
            .expect("the real Thunder cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 7);
        assert!(!marker.exists());
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_thunder_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} target_root_count=1 preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }
