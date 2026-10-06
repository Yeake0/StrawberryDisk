//! Native topology notifications invalidate metadata without touching sampling or UI state.
use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    NetworkManagement::IpHelper::{
        CancelMibChangeNotify2, NotifyIpInterfaceChange, NotifyRouteChange2, MIB_IPFORWARD_ROW2,
        MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE,
    },
    Networking::WinSock::AF_UNSPEC,
};

// Callbacks reference only process-lifetime state, never a reader that could be
// dropped. They neither take locks nor call back into Windows or the UI.
static EPOCH: AtomicU64 = AtomicU64::new(0);

pub(super) fn epoch() -> u64 {
    EPOCH.load(Ordering::Relaxed)
}

unsafe extern "system" fn route_changed(
    _context: *const c_void,
    _row: *const MIB_IPFORWARD_ROW2,
    _kind: MIB_NOTIFICATION_TYPE,
) {
    EPOCH.fetch_add(1, Ordering::Relaxed);
}

unsafe extern "system" fn interface_changed(
    _context: *const c_void,
    _row: *const MIB_IPINTERFACE_ROW,
    _kind: MIB_NOTIFICATION_TYPE,
) {
    EPOCH.fetch_add(1, Ordering::Relaxed);
}

pub(super) struct Subscription([usize; 2]);
impl Subscription {
    pub(super) fn open() -> Option<Self> {
        let mut subscription = Self([0; 2]);
        let mut handle = ptr::null_mut();
        let code = unsafe {
            NotifyRouteChange2(
                AF_UNSPEC,
                Some(route_changed),
                ptr::null(),
                false,
                &mut handle,
            )
        };
        if code != 0 {
            log::warn!("network_metadata_notifications_unavailable stage=routes native_code={code} fallback=per_sample");
            return None;
        }
        subscription.0[0] = handle as usize;
        let code = unsafe {
            NotifyIpInterfaceChange(
                AF_UNSPEC,
                Some(interface_changed),
                ptr::null(),
                false,
                &mut handle,
            )
        };
        if code != 0 {
            log::warn!("network_metadata_notifications_unavailable stage=interfaces native_code={code} fallback=per_sample");
            return None;
        }
        subscription.0[1] = handle as usize;
        Some(subscription)
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        // Cancellation runs on the owning worker, never from a callback. The
        // callbacks cannot wait on this worker, so cancellation cannot deadlock
        // through a shared lock or a UI dependency.
        for handle in self.0.into_iter().filter(|handle| *handle != 0) {
            let code = unsafe { CancelMibChangeNotify2(handle as _) };
            if code != 0 {
                log::warn!("network_metadata_notifications_cancel_failed native_code={code}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callbacks_invalidate_without_dereferencing_native_or_reader_pointers() {
        let before = epoch();
        unsafe {
            route_changed(ptr::null(), ptr::null(), 0);
            interface_changed(ptr::null(), ptr::null(), 0);
        }
        assert!(epoch().wrapping_sub(before) >= 2);
    }

    #[test]
    fn native_topology_subscriptions_can_be_repeatedly_created_and_released() {
        for _ in 0..3 {
            let subscription = Subscription::open().expect("native notifications must register");
            assert!(subscription.0.iter().all(|handle| *handle != 0));
            drop(subscription);
        }
    }
}
