//! Aggregate capacity parity and latency, without collecting file contents or volume IDs.
//! Run with `cargo run -p mangodisk-platform --example volume_capacity_probe`.
use mangodisk_platform::{current_platform, system_identity, system_resources::disk, Platform};
use std::time::Instant;

fn measure(name: &str, count: usize, mut query: impl FnMut()) {
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        let started = Instant::now();
        query();
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({
            "query": name, "iterations": count, "medianMs": samples[count / 2],
            "p95Ms": samples[count * 95 / 100], "maxMs": samples[count - 1]
        })
    );
}

fn main() {
    let identity = system_identity::current();
    println!(
        "{}",
        serde_json::json!({"os": identity.operating_system, "version": identity.version, "architecture": identity.architecture})
    );
    let platform = current_platform();
    let system = platform
        .system_volume()
        .expect("system volume must be readable");
    let volumes = disk::list().expect("resident volumes must be readable");
    let volume = volumes
        .iter()
        .find(|volume| volume.system)
        .expect("system volume must be identifiable");
    let resident = disk::capacity(volume).expect("resident capacity must be readable");
    assert_eq!(system.total_bytes, resident.total_bytes);
    // macOS consumers share a cached snapshot. Uncached platforms may change
    // slightly between reads on a live filesystem.
    #[cfg(target_os = "macos")]
    assert_eq!(system.available_bytes, resident.available_bytes);
    println!(
        "{}",
        serde_json::json!({
            "totalBytes": system.total_bytes, "mainAvailableBytes": system.available_bytes,
            "residentAvailableBytes": resident.available_bytes
        })
    );
    #[cfg(target_os = "macos")]
    measure("previous_statvfs", 100, || {
        let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::statvfs(c"/".as_ptr(), &mut stats) }, 0);
        std::hint::black_box(stats.f_bavail);
    });
    measure("shared_main_capacity", 100, || {
        std::hint::black_box(platform.system_volume().unwrap());
    });
    measure("shared_resident_capacity", 100, || {
        std::hint::black_box(disk::capacity(volume).unwrap());
    });
    measure("forced_main_capacity", 10, || {
        disk::invalidate_capacity_cache();
        std::hint::black_box(platform.system_volume().unwrap());
    });
}
