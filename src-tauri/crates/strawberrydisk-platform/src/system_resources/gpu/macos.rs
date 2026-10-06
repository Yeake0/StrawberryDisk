//! Driver statistics are capability-probed; their keys are not a stable OS contract.
use super::details::{
    GpuActivity, GpuActivityKind, GpuDetails, GpuMemoryArchitecture, GpuMemoryStatus, GpuTelemetry,
};
use super::observation_diagnostics::ObservationDiagnostics;
use super::{failed, unsupported, GpuAdapter, GpuAdapterUsage, GpuSample, PlatformResult};
use crate::diagnostics::text;
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    data::CFData,
    dictionary::{CFDictionary, CFMutableDictionaryRef},
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use std::{
    ffi::c_char,
    ptr,
    time::{Duration, Instant},
};

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
    fn IOServiceGetMatchingServices(
        port: u32,
        matching: CFMutableDictionaryRef,
        iterator: *mut u32,
    ) -> i32;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOObjectConformsTo(object: u32, class_name: *const c_char) -> u32;
    fn IORegistryEntryGetParentEntry(entry: u32, plane: *const c_char, parent: *mut u32) -> i32;
    fn IORegistryEntryGetPath(entry: u32, plane: *const c_char, path: *mut c_char) -> i32;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: CFStringRef,
        allocator: *const std::ffi::c_void,
        options: u32,
    ) -> CFTypeRef;
}

struct Object(u32);
impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            IOObjectRelease(self.0);
        }
    }
}
struct Adapter {
    object: Object,
    id: String,
    name: String,
    unified: bool,
    core_count: Option<u32>,
}

pub struct GpuReader {
    adapters: Vec<Adapter>,
    discovered_at: Option<Instant>,
    statistics_key: CFString,
    diagnostics: ObservationDiagnostics,
    discovery_failure: Option<(&'static str, String)>,
}
impl Default for GpuReader {
    fn default() -> Self {
        Self {
            adapters: Vec::new(),
            discovered_at: None,
            statistics_key: CFString::new("PerformanceStatistics"),
            diagnostics: ObservationDiagnostics::default(),
            discovery_failure: None,
        }
    }
}

fn property(object: u32, key: &CFString) -> Option<CFType> {
    let raw = unsafe {
        IORegistryEntryCreateCFProperty(object, key.as_concrete_TypeRef(), ptr::null(), 0)
    };
    (!raw.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(raw) })
}

fn model_name(value: &CFType) -> Option<String> {
    value
        .downcast::<CFString>()
        .map(|value| value.to_string())
        .or_else(|| {
            let data = value.downcast::<CFData>()?;
            std::str::from_utf8(data.bytes()).ok().map(str::to_owned)
        })
        .map(|value| value.trim_end_matches('\0').trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn adapter_model(object: u32, key: &CFString) -> Option<String> {
    if let Some(name) = property(object, key).as_ref().and_then(model_name) {
        return Some(name);
    }
    // Intel and AMD expose the model on the PCI device, sometimes behind a
    // controller node. Restrict the fallback to the nearest PCI device instead
    // of a recursive property search that could return the computer model.
    let mut parent: Option<Object> = None;
    for _ in 0..4 {
        let current = parent.as_ref().map_or(object, |parent| parent.0);
        let mut entry = 0;
        if unsafe { IORegistryEntryGetParentEntry(current, c"IOService".as_ptr(), &mut entry) } != 0
        {
            return None;
        }
        parent = Some(Object(entry));
        if unsafe { IOObjectConformsTo(entry, c"IOPCIDevice".as_ptr()) } != 0 {
            return property(entry, key).as_ref().and_then(model_name);
        }
    }
    None
}

fn activity(property: &CFType) -> Option<(f64, &'static str)> {
    let untyped = property.downcast::<CFDictionary>()?;
    let dictionary: CFDictionary<CFString, CFType> =
        unsafe { CFDictionary::wrap_under_get_rule(untyped.as_concrete_TypeRef()) };
    ["Device Utilization %", "GPU Activity(%)"]
        .into_iter()
        .find_map(|key| percentage(&dictionary, key).map(|value| (value, key)))
}

fn percentage(dictionary: &CFDictionary<CFString, CFType>, key: &str) -> Option<f64> {
    let value = dictionary
        .find(CFString::new(key))?
        .downcast::<CFNumber>()?
        .to_f64()?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
}

fn statistic(dictionary: &CFDictionary<CFString, CFType>, key: &str, maximum: f64) -> Option<f64> {
    let value = dictionary
        .find(CFString::new(key))?
        .downcast::<CFNumber>()?
        .to_f64()?;
    (value.is_finite() && value > 0.0 && value <= maximum).then_some(value)
}

fn details(property: &CFType, unified: bool, core_count: Option<u32>) -> GpuDetails {
    let mut activities = Vec::new();
    let mut telemetry = GpuTelemetry {
        core_count,
        ..Default::default()
    };
    if let Some(untyped) = property.downcast::<CFDictionary>() {
        let dictionary: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(untyped.as_concrete_TypeRef()) };
        // Optional driver facts share the existing property read. Zero clocks or
        // temperature are unavailable sentinels, while an idle fan may be zero.
        telemetry.temperature_celsius = statistic(&dictionary, "Temperature(C)", 150.0);
        telemetry.core_clock_mhz = statistic(&dictionary, "Core Clock(MHz)", 100_000.0);
        telemetry.memory_clock_mhz = statistic(&dictionary, "Memory Clock(MHz)", 100_000.0);
        telemetry.fan_percent = percentage(&dictionary, "Fan Speed(%)");
        for (key, kind, id) in [
            (
                "Renderer Utilization %",
                GpuActivityKind::Renderer,
                "renderer",
            ),
            ("Tiler Utilization %", GpuActivityKind::Tiler, "tiler"),
        ] {
            if let Some(used_percent) = percentage(&dictionary, key) {
                activities.push(GpuActivity {
                    id: id.into(),
                    kind,
                    name: None,
                    used_percent,
                    included_in_summary: false,
                });
            }
        }
    }
    GpuDetails {
        activities,
        telemetry,
        memory_architecture: if unified {
            GpuMemoryArchitecture::Unified
        } else {
            GpuMemoryArchitecture::Unknown
        },
        memory_status: GpuMemoryStatus::Unsupported,
        memory: None,
    }
}

impl GpuReader {
    fn discovery_error(
        &mut self,
        stage: &'static str,
        code: impl std::fmt::Display,
    ) -> crate::PlatformError {
        let code = code.to_string();
        if self
            .discovery_failure
            .as_ref()
            .is_none_or(|previous| previous.0 != stage || previous.1 != code)
        {
            log::warn!("gpu_inventory_unavailable source=ioreg_ioaccelerator stage={stage} native_code={} outcome=retain_previous_adapters", text(&code));
        }
        self.discovery_failure = Some((stage, code.clone()));
        failed(stage, code)
    }
    fn discover(&mut self) -> PlatformResult<()> {
        let matching = unsafe { IOServiceMatching(c"IOAccelerator".as_ptr()) };
        if matching.is_null() {
            return Err(self.discovery_error("matching", "null_dictionary"));
        }
        let mut iterator = 0;
        let result = unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) };
        if result != 0 {
            return Err(self.discovery_error("enumerate", result));
        }
        let iterator = Object(iterator);
        let model_key = CFString::new("model");
        let mut adapters = Vec::new();
        loop {
            let object = unsafe { IOIteratorNext(iterator.0) };
            if object == 0 {
                break;
            }
            let object = Object(object);
            let mut path = [0i8; 512];
            let result = unsafe {
                IORegistryEntryGetPath(object.0, c"IOService".as_ptr(), path.as_mut_ptr())
            };
            if result != 0 {
                return Err(self.discovery_error("identity", result));
            }
            let id = unsafe { std::ffi::CStr::from_ptr(path.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            let name = adapter_model(object.0, &model_key)
                .unwrap_or_else(|| format!("GPU {}", adapters.len()));
            let unified = property(object.0, &CFString::new("IOClass"))
                .and_then(|value| value.downcast::<CFString>())
                .is_some_and(|value| value.to_string().to_ascii_lowercase().contains("agx"));
            let core_count = property(object.0, &CFString::new("gpu-core-count"))
                .and_then(|value| value.downcast::<CFNumber>())
                .and_then(|value| value.to_i64())
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value > 0 && *value <= 16_384);
            adapters.push(Adapter {
                object,
                id,
                name,
                unified,
                core_count,
            });
        }
        if self.discovered_at.is_none() || adapters.len() != self.adapters.len() {
            log::info!(
                "gpu_inventory source=ioreg_ioaccelerator hardware_adapters={}",
                adapters.len()
            );
        }
        for adapter in &adapters {
            if !self.adapters.iter().any(|previous| {
                previous.id == adapter.id
                    && previous.name == adapter.name
                    && previous.unified == adapter.unified
                    && previous.core_count == adapter.core_count
            }) {
                log::info!("gpu_adapter_discovered source=ioreg_ioaccelerator adapter={} name={} unified={} core_count={:?} summary_policy=driver_device_utilization", text(&adapter.id), text(&adapter.name), adapter.unified, adapter.core_count);
            }
        }
        for previous in &self.adapters {
            if !adapters.iter().any(|adapter| adapter.id == previous.id) {
                log::info!(
                    "gpu_adapter_removed source=ioreg_ioaccelerator adapter={} name={}",
                    text(&previous.id),
                    text(&previous.name)
                );
            }
        }
        self.diagnostics
            .retain(|id| adapters.iter().any(|adapter| adapter.id == id));
        if self.discovery_failure.take().is_some() {
            log::info!("gpu_inventory_recovered source=ioreg_ioaccelerator");
        }
        self.adapters = adapters;
        self.discovered_at = Some(Instant::now());
        Ok(())
    }

    pub fn reset(&mut self) {
        // These driver values are instantaneous, with no interval baseline to reset.
        // Demand generations also change on tab switches; preserve metadata and diagnostics
        // until their regular refresh or the owning worker releases the reader.
    }

    fn refresh(&mut self) -> PlatformResult<()> {
        if self
            .discovered_at
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(30))
        {
            self.discover()?;
        }
        Ok(())
    }
    pub fn catalogue(&mut self) -> PlatformResult<Vec<GpuAdapter>> {
        self.refresh()?;
        Ok(self.adapters())
    }
    pub fn adapters(&self) -> Vec<GpuAdapter> {
        self.adapters
            .iter()
            .enumerate()
            .map(|(index, adapter)| GpuAdapter {
                id: adapter.id.clone(),
                name: format!("GPU {index} · {}", adapter.name),
            })
            .collect()
    }
    pub fn read(&mut self) -> PlatformResult<GpuSample> {
        self.sample()
    }
    pub fn read_detailed(&mut self) -> PlatformResult<GpuSample> {
        self.read()
    }
    fn sample(&mut self) -> PlatformResult<GpuSample> {
        self.refresh()?;
        let diagnostics = &mut self.diagnostics;
        let values: Vec<_> = self
            .adapters
            .iter()
            .filter_map(|adapter| {
                let Some(statistics) = property(adapter.object.0, &self.statistics_key) else {
                    diagnostics.observe(
                        &adapter.id,
                        "ioreg_performance_statistics",
                        Err("statistics_missing"),
                    );
                    return None;
                };
                let Some((used_percent, source)) = activity(&statistics) else {
                    diagnostics.observe(
                        &adapter.id,
                        "ioreg_performance_statistics",
                        Err("utilization_missing_or_invalid"),
                    );
                    return None;
                };
                let details = details(&statistics, adapter.unified, adapter.core_count);
                diagnostics.observe(&adapter.id, source, Ok(Some(&details)));
                Some(GpuAdapterUsage {
                    id: adapter.id.clone(),
                    name: adapter.name.clone(),
                    used_percent,
                    // Engine history must continue while the detail panel is hidden.
                    // These facts reuse the same native property read as total usage.
                    details: Some(details),
                })
            })
            .collect();
        if values.is_empty() {
            Err(unsupported())
        } else {
            Ok(GpuSample::Usage(values))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn driver_models_accept_strings_and_pci_bytes_without_empty_or_invalid_labels() {
        assert_eq!(
            model_name(&CFData::from_buffer(b"Intel UHD Graphics 630\0").as_CFType()),
            Some("Intel UHD Graphics 630".into())
        );
        assert_eq!(
            model_name(&CFData::from_buffer(b"Radeon Pro 560X").as_CFType()),
            Some("Radeon Pro 560X".into())
        );
        assert_eq!(
            model_name(&CFString::new("Apple M3 Max").as_CFType()),
            Some("Apple M3 Max".into())
        );
        assert!(model_name(&CFData::from_buffer(b"\xff").as_CFType()).is_none());
        assert!(model_name(&CFString::new(" \0").as_CFType()).is_none());
        assert!(model_name(&CFNumber::from(1).as_CFType()).is_none());
    }
    #[test]
    fn demand_resets_preserve_cached_native_adapter_handles() {
        let mut reader = GpuReader::default();
        reader.catalogue().unwrap();
        let discovered_at = reader.discovered_at;
        let objects: Vec<_> = reader
            .adapters
            .iter()
            .map(|adapter| adapter.object.0)
            .collect();
        reader.reset();
        assert_eq!(reader.discovered_at, discovered_at);
        assert_eq!(
            reader
                .adapters
                .iter()
                .map(|adapter| adapter.object.0)
                .collect::<Vec<_>>(),
            objects
        );
        reader.catalogue().unwrap();
        assert_eq!(reader.discovered_at, discovered_at);
    }
    #[test]
    fn optional_engines_never_infer_a_separate_vram_capacity() {
        let statistics = CFDictionary::from_CFType_pairs(&[
            (
                CFString::new("Renderer Utilization %"),
                CFNumber::from(12.0).as_CFType(),
            ),
            (
                CFString::new("Tiler Utilization %"),
                CFNumber::from(f64::NAN).as_CFType(),
            ),
            (
                CFString::new("In use system memory"),
                CFNumber::from(1024).as_CFType(),
            ),
        ])
        .as_CFType();
        let value = details(&statistics, true, Some(40));
        assert_eq!(value.activities.len(), 1);
        assert_eq!(value.activities[0].kind, GpuActivityKind::Renderer);
        assert_eq!(value.memory_architecture, GpuMemoryArchitecture::Unified);
        assert!(value.memory.is_none());
        assert_eq!(value.memory_status, GpuMemoryStatus::Unsupported);
    }

    #[test]
    fn optional_driver_telemetry_rejects_sentinels_and_invalid_values() {
        let statistics = CFDictionary::from_CFType_pairs(&[
            (
                CFString::new("Temperature(C)"),
                CFNumber::from(0.0).as_CFType(),
            ),
            (
                CFString::new("Core Clock(MHz)"),
                CFNumber::from(f64::INFINITY).as_CFType(),
            ),
            (
                CFString::new("Memory Clock(MHz)"),
                CFNumber::from(1500.0).as_CFType(),
            ),
            (
                CFString::new("Fan Speed(%)"),
                CFNumber::from(0.0).as_CFType(),
            ),
        ])
        .as_CFType();
        let value = details(&statistics, false, Some(40));
        assert_eq!(value.telemetry.core_count, Some(40));
        assert!(value.telemetry.temperature_celsius.is_none());
        assert!(value.telemetry.core_clock_mhz.is_none());
        assert_eq!(value.telemetry.memory_clock_mhz, Some(1500.0));
        assert_eq!(value.telemetry.fan_percent, Some(0.0));
    }

    #[test]
    fn malformed_statistics_never_become_idle() {
        let value = |number: f64| {
            CFDictionary::from_CFType_pairs(&[(
                CFString::new("Device Utilization %"),
                CFNumber::from(number).as_CFType(),
            )])
            .as_CFType()
        };
        assert_eq!(activity(&value(0.0)), Some((0.0, "Device Utilization %")));
        assert_eq!(
            activity(&value(100.0)),
            Some((100.0, "Device Utilization %"))
        );
        assert_eq!(activity(&value(-1.0)), None);
        assert_eq!(activity(&value(f64::NAN)), None);
        assert_eq!(activity(&value(101.0)), None);
        assert_eq!(activity(&CFString::new("invalid").as_CFType()), None);
    }
}
