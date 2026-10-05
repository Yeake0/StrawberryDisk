//! Read-only volume facts for a specific Windows path, including mounted folders.

use std::{
    ffi::OsString,
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Component, Path, PathBuf, Prefix},
    ptr,
};

use windows_sys::Win32::{
    Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW},
    System::Diagnostics::Debug::{GetThreadErrorMode, SetThreadErrorMode, SEM_FAILCRITICALERRORS},
};

// Avoid media-insertion dialogs without changing the error mode of other threads.
struct CriticalErrorMode(u32);

impl CriticalErrorMode {
    fn enter() -> io::Result<Self> {
        let previous = unsafe { GetThreadErrorMode() };
        if unsafe { SetThreadErrorMode(previous | SEM_FAILCRITICALERRORS, ptr::null_mut()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(previous))
    }
}

impl Drop for CriticalErrorMode {
    fn drop(&mut self) {
        unsafe { SetThreadErrorMode(self.0, ptr::null_mut()) };
    }
}

fn wide_absolute(path: &Path) -> io::Result<Vec<u16>> {
    // Reject relative paths rather than silently reporting the boot volume.
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "volume path must be absolute",
        ));
    }
    let supported = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_)
            | Prefix::VerbatimDisk(_)
            | Prefix::UNC(_, _)
            | Prefix::VerbatimUNC(_, _) => true,
            Prefix::Verbatim(name) => name
                .to_str()
                .is_some_and(|name| name.starts_with("Volume{")),
            _ => false,
        },
        _ => false,
    };
    if !supported {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported volume path namespace",
        ));
    }
    let mut value = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if value.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "volume path contains NUL",
        ));
    }
    if value.len() >= 32_767 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "volume path is too long",
        ));
    }
    let slash = u16::from(b'\\');
    let verbatim = [slash, slash, u16::from(b'?'), slash];
    if !value.starts_with(&verbatim) {
        value = std::path::absolute(path)?
            .as_os_str()
            .encode_wide()
            .collect();
        let mut prefixed = verbatim.to_vec();
        if value.starts_with(&[slash, slash]) {
            prefixed.extend("UNC\\".encode_utf16());
            prefixed.extend_from_slice(&value[2..]);
        } else {
            prefixed.extend_from_slice(&value);
        }
        value = prefixed;
    }
    value.push(0);
    Ok(value)
}

/// Resolves the containing volume instead of assuming that its root is a drive letter.
/// Missing trailing path elements may still resolve to an existing containing volume.
pub fn volume_root(path: &Path) -> io::Result<PathBuf> {
    let path = wide_absolute(path)?;
    // A junction can resolve to a root longer than the supplied path.
    let mut root = vec![0_u16; 32_768];
    let _error_mode = CriticalErrorMode::enter()?;
    if unsafe { GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let length = root.iter().position(|value| *value == 0).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "volume root is not terminated")
    })?;
    if length == 0 || root[length - 1] != u16::from(b'\\') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "volume root has no trailing separator",
        ));
    }
    Ok(PathBuf::from(OsString::from_wide(&root[..length])))
}

/// Reads the filesystem name reported by the volume driver; query errors remain native.
pub fn filesystem_name(volume_root: &Path) -> io::Result<String> {
    let root = wide_absolute(volume_root)?;
    if root.get(root.len() - 2) != Some(&u16::from(b'\\')) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "volume root requires a trailing separator",
        ));
    }
    let mut filesystem = [0_u16; 261];
    let _error_mode = CriticalErrorMode::enter()?;
    if unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let length = filesystem
        .iter()
        .position(|value| *value == 0)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "filesystem name is not terminated",
            )
        })?;
    if length == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "filesystem name is empty",
        ));
    }
    Ok(OsString::from_wide(&filesystem[..length])
        .to_string_lossy()
        .into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_paths_cannot_report_the_boot_volume() {
        for path in ["", "relative.bin", "C:relative.bin", "C:\\invalid\0.bin"] {
            assert_eq!(
                volume_root(Path::new(path)).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert_eq!(
            filesystem_name(Path::new("C:\\folder")).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn native_queries_preserve_the_thread_error_mode() {
        let previous = unsafe { GetThreadErrorMode() };
        let fixture = tempfile::tempdir().unwrap();
        let root = volume_root(fixture.path()).unwrap();
        assert!(!filesystem_name(&root).unwrap().is_empty());
        assert_eq!(unsafe { GetThreadErrorMode() }, previous);
        let invalid = Path::new("\\\\?\\Volume{00000000-0000-0000-0000-000000000000}\\");
        assert!(filesystem_name(invalid)
            .unwrap_err()
            .raw_os_error()
            .is_some());
        assert_eq!(unsafe { GetThreadErrorMode() }, previous);
    }

    #[test]
    #[ignore = "requires a disposable cross-volume directory junction"]
    fn mounted_folder_reports_its_target_filesystem() {
        let path = std::env::var_os("MANGODISK_VOLUME_MOUNT_FIXTURE")
            .expect("provide a disposable junction");
        let expected = std::env::var("MANGODISK_VOLUME_EXPECTED_FILESYSTEM").unwrap();
        let root = volume_root(Path::new(&path)).unwrap();
        assert_eq!(filesystem_name(&root).unwrap(), expected);
        println!(
            "MOUNT_VOLUME filesystem={expected} root={}",
            crate::diagnostics::text(&root.display())
        );
    }

    #[test]
    fn unicode_long_and_missing_paths_resolve_to_the_same_volume() {
        let fixture = tempfile::tempdir().unwrap();
        let expected = volume_root(fixture.path()).unwrap();
        let mut path = fixture.path().to_owned();
        for _ in 0..18 {
            path.push("unicode-\u{8d44}\u{6e90}-\u{76ee}\u{5f55}");
        }
        std::fs::create_dir_all(&path).unwrap();
        path.push("missing.bin");
        assert!(path.as_os_str().encode_wide().count() > 260);
        assert_eq!(volume_root(&path).unwrap(), expected);
    }
}
