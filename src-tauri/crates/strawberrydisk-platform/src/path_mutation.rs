//! Atomic filesystem mutations whose replacement semantics differ by OS.

use std::{io, path::Path};

/// Moves a path only if the destination is absent, without an exists/rename race.
///
/// Recovery callers must preserve both the staged remainder and a concurrently
/// recreated destination. Unsupported filesystem capabilities fail closed;
/// ordinary rename is never an acceptable fallback because it can replace data.
pub fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    rename_exclusive(source, destination)
}

#[cfg(unix)]
fn rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: both C strings are terminated and remain alive for the syscall.
    #[cfg(target_os = "macos")]
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    // SAFETY: AT_FDCWD gives ordinary path resolution; RENAME_NOREPLACE is
    // checked atomically by the kernel, including an existing empty directory.
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        // std::fs accepts long paths independently of the app manifest. Preserve
        // that contract when calling Win32 directly, without resolving symlinks.
        let absolute = std::path::absolute(path)?;
        let mut value: Vec<u16> = absolute.as_os_str().encode_wide().collect();
        if value.contains(&0) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let slash = u16::from(b'\\');
        let verbatim = [slash, slash, u16::from(b'?'), slash];
        if !value.starts_with(&verbatim) {
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
    let source = wide(source)?;
    let destination = wide(destination)?;
    // SAFETY: both buffers are NUL terminated and live through this call.
    // Omitting MOVEFILE_REPLACE_EXISTING preserves an existing destination.
    let result = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) };
    if result != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn exclusive_rename_preserves_existing_empty_directory_and_source() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("staged");
        let destination = root.path().join("recreated");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("remaining"), b"keep").unwrap();
        fs::create_dir(&destination).unwrap();
        let error = rename_no_replace(&source, &destination).unwrap_err();
        assert!(error.raw_os_error().is_some());
        assert!(destination.read_dir().unwrap().next().is_none());
        assert_eq!(fs::read(source.join("remaining")).unwrap(), b"keep");
    }

    #[test]
    fn exclusive_rename_restores_to_absent_destination() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("staged");
        let destination = root.path().join("original");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("remaining"), b"keep").unwrap();
        rename_no_replace(&source, &destination).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(destination.join("remaining")).unwrap(), b"keep");
    }

    #[test]
    fn exclusive_rename_preserves_existing_file() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("staged");
        let destination = root.path().join("recreated");
        fs::write(&source, b"remaining").unwrap();
        fs::write(&destination, b"new").unwrap();
        assert!(rename_no_replace(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"new");
        assert_eq!(fs::read(&source).unwrap(), b"remaining");
    }
    #[cfg(windows)]
    #[test]
    fn exclusive_rename_accepts_long_paths_without_a_verbatim_prefix() {
        let root = tempfile::tempdir().unwrap();
        let parent = root
            .path()
            .join("nested-directory-".repeat(12))
            .join("nested-directory-".repeat(12));
        fs::create_dir_all(&parent).unwrap();
        let source = parent.join("staged");
        let destination = parent.join("restored");
        fs::write(&source, b"remaining").unwrap();
        rename_no_replace(&source, &destination).unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"remaining");
    }

    #[cfg(unix)]
    #[test]
    fn exclusive_rename_preserves_an_existing_symlink() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("staged");
        let destination = root.path().join("recreated");
        let unrelated = root.path().join("unrelated");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&unrelated).unwrap();
        std::os::unix::fs::symlink(&unrelated, &destination).unwrap();
        assert!(rename_no_replace(&source, &destination).is_err());
        assert!(destination
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(source.is_dir());
    }
}
