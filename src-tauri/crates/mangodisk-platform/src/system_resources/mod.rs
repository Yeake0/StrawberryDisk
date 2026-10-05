//! Native resource sampling. Each sensor owns its OS handles and refresh policy;
//! product aggregation and desktop scheduling stay in their respective layers.
#[cfg(target_os = "macos")]
mod application_metadata_macos;
pub mod cpu;
pub mod disk;
pub mod disk_io;
pub mod gpu;
pub mod memory;
#[cfg(target_os = "macos")]
mod memory_macos;
pub mod network;
pub mod process_cpu;
#[cfg(target_os = "macos")]
mod process_cpu_macos;
#[cfg(target_os = "macos")]
mod process_snapshot_macos;
#[cfg(windows)]
mod process_snapshot_windows;
pub mod release;

#[cfg(windows)]
mod process_image_windows;
