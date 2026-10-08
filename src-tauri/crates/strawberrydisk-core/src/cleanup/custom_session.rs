use crate::filesystem::ScanExclusionOptions;
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, OnceLock,
    },
};

use super::CustomCleanupRule;
use crate::filesystem::permanent_delete::PhysicalPathIdentity;

const CUSTOM_CLEANUP_SESSION_LIMIT: usize = 8;
static NEXT_CUSTOM_CLEANUP_SCAN_ID: AtomicU64 = AtomicU64::new(1);
static CUSTOM_CLEANUP_SESSIONS: OnceLock<Mutex<VecDeque<CustomCleanupSession>>> = OnceLock::new();

struct CustomCleanupSession {
    scan_id: u64,
    rules: Vec<CustomCleanupRule>,
    effective_rules: Vec<CustomCleanupRule>,
    include_standard_rules: bool,
    exclusion_options: ScanExclusionOptions,
    empty_directory_authorizations: Arc<EmptyDirectoryAuthorizations>,
}

pub(super) type EmptyDirectoryAuthorizations =
    HashMap<String, HashMap<PathBuf, PhysicalPathIdentity>>;

pub(super) struct ResolvedCustomCleanupSession {
    pub(super) rules: Vec<CustomCleanupRule>,
    pub(super) empty_directory_authorizations: Arc<EmptyDirectoryAuthorizations>,
}

fn sessions() -> &'static Mutex<VecDeque<CustomCleanupSession>> {
    CUSTOM_CLEANUP_SESSIONS.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn lock_sessions() -> Result<MutexGuard<'static, VecDeque<CustomCleanupSession>>, String> {
    sessions()
        .lock()
        .map_err(|_| "the custom cleanup result session is unavailable".to_string())
}

pub(super) fn publish(
    rules: Vec<CustomCleanupRule>,
    effective_rules: Vec<CustomCleanupRule>,
    include_standard_rules: bool,
    exclusion_options: impl Into<ScanExclusionOptions>,
    empty_directory_authorizations: EmptyDirectoryAuthorizations,
) -> Result<u64, String> {
    let exclusion_options = exclusion_options.into();
    let scan_id = NEXT_CUSTOM_CLEANUP_SCAN_ID.fetch_add(1, Ordering::Relaxed);
    let rule_count = rules.len();
    let empty_directory_count = empty_directory_authorizations
        .values()
        .map(HashMap::len)
        .sum::<usize>();
    let mut sessions = lock_sessions()?;
    sessions.push_front(CustomCleanupSession {
        scan_id,
        rules,
        effective_rules,
        include_standard_rules,
        exclusion_options,
        empty_directory_authorizations: Arc::new(empty_directory_authorizations),
    });
    sessions.truncate(CUSTOM_CLEANUP_SESSION_LIMIT);
    log::info!(
        "custom_cleanup_session_published scan_id={scan_id} rule_count={rule_count} include_standard_rules={include_standard_rules} empty_directory_count={empty_directory_count}"
    );
    Ok(scan_id)
}

/// Resolves the exact rules retained by Core after a successful custom scan.
///
/// The WebView can retain names for presentation, but it cannot substitute a
/// different directory or matcher when cleanup begins.
pub(super) fn resolve(
    scan_id: u64,
    requested_rules: &[CustomCleanupRule],
    include_standard_rules: bool,
    exclusion_options: &ScanExclusionOptions,
    validate_exclusions: bool,
) -> Result<ResolvedCustomCleanupSession, String> {
    let sessions = lock_sessions()?;
    let Some(session) = sessions.iter().find(|session| session.scan_id == scan_id) else {
        log::warn!("custom_cleanup_session_resolution_failed scan_id={scan_id} reason=notFound");
        return Err("the custom cleanup result expired; scan again".to_string());
    };
    if session.rules != requested_rules {
        log::warn!(
            "custom_cleanup_session_resolution_failed scan_id={} reason=rulesChanged expected_rule_count={} requested_rule_count={}",
            scan_id,
            session.rules.len(),
            requested_rules.len()
        );
        return Err("the custom cleanup rules no longer match the scan result".to_string());
    }
    if session.include_standard_rules != include_standard_rules {
        log::warn!(
            "custom_cleanup_session_resolution_failed scan_id={scan_id} reason=scopeChanged expected_include_standard_rules={} requested_include_standard_rules={include_standard_rules}",
            session.include_standard_rules
        );
        return Err("the custom cleanup scope no longer matches the scan result".to_string());
    }
    if session.exclusion_options.names != exclusion_options.names
        || (validate_exclusions && session.exclusion_options.paths != exclusion_options.paths)
    {
        log::warn!(
            "custom_cleanup_session_resolution_failed scan_id={} reason=exclusionsChanged expected_count={} requested_count={}",
            scan_id,
            session.exclusion_options.len(),
            exclusion_options.len()
        );
        return Err("the cleanup exclusions no longer match the scan result".to_string());
    }
    Ok(ResolvedCustomCleanupSession {
        // Missing roots may reappear after scanning. They remain unauthorized
        // until a new scan, even though the saved preferences still include them.
        rules: session.effective_rules.clone(),
        empty_directory_authorizations: Arc::clone(&session.empty_directory_authorizations),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleanup::{CustomCleanupModifiedTime, CUSTOM_CLEANUP_RULE_SCHEMA_VERSION};

    fn rule(root: &str) -> CustomCleanupRule {
        CustomCleanupRule {
            schema_version: CUSTOM_CLEANUP_RULE_SCHEMA_VERSION,
            id: "fixture-rule".to_string(),
            name: "Fixture files".to_string(),
            roots: vec![root.to_string()],
            name_patterns: vec!["*.tmp".to_string()],
            minimum_bytes: None,
            maximum_bytes: None,
            modified_time: CustomCleanupModifiedTime::Any,
            recursive: true,
            remove_empty_directories: false,
        }
    }

    #[test]
    fn execution_rules_must_match_the_authoritative_scan_session() {
        let rules = vec![rule("/fixture")];
        let scan_id = publish(
            rules.clone(),
            rules.clone(),
            false,
            Vec::new(),
            HashMap::new(),
        )
        .expect("publish the custom cleanup session");

        assert_eq!(
            resolve(
                scan_id,
                &rules,
                false,
                &ScanExclusionOptions::default(),
                false
            )
            .expect("resolve the matching custom cleanup session")
            .rules,
            rules
        );
        assert!(resolve(
            scan_id,
            &[rule("/different")],
            false,
            &ScanExclusionOptions::default(),
            false
        )
        .is_err());
        assert!(resolve(
            scan_id,
            &rules,
            true,
            &ScanExclusionOptions::default(),
            false
        )
        .is_err());
        assert!(resolve(
            scan_id,
            &rules,
            false,
            &vec!["/excluded".to_string()].into(),
            true
        )
        .is_err());
        assert!(resolve(
            scan_id,
            &rules,
            false,
            &vec!["/excluded".to_string()].into(),
            false
        )
        .is_ok());
        assert!(resolve(
            u64::MAX,
            &rules,
            false,
            &ScanExclusionOptions::default(),
            false
        )
        .is_err());
    }

    #[test]
    fn unrelated_custom_selection_does_not_depend_on_project_exclusions() {
        let rules = vec![rule("/fixture")];
        let scan_id = publish(
            rules.clone(),
            rules.clone(),
            true,
            vec!["/previous-exclusion".to_string()],
            HashMap::new(),
        )
        .expect("publish a mixed cleanup session");

        assert!(resolve(
            scan_id,
            &rules,
            true,
            &ScanExclusionOptions::default(),
            false
        )
        .is_ok());
        assert!(resolve(
            scan_id,
            &rules,
            true,
            &ScanExclusionOptions::default(),
            true
        )
        .is_err());
    }
}
