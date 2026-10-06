//! Release preferences and scheduling policy shared by manual and automatic entry points.
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExcludedApplication {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleasePreferences {
    pub schema_version: u32,
    pub revision: u64,
    pub automatic: bool,
    pub interval_minutes: u32,
    pub threshold_percent: u8,
    pub skip_foreground: bool,
    pub exclusions: Vec<ExcludedApplication>,
}

impl Default for ReleasePreferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            automatic: false,
            interval_minutes: 15,
            threshold_percent: 80,
            skip_foreground: cfg!(windows),
            exclusions: vec![],
        }
    }
}

impl ReleasePreferences {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || ![3, 5, 15, 30, 60, 120].contains(&self.interval_minutes)
            || ![0, 70, 80, 90].contains(&self.threshold_percent)
            || self.exclusions.len() > 200
        {
            return Err("invalid_memory_release_preferences");
        }
        // The macOS native operation reclaims global volatile objects. Reject
        // process-level settings rather than silently ignoring their protection.
        if !cfg!(windows) && (self.skip_foreground || !self.exclusions.is_empty()) {
            return Err("memory_release_process_policy_unsupported");
        }
        let mut paths = std::collections::HashSet::new();
        for entry in &self.exclusions {
            if entry.name.trim().is_empty()
                || entry.name.len() > 512
                || entry.path.len() > 32768
                || entry.path.chars().any(char::is_control)
                || entry.name.chars().any(char::is_control)
                || !((entry.path.as_bytes().get(1) == Some(&b':')
                    && entry.path.as_bytes().get(2) == Some(&b'\\'))
                    || entry.path.starts_with("\\\\"))
                || !paths.insert(entry.path.to_lowercase())
            {
                return Err("invalid_memory_release_exclusion");
            }
        }
        Ok(())
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.interval_minutes) * 60)
    }
}

/// Reset after configuration changes, manual actions, or a suspended worker.
/// Missed intervals never accumulate; resuming a laptop must not trigger a release burst.
#[derive(Default)]
pub struct ReleaseSchedule {
    last_tick: Option<Duration>,
    next: Duration,
    generation: Option<(u64, u64)>,
}
impl ReleaseSchedule {
    pub fn due(&mut self, now: Duration, prefs: &ReleasePreferences, actions: u64) -> bool {
        let generation = (prefs.revision, actions);
        let interrupted = self
            .last_tick
            .is_none_or(|last| now < last || now.saturating_sub(last) > Duration::from_secs(30));
        self.last_tick = Some(now);
        if !prefs.automatic || interrupted || self.generation != Some(generation) {
            self.generation = Some(generation);
            self.next = now.saturating_add(prefs.interval());
            return false;
        }
        if now < self.next {
            return false;
        }
        self.next = now.saturating_add(prefs.interval());
        true
    }
}

/// List image identities independently of the top-memory ranking cutoff.
pub fn running_applications() -> crate::CoreResult<Vec<ExcludedApplication>> {
    use strawberrydisk_platform::system_resources::memory::{MemorySampler, MemorySource};
    let mut unique = std::collections::BTreeMap::new();
    for process in MemorySampler::default()
        .sample(true)?
        .processes
        .unwrap_or_default()
    {
        if let Some(path) = process.executable {
            let path = path.to_string_lossy().into_owned();
            if process.name.is_empty() {
                continue;
            }
            unique
                .entry(path.to_lowercase())
                .or_insert(ExcludedApplication {
                    name: process.name,
                    path,
                });
        }
    }
    let mut apps: Vec<_> = unique.into_values().collect();
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.path.cmp(&b.path))
    });
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_policies_are_rejected_when_native_reclaim_is_global() {
        let mut preferences = ReleasePreferences {
            skip_foreground: true,
            ..Default::default()
        };
        assert_eq!(preferences.validate().is_ok(), cfg!(windows));
        preferences.skip_foreground = false;
        preferences.exclusions.push(ExcludedApplication {
            name: "App".into(),
            path: r"C:\App.exe".into(),
        });
        assert_eq!(preferences.validate().is_ok(), cfg!(windows));
        preferences.exclusions.clear();
        assert!(preferences.validate().is_ok());
    }
    #[test]
    fn short_intervals_round_trip_and_wait_for_the_complete_deadline() {
        assert_eq!(ReleasePreferences::default().interval_minutes, 15);
        for minutes in [3, 5, 15, 30, 60, 120] {
            let p = ReleasePreferences {
                automatic: true,
                interval_minutes: minutes,
                ..Default::default()
            };
            assert!(p.validate().is_ok());
            let restored: ReleasePreferences =
                serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
            assert_eq!(restored, p);
            let mut schedule = ReleaseSchedule::default();
            let deadline = u64::from(minutes) * 60;
            for second in (0..deadline).step_by(5) {
                assert!(!schedule.due(Duration::from_secs(second), &p, 0));
            }
            assert!(schedule.due(Duration::from_secs(deadline), &p, 0));
            assert!(!schedule.due(Duration::from_secs(deadline + 5), &p, 0));
        }
    }
    #[test]
    fn rejects_unknown_versions_invalid_intervals_and_duplicate_paths() {
        let mut p = ReleasePreferences::default();
        assert!(p.validate().is_ok());
        p.interval_minutes = 1;
        assert!(p.validate().is_err());
        p.interval_minutes = 15;
        p.exclusions = vec![
            ExcludedApplication {
                name: "App".into(),
                path: r"C:\App.exe".into()
            };
            2
        ];
        assert!(p.validate().is_err());
        assert!(serde_json::from_str::<ReleasePreferences>(r#"{"schemaVersion":99}"#).is_err());
    }
    #[test]
    fn timer_waits_resets_on_manual_action_and_never_catches_up_after_sleep() {
        let mut schedule = ReleaseSchedule::default();
        let p = ReleasePreferences {
            automatic: true,
            interval_minutes: 15,
            ..Default::default()
        };
        for sec in 0..900 {
            assert!(!schedule.due(Duration::from_secs(sec), &p, 0));
        }
        assert!(schedule.due(Duration::from_secs(900), &p, 0));
        assert!(!schedule.due(Duration::from_secs(901), &p, 1));
        assert!(!schedule.due(Duration::from_secs(5000), &p, 1));
        for sec in 5001..5900 {
            assert!(!schedule.due(Duration::from_secs(sec), &p, 1));
        }
        assert!(schedule.due(Duration::from_secs(5900), &p, 1));
    }
}
