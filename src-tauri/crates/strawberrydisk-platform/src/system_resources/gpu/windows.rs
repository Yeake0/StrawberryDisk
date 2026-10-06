//! A persistent PDH query summarizes standard WDDM engines, matching the Windows overview.
use super::observation_diagnostics::ObservationDiagnostics;
use super::windows_counter::Query;
use super::windows_metadata::Metadata;
use super::{failed, unsupported, GpuAdapter, GpuAdapterUsage, GpuSample, PlatformResult};
#[cfg(test)]
use super::{
    windows_counter::decode,
    windows_metadata::{node_enumeration_ended, physical_address_valid, standard_engine},
};
use crate::diagnostics::text;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
#[cfg(test)]
use windows_sys::Wdk::Graphics::Direct3D::*;
#[cfg(test)]
use windows_sys::Win32::Foundation::STATUS_INVALID_PARAMETER;
use windows_sys::Win32::System::Performance::*;

// pdh.h defines this public formatting flag; windows-sys omits it from metadata.
pub(super) const PDH_FMT_NOCAP100: u32 = 0x8000;
// Newer WDDM headers add this standard type; windows-sys 0.61 predates it.
pub(super) const DXGK_ENGINE_TYPE_VIDEO_CODEC:
    windows_sys::Wdk::Graphics::Direct3D::DXGK_ENGINE_TYPE = 9;
pub(super) const MAX_BUFFER_BYTES: usize = 8 * 1024 * 1024;
pub(super) type Engine = (u32, u32, u32, u32);
type Adapter = (u32, u32, u32);

pub(super) fn engine(name: &str) -> Option<Engine> {
    let (pid, rest) = name.strip_prefix("pid_")?.split_once("_luid_")?;
    pid.parse::<u32>().ok()?;
    let (luid, rest) = rest.split_once("_phys_")?;
    let (high, low) = luid.split_once('_')?;
    let (physical, rest) = rest.split_once("_eng_")?;
    let (index, _) = rest.split_once("_engtype_")?;
    Some((
        u32::from_str_radix(high.strip_prefix("0x")?, 16).ok()?,
        u32::from_str_radix(low.strip_prefix("0x")?, 16).ok()?,
        physical.parse().ok()?,
        index.parse().ok()?,
    ))
}

fn adapters(
    engines: &HashMap<Engine, f64>,
    names: &HashMap<(u32, u32), Metadata>,
    software: &HashSet<(u32, u32)>,
) -> Vec<GpuAdapterUsage> {
    let mut usage: HashMap<Adapter, f64> = HashMap::new();
    for (&(high, low, physical, node), &value) in engines {
        if software.contains(&(high, low))
            || !names
                .get(&(high, low))
                .is_some_and(|metadata| metadata.engines.contains(&(physical, node)))
        {
            continue;
        }
        let percent = usage.entry((high, low, physical)).or_default();
        *percent = percent.max(value.min(100.0));
    }
    let mut values: Vec<_> = usage
        .into_iter()
        .map(|((high, low, physical), used_percent)| GpuAdapterUsage {
            id: names
                .get(&(high, low))
                .map(|value| format!("{}:{physical}", value.id))
                .unwrap_or_else(|| format!("{high:08x}:{low:08x}:{physical}")),
            name: names
                .get(&(high, low))
                .map(|value| value.name.clone())
                .unwrap_or_else(|| format!("GPU {high:08x}:{low:08x}:{physical}")),
            used_percent,
            details: None,
        })
        .collect();
    values.sort_by(|a, b| a.id.cmp(&b.id));
    values
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) stage: &'static str,
    pub(super) code: u32,
}
pub(super) fn check(stage: &'static str, code: u32) -> Result<(), Failure> {
    if code == 0 {
        Ok(())
    } else {
        Err(Failure { stage, code })
    }
}

#[derive(Default)]
pub struct GpuReader {
    query: Option<Query>,
    retry_at: Option<Instant>,
    failure: Option<Failure>,
    names: HashMap<(u32, u32), Metadata>,
    named_at: Option<Instant>,
    software: HashSet<(u32, u32)>,
    metadata_failure: Option<i32>,
    adapter_failures: HashMap<(u32, u32), Failure>,
    memory_failure: Option<Failure>,
    memory_retry_at: Option<Instant>,
    diagnostics: ObservationDiagnostics,
    inventory_counts: Option<(usize, usize, usize)>,
    enumeration_failure: Option<(u32, Failure)>,
}
impl GpuReader {
    pub fn reset(&mut self) {
        if let Some(query) = self.query.as_mut() {
            query.previous = None;
        }
    }

    fn discover(&mut self) {
        if self
            .named_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(30))
        {
            return;
        }
        self.named_at = Some(Instant::now());
        // Cached metadata defines stable identities and which engines enter the summary.
        let factory: Result<IDXGIFactory1, _> = unsafe { CreateDXGIFactory1() };
        let factory = match factory {
            Ok(factory) => factory,
            Err(error) => {
                let code = error.code().0;
                if self.metadata_failure != Some(code) {
                    log::warn!("gpu_metadata_unavailable source=dxgi stage=factory native_code={code:#x} outcome=retain_previous_names retry_seconds=30");
                }
                self.metadata_failure = Some(code);
                return;
            }
        };
        if self.metadata_failure.take().is_some() {
            log::info!("gpu_metadata_recovered source=dxgi");
        }
        let mut software = HashSet::new();
        let mut names = HashMap::new();
        let mut present = HashSet::new();
        let mut enumeration_failure = None;
        for index in 0..64 {
            let adapter = match unsafe { factory.EnumAdapters1(index) } {
                Ok(adapter) => adapter,
                Err(error) => {
                    if error.code() != windows::Win32::Graphics::Dxgi::DXGI_ERROR_NOT_FOUND {
                        enumeration_failure = Some((
                            index,
                            Failure {
                                stage: "enumerate",
                                code: error.code().0 as u32,
                            },
                        ));
                    }
                    break;
                }
            };
            let description = match unsafe { adapter.GetDesc1() } {
                Ok(description) => description,
                Err(error) => {
                    enumeration_failure = Some((
                        index,
                        Failure {
                            stage: "description",
                            code: error.code().0 as u32,
                        },
                    ));
                    continue;
                }
            };
            let identity = (
                description.AdapterLuid.HighPart as u32,
                description.AdapterLuid.LowPart,
            );
            present.insert(identity);
            if description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                self.adapter_failures.remove(&identity);
                software.insert(identity);
                continue;
            }
            match Metadata::read(&description, names.len() as u32) {
                Ok(mut metadata) => {
                    if self
                        .names
                        .get(&identity)
                        .is_none_or(|previous| !metadata.same_inventory(previous))
                    {
                        metadata.log_inventory(identity);
                    }
                    if let Some(previous) = self.names.get_mut(&identity) {
                        metadata.retain_temperature_state(previous);
                    }
                    if self.adapter_failures.remove(&identity).is_some() {
                        log::info!(
                            "gpu_adapter_metadata_recovered adapter_luid={:08x}:{:08x}",
                            identity.0,
                            identity.1
                        );
                    }
                    names.insert(identity, metadata);
                }
                Err(error) => {
                    if self.adapter_failures.insert(identity, error) != Some(error) {
                        log::warn!("gpu_adapter_metadata_unavailable adapter_luid={:08x}:{:08x} stage={} native_code={:#x} outcome=unavailable retry_seconds=30", identity.0, identity.1, error.stage, error.code);
                    }
                }
            }
        }
        if self.enumeration_failure != enumeration_failure {
            match enumeration_failure {
                Some((index, error)) => log::warn!("gpu_inventory_incomplete source=dxgi adapter_index={} stage={} native_code={:#x} outcome=partial_inventory retry_seconds=30", index, error.stage, error.code),
                None => log::info!("gpu_inventory_recovered source=dxgi"),
            }
            self.enumeration_failure = enumeration_failure;
        }
        for (luid, previous) in &self.names {
            if !names.contains_key(luid) {
                log::info!(
                    "gpu_adapter_removed source=dxgi_wddm adapter={} name={} metadata_failed={}",
                    text(&previous.id),
                    text(&previous.name),
                    self.adapter_failures.contains_key(luid)
                );
            }
        }
        self.adapter_failures
            .retain(|luid, _| present.contains(luid));
        let counts = (names.len(), software.len(), self.adapter_failures.len());
        if self.inventory_counts != Some(counts) {
            log::info!("gpu_inventory source=dxgi_wddm hardware_adapters={} excluded_software_adapters={} failed_metadata_adapters={}", counts.0, counts.1, counts.2);
            self.inventory_counts = Some(counts);
        }
        self.diagnostics.retain(|id| {
            names.values().any(|adapter| {
                (0..adapter.physical_count)
                    .any(|physical| id == format!("{}:{physical}", adapter.id))
            })
        });
        self.names = names;
        self.software = software;
    }

    pub fn catalogue(&mut self) -> PlatformResult<Vec<GpuAdapter>> {
        self.discover();
        Ok(self.adapters())
    }

    pub fn adapters(&self) -> Vec<GpuAdapter> {
        let mut values: Vec<_> = self
            .names
            .values()
            .flat_map(|value| {
                (0..value.physical_count).map(|physical| GpuAdapter {
                    id: format!("{}:{physical}", value.id),
                    name: if value.physical_count == 1 {
                        format!("GPU {} · {}", value.ordinal, value.name)
                    } else {
                        format!("GPU {}.{physical} · {}", value.ordinal, value.name)
                    },
                })
            })
            .collect();
        values.sort_by(|a, b| a.id.cmp(&b.id));
        values
    }

    pub fn read(&mut self) -> PlatformResult<GpuSample> {
        self.sample(false)
    }
    pub fn read_detailed(&mut self) -> PlatformResult<GpuSample> {
        self.sample(true)
    }
    fn sample(&mut self, detailed: bool) -> PlatformResult<GpuSample> {
        self.discover();
        if self.names.is_empty() {
            self.query = None;
            return Err(unsupported());
        }
        if self.retry_at.is_some_and(|at| Instant::now() < at) {
            return Err(self.error(self.failure.expect("retry retains native failure")));
        }
        let result = (|| {
            if self.query.is_none() {
                self.query = Some(Query::open()?);
            }
            let query = self.query.as_mut().expect("query was acquired");
            let memory_enabled =
                detailed && self.memory_retry_at.is_none_or(|at| Instant::now() >= at);
            if let Err(error) = query.set_memory(memory_enabled) {
                if self.memory_failure != Some(error) {
                    log::warn!(
                        "gpu_memory_unavailable stage={} native_code={:#x} retry_seconds=30",
                        error.stage,
                        error.code
                    );
                }
                self.memory_failure = Some(error);
                self.memory_retry_at = Some(Instant::now() + Duration::from_secs(30));
            }
            query.read()
        })();
        match result {
            Ok(values) => {
                if self.failure.take().is_some() {
                    log::info!("gpu_source_recovered source=pdh_gpu_engine");
                }
                self.retry_at = None;
                let Some(values) = values else {
                    return Ok(GpuSample::Baseline);
                };
                let memory = detailed.then(|| {
                    if self.memory_retry_at.is_some_and(|at| Instant::now() < at) {
                        return Err(self.memory_failure.expect("memory backoff retains failure"));
                    }
                    let result = self.query.as_mut().expect("query was acquired").memory();
                    match &result {
                        Ok(_) => {
                            if self.memory_failure.take().is_some() { log::info!("gpu_memory_recovered source=pdh_adapter_memory"); }
                            self.memory_retry_at = None;
                        }
                        Err(error) => {
                            if self.memory_failure != Some(*error) { log::warn!("gpu_memory_unavailable stage={} native_code={:#x} retry_seconds=30", error.stage, error.code); }
                            self.memory_failure = Some(*error);
                            self.memory_retry_at = Some(Instant::now() + Duration::from_secs(30));
                        }
                    }
                    result
                });
                let mut readings = adapters(&values, &self.names, &self.software);
                if let Some(memory) = memory {
                    super::windows_details::attach(
                        &mut readings,
                        &values,
                        &mut self.names,
                        &memory,
                    );
                }
                for adapter in self.names.values() {
                    for physical in 0..adapter.physical_count {
                        let id = format!("{}:{physical}", adapter.id);
                        let result = readings
                            .iter()
                            .find(|reading| reading.id == id)
                            .map(|reading| reading.details.as_ref())
                            .ok_or("no_classified_engine_observation");
                        self.diagnostics
                            .observe(&id, "pdh_gpu_engine_peak_standard", result);
                    }
                }
                Ok(GpuSample::Usage(readings))
            }
            Err(error) => {
                if self.failure != Some(error) {
                    log::warn!("gpu_source_unavailable source=pdh_gpu_engine stage={} native_code={:#x} retry_seconds=30", error.stage, error.code);
                }
                self.failure = Some(error);
                self.query = None;
                self.retry_at = Some(Instant::now() + Duration::from_secs(30));
                Err(self.error(error))
            }
        }
    }

    fn error(&self, error: Failure) -> crate::PlatformError {
        if matches!(error.code, PDH_CSTATUS_NO_OBJECT | PDH_CSTATUS_NO_COUNTER) {
            unsupported()
        } else {
            failed(error.stage, format!("{:#x}", error.code))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    fn test_metadata(id: &str, name: &str, engines: &[(u32, u32)]) -> Metadata {
        Metadata {
            id: id.into(),
            name: name.into(),
            ordinal: 0,
            physical_count: 2,
            engines: engines.iter().copied().collect(),
            nodes: HashMap::new(),
            dedicated_bytes: 0,
            temperature: Default::default(),
        }
    }
    #[test]
    fn session_adapter_addresses_cannot_alias_hardware_selections() {
        assert!(physical_address_valid(&D3DKMT_ADAPTERADDRESS {
            BusNumber: 1,
            DeviceNumber: 0,
            FunctionNumber: 0,
        }));
        assert!(!physical_address_valid(&D3DKMT_ADAPTERADDRESS {
            BusNumber: u32::MAX,
            DeviceNumber: u16::MAX as u32,
            FunctionNumber: u16::MAX as u32,
        }));
        assert!(!physical_address_valid(&D3DKMT_ADAPTERADDRESS {
            BusNumber: 1,
            DeviceNumber: 32,
            FunctionNumber: 0,
        }));
        assert!(!physical_address_valid(&D3DKMT_ADAPTERADDRESS {
            BusNumber: 1,
            DeviceNumber: 0,
            FunctionNumber: 8,
        }));
    }
    #[test]
    fn unexpected_metadata_errors_cannot_silently_truncate_the_summary() {
        assert!(!node_enumeration_ended(0, STATUS_INVALID_PARAMETER as u32));
        assert!(node_enumeration_ended(20, STATUS_INVALID_PARAMETER as u32));
        assert!(!node_enumeration_ended(20, 0xc0000022));
    }
    #[test]
    fn catalogue_discovery_never_acquires_a_pdh_query() {
        let mut reader = GpuReader::default();
        reader.catalogue().unwrap();
        assert!(reader.query.is_none());
    }
    #[test]
    fn custom_nodes_do_not_replace_the_windows_aggregate() {
        assert!(!standard_engine(DXGK_ENGINE_TYPE_OTHER));
        assert!(standard_engine(DXGK_ENGINE_TYPE_VIDEO_ENCODE));
        assert!(!standard_engine(i32::MAX));
        assert!(standard_engine(DXGK_ENGINE_TYPE_VIDEO_CODEC));
        let values = adapters(
            &HashMap::from([((0, 1, 0, 0), 3.0), ((0, 1, 0, 12), 56.0)]),
            &HashMap::from([((0, 1), test_metadata("pci:1", "NVIDIA", &[(0, 0)]))]),
            &HashSet::new(),
        );
        assert_eq!(values[0].used_percent, 3.0);
        let graphics = adapters(
            &HashMap::from([((0, 1, 0, 0), 85.0), ((0, 1, 0, 2), 10.0)]),
            &HashMap::from([((0, 1), test_metadata("pci:1", "NVIDIA", &[(0, 0), (0, 2)]))]),
            &HashSet::new(),
        );
        assert_eq!(graphics[0].used_percent, 85.0);
    }
    #[test]
    fn instance_identity_distinguishes_adapters_and_physical_engines() {
        assert_eq!(
            engine("pid_123_luid_0x00000000_0x00001234_phys_0_eng_2_engtype_Compute"),
            Some((0, 0x1234, 0, 2))
        );
        assert_eq!(engine("pid_1_luid_invalid_phys_0_eng_0_engtype_3D"), None);
    }
    #[test]
    fn busiest_engine_sums_processes_without_summing_independent_engines() {
        let mut engines = HashMap::new();
        *engines.entry((0, 1, 0, 0)).or_default() += 40.0;
        *engines.entry((0, 1, 0, 0)).or_default() += 30.0;
        engines.insert((0, 1, 0, 1), 50.0);
        engines.insert((0, 2, 0, 0), 12.0);
        engines.insert((0, 2, 1, 0), 110.0);
        let result = adapters(
            &engines,
            &HashMap::from([
                ((0, 1), test_metadata("pci:1", "NVIDIA", &[(0, 0), (0, 1)])),
                ((0, 2), test_metadata("pci:2", "GPU 2", &[(0, 0), (1, 0)])),
            ]),
            &HashSet::new(),
        );
        assert_eq!(result.len(), 3);
        assert_eq!(
            result
                .iter()
                .find(|value| value.id == "pci:1:0")
                .unwrap()
                .used_percent,
            70.0
        );
        assert_eq!(
            result
                .iter()
                .find(|value| value.name == "NVIDIA")
                .unwrap()
                .used_percent,
            70.0
        );
        assert_eq!(
            result
                .iter()
                .find(|value| value.id == "pci:2:0")
                .unwrap()
                .used_percent,
            12.0
        );
        assert_eq!(
            result
                .iter()
                .find(|value| value.id == "pci:2:1")
                .unwrap()
                .used_percent,
            100.0
        );
    }
    #[test]
    fn software_adapters_cannot_be_reported_as_hardware_usage() {
        let values = adapters(
            &HashMap::from([((0, 1, 0, 0), 50.0), ((0, 2, 0, 0), 95.0)]),
            &HashMap::from([
                ((0, 1), test_metadata("pci:1", "GPU 1", &[(0, 0)])),
                ((0, 2), test_metadata("pci:2", "GPU 2", &[(0, 0)])),
            ]),
            &HashSet::from([(0, 2)]),
        );
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].used_percent, 50.0);
        assert_eq!(
            engine("pid_invalid_luid_0x0_0x1_phys_0_eng_0_engtype_3D"),
            None
        );
    }

    #[test]
    fn malformed_native_buffers_are_rejected_before_reading_names() {
        let mut buffer = vec![0u64; 8];
        let bytes = buffer.len() * 8;
        assert_eq!(decode(&buffer, bytes + 1, 1).unwrap_err().stage, "bounds");
        assert_eq!(
            decode(&buffer, bytes, u32::MAX).unwrap_err().stage,
            "bounds"
        );
        let item = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        unsafe {
            (*item).FmtValue.CStatus = PDH_CSTATUS_VALID_DATA;
            (*item).szName = ptr::null_mut();
        }
        assert_eq!(decode(&buffer, bytes, 1).unwrap_err().stage, "name_bounds");
        unsafe {
            (*item).szName = (buffer.as_ptr() as usize + 1) as *mut u16;
        }
        assert_eq!(decode(&buffer, bytes, 1).unwrap_err().stage, "name_bounds");
        let last_unit = (buffer.as_ptr() as usize + bytes - 2) as *mut u16;
        unsafe {
            *last_unit = 1;
            (*item).szName = last_unit;
        }
        assert_eq!(decode(&buffer, bytes, 1).unwrap_err().stage, "name_length");
    }
}
