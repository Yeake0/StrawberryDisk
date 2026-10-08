//! Out-of-context notifications wake the existing window thread; no Explorer
//! injection or cross-thread UI calls are needed for prompt fullscreen checks.
use std::{cell::Cell, ptr};
use windows_sys::Win32::{
    Foundation::*,
    UI::{Accessibility::*, WindowsAndMessaging::*},
};

pub const WAKE: u32 = WM_APP + 72;

thread_local! {
    static TARGET: Cell<HWND> = const { Cell::new(ptr::null_mut()) };
    static PENDING: Cell<bool> = const { Cell::new(false) };
}

pub struct Subscription([HWINEVENTHOOK; 1]);

impl Subscription {
    pub unsafe fn install(hwnd: HWND) -> Self {
        TARGET.set(hwnd);
        PENDING.set(false);
        let hooks = [EVENT_SYSTEM_FOREGROUND].map(|event| {
            let hook = SetWinEventHook(
                event,
                event,
                ptr::null_mut(),
                Some(notify),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNTHREAD,
            );
            if hook.is_null() {
                log::warn!("resident_taskbar_event_subscription_failed event={event:#x} code={} fallback=timer", GetLastError());
            }
            hook
        });
        Self(hooks)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        // Clear the target before removing hooks so queued callbacks cannot wake
        // a destroyed window or a reused handle after Explorer restarts.
        TARGET.set(ptr::null_mut());
        PENDING.set(false);
        for hook in self.0 {
            if !hook.is_null() {
                unsafe { UnhookWinEvent(hook) };
            }
        }
    }
}

pub fn dispatched() {
    PENDING.set(false);
}

unsafe extern "system" fn notify(
    _: HWINEVENTHOOK,
    event: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    time: u32,
) {
    let target = TARGET.get();
    if target.is_null() {
        return;
    }
    if !PENDING.replace(true) && PostMessageW(target, WAKE, time as _, event as _) == 0 {
        PENDING.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::core::w;

    #[test]
    fn notifications_coalesce_and_unsubscription_clears_the_target() {
        unsafe {
            // A message-only window exercises the actual queue without showing
            // UI, changing foreground, or depending on an interactive desktop.
            let hwnd = CreateWindowExW(
                0,
                w!("STATIC"),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            );
            assert!(!hwnd.is_null());
            let subscription = Subscription::install(hwnd);
            notify(
                ptr::null_mut(),
                EVENT_SYSTEM_FOREGROUND,
                hwnd,
                OBJID_CLIENT,
                0,
                0,
                2,
            );
            notify(
                ptr::null_mut(),
                EVENT_SYSTEM_FOREGROUND,
                hwnd,
                OBJID_WINDOW,
                0,
                0,
                3,
            );
            let mut message = MSG::default();
            assert_ne!(PeekMessageW(&mut message, hwnd, WAKE, WAKE, PM_REMOVE), 0);
            assert_eq!(
                message.wParam, 2,
                "the first notification time must survive coalescing"
            );
            assert_eq!(PeekMessageW(&mut message, hwnd, WAKE, WAKE, PM_REMOVE), 0);
            dispatched();
            notify(
                ptr::null_mut(),
                EVENT_SYSTEM_FOREGROUND,
                hwnd,
                OBJID_WINDOW,
                0,
                0,
                4,
            );
            assert!(PENDING.get(), "dispatch must allow the next notification");
            drop(subscription);
            assert!(TARGET.get().is_null());
            assert!(!PENDING.get());
            notify(
                ptr::null_mut(),
                EVENT_SYSTEM_FOREGROUND,
                hwnd,
                OBJID_WINDOW,
                0,
                0,
                5,
            );
            assert!(
                !PENDING.get(),
                "late callbacks must not target a destroyed subscription"
            );
            DestroyWindow(hwnd);
        }
    }
}
