//! Bounded capability and availability transitions, never per-sample measurements.
use super::details::{GpuActivityKind, GpuDetails, GpuMemoryStatus};
use crate::diagnostics::text;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const LOG_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Capabilities {
    engine_kind_mask: u16,
    standard_nodes: usize,
    custom_nodes: usize,
    core_count: bool,
    temperature_celsius: bool,
    core_clock_mhz: bool,
    memory_clock_mhz: bool,
    fan_percent: bool,
    memory_status: GpuMemoryStatus,
    dedicated_used_bytes: bool,
    dedicated_total_bytes: bool,
    shared_used_bytes: bool,
}
impl Capabilities {
    fn read(details: Option<&GpuDetails>) -> Option<Self> {
        let details = details?;
        let telemetry = &details.telemetry;
        let mut engines = 0;
        let mut standard_nodes = 0;
        let mut custom_nodes = 0;
        for activity in &details.activities {
            engines |= match activity.kind {
                GpuActivityKind::Graphics => 1,
                GpuActivityKind::Copy => 2,
                GpuActivityKind::VideoDecode => 4,
                GpuActivityKind::VideoEncode => 8,
                GpuActivityKind::VideoProcessing => 16,
                GpuActivityKind::Other => 32,
                GpuActivityKind::Renderer => 64,
                GpuActivityKind::Tiler => 128,
            };
            if activity.included_in_summary {
                standard_nodes += 1;
            } else if !matches!(
                activity.kind,
                GpuActivityKind::Renderer | GpuActivityKind::Tiler
            ) {
                custom_nodes += 1;
            }
        }
        Some(Self {
            engine_kind_mask: engines,
            standard_nodes,
            custom_nodes,
            core_count: telemetry.core_count.is_some(),
            temperature_celsius: telemetry.temperature_celsius.is_some(),
            core_clock_mhz: telemetry.core_clock_mhz.is_some(),
            memory_clock_mhz: telemetry.memory_clock_mhz.is_some(),
            fan_percent: telemetry.fan_percent.is_some(),
            memory_status: details.memory_status,
            dedicated_used_bytes: details
                .memory
                .as_ref()
                .is_some_and(|m| m.dedicated_used_bytes.is_some()),
            dedicated_total_bytes: details
                .memory
                .as_ref()
                .is_some_and(|m| m.dedicated_total_bytes.is_some()),
            shared_used_bytes: details
                .memory
                .as_ref()
                .is_some_and(|m| m.shared_used_bytes.is_some()),
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ready {
        source: &'static str,
        capabilities: Option<Capabilities>,
    },
    Unavailable {
        source: &'static str,
        stage: &'static str,
    },
}
struct Entry {
    state: State,
    logged_at: Instant,
    changes: u64,
    unavailable_stage: Option<&'static str>,
}
impl Entry {
    fn changed(&mut self, state: State, now: Instant) -> bool {
        if self.state != state {
            self.state = state;
            self.changes = self.changes.saturating_add(1);
            if let State::Unavailable { stage, .. } = state {
                self.unavailable_stage = Some(stage);
            }
        }
        self.changes > 0 && now.duration_since(self.logged_at) >= LOG_INTERVAL
    }
}
#[derive(Default)]
pub(super) struct ObservationDiagnostics {
    entries: HashMap<String, Entry>,
}
impl ObservationDiagnostics {
    pub(super) fn retain(&mut self, mut present: impl FnMut(&str) -> bool) {
        self.entries.retain(|id, entry| {
            if present(id) {
                true
            } else {
                log_pending(id, entry, "adapter_removed");
                false
            }
        });
    }
    pub(super) fn observe(
        &mut self,
        id: &str,
        source: &'static str,
        result: Result<Option<&GpuDetails>, &'static str>,
    ) {
        let state = match result {
            Ok(details) => State::Ready {
                source,
                capabilities: Capabilities::read(details),
            },
            Err(stage) => State::Unavailable { source, stage },
        };
        let now = Instant::now();
        if let Some(entry) = self.entries.get_mut(id) {
            if !entry.changed(state, now) {
                return;
            }
            log_pending(id, entry, "interval");
            entry.logged_at = now;
        } else {
            log::info!(
                "gpu_observation_started adapter={} observation={state:?} min_interval_seconds=30",
                text(id)
            );
            self.entries.insert(
                id.into(),
                Entry {
                    state,
                    logged_at: now,
                    changes: 0,
                    unavailable_stage: None,
                },
            );
        }
    }
}
// Lifecycle boundaries must flush suppressed transitions before their owner disappears.
// This final summary can precede the next periodic deadline, but never repeats an emitted change.
fn log_pending(id: &str, entry: &mut Entry, trigger: &'static str) {
    if entry.changes == 0 {
        return;
    }
    log::info!("gpu_observation_changed adapter={} observation={:?} changes_since_log={} last_unavailable_stage={:?} trigger={trigger}", text(id), entry.state, entry.changes, entry.unavailable_stage);
    entry.changes = 0;
    entry.unavailable_stage = None;
}
impl Drop for ObservationDiagnostics {
    fn drop(&mut self) {
        for (id, entry) in &mut self.entries {
            log_pending(id, entry, "reader_released");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transitions_are_bounded_and_short_outages_remain_in_the_next_summary() {
        let now = Instant::now();
        let ready = State::Ready {
            source: "test",
            capabilities: None,
        };
        let missing = State::Unavailable {
            source: "test",
            stage: "statistics_missing",
        };
        let mut entry = Entry {
            state: ready,
            logged_at: now,
            changes: 0,
            unavailable_stage: None,
        };
        assert!(!entry.changed(ready, now + LOG_INTERVAL));
        assert!(!entry.changed(missing, now + Duration::from_secs(1)));
        assert!(!entry.changed(ready, now + Duration::from_secs(2)));
        assert!(entry.changed(ready, now + LOG_INTERVAL));
        assert_eq!(entry.changes, 2);
        assert_eq!(entry.unavailable_stage, Some("statistics_missing"));
    }
    #[test]
    fn live_measurements_do_not_change_capability_diagnostics() {
        use super::super::details::{GpuActivity, GpuMemoryArchitecture, GpuTelemetry};
        let mut details = GpuDetails {
            activities: vec![GpuActivity {
                id: "renderer".into(),
                name: None,
                kind: GpuActivityKind::Renderer,
                used_percent: 0.0,
                included_in_summary: false,
            }],
            telemetry: GpuTelemetry {
                temperature_celsius: Some(40.0),
                ..Default::default()
            },
            memory_architecture: GpuMemoryArchitecture::Unified,
            memory_status: GpuMemoryStatus::Unsupported,
            memory: None,
        };
        let capabilities = Capabilities::read(Some(&details));
        details.activities[0].used_percent = 100.0;
        details.telemetry.temperature_celsius = Some(70.0);
        assert_eq!(Capabilities::read(Some(&details)), capabilities);
        details.telemetry.temperature_celsius = None;
        assert_ne!(Capabilities::read(Some(&details)), capabilities);
        assert_eq!(Capabilities::read(None), None);
    }
    struct LifecycleLogger;
    static LIFECYCLE_LOGGER: LifecycleLogger = LifecycleLogger;
    static LIFECYCLE_RECORDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    impl log::Log for LifecycleLogger {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() <= log::Level::Info
        }
        fn log(&self, record: &log::Record<'_>) {
            let message = record.args().to_string();
            if message.contains("lifecycle-repro") {
                LIFECYCLE_RECORDS.lock().unwrap().push(message);
            }
        }
        fn flush(&self) {}
    }
    #[test]
    fn pending_outage_diagnostics_survive_device_removal_and_reader_release() {
        log::set_logger(&LIFECYCLE_LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        for trigger in [
            "reader_released",
            "adapter_removed",
            "interval",
            "unchanged",
        ] {
            LIFECYCLE_RECORDS.lock().unwrap().clear();
            let mut diagnostics = ObservationDiagnostics::default();
            diagnostics.observe("lifecycle-repro", "test", Ok(None));
            if trigger != "unchanged" {
                diagnostics.observe("lifecycle-repro", "test", Err("statistics_missing"));
                diagnostics.observe("lifecycle-repro", "test", Ok(None));
            }
            if trigger == "interval" {
                diagnostics
                    .entries
                    .get_mut("lifecycle-repro")
                    .unwrap()
                    .logged_at -= LOG_INTERVAL;
                diagnostics.observe("lifecycle-repro", "test", Ok(None));
            }
            if trigger == "adapter_removed" {
                diagnostics.retain(|_| false);
            }
            drop(diagnostics);
            let records = LIFECYCLE_RECORDS.lock().unwrap();
            if trigger == "unchanged" {
                assert_eq!(
                    records.len(),
                    1,
                    "unchanged observations must not emit a release summary"
                );
                continue;
            }
            assert_eq!(
                records.len(),
                2,
                "the last suppressed outage must reach the logger before its state is dropped"
            );
            assert!(records[1].contains("changes_since_log=2"));
            assert!(records[1].contains("statistics_missing"));
            assert!(records[1].contains(&format!("trigger={trigger}")));
        }
    }
    #[test]
    fn removed_devices_do_not_accumulate_diagnostic_state() {
        let mut diagnostics = ObservationDiagnostics::default();
        diagnostics.observe("present", "test", Ok(None));
        diagnostics.observe("removed", "test", Err("missing"));
        diagnostics.retain(|id| id == "present");
        assert_eq!(diagnostics.entries.len(), 1);
    }
}
