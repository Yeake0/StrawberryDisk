use std::{fs, os::windows::ffi::OsStrExt, path::Path};

use windows_sys::Win32::{
    Foundation::{GetLastError, SetLastError, ERROR_SUCCESS},
    Storage::FileSystem::GetCompressedFileSizeW,
};

use crate::FileSpaceUsage;

pub(super) fn usage(path: &Path, metadata: &fs::Metadata) -> FileSpaceUsage {
    let logical_bytes = metadata.len();
    // WOF can expose ordinary attributes while storing compressed data. Neither enumeration
    // nor live metadata flags can authorize substituting logical length for physical usage.
    query_usage(path, logical_bytes)
}

fn query_usage(path: &Path, logical_bytes: u64) -> FileSpaceUsage {
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut high = 0_u32;
    // A valid allocation can have a low word of `u32::MAX`, so clear and inspect
    // the thread-local error code instead of treating that return value alone as
    // failure.
    unsafe { SetLastError(ERROR_SUCCESS) };
    let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
    if low == u32::MAX && unsafe { GetLastError() } != ERROR_SUCCESS {
        return FileSpaceUsage::logical_only(logical_bytes);
    }
    FileSpaceUsage {
        logical_bytes,
        allocated_bytes: (u64::from(high) << 32) | u64::from(low),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{COMPRESSION_FORMAT_DEFAULT, FILE_ATTRIBUTE_COMPRESSED},
        System::{
            Ioctl::{FSCTL_SET_COMPRESSION, FSCTL_SET_SPARSE},
            IO::DeviceIoControl,
        },
    };

    #[test]
    fn ordinary_file_usage_preserves_lengths_at_size_boundaries() {
        let root = tempfile::tempdir().expect("create file-space fixture");
        for bytes in [0, 1, 257, 4095, 4096, 4097, 65536] {
            let path = root.path().join(format!("ordinary-{bytes}.bin"));
            fs::write(&path, vec![0_u8; bytes]).expect("write ordinary fixture");
            let metadata = fs::symlink_metadata(&path).expect("read fixture metadata");
            assert_eq!(
                usage(&path, &metadata),
                FileSpaceUsage::logical_only(bytes as u64),
                "ordinary file allocation must retain the existing length metric"
            );
        }
    }

    #[test]
    fn compressed_file_usage_retains_native_allocation() {
        let root = tempfile::tempdir().expect("create compressed fixture");
        let path = root.path().join("compressed.bin");
        fs::write(&path, vec![0_u8; 1024 * 1024]).expect("write compressible content");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open compression fixture");
        let format = COMPRESSION_FORMAT_DEFAULT;
        let mut returned = 0;
        let compressed = unsafe {
            DeviceIoControl(
                file.as_raw_handle() as HANDLE,
                FSCTL_SET_COMPRESSION,
                (&format as *const u16).cast(),
                std::mem::size_of_val(&format) as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(
            compressed, 0,
            "NTFS compression must be available for the fixture"
        );
        let metadata = file.metadata().expect("read compressed metadata");
        assert_ne!(metadata.file_attributes() & FILE_ATTRIBUTE_COMPRESSED, 0);
        let measured = usage(&path, &metadata);
        assert_eq!(measured, query_usage(&path, metadata.len()));
        assert!(measured.allocated_bytes < measured.logical_bytes);
    }

    #[test]
    fn wof_compressed_file_retains_native_usage_with_enumerated_and_live_metadata() {
        let root = tempfile::tempdir().expect("create WOF compression fixture");
        let path = root.path().join("wof.bin");
        fs::write(&path, vec![0_u8; 1024 * 1024]).expect("write WOF fixture");
        let output = std::process::Command::new("compact.exe")
            .args(["/C", "/F", "/EXE:XPRESS4K"])
            .arg(&path)
            .output()
            .expect("apply WOF file compression");
        assert!(output.status.success(), "WOF compression must succeed");
        let entry = fs::read_dir(root.path())
            .expect("enumerate WOF fixture")
            .next()
            .expect("WOF fixture must be present")
            .expect("read WOF directory entry");
        // WOF can hide compression attributes from both directory and live metadata. Its
        // native physical usage must not be inferred from those attributes or logical length.
        for metadata in [
            entry.metadata().unwrap(),
            fs::symlink_metadata(&path).unwrap(),
        ] {
            let expected = query_usage(&path, metadata.len());
            assert!(expected.allocated_bytes < expected.logical_bytes);
            assert_eq!(
                usage(&path, &metadata),
                expected,
                "WOF allocation must survive transparent metadata attributes"
            );
        }
    }

    #[test]
    fn sparse_file_reports_less_allocated_space_than_logical_space() {
        let root =
            std::env::temp_dir().join(format!("strawberrydisk-file-space-{}", std::process::id()));
        fs::create_dir_all(&root).expect("create file-space fixture");
        let path = root.join("sparse.bin");
        let file = fs::File::create(&path).expect("create sparse fixture");
        let mut returned = 0_u32;
        let marked_sparse = unsafe {
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
        assert_ne!(marked_sparse, 0, "mark fixture as an NTFS sparse file");
        file.set_len(64 * 1024 * 1024)
            .expect("extend sparse fixture");
        let metadata = file.metadata().expect("read sparse fixture metadata");
        let usage = usage(&path, &metadata);

        assert_eq!(usage.logical_bytes, 64 * 1024 * 1024);
        assert!(usage.allocated_bytes < usage.logical_bytes);
        fs::remove_dir_all(root).expect("remove file-space fixture");
    }
}
