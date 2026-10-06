use super::{unavailable, ResourceVolume, VolumeCapacity};
use crate::PlatformResult;
use std::ptr;
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW, GetDriveTypeW,
        GetVolumeInformationW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        GetVolumePathNamesForVolumeNameW,
    },
    System::SystemInformation::GetWindowsDirectoryW,
};

struct Enumeration(HANDLE);
impl Drop for Enumeration {
    fn drop(&mut self) {
        unsafe { FindVolumeClose(self.0) };
    }
}
fn text(value: &[u16]) -> String {
    String::from_utf16_lossy(
        &value[..value
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(value.len())],
    )
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn system_identity() -> PlatformResult<String> {
    let mut directory = [0u16; 32768];
    let mut mount = [0u16; 32768];
    let mut identity = [0u16; 128];
    unsafe {
        let count = GetWindowsDirectoryW(directory.as_mut_ptr(), directory.len() as _);
        if count == 0
            || count >= directory.len() as u32
            || GetVolumePathNameW(directory.as_ptr(), mount.as_mut_ptr(), mount.len() as _) == 0
            || GetVolumeNameForVolumeMountPointW(
                mount.as_ptr(),
                identity.as_mut_ptr(),
                identity.len() as _,
            ) == 0
        {
            return Err(unavailable());
        }
    }
    Ok(text(&identity))
}

pub fn list() -> PlatformResult<Vec<ResourceVolume>> {
    let system = system_identity()?;
    let mut identity = [0u16; 128];
    let handle = unsafe { FindFirstVolumeW(identity.as_mut_ptr(), identity.len() as _) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(unavailable());
    }
    let _owned = Enumeration(handle);
    let mut volumes = Vec::new();
    loop {
        let id = text(&identity);
        let mut paths = vec![0u16; 32768];
        let mut required = 0;
        unsafe {
            if GetVolumePathNamesForVolumeNameW(
                identity.as_ptr(),
                paths.as_mut_ptr(),
                paths.len() as _,
                &mut required,
            ) != 0
                && paths[0] != 0
            {
                let mount_point = text(&paths);
                // DRIVE_REMOVABLE and DRIVE_FIXED only; never query an SMB share.
                if matches!(GetDriveTypeW(paths.as_ptr()), 2 | 3) {
                    let mut label = [0u16; 261];
                    GetVolumeInformationW(
                        identity.as_ptr(),
                        label.as_mut_ptr(),
                        label.len() as _,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        0,
                    );
                    let label = text(&label);
                    let name = if label.is_empty() {
                        mount_point.clone()
                    } else {
                        format!("{label} ({mount_point})")
                    };
                    volumes.push(ResourceVolume {
                        system: id.eq_ignore_ascii_case(&system),
                        id,
                        name,
                        mount_point,
                    });
                }
            }
            if FindNextVolumeW(handle, identity.as_mut_ptr(), identity.len() as _) == 0 {
                if GetLastError() != ERROR_NO_MORE_FILES {
                    return Err(unavailable());
                }
                break;
            }
        }
    }
    volumes.sort_by(|left, right| right.system.cmp(&left.system).then(left.id.cmp(&right.id)));
    Ok(volumes)
}

pub fn capacity(volume: &ResourceVolume) -> PlatformResult<VolumeCapacity> {
    let path = wide(&volume.id);
    let mut available = 0;
    let mut total = 0;
    // Query the volume GUID, not a mutable drive letter. An unplugged selection
    // fails instead of silently resolving a newly attached disk at that letter.
    if unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, ptr::null_mut()) }
        == 0
    {
        return Err(unavailable());
    }
    Ok(VolumeCapacity {
        total_bytes: total,
        available_bytes: available,
    })
}
