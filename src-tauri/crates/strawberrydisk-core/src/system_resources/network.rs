use serde::Serialize;
use strawberrydisk_platform::system_resources::network::{
    InterfaceSample, NetworkCounters, NetworkInterface,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkSelectionReason {
    Manual,
    DefaultRoute,
    PhysicalFallback,
    Disconnected,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRate {
    pub interface: NetworkInterface,
    pub received_bytes_per_second: f64,
    pub transmitted_bytes_per_second: f64,
    pub selection_reason: NetworkSelectionReason,
}

/// A manual identity is sticky across disconnects. Automatic selection prefers
/// a physical default route, then retains a connected physical fallback rather
/// than chasing whichever adapter happens to transfer most bytes this second.
pub fn select<'a>(
    interfaces: &'a [InterfaceSample],
    manual: Option<&str>,
    previous: Option<&str>,
) -> (Option<&'a InterfaceSample>, NetworkSelectionReason) {
    if let Some(id) = manual {
        return (
            interfaces.iter().find(|sample| sample.interface.id == id),
            NetworkSelectionReason::Manual,
        );
    }
    let candidates = || {
        interfaces
            .iter()
            .filter(|sample| sample.interface.connected && sample.interface.physical)
    };
    if let Some(route) = candidates()
        .filter(|sample| sample.interface.default_route_metric.is_some())
        .min_by_key(|sample| (sample.interface.default_route_metric, &sample.interface.id))
    {
        return (Some(route), NetworkSelectionReason::DefaultRoute);
    }
    let chosen = previous
        .and_then(|id| candidates().find(|sample| sample.interface.id == id))
        .or_else(|| candidates().min_by_key(|sample| &sample.interface.id));
    let reason = if chosen.is_some() {
        NetworkSelectionReason::PhysicalFallback
    } else {
        NetworkSelectionReason::Disconnected
    };
    (chosen, reason)
}

#[derive(Default)]
pub struct NetworkDelta {
    previous: Option<(String, NetworkCounters, u64)>,
}

impl NetworkDelta {
    pub fn sample(
        &mut self,
        interface: &NetworkInterface,
        counters: NetworkCounters,
        elapsed_ms: u64,
        selection_reason: NetworkSelectionReason,
    ) -> Option<NetworkRate> {
        let previous = self
            .previous
            .replace((interface.id.clone(), counters, elapsed_ms))?;
        if previous.0 != interface.id {
            return None;
        }
        let elapsed = elapsed_ms.checked_sub(previous.2)?;
        if !(100..=5_000).contains(&elapsed) {
            return None;
        }
        let received = counters.received.checked_sub(previous.1.received)?;
        let transmitted = counters.transmitted.checked_sub(previous.1.transmitted)?;
        Some(NetworkRate {
            interface: interface.clone(),
            received_bytes_per_second: received as f64 * 1000.0 / elapsed as f64,
            transmitted_bytes_per_second: transmitted as f64 * 1000.0 / elapsed as f64,
            selection_reason,
        })
    }

    pub fn reset(&mut self) {
        self.previous = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strawberrydisk_platform::system_resources::network::InterfaceKind;

    fn interface(id: &str, physical: bool, metric: Option<u32>) -> InterfaceSample {
        InterfaceSample {
            interface: NetworkInterface {
                id: id.into(),
                name: id.into(),
                kind: InterfaceKind::Ethernet,
                physical,
                connected: true,
                default_route_metric: metric,
            },
            counters: None,
        }
    }

    #[test]
    fn automatic_selection_avoids_virtual_double_counting_and_is_stable() {
        let rows = [
            interface("vpn", false, Some(0)),
            interface("lan", true, Some(10)),
            interface("wifi", true, Some(20)),
        ];
        assert_eq!(select(&rows, None, None).0.unwrap().interface.id, "lan");
        assert_eq!(
            select(&rows, Some("vpn"), None).0.unwrap().interface.id,
            "vpn"
        );
        assert!(select(&rows, Some("missing"), None).0.is_none());
        let fallback = [interface("a", true, None), interface("b", true, None)];
        assert_eq!(
            select(&fallback, None, Some("b")).0.unwrap().interface.id,
            "b"
        );
        assert_eq!(
            select(&fallback, None, None).1,
            NetworkSelectionReason::PhysicalFallback
        );
    }

    #[test]
    fn rates_use_actual_elapsed_time_in_both_directions() {
        let interface = interface("lan", true, None).interface;
        let mut delta = NetworkDelta::default();
        let reason = NetworkSelectionReason::Manual;
        assert!(delta
            .sample(
                &interface,
                NetworkCounters {
                    received: 100,
                    transmitted: 200
                },
                0,
                reason
            )
            .is_none());
        let value = delta
            .sample(
                &interface,
                NetworkCounters {
                    received: 3100,
                    transmitted: 6200,
                },
                1500,
                reason,
            )
            .unwrap();
        assert_eq!(value.received_bytes_per_second, 2000.0);
        assert_eq!(value.transmitted_bytes_per_second, 4000.0);
        let idle = delta
            .sample(
                &interface,
                NetworkCounters {
                    received: 3100,
                    transmitted: 6200,
                },
                2500,
                reason,
            )
            .unwrap();
        assert_eq!(idle.received_bytes_per_second, 0.0);
    }

    #[test]
    fn resets_interface_changes_and_sleep_do_not_make_peaks() {
        let one = interface("one", true, None).interface;
        let two = interface("two", true, None).interface;
        let mut delta = NetworkDelta::default();
        let reason = NetworkSelectionReason::Manual;
        let counters = NetworkCounters {
            received: 100,
            transmitted: 100,
        };
        assert!(delta.sample(&one, counters, 0, reason).is_none());
        assert!(delta.sample(&two, counters, 1000, reason).is_none());
        assert!(delta
            .sample(
                &two,
                NetworkCounters {
                    received: 1,
                    transmitted: 1
                },
                2000,
                reason
            )
            .is_none());
        assert!(delta.sample(&two, counters, 60000, reason).is_none());
        assert!(delta.sample(&two, counters, 61000, reason).is_some());
        delta.reset();
        assert!(delta.sample(&two, counters, 62000, reason).is_none());
    }
}
