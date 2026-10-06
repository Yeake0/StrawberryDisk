    /// Battle.net keeps a large launcher-content cache beside CachedData.db and
    /// embeds Chromium profiles below BrowserCaches. The production rule owns
    /// the dedicated content root and renderer leaves only. Hashing the launcher
    /// database, account configuration, and browser storage prevents a broad
    /// third-party pattern from silently turning into account-state deletion.
    #[test]
    #[ignore = "clears real Battle.net caches in an isolated Windows VM"]
    fn real_battlenet_cache_preserves_launcher_and_browser_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_BATTLENET_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_BATTLENET_CACHE=1 only in an isolated Windows VM"
        );
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let battle_net_root = local_app_data.join("Battle.net");
        let browser_profile = battle_net_root.join("BrowserCaches/common");
        let content_cache = battle_net_root.join("Cache");
        assert!(
            content_cache.is_dir() && browser_profile.is_dir(),
            "Battle.net must have completed a first interactive launch"
        );
        let required_processes = ["Battle.net.exe", "Agent.exe"].map(str::to_string);
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&required_processes);
        assert!(
            running.is_empty(),
            "Battle.net and Agent must be stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            battle_net_root.join("CachedData.db"),
            roaming_app_data.join("Battle.net/Battle.net.config"),
            browser_profile.join("Local Storage"),
            browser_profile.join("Network"),
            browser_profile.join("Session Storage"),
        ];
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the real Battle.net profile must expose representative non-cache state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let browser_cache = browser_profile.join("Cache");
        fs::create_dir_all(&browser_cache)
            .expect("the Battle.net browser cache root must be writable");
        let content_marker = content_cache.join("strawberrydisk-rule-validation.bin");
        let browser_marker = browser_cache.join("strawberrydisk-rule-validation.bin");
        fs::write(&content_marker, b"payload")
            .expect("the Battle.net content marker must be created");
        fs::write(&browser_marker, b"payload")
            .expect("the Battle.net browser marker must be created");
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.battlenet-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Battle.net cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 14);
        assert!(content_marker.exists() && browser_marker.exists());

        let result = CleanupService::execute(request(false))
            .expect("the real Battle.net cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 14);
        assert!(!content_marker.exists() && !browser_marker.exists());
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_battlenet_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }
    /// VS Code's user-data root contains both large generated caches and the
    /// user's durable editor state. This test runs the production rule against
    /// the signed installed client and hashes representative settings, backup,
    /// workspace, authentication-adjacent, and storage roots. CachedData and
    /// CachedExtensionVSIXs are included because the matching VS Code source tag
    /// identifies them as generated code and bounded extension-download caches.
    #[test]
    #[ignore = "clears real VS Code caches in an isolated Windows VM"]
    fn real_vscode_cache_preserves_editor_and_workspace_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_VSCODE_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_VSCODE_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let code_root = roaming_app_data.join("Code");
        assert!(
            code_root.join("Cache").is_dir() && code_root.join("CachedData").is_dir(),
            "VS Code must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Code.exe".to_string()]);
        assert!(
            running.is_empty(),
            "VS Code must be completely stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            code_root.join("User"),
            code_root.join("Backups"),
            code_root.join("Local Storage"),
            code_root.join("Network"),
            code_root.join("Session Storage"),
            code_root.join("WebStorage"),
            code_root.join("CachedConfigurations"),
            code_root.join("CachedProfilesData"),
        ];
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the real VS Code profile must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let target_roots = [
            code_root.join("Cache"),
            code_root.join("CachedData"),
            code_root.join("CachedExtensionVSIXs"),
            code_root.join("Crashpad/reports"),
        ];
        assert!(
            target_roots.iter().all(|path| path.is_dir()),
            "the real VS Code profile must expose the source-proven target roots"
        );
        let markers = target_roots.map(|root| root.join("strawberrydisk-rule-validation.bin"));
        for marker in &markers {
            fs::write(marker, b"payload").expect("the VS Code cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["dev.editor-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real VS Code cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 28);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real VS Code cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 28);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_vscode_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }

    /// Proves that the dedicated Postman rule owns only the Postman process
    /// boundary. The shared Electron rule used to couple this cleanup to every
    /// supported Electron application, so this real-process assertion guards
    /// both safe early blocking and the narrower application-specific design.
    #[test]
    #[ignore = "requires the real Postman application to be running"]
    fn real_postman_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_POSTMAN_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_POSTMAN_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Postman.exe".to_string()]);
        assert!(!running.is_empty(), "Postman must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.postman-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Postman cleanup must return a structured result");
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
            "real_windows_postman_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// Postman stores each Electron session in a dynamic partition directory.
    /// Its official recovery guidance distinguishes Clear Cache and Reload from
    /// deleting local data, and explicitly identifies Partitions plus storage as
    /// data to carry into a fresh profile. This test therefore hashes every
    /// non-cache state family while exercising only fixed cache suffixes across
    /// all real partitions discovered by the production declarative rule.
    #[test]
    #[ignore = "clears real Postman partition caches in an isolated Windows VM"]
    fn real_postman_partition_cache_preserves_workspace_and_session_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_POSTMAN_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_POSTMAN_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let postman_root = roaming_app_data.join("Postman");
        let partitions_root = postman_root.join("Partitions");
        assert!(
            partitions_root.is_dir(),
            "Postman must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Postman.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Postman must be completely stopped before the real cleanup diagnostic"
        );

        let mut partition_roots = fs::read_dir(&partitions_root)
            .expect("the Postman partitions root must be readable")
            .map(|entry| entry.expect("the partition entry must be readable").path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        partition_roots.sort();
        assert!(
            !partition_roots.is_empty(),
            "the real Postman profile must expose at least one partition"
        );
        let mut preserved_paths = vec![
            postman_root.join("storage"),
            postman_root.join("Local Storage"),
            postman_root.join("Network"),
        ];
        for partition in &partition_roots {
            for relative in [
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
        assert!(
            preserved_paths.len() >= 8,
            "the real Postman profile must expose representative partition state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let markers = partition_roots
            .iter()
            .map(|partition| {
                let cache = partition.join("Cache");
                fs::create_dir_all(&cache).expect("the Postman partition cache must be writable");
                let marker = cache.join("strawberrydisk-rule-validation.bin");
                fs::write(&marker, b"payload")
                    .expect("the Postman partition marker must be created");
                marker
            })
            .collect::<Vec<_>>();
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.postman-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Postman cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= (markers.len() as u64 * 7));
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Postman cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= (markers.len() as u64 * 7));
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_postman_partition_cleanup expected_bytes={} released_bytes={} affected_item_count={} partition_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            partition_roots.len(),
            preserved_paths.len()
        );
    }

    /// Spotify documents cache and offline downloads as separate data types
    /// and allows users to move the cache. The real client keeps two Chromium
    /// profiles, media Storage, and account state beside each other, so a broad
    /// LocalAppData rule would be unsafe. This diagnostic writes markers only
    /// to fixed renderer leaves and hashes login, history, network, media,
    /// Local State, and installation preferences before and after cleanup.
    #[test]
    #[ignore = "clears real Spotify rendering caches in an isolated Windows VM"]
    fn real_spotify_rendering_cache_preserves_account_and_offline_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_SPOTIFY_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_SPOTIFY_CACHE=1 only in an isolated Windows VM"
        );
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA must be available");
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let spotify_root = local_app_data.join("Spotify");
        let browser_profile = spotify_root.join("Browser");
        let default_profile = spotify_root.join("Default");
        assert!(
            browser_profile.is_dir() && default_profile.is_dir(),
            "Spotify must have completed a first interactive launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Spotify.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Spotify must be completely stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            spotify_root.join("Local State"),
            spotify_root.join("Storage"),
            browser_profile.join("History"),
            browser_profile.join("Login Data"),
            browser_profile.join("Preferences"),
            browser_profile.join("Local Storage"),
            browser_profile.join("Network"),
            browser_profile.join("Session Storage"),
            default_profile.join("History"),
            default_profile.join("Login Data"),
            default_profile.join("Preferences"),
            default_profile.join("Local Storage"),
            default_profile.join("Network"),
            roaming_app_data.join("Spotify/prefs"),
        ];
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the real Spotify profile must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let target_roots = [
            browser_profile.join("Cache"),
            browser_profile.join("GPUCache"),
            default_profile.join("Cache"),
            spotify_root.join("ShaderCache"),
        ];
        assert!(
            target_roots.iter().all(|path| path.is_dir()),
            "the real Spotify profile must expose the verified renderer caches"
        );
        let markers = target_roots.map(|root| root.join("strawberrydisk-rule-validation.bin"));
        for marker in &markers {
            fs::write(marker, b"payload").expect("the Spotify cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.spotify-rendering-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Spotify cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 28);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Spotify cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 28);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_spotify_rendering_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }

    /// Dropbox mixes renderer caches and account state in both its Roaming
    /// AppData root and dynamic partitions. This test enumerates the real
    /// partitions, writes markers only to fixed Cache leaves, and hashes each
    /// partition's IndexedDB, local and network storage, sessions, WebStorage,
    /// and preferences so the UI rule cannot reach sign-in or synced content.
    #[test]
    #[ignore = "clears real Dropbox rendering caches in an isolated Windows VM"]
    fn real_dropbox_rendering_cache_preserves_account_and_sync_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_DROPBOX_RENDERING_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_DROPBOX_RENDERING_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let dropbox_root = roaming_app_data.join("Dropbox");
        let partitions_root = dropbox_root.join("Partitions");
        assert!(
            partitions_root.is_dir(),
            "Dropbox must be signed in and expose its partition root"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Dropbox.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Dropbox must be completely stopped before the real cleanup diagnostic"
        );

        let mut partition_roots = fs::read_dir(&partitions_root)
            .expect("the Dropbox partitions root must be readable")
            .map(|entry| entry.expect("the partition entry must be readable").path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        partition_roots.sort();
        assert!(
            partition_roots.len() >= 2,
            "the signed-in Dropbox profile must expose representative partitions"
        );
        let mut preserved_paths = vec![
            dropbox_root.join("Local State"),
            dropbox_root.join("Preferences"),
            dropbox_root.join("Local Storage"),
            dropbox_root.join("Network"),
            dropbox_root.join("SharedStorage"),
        ];
        for partition in &partition_roots {
            for relative in [
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
        assert!(
            preserved_paths.len() >= 12,
            "the Dropbox profile must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let mut markers = vec![dropbox_root.join("Cache/strawberrydisk-rule-validation.bin")];
        for partition in &partition_roots {
            markers.push(partition.join("Cache/strawberrydisk-rule-validation.bin"));
        }
        for marker in &markers {
            let parent = marker
                .parent()
                .expect("the Dropbox marker must have a cache parent");
            fs::create_dir_all(parent).expect("the Dropbox cache root must be writable");
            fs::write(marker, b"payload").expect("the Dropbox cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.dropbox-rendering-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Dropbox rendering cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= (markers.len() as u64 * 7));
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Dropbox rendering cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= (markers.len() as u64 * 7));
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_dropbox_rendering_cleanup expected_bytes={} released_bytes={} affected_item_count={} partition_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            partition_roots.len(),
            preserved_paths.len()
        );
    }

    /// Docker Desktop's dashboard uses an Electron profile, while images,
    /// containers, volumes, build cache, and WSL or Hyper-V disks are separate
    /// high-value boundaries. This test cleans only fixed renderer leaves in
    /// Roaming AppData and hashes dashboard configuration, projects,
    /// notifications, network, and session state around the operation.
    #[test]
    #[ignore = "clears real Docker Desktop rendering caches in an isolated Windows VM"]
    fn real_docker_desktop_rendering_cache_preserves_engine_and_dashboard_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_DOCKER_DESKTOP_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_DOCKER_DESKTOP_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let dashboard_root = roaming_app_data.join("Docker Desktop");
        assert!(
            dashboard_root.is_dir(),
            "Docker Desktop must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Docker Desktop.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Docker Desktop must be stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            dashboard_root.join("Local State"),
            dashboard_root.join("Preferences"),
            dashboard_root.join("Local Storage"),
            dashboard_root.join("Network"),
            dashboard_root.join("Session Storage"),
            dashboard_root.join("SharedStorage"),
            dashboard_root.join("install-state.json"),
            dashboard_root.join("notifications.json"),
            dashboard_root.join("projects.json"),
            dashboard_root.join("window-management.json"),
        ];
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the Docker Desktop profile must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let target_roots = [
            dashboard_root.join("Cache"),
            dashboard_root.join("GPUCache"),
            dashboard_root.join("DawnCache"),
        ];
        assert!(
            target_roots.iter().all(|path| path.is_dir()),
            "the Docker Desktop profile must expose the verified renderer caches"
        );
        let markers = target_roots.map(|root| root.join("strawberrydisk-rule-validation.bin"));
        for marker in &markers {
            fs::write(marker, b"payload").expect("the Docker Desktop cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["container.docker-desktop-rendering-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Docker Desktop cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 21);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Docker Desktop cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 21);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_docker_desktop_rendering_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }

    /// Exercises the dedicated Discord process boundary with the signed client
    /// running. Real mode is intentional: a blocked result proves that cleanup
    /// stops before traversing account, chat, or browser-state directories.
    #[test]
    #[ignore = "requires the real Discord application to be running"]
    fn real_discord_cache_blocks_while_running() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_DISCORD_CACHE_BLOCK").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_DISCORD_CACHE_BLOCK=1 for the real process-gate diagnostic"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Discord.exe".to_string()]);
        assert!(!running.is_empty(), "Discord must be running");

        let result = CleanupService::execute(CleanupRequest {
            rule_ids: vec!["app.discord-cache".to_string()],
            source_selections: Vec::new(),
            dry_run: false,
            project_roots: Vec::new(),
        })
        .expect("the blocked Discord cleanup must return a structured result");
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
            "real_windows_discord_cache_block running_process_count={}",
            action.running_processes.len()
        );
    }

    /// A directory name alone cannot justify an application in the shared
    /// Electron rule. Discord's current profile mixes account network state,
    /// Local Storage, Service Worker data, sessions, and WebStorage. This test
    /// writes markers only to fixed renderer and log leaves and hashes the
    /// representative durable state around the production cleanup.
    #[test]
    #[ignore = "clears real Discord rendering caches in an isolated Windows VM"]
    fn real_discord_cache_preserves_account_and_session_state() {
        assert_eq!(
            std::env::var("STRAWBERRYDISK_TEST_REAL_DISCORD_CACHE").as_deref(),
            Ok("1"),
            "set STRAWBERRYDISK_TEST_REAL_DISCORD_CACHE=1 only in an isolated Windows VM"
        );
        let roaming_app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .expect("APPDATA must be available");
        let discord_root = roaming_app_data.join("discord");
        assert!(
            discord_root.is_dir(),
            "Discord must have completed a first launch"
        );
        let running = ProcessSnapshot::capture()
            .expect("the Windows process inventory must be available")
            .matching_processes(&["Discord.exe".to_string()]);
        assert!(
            running.is_empty(),
            "Discord must be completely stopped before the real cleanup diagnostic"
        );

        let preserved_paths = [
            discord_root.join("Local State"),
            discord_root.join("Preferences"),
            discord_root.join("settings.json"),
            discord_root.join("Local Storage"),
            discord_root.join("Network"),
            discord_root.join("Service Worker"),
            discord_root.join("Session Storage"),
            discord_root.join("WebStorage"),
            discord_root.join("shared_proto_db"),
        ];
        assert!(
            preserved_paths.iter().all(|path| path.exists()),
            "the Discord profile must expose representative durable state"
        );
        let preserved_before = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();

        let target_roots = [
            discord_root.join("Cache"),
            discord_root.join("GPUCache"),
            discord_root.join("logs"),
        ];
        assert!(
            target_roots.iter().all(|path| path.is_dir()),
            "the Discord profile must expose the verified cache and log roots"
        );
        let markers = target_roots.map(|root| root.join("strawberrydisk-rule-validation.bin"));
        for marker in &markers {
            fs::write(marker, b"payload").expect("the Discord cache marker must be created");
        }
        let request = |dry_run| CleanupRequest {
            rule_ids: vec!["app.discord-cache".to_string()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };

        let preview = CleanupService::execute(request(true))
            .expect("the real Discord cache dry run must succeed");
        assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
        assert!(preview.expected_bytes >= 21);
        assert!(markers.iter().all(|marker| marker.exists()));

        let result = CleanupService::execute(request(false))
            .expect("the real Discord cache cleanup must succeed");
        assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
        assert!(result.released_bytes >= 21);
        assert!(markers.iter().all(|marker| !marker.exists()));
        let preserved_after = preserved_paths
            .iter()
            .map(|path| digest_tree(path))
            .collect::<Vec<_>>();
        assert_eq!(preserved_after, preserved_before);
        println!(
            "real_discord_cache_cleanup expected_bytes={} released_bytes={} affected_item_count={} preserved_root_count={}",
            preview.expected_bytes,
            result.released_bytes,
            result.affected_item_count,
            preserved_paths.len()
        );
    }
