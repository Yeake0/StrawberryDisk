/// Checks the signed Windows Notion client after it has created a renderer
/// partition, keeping its local database and worker scripts unchanged.
/// This does not exercise page loading without a network connection.
#[test]
#[ignore = "clears the real Windows Notion response cache after Notion exits"]
fn real_windows_notion_service_worker_cache_preserves_persistent_state() {
    assert_eq!(
        std::env::var("STRAWBERRYDISK_TEST_REAL_WINDOWS_NOTION_CACHE").as_deref(),
        Ok("1")
    );
    let running = ProcessSnapshot::capture()
        .expect("Windows process inventory must be available")
        .matching_processes(&["Notion.exe".to_string()]);
    assert!(running.is_empty(), "Notion must be stopped");

    let root = PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA must be set"))
        .join("Notion");
    let mut partitions = fs::read_dir(root.join("Partitions"))
        .expect("real Notion partitions must be readable")
        .map(|entry| entry.expect("partition entry must be readable").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    partitions.sort();
    assert!(!partitions.is_empty(), "Notion must have a renderer partition");

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
    let before = preserved.iter().map(|path| digest_tree(path)).collect::<Vec<_>>();
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
    assert!(marker.exists(), "dry-run must preserve the marker");

    let result = CleanupService::execute(request(false)).expect("cleanup must succeed");
    assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
    assert_eq!(result.released_bytes, preview.expected_bytes);
    assert!(!marker.exists(), "selected cache marker must be removed");
    assert_eq!(
        preserved.iter().map(|path| digest_tree(path)).collect::<Vec<_>>(),
        before,
        "persistent Notion state changed"
    );
    println!(
        "real_windows_notion_response_cache expected_bytes={} released_bytes={} preserved_roots={}",
        preview.expected_bytes,
        result.released_bytes,
        preserved.len()
    );
}

/// Claude Code's unauthenticated Windows startup does not create a changelog.
/// This test places a disposable cache fixture at its documented location and
/// verifies that StrawberryDisk removes only that file, leaving CLI state intact.
#[test]
#[ignore = "creates and clears a Windows Claude Code changelog fixture"]
fn real_windows_claude_code_changelog_cache_preserves_user_state() {
    assert_eq!(
        std::env::var("STRAWBERRYDISK_TEST_REAL_WINDOWS_CLAUDE_CACHE").as_deref(),
        Ok("1")
    );
    let home = PathBuf::from(std::env::var_os("USERPROFILE").expect("USERPROFILE must be set"));
    let changelog = home.join(".claude/cache/changelog.md");
    assert!(
        !changelog.exists(),
        "test must not overwrite an existing Claude Code changelog"
    );
    fs::create_dir_all(changelog.parent().expect("cache parent must exist"))
        .expect("cache fixture directory must be writable");
    let mut fixture = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&changelog)
        .expect("cache fixture must not replace an existing file");
    std::io::Write::write_all(&mut fixture, b"fixture release notes")
        .expect("cache fixture must be writable");
    drop(fixture);
    let preserved = [home.join(".claude.json"), home.join(".claude/backups")]
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    assert!(!preserved.is_empty(), "Claude Code must have real user state");
    let before = preserved.iter().map(|path| digest_tree(path)).collect::<Vec<_>>();

    let request = |dry_run| CleanupRequest {
        rule_ids: vec!["ai.claude-code-cache".to_string()],
        source_selections: Vec::new(),
        dry_run,
        project_roots: Vec::new(),
    };
    let preview = CleanupService::execute(request(true)).expect("dry-run must succeed");
    assert_eq!(preview.failed_item_count, 0, "{:?}", preview.actions);
    assert_eq!(preview.expected_bytes, b"fixture release notes".len() as u64);
    assert!(changelog.exists(), "dry-run must preserve the fixture");

    let result = CleanupService::execute(request(false)).expect("cleanup must succeed");
    assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
    assert_eq!(result.released_bytes, preview.expected_bytes);
    assert!(!changelog.exists(), "cached changelog fixture must be removed");
    assert_eq!(
        preserved.iter().map(|path| digest_tree(path)).collect::<Vec<_>>(),
        before,
        "Claude Code state changed"
    );
    println!(
        "real_windows_claude_cache expected_bytes={} released_bytes={} preserved_roots={}",
        preview.expected_bytes,
        result.released_bytes,
        preserved.len()
    );
}
