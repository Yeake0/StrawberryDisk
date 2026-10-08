    /// Proves that every newly verified macOS application rule reaches the
    /// production process gate before a destructive traversal starts. This is
    /// intentionally a real-profile diagnostic: synthetic process fixtures
    /// cannot validate executable-name matching for signed application bundles.
    #[test]
    #[ignore = "requires the real macOS cache-owner applications to be running"]
    fn real_application_cache_rules_block_running_owners() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_APP_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_APP_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let cases = [
            ("app.vlc-cache", "VLC"),
            ("app.postman-cache", "Postman"),
            ("app.discord-cache", "Discord"),
            ("app.telegram-temporary-cache", "Telegram"),
            ("app.slack-cache", "Slack"),
            ("app.lobsterai-update-cache", "LobsterAI"),
            ("dev.qoder-rendering-cache", "Qoder"),
            ("app.qwenwork-cache", "QwenWorkCN"),
        ];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");

        for (rule_id, process_name) in cases {
            assert!(
                !process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "the real process-gate diagnostic requires every cache owner to be running"
            );
            // Use real mode deliberately. A successful assertion proves the
            // process gate, rather than dry-run semantics, prevented mutation.
            let result = CleanupService::execute(CleanupRequest {
                rule_ids: vec![rule_id.to_string()],
                source_selections: Vec::new(),
                dry_run: false,
                project_roots: Vec::new(),
            })
            .expect("the blocked cleanup request must return a structured result");
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
            "real_macos_application_cache_block owner_count={}",
            cases.len()
        );
    }
    /// Clears the four real cache families only after their owners are stopped.
    /// Representative account and application state is hashed before and after
    /// cleanup. Markers live inside already verified cache boundaries, making
    /// the assertion independent of cache contents that vary between launches.
    #[test]
    #[ignore = "permanently clears real VLC, Postman, Discord, and Telegram caches"]
    fn real_application_cache_rules_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_APP_CACHE_CLEANUP").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_APP_CACHE_CLEANUP=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in ["VLC", "Postman", "Discord", "Telegram"] {
            assert!(
                process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "every cache owner must be stopped before the real cleanup diagnostic"
            );
        }

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let application_support = home.join("Library/Application Support");
        let user_caches = home.join("Library/Caches");
        let postman_root = application_support.join("Postman");
        let postman_partitions = postman_root.join("Partitions");
        let discord_root = application_support.join("discord");
        let telegram_tdata = application_support.join("Telegram Desktop/tdata");
        assert!(postman_partitions.is_dir());
        assert!(discord_root.is_dir());
        assert!(telegram_tdata.is_dir());

        let mut partitions = fs::read_dir(&postman_partitions)
            .expect("the Postman partitions root must be readable")
            .map(|entry| {
                entry
                    .expect("the Postman partition must be readable")
                    .path()
            })
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        partitions.sort();
        assert!(
            partitions.len() >= 2,
            "the real Postman profile must expose representative partitions"
        );

        // Only known durable roots are hashed. Cache leaves are deliberately
        // absent from this list because rebuilding them is the expected result.
        let mut preserved_paths = Vec::new();
        for relative in [
            "storage",
            "Local Storage",
            "Network",
            "IndexedDB",
            "Session Storage",
            "WebStorage",
            "Preferences",
            "Local State",
            "databases",
        ] {
            let path = postman_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for partition in &partitions {
            for relative in [
                "Storage",
                "IndexedDB",
                "Local Storage",
                "Network",
                "Session Storage",
                "WebStorage",
                "Preferences",
            ] {
                let path = partition.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        for relative in [
            "Local Storage",
            "Network",
            "IndexedDB",
            "Service Worker",
            "Session Storage",
            "WebStorage",
            "Preferences",
            "Local State",
            "settings.json",
            "shared_proto_db",
        ] {
            let path = discord_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for path in [
            application_support.join("org.videolan.vlc"),
            home.join("Library/Preferences/org.videolan.vlc"),
        ] {
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        assert!(
            preserved_paths.len() >= 12,
            "the real profiles must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();
        // Telegram's hashed tdata entries are account and message state. Hash
        // the complete tree except the two explicitly selected cache roots.
        let telegram_before = digest_macos_tree(&telegram_tdata, &["temp", "dumps"]);

        let mut markers = vec![
            postman_root.join("Cache/strawberrydisk-rule-validation.bin"),
            discord_root.join("Cache/strawberrydisk-rule-validation.bin"),
            user_caches.join("com.hnc.Discord/strawberrydisk-rule-validation.bin"),
            telegram_tdata.join("dumps/strawberrydisk-rule-validation.dmp"),
            user_caches.join("org.videolan.vlc/strawberrydisk-rule-validation.bin"),
        ];
        for partition in &partitions {
            markers.push(partition.join("Cache/strawberrydisk-rule-validation.bin"));
        }
        for marker in &markers {
            fs::create_dir_all(
                marker
                    .parent()
                    .expect("the cache marker must have a parent"),
            )
            .expect("the verified cache root must be writable");
            fs::write(marker, b"payload").expect("the isolated cache marker must be written");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.vlc-cache".to_string(),
                "app.postman-cache".to_string(),
                "app.discord-cache".to_string(),
                "app.telegram-temporary-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real application cache preview must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real application cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));

        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        assert_eq!(
            digest_macos_tree(&telegram_tdata, &["temp", "dumps"]),
            telegram_before
        );
        println!(
            "real_macos_application_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} partition_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            partitions.len(),
            preserved_paths.len() + 1
        );
    }

    /// Validates the next macOS expansion wave against signed, initialized
    /// applications. The target roots contain large rebuildable web and update
    /// payloads beside account, project, skill, editor, and agent state, so the
    /// diagnostic hashes representative durable paths around production cleanup.
    #[test]
    #[ignore = "permanently clears real Slack, LobsterAI, Qoder, and QwenWork caches"]
    fn real_next_wave_application_caches_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_NEXT_WAVE_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_NEXT_WAVE_CACHE=1 to authorize this real cache diagnostic"
        );
        let required_processes = [
            "Slack",
            "LobsterAI",
            "Qoder",
            "Qoder Helper",
            "QoderCN",
            "Qoder CN Helper",
            "QwenWorkCN",
        ];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in required_processes {
            assert!(
                process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "every next-wave cache owner must be stopped before cleanup"
            );
        }

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let application_support = home.join("Library/Application Support");
        let slack_root = application_support.join("Slack");
        let lobster_root = application_support.join("LobsterAI");
        let qoder_root = application_support.join("Qoder");
        let qoder_cn_root = application_support.join("QoderCN");
        let qwen_root = application_support.join("QwenWorkCN");
        for root in [
            &slack_root,
            &lobster_root,
            &qoder_root,
            &qoder_cn_root,
            &qwen_root,
        ] {
            assert!(root.is_dir(), "every real application profile must exist");
        }

        let mut preserved_paths = Vec::new();
        for path in [
            slack_root.join("Cookies"),
            slack_root.join("IndexedDB"),
            slack_root.join("Local Storage"),
            slack_root.join("Session Storage"),
            slack_root.join("WebStorage"),
            slack_root.join("Preferences"),
            slack_root.join("installation"),
            slack_root.join("Service Worker/Database"),
            slack_root.join("Service Worker/ScriptCache"),
            lobster_root.join("lobsterai.sqlite"),
            lobster_root.join("Preferences"),
            lobster_root.join("Cookies"),
            lobster_root.join("Local Storage"),
            lobster_root.join("Session Storage"),
            lobster_root.join("SKILLs/skills.config.json"),
            lobster_root.join("openclaw/state/openclaw.json"),
            qwen_root.join("auth.dat"),
            qwen_root.join("auth-v2.dat"),
            qwen_root.join("Preferences"),
            qwen_root.join("Local Storage"),
            qwen_root.join("Session Storage"),
            qwen_root.join("rum-electron-store"),
            qwen_root.join("data/agents.db"),
            qwen_root.join("data/agents.db-shm"),
            qwen_root.join("data/agents.db-wal"),
        ] {
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for root in [&qoder_root, &qoder_cn_root] {
            for relative in [
                "User",
                "Backups",
                "Local Storage",
                "Session Storage",
                "WebStorage",
                "Cookies",
                "Preferences",
                "Local State",
                "SharedClientCache",
                "CachedProfilesData",
                "CachedConfigurations",
            ] {
                let path = root.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        let mut qoder_partitions = direct_directory_children(&qoder_cn_root.join("Partitions"));
        let mut qwen_partitions = direct_directory_children(&qwen_root.join("Partitions"));
        for partition in qoder_partitions.iter().chain(&qwen_partitions) {
            for relative in [
                "Local Storage",
                "Session Storage",
                "WebStorage",
                "Cookies",
                "Preferences",
                "Network Persistent State",
            ] {
                let path = partition.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        assert!(
            preserved_paths.len() >= 30,
            "the real profiles must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();

        let mut markers = vec![
            slack_root.join("Service Worker/CacheStorage/strawberrydisk-rule-validation.bin"),
            lobster_root.join("updates/lobsterai-update-auto-strawberrydisk-validation.dmg"),
            qoder_root.join("Cache/strawberrydisk-rule-validation.bin"),
            qoder_cn_root.join("Cache/strawberrydisk-rule-validation.bin"),
            qwen_root.join("Cache/strawberrydisk-rule-validation.bin"),
        ];
        qoder_partitions.sort();
        qwen_partitions.sort();
        for partition in qoder_partitions.iter().chain(&qwen_partitions) {
            markers.push(partition.join("Cache/strawberrydisk-rule-validation.bin"));
        }
        for marker in &markers {
            fs::create_dir_all(
                marker
                    .parent()
                    .expect("the next-wave marker must have a parent"),
            )
            .expect("the verified next-wave cache root must be writable");
            fs::write(marker, b"payload").expect("the isolated cache marker must be written");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.slack-cache".to_string(),
                "app.lobsterai-update-cache".to_string(),
                "dev.qoder-rendering-cache".to_string(),
                "app.qwenwork-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the next-wave real cache preview must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the next-wave real cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_macos_next_wave_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} qoder_partition_count={} qwen_partition_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            qoder_partitions.len(),
            qwen_partitions.len(),
            preserved_paths.len()
        );
    }

    /// Proves that the third-wave rules reach the process gate with real,
    /// signed applications.
    ///
    /// The diagnostic deliberately submits a real cleanup request instead of a
    /// dry run. Only a `RunningProcesses` result proves that production stopped
    /// before traversal or mutation; static process-name configuration alone
    /// would be insufficient evidence for the runtime safety boundary.
    #[test]
    #[ignore = "requires the real ZenAion and Xmind applications to be running"]
    fn real_third_wave_application_caches_block_running_owners() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_THIRD_WAVE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_THIRD_WAVE_BLOCK=1 for the real process-gate diagnostic"
        );
        let cases = [
            ("app.zenaion-cache", "ZenAI"),
            ("app.xmind-rendering-cache", "Xmind"),
        ];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");

        for (rule_id, process_name) in cases {
            assert!(
                !process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "every third-wave cache owner must be running"
            );
            let result = CleanupService::execute(CleanupRequest {
                rule_ids: vec![rule_id.to_string()],
                source_selections: Vec::new(),
                dry_run: false,
                project_roots: Vec::new(),
            })
            .expect("the blocked cleanup request must return a structured result");
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
            "real_macos_third_wave_cache_block owner_count={}",
            cases.len()
        );
    }

    /// Executes production cleanup against real ZenAion and Xmind profiles.
    /// Tree digests prove that identity, settings, skills, agent state, document
    /// recovery, and persistent browser data remain unchanged. Markers are
    /// written only inside verified cache leaves, proving dry-run behavior
    /// without depending on nondeterministic cache contents after application
    /// startup.
    #[test]
    #[ignore = "permanently clears real ZenAion and Xmind caches"]
    fn real_third_wave_application_caches_preserve_durable_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_THIRD_WAVE_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_MACOS_THIRD_WAVE_CACHE=1 to authorize this real cache diagnostic"
        );
        let required_processes = ["ZenAI", "zenai-host", "Xmind", "Xmind Helper"];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in required_processes {
            assert!(
                process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "every third-wave cache owner must be stopped before cleanup"
            );
        }

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let application_support = home.join("Library/Application Support");
        let zen_root = application_support.join("bot.zenai");
        let xmind_root = application_support.join("Xmind/Electron v3");
        assert!(
            zen_root.is_dir(),
            "ZenAion must have completed a first launch"
        );
        assert!(
            xmind_root.is_dir(),
            "Xmind must have completed a first launch"
        );

        // Both digests cover every direct sibling outside the target caches.
        // Xmind's complete Crashpad directory is excluded because the rule owns
        // only its reports child. Crashpad has no document or account state;
        // every other persistent sibling remains part of the digest.
        let zen_before = digest_macos_tree(&zen_root, &[".caches", "logs"]);
        let xmind_before = digest_macos_tree(
            &xmind_root,
            &[
                "Cache",
                "Code Cache",
                "GPUCache",
                "DawnCache",
                "DawnGraphiteCache",
                "DawnWebGPUCache",
                "GrShaderCache",
                "GraphiteDawnCache",
                "Shared Dictionary",
                "Crashpad",
            ],
        );

        let markers = [
            zen_root.join(".caches/strawberrydisk-rule-validation.json"),
            zen_root.join("logs/strawberrydisk-rule-validation.log"),
            xmind_root.join("Cache/strawberrydisk-rule-validation.bin"),
            xmind_root.join("GPUCache/strawberrydisk-rule-validation.bin"),
        ];
        for marker in &markers {
            fs::create_dir_all(
                marker
                    .parent()
                    .expect("the third-wave marker must have a parent"),
            )
            .expect("the verified third-wave cache root must be writable");
            fs::write(marker, b"payload").expect("the isolated cache marker must be written");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![
                "app.zenaion-cache".to_string(),
                "app.xmind-rendering-cache".to_string(),
            ],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the third-wave real cache preview must succeed");
        assert_eq!(preview.actions.len(), 2);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the third-wave real cache cleanup must succeed");
        assert_eq!(result.actions.len(), 2);
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| !marker.exists()));
        assert_eq!(
            digest_macos_tree(&zen_root, &[".caches", "logs"]),
            zen_before
        );
        assert_eq!(
            digest_macos_tree(
                &xmind_root,
                &[
                    "Cache",
                    "Code Cache",
                    "GPUCache",
                    "DawnCache",
                    "DawnGraphiteCache",
                    "DawnWebGPUCache",
                    "GrShaderCache",
                    "GraphiteDawnCache",
                    "Shared Dictionary",
                    "Crashpad",
                ],
            ),
            xmind_before
        );
        println!(
            "real_macos_third_wave_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_snapshot_count=2",
            preview.expected_bytes, result.released_bytes, result.affected_item_count
        );
    }

    /// Exercises the production process gate against the signed DingTalk
    /// client before any account-scoped cache discovery can start. A real-mode
    /// blocked result is required because dry-run alone cannot prove that the
    /// destructive path stops before traversing dynamic account directories.
    #[test]
    #[ignore = "requires the real DingTalk application to be running"]
    fn real_dingtalk_content_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_DINGTALK_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_DINGTALK_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        assert!(
            !process_snapshot
                .matching_processes(&["DingTalk".to_string(), "DingTalkMac".to_string()])
                .is_empty(),
            "DingTalk must be running"
        );

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.dingtalk-content-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked DingTalk cleanup must return a structured result");
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
            "real_macos_dingtalk_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Validates DingTalk's application-classified cache against a real logged-in
    /// profile. Large chat databases and resource files are intentionally hashed
    /// even though their names sit beside cache leaves; this proves that dynamic
    /// account expansion cannot drift into messages, downloads, or account state.
    #[test]
    #[ignore = "permanently clears real DingTalk content caches"]
    fn real_dingtalk_content_cache_preserves_chat_and_account_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_DINGTALK_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_DINGTALK_CACHE=1 to authorize this real cache diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        assert!(
            process_snapshot
                .matching_processes(&["DingTalk".to_string(), "DingTalkMac".to_string()])
                .is_empty(),
            "DingTalk must be completely stopped before cleanup"
        );

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let application_root = home.join("Library/Application Support/DingTalkMac");
        let cache_root = home.join("Library/Caches/com.alibaba.DingTalkMac");
        assert!(
            application_root.is_dir(),
            "DingTalk must have completed a first launch"
        );
        let account_roots = direct_directory_children(&application_root)
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("_v2"))
            })
            .collect::<Vec<_>>();
        assert!(
            !account_roots.is_empty(),
            "the logged-in DingTalk profile must expose an account root"
        );

        let mut preserved_paths = Vec::new();
        for relative in ["globalStorage", "config", "wukong", "emotions"] {
            let path = application_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for account in &account_roots {
            for relative in [
                "DBFiles",
                "NativeIM",
                "UserStorage",
                "CommonStorage",
                "dtnest_db",
                "resource_cache",
                "SafetyFiles",
                "SyncPoint",
            ] {
                let path = account.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        assert!(
            preserved_paths.len() >= 10,
            "the real profile must expose representative chat and account state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_macos_tree(path, &[]))
            .collect::<Vec<_>>();

        let mut target_roots = Vec::new();
        for account in &account_roots {
            for relative in [
                "EAppFiles",
                "ImageFiles",
                "GifEmotionFiles",
                "wave_cards",
                "theme_cache",
                "Sync_v2/cache",
            ] {
                let path = account.join(relative);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        for relative in [
            "WebKit/NetworkCache",
            "WebKit/CacheStorage",
            "thumbnails",
            "fsCachedData",
        ] {
            let path = cache_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        assert!(
            target_roots.len() >= 8,
            "the real profile must expose the verified DingTalk cache roots"
        );
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the DingTalk cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.dingtalk-content-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real DingTalk cache dry run must succeed");
        assert_eq!(preview.actions.len(), 1);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real DingTalk content cache cleanup must succeed");
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
            "real_macos_dingtalk_content_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} account_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            account_roots.len(),
            preserved_paths.len()
        );
    }

    /// Exercises the production gate against the real signed Lark client.
    /// The application owns several multi-gigabyte account-scoped Chromium
    /// caches, so proving the destructive request stops before dynamic profile
    /// discovery is part of the rule's safety evidence.
    #[test]
    #[ignore = "requires the real Lark application to be running"]
    fn real_lark_renderer_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_LARK_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_LARK_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        assert!(
            !process_snapshot
                .matching_processes(&[
                    "Lark".to_string(),
                    "Feishu".to_string(),
                    "LarkShell".to_string(),
                    "Lark Helper".to_string(),
                    "Lark Helper (GPU)".to_string(),
                    "Lark Helper (Renderer)".to_string(),
                ])
                .is_empty(),
            "Lark must be running"
        );

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.lark-renderer-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Lark cleanup must return a structured result");
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
            "real_macos_lark_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Clears only fixed rendering-cache leaves in real AHA and Iron profiles.
    /// Lark stores cookies, history, workspace state, downloads, IndexedDB, and
    /// Service Worker registrations beside those leaves. Their digests must be
    /// byte-identical after production cleanup, while isolated markers prove
    /// dry-run and dynamic-profile selection behavior.
    #[test]
    #[ignore = "permanently clears real Lark renderer caches"]
    fn real_lark_renderer_cache_preserves_account_and_workspace_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_LARK_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_LARK_CACHE=1 to authorize this real cache diagnostic"
        );
        let required_processes = [
            "Lark",
            "Feishu",
            "LarkShell",
            "Lark Helper",
            "Lark Helper (GPU)",
            "Lark Helper (Renderer)",
        ];
        let process_snapshot =
            ProcessSnapshot::capture().expect("the macOS process inventory must be available");
        for process_name in required_processes {
            assert!(
                process_snapshot
                    .matching_processes(&[process_name.to_string()])
                    .is_empty(),
                "every Lark cache owner must be stopped before cleanup"
            );
        }

        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME must be available");
        let application_root = home.join("Library/Application Support/LarkShell");
        let cache_root = home.join("Library/Caches/LarkShell");
        assert!(
            application_root.is_dir(),
            "Lark must have completed a first launch"
        );

        let mut user_roots = Vec::new();
        let mut application_profiles = Vec::new();
        for area in ["aha", "iron"] {
            let users_root = application_root.join(area).join("users");
            for user_root in direct_directory_children(&users_root) {
                for profile in direct_directory_children(&user_root) {
                    if profile
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("profile_"))
                    {
                        application_profiles.push(profile);
                    }
                }
                user_roots.push(user_root);
            }
        }
        assert!(
            application_profiles.len() >= 6,
            "the logged-in Lark profile must expose AHA and Iron profiles"
        );

        let mut preserved_paths = Vec::new();
        for relative in [
            "update",
            "PC_Gadget",
            "sdk_storage",
            "meego",
            "passport_storage",
            "persistent_storage.db",
            "persistent_storage.enc.db",
            "persistent_storage.preload.db",
            "iron/Local Storage",
            "iron/IndexedDB",
            "iron/Session Storage",
            "iron/WebStorage",
            "iron/Local State",
        ] {
            let path = application_root.join(relative);
            if path.exists() {
                preserved_paths.push(path);
            }
        }
        for user_root in &user_roots {
            for relative in [
                "morpheus",
                "Partitions",
                "PartitionsV2",
                "fgs",
                "AllDownloadHistory",
            ] {
                let path = user_root.join(relative);
                if path.exists() {
                    preserved_paths.push(path);
                }
            }
        }
        for profile in &application_profiles {
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
                "Network",
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
        assert!(
            preserved_paths.len() >= 60,
            "the real profiles must expose representative account and workspace state"
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
        for profile in &application_profiles {
            for suffix in dynamic_suffixes {
                let path = profile.join(suffix);
                if path.is_dir() {
                    target_roots.push(path);
                }
            }
        }
        let cache_users_root = cache_root.join("aha/users");
        for user_root in direct_directory_children(&cache_users_root) {
            for profile in direct_directory_children(&user_root) {
                if !profile
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("profile_"))
                {
                    continue;
                }
                for suffix in dynamic_suffixes {
                    let path = profile.join(suffix);
                    if path.is_dir() {
                        target_roots.push(path);
                    }
                }
            }
        }
        for relative in [
            "ShaderCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "CodeCache",
            "component_crx_cache",
            "iron/Cache",
            "iron/Code Cache",
            "iron/GPUCache",
            "iron/DawnCache",
            "iron/DawnGraphiteCache",
            "iron/DawnWebGPUCache",
            "iron/GrShaderCache",
            "iron/GraphiteDawnCache",
            "iron/Shared Dictionary/cache",
            "iron/Service Worker/CacheStorage",
        ] {
            let path = application_root.join(relative);
            if path.is_dir() {
                target_roots.push(path);
            }
        }
        target_roots.sort();
        target_roots.dedup();
        assert!(
            target_roots.len() >= 30,
            "the real profiles must expose verified Lark cache roots"
        );
        let markers = target_roots
            .iter()
            .map(|root| root.join("strawberrydisk-rule-validation.bin"))
            .collect::<Vec<_>>();
        for marker in &markers {
            fs::write(marker, b"payload").expect("the Lark cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.lark-renderer-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Lark cache dry run must succeed");
        assert_eq!(preview.actions.len(), 1);
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= markers.len() as u64 * 7);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Lark renderer cache cleanup must succeed");
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
            "real_macos_lark_renderer_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} user_count={} profile_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            user_roots.len(),
            application_profiles.len(),
            preserved_paths.len()
        );
    }
