//! Open only the original caller, including consent supplied by a different administrator.

use super::{process_created, OwnedHandle};
use std::{io, ptr};
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ACCESS_DENIED, ERROR_SUCCESS, HANDLE, LUID},
    Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    },
    Storage::FileSystem::SYNCHRONIZE,
    System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_DUP_HANDLE,
        PROCESS_QUERY_LIMITED_INFORMATION,
    },
};

const PARENT_ACCESS: u32 = PROCESS_DUP_HANDLE | PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE;

pub(super) fn open_verified(id: u32, created: u64) -> io::Result<OwnedHandle> {
    let handle = unsafe { OpenProcess(PARENT_ACCESS, 0, id) };
    let handle = if handle.is_null() {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_ACCESS_DENIED as i32) {
            return Err(error);
        }
        open_with_debug_privilege(id)?
    } else {
        OwnedHandle(handle)
    };
    if process_created(handle.0)? != created {
        return Err(io::Error::from_raw_os_error(ERROR_ACCESS_DENIED as i32));
    }
    Ok(handle)
}

fn open_with_debug_privilege(id: u32) -> io::Result<OwnedHandle> {
    // A different account's default process ACL does not grant DUP_HANDLE to the
    // administrator who supplied consent. Temporarily use an existing administrator
    // privilege for this single OpenProcess, then restore the exact previous token
    // state before spawning any thread or child. No machine policy or ACL is changed.
    let mut token: HANDLE = ptr::null_mut();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let token = OwnedHandle(token);
    let mut luid = LUID::default();
    let name = super::wide(std::ffi::OsStr::new("SeDebugPrivilege"));
    if unsafe { LookupPrivilegeValueW(ptr::null(), name.as_ptr(), &mut luid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let enabled = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    let mut previous = TOKEN_PRIVILEGES::default();
    let mut size = 0;
    if unsafe {
        AdjustTokenPrivileges(
            token.0,
            0,
            &enabled,
            std::mem::size_of::<TOKEN_PRIVILEGES>() as u32,
            &mut previous,
            &mut size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let code = unsafe { GetLastError() };
    if code != ERROR_SUCCESS {
        // ERROR_NOT_ALL_ASSIGNED means local policy withheld the privilege; fail
        // closed rather than changing that policy or requesting another UAC prompt.
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let handle = unsafe { OpenProcess(PARENT_ACCESS, 0, id) };
    let result = if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(OwnedHandle(handle))
    };
    if unsafe { AdjustTokenPrivileges(token.0, 0, &previous, 0, ptr::null_mut(), ptr::null_mut()) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_parent_requires_the_exact_process_creation_time() {
        let created = process_created(unsafe { GetCurrentProcess() }).unwrap();
        assert!(open_verified(std::process::id(), created).is_ok());
        assert!(open_verified(std::process::id(), created + 1).is_err());
    }
}
