use std::{fs, io, os::fd::AsRawFd};

use crate::{PlatformError, PlatformResult};

/// Queries the first physical data extent without reading file content.
///
/// APFS and other Darwin filesystems return `ENXIO` when no data extent exists at or after the
/// requested offset. Unsupported filesystems select the ordinary reader; unexpected failures are
/// preserved as typed platform errors so Core can record a bounded fallback diagnostic.
pub(super) fn has_allocated_content(
    file: &fs::File,
    logical_bytes: u64,
) -> PlatformResult<Option<bool>> {
    if logical_bytes == 0 {
        return Ok(Some(false));
    }
    let offset = unsafe { libc::lseek(file.as_raw_fd(), 0, libc::SEEK_DATA) };
    if offset >= 0 {
        return Ok(Some(true));
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENXIO) => Ok(Some(false)),
        Some(libc::EINVAL) | Some(libc::ENOTSUP) => Ok(None),
        _ => Err(PlatformError::io("query allocated file ranges", &error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    fn distinguishes_fully_sparse_and_allocated_files() {
        let root = std::env::temp_dir().join(format!(
            "strawberrydisk-allocated-content-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create allocated-content fixture");

        let sparse = fs::File::create(root.join("sparse.bin")).expect("create sparse file");
        sparse
            .set_len(64 * 1024 * 1024)
            .expect("extend sparse file");
        assert_eq!(
            has_allocated_content(&sparse, 64 * 1024 * 1024).expect("query sparse ranges"),
            Some(false)
        );

        let mut allocated =
            fs::File::create(root.join("allocated.bin")).expect("create allocated file");
        allocated
            .seek(SeekFrom::Start(32 * 1024 * 1024))
            .expect("seek allocated file");
        allocated.write_all(&[1]).expect("allocate one data extent");
        allocated
            .set_len(64 * 1024 * 1024)
            .expect("extend allocated file");
        assert_eq!(
            has_allocated_content(&allocated, 64 * 1024 * 1024).expect("query allocated ranges"),
            Some(true)
        );

        fs::remove_dir_all(root).expect("remove allocated-content fixture");
    }
}
