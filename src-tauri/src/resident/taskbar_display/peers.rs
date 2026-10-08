//! Foreign taskbar children can own competing task-button reservations. Their
//! native bounds matter even when an owner-drawn surface exposes no UIA controls.
use super::layout::Bounds;
use windows_sys::{
    core::w,
    Win32::{
        Foundation::*,
        Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONULL},
        UI::WindowsAndMessaging::*,
    },
};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Peers {
    pub present: bool,
    pub blocks_reservation: bool,
    pub occupied: Vec<Bounds>,
}

impl Peers {
    pub fn allows_reservation(&self, environment: super::position::Environment) -> bool {
        use super::position::Environment;
        match environment {
            // Only the classic shell has independent inner/outer HWND leases.
            Environment::Windows10 => !self.blocks_reservation,
            // XAML reservations cannot coordinate with another program's panel.
            // This also covers hidden peers while they initialize their layout.
            Environment::Windows11Centered | Environment::Windows11LeftAligned => !self.present,
            Environment::Unknown => false,
        }
    }
}

pub unsafe fn inspect(parent: HWND) -> Peers {
    let mut peers = Peers::default();
    EnumChildWindows(parent, Some(visit), (&mut peers as *mut Peers) as LPARAM);
    // TrafficMonitor still resizes task buttons when SetParent fails, then
    // positions its named top-level dialog over the taskbar instead.
    let mut detached = std::ptr::null_mut();
    loop {
        detached = FindWindowExW(
            std::ptr::null_mut(),
            detached,
            std::ptr::null(),
            w!("TrafficMonitorTaskbarWindow"),
        );
        if detached.is_null() {
            break;
        }
        record_detached(detached, parent, &mut peers);
    }
    peers
}

unsafe fn record_detached(window: HWND, parent: HWND, peers: &mut Peers) {
    // A fallback on a secondary display must not disable primary reservations.
    // Do not map offscreen/uninitialized windows to the nearest display.
    let host_monitor = MonitorFromWindow(parent, MONITOR_DEFAULTTONULL);
    let peer_monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONULL);
    if !host_monitor.is_null() && host_monitor == peer_monitor {
        record(window, peers);
    }
}

unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> i32 {
    // GetParent returns an owner for WS_POPUP dialogs; GA_PARENT returns the
    // actual hosting parent even when a peer retains its original popup style.
    let parent = GetAncestor(window, GA_PARENT);
    let mut owner = 0;
    let mut parent_owner = 0;
    GetWindowThreadProcessId(window, &mut owner);
    GetWindowThreadProcessId(parent, &mut parent_owner);
    // Only a process boundary introduces a foreign embedded surface. Nested
    // controls are covered by that surface's full rectangle.
    if owner == 0 || owner == parent_owner {
        return 1;
    }
    let mut class = [0u16; 128];
    let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) as usize;
    if class[..length]
        .iter()
        .copied()
        .eq("StrawberryDiskTaskbarStatus".encode_utf16())
    {
        return 1;
    }
    record(window, &mut *(data as *mut Peers));
    1
}

unsafe fn record(window: HWND, peers: &mut Peers) {
    // Hidden peers may already be adjusting Explorer before their first paint.
    peers.present = true;
    let mut title = [0u16; 128];
    let length = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) as usize;
    // TrafficMonitor owns MSTaskSwWClass, leaving its inner task list available.
    // Unknown integrations may mutate either level, so retain the read-only fallback.
    peers.blocks_reservation |= !title[..length]
        .iter()
        .copied()
        .eq("TrafficMonitorTaskbarWindow".encode_utf16());
    let mut rect = RECT::default();
    if IsWindowVisible(window) != 0 && GetWindowRect(window, &mut rect) != 0 {
        peers.occupied.push(Bounds {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_peer_only_allows_independent_reservation_on_the_classic_shell() {
        use super::super::position::Environment;
        let mut peers = Peers::default();
        for environment in [
            Environment::Windows11Centered,
            Environment::Windows11LeftAligned,
        ] {
            assert!(peers.allows_reservation(environment));
        }
        // A hidden known peer has no occupied pixels yet, but already owns layout.
        peers.present = true;
        assert!(peers.allows_reservation(Environment::Windows10));
        for environment in [
            Environment::Windows11Centered,
            Environment::Windows11LeftAligned,
        ] {
            assert!(!peers.allows_reservation(environment));
        }
        peers.blocks_reservation = true;
        assert!(!peers.allows_reservation(Environment::Windows10));
        assert!(!Peers::default().allows_reservation(Environment::Unknown));
    }

    #[test]
    fn unknown_surfaces_block_reservation_but_traffic_monitor_owns_only_outer_host() {
        unsafe {
            let window = CreateWindowExW(
                0,
                w!("STATIC"),
                w!("TrafficMonitorTaskbarWindow"),
                WS_POPUP,
                0,
                0,
                20,
                20,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!window.is_null());
            let mut known = Peers::default();
            record(window, &mut known);
            SetWindowTextW(window, w!("Other taskbar extension"));
            let mut unknown = Peers::default();
            record(window, &mut unknown);
            DestroyWindow(window);
            assert!(known.present && !known.blocks_reservation);
            assert!(unknown.present && unknown.blocks_reservation);
            assert!(
                known.occupied.is_empty(),
                "hidden surfaces do not occupy pixels"
            );
        }
    }

    #[test]
    fn detached_search_continues_after_an_offscreen_first_match() {
        unsafe {
            use windows_sys::Win32::UI::HiDpi::{
                SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            };
            let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let outside =
                GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) + 1024;
            // All three HWNDs belong to this test. The first named match is
            // deliberately offscreen; a single FindWindow would miss the local peer.
            let windows = [10, 20, outside].map(|x| {
                CreateWindowExW(
                    0,
                    w!("STATIC"),
                    std::ptr::null(),
                    WS_POPUP,
                    x,
                    10,
                    20,
                    20,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            });
            assert!(windows.iter().all(|window| !window.is_null()));
            let [host, local, unrelated] = windows;
            SetWindowTextW(local, w!("TrafficMonitorTaskbarWindow"));
            SetWindowTextW(unrelated, w!("TrafficMonitorTaskbarWindow"));
            SetWindowPos(
                unrelated,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let first = FindWindowW(std::ptr::null(), w!("TrafficMonitorTaskbarWindow"));
            let peers = inspect(host);
            for window in windows {
                DestroyWindow(window);
            }
            SetThreadDpiAwarenessContext(previous);
            assert_eq!(
                first, unrelated,
                "the regression requires an unrelated first match"
            );
            assert!(
                peers.present,
                "the later local peer must still be discovered"
            );
            assert!(!peers.blocks_reservation);
        }
    }

    #[test]
    fn detached_windows_only_affect_their_host_monitor() {
        unsafe {
            use windows_sys::Win32::Graphics::Gdi::{
                EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
            };
            use windows_sys::Win32::UI::HiDpi::{
                SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            };
            let previous_dpi =
                SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            unsafe extern "system" fn collect(
                _: HMONITOR,
                _: HDC,
                rect: *mut RECT,
                data: LPARAM,
            ) -> i32 {
                (&mut *(data as *mut Vec<RECT>)).push(*rect);
                1
            }
            // Hidden test-owned HWNDs exercise native monitor assignment without
            // touching Explorer or requiring its taskbar to be initialized.
            let shell = CreateWindowExW(
                0,
                w!("STATIC"),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                20,
                20,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!shell.is_null());
            let monitor = MonitorFromWindow(shell, MONITOR_DEFAULTTONULL);
            assert!(!monitor.is_null());
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            assert_ne!(GetMonitorInfoW(monitor, &mut info), 0);
            let window = CreateWindowExW(
                0,
                w!("STATIC"),
                std::ptr::null(),
                WS_POPUP,
                info.rcMonitor.left + 10,
                info.rcMonitor.top + 10,
                20,
                20,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!window.is_null());
            let mut local = Peers::default();
            record_detached(window, shell, &mut local);
            let mut monitors = Vec::<RECT>::new();
            assert_ne!(
                EnumDisplayMonitors(
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    Some(collect),
                    (&mut monitors as *mut Vec<RECT>) as LPARAM
                ),
                0
            );
            for rect in &monitors {
                assert_ne!(
                    MoveWindow(window, rect.left + 10, rect.top + 10, 20, 20, 0),
                    0
                );
                let mut selected = Peers::default();
                record_detached(window, shell, &mut selected);
                assert_eq!(
                    selected.present,
                    rect.left == info.rcMonitor.left && rect.top == info.rcMonitor.top
                );
            }
            println!(
                "detached_monitor_assignment verified_displays={}",
                monitors.len()
            );
            let outside =
                GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) + 1024;
            assert_ne!(MoveWindow(window, outside, 0, 20, 20, 0), 0);
            let mut unrelated = Peers::default();
            record_detached(window, shell, &mut unrelated);
            DestroyWindow(window);
            DestroyWindow(shell);
            SetThreadDpiAwarenessContext(previous_dpi);
            assert!(local.present);
            assert!(!unrelated.present);
        }
    }

    #[test]
    #[ignore = "requires an interactive Windows 10 desktop with TrafficMonitor taskbar display enabled"]
    fn live_traffic_monitor_keeps_its_outer_container_during_inner_reservation() {
        unsafe {
            windows_sys::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(
                windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            );
            let shell = FindWindowW(w!("Shell_TrayWnd"), std::ptr::null());
            let parent = super::super::hosting::parent(shell);
            assert!(!parent.is_null());
            let embedded = FindWindowExW(
                parent,
                std::ptr::null_mut(),
                std::ptr::null(),
                w!("TrafficMonitorTaskbarWindow"),
            );
            let detached = FindWindowW(std::ptr::null(), w!("TrafficMonitorTaskbarWindow"));
            assert!(
                !embedded.is_null() || !detached.is_null(),
                "enable TrafficMonitor taskbar display before running this test"
            );
            let peers = inspect(parent);
            println!(
                "shared_host={} occupied={:?} detached={}",
                peers.present,
                peers.occupied,
                !detached.is_null()
            );
            assert!(peers.present);
            assert!(!peers.occupied.is_empty());
            let host = super::super::hosting::client_bounds(parent).unwrap();
            let request = super::super::reservation::Request::new(
                shell as usize,
                parent as usize,
                host,
                (344, 72),
                8,
                super::super::position::Edge::Right,
            );
            assert!(!peers.blocks_reservation);
            // Run with StrawberryDisk closed: this acceptance test owns a temporary
            // lease and restores it before returning, even if an assertion fails.
            assert!(
                FindWindowExW(
                    parent,
                    std::ptr::null_mut(),
                    w!("StrawberryDiskTaskbarStatus"),
                    std::ptr::null()
                )
                .is_null(),
                "close StrawberryDisk before this test"
            );
            let (container, buttons) = super::super::hosting::task_list(parent).unwrap();
            let outer = super::super::hosting::client_bounds(container).unwrap();
            let before = super::super::hosting::client_bounds(buttons).unwrap();
            let mut lease = super::super::reservation_windows::Lease::new(Default::default());
            for edge in [
                super::super::position::Edge::Left,
                super::super::position::Edge::Right,
            ] {
                let request = super::super::reservation::Request { edge, ..request };
                for _ in 0..20 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let placed = lease.apply(request).unwrap();
                    let inner = super::super::hosting::client_bounds(buttons).unwrap();
                    assert_eq!(super::super::hosting::client_bounds(container), Some(outer));
                    assert!(placed.fits_in(outer));
                    assert!(placed.right <= inner.left || placed.left >= inner.right);
                    assert_eq!(
                        inner.width(),
                        before.width() - request.width - 2 * request.gap
                    );
                }
            }
            drop(lease);
            assert_eq!(super::super::hosting::client_bounds(buttons), Some(before));
            assert_eq!(super::super::hosting::client_bounds(container), Some(outer));
        }
    }
}
