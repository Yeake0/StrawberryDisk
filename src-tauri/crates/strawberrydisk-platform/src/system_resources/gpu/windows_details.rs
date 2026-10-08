//! Adapter-wide memory and individual engines stay separate from the summary policy.
use super::{
    details::*,
    windows::{Engine, Failure},
    windows_metadata::Metadata,
    GpuAdapterUsage,
};
use std::collections::HashMap;
use windows_sys::Win32::System::Performance::{PDH_CSTATUS_NO_COUNTER, PDH_CSTATUS_NO_OBJECT};

type MemoryValues = [Vec<(String, f64)>; 2];
fn identity(name: &str) -> Option<(u32, u32, u32)> {
    let (luid, physical) = name.strip_prefix("luid_")?.split_once("_phys_")?;
    let (high, low) = luid.split_once('_')?;
    Some((
        u32::from_str_radix(high.strip_prefix("0x")?, 16).ok()?,
        u32::from_str_radix(low.strip_prefix("0x")?, 16).ok()?,
        physical.parse().ok()?,
    ))
}
fn bytes(values: &[(String, f64)], adapter: (u32, u32, u32)) -> Option<u64> {
    let mut matches = values
        .iter()
        .filter(|(name, _)| identity(name) == Some(adapter));
    let (_, value) = matches.next()?;
    // Duplicate instances or malformed values cannot become plausible idle memory.
    (matches.next().is_none() && value.is_finite() && *value >= 0.0 && *value < u64::MAX as f64)
        .then_some(*value as u64)
}
pub(super) fn attach(
    readings: &mut [GpuAdapterUsage],
    engines: &HashMap<Engine, f64>,
    metadata: &mut HashMap<(u32, u32), Metadata>,
    memory: &Result<MemoryValues, Failure>,
) {
    for ((high, low), adapter) in metadata {
        for physical in 0..adapter.physical_count {
            let id = format!("{}:{physical}", adapter.id);
            let Some(reading) = readings.iter_mut().find(|reading| reading.id == id) else {
                continue;
            };
            let mut activities = Vec::new();
            for ((node_physical, node), descriptor) in &adapter.nodes {
                if *node_physical != physical {
                    continue;
                }
                let Some(value) = engines.get(&(*high, *low, physical, *node)) else {
                    continue;
                };
                activities.push(GpuActivity {
                    id: format!("node:{node}"),
                    kind: descriptor.kind,
                    name: descriptor.name.clone(),
                    used_percent: value.clamp(0.0, 100.0),
                    included_in_summary: descriptor.standard,
                });
            }
            activities.sort_by(|a, b| {
                b.included_in_summary
                    .cmp(&a.included_in_summary)
                    .then_with(|| a.id.cmp(&b.id))
            });
            let allocation = memory.as_ref().ok().map(|values| GpuMemory {
                dedicated_used_bytes: bytes(&values[0], (*high, *low, physical)),
                dedicated_total_bytes: (adapter.physical_count == 1 && adapter.dedicated_bytes > 0)
                    .then_some(adapter.dedicated_bytes),
                shared_used_bytes: bytes(&values[1], (*high, *low, physical)),
            });
            let memory_status = match memory {
                Err(error)
                    if matches!(error.code, PDH_CSTATUS_NO_COUNTER | PDH_CSTATUS_NO_OBJECT) =>
                {
                    GpuMemoryStatus::Unsupported
                }
                Err(_) => GpuMemoryStatus::Failed,
                Ok(_)
                    if allocation.as_ref().is_some_and(|value| {
                        value.dedicated_used_bytes.is_some() || value.shared_used_bytes.is_some()
                    }) =>
                {
                    GpuMemoryStatus::Ready
                }
                Ok(_) => GpuMemoryStatus::Failed,
            };
            reading.details = Some(GpuDetails {
                activities,
                telemetry: GpuTelemetry {
                    temperature_celsius: adapter.temperature.read(physical, &id),
                    ..Default::default()
                },
                memory_architecture: if adapter.dedicated_bytes > 0 {
                    GpuMemoryArchitecture::Dedicated
                } else {
                    GpuMemoryArchitecture::Shared
                },
                memory_status,
                memory: allocation,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapter_memory_never_sums_shared_process_allocations() {
        let name = "luid_0x00000000_0x00001234_phys_0".to_owned();
        let values = vec![
            (name.clone(), 1024.0),
            ("luid_0x0_0x5678_phys_0".into(), 4096.0),
        ];
        assert_eq!(bytes(&values, (0, 0x1234, 0)), Some(1024));
        assert_eq!(bytes(&values, (0, 0x1234, 1)), None);
        assert_eq!(bytes(&[(name.clone(), f64::NAN)], (0, 0x1234, 0)), None);
        assert_eq!(bytes(&[(name.clone(), -1.0)], (0, 0x1234, 0)), None);
        assert_eq!(
            bytes(&[(name.clone(), 1.0), (name, 2.0)], (0, 0x1234, 0)),
            None
        );
    }
}
