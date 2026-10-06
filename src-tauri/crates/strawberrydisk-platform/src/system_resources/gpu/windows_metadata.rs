//! Cached identities and engine classification from public WDDM metadata queries.
use super::windows::{check, Failure, DXGK_ENGINE_TYPE_VIDEO_CODEC};
use crate::diagnostics::text;
use std::{
    collections::{HashMap, HashSet},
    mem,
    time::{Duration, Instant},
};
use windows_sys::{
    Wdk::Graphics::Direct3D::*,
    Win32::Foundation::{LUID, STATUS_INVALID_PARAMETER},
    Win32::System::Performance::PDH_INVALID_DATA,
};

pub(super) fn node_enumeration_ended(node: u32, code: u32) -> bool {
    node > 0 && code == STATUS_INVALID_PARAMETER as u32
}

pub(super) fn standard_engine(kind: DXGK_ENGINE_TYPE) -> bool {
    matches!(
        kind,
        DXGK_ENGINE_TYPE_3D
            | DXGK_ENGINE_TYPE_VIDEO_DECODE
            | DXGK_ENGINE_TYPE_VIDEO_ENCODE
            | DXGK_ENGINE_TYPE_VIDEO_PROCESSING
            | DXGK_ENGINE_TYPE_SCENE_ASSEMBLY
            | DXGK_ENGINE_TYPE_COPY
            | DXGK_ENGINE_TYPE_OVERLAY
            | DXGK_ENGINE_TYPE_CRYPTO
            | DXGK_ENGINE_TYPE_VIDEO_CODEC
    )
}

pub(super) fn physical_address_valid(address: &D3DKMT_ADAPTERADDRESS) -> bool {
    address.BusNumber != u32::MAX && address.DeviceNumber <= 31 && address.FunctionNumber <= 7
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Node {
    pub kind: super::details::GpuActivityKind,
    pub native_kind: DXGK_ENGINE_TYPE,
    pub name: Option<String>,
    pub standard: bool,
}

pub(super) struct Metadata {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) ordinal: u32,
    pub(super) physical_count: u32,
    pub(super) engines: HashSet<(u32, u32)>,
    pub(super) nodes: HashMap<(u32, u32), Node>,
    pub(super) dedicated_bytes: u64,
    pub(super) temperature: TemperatureReader,
}

struct AdapterHandle(u32);
impl Drop for AdapterHandle {
    fn drop(&mut self) {
        unsafe {
            D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: self.0 });
        }
    }
}
impl AdapterHandle {
    fn open(luid: LUID) -> Result<Self, Failure> {
        let mut value = D3DKMT_OPENADAPTERFROMLUID {
            AdapterLuid: luid,
            hAdapter: 0,
        };
        check(
            "adapter_open",
            unsafe { D3DKMTOpenAdapterFromLuid(&mut value) } as u32,
        )?;
        Ok(Self(value.hAdapter))
    }
    fn query<T>(
        &self,
        kind: KMTQUERYADAPTERINFOTYPE,
        value: &mut T,
        stage: &'static str,
    ) -> Result<(), Failure> {
        let mut query = D3DKMT_QUERYADAPTERINFO {
            hAdapter: self.0,
            Type: kind,
            pPrivateDriverData: (value as *mut T).cast(),
            PrivateDriverDataSize: mem::size_of::<T>() as u32,
        };
        check(stage, unsafe { D3DKMTQueryAdapterInfo(&mut query) } as u32)
    }
}

// The discovery cache owns this handle, so detailed sampling needs no additional
// adapter opens. Unsupported drivers are retried at most once per 30 seconds.
#[derive(Default)]
pub(super) struct TemperatureReader {
    handle: Option<AdapterHandle>,
    retry_at: Vec<Option<Instant>>,
    observations: Vec<Option<Result<bool, Failure>>>,
}

fn temperature_celsius(deci_celsius: u32) -> Option<f64> {
    (1..=1500)
        .contains(&deci_celsius)
        .then_some(f64::from(deci_celsius) / 10.0)
}

impl TemperatureReader {
    pub(super) fn read(&mut self, physical: u32, id: &str) -> Option<f64> {
        let retry_at = self.retry_at.get_mut(physical as usize)?;
        if retry_at.is_some_and(|at| Instant::now() < at) {
            return None;
        }
        let mut value = D3DKMT_ADAPTER_PERFDATA {
            PhysicalAdapterIndex: physical,
            ..Default::default()
        };
        let result = self.handle.as_ref()?.query(
            KMTQAITYPE_ADAPTERPERFDATA,
            &mut value,
            "adapter_temperature",
        );
        let observation = result.map(|()| temperature_celsius(value.Temperature).is_some());
        if let Some(previous) = self.observations.get_mut(physical as usize) {
            if *previous != Some(observation) {
                match observation {
                    Ok(available) => log::info!("gpu_temperature_capability adapter={} physical={} source=wddm_adapter_perfdata available={} native_deci_celsius={} retry_seconds=30", text(id), physical, available, value.Temperature),
                    Err(error) => log::info!("gpu_temperature_unavailable adapter={} physical={} source=wddm_adapter_perfdata stage={} native_code={:#x} retry_seconds=30", text(id), physical, error.stage, error.code),
                }
                *previous = Some(observation);
            }
        }
        let temperature = match result {
            Ok(()) => temperature_celsius(value.Temperature),
            Err(_) => None,
        };
        *retry_at = temperature
            .is_none()
            .then(|| Instant::now() + Duration::from_secs(30));
        temperature
    }
}

impl Metadata {
    pub(super) fn same_inventory(&self, previous: &Self) -> bool {
        self.id == previous.id
            && self.name == previous.name
            && self.ordinal == previous.ordinal
            && self.physical_count == previous.physical_count
            && self.dedicated_bytes == previous.dedicated_bytes
            && self.nodes == previous.nodes
    }
    pub(super) fn log_inventory(&self, luid: (u32, u32)) {
        const MAX_LOGGED_NODES: usize = 64;
        log::info!("gpu_adapter_discovered source=dxgi_wddm adapter={} name={} adapter_luid={:08x}:{:08x} ordinal={} physical_count={} dedicated_capacity_bytes={} node_count={} standard_node_count={} summary_policy=peak_standard_engine custom_in_summary=false", text(&self.id), text(&self.name), luid.0, luid.1, self.ordinal, self.physical_count, self.dedicated_bytes, self.nodes.len(), self.engines.len());
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by_key(|(identity, _)| **identity);
        for ((physical, node), descriptor) in nodes.into_iter().take(MAX_LOGGED_NODES) {
            log::info!("gpu_engine_discovered adapter={} physical={} node={} native_type={} kind={:?} name={} included_in_summary={}", text(&self.id), physical, node, descriptor.native_kind, descriptor.kind, text(descriptor.name.as_deref().unwrap_or("")), descriptor.standard);
        }
        if self.nodes.len() > MAX_LOGGED_NODES {
            log::info!(
                "gpu_engine_inventory_truncated adapter={} logged_nodes={} omitted_nodes={}",
                text(&self.id),
                MAX_LOGGED_NODES,
                self.nodes.len() - MAX_LOGGED_NODES
            );
        }
    }
    // Preserve retry and diagnostic state across the regular metadata refresh.
    pub(super) fn retain_temperature_state(&mut self, previous: &mut Self) {
        if self.id == previous.id && self.physical_count == previous.physical_count {
            self.temperature.retry_at = std::mem::take(&mut previous.temperature.retry_at);
            self.temperature.observations = std::mem::take(&mut previous.temperature.observations);
        }
    }

    pub(super) fn read(
        description: &windows::Win32::Graphics::Dxgi::DXGI_ADAPTER_DESC1,
        ordinal: u32,
    ) -> Result<Self, Failure> {
        let handle = AdapterHandle::open(LUID {
            HighPart: description.AdapterLuid.HighPart,
            LowPart: description.AdapterLuid.LowPart,
        })?;
        let mut address = D3DKMT_ADAPTERADDRESS::default();
        handle.query(KMTQAITYPE_ADAPTERADDRESS, &mut address, "adapter_address")?;
        // Session adapters can repeat a GPU name with an invalid PCI address.
        // Persisting that address would alias several devices to one selection.
        if !physical_address_valid(&address) {
            return Err(Failure {
                stage: "adapter_address_invalid",
                code: PDH_INVALID_DATA,
            });
        }
        let id = format!(
            "pci:{:04x}:{:04x}:{}:{}:{}",
            description.VendorId,
            description.DeviceId,
            address.BusNumber,
            address.DeviceNumber,
            address.FunctionNumber
        );
        let mut count = D3DKMT_PHYSICAL_ADAPTER_COUNT::default();
        handle.query(
            KMTQAITYPE_PHYSICALADAPTERCOUNT,
            &mut count,
            "physical_count",
        )?;
        if !(1..=16).contains(&count.Count) {
            return Err(Failure {
                stage: "physical_count_limit",
                code: PDH_INVALID_DATA,
            });
        }
        let mut engines = HashSet::new();
        let mut nodes = HashMap::new();
        for physical in 0..count.Count {
            let mut ended = false;
            for node in 0..256 {
                let mut metadata = D3DKMT_NODEMETADATA {
                    NodeOrdinalAndAdapterIndex: node | (physical << 16),
                    ..Default::default()
                };
                if let Err(error) =
                    handle.query(KMTQAITYPE_NODEMETADATA, &mut metadata, "node_metadata")
                {
                    if !node_enumeration_ended(node, error.code) {
                        return Err(error);
                    }
                    ended = true;
                    break;
                }
                // Custom nodes remain outside this summary even if their driver
                // name resembles a standard engine. They may still execute work.
                let native_kind = metadata.NodeData.EngineType;
                let units = metadata.NodeData.FriendlyName;
                let end = units
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(units.len());
                let name = String::from_utf16_lossy(&units[..end]);
                use super::details::GpuActivityKind as Kind;
                let kind = match native_kind {
                    DXGK_ENGINE_TYPE_3D => Kind::Graphics,
                    DXGK_ENGINE_TYPE_COPY => Kind::Copy,
                    DXGK_ENGINE_TYPE_VIDEO_DECODE => Kind::VideoDecode,
                    DXGK_ENGINE_TYPE_VIDEO_ENCODE => Kind::VideoEncode,
                    DXGK_ENGINE_TYPE_VIDEO_PROCESSING => Kind::VideoProcessing,
                    _ => Kind::Other,
                };
                nodes.insert(
                    (physical, node),
                    Node {
                        kind,
                        native_kind,
                        name: (!name.is_empty()).then_some(name),
                        standard: standard_engine(native_kind),
                    },
                );
                if standard_engine(native_kind) {
                    engines.insert((physical, node));
                }
            }
            if !ended {
                return Err(Failure {
                    stage: "node_count_limit",
                    code: PDH_INVALID_DATA,
                });
            }
        }
        let length = description
            .Description
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(description.Description.len());
        Ok(Self {
            id,
            name: String::from_utf16_lossy(&description.Description[..length]),
            ordinal,
            physical_count: count.Count,
            engines,
            nodes,
            dedicated_bytes: description.DedicatedVideoMemory as u64,
            temperature: TemperatureReader {
                handle: Some(handle),
                retry_at: vec![None; count.Count as usize],
                observations: vec![None; count.Count as usize],
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_converts_deci_celsius_and_rejects_unavailable_values() {
        assert_eq!(temperature_celsius(420), Some(42.0));
        assert_eq!(temperature_celsius(425), Some(42.5));
        assert_eq!(temperature_celsius(1500), Some(150.0));
        assert_eq!(temperature_celsius(0), None);
        assert_eq!(temperature_celsius(1501), None);
        assert_eq!(temperature_celsius(u32::MAX), None);
    }

    #[test]
    fn metadata_refresh_retains_temperature_backoff_and_diagnostic_history() {
        fn adapter(id: &str) -> Metadata {
            Metadata {
                id: id.into(),
                name: "GPU".into(),
                ordinal: 0,
                physical_count: 1,
                engines: HashSet::new(),
                nodes: HashMap::new(),
                dedicated_bytes: 0,
                temperature: TemperatureReader {
                    retry_at: vec![None],
                    observations: vec![None],
                    ..Default::default()
                },
            }
        }
        let retry_at = Instant::now() + Duration::from_secs(30);
        let mut previous = adapter("pci:1");
        previous.temperature.retry_at[0] = Some(retry_at);
        previous.temperature.observations[0] = Some(Ok(false));
        let mut refreshed = adapter("pci:1");
        refreshed.retain_temperature_state(&mut previous);
        assert_eq!(refreshed.temperature.retry_at[0], Some(retry_at));
        assert_eq!(refreshed.temperature.observations[0], Some(Ok(false)));
        let mut different = adapter("pci:2");
        different.retain_temperature_state(&mut refreshed);
        assert_eq!(different.temperature.retry_at[0], None);
        assert_eq!(different.temperature.observations[0], None);
    }
    #[test]
    fn absent_adapter_and_invalid_physical_index_have_no_temperature() {
        let mut reader = TemperatureReader::default();
        assert_eq!(reader.read(0, "unavailable"), None);
        reader.retry_at = vec![None];
        assert_eq!(reader.read(0, "unavailable"), None);
        assert_eq!(reader.read(1, "unavailable"), None);
    }
}
