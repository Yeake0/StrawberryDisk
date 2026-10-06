use std::{fs, io, mem::size_of, os::windows::io::AsRawHandle};

use windows_sys::Win32::{
    Foundation::{
        GetLastError, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_MORE_DATA,
        ERROR_NOT_SUPPORTED, HANDLE,
    },
    System::{
        Ioctl::{FILE_ALLOCATED_RANGE_BUFFER, FSCTL_QUERY_ALLOCATED_RANGES},
        IO::DeviceIoControl,
    },
};

use crate::{PlatformError, PlatformResult};

/// Queries at most the first NTFS allocated range; one range is enough to reject the fast path.
pub(super) fn has_allocated_content(
    file: &fs::File,
    logical_bytes: u64,
) -> PlatformResult<Option<bool>> {
    if logical_bytes == 0 {
        return Ok(Some(false));
    }
    let Ok(length) = i64::try_from(logical_bytes) else {
        return Ok(None);
    };
    let input = FILE_ALLOCATED_RANGE_BUFFER {
        FileOffset: 0,
        Length: length,
    };
    let mut output = FILE_ALLOCATED_RANGE_BUFFER {
        FileOffset: 0,
        Length: 0,
    };
    let mut returned = 0_u32;
    let succeeded = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as HANDLE,
            FSCTL_QUERY_ALLOCATED_RANGES,
            (&input as *const FILE_ALLOCATED_RANGE_BUFFER).cast(),
            u32::try_from(size_of::<FILE_ALLOCATED_RANGE_BUFFER>()).unwrap_or(u32::MAX),
            (&mut output as *mut FILE_ALLOCATED_RANGE_BUFFER).cast(),
            u32::try_from(size_of::<FILE_ALLOCATED_RANGE_BUFFER>()).unwrap_or(u32::MAX),
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if succeeded != 0 {
        return Ok(Some(
            returned >= u32::try_from(size_of::<FILE_ALLOCATED_RANGE_BUFFER>()).unwrap_or(u32::MAX),
        ));
    }
    let error_code = unsafe { GetLastError() };
    match error_code {
        ERROR_MORE_DATA => Ok(Some(true)),
        ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED => Ok(None),
        _ => Err(PlatformError::io(
            "query allocated file ranges",
            &io::Error::from_raw_os_error(error_code as i32),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;

    fn mark_sparse(file: &fs::File) {
        let mut returned = 0_u32;
        let succeeded = unsafe {
            DeviceIoControl(
                file.as_raw_handle() as HANDLE,
                FSCTL_SET_SPARSE,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(succeeded, 0, "mark fixture as sparse");
    }

    #[test]
    fn distinguishes_fully_sparse_and_allocated_files() {
        use std::io::{Seek, SeekFrom, Write};

        let root = std::env::temp_dir().join(format!(
            "strawberrydisk-allocated-content-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create allocated-content fixture");

        let sparse_path = root.join("sparse.bin");
        let sparse = fs::File::create(&sparse_path).expect("create sparse file");
        mark_sparse(&sparse);
        sparse
            .set_len(64 * 1024 * 1024)
            .expect("extend sparse file");
        drop(sparse);
        let sparse = fs::File::open(&sparse_path).expect("reopen sparse file for reading");
        assert_eq!(
            has_allocated_content(&sparse, 64 * 1024 * 1024).expect("query sparse ranges"),
            Some(false)
        );

        let allocated_path = root.join("allocated.bin");
        let mut allocated = fs::File::create(&allocated_path).expect("create allocated file");
        mark_sparse(&allocated);
        allocated
            .seek(SeekFrom::Start(32 * 1024 * 1024))
            .expect("seek allocated file");
        allocated.write_all(&[1]).expect("allocate one data extent");
        allocated
            .set_len(64 * 1024 * 1024)
            .expect("extend allocated file");
        drop(allocated);
        let allocated = fs::File::open(&allocated_path).expect("reopen allocated file for reading");
        assert_eq!(
            has_allocated_content(&allocated, 64 * 1024 * 1024).expect("query allocated ranges"),
            Some(true)
        );

        fs::remove_dir_all(root).expect("remove allocated-content fixture");
    }
}
