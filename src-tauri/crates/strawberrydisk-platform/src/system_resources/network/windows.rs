use std::{
    collections::HashMap,
    ptr,
    time::{Duration, Instant},
};

#[path = "windows_changes.rs"]
mod changes;

use windows_sys::Win32::{
    NetworkManagement::{
        IpHelper::{
            FreeMibTable, GetIfTable2, GetIpForwardTable2, GetIpInterfaceEntry, MIB_IF_TABLE2,
            MIB_IPFORWARD_TABLE2, MIB_IPINTERFACE_ROW,
        },
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::AF_UNSPEC,
};

use super::{InterfaceKind, InterfaceSample, NetworkCounters, NetworkInterface};
use crate::{PlatformError, PlatformErrorCode, PlatformResult};

/// IP Helper owns table allocation; free on every exit, including decoding failures.
struct Table(*mut std::ffi::c_void);
impl Drop for Table {
    fn drop(&mut self) {
        unsafe { FreeMibTable(self.0) };
    }
}

fn default_routes() -> PlatformResult<HashMap<u32, u32>> {
    let mut result = HashMap::new();
    let mut table: *mut MIB_IPFORWARD_TABLE2 = ptr::null_mut();
    unsafe {
        let code = GetIpForwardTable2(AF_UNSPEC, &mut table);
        if code != 0 || table.is_null() {
            return Err(PlatformError::operation_failed(format!(
                "network route query failed native_code={code}"
            )));
        }
        let _owned = Table(table.cast());
        for row in std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize)
        {
            if row.DestinationPrefix.PrefixLength != 0 {
                continue;
            }
            let mut interface = MIB_IPINTERFACE_ROW {
                Family: row.DestinationPrefix.Prefix.si_family,
                InterfaceIndex: row.InterfaceIndex,
                ..Default::default()
            };
            // Route preference includes both route and interface metrics. IPv4
            // and IPv6 routes for one adapter must never cause double counting.
            let metric = if GetIpInterfaceEntry(&mut interface) == 0 {
                row.Metric.saturating_add(interface.Metric)
            } else {
                row.Metric
            };
            result
                .entry(row.InterfaceIndex)
                .and_modify(|current: &mut u32| *current = (*current).min(metric))
                .or_insert(metric);
        }
    }
    Ok(result)
}

#[derive(Default)]
pub struct Reader {
    routes: HashMap<u32, u32>,
    refreshed: Option<(Instant, u64)>,
    subscription: Option<changes::Subscription>,
    registration_attempted: bool,
    route_failed: bool,
}
impl Reader {
    pub fn read(&mut self) -> PlatformResult<Vec<InterfaceSample>> {
        if !self.registration_attempted {
            self.registration_attempted = true;
            self.subscription = changes::Subscription::open();
        }
        let now = Instant::now();
        let epoch = changes::epoch();
        if metadata_due(self.refreshed, now, epoch, self.subscription.is_some()) {
            match default_routes() {
                Ok(routes) => {
                    self.routes = routes;
                    // Capture the epoch before querying: a concurrent callback must trigger
                    // another refresh rather than being acknowledged by an older snapshot.
                    self.refreshed = Some((now, epoch));
                    if self.route_failed {
                        log::info!("network_route_metadata_recovered");
                    }
                    self.route_failed = false;
                }
                Err(error) => {
                    // A failed lookup must not cache an empty or old routing table for
                    // thirty seconds. Preserve the previous per-sample retry behavior.
                    self.routes.clear();
                    self.refreshed = None;
                    if !self.route_failed {
                        log::warn!("network_route_metadata_unavailable error={} fallback=no_route_preference", crate::diagnostics::text(&error));
                    }
                    self.route_failed = true;
                }
            }
        }
        let routes = &self.routes;
        let mut table: *mut MIB_IF_TABLE2 = ptr::null_mut();
        unsafe {
            if GetIfTable2(&mut table) != 0 || table.is_null() {
                return Err(PlatformError::new(
                    PlatformErrorCode::OperationFailed,
                    "network counters unavailable",
                ));
            }
            let _owned = Table(table.cast());
            let rows =
                std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
            Ok(rows
                .iter()
                .map(|row| {
                    let physical = row.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
                    let kind = match (physical, row.Type) {
                        (true, 71) => InterfaceKind::Wifi,
                        (true, 6) => InterfaceKind::Ethernet,
                        (true, _) => InterfaceKind::Other,
                        (false, _) => InterfaceKind::Virtual,
                    };
                    let connected = row.OperStatus == IfOperStatusUp;
                    let guid = row.InterfaceGuid;
                    let suffix = guid
                        .data4
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>();
                    InterfaceSample {
                        interface: NetworkInterface {
                            id: format!(
                                "{:08x}-{:04x}-{:04x}-{suffix}",
                                guid.data1, guid.data2, guid.data3
                            ),
                            name: String::from_utf16_lossy(
                                &row.Alias[..row
                                    .Alias
                                    .iter()
                                    .position(|value| *value == 0)
                                    .unwrap_or(row.Alias.len())],
                            ),
                            kind,
                            connected,
                            physical,
                            default_route_metric: routes.get(&row.InterfaceIndex).copied(),
                        },
                        counters: connected.then_some(NetworkCounters {
                            received: row.InOctets,
                            transmitted: row.OutOctets,
                        }),
                    }
                })
                .collect())
        }
    }
}

fn metadata_due(
    previous: Option<(Instant, u64)>,
    now: Instant,
    epoch: u64,
    subscribed: bool,
) -> bool {
    !subscribed
        || previous.is_none_or(|(at, before)| {
            before != epoch || now.saturating_duration_since(at) >= Duration::from_secs(30)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_changes_expiry_and_missing_notifications_force_metadata_refresh() {
        let now = Instant::now();
        assert!(metadata_due(None, now, 1, true));
        let previous = Some((now, 1));
        assert!(!metadata_due(
            previous,
            now + Duration::from_secs(29),
            1,
            true
        ));
        assert!(metadata_due(
            previous,
            now + Duration::from_secs(30),
            1,
            true
        ));
        assert!(metadata_due(previous, now, 2, true));
        assert!(metadata_due(previous, now, 1, false));
    }

    #[test]
    fn cached_routes_match_a_fresh_native_read_and_invalidation_reloads_them() {
        let mut reader = Reader::default();
        reader.read().expect("initial counters must be readable");
        assert_eq!(reader.routes, default_routes().unwrap());
        reader.refreshed = Some((Instant::now(), changes::epoch().wrapping_sub(1)));
        reader.routes.clear();
        reader
            .read()
            .expect("invalidated counters must be readable");
        assert_eq!(reader.routes, default_routes().unwrap());
    }
}
