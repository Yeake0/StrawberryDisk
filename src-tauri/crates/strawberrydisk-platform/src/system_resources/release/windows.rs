use crate::{PlatformError, PlatformResult};
use std::{
    mem::{size_of, size_of_val},
    ptr::null_mut,
    time::{Duration, Instant},
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HANDLE},
    Security::{EqualSid, GetTokenInformation, TokenSessionId, TokenUser, TOKEN_QUERY, TOKEN_USER},
    System::{
        ProcessStatus::EmptyWorkingSet,
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
        },
    },
    UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

// TOKEN_USER contains a pointer into this aligned allocation. Keep the allocation
// alive while comparing SIDs; reading the opened handle also closes the PID-reuse race.
struct Owner {
    user: Vec<usize>,
    session: u32,
}
impl Owner {
    fn read(process: HANDLE) -> Option<Self> {
        unsafe {
            let mut token = null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return None;
            }
            let token = Handle(token);
            let mut required = 0;
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut required);
            if !(size_of::<TOKEN_USER>() as u32..=65536).contains(&required) {
                return None;
            }
            let mut user = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
            if GetTokenInformation(
                token.0,
                TokenUser,
                user.as_mut_ptr().cast(),
                required,
                &mut required,
            ) == 0
            {
                return None;
            }
            let mut session = 0u32;
            if GetTokenInformation(
                token.0,
                TokenSessionId,
                (&mut session as *mut u32).cast(),
                size_of_val(&session) as u32,
                &mut required,
            ) == 0
            {
                return None;
            }
            Some(Self { user, session })
        }
    }
    fn matches(&self, other: &Self) -> bool {
        unsafe {
            self.session == other.session
                && EqualSid(
                    (*(self.user.as_ptr().cast::<TOKEN_USER>())).User.Sid,
                    (*(other.user.as_ptr().cast::<TOKEN_USER>())).User.Sid,
                ) != 0
        }
    }
}

fn image_path(process: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

fn normalized_path(path: &str) -> String {
    path.strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('/', r"\")
        .to_lowercase()
}

fn foreground_path() -> Option<String> {
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
    }
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    image_path(Handle(handle).0).map(|path| normalized_path(&path))
}

pub(super) fn release(options: &super::ReleaseOptions) -> PlatformResult<()> {
    let mut excluded: std::collections::HashSet<String> = options
        .excluded_paths
        .iter()
        .map(|p| normalized_path(p))
        .collect();
    if options.skip_foreground {
        let path = foreground_path().ok_or_else(|| {
            PlatformError::operation_failed(
                "foreground application identity unavailable; automatic release skipped",
            )
        })?;
        log::info!(
            "memory_release_foreground_excluded path={}",
            crate::diagnostics::text(&path)
        );
        excluded.insert(path);
    }
    let owner = Owner::read(unsafe { GetCurrentProcess() }).ok_or_else(|| {
        PlatformError::operation_failed("memory reclamation identity is unavailable")
    })?;
    let started = Instant::now();
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let total = system.processes().len();
    let mut completed = 0u32;
    let mut excluded_count = 0u32;
    let mut excluded_images = std::collections::BTreeMap::<String, u32>::new();
    let mut identity_unavailable = 0u32;
    let mut open_failed = 0u32;
    let mut owner_unavailable = 0u32;
    let mut owner_mismatch = 0u32;
    let mut trim_failed = 0u32;
    let mut last_error = 0u32;
    let mut timed_out = false;
    for pid in system.processes().keys() {
        if started.elapsed() > Duration::from_secs(10) {
            timed_out = true;
            break;
        }
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA,
                0,
                pid.as_u32(),
            )
        };
        if handle.is_null() {
            open_failed += 1;
            last_error = unsafe { GetLastError() };
            continue;
        }
        let process = Handle(handle);
        // Never elevate, alter quotas, or terminate processes. Other users and sessions
        // are excluded even when StrawberryDisk itself happens to run with administrator rights.
        let Some(other) = Owner::read(process.0) else {
            owner_unavailable += 1;
            continue;
        };
        if !owner.matches(&other) {
            owner_mismatch += 1;
            continue;
        }
        // Query the opened handle instead of trusting a potentially stale PID snapshot.
        // Exclude every process using the same image; PID changes do not invalidate a rule.
        let path = image_path(process.0);
        if !excluded.is_empty() && path.is_none() {
            identity_unavailable += 1;
            log::debug!(
                "memory_release_skipped pid={} reason=image_unavailable code={}",
                pid.as_u32(),
                unsafe { GetLastError() }
            );
            continue;
        }
        if path
            .as_ref()
            .is_some_and(|p| excluded.contains(&normalized_path(p)))
        {
            excluded_count += 1;
            *excluded_images.entry(path.unwrap_or_default()).or_default() += 1;
            continue;
        }
        if unsafe { EmptyWorkingSet(process.0) } != 0 {
            completed += 1;
        } else {
            trim_failed += 1;
            last_error = unsafe { GetLastError() };
            if trim_failed <= 8 {
                log::warn!(
                    "memory_release_process_failed stage=trim pid={} path={} code={last_error}",
                    pid.as_u32(),
                    crate::diagnostics::text(&path.as_deref().unwrap_or_default())
                );
            }
        }
    }
    for (path, count) in excluded_images {
        log::info!(
            "memory_release_excluded path={} process_count={count}",
            crate::diagnostics::text(&path)
        );
    }
    log::info!(
        "memory_native_reclaim method=working_sets total={total} completed={completed} excluded={excluded_count} identity_unavailable={identity_unavailable} open_failed={open_failed} owner_unavailable={owner_unavailable} owner_mismatch={owner_mismatch} trim_failed={trim_failed} timed_out={timed_out} last_error={last_error} elapsed_ms={}",
        started.elapsed().as_millis()
    );
    if completed == 0 && excluded_count == 0 {
        return Err(PlatformError::operation_failed(
            "no working sets could be reclaimed",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_process_identity_matches_only_the_same_session() {
        let owner =
            Owner::read(unsafe { GetCurrentProcess() }).expect("current token must be readable");
        let mut other = Owner::read(unsafe { GetCurrentProcess() }).unwrap();
        assert!(owner.matches(&other));
        other.session = other.session.wrapping_add(1);
        assert!(!owner.matches(&other));
        assert!(Owner::read(null_mut()).is_none());
    }
    #[test]
    fn different_user_sids_are_excluded_even_in_the_same_session() {
        use windows_sys::Win32::Security::{
            CreateWellKnownSid, WinLocalServiceSid, WinLocalSystemSid, WELL_KNOWN_SID_TYPE,
        };
        fn owner(kind: WELL_KNOWN_SID_TYPE) -> Owner {
            let mut user = vec![0usize; 32];
            unsafe {
                let sid = user
                    .as_mut_ptr()
                    .cast::<u8>()
                    .add(size_of::<TOKEN_USER>())
                    .cast();
                let mut size = (size_of_val(user.as_slice()) - size_of::<TOKEN_USER>()) as u32;
                assert_ne!(CreateWellKnownSid(kind, null_mut(), sid, &mut size), 0);
                (*user.as_mut_ptr().cast::<TOKEN_USER>()).User.Sid = sid;
            }
            Owner { user, session: 1 }
        }
        assert!(owner(WinLocalSystemSid).matches(&owner(WinLocalSystemSid)));
        assert!(!owner(WinLocalSystemSid).matches(&owner(WinLocalServiceSid)));
    }

    #[test]
    fn opened_process_image_matches_exclusion_across_case_and_prefix() {
        let path = image_path(unsafe { GetCurrentProcess() }).expect("image path available");
        assert_eq!(
            normalized_path(&path),
            normalized_path(&path.to_uppercase())
        );
        assert_eq!(
            normalized_path(r"\\?\C:\Apps\Code.exe"),
            normalized_path(r"c:\apps\code.exe")
        );
        assert_ne!(
            normalized_path(r"C:\App1\Code.exe"),
            normalized_path(r"C:\App2\Code.exe")
        );
    }

    #[test]
    fn native_working_set_api_can_trim_this_test_process() {
        assert_ne!(unsafe { EmptyWorkingSet(GetCurrentProcess()) }, 0);
    }
}
