//! Only the companion process changes Explorer's task-button rectangle. A lease
//! remembers both the original and last applied layout so restoration never
//! overwrites a later layout established by Explorer or another application.
use super::{
    hosting,
    layout::Bounds,
    reservation::{Failure, Request, Stage},
    reservation_layout,
};
use std::{fs::OpenOptions, io::Write, path::PathBuf, ptr};
use windows_sys::{
    core::w,
    Win32::{
        Foundation::*, Graphics::Gdi::ScreenToClient, System::Threading::*,
        UI::WindowsAndMessaging::*,
    },
};

/// A rapid disable/re-enable can briefly leave two companions alive. Serialize
/// their complete lease lifetimes, including restoration, so the retiring helper
/// cannot undo the new helper's allocation. The Local namespace isolates sessions.
pub struct Ownership(HANDLE);
impl Ownership {
    pub fn acquire() -> Option<Self> {
        unsafe {
            let handle = CreateMutexW(ptr::null(), 0, w!("Local\\StrawberryDisk.TaskbarLayout.v1"));
            if handle.is_null() {
                return None;
            }
            // Only this companion thread waits; the GUI stays asynchronous. A
            // crashed owner releases the kernel mutex automatically.
            if !matches!(
                WaitForSingleObject(handle, INFINITE),
                WAIT_OBJECT_0 | WAIT_ABANDONED
            ) {
                CloseHandle(handle);
                return None;
            }
            Some(Self(handle))
        }
    }
}
impl Drop for Ownership {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

#[derive(Clone, Copy)]
struct Record {
    shell: HWND,
    parent: HWND,
    buttons: HWND,
    shell_pid: u32,
    original: Bounds,
    applied: Bounds,
}
pub struct Lease {
    record: Option<Record>,
    log_file: PathBuf,
}
impl Lease {
    pub fn new(log_file: PathBuf) -> Self {
        Self {
            record: None,
            log_file,
        }
    }

    pub unsafe fn apply(&mut self, request: Request) -> Result<Bounds, Failure> {
        let shell = FindWindowW(w!("Shell_TrayWnd"), ptr::null());
        let rebar = hosting::parent(shell);
        if shell.is_null()
            || shell as usize != request.shell
            || rebar.is_null()
            || rebar as usize != request.parent
            || super::position::read_environment() != super::position::Environment::Windows10
        {
            return Err(Failure::new(Stage::Host, 0));
        }
        let host =
            hosting::client_bounds(rebar).ok_or(Failure::new(Stage::Host, GetLastError()))?;
        if host != request.host {
            return Err(Failure::new(Stage::Geometry, 0));
        }
        // Reserve the inner list, never the outer container TrafficMonitor owns.
        // Its width watcher would treat our outer contraction as a fresh baseline
        // and contract it again. Inner list changes do not trigger that feedback.
        let peers = super::peers::inspect(rebar);
        if peers.blocks_reservation {
            self.restore();
            return Err(Failure::new(Stage::SharedHost, 0));
        }
        let Some((parent, buttons)) = hosting::task_list(rebar) else {
            self.restore();
            return Err(Failure::new(Stage::SharedHost, 0));
        };
        let host = hosting::client_bounds(parent)
            .filter(|bounds| bounds.fits_in(request.host))
            .ok_or(Failure::new(Stage::Geometry, 0))?;
        let mut shell_pid = 0;
        GetWindowThreadProcessId(shell, &mut shell_pid);
        let mut buttons_pid = 0;
        GetWindowThreadProcessId(buttons, &mut buttons_pid);
        if buttons_pid != shell_pid {
            return Err(Failure::new(Stage::Host, 0));
        }
        let current =
            relative_bounds(buttons, parent).ok_or(Failure::new(Stage::Host, GetLastError()))?;
        if self
            .record
            .is_some_and(|r| r.parent != parent || r.buttons != buttons || r.shell_pid != shell_pid)
        {
            self.restore();
        }
        let base = self.record.map_or(current, |r| {
            reservation_layout::original(current, r.original, r.applied)
        });
        let client = Bounds {
            left: 0,
            top: 0,
            right: host.width(),
            bottom: host.height(),
        };
        if base.width() <= 0 || base.height() <= 0 || !base.fits_in(client) {
            return Err(Failure::new(Stage::Geometry, 0));
        }
        // Bounds originate in the private GUI snapshot, but validate them again
        // before native arithmetic. This mode never accepts an arbitrary window.
        if !(1..=32768).contains(&request.width)
            || !(1..=32768).contains(&request.height)
            || !(1..=128).contains(&request.gap)
        {
            return Err(Failure::new(Stage::Protocol, 0));
        }
        let Some(plan) = reservation_layout::arrange(
            base,
            request.width,
            request.height,
            request.gap,
            request.edge,
        ) else {
            // An unsupported size must not keep an earlier, now empty slot.
            // Ordinary button crowding cannot reach this branch: it only depends
            // on the host's physical dimensions and the requested monitor size.
            self.restore();
            return Err(Failure::new(Stage::Space, 0));
        };
        let monitor = Bounds {
            left: host.left + plan.monitor.left,
            top: host.top + plan.monitor.top,
            right: host.left + plan.monitor.right,
            bottom: host.top + plan.monitor.bottom,
        };
        if peers.occupied.iter().any(|peer| {
            monitor.left < peer.right
                && monitor.right > peer.left
                && monitor.top < peer.bottom
                && monitor.bottom > peer.top
        }) {
            self.restore();
            return Err(Failure::new(Stage::Space, 0));
        }
        if current != plan.buttons {
            if MoveWindow(
                buttons,
                plan.buttons.left,
                plan.buttons.top,
                plan.buttons.width(),
                plan.buttons.height(),
                1,
            ) == 0
            {
                return Err(Failure::new(Stage::Position, GetLastError()));
            }
            self.record_event(&format!("resident_taskbar_space_reserved shell={shell:?} parent={parent:?} buttons={buttons:?} edge={:?} original={base:?} applied={:?} monitor={:?}", request.edge, plan.buttons, plan.monitor));
        }
        self.record = Some(Record {
            shell,
            parent,
            buttons,
            shell_pid,
            original: base,
            applied: plan.buttons,
        });
        if relative_bounds(buttons, parent) != Some(plan.buttons) {
            return Err(Failure::new(Stage::Geometry, 0));
        }
        Ok(monitor)
    }

    unsafe fn restore(&mut self) {
        let Some(record) = self.record.take() else {
            return;
        };
        let mut shell_pid = 0;
        GetWindowThreadProcessId(record.shell, &mut shell_pid);
        if FindWindowW(w!("Shell_TrayWnd"), ptr::null()) != record.shell
            || shell_pid != record.shell_pid
            || GetParent(record.buttons) != record.parent
            || IsChild(record.shell, record.buttons) == 0
        {
            self.record_event("resident_taskbar_space_released reason=shell_replaced");
            return;
        }
        let current = relative_bounds(record.buttons, record.parent);
        let original = current
            .map(|bounds| reservation_layout::original(bounds, record.original, record.applied));
        if current == original || original.is_none() {
            self.record_event(&format!("resident_taskbar_space_released reason=layout_replaced current={current:?} applied={:?}", record.applied));
            return;
        }
        let original = original.expect("checked available layout");
        let ok = MoveWindow(
            record.buttons,
            original.left,
            original.top,
            original.width(),
            original.height(),
            1,
        ) != 0;
        let code = if ok { 0 } else { GetLastError() };
        self.record_event(&format!("resident_taskbar_space_restored success={ok} code={code} buttons={:?} original={original:?}", record.buttons));
    }

    fn record_event(&self, event: &str) {
        // Open for each transition so the GUI's log rotation cannot leave this
        // process writing to an old archive. The receipt survives GUI crashes;
        // only fixed event names, native handles and rectangles are recorded.
        unsafe {
            let mut now = SYSTEMTIME::default();
            windows_sys::Win32::System::SystemInformation::GetSystemTime(&mut now);
            if let Ok(mut file) = OpenOptions::new().append(true).open(&self.log_file) {
                let _ = writeln!(
                    file,
                    "[{:04}-{:02}-{:02}][{:02}:{:02}:{:02}][taskbar_layout_helper][INFO] {event}",
                    now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
                );
            }
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        unsafe {
            self.restore();
        }
    }
}

unsafe fn relative_bounds(window: HWND, parent: HWND) -> Option<Bounds> {
    let mut rect = RECT::default();
    if GetWindowRect(window, &mut rect) == 0 {
        return None;
    }
    let mut origin = POINT {
        x: rect.left,
        y: rect.top,
    };
    if ScreenToClient(parent, &mut origin) == 0 {
        return None;
    }
    Some(Bounds {
        left: origin.x,
        top: origin.y,
        right: origin.x + rect.right - rect.left,
        bottom: origin.y + rect.bottom - rect.top,
    })
}
