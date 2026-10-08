use super::{compile_declarative_source, parse_current_platform_catalog};
use std::{fs, path::Path};

use crate::{
    applications::catalog::ProcessSnapshot,
    cleanup::{
        exclusions::CleanupExclusions,
        rule_execution::{execute_rule, measure_owned_rule, RuleExecutionContext},
        rules::{compile_scan_plan, validation::compile_rules, ScanPlan},
        CleanupActionResult, CleanupActionStatus,
    },
    shared::operation::{CoordinatedOperationKind, OperationGuard},
};

const CACHE_CONTENT: &[u8] = b"rebuildable cache";
const STATE_CONTENT: &[u8] = b"persistent state must remain unchanged";

/// Relocates the production source into a private temporary tree without
/// changing HOME or touching installed applications. Root expansion and
/// matching still use the same catalog compiler as a normal cleanup.
fn isolated_plan(id: &str, sandbox: &Path, relative_root: &str) -> ScanPlan {
    let mut source = parse_current_platform_catalog()
        .expect("embedded rules must be valid")
        .into_iter()
        .find(|source| source.id == id)
        .expect("the platform must provide the requested rule");
    assert_eq!(source.roots.len(), 1);
    let name = sandbox
        .file_name()
        .and_then(|name| name.to_str())
        .expect("the temporary directory name must be UTF-8");
    source.roots[0].template = format!("${{temp}}/{name}/{relative_root}");

    // These fixtures have no application writers. The catalog test separately
    // checks process policy; filesystem tests must also pass with Notion open.
    source.required_stopped_processes.clear();
    let mut spec = compile_declarative_source(source).expect("fixture roots must resolve");
    // macOS exposes its temporary directory through /var -> /private/var.
    // Use one physical spelling for ownership checks and destructive traversal.
    for root in &mut spec.roots {
        root.resolved_path = fs::canonicalize(&root.resolved_path)
            .expect("fixture roots must exist before compilation");
    }
    let rules = compile_rules(vec![spec]).expect("the isolated rule must compile");
    compile_scan_plan(rules, &[true], &[]).expect("the isolated scan plan must compile")
}

fn execute_fixture(plan: &ScanPlan, dry_run: bool) -> CleanupActionResult {
    let before = measure_owned_rule(plan, 0, None, &CleanupExclusions::default())
        .expect("fixture measurement must succeed");
    assert_eq!(before.skipped_count, 0);
    let process_snapshot = ProcessSnapshot::default();
    let operation = OperationGuard::start(CoordinatedOperationKind::Cleanup)
        .expect("the isolated cleanup operation must start");
    let action = execute_rule(
        &plan.rules[0],
        0,
        Some(before),
        &RuleExecutionContext {
            ownership_plan: plan,
            process_snapshot: &process_snapshot,
            source_scope: None,
            empty_directory_authorizations: None,
            operation: &operation,
            dry_run,
        },
        &mut |_, _| {},
    );
    operation.complete();
    assert_eq!(action.failed_item_count, 0, "{action:?}");
    assert_eq!(
        action.status,
        if dry_run {
            CleanupActionStatus::Previewed
        } else {
            CleanupActionStatus::Completed
        }
    );
    action
}

fn write_fixture(path: &Path, content: &[u8]) {
    fs::create_dir_all(path.parent().expect("fixture must have a parent"))
        .expect("fixture directory must be created");
    fs::write(path, content).expect("fixture file must be written");
}

#[test]
fn claude_changelog_cleanup_preserves_nested_names_and_user_state() {
    let _operation_lock = crate::shared::operation::test_operation_lock();
    let sandbox = tempfile::tempdir().expect("private fixture directory must be created");
    let root = sandbox.path().join(".claude/cache");
    let changelog = root.join("changelog.md");
    write_fixture(&changelog, CACHE_CONTENT);
    let preserved = [
        root.join("nested/changelog.md"),
        root.join("changelog.md.backup"),
        root.join("settings.json"),
        sandbox.path().join(".claude/history.jsonl"),
        sandbox
            .path()
            .join(".claude/plugins/marketplaces/changelog.md"),
    ];
    for path in &preserved {
        write_fixture(path, STATE_CONTENT);
    }
    let plan = isolated_plan("ai.claude-code-cache", sandbox.path(), ".claude/cache");
    let preview = execute_fixture(&plan, true);
    assert_eq!(preview.bytes_expected, CACHE_CONTENT.len() as u64);
    assert_eq!(preview.released_bytes, 0);
    assert_eq!(fs::read(&changelog).unwrap(), CACHE_CONTENT);

    let result = execute_fixture(&plan, false);
    assert_eq!(result.released_bytes, CACHE_CONTENT.len() as u64);
    assert_eq!(result.affected_item_count, 1);
    assert!(!changelog.exists());
    for path in &preserved {
        assert_eq!(fs::read(path).unwrap(), STATE_CONTENT, "{path:?}");
    }

    // A directory with the expected filename is not the documented cache file.
    // The depth gate must prevent deletion of its similarly named descendant.
    let unexpected_directory_file = changelog.join("changelog.md");
    write_fixture(&unexpected_directory_file, STATE_CONTENT);
    assert_eq!(execute_fixture(&plan, true).bytes_expected, 0);
    assert_eq!(execute_fixture(&plan, false).affected_item_count, 0);
    assert_eq!(fs::read(unexpected_directory_file).unwrap(), STATE_CONTENT);
}

#[test]
fn notion_response_cleanup_preserves_partition_databases_and_scripts() {
    let _operation_lock = crate::shared::operation::test_operation_lock();
    let sandbox = tempfile::tempdir().expect("private fixture directory must be created");
    let root = sandbox.path().join("Notion");
    let mut preserved = vec![
        root.join("notion.db"),
        root.join("Local Storage/leveldb/record"),
        root.join("Service Worker/CacheStorage/response"),
    ];
    let mut selected = Vec::new();
    for name in ["notion", "second-workspace"] {
        let partition = root.join("Partitions").join(name);
        let response = partition.join("Service Worker/CacheStorage/origin/cache/response");
        write_fixture(&response, CACHE_CONTENT);
        selected.push(response);
        preserved.extend(
            [
                "IndexedDB/record",
                "Local Storage/leveldb/record",
                "Session Storage/record",
                "Cookies",
                "Service Worker/Database/record",
                "Service Worker/ScriptCache/script",
                "Service Worker/CacheStorageBackup/response",
            ]
            .map(|relative| partition.join(relative)),
        );
    }
    for path in &preserved {
        write_fixture(path, STATE_CONTENT);
    }
    let plan = isolated_plan(
        "app.notion-service-worker-cache",
        sandbox.path(),
        "Notion/Partitions",
    );
    assert_eq!(plan.rules[0].roots.len(), selected.len());
    let expected_bytes = (selected.len() * CACHE_CONTENT.len()) as u64;
    let preview = execute_fixture(&plan, true);
    assert_eq!(preview.bytes_expected, expected_bytes);
    assert_eq!(preview.released_bytes, 0);
    for path in &selected {
        assert_eq!(fs::read(path).unwrap(), CACHE_CONTENT);
    }

    let result = execute_fixture(&plan, false);
    assert_eq!(result.released_bytes, expected_bytes);
    assert_eq!(result.affected_item_count, selected.len() as u64);
    assert!(selected.iter().all(|path| !path.exists()));
    for path in &preserved {
        assert_eq!(fs::read(path).unwrap(), STATE_CONTENT, "{path:?}");
    }
}
