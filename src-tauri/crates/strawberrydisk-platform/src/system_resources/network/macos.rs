use std::{
    collections::HashMap,
    ffi::CStr,
    mem::size_of,
    ptr,
    time::{Duration, Instant},
};

use core_foundation::{
    base::{CFType, TCFType, ToVoid},
    dictionary::CFDictionary,
    string::CFString,
};
use system_configuration::{
    dynamic_store::{SCDynamicStore, SCDynamicStoreBuilder},
    network_configuration::{get_interfaces, SCNetworkInterfaceType},
};

use super::{InterfaceKind, InterfaceSample, NetworkCounters, NetworkInterface};
use crate::{PlatformError, PlatformErrorCode, PlatformResult};

fn primary_interfaces(store: Option<&SCDynamicStore>) -> Vec<String> {
    // SystemConfiguration exposes local routing state without opening a socket
    // to a public destination. VPN primaries are filtered by physical identity below.
    ["State:/Network/Global/IPv4", "State:/Network/Global/IPv6"]
        .iter()
        .filter_map(|key| {
            let value = store?
                .get(CFString::new(key))?
                .downcast_into::<CFDictionary>()?;
            let key = CFString::new("PrimaryInterface");
            let raw = value.find(key.to_void())?;
            unsafe { CFType::wrap_under_get_rule(*raw) }
                .downcast_into::<CFString>()
                .map(|name| name.to_string())
        })
        .collect()
}

#[derive(Default)]
pub struct Reader {
    store: Option<SCDynamicStore>,
    physical: HashMap<String, (InterfaceKind, Option<String>)>,
    topology: Vec<String>,
    refreshed: Option<Instant>,
}
impl Reader {
    pub fn read(&mut self) -> PlatformResult<Vec<InterfaceSample>> {
        if self.store.is_none() {
            self.store = SCDynamicStoreBuilder::new("StrawberryDisk resource interfaces").build();
        }
        let primary = primary_interfaces(self.store.as_ref());
        let mut mib = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
        let mut length = 0;
        unsafe {
            if libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as _,
                ptr::null_mut(),
                &mut length,
                ptr::null_mut(),
                0,
            ) != 0
                || length > 16 * 1024 * 1024
            {
                return Err(unavailable());
            }
        }
        let mut bytes = vec![0u8; length];
        unsafe {
            if libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as _,
                bytes.as_mut_ptr().cast(),
                &mut length,
                ptr::null_mut(),
                0,
            ) != 0
            {
                return Err(unavailable());
            }
        }
        bytes.truncate(length);
        let mut offset = 0;
        let mut result = Vec::new();
        while offset + 4 <= bytes.len() {
            // All route messages share only the first four bytes. Address records
            // are shorter than if_msghdr and must be skipped using their own length.
            let size = usize::from(u16::from_ne_bytes([bytes[offset], bytes[offset + 1]]));
            let message_type = i32::from(bytes[offset + 3]);
            if size < 4 || offset + size > bytes.len() {
                return Err(unavailable());
            }
            if message_type == libc::RTM_IFINFO2 && size >= size_of::<libc::if_msghdr2>() {
                let info = unsafe {
                    ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<libc::if_msghdr2>())
                };
                // IFMIB supplies both the 64-bit counters and interface name.
                // Calling if_indextoname for every interface would repeatedly
                // enumerate the entire interface table on macOS.
                let mut data: libc::ifmibdata = unsafe { std::mem::zeroed() };
                let mut count = size_of::<libc::ifmibdata>();
                let mut query = [
                    libc::CTL_NET,
                    libc::PF_LINK,
                    0,
                    2,
                    i32::from(info.ifm_index),
                    1,
                ];
                let valid = unsafe {
                    libc::sysctl(
                        query.as_mut_ptr(),
                        query.len() as _,
                        (&mut data as *mut libc::ifmibdata).cast(),
                        &mut count,
                        ptr::null_mut(),
                        0,
                    )
                } == 0
                    && count == size_of::<libc::ifmibdata>();
                let mut name = data.ifmd_name;
                if valid
                    || !unsafe {
                        libc::if_indextoname(u32::from(info.ifm_index), name.as_mut_ptr())
                    }
                    .is_null()
                {
                    name[libc::IFNAMSIZ - 1] = 0;
                    let id = unsafe { CStr::from_ptr(name.as_ptr()) }
                        .to_string_lossy()
                        .into_owned();
                    let connected = info.ifm_flags & libc::IFF_UP != 0
                        && info.ifm_flags & libc::IFF_RUNNING != 0;
                    result.push(InterfaceSample {
                        interface: NetworkInterface {
                            name: id.clone(),
                            kind: InterfaceKind::Virtual,
                            connected,
                            physical: false,
                            default_route_metric: primary
                                .iter()
                                .any(|name| name == &id)
                                .then_some(0),
                            id,
                        },
                        counters: (valid && connected).then_some(NetworkCounters {
                            received: data.ifmd_data.ifi_ibytes,
                            transmitted: data.ifmd_data.ifi_obytes,
                        }),
                    });
                }
            }
            offset += size;
        }
        let topology = result
            .iter()
            .map(|sample| sample.interface.id.clone())
            .collect::<Vec<_>>();
        // IOKit-backed interface discovery is much slower than reading counters.
        // Refresh immediately on topology changes and every ten seconds for renames;
        // link state, routing and byte counters are still read on every tick.
        if self.topology != topology
            || self
                .refreshed
                .is_none_or(|time| time.elapsed() >= Duration::from_secs(10))
        {
            self.physical.clear();
            for interface in get_interfaces().iter() {
                let kind = match interface.interface_type() {
                    Some(SCNetworkInterfaceType::Ethernet) => InterfaceKind::Ethernet,
                    Some(SCNetworkInterfaceType::IEEE80211) => InterfaceKind::Wifi,
                    _ => continue,
                };
                if let Some(name) = interface.bsd_name() {
                    self.physical.insert(
                        name.to_string(),
                        (kind, interface.display_name().map(|name| name.to_string())),
                    );
                }
            }

            self.topology = topology;
            self.refreshed = Some(Instant::now());
        }
        for sample in &mut result {
            if let Some((kind, name)) = self.physical.get(&sample.interface.id) {
                sample.interface.kind = *kind;
                sample.interface.physical = true;
                if let Some(name) = name {
                    sample.interface.name.clone_from(name);
                }
            }
        }
        Ok(result)
    }
}

fn unavailable() -> PlatformError {
    PlatformError::new(
        PlatformErrorCode::OperationFailed,
        "network counters unavailable",
    )
}
