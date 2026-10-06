use serde::Serialize;
use strawberrydisk_platform::system_resources::disk::{ResourceVolume, VolumeCapacity};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub volume: ResourceVolume,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
}

pub fn select<'a>(
    volumes: &'a [ResourceVolume],
    manual: Option<&str>,
) -> Option<&'a ResourceVolume> {
    volumes
        .iter()
        .find(|volume| manual.map_or(volume.system, |id| id == volume.id))
}

pub fn usage(volume: ResourceVolume, capacity: VolumeCapacity) -> Option<DiskUsage> {
    let used = capacity.total_bytes.checked_sub(capacity.available_bytes)?;
    if capacity.total_bytes == 0 {
        return None;
    }
    Some(DiskUsage {
        volume,
        total_bytes: capacity.total_bytes,
        available_bytes: capacity.available_bytes,
        used_bytes: used,
        used_percent: used as f64 / capacity.total_bytes as f64 * 100.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn volume(id: &str, system: bool) -> ResourceVolume {
        ResourceVolume {
            id: id.into(),
            name: "Disk".into(),
            system,
            mount_point: String::new(),
        }
    }

    #[test]
    fn system_volume_is_explicit_and_disconnected_manual_volume_stays_selected() {
        let volumes = [volume("external", false), volume("system", true)];
        assert_eq!(select(&volumes, None).unwrap().id, "system");
        assert!(select(&volumes, Some("missing")).is_none());
        assert_eq!(select(&volumes, Some("external")).unwrap().id, "external");
    }

    #[test]
    fn invalid_capacity_is_not_presented_as_zero_use() {
        assert!(usage(
            volume("one", true),
            VolumeCapacity {
                total_bytes: 0,
                available_bytes: 0
            }
        )
        .is_none());
        assert!(usage(
            volume("one", true),
            VolumeCapacity {
                total_bytes: 100,
                available_bytes: 101
            }
        )
        .is_none());
        assert_eq!(
            usage(
                volume("one", true),
                VolumeCapacity {
                    total_bytes: 100,
                    available_bytes: 25
                }
            )
            .unwrap()
            .used_percent,
            75.0
        );
    }
}
