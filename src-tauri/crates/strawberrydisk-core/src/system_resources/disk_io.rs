//! Rates use monotonic elapsed time and stable device sets, never cumulative totals as speeds.
use serde::Serialize;
use strawberrydisk_platform::system_resources::disk_io::DeviceCounters;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskIoRate {
    pub read_bytes_per_second: f64,
    pub written_bytes_per_second: f64,
}
#[derive(Default)]
pub struct DiskIoDelta {
    previous: Option<(Vec<DeviceCounters>, u64)>,
}
impl DiskIoDelta {
    pub fn reset(&mut self) {
        self.previous = None;
    }
    pub fn sample(&mut self, mut devices: Vec<DeviceCounters>, time_ms: u64) -> Option<DiskIoRate> {
        devices.sort_by(|a, b| a.id.cmp(&b.id));
        if devices.is_empty() || devices.windows(2).any(|pair| pair[0].id == pair[1].id) {
            self.reset();
            return None;
        }
        let previous = self.previous.replace((devices.clone(), time_ms))?;
        let elapsed = time_ms.checked_sub(previous.1)?;
        if !(100..=5000).contains(&elapsed) || previous.0.len() != devices.len() {
            return None;
        }
        let (mut read, mut written) = (0u128, 0u128);
        for (current, old) in devices.iter().zip(previous.0) {
            if current.id != old.id {
                return None;
            }
            read += u128::from(current.read_bytes.checked_sub(old.read_bytes)?);
            written += u128::from(current.written_bytes.checked_sub(old.written_bytes)?);
        }
        Some(DiskIoRate {
            read_bytes_per_second: read as f64 * 1000.0 / elapsed as f64,
            written_bytes_per_second: written as f64 * 1000.0 / elapsed as f64,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn device(id: &str, value: u64) -> DeviceCounters {
        DeviceCounters {
            id: id.into(),
            read_bytes: value,
            written_bytes: value * 2,
        }
    }
    #[test]
    fn aggregates_deltas_and_uses_actual_elapsed_time() {
        let mut delta = DiskIoDelta::default();
        assert!(delta
            .sample(vec![device("a", 100), device("b", 200)], 0)
            .is_none());
        let rate = delta
            .sample(vec![device("b", 500), device("a", 400)], 1500)
            .unwrap();
        assert_eq!(rate.read_bytes_per_second, 400.0);
        assert_eq!(rate.written_bytes_per_second, 800.0);
        assert_eq!(
            delta
                .sample(vec![device("a", 400), device("b", 500)], 2500)
                .unwrap()
                .read_bytes_per_second,
            0.0
        );
    }
    #[test]
    fn reset_topology_rollback_and_sleep_require_a_new_baseline() {
        for (next, time) in [
            (vec![device("b", 9000)], 2000),
            (vec![device("a", 1)], 2000),
            (vec![device("a", 9000)], 6000),
            (vec![device("a", 9000)], 0),
            (vec![], 2000),
            (vec![device("a", 9000), device("a", 9000)], 2000),
        ] {
            let mut delta = DiskIoDelta::default();
            delta.sample(vec![device("a", 100)], 0);
            assert!(delta.sample(next, time).is_none());
        }
    }
}
