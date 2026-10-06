//! Resident disk activity worker. Slow capacity queries cannot delay I/O samples.
use super::sampling_workers::{timestamp_ms, SamplingEvent};
use std::{
    sync::mpsc::{self, SyncSender},
    time::Instant,
};
use strawberrydisk_platform::system_resources::disk_io::{DeviceCounters, DiskIoReader};

pub struct Completion {
    pub generation: u64,
    pub monotonic_ms: u64,
    pub timestamp_ms: u64,
    pub duration_ms: u64,
    pub result: strawberrydisk_platform::PlatformResult<Vec<DeviceCounters>>,
}
pub fn start(origin: Instant, events: SyncSender<SamplingEvent>) -> SyncSender<u64> {
    let (sender, requests) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        // Native handles are created and destroyed on their owning worker.
        let mut reader = DiskIoReader::default();
        while let Ok(generation) = requests.recv() {
            let started = Instant::now();
            let result = reader.read();
            let completion = Completion {
                generation,
                result,
                monotonic_ms: origin.elapsed().as_millis() as u64,
                timestamp_ms: timestamp_ms(),
                duration_ms: started.elapsed().as_millis() as u64,
            };
            if events.send(SamplingEvent::DiskIo(completion)).is_err() {
                break;
            }
        }
    });
    sender
}
