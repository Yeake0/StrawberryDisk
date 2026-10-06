use super::{unavailable, DeviceCounters};
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    dictionary::{CFDictionary, CFMutableDictionaryRef},
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use std::{ffi::c_char, ptr};

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
    fn IORegistryEntryGetRegistryEntryID(entry: u32, id: *mut u64) -> i32;
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

#[derive(Default)]
pub struct DiskIoReader {}
impl DiskIoReader {
    pub fn read(&mut self) -> crate::PlatformResult<Vec<DeviceCounters>> {
        let matching = unsafe { IOServiceMatching(c"IOBlockStorageDriver".as_ptr()) };
        if matching.is_null() {
            return Err(unavailable());
        }
        let mut iterator = 0;
        // IOKit consumes the matching dictionary. Driver-level enumeration avoids
        // counting shared APFS volumes repeatedly and requires no privileged helper.
        if unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) } != 0 {
            return Err(unavailable());
        }
        let iterator = Object(iterator);
        let key = CFString::new("Statistics");
        let mut devices = Vec::new();
        loop {
            let entry = unsafe { IOIteratorNext(iterator.0) };
            if entry == 0 {
                break;
            }
            let entry = Object(entry);
            let mut id = 0;
            if unsafe { IORegistryEntryGetRegistryEntryID(entry.0, &mut id) } != 0 {
                return Err(unavailable());
            }
            let raw = unsafe {
                IORegistryEntryCreateCFProperty(entry.0, key.as_concrete_TypeRef(), ptr::null(), 0)
            };
            if raw.is_null() {
                continue;
            }
            let property = unsafe { CFType::wrap_under_create_rule(raw) };
            let Some(untyped) = property.downcast::<CFDictionary>() else {
                return Err(unavailable());
            };
            let dictionary: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(untyped.as_concrete_TypeRef()) };
            let number = |name: &str| -> Option<u64> {
                dictionary
                    .find(CFString::new(name))?
                    .downcast::<CFNumber>()?
                    .to_i64()?
                    .try_into()
                    .ok()
            };
            devices.push(DeviceCounters {
                id: id.to_string(),
                read_bytes: number("Bytes (Read)").ok_or_else(unavailable)?,
                written_bytes: number("Bytes (Write)").ok_or_else(unavailable)?,
            });
        }
        devices.sort_by(|a, b| a.id.cmp(&b.id));
        if devices.is_empty() {
            Err(unavailable())
        } else {
            Ok(devices)
        }
    }
}
