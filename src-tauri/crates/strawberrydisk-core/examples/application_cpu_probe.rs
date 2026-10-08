//! Controlled live CPU workload: verify a busy application rises and then falls.
use std::time::{Duration, Instant};
use strawberrydisk_core::system_resources::process_cpu::ProcessCpuService;
fn main() {
    let ranking = std::env::args().any(|arg| arg == "--ranking");
    let mut service = ProcessCpuService::default();
    let started = Instant::now();
    service.sample(0).unwrap();
    for tick in 0..20 {
        let at = Instant::now();
        if !ranking && tick < 10 {
            while at.elapsed() < Duration::from_millis(900) {
                std::hint::black_box(
                    (0..1000).fold(1u64, |acc, n| acc.wrapping_mul(31).wrapping_add(n)),
                );
            }
        }
        std::thread::sleep(Duration::from_secs(1).saturating_sub(at.elapsed()));
        let value = service
            .sample(started.elapsed().as_millis() as u64)
            .unwrap()
            .unwrap();
        if ranking {
            println!(
                "{}",
                serde_json::json!({ "usageScale": value.usage_scale, "readableProcesses": value.readable_process_count,
                "omittedProcesses": value.omitted_process_count, "processes": value.applications.iter().map(|row| {
                    serde_json::json!({ "pid": row.pid, "name": row.application.name, "usedPercent": row.used_percent })
                }).collect::<Vec<_>>() })
            );
            continue;
        }
        let own = value
            .applications
            .iter()
            .enumerate()
            .find(|(_, row)| row.pid == std::process::id());
        println!(
            "{}",
            serde_json::json!({ "phase": if tick < 10 { "busy" } else { "idle" },
            "usedPercent": own.map(|(_, row)| row.used_percent).unwrap_or(0.0),
            "rank": own.map(|(index, _)| index + 1),
            "readableProcesses": value.readable_process_count, "omittedProcesses": value.omitted_process_count })
        );
    }
}
