use std::{
    env,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::{
    cleanup::{CleanupSourceSelection, CleanupSourceSelectionMode},
    shared::operation::{test_operation_lock, CoordinatedOperationKind},
};

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

#[test]
fn excluded_project_is_pruned_while_sibling_build_artifact_remains_cleanable() {
    let _lock = test_operation_lock();
    let fixture = Fixture::new("project-exclusion");
    for name in ["included", "excluded"] {
        let project = fixture.0.join(name);
        fs::create_dir_all(project.join("target")).expect("create artifact directory");
        fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n")
            .expect("write project marker");
        fs::write(project.join("target/output.bin"), [1_u8; 64]).expect("write build artifact");
    }
    let excluded = fixture.0.join("excluded");
    let exclusions =
        CleanupExclusions::resolve(&[display_path(&excluded)]).expect("resolve excluded project");
    let roots = vec![display_path(&fixture.0)];
    let visited = Mutex::new(Vec::<PathBuf>::new());
    let rules = preview_all(
        &roots,
        true,
        &|| false,
        &|path| visited.lock().unwrap().push(path.to_path_buf()),
        &|_, _, _| {},
        &exclusions,
    );
    let rust = rules
        .rules
        .iter()
        .find(|rule| rule.rule_id == "project.rust-build-artifacts")
        .expect("Rust build artifacts remain in the catalog");
    assert_eq!(rust.bytes, 64);
    assert_eq!(rust.sources.len(), 1);
    assert!(rust.sources[0].path.contains("included"));
    assert!(visited
        .lock()
        .unwrap()
        .iter()
        .all(|path| !path.starts_with(&excluded)));

    let operation =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("start fixture cleanup");
    let selected_ids = vec![rust.rule_id.clone()];
    let actions = execute_selected_with_progress(
        ProjectExecutionRequest {
            selected_ids: &selected_ids,
            configured_roots: &roots,
            selected_volume_scope: true,
            source_selections: &SourceSelectionPolicy::empty(),
            dry_run: false,
            operation: &operation,
            exclusions: &exclusions,
        },
        |_, _| {},
    );
    operation.complete();
    assert_eq!(actions[0].released_bytes, 64);
    assert!(!fixture.0.join("included/target").exists());
    assert!(excluded.join("target/output.bin").exists());
}

#[test]
fn exclusion_inside_an_artifact_prevents_deleting_its_parent() {
    let fixture = Fixture::new("nested-artifact-exclusion");
    let project = fixture.0.join("project");
    let protected = project.join("target/protected");
    fs::create_dir_all(&protected).expect("create protected artifact descendant");
    fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n")
        .expect("write project marker");
    fs::write(project.join("target/output.bin"), [1_u8; 64]).expect("write build artifact");
    fs::write(protected.join("keep.bin"), [2_u8; 16]).expect("write protected content");
    let exclusions = CleanupExclusions::resolve(&[display_path(&protected)])
        .expect("resolve protected descendant");
    let plan = build_plan(
        &[display_path(&fixture.0)],
        false,
        current_platform_rules().expect("catalog must load"),
        &|| false,
        &exclusions,
    )
    .expect("build the filtered project plan");
    let rust = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .expect("Rust rule remains in the plan");
    assert!(rust.candidates.is_empty());
    assert!(protected.join("keep.bin").exists());
}

#[test]
fn exclusion_inside_a_project_keeps_unrelated_build_artifacts() {
    let fixture = Fixture::new("unrelated-project-exclusion");
    let project = fixture.0.join("project");
    let excluded = project.join("documents");
    fs::create_dir_all(&excluded).expect("create excluded directory");
    fs::create_dir_all(project.join("target")).expect("create build directory");
    fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n")
        .expect("write project marker");
    fs::write(project.join("target/output.bin"), [1_u8; 64]).expect("write build artifact");
    let exclusions =
        CleanupExclusions::resolve(&[display_path(&excluded)]).expect("resolve excluded folder");
    let plan = build_plan(
        &[display_path(&project)],
        false,
        current_platform_rules().expect("catalog must load"),
        &|| false,
        &exclusions,
    )
    .expect("build project plan");
    let rust = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .expect("Rust rule remains in the plan");

    assert_eq!(rust.candidates.len(), 1);
    assert_eq!(rust.candidates[0].bytes, 64);
}

#[test]
fn execution_preflight_preserves_an_artifact_newly_covered_by_an_exclusion() {
    let _lock = test_operation_lock();
    let fixture = Fixture::new("late-artifact-exclusion");
    let project = fixture.0.join("project");
    let artifact = project.join("target");
    fs::create_dir_all(&artifact).expect("create artifact directory");
    fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n")
        .expect("write project marker");
    fs::write(artifact.join("keep.bin"), [1_u8; 64]).expect("write artifact content");
    let plan = build_plan(
        &[display_path(&project)],
        false,
        current_platform_rules().expect("catalog must load"),
        &|| false,
        &CleanupExclusions::default(),
    )
    .expect("build the unfiltered project plan");
    let rust = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .expect("Rust rule remains in the plan");
    let exclusions = CleanupExclusions::resolve(&[display_path(&artifact)])
        .expect("resolve the newly excluded artifact");
    let operation =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("start fixture cleanup");
    let action = execute_rule_with_exclusions(rust, None, false, &operation, &exclusions);
    operation.complete();

    assert_eq!(action.released_bytes, 0);
    assert_eq!(
        action.reason_code,
        Some(CleanupActionReason::PreflightFailed)
    );
    assert!(artifact.join("keep.bin").exists());
}

pub(super) fn initialize_git_admin(admin: &Path) {
    fs::create_dir_all(admin.join("objects")).unwrap();
    fs::create_dir_all(admin.join("refs/heads")).unwrap();
    fs::write(admin.join("HEAD"), "ref: refs/heads/main\n").unwrap();
}

#[test]
fn stale_artifact_plan_preserves_a_new_program_key() {
    let _lock = test_operation_lock();
    let fixture = Fixture::new("authored-race");
    let project = fixture.0.join("project");
    fs::create_dir_all(project.join("target/deploy")).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n").unwrap();
    fs::write(project.join("target/output"), [1; 64]).unwrap();
    let plan = build_plan(
        &[display_path(&project)],
        false,
        current_platform_rules().unwrap(),
        &|| false,
        &CleanupExclusions::default(),
    )
    .unwrap();
    let rule = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .unwrap();
    assert!(!rule.candidates[0].measurement_limited);
    let key = project.join("target/deploy/program-keypair.json");
    fs::write(&key, b"not a real key").unwrap();
    let operation = OperationGuard::start(CoordinatedOperationKind::Cleanup).unwrap();
    let action = execute_rule(rule, None, false, &operation);
    assert_eq!(
        action.reason_code,
        Some(CleanupActionReason::PreflightFailed)
    );
    assert_eq!(action.released_bytes, 0);
    assert_eq!(fs::read(&key).unwrap(), b"not a real key");
    assert!(project.join("target/output").exists());
    operation.complete();
}

#[test]
fn authored_entry_keeps_preview_visible_but_limited() {
    let fixture = Fixture::new("authored-preview");
    let project = fixture.0.join("project");
    fs::create_dir_all(project.join("target/deploy")).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n").unwrap();
    fs::write(project.join("target/output"), [1; 64]).unwrap();
    fs::write(
        project.join("target/deploy/program-keypair.json"),
        b"not a real key",
    )
    .unwrap();

    let plan = build_plan(
        &[display_path(&project)],
        false,
        current_platform_rules().unwrap(),
        &|| false,
        &CleanupExclusions::default(),
    )
    .unwrap();
    let rule = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .unwrap();

    assert_eq!(rule.candidates.len(), 1);
    assert!(rule.candidates[0].measurement_limited);
    assert_eq!(rule.candidates[0].file_count, 2);
    assert!(rule.candidates[0].bytes >= 64);
}

#[test]
fn portable_measurement_flags_authored_entries_without_pruning_totals() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("target");
    fs::create_dir_all(root.join("debug/.git")).unwrap();
    fs::create_dir_all(root.join("deploy")).unwrap();
    fs::write(root.join("debug/.git/index"), [0_u8; 3]).unwrap();
    fs::write(root.join("debug.bin"), [0_u8; 4]).unwrap();
    fs::write(root.join("deploy/program-keypair.json"), [0_u8; 5]).unwrap();

    let measured =
        portable_measure_directory_with_progress(&root, &|| false, &|_| {}, &|_, _, _| {});

    assert_eq!(measured.measured.file_count, 3);
    assert_eq!(measured.measured.bytes, 12);
    assert_eq!(measured.measured.skipped_count, 0);
    assert!(measured.authored_entry.is_some());
    assert!(validate_artifact_protection(&root, &measured, &|| false).is_err());
}

#[test]
#[ignore = "generates 5000 disposable build files to measure ownership inspection overhead"]
fn artifact_content_protection_workload() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("target");
    for directory in 0..50 {
        let directory = root.join(directory.to_string());
        fs::create_dir_all(&directory).unwrap();
        for index in 0..100 {
            fs::write(directory.join(format!("{index}.o")), [1; 256]).unwrap();
        }
    }
    // Warm every path before sampling so the comparison measures traversal overhead rather than
    // whichever variant happens to populate the filesystem cache first.
    measure_without_authored_entry_check(&root);
    measure_with_legacy_content_walk(&root);
    measure_with_integrated_content_check(&root);

    let mut baseline = Vec::new();
    let mut legacy = Vec::new();
    let mut optimized = Vec::new();
    for run in 0..15 {
        // Rotate order to avoid giving one implementation a systematic cache advantage.
        match run % 3 {
            0 => {
                baseline.push(timed_us(|| measure_without_authored_entry_check(&root)));
                legacy.push(timed_us(|| measure_with_legacy_content_walk(&root)));
                optimized.push(timed_us(|| measure_with_integrated_content_check(&root)));
            }
            1 => {
                optimized.push(timed_us(|| measure_with_integrated_content_check(&root)));
                baseline.push(timed_us(|| measure_without_authored_entry_check(&root)));
                legacy.push(timed_us(|| measure_with_legacy_content_walk(&root)));
            }
            _ => {
                legacy.push(timed_us(|| measure_with_legacy_content_walk(&root)));
                optimized.push(timed_us(|| measure_with_integrated_content_check(&root)));
                baseline.push(timed_us(|| measure_without_authored_entry_check(&root)));
            }
        }
    }
    println!(
        "artifact_protection_workload files=5000 bytes=1280000 runs=15 baseline_p50_us={} baseline_p95_us={} legacy_p50_us={} legacy_p95_us={} optimized_p50_us={} optimized_p95_us={}",
        percentile(&mut baseline, 50),
        percentile(&mut baseline, 95),
        percentile(&mut legacy, 50),
        percentile(&mut legacy, 95),
        percentile(&mut optimized, 50),
        percentile(&mut optimized, 95)
    );

    let git = std::process::Command::new("git")
        .arg("-C")
        .arg(fixture.path())
        .args(["init", "--quiet"])
        .output()
        .unwrap();
    assert!(git.status.success());
    measure_with_legacy_content_walk(&root);
    measure_with_integrated_content_check(&root);
    let mut git_legacy = Vec::new();
    let mut git_optimized = Vec::new();
    for run in 0..15 {
        if run % 2 == 0 {
            git_legacy.push(timed_us(|| measure_with_legacy_content_walk(&root)));
            git_optimized.push(timed_us(|| measure_with_integrated_content_check(&root)));
        } else {
            git_optimized.push(timed_us(|| measure_with_integrated_content_check(&root)));
            git_legacy.push(timed_us(|| measure_with_legacy_content_walk(&root)));
        }
    }
    println!(
        "artifact_protection_git_workload files=5000 runs=15 legacy_p50_us={} legacy_p95_us={} optimized_p50_us={} optimized_p95_us={}",
        percentile(&mut git_legacy, 50),
        percentile(&mut git_legacy, 95),
        percentile(&mut git_optimized, 50),
        percentile(&mut git_optimized, 95)
    );
}

#[test]
#[ignore = "generates 505000 disposable files to verify that content protection has no entry cap"]
fn artifact_content_protection_large_tree_workload() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("target");
    for directory in 0..1010 {
        let directory = root.join(directory.to_string());
        fs::create_dir_all(&directory).unwrap();
        for index in 0..500 {
            fs::write(directory.join(format!("{index}.o")), []).unwrap();
        }
    }

    let started = Instant::now();
    let measured = measure_directory(&root, &|| false);
    validate_artifact_protection(&root, &measured, &|| false).unwrap();
    let elapsed = started.elapsed();
    assert_eq!(measured.measured.file_count, 505_000);
    assert_eq!(measured.measured.skipped_count, 0);
    assert!(measured.authored_entry.is_none());
    println!(
        "artifact_protection_large_tree files=505000 elapsed_ms={}",
        elapsed.as_millis()
    );
}

fn timed_us(action: impl FnOnce()) -> u128 {
    let started = Instant::now();
    action();
    started.elapsed().as_micros()
}

fn percentile(samples: &mut [u128], percentile: usize) -> u128 {
    samples.sort_unstable();
    let index = samples.len().saturating_mul(percentile).saturating_sub(1) / 100;
    samples[index.min(samples.len().saturating_sub(1))]
}

fn measure_without_authored_entry_check(root: &Path) {
    let aggregate = current_platform()
        .fast_project_artifact_tree_aggregate(root, &|| false, &|_, _, _| {}, |_| false)
        .unwrap()
        .expect("macOS and Windows provide a native project-artifact aggregate");
    assert_eq!(aggregate.file_count, 5000);
    assert_eq!(aggregate.bytes, 5000 * 256);
}

fn measure_with_legacy_content_walk(root: &Path) {
    measure_without_authored_entry_check(root);
    legacy_validate_authored_entry_names(root).unwrap();
    super::super::project_artifact_protection::validate_ownership(root, &|| false).unwrap();
}

fn measure_with_integrated_content_check(root: &Path) {
    let measured = measure_directory(root, &|| false);
    validate_artifact_protection(root, &measured, &|| false).unwrap();
    assert_eq!(measured.measured.file_count, 5000);
    assert_eq!(measured.measured.bytes, 5000 * 256);
}

/// Models the removed protection pass for a stable before/after workload. It deliberately calls
/// `symlink_metadata` for every entry because that extra per-path lookup was the dominant cost of
/// the old implementation compared with the native aggregate.
fn legacy_validate_authored_entry_names(root: &Path) -> Result<(), String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if path != root
            && path
                .file_name()
                .is_some_and(super::super::project_artifact_protection::is_authored_entry_name)
        {
            return Err("artifact contains protected authored content".to_string());
        }
        if metadata.is_dir() && !is_link_like(&metadata) {
            for entry in fs::read_dir(&path).map_err(|error| error.to_string())? {
                stack.push(entry.map_err(|error| error.to_string())?.path());
            }
        }
    }
    Ok(())
}

#[test]
fn codex_worktree_artifacts_use_regular_project_rules_and_preserve_durable_data() {
    let fixture = Fixture::new("codex-artifacts");
    let checkout = fixture.0.join("worktrees/1234/project");
    let admin = fixture.0.join("repository/.git/worktrees/project");
    initialize_git_admin(&admin);
    fs::create_dir_all(checkout.join("target")).unwrap();
    fs::create_dir_all(&admin).unwrap();
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}", admin.display()),
    )
    .unwrap();
    fs::write(
        admin.join("gitdir"),
        checkout.join(".git").to_str().unwrap(),
    )
    .unwrap();
    fs::write(
        admin.join("codex-thread.json"),
        r#"{"ownerThreadId":"fixture"}"#,
    )
    .unwrap();
    fs::write(checkout.join("Cargo.toml"), "[package]\nname='fixture'\n").unwrap();
    fs::write(checkout.join("target/output"), [7; 64]).unwrap();
    fs::write(checkout.join("source.rs"), "source must remain").unwrap();
    fs::write(fixture.0.join("auth.json"), "credential fixture").unwrap();

    let roots = codex_worktrees::discover(&fixture.0, &|| false).unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(
        codex_worktrees::protected_checkout(&checkout.join("nested"), None),
        Some(checkout.clone()),
        "verified checkouts outside the configured home must remain protected"
    );
    let plan = build_plan(
        &roots
            .iter()
            .map(|root| display_path(root))
            .collect::<Vec<_>>(),
        false,
        current_platform_rules().unwrap(),
        &|| false,
        &CleanupExclusions::default(),
    )
    .unwrap();
    let rule = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .unwrap();
    assert_eq!(rule.candidates.len(), 1);
    let candidate = &rule.candidates[0];
    assert_eq!(candidate.bytes, 64);
    assert_eq!(
        candidate.codex_checkout,
        Some(fs::canonicalize(&checkout).unwrap())
    );
    assert!(candidate.path.ends_with("target"));

    let mut result = scan_result(
        &rule.source,
        ScanItemStatus::Found,
        64,
        1,
        0,
        cleanup_source_details(&rule.candidates),
    );
    apply_codex_process_guard(
        &mut result,
        &rule.candidates,
        Some(&Ok(vec!["Codex".into()])),
    );
    assert_eq!(result.status, ScanItemStatus::Found);
    assert!(result.selectable);
    assert!(
        result.requires_app_close,
        "known Codex apps must support the user-confirmed close flow"
    );
    assert_eq!(
        result.sources[0].block_reason,
        Some(crate::cleanup::CleanupSourceBlockReason::RequiresClose)
    );

    // Invalidate ownership after discovery. Even a manually forged selection
    // must not delete a candidate whose recorded provenance has changed.
    fs::remove_file(admin.join("gitdir")).unwrap();
    let _lock = test_operation_lock();
    let operation = OperationGuard::start(CoordinatedOperationKind::Cleanup).unwrap();
    let action = execute_rule(rule, None, false, &operation);
    assert_eq!(action.released_bytes, 0);
    assert_eq!(action.failed_item_count, 1);
    assert!(checkout.join("target/output").exists());
    assert!(checkout.join("source.rs").exists());
    assert!(fixture.0.join("auth.json").exists());
}

#[test]
fn codex_discovery_failure_and_rebuilt_plans_preserve_automatic_source_protection() {
    let fixture = Fixture::new("codex-discovery-failure");
    let checkout = fixture.0.join("worktrees/1234/project");
    let admin = fixture.0.join("repository/.git/worktrees/project");
    initialize_git_admin(&admin);
    fs::create_dir_all(checkout.join("target")).unwrap();
    fs::create_dir_all(&admin).unwrap();
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}", admin.display()),
    )
    .unwrap();
    fs::write(
        admin.join("gitdir"),
        checkout.join(".git").to_str().unwrap(),
    )
    .unwrap();
    fs::write(
        admin.join("codex-thread.json"),
        r#"{"ownerThreadId":"fixture"}"#,
    )
    .unwrap();
    fs::write(checkout.join("Cargo.toml"), "[package]\nname='fixture'\n").unwrap();
    fs::write(checkout.join("target/output"), [7; 64]).unwrap();
    fs::write(checkout.join("source.rs"), "source must remain").unwrap();
    // Exceed the bounded enumerator without depending on platform permissions.
    // Native or cached project discovery can still return the valid checkout.
    for index in 0..513 {
        fs::write(fixture.0.join(format!("worktrees/entry-{index}")), "").unwrap();
    }
    assert!(codex_worktrees::discover(&fixture.0, &|| false).is_err());
    let roots = [display_path(&checkout)];
    let rebuild = || {
        build_plan_with_progress(ProjectPlanRequest {
            configured_roots: &roots,
            deep_project_discovery: true,
            rules: current_platform_rules().unwrap(),
            is_cancelled: &|| false,
            report_path: &|_| {},
            report_files: &|_, _, _| {},
            codex_home: Some(&fixture.0),
            exclusions: &CleanupExclusions::default(),
        })
        .unwrap()
    };
    let plan = rebuild();
    assert!(!plan.limited);
    let rule = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .unwrap();
    assert_eq!(rule.candidates.len(), 1);
    assert_eq!(
        rule.candidates[0].codex_checkout,
        Some(fs::canonicalize(&checkout).unwrap())
    );
    let mut result = scan_result(
        &rule.source,
        ScanItemStatus::Found,
        64,
        1,
        0,
        cleanup_source_details(&rule.candidates),
    );
    apply_codex_process_guard(
        &mut result,
        &rule.candidates,
        Some(&Ok(vec!["Codex".into()])),
    );
    assert!(result.selectable);
    assert_eq!(result.status, ScanItemStatus::Found);

    // Execution rebuilds its plan instead of trusting preview provenance.
    // Breaking the Git association must not reclassify this candidate as ordinary.
    fs::remove_file(admin.join("gitdir")).unwrap();
    let rebuilt = rebuild();
    let rule = rebuilt
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .unwrap();
    assert_eq!(rule.candidates.len(), 1);
    assert!(rule.candidates[0].codex_checkout.is_some());
    let _lock = test_operation_lock();
    let operation = OperationGuard::start(CoordinatedOperationKind::Cleanup).unwrap();
    let action = execute_rule(rule, None, false, &operation);
    assert_eq!(action.released_bytes, 0);
    assert_eq!(action.failed_item_count, 1);
    assert_eq!(
        fs::read(checkout.join("target/output")).unwrap(),
        vec![7; 64]
    );
    assert_eq!(
        fs::read_to_string(checkout.join("source.rs")).unwrap(),
        "source must remain"
    );
}

#[test]
fn codex_process_guard_leaves_normal_project_sources_selectable() {
    let fixture = Fixture::new("mixed-codex-sources");
    let candidates = [false, true].map(|codex| ArtifactCandidate {
        project_root: fixture.0.clone(),
        codex_checkout: codex.then(|| fixture.0.clone()),
        path: fixture.0.join(if codex {
            "codex/target"
        } else {
            "ordinary/target"
        }),
        bytes: 32,
        file_count: 1,
        modified_at_ms: None,
        measurement_limited: false,
    });
    let rule = &current_platform_rules().unwrap()[0];
    for processes in [
        Ok(vec!["Codex".into()]),
        Err("inventory unavailable".into()),
    ] {
        let mut result = scan_result(
            rule,
            ScanItemStatus::Found,
            64,
            2,
            0,
            cleanup_source_details(&candidates),
        );
        apply_codex_process_guard(&mut result, &candidates, Some(&processes));
        assert!(result.selectable);
        assert_eq!(result.bytes, if processes.is_ok() { 64 } else { 32 });
        assert_eq!(result.file_count, if processes.is_ok() { 2 } else { 1 });
        assert_eq!(
            result
                .sources
                .iter()
                .map(|source| source.bytes)
                .sum::<u64>(),
            64
        );
        assert_eq!(
            result
                .sources
                .iter()
                .filter(|source| source.block_reason.is_none())
                .count(),
            1
        );
    }
    let mut result = scan_result(
        rule,
        ScanItemStatus::Found,
        64,
        2,
        0,
        cleanup_source_details(&candidates),
    );
    apply_codex_process_guard(&mut result, &candidates, Some(&Ok(Vec::new())));
    assert_eq!(result.bytes, 64);
    assert_eq!(result.file_count, 2);
    assert!(result
        .sources
        .iter()
        .all(|source| source.block_reason.is_none()));
}

/// Apply denial only to disposable fixtures and always restore access before their cleanup.
#[cfg(any(unix, windows))]
struct DeniedDirectoryReads(PathBuf);

#[cfg(any(unix, windows))]
impl DeniedDirectoryReads {
    fn new(path: &Path) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
        }
        #[cfg(windows)]
        {
            let output = std::process::Command::new("icacls.exe")
                .arg(path)
                .args(["/deny", "*S-1-1-0:(RD)"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "fixture read denial must succeed: {output:?}"
            );
        }
        let guard = Self(path.to_path_buf());
        // Privileged SSH tokens on Windows can bypass directory ACLs. These fixtures must
        // exercise real denials, so validate the token before interpreting scan counters.
        let read_error = fs::read_dir(path).err();
        assert!(
            read_error.as_ref().is_some_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied),
            "fixture must deny directory reads; run without ACL-bypassing privileges (Windows: a normal desktop terminal); error={read_error:?}"
        );
        guard
    }
}

#[cfg(any(unix, windows))]
impl Drop for DeniedDirectoryReads {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)).unwrap();
        }
        #[cfg(windows)]
        {
            let output = std::process::Command::new("icacls.exe")
                .arg(&self.0)
                .args(["/remove:d", "*S-1-1-0"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "fixture read access must be restored: {output:?}"
            );
        }
    }
}

struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Self {
        let path = env::temp_dir().join(format!(
            "mangodisk-project-artifacts-{label}-{}-{}",
            std::process::id(),
            NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("fixture root must be created");
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn standard_roots_find_projects_without_workspace_name_conventions() {
    let fixture = Fixture::new("standard-roots");
    let home = fixture.0.join("profile");
    let arbitrary_parent = home.join("client-deliveries/customer-a");
    let project = arbitrary_parent.join("demo");
    let runtime_data = home.join("runtime-data/local");
    let runtime_project = runtime_data.join("generated-project");
    let hidden_project = home.join(".private/hidden-project");
    for directory in [&project, &runtime_project, &hidden_project] {
        fs::create_dir_all(directory).expect("fixture directory must exist");
        fs::write(directory.join("Cargo.toml"), "[package]\nname='fixture'\n")
            .expect("project marker must exist");
    }

    let roots = standard_discovery_roots(&home, &[runtime_data]);
    assert!(roots.contains(&home.join("client-deliveries")));
    assert!(!roots.contains(&home.join("runtime-data")));
    assert!(!roots.contains(&home.join(".private")));

    let rules = current_platform_rules().expect("catalog must load");
    let prune_names = artifact_prune_names(rules);
    assert!(automatic_project_root_allowed(
        &project,
        &roots,
        &prune_names
    ));
    assert!(!automatic_project_root_allowed(
        &runtime_project,
        &roots,
        &prune_names
    ));
    assert!(!automatic_project_root_allowed(
        &fixture.0.join("other-profile/project"),
        &roots,
        &prune_names
    ));
    let (projects, limited) = discover_projects(
        &roots,
        rules,
        ProjectTraversalPolicy {
            skip_hidden_directories: true,
            deep: true,
            maximum_depth: MAX_DISCOVERY_DEPTH,
            exclusions: &CleanupExclusions::default(),
        },
        &|| false,
        &|_| {},
    )
    .expect("standard project discovery must succeed");
    assert!(!limited);
    assert!(projects
        .iter()
        .any(|candidate| candidate.project_root == project));
    assert!(!projects
        .iter()
        .any(|candidate| candidate.project_root == runtime_project));
    assert!(!projects
        .iter()
        .any(|candidate| candidate.project_root == hidden_project));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_consent_protected_roots_do_not_include_custom_project_directories() {
    let home = current_platform()
        .user_directories()
        .expect("macOS user directories must resolve")
        .home_directory()
        .to_path_buf();

    for name in [
        "Desktop",
        "Documents",
        "Downloads",
        "Movies",
        "Music",
        "Pictures",
    ] {
        assert!(macos_root_requires_explicit_consent(&home.join(name)));
    }
    assert!(!macos_root_requires_explicit_consent(
        &home.join("Projects")
    ));
}

#[test]
fn deep_roots_include_user_content_and_exclude_system_locations() {
    let fixture = Fixture::new("deep-roots");
    let system_volume = fixture.0.join("system-volume");
    let home = system_volume.join("Users/current");
    let downloads = home.join("Downloads");
    let library = home.join("Library");
    let hidden_cache = home.join(".cache");
    let root_project = system_volume.join("work");
    let users = system_volume.join("Users");
    let external_volume = fixture.0.join("external-volume");
    for directory in [
        &downloads,
        &library,
        &hidden_cache,
        &root_project,
        &users,
        &external_volume,
    ] {
        fs::create_dir_all(directory).expect("fixture directory must exist");
    }

    let roots = deep_discovery_roots(
        &home,
        &system_volume,
        &[system_volume.clone(), external_volume.clone()],
    );

    assert!(roots.contains(&downloads));
    assert!(roots.contains(&root_project));
    assert!(roots.contains(&external_volume));
    assert!(!roots.contains(&library));
    assert!(!roots.contains(&hidden_cache));
    assert!(!roots.contains(&users));
}

#[test]
fn exact_marker_roots_preserve_nested_projects() {
    let fixture = Fixture::new("exact-marker-roots");
    let parent = fixture.0.join("workspace");
    let nested = parent.join("tools/web");
    fs::create_dir_all(&nested).expect("nested project directory must exist");

    let exact = normalize_exact_root_paths(
        vec![parent.clone(), nested.clone(), parent.clone()],
        "testExact",
    )
    .expect("exact marker roots must normalize");
    let recursive = normalize_root_paths(vec![parent.clone(), nested], "testRecursive")
        .expect("recursive roots must normalize");

    let canonical_parent = parent
        .canonicalize()
        .expect("fixture parent must canonicalize");
    assert_eq!(
        exact,
        vec![canonical_parent.clone(), canonical_parent.join("tools/web")]
    );
    assert_eq!(recursive, vec![canonical_parent]);
}

#[test]
fn marker_candidates_reject_hidden_and_generated_subtrees() {
    let fixture = Fixture::new("marker-scope");
    let allowed_root = fixture.0.join("Documents");
    let project = allowed_root.join("product");
    let dependency = project.join("node_modules/dependency");
    let hidden = allowed_root.join(".cache/project");
    for directory in [&project, &dependency, &hidden] {
        fs::create_dir_all(directory).expect("fixture directory must exist");
        fs::write(directory.join("package.json"), "{}").expect("project marker must exist");
    }
    let rules = current_platform_rules().expect("catalog must load");
    let prune_names = artifact_prune_names(rules);

    assert_eq!(
        validated_marker_project_root(
            &project.join("package.json"),
            std::slice::from_ref(&allowed_root),
            &prune_names,
            MAX_DISCOVERY_DEPTH,
        ),
        Some(project.clone())
    );
    assert!(validated_marker_project_root(
        &dependency.join("package.json"),
        std::slice::from_ref(&allowed_root),
        &prune_names,
        MAX_DISCOVERY_DEPTH,
    )
    .is_none());
    assert!(validated_marker_project_root(
        &hidden.join("package.json"),
        std::slice::from_ref(&allowed_root),
        &prune_names,
        MAX_DISCOVERY_DEPTH,
    )
    .is_none());

    assert!(validated_marker_project_root(
        &project.join("package.json"),
        std::slice::from_ref(&allowed_root),
        &prune_names,
        0,
    )
    .is_none());
}

#[test]
fn discovers_nested_projects_without_entering_build_artifacts() {
    let fixture = Fixture::new("discovery");
    let rust_project = fixture.0.join("workspace/rust-app");
    let nested_node = rust_project.join("tools/web");
    fs::create_dir_all(rust_project.join("target/debug")).expect("target must exist");
    fs::create_dir_all(nested_node.join("node_modules/pkg")).expect("node_modules must exist");
    fs::write(
        rust_project.join("Cargo.toml"),
        "[package]\nname='fixture'\n",
    )
    .expect("Cargo marker must exist");
    fs::write(nested_node.join("package.json"), "{}").expect("Node marker must exist");
    fs::write(rust_project.join("target/debug/app"), vec![1_u8; 128])
        .expect("Rust artifact must exist");
    fs::write(
        nested_node.join("node_modules/pkg/index.js"),
        vec![2_u8; 64],
    )
    .expect("Node artifact must exist");

    let rules = current_platform_rules().expect("catalog must load");
    let plan = build_plan(
        &[fixture.0.to_string_lossy().into_owned()],
        false,
        rules,
        &|| false,
        &CleanupExclusions::default(),
    )
    .expect("plan must build");
    let rust = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rust-build-artifacts")
        .expect("Rust rule must exist");
    let node = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.node-build-artifacts")
        .expect("Node rule must exist");
    assert_eq!(rust.candidates.len(), 1);
    assert_eq!(rust.candidates[0].bytes, 128);
    assert_eq!(node.candidates.len(), 1);
    assert_eq!(node.candidates[0].bytes, 64);
    assert!(!plan.limited);
}

#[test]
fn artifact_measurement_reports_file_progress_in_bounded_batches() {
    let fixture = Fixture::new("measurement-progress");
    let artifact = fixture.0.join("target");
    fs::create_dir_all(&artifact).expect("artifact directory must exist");
    for index in 0..(PROGRESS_FILE_BATCH_SIZE + 1) {
        fs::write(artifact.join(format!("file-{index}.bin")), [1_u8])
            .expect("artifact file must exist");
    }
    let observed_files = AtomicU64::new(0);
    let observed_bytes = AtomicU64::new(0);

    let measured =
        measure_directory_with_progress(&artifact, &|| false, &|_| {}, &|_, file_count, bytes| {
            observed_files.fetch_add(file_count, Ordering::Relaxed);
            observed_bytes.fetch_add(bytes, Ordering::Relaxed);
        });

    assert_eq!(measured.measured.file_count, PROGRESS_FILE_BATCH_SIZE + 1);
    assert_eq!(
        observed_files.load(Ordering::Relaxed),
        measured.measured.file_count
    );
    assert_eq!(
        observed_bytes.load(Ordering::Relaxed),
        measured.measured.bytes
    );
}

#[test]
fn native_artifact_measurement_matches_portable_reference() {
    let fixture = Fixture::new("native-measurement-equivalence");
    let artifact = fixture.0.join("target");
    fs::create_dir_all(artifact.join("debug/deps"))
        .expect("nested artifact directories must exist");
    fs::write(artifact.join("root.bin"), vec![1_u8; 17]).expect("root artifact must exist");
    fs::write(artifact.join("debug/output.bin"), vec![2_u8; 23])
        .expect("nested artifact must exist");
    fs::write(artifact.join("debug/deps/library.bin"), vec![3_u8; 31])
        .expect("deep artifact must exist");

    let portable =
        portable_measure_directory_with_progress(&artifact, &|| false, &|_| {}, &|_, _, _| {});
    let optimized = measure_directory(&artifact, &|| false);

    assert_eq!(optimized.measured.bytes, portable.measured.bytes);
    assert_eq!(optimized.measured.file_count, portable.measured.file_count);
    assert_eq!(
        optimized.measured.skipped_count,
        portable.measured.skipped_count
    );
    assert_eq!(optimized.modified_at_ms, portable.modified_at_ms);
}

#[cfg(any(unix, windows))]
#[test]
fn incomplete_artifact_measurement_preserves_visible_accessible_bytes() {
    assert_incomplete_artifact_diagnostics(64);
}

#[cfg(any(unix, windows))]
#[test]
fn unreadable_artifact_without_visible_bytes_still_reports_failure() {
    assert_incomplete_artifact_diagnostics(0);
}

#[cfg(any(unix, windows))]
fn assert_incomplete_artifact_diagnostics(visible_bytes: usize) {
    let fixture = Fixture::new("partial-measurement");
    let project = fixture.0.join("app");
    let artifact = project.join("target");
    let restricted = artifact.join("restricted");
    fs::create_dir_all(&restricted).expect("restricted directory must exist");
    fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n")
        .expect("Cargo marker must exist");
    if visible_bytes > 0 {
        fs::write(artifact.join("visible.bin"), vec![1_u8; visible_bytes])
            .expect("visible artifact must exist");
    }
    fs::write(restricted.join("hidden.bin"), vec![2_u8; 32])
        .expect("restricted artifact must exist");
    let denied_reads = DeniedDirectoryReads::new(&restricted);

    let native = measure_directory_with_progress(&artifact, &|| false, &|_| {}, &|_, _, _| {});
    let portable =
        portable_measure_directory_with_progress(&artifact, &|| false, &|_| {}, &|_, _, _| {});
    let roots = vec![fixture.0.to_string_lossy().into_owned()];
    let rules = preview_all(
        &roots,
        false,
        &|| false,
        &|_| {},
        &|_, _, _| {},
        &CleanupExclusions::default(),
    );
    drop(denied_reads);
    let rust = rules
        .rules
        .iter()
        .find(|rule| rule.rule_id == "project.rust-build-artifacts")
        .expect("Rust artifact rule must exist");

    assert_eq!(native.measured.read_failures.count, 1);
    let normalized_failures = |failures: &mangodisk_platform::FileReadFailures| {
        let mut normalized = failures.clone();
        // Native traversal can resolve macOS /var aliases; compare the same filesystem objects.
        for detail in &mut normalized.details {
            // A denied directory may fail during opening or enumeration depending on
            // the native API. Both must preserve the same object and denial reason.
            assert!(matches!(
                detail.stage,
                mangodisk_platform::FileReadStage::OpenDirectory
                    | mangodisk_platform::FileReadStage::ReadDirectory
            ));
            detail.stage = mangodisk_platform::FileReadStage::ReadDirectory;
            detail.path = fs::canonicalize(&detail.path)
                .expect("restored directory must resolve")
                .to_string_lossy()
                .into_owned();
        }
        normalized
    };
    assert_eq!(
        normalized_failures(&rules.read_failures),
        normalized_failures(&native.measured.read_failures)
    );
    assert_eq!(
        normalized_failures(&portable.measured.read_failures),
        normalized_failures(&native.measured.read_failures)
    );
    assert_eq!(portable.measured.read_failures.count, 1);
    assert_eq!(rust.status, ScanItemStatus::Limited);
    assert!(!rust.selectable);
    assert_eq!(rust.bytes, visible_bytes as u64);
    assert_eq!(rust.file_count, u64::from(visible_bytes > 0));
    assert_eq!(rust.sources.len(), 1);
    assert_eq!(
        rust.sources[0].block_reason,
        Some(crate::cleanup::CleanupSourceBlockReason::IncompleteMeasurement)
    );
}

#[cfg(any(unix, windows))]
#[test]
fn incomplete_candidate_does_not_block_complete_sibling_projects() {
    let _operation_lock = test_operation_lock();
    let fixture = Fixture::new("mixed-measurement");
    for project_name in ["complete", "limited"] {
        let project = fixture.0.join(project_name);
        fs::create_dir_all(project.join("target")).expect("artifact directory must exist");
        fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n")
            .expect("Cargo marker must exist");
        fs::write(project.join("target/output.bin"), vec![1_u8; 64])
            .expect("artifact file must exist");
    }
    let restricted = fixture.0.join("limited/target/restricted");
    fs::create_dir_all(&restricted).expect("restricted directory must exist");
    let denied_reads = DeniedDirectoryReads::new(&restricted);

    let roots = vec![fixture.0.to_string_lossy().into_owned()];
    let rules = preview_all(
        &roots,
        false,
        &|| false,
        &|_| {},
        &|_, _, _| {},
        &CleanupExclusions::default(),
    );
    let operation =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("operation must start");
    let actions = execute_selected(
        &["project.rust-build-artifacts".to_string()],
        &roots,
        &SourceSelectionPolicy::empty(),
        true,
        &operation,
    );
    drop(denied_reads);
    let rust = rules
        .rules
        .iter()
        .find(|rule| rule.rule_id == "project.rust-build-artifacts")
        .expect("Rust artifact rule must exist");

    assert_eq!(rules.read_failures.count, 1);
    assert_eq!(rules.read_failures.permission_denied_count, 1);
    assert_eq!(rust.status, ScanItemStatus::Found);
    assert!(rust.selectable);
    assert_eq!(rust.bytes, 64);
    assert_eq!(rust.sources.len(), 2);
    assert_eq!(
        rust.sources
            .iter()
            .filter(|source| source.block_reason.is_some())
            .count(),
        1
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].status, CleanupActionStatus::Previewed);
    assert_eq!(actions[0].bytes_expected, 64);
}

#[cfg(unix)]
#[test]
fn artifact_measurement_counts_links_without_following_targets() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("symlink-measurement");
    let artifact = fixture.0.join("node_modules");
    let external = fixture.0.join("external");
    fs::create_dir_all(&artifact).expect("artifact directory must exist");
    fs::create_dir_all(&external).expect("external directory must exist");
    fs::write(external.join("preserved.bin"), vec![1_u8; 4096]).expect("external file must exist");
    symlink(&external, artifact.join("linked-package")).expect("dependency link must exist");

    let measured = measure_directory(&artifact, &|| false);

    assert_eq!(measured.measured.file_count, 1);
    assert_eq!(measured.measured.skipped_count, 0);
    assert!(measured.measured.bytes < 4096);
}

#[test]
fn dry_run_preserves_artifacts_and_real_execution_preserves_sources() {
    let _operation_lock = test_operation_lock();
    let fixture = Fixture::new("execution");
    let project = fixture.0.join("app");
    fs::create_dir_all(project.join("target/debug")).expect("target must exist");
    fs::create_dir_all(project.join("src")).expect("source must exist");
    fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n")
        .expect("Cargo marker must exist");
    fs::write(project.join("src/main.rs"), "fn main() {}").expect("source must exist");
    fs::write(project.join("target/debug/app"), vec![3_u8; 256]).expect("artifact must exist");
    let roots = vec![fixture.0.to_string_lossy().into_owned()];
    let id = "project.rust-build-artifacts".to_string();

    let dry_run =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("operation must start");
    let source_selections = SourceSelectionPolicy::empty();
    let actions = execute_selected(
        std::slice::from_ref(&id),
        &roots,
        &source_selections,
        true,
        &dry_run,
    );
    dry_run.complete();
    drop(dry_run);
    assert_eq!(actions[0].status, CleanupActionStatus::Previewed);
    assert!(project.join("target").exists());

    let cleanup =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("operation must start");
    let actions = execute_selected(&[id], &roots, &source_selections, false, &cleanup);
    cleanup.complete();
    assert_eq!(actions[0].status, CleanupActionStatus::Completed);
    assert_eq!(actions[0].released_bytes, 256);
    assert!(!project.join("target").exists());
    assert!(project.join("Cargo.toml").exists());
    assert!(project.join("src/main.rs").exists());
}

#[test]
fn source_selection_removes_only_the_selected_project_artifact() {
    let _operation_lock = test_operation_lock();
    let fixture = Fixture::new("source-selection");
    let selected_project = fixture.0.join("selected");
    let preserved_project = fixture.0.join("preserved");
    for project in [&selected_project, &preserved_project] {
        fs::create_dir_all(project.join("target/debug")).expect("target must exist");
        fs::write(project.join("Cargo.toml"), "[package]\nname='fixture'\n")
            .expect("Cargo marker must exist");
        fs::write(project.join("target/debug/app"), vec![7_u8; 128]).expect("artifact must exist");
    }

    let rule_id = "project.rust-build-artifacts".to_string();
    let roots = vec![fixture.0.to_string_lossy().into_owned()];
    let plan = build_plan(
        &roots,
        false,
        current_platform_rules().expect("catalog must load"),
        &|| false,
        &CleanupExclusions::default(),
    )
    .expect("the isolated source plan must build");
    let rule = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == rule_id)
        .expect("the Rust artifact rule must exist");
    assert_eq!(rule.candidates.len(), 2);
    let selected_source_path = rule.candidates[0].path.clone();
    let preserved_source_path = rule.candidates[1].path.clone();
    let selected_source = selected_source_path.to_string_lossy().into_owned();
    let selected_rule_ids = HashSet::from([rule_id.clone()]);
    let source_selections = SourceSelectionPolicy::from_request(
        &selected_rule_ids,
        &[CleanupSourceSelection {
            rule_id: rule_id.clone(),
            mode: CleanupSourceSelectionMode::Include,
            paths: vec![selected_source],
        }],
    )
    .expect("the isolated source selection must be valid");
    let operation =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("cleanup must start");

    let actions = execute_selected(&[rule_id], &roots, &source_selections, false, &operation);
    operation.complete();

    assert_eq!(
        actions[0].status,
        CleanupActionStatus::Completed,
        "{:?}",
        actions[0]
    );
    assert_eq!(actions[0].affected_item_count, 1);
    assert!(!selected_source_path.exists());
    assert!(
        preserved_source_path.exists(),
        "a sibling source outside the include selection must be preserved"
    );
}

#[test]
fn descendant_python_caches_are_detected_and_deduplicated() {
    let fixture = Fixture::new("python");
    let project = fixture.0.join("python-app");
    let cache = project.join("src/package/__pycache__");
    fs::create_dir_all(&cache).expect("cache must exist");
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname='fixture'\n",
    )
    .expect("Python marker must exist");
    fs::write(cache.join("module.pyc"), vec![4_u8; 96]).expect("cache file must exist");

    let rules = current_platform_rules().expect("catalog must load");
    let plan = build_plan(
        &[fixture.0.to_string_lossy().into_owned()],
        false,
        rules,
        &|| false,
        &CleanupExclusions::default(),
    )
    .expect("plan must build");
    let python = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.python-build-artifacts")
        .expect("Python rule must exist");
    assert_eq!(python.candidates.len(), 1);
    assert_eq!(python.candidates[0].bytes, 96);
}

#[test]
fn cached_projects_skip_recursive_artifacts_but_keep_direct_artifacts() {
    let fixture = Fixture::new("cached-python");
    let project = fixture.0.join("python-app");
    let direct_cache = project.join(".pytest_cache");
    let descendant_cache = project.join("src/package/__pycache__");
    fs::create_dir_all(&direct_cache).expect("direct cache must exist");
    fs::create_dir_all(&descendant_cache).expect("descendant cache must exist");
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname='fixture'\n",
    )
    .expect("Python marker must exist");

    let rules = current_platform_rules().expect("catalog must load");
    let rule_index = rules
        .iter()
        .position(|rule| rule.id == "project.python-build-artifacts")
        .expect("Python rule must exist");
    let project = project
        .canonicalize()
        .expect("cached project root must be canonical");
    let direct_cache = direct_cache
        .canonicalize()
        .expect("direct cache must be canonical");
    let descendant_cache = descendant_cache
        .canonicalize()
        .expect("descendant cache must be canonical");
    let projects = vec![ProjectMatch {
        rule_index,
        project_root: project,
        allow_descendant_scan: false,
    }];
    let drafts = collect_artifact_drafts(
        &projects,
        rules,
        &|| false,
        &|_| {},
        &CleanupExclusions::default(),
    );

    assert!(drafts.iter().any(|draft| draft.path == direct_cache));
    assert!(!drafts.iter().any(|draft| draft.path == descendant_cache));
}

#[test]
fn discovers_extended_ecosystem_artifacts_from_strong_project_markers() {
    let _operation_lock = test_operation_lock();
    let fixture = Fixture::new("extended-ecosystems");
    let cases = [
        (
            "autoconf",
            "configure.ac",
            "autom4te.cache",
            "project.autoconf-cache",
            41_u64,
        ),
        (
            "clojure",
            "deps.edn",
            ".cpcache",
            "project.clojure-cli-cache",
            42,
        ),
        (
            "dune",
            "dune-project",
            "_build",
            "project.dune-build-artifacts",
            43,
        ),
        (
            "leiningen",
            "project.clj",
            "target",
            "project.leiningen-build-artifacts",
            44,
        ),
        (
            "meson",
            "meson.build",
            "builddir",
            "project.meson-build-artifacts",
            45,
        ),
        (
            "mill",
            "build.mill",
            "out",
            "project.mill-build-artifacts",
            46,
        ),
        (
            "pants",
            "pants.toml",
            ".pants.d",
            "project.pants-workdir",
            47,
        ),
        (
            "rebar",
            "rebar.config",
            "_build",
            "project.rebar-build-artifacts",
            48,
        ),
    ];

    for (directory, marker, artifact, _, bytes) in cases {
        let project = fixture.0.join(directory);
        fs::create_dir_all(project.join(artifact)).expect("artifact directory must exist");
        fs::write(project.join(marker), "").expect("project marker must exist");
        fs::write(
            project.join(artifact).join("generated.bin"),
            vec![1_u8; bytes as usize],
        )
        .expect("generated artifact must exist");
    }

    let rails = fixture.0.join("rails");
    fs::create_dir_all(rails.join("config")).expect("Rails config directory must exist");
    fs::create_dir_all(rails.join("tmp/cache")).expect("Rails cache directory must exist");
    fs::write(rails.join("Gemfile"), "").expect("Gemfile must exist");
    fs::write(rails.join("config/application.rb"), "").expect("Rails marker must exist");
    fs::write(rails.join("config/environment.rb"), "").expect("Rails environment must exist");
    fs::write(rails.join("tmp/cache/generated.bin"), vec![1_u8; 49])
        .expect("Rails cache artifact must exist");

    let rules = current_platform_rules().expect("catalog must load");
    let plan = build_plan(
        &[fixture.0.to_string_lossy().into_owned()],
        false,
        rules,
        &|| false,
        &CleanupExclusions::default(),
    )
    .expect("plan must build");
    for (_, _, _, rule_id, bytes) in cases {
        let rule = plan
            .rules
            .iter()
            .find(|rule| rule.source.id == rule_id)
            .expect("extended ecosystem rule must exist");
        assert_eq!(rule.candidates.len(), 1, "{rule_id}");
        assert_eq!(rule.candidates[0].bytes, bytes, "{rule_id}");
    }
    let rails_rule = plan
        .rules
        .iter()
        .find(|rule| rule.source.id == "project.rails-cache")
        .expect("Rails rule must exist");
    assert_eq!(rails_rule.candidates.len(), 1);
    assert_eq!(rails_rule.candidates[0].bytes, 49);

    let roots = vec![fixture.0.to_string_lossy().into_owned()];
    let selected_ids = cases
        .iter()
        .map(|(_, _, _, rule_id, _)| (*rule_id).to_string())
        .chain(std::iter::once("project.rails-cache".to_string()))
        .collect::<Vec<_>>();
    let preview =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("preview must start");
    let source_selections = SourceSelectionPolicy::empty();
    let mut progress_boundaries = Vec::new();
    let preview_actions = execute_selected_with_progress(
        ProjectExecutionRequest {
            selected_ids: &selected_ids,
            configured_roots: &roots,
            selected_volume_scope: false,
            source_selections: &source_selections,
            dry_run: true,
            operation: &preview,
            exclusions: &CleanupExclusions::default(),
        },
        |rule_id, action| progress_boundaries.push((rule_id.to_string(), action.is_some())),
    );
    preview.complete();
    drop(preview);
    let expected_boundaries = selected_ids
        .iter()
        .flat_map(|rule_id| [(rule_id.clone(), false), (rule_id.clone(), true)])
        .collect::<Vec<_>>();
    assert_eq!(progress_boundaries, expected_boundaries);
    assert!(preview_actions
        .iter()
        .all(|action| action.status == CleanupActionStatus::Previewed));
    for (directory, _, artifact, _, _) in cases {
        assert!(fixture.0.join(directory).join(artifact).exists());
    }
    assert!(rails.join("tmp/cache").exists());

    let cleanup =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("cleanup must start");
    let cleanup_actions =
        execute_selected(&selected_ids, &roots, &source_selections, false, &cleanup);
    cleanup.complete();
    assert!(cleanup_actions
        .iter()
        .all(|action| action.status == CleanupActionStatus::Completed));
    for (directory, marker, artifact, _, _) in cases {
        let project = fixture.0.join(directory);
        assert!(!project.join(artifact).exists());
        assert!(project.join(marker).exists());
    }
    assert!(!rails.join("tmp/cache").exists());
    assert!(rails.join("Gemfile").exists());
    assert!(rails.join("config/application.rb").exists());
    assert!(rails.join("config/environment.rb").exists());
}

#[test]
fn embedded_catalog_is_valid_and_stable() {
    let rules = current_platform_rules().expect("catalog must load");
    assert!(rules.len() >= 30);
    assert!(contains("project.rust-build-artifacts"));
    assert!(!contains("project.unknown"));
    assert_eq!(catalog_digest().len(), 64);
}

#[test]
fn artifact_pruning_preserves_case_sensitive_directory_names() {
    let rules = current_platform_rules().expect("catalog must load");
    let names = artifact_prune_names(rules);
    assert!(names.contains("Library"));
    #[cfg(target_os = "macos")]
    assert!(names.contains("Pods"));
    #[cfg(windows)]
    assert!(path_name_eq("library", "Library"));
}

#[cfg(windows)]
#[test]
fn windows_project_and_artifact_aliases_share_one_path_identity() {
    let display_project = PathBuf::from(r"C:\Users\Developer\Projects\MangoDisk");
    let canonical_project = PathBuf::from(r"\\?\C:\Users\Developer\Projects\MangoDisk");
    let display_artifact = display_project.join("target");
    let canonical_artifact = canonical_project.join("target");
    let mut projects = vec![
        ProjectMatch {
            rule_index: 0,
            project_root: display_project.clone(),
            allow_descendant_scan: false,
        },
        ProjectMatch {
            rule_index: 0,
            project_root: canonical_project.clone(),
            allow_descendant_scan: true,
        },
    ];

    sort_and_deduplicate_project_matches(&mut projects);

    assert_eq!(projects.len(), 1);
    assert!(projects[0].allow_descendant_scan);
    let drafts = deduplicate_artifacts(vec![
        ArtifactDraft {
            rule_index: 0,
            project_root: display_project,
            path: display_artifact,
        },
        ArtifactDraft {
            rule_index: 0,
            project_root: canonical_project,
            path: canonical_artifact,
        },
    ]);
    assert_eq!(drafts.len(), 1);
}

#[test]
fn artifact_deduplication_retains_many_sibling_artifacts() {
    let project_root = PathBuf::from("/fixture/projects");
    let drafts = (0..4_096)
        .map(|index| ArtifactDraft {
            rule_index: 0,
            project_root: project_root.clone(),
            path: project_root.join(format!("project-{index}/target")),
        })
        .collect::<Vec<_>>();

    let deduplicated = deduplicate_artifacts(drafts);

    assert_eq!(deduplicated.len(), 4_096);
}

#[test]
#[ignore = "scans the current MangoDisk repository and its real build artifacts"]
fn real_repository_preview_and_dry_run_are_read_only() {
    let _operation_lock = test_operation_lock();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate must be inside the MangoDisk repository")
        .to_path_buf();
    let roots = vec![repository.to_string_lossy().into_owned()];
    let rules = preview_all(
        &roots,
        false,
        &|| false,
        &|_| {},
        &|_, _, _| {},
        &CleanupExclusions::default(),
    );
    let detected = rules
        .rules
        .iter()
        .filter(|rule| rule.bytes > 0)
        .collect::<Vec<_>>();
    assert!(
        detected
            .iter()
            .any(|rule| rule.rule_id == "project.rust-build-artifacts"),
        "the repository target directory must be detected"
    );

    let operation =
        OperationGuard::start(CoordinatedOperationKind::Cleanup).expect("operation must start");
    let selected_ids = detected
        .iter()
        .filter(|rule| rule.selectable)
        .map(|rule| rule.rule_id.clone())
        .collect::<Vec<_>>();
    let source_selections = SourceSelectionPolicy::empty();
    let actions = execute_selected(&selected_ids, &roots, &source_selections, true, &operation);
    operation.complete();
    assert!(actions
        .iter()
        .all(|action| action.status == CleanupActionStatus::Previewed));
    assert!(repository.join("Cargo.toml").exists());
    assert!(repository.join("target").exists());
    println!(
            "Real project preview: detected_rules={}, detected_bytes={}, files={}, limited_source_bytes={}",
            detected.len(),
            detected.iter().map(|rule| rule.bytes).sum::<u64>(),
            detected.iter().map(|rule| rule.file_count).sum::<u64>(),
            detected
                .iter()
                .flat_map(|rule| &rule.sources)
                .filter(|source| source.block_reason.is_some())
                .map(|source| source.bytes)
                .sum::<u64>()
        );
}
