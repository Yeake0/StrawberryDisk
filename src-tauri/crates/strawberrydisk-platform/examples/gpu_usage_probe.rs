//! Native GPU observations for device comparison and reader lifecycle verification.
use std::time::{Duration, Instant};
use strawberrydisk_platform::system_resources::gpu::{GpuReader, GpuSample};

struct ProbeLogger;
impl log::Log for ProbeLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}
static LOGGER: ProbeLogger = ProbeLogger;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    // Keep diagnostics on stderr so the observation protocol stays machine-readable.
    if args.iter().any(|arg| arg == "--diagnostics") {
        log::set_logger(&LOGGER).expect("probe logger initialization");
        log::set_max_level(log::LevelFilter::Info);
    }
    let seconds: u64 = args.get(1).map(|arg| arg.parse().unwrap()).unwrap_or(60);
    assert!((10..=3600).contains(&seconds));
    let lifecycle = args.iter().any(|arg| arg == "--lifecycle");
    let catalogue = args.iter().any(|arg| arg == "--catalogue");
    let detailed = args.iter().any(|arg| arg == "--details");
    let mut reader = GpuReader::default();
    for tick in 0..seconds.div_ceil(2) {
        // Reset and reacquisition must prime Windows intervals instead of treating
        // elapsed disabled time as a valid sample. macOS has instantaneous values.
        if lifecycle && tick == 3 {
            reader.reset();
        }
        if lifecycle && tick == 6 {
            reader = GpuReader::default();
        }
        let started = Instant::now();
        let value = if catalogue {
            match reader.catalogue() {
                Ok(adapters) => serde_json::json!({"status": "catalogue", "adapters": adapters}),
                Err(error) => {
                    serde_json::json!({"status": "failed", "code": format!("{:?}", error.code()), "error": error.to_string()})
                }
            }
        } else {
            // Exercise a pending capability transition before reader release without
            // provoking a driver failure or changing system permissions.
            let collect_details = detailed && !(lifecycle && (4..=5).contains(&tick));
            match if collect_details {
                reader.read_detailed()
            } else {
                reader.read()
            } {
                Ok(GpuSample::Baseline) => serde_json::json!({ "status": "baseline" }),
                Ok(GpuSample::Usage(adapters)) => serde_json::json!({
                    "status": "ready", "adapters": adapters.iter().map(|adapter| serde_json::json!({
                        "id": adapter.id, "name": adapter.name, "usedPercent": adapter.used_percent, "details": adapter.details,
                    })).collect::<Vec<_>>()
                }),
                Err(error) => {
                    serde_json::json!({ "status": "failed", "code": format!("{:?}", error.code()), "error": error.to_string() })
                }
            }
        };
        println!(
            "{}",
            serde_json::json!({"schemaVersion": 1, "tick": tick, "elapsedMicros": started.elapsed().as_micros(), "reading": value})
        );
        std::thread::sleep(Duration::from_secs(2).saturating_sub(started.elapsed()));
    }
}
