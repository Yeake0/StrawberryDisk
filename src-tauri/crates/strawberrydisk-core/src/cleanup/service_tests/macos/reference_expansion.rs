#[test]
fn reference_expansion_logs_preserve_recent_unrelated_and_deep_files() {
    let sandbox = tempfile::tempdir().unwrap();
    let rules = crate::cleanup::rules::registry().unwrap();
    for name in ["old.xlog", "old.log"] {
        let id = "app.wechat-diagnostic-cache";
        let root = fs::canonicalize(sandbox.path()).unwrap().join(name);
        fs::create_dir_all(root.join("a/b/c")).unwrap();
        let old = root.join(name);
        let recent = root.join(format!("recent-{name}"));
        let unrelated = root.join("keep.db");
        let buffered = root.join("active.log.buffer");
        let deep = root.join("a/b/c").join(name);
        for path in [&old, &recent, &unrelated, &buffered, &deep] {
            fs::write(path, b"fixture").unwrap();
        }
        for path in [&old, &unrelated, &buffered, &deep] {
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(
                    fs::FileTimes::new()
                        .set_modified(SystemTime::now() - Duration::from_secs(8 * 86400)),
                )
                .unwrap();
        }
        let rule = rules.iter().find(|rule| rule.id == id).unwrap();
        let canonical = validate_rule_root(&root, &rule.matcher).unwrap();
        let mut stats = DeleteStats {
            matched_bytes: 0,
            deleted_bytes: 0,
            affected_item_count: 0,
            failed_item_count: 0,
            removed_empty_directory_count: 0,
            logged_failure_count: 0,
        };
        delete_root_contents(
            &root,
            &canonical,
            &rule.matcher,
            &|_, _| true,
            &|| false,
            &mut stats,
        );
        assert!(!old.exists());
        assert!([recent, unrelated, buffered, deep]
            .iter()
            .all(|path| path.exists()));
        assert_eq!(stats.affected_item_count, 1);
        assert_eq!(stats.failed_item_count, 0);
    }
}

#[test]
#[ignore = "inspects or clears only the two explicitly selected real Tencent cache rules"]
fn reference_expansion_real_tencent_validation() {
    let mode =
        std::env::var("STRAWBERRYDISK_REFERENCE_VALIDATION").expect("explicit validation mode required");
    assert!(["preview", "blocked", "apply"].contains(&mode.as_str()));
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let wechat = home.join("Library/Containers/com.tencent.xinWeChat/Data");
    let wecom = home.join("Library/Containers/com.tencent.WeWorkMac/Data");
    let ids = ["app.wechat-diagnostic-cache", "app.wecom-cache"];
    let snapshot = || {
        let mut values = Vec::new();
        for path in [
            wechat.join("Documents/xwechat_files"),
            wechat.join("Library/Preferences"),
            wecom.join("Documents/Profiles"),
            wecom.join("Library/Preferences"),
        ] {
            if path.exists() {
                values.push(digest_macos_tree_without_following_links(&path));
            }
        }
        let cef = wecom.join("Documents/cefcache");
        if cef.exists() {
            let caches = [
                "Cache",
                "Code Cache",
                "GPUCache",
                "DawnCache",
                "DawnGraphiteCache",
                "DawnWebGPUCache",
                "GrShaderCache",
                "GraphiteDawnCache",
                "ShaderCache",
                "Shared Dictionary",
                "Service Worker",
                "component_crx_cache",
            ];
            let profiles = direct_directory_children(&cef)
                .into_iter()
                .filter(|path| {
                    path.file_name().is_some_and(|name| {
                        name == "Default" || name.to_string_lossy().starts_with("wew_")
                    })
                })
                .collect::<Vec<_>>();
            for profile in profiles {
                values.push(digest_macos_tree_with_exclusions_without_following_links(
                    &profile, &caches,
                ));
                for (directory, exclusion) in [
                    ("Service Worker", "CacheStorage"),
                    ("Shared Dictionary", "cache"),
                ] {
                    let path = profile.join(directory);
                    if path.exists() {
                        values.push(digest_macos_tree_with_exclusions_without_following_links(
                            &path,
                            &[exclusion],
                        ));
                    }
                }
            }
        }
        assert!(values.len() >= 3, "real durable state must be present");
        values
    };
    let before = if mode == "apply" {
        Some(snapshot())
    } else {
        None
    };
    for id in ids {
        let request = |dry_run| CleanupRequest {
            rule_ids: vec![id.into()],
            source_selections: Vec::new(),
            dry_run,
            project_roots: Vec::new(),
        };
        let preview = CleanupService::execute(request(true)).unwrap();
        println!(
            "reference_expansion rule={id} preview_bytes={} status={:?}",
            preview.expected_bytes, preview.actions[0].status
        );
        if mode == "preview" {
            continue;
        }
        let result = CleanupService::execute(request(false)).unwrap();
        if mode == "blocked" {
            assert_eq!(
                result.actions[0].status,
                crate::cleanup::CleanupActionStatus::Blocked,
                "{id}"
            );
            assert_eq!(result.released_bytes, 0);
        } else {
            assert_eq!(result.failed_item_count, 0, "{:?}", result.actions);
            assert_eq!(result.released_bytes, preview.expected_bytes, "{id}");
        }
        println!(
            "reference_expansion rule={id} released_bytes={} items={} status={:?}",
            result.released_bytes, result.affected_item_count, result.actions[0].status
        );
    }
    if let Some(before) = before {
        assert_eq!(snapshot(), before, "durable state changed");
    }
}
