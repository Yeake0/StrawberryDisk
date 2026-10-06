use super::{unavailable, ResourceVolume, VolumeCapacity};
use crate::PlatformResult;
use std::{
    ffi::{CStr, CString},
    mem::size_of,
    ptr,
};

fn identity(path: &CStr) -> Option<String> {
    // getattrlist packs the UUID after a four-byte length. An aligned byte
    // buffer avoids assuming the compiler's padding matches the kernel ABI.
    let mut attributes: libc::attrlist = unsafe { std::mem::zeroed() };
    attributes.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
    attributes.volattr = libc::ATTR_VOL_INFO | libc::ATTR_VOL_UUID;
    let mut bytes = [0u8; 20];
    if unsafe {
        libc::getattrlist(
            path.as_ptr(),
            (&mut attributes as *mut libc::attrlist).cast(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            0,
        )
    } != 0
    {
        return None;
    }
    if u32::from_ne_bytes(bytes[..4].try_into().ok()?) != 20
        || bytes[4..].iter().all(|byte| *byte == 0)
    {
        return None;
    }
    Some(
        bytes[4..]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

fn volume_name(path: &CStr) -> Option<String> {
    let mut attributes: libc::attrlist = unsafe { std::mem::zeroed() };
    attributes.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
    attributes.volattr = libc::ATTR_VOL_INFO | libc::ATTR_VOL_NAME;
    let mut bytes = [0u8; 1024];
    if unsafe {
        libc::getattrlist(
            path.as_ptr(),
            (&mut attributes as *mut libc::attrlist).cast(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            0,
        )
    } != 0
    {
        return None;
    }
    // The variable-length name is addressed relative to attrreference_t,
    // which follows the returned length. Validate both offsets before decoding.
    let returned = u32::from_ne_bytes(bytes[..4].try_into().ok()?) as usize;
    let offset = i32::from_ne_bytes(bytes[4..8].try_into().ok()?);
    let length = u32::from_ne_bytes(bytes[8..12].try_into().ok()?) as usize;
    let start = 4usize.checked_add_signed(offset as isize)?;
    let end = start.checked_add(length)?;
    if returned > bytes.len() || start < 12 || end > returned {
        return None;
    }
    CStr::from_bytes_with_nul(&bytes[start..end])
        .ok()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string_lossy().into_owned())
}

pub fn list() -> PlatformResult<Vec<ResourceVolume>> {
    let count = unsafe { libc::getfsstat(ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count <= 0 || count > 4096 {
        return Err(unavailable());
    }
    let mut mounts = vec![unsafe { std::mem::zeroed::<libc::statfs>() }; count as usize + 16];
    let count = unsafe {
        libc::getfsstat(
            mounts.as_mut_ptr(),
            (mounts.len() * size_of::<libc::statfs>()) as _,
            libc::MNT_NOWAIT,
        )
    };
    if count < 0 {
        return Err(unavailable());
    }
    let mounts = &mounts[..(count as usize).min(mounts.len())];
    let has_data = mounts.iter().any(|mount| {
        unsafe { CStr::from_ptr(mount.f_mntonname.as_ptr()) }.to_bytes() == b"/System/Volumes/Data"
    });
    let mut result = Vec::new();
    for mount in mounts {
        if mount.f_flags & libc::MNT_LOCAL as u32 == 0 {
            continue;
        }
        let path = unsafe { CStr::from_ptr(mount.f_mntonname.as_ptr()) };
        let mount_point = path.to_string_lossy().into_owned();
        let system = mount_point
            == if has_data {
                "/System/Volumes/Data"
            } else {
                "/"
            };
        // APFS system, preboot and recovery volumes share container capacity;
        // expose the writable system data volume once, plus user-mounted volumes.
        if !system && !mount_point.starts_with("/Volumes/") {
            continue;
        }
        if let Some(id) = identity(path) {
            let name = if system {
                // Show the user's system volume label while sampling its writable
                // Data volume; a bare mount separator is not a useful UI label.
                volume_name(c"/").unwrap_or_else(|| mount_point.clone())
            } else {
                volume_name(path).unwrap_or_else(|| {
                    mount_point
                        .rsplit('/')
                        .next()
                        .unwrap_or(&mount_point)
                        .to_string()
                })
            };
            result.push(ResourceVolume {
                id,
                name,
                system,
                mount_point,
            });
        }
    }
    result.sort_by(|left, right| right.system.cmp(&left.system).then(left.id.cmp(&right.id)));
    result.dedup_by(|left, right| left.id == right.id);
    Ok(result)
}

pub fn capacity(volume: &ResourceVolume) -> PlatformResult<VolumeCapacity> {
    let path = CString::new(volume.mount_point.as_str()).map_err(|_| unavailable())?;
    // A different removable disk can reuse the same mount point between list
    // and read. Revalidate identity instead of attributing its space to the old disk.
    if identity(&path).as_ref() != Some(&volume.id) {
        return Err(unavailable());
    }
    let mut stats: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(path.as_ptr(), &mut stats) } != 0
        || stats.f_flags & libc::MNT_LOCAL as u32 == 0
    {
        return Err(unavailable());
    }
    crate::macos::volume_capacity::read(std::path::Path::new(&volume.mount_point)).map_err(
        |error| crate::PlatformError::new(crate::PlatformErrorCode::OperationFailed, error),
    )
}
