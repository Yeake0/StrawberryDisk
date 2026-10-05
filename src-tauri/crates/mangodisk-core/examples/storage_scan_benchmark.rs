//! Read-only Core benchmark over an existing fixture. Alternate baseline and candidate binaries.
use std::{
    env,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

use mangodisk_core::{
    AnalysisService, ApplicationPaths, DuplicateFileService, DuplicateScanLocation,
    DuplicateScanLocationMode, LargeFileScanMode, LargeFileService, ScanExclusionOptions,
    TraversalProgress,
};

static LOGGER: BenchmarkLogger = BenchmarkLogger;
struct BenchmarkLogger;
impl log::Log for BenchmarkLogger {
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

fn digest(mut rows: Vec<(String, u64, u64)>) -> String {
    rows.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    for (path, bytes, count) in rows {
        hasher.update(&(path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update(&bytes.to_le_bytes());
        hasher.update(&count.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .map_err(|error| format!("failed to initialize benchmark logging: {error}"))?;
    let root = env::args()
        .nth(1)
        .ok_or("supply an existing fixture root")?;
    let mode = env::args().nth(2).unwrap_or_else(|| "analysis".into());
    let state = PathBuf::from(
        env::var_os("MANGODISK_BENCHMARK_STATE_ROOT")
            .ok_or("supply a separate benchmark state root")?,
    );
    mangodisk_core::configure_application_paths(ApplicationPaths::new(
        state.join("data"),
        state.join("cache"),
        state.join("runtime"),
    )?)?;
    let exclusions = ScanExclusionOptions::default();
    let observed_analysis_files = Arc::new(AtomicU64::new(0));
    let started = Instant::now();
    let (files, bytes, skipped, result_digest) = match mode.as_str() {
        "analysis" => {
            let observed = Arc::clone(&observed_analysis_files);
            let result = AnalysisService::analyze_with_exclusions_progress(
                Some(root),
                true,
                exclusions,
                move |progress: TraversalProgress| {
                    observed.store(progress.items_scanned, Ordering::Relaxed);
                },
            )?;
            (
                result.entries.iter().map(|entry| entry.file_count).sum(),
                result.total_bytes,
                result.skipped_count,
                digest(
                    result
                        .entries
                        .into_iter()
                        .map(|entry| (entry.path, entry.bytes, entry.file_count))
                        .collect(),
                ),
            )
        }
        "large" => {
            let result = LargeFileService::find_with_progress(
                vec![root],
                1,
                LargeFileScanMode::Complete,
                exclusions,
                |_| {},
            )?;
            (
                result.total_count,
                result.total_bytes,
                result.skipped_count,
                digest(
                    result
                        .entries
                        .into_iter()
                        .map(|entry| (entry.path, entry.bytes, 1))
                        .collect(),
                ),
            )
        }
        "duplicates" => {
            let result = DuplicateFileService::find_paged_with_locations_and_exclusions(
                vec![DuplicateScanLocation {
                    path: root,
                    mode: DuplicateScanLocationMode::Cleanable,
                }],
                exclusions,
                1,
                |_| {},
                |_| {},
            )?;
            (
                result.scanned_file_count,
                result.total_duplicate_bytes,
                result.skipped_count,
                digest(
                    result
                        .groups
                        .into_iter()
                        .flat_map(|group| group.entries)
                        .map(|entry| (entry.path, entry.allocated_bytes, 1))
                        .collect(),
                ),
            )
        }
        _ => return Err("mode must be analysis, large, or duplicates".into()),
    };
    // Analysis's returned rows are bounded. Progress contains the full completed traversal count.
    println!("storage_scan_benchmark mode={mode} files={files} bytes={bytes} skipped={skipped} result_digest={result_digest} elapsed_us={} analysis_files_observed={}", started.elapsed().as_micros(), observed_analysis_files.load(Ordering::Relaxed));
    Ok(())
}
