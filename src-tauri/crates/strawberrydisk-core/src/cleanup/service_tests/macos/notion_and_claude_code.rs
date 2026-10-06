/// Verifies that an active Notion process blocks the newly selected response
/// cache before any application data is removed.
#[test]
#[ignore = "requires the real macOS Notion client to be running"]
fn real_notion_service_worker_cache_blocks_while_running() {
    assert_eq!(
        std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_NOTION_CACHE_BLOCK").as_deref(),
        Ok("1")
    );
    let running = ProcessSnapshot::capture()
        .expect("macOS process inventory must be available")
        .matching_processes(&["Notion".to_string()]);
    assert!(!running.is_empty(), "Notion must be running");

    let result = CleanupService::execute(CleanupRequest {
        rule_ids: vec!["app.notion-service-worker-cache".to_string()],
        source_selections: Vec::new(),
        dry_run: false,
        project_roots: Vec::new(),
    })
    .expect("the process gate must return a structured result");
    assert_eq!(result.actions.len(), 1);
    assert_eq!(
        result.actions[0].reason_code,
        Some(crate::cleanup::CleanupActionReason::RunningProcesses)
    );
    assert_eq!(result.released_bytes, 0);
}

/// Hashes persistent Notion files before and after response-cache cleanup.
/// This does not exercise page loading without a network connection.
#[test]
#[ignore = "clears the real macOS Notion response cache after Notion exits"]
fn real_notion_service_worker_cache_preserves_persistent_state() {
    assert_eq!(
        std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_NOTION_CACHE").as_deref(),
        Ok("1")
    );
    let running = ProcessSnapshot::capture()
        .expect("macOS process inventory must be available")
        .matching_processes(&["Notion".to_string()]);
    assert!(running.is_empty(), "Notion must be stopped");

    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME must be set"));
    let root = home.join("Library/Application Support/Notion");
    let partitions = direct_directory_children(&root.join("Partitions"));
    assert!(!partitions.is_empty(), "Notion must have a real partition");

    let mut preserved = vec![root.join("notion.db"), root.join("Local Storage")];
    let mut response_caches = Vec::new();
    for partition in &partitions {
        preserved.extend(
            [
                "IndexedDB",
                "Local Storage",
                "Session Storage",
                "WebStorage",
                "Service Worker/Database",
                "Service Worker/ScriptCache",
            ]
            .map(|relative| partition.join(relative))
            .into_iter()
            .filter(|path| path.exists()),
        );
        let cache = partition.join("Service Worker/CacheStorage");
        if cache.is_dir() {
            response_caches.push(cache);
        }
    }
    assert!(preserved.len() >= 7, "durable state must be present");
    assert!(!response_caches.is_empty(), "response cache must be present");
    let before = preserved
        .iter()
        .map(|path| digest_macos_tree_without_following_links(path))
        .collect::<Vec<_>>();
    let marker = tempfile::Builder::new()
        .prefix("strawberrydisk-rule-validation-")
        .tempfile_in(&response_caches[0])
        .expect("cache marker must be created without replacing an existing file")
        .into_temp_path();
    fs::write(&marker, b"notion-cache-test").expect("cache marker must be writable");

    let request = |dry_run| CleanupRequest {
        rule_ids: vec!["app.notion-service-worker-cache".to_string()],
        source_selections: Vec::new(),
        dry_run,
        project_roots: Vec::new(),
    };
    let preview = CleanupService::execute(request(true)).expect("dry-run must succeed");
    assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
    assert!(
        preview.expected_bytes > b"notion-cache-test".len() as u64,
        "Notion must have generated real response-cache data"
    );
    assert!(marker.exists(), "dry-run must not remove the marker");

    let result = CleanupService::execute(request(false)).expect("cleanup must succeed");
    assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
    assert_eq!(result.released_bytes, preview.expected_bytes);
    assert!(!marker.exists(), "selected cache marker must be removed");
    assert_eq!(
        preserved
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>(),
        before,
        "persistent Notion state changed"
    );
    println!(
        "real_macos_notion_response_cache expected_bytes={} released_bytes={} preserved_roots={}",
        preview.expected_bytes,
        result.released_bytes,
        preserved.len()
    );
}

/// The changelog is the only selected Claude Code file. In particular, the
/// prompt-history file and installed marketplace must remain unchanged.
#[test]
#[ignore = "clears the real macOS Claude Code cached changelog"]
fn real_claude_code_changelog_cache_preserves_user_state() {
    assert_eq!(
        std::env::var("STRAWBERRYDISK_TEST_REAL_MACOS_CLAUDE_CACHE").as_deref(),
        Ok("1")
    );
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME must be set"));
    let root = home.join(".claude");
    let changelog = root.join("cache/changelog.md");
    assert!(changelog.is_file(), "real cached changelog must exist");
    let preserved = [root.join("history.jsonl"), root.join("plugins/marketplaces")]
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    assert!(!preserved.is_empty(), "representative user state must exist");
    let before = preserved
        .iter()
        .map(|path| digest_macos_tree_without_following_links(path))
        .collect::<Vec<_>>();

    let request = |dry_run| CleanupRequest {
        rule_ids: vec!["ai.claude-code-cache".to_string()],
        source_selections: Vec::new(),
        dry_run,
        project_roots: Vec::new(),
    };
    let preview = CleanupService::execute(request(true)).expect("dry-run must succeed");
    assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
    assert!(preview.expected_bytes > 0, "changelog must be selected");
    assert!(changelog.exists(), "dry-run must preserve changelog");

    let result = CleanupService::execute(request(false)).expect("cleanup must succeed");
    assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
    assert_eq!(result.released_bytes, preview.expected_bytes);
    assert!(!changelog.exists(), "cached changelog must be removed");
    assert_eq!(
        preserved
            .iter()
            .map(|path| digest_macos_tree_without_following_links(path))
            .collect::<Vec<_>>(),
        before,
        "Claude Code user state changed"
    );
    println!(
        "real_macos_claude_cache expected_bytes={} released_bytes={} preserved_roots={}",
        preview.expected_bytes,
        result.released_bytes,
        preserved.len()
    );
}
