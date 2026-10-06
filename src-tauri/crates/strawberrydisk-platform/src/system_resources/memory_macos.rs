//! Native footprint and physical-memory counters; unavailable values are never RSS substitutes.
use super::memory::ProcessMemory;
use crate::{PlatformError, PlatformErrorCode, PlatformResult};
use std::mem::MaybeUninit;

pub(super) fn overview(total: u64) -> PlatformResult<(u64, u64)> {
    static HOST: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    unsafe extern "C" {
        fn mach_host_self() -> libc::mach_port_t;
    }
    let host = *HOST.get_or_init(|| unsafe { mach_host_self() });
    let mut counters = MaybeUninit::<libc::vm_statistics64>::zeroed();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let status = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            counters.as_mut_ptr().cast(),
            &mut count,
        )
    };
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let required = (std::mem::offset_of!(libc::vm_statistics64, internal_page_count)
        + std::mem::size_of::<libc::natural_t>())
        / std::mem::size_of::<libc::integer_t>();
    if status != libc::KERN_SUCCESS || count < required as u32 || page_size <= 0 {
        return Err(PlatformError::new(PlatformErrorCode::OperationFailed,
            format!("macOS memory counters unavailable status={status} count={count} page_size={page_size}")));
    }
    let counters = unsafe { counters.assume_init() };
    Ok(physical_usage(
        total,
        page_size as u64,
        counters.free_count.into(),
        counters.speculative_count.into(),
        counters.external_page_count.into(),
    ))
}

fn physical_usage(
    total: u64,
    page_size: u64,
    free: u64,
    speculative: u64,
    file_backed: u64,
) -> (u64, u64) {
    // Inactive anonymous pages still belong to apps. Subtract native free and file-backed
    // pages from installed RAM instead of counting only active pages. Speculative pages
    // are already included in free_count; subtract them only for the free-page display.
    (
        total.saturating_sub(free.saturating_add(file_backed).saturating_mul(page_size)),
        free.saturating_sub(speculative)
            .saturating_mul(page_size)
            .min(total),
    )
}

pub(super) fn fill_application_metadata(processes: &mut [ProcessMemory]) {
    let apps = super::application_metadata_macos::read();
    for process in processes {
        if let Some(application) = apps.get(&process.pid) {
            process.is_application = true;
            if let Some(name) = &application.name {
                process.name.clone_from(name);
            }
            if process.executable.is_none() {
                process.executable.clone_from(&application.executable);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usage_includes_inactive_app_pages_and_counts_speculative_pages_once() {
        assert_eq!(physical_usage(1000, 10, 5, 2, 20), (750, 30));
        assert_eq!(physical_usage(10, u64::MAX, 5, 20, 3), (0, 0));
    }
    #[test]
    fn own_footprint_is_readable_and_invalid_pid_remains_unknown() {
        assert!(
            super::super::process_cpu_macos::read_usage(std::process::id())
                .is_some_and(|usage| usage.ri_phys_footprint > 0)
        );
        assert!(super::super::process_cpu_macos::read_usage(u32::MAX).is_none());
    }
}
