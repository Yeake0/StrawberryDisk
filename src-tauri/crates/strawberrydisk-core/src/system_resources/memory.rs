use strawberrydisk_platform::system_resources::memory::{NativeMemorySnapshot, ProcessMemory};

use super::application_identity;
use super::models::{ApplicationMemory, MemoryOverview, ProcessMemorySummary};
use crate::{applications::running_identity, CoreError, CoreResult};

pub(super) fn overview(raw: &NativeMemorySnapshot) -> CoreResult<MemoryOverview> {
    if raw.total_bytes == 0 {
        return Err(CoreError::operation_failed(
            "memory capacity is unavailable",
        ));
    }
    // Counters are sampled separately by the OS. Bound transient inconsistencies without
    // treating free memory as available memory or inventing a pressure classification.
    let used_bytes = raw.used_bytes.min(raw.total_bytes);
    Ok(MemoryOverview {
        total_bytes: raw.total_bytes,
        used_bytes,
        free_bytes: raw.free_bytes.min(raw.total_bytes),
        swap_used_bytes: raw.swap_used_bytes,
        used_percent: ((used_bytes as u128 * 100 + raw.total_bytes as u128 / 2)
            / raw.total_bytes as u128) as u8,
    })
}

pub(super) fn summarize(
    processes: Vec<ProcessMemory>,
    current_pid: u32,
    usage_kind: strawberrydisk_platform::system_resources::memory::ProcessMemoryKind,
) -> ProcessMemorySummary {
    let own_path = processes
        .iter()
        .find(|process| process.pid == current_pid)
        .and_then(|process| process.executable.as_ref())
        .map(|path| running_identity::application_path(path));
    let mut rows = Vec::new();
    let mut readable_process_count = 0;
    let mut omitted_process_count = 0;
    for process in processes {
        if process.name.trim().is_empty() {
            continue;
        }
        if process.used_bytes.is_some() {
            readable_process_count += 1;
        } else {
            omitted_process_count += 1;
        }
        let application = application_identity::identify(
            process.pid,
            process.name,
            process.executable.as_deref(),
            own_path.as_deref(),
        );
        let readable = u32::from(process.used_bytes.is_some());
        rows.push((
            application,
            (process.used_bytes, readable, process.is_application),
        ));
    }
    let mut applications = super::application_groups::aggregate(rows, |total, value| {
        // Preserve readable measurements and publish coverage beside the partial sum.
        total.0 = match (total.0, value.0) {
            (Some(a), Some(b)) => Some(a.saturating_add(b)),
            (a, b) => a.or(b),
        };
        total.1 += value.1;
        total.2 |= value.2;
    })
    .into_iter()
    .map(
        |(application, (used_bytes, readable_process_count, is_application))| {
            (
                ApplicationMemory {
                    application,
                    used_bytes,
                    readable_process_count,
                },
                is_application,
            )
        },
    )
    .collect::<Vec<_>>();
    // Retain running GUI applications whose counters are denied, so a busy VM cannot
    // silently disappear. They have no numeric rank; other unreadable rows use remaining slots.
    applications.sort_by(|(a, gui_a), (b, gui_b)| {
        let priority = |row: &ApplicationMemory, gui: bool| {
            if row.used_bytes.is_none() && gui {
                2
            } else if row.used_bytes.is_some() {
                1
            } else {
                0
            }
        };
        priority(b, *gui_b)
            .cmp(&priority(a, *gui_a))
            .then_with(|| b.used_bytes.cmp(&a.used_bytes))
            .then_with(|| a.application.name.cmp(&b.application.name))
            .then_with(|| a.application.id.cmp(&b.application.id))
    });
    applications.truncate(super::application_groups::RANKING_LIMIT);
    let mut applications = applications
        .into_iter()
        .map(|(row, _)| row)
        .collect::<Vec<_>>();
    applications.sort_by(|a, b| {
        b.used_bytes
            .cmp(&a.used_bytes)
            .then_with(|| a.application.name.cmp(&b.application.name))
            .then_with(|| a.application.id.cmp(&b.application.id))
    });
    ProcessMemorySummary {
        usage_kind,
        applications,
        readable_process_count,
        omitted_process_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn summarize(processes: Vec<ProcessMemory>, current_pid: u32) -> ProcessMemorySummary {
        super::summarize(
            processes,
            current_pid,
            strawberrydisk_platform::system_resources::memory::ProcessMemoryKind::native(),
        )
    }

    fn process(pid: u32, path: Option<&str>, bytes: u64) -> ProcessMemory {
        ProcessMemory {
            pid,
            name: "Helper".into(),
            executable: path.map(PathBuf::from),
            used_bytes: Some(bytes),
            is_application: false,
        }
    }

    #[test]
    fn partial_groups_keep_readable_bytes_and_report_coverage() {
        let mut known = (1..=60)
            .map(|pid| process(pid, None, pid as u64))
            .collect::<Vec<_>>();
        let mut denied = process(100, Some("/Applications/VM.app/Contents/MacOS/VM"), 1);
        denied.used_bytes = None;
        denied.is_application = true;
        known.push(denied.clone());
        known.push(process(
            101,
            Some("/Applications/VM.app/Contents/MacOS/VM"),
            10000,
        ));
        let result = summarize(known, u32::MAX);
        assert_eq!(result.applications.len(), 50);
        let vm = result
            .applications
            .iter()
            .find(|row| row.application.name == "VM")
            .unwrap();
        assert_eq!(vm.used_bytes, Some(10000));
        assert_eq!(vm.readable_process_count, 1);
        assert_eq!(vm.application.process_count, 2);
        assert_eq!(result.readable_process_count, 61);
        assert_eq!(result.omitted_process_count, 1);
        assert_eq!(result.applications[0].used_bytes, Some(10000));
    }

    #[test]
    fn unreadable_running_apps_survive_the_ranking_limit_without_invented_values() {
        let mut processes = (1..=60)
            .map(|pid| process(pid, None, pid as u64))
            .collect::<Vec<_>>();
        let mut denied = process(100, Some("/Applications/VM.app/Contents/MacOS/VM"), 1);
        denied.used_bytes = None;
        denied.is_application = true;
        processes.push(denied);
        let result = summarize(processes, u32::MAX);
        assert_eq!(result.applications.len(), 50);
        let vm = result
            .applications
            .iter()
            .find(|row| row.application.name == "VM")
            .unwrap();
        assert_eq!(vm.used_bytes, None);
        assert_eq!(vm.readable_process_count, 0);
    }

    #[test]
    fn bundles_aggregate_helpers_but_same_names_and_unknown_images_stay_separate() {
        let summary = summarize(vec![
            process(1, Some("/Applications/Browser.app/Contents/MacOS/Browser"), 10),
            process(2, Some("/Applications/Browser.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"), 20),
            process(3, Some("/elsewhere/Browser.app/Contents/MacOS/Browser"), 5),
            process(4, None, 4), process(5, None, 3), process(6, None, 0),
        ], u32::MAX);
        assert_eq!(summary.applications.len(), 5);
        assert_eq!(summary.applications[0].application.name, "Browser");
        assert_eq!(summary.applications[0].used_bytes, Some(30));
        assert_eq!(summary.applications[0].application.process_count, 2);
        assert_eq!(summary.readable_process_count, 6);
        assert_eq!(summary.omitted_process_count, 0);
    }

    #[test]
    fn own_bundle_helpers_and_unknown_images_cannot_offer_quit() {
        let summary = summarize(
            vec![
                process(
                    1,
                    Some("/StrawberryDisk.app/Contents/MacOS/StrawberryDisk"),
                    10,
                ),
                process(
                    2,
                    Some(
                        "/StrawberryDisk.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper",
                    ),
                    5,
                ),
                process(3, Some("/Browser.app/Contents/MacOS/Browser"), 20),
                process(4, None, 4),
                process(5, Some("/usr/bin/node"), 3),
            ],
            1,
        );
        assert!(
            summary
                .applications
                .iter()
                .find(|row| row.application.name == "Browser")
                .unwrap()
                .application
                .can_quit
        );
        assert!(summary
            .applications
            .iter()
            .filter(|row| row.application.name != "Browser")
            .all(|row| !row.application.can_quit));
    }

    #[test]
    fn bounded_ranking_is_deterministic_and_resists_overflow() {
        let inputs = (1..=100)
            .map(|id| process(id, None, id as u64))
            .collect::<Vec<_>>();
        let forward = summarize(inputs.clone(), u32::MAX);
        let reverse = summarize(inputs.into_iter().rev().collect(), u32::MAX);
        assert_eq!(forward.applications.len(), 50);
        assert_eq!(forward.applications[0].used_bytes, Some(100));
        assert_eq!(forward.applications[49].used_bytes, Some(51));
        assert_eq!(forward.readable_process_count, 100);
        assert_eq!(
            serde_json::to_value(forward).unwrap(),
            serde_json::to_value(reverse).unwrap()
        );
        let overflow = summarize(
            vec![
                process(1, Some("/app.exe"), u64::MAX),
                process(2, Some("/app.exe"), 1),
            ],
            u32::MAX,
        );
        assert_eq!(overflow.applications[0].used_bytes, Some(u64::MAX));
    }

    #[test]
    fn missing_capacity_is_not_zero_usage_and_racing_counters_are_bounded() {
        let mut raw = NativeMemorySnapshot {
            total_bytes: 0,
            used_bytes: 20,
            free_bytes: 30,
            swap_used_bytes: 5,
            process_memory_kind:
                strawberrydisk_platform::system_resources::memory::ProcessMemoryKind::native(),
            processes: None,
        };
        assert!(overview(&raw).is_err());
        raw.total_bytes = 10;
        let result = overview(&raw).unwrap();
        assert_eq!(result.used_percent, 100);
        assert_eq!(result.free_bytes, 10);
        raw.total_bytes = u64::MAX;
        raw.used_bytes = u64::MAX;
        assert_eq!(overview(&raw).unwrap().used_percent, 100);
    }
}
