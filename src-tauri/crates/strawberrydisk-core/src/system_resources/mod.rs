//! System resource snapshots and explicit memory reclamation, independent of desktop UI.
mod application_groups;
mod application_identity;
pub mod cpu;
pub mod disk;
pub mod disk_io;
pub mod gpu;
mod memory;
pub mod metrics;
pub mod models;
pub mod network;
pub mod process_cpu;
pub mod readings;
pub mod release;
pub mod release_policy;
pub mod service;
