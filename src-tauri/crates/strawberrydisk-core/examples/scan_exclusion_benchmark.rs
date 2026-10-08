//! Reproducible storage workload. Run separate baseline/candidate processes in alternating order.
use std::{fs, path::PathBuf, time::Instant};

use strawberrydisk_core::{
    AnalysisService, DuplicateFileService, DuplicateScanLocation, DuplicateScanLocationMode,
    LargeFileScanMode, LargeFileService,
};

fn main() {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("supply an isolated fixture directory"),
    );
    fs::create_dir_all(&root).unwrap();
    let state = root
        .canonicalize()
        .unwrap()
        .with_file_name("benchmark-state");
    strawberrydisk_core::configure_application_paths(
        strawberrydisk_core::ApplicationPaths::new(
            state.join("data"),
            state.join("cache"),
            state.join("runtime"),
        )
        .unwrap(),
    )
    .unwrap();
    if !root.join("fixture-ready").exists() {
        for project in 0..24 {
            let dependency = root.join(format!("project-{project}/node_modules/package"));
            fs::create_dir_all(&dependency).unwrap();
            for file in 0..256 {
                fs::write(
                    dependency.join(format!("file-{file}.js")),
                    vec![file as u8; 256],
                )
                .unwrap();
            }
            fs::write(
                root.join(format!("project-{project}/source.txt")),
                b"source",
            )
            .unwrap();
        }
        fs::write(root.join("large-a.bin"), vec![17_u8; 64 * 1024 * 1024]).unwrap();
        fs::write(root.join("large-b.bin"), vec![17_u8; 64 * 1024 * 1024]).unwrap();
        fs::write(root.join("fixture-ready"), []).unwrap();
    }
    let root = root.canonicalize().unwrap().to_string_lossy().into_owned();
    let excluded: Vec<String> = if std::env::args().nth(2).as_deref() == Some("paths") {
        (0..24)
            .map(|i| {
                PathBuf::from(&root)
                    .join(format!("project-{i}/node_modules"))
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    } else {
        Vec::new()
    };
    let excluded = strawberrydisk_core::ScanExclusionOptions {
        paths: excluded,
        names: if std::env::args().nth(2).as_deref() == Some("names") {
            vec![strawberrydisk_core::ScanNameExclusion {
                name: "node_modules".into(),
                kind: strawberrydisk_core::ExcludedNameKind::Folder,
            }]
        } else {
            Vec::new()
        },
    };
    let start = Instant::now();
    let analysis = AnalysisService::analyze_with_exclusions_progress(
        Some(root.clone()),
        true,
        excluded.clone(),
        |_| {},
    )
    .unwrap();
    let analysis_us = start.elapsed().as_micros();
    let start = Instant::now();
    let large = LargeFileService::find_with_progress(
        vec![root.clone()],
        1,
        LargeFileScanMode::Complete,
        excluded.clone(),
        |_| {},
    )
    .unwrap();
    let large_us = start.elapsed().as_micros();
    let start = Instant::now();
    let duplicates = DuplicateFileService::find_paged_with_locations_and_exclusions(
        vec![DuplicateScanLocation {
            path: root,
            mode: DuplicateScanLocationMode::Cleanable,
        }],
        excluded,
        1,
        |_| {},
        |_| {},
    )
    .unwrap();
    println!("analysis_us={analysis_us} large_us={large_us} duplicates_us={} analysis_files={} large_files={} duplicate_scanned={}",start.elapsed().as_micros(),analysis.entries.iter().map(|e|e.file_count).sum::<u64>(),large.total_count,duplicates.scanned_file_count);
}
