//! Reclaim only memory explicitly marked volatile by its owner. Unlike `purge`,
//! this public Mach operation needs no administrator helper and does not flush
//! every file cache or manufacture pressure by allocating large temporary buffers.
use crate::{PlatformError, PlatformErrorCode, PlatformResult};
use std::time::Instant;

// Public SDK declarations from mach/mach_vm.h and mach/vm_purgable.h. libc does
// not expose this operation. Keep the ABI and constants at this platform boundary.
const VM_PURGABLE_PURGE_ALL: libc::c_int = 2;
unsafe extern "C" {
    static mach_task_self_: libc::mach_port_t;
    fn mach_vm_purgable_control(
        target: libc::mach_port_t,
        address: u64,
        control: libc::c_int,
        state: *mut libc::c_int,
    ) -> libc::kern_return_t;
}

pub(super) fn release() -> PlatformResult<()> {
    let started = Instant::now();
    let mut state = 0;
    // PURGE_ALL ignores the address. The kernel only discards objects whose
    // owners opted into volatility; ordinary application heaps remain intact.
    // Use our own task port, never obtain another application's task port.
    let code =
        unsafe { mach_vm_purgable_control(mach_task_self_, 0, VM_PURGABLE_PURGE_ALL, &mut state) };
    log::info!(
        "memory_native_reclaim method=volatile_objects code={code} elapsed_ms={}",
        started.elapsed().as_millis()
    );
    result(code)
}

fn result(code: libc::kern_return_t) -> PlatformResult<()> {
    match code {
        libc::KERN_SUCCESS => Ok(()),
        libc::KERN_NOT_SUPPORTED | libc::KERN_INVALID_ARGUMENT => Err(PlatformError::new(
            PlatformErrorCode::Unsupported,
            "volatile memory reclamation is unavailable",
        )),
        _ => Err(PlatformError::operation_failed(
            "volatile memory reclamation did not complete",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn mach_vm_allocate(
            target: libc::mach_port_t,
            address: *mut u64,
            size: u64,
            flags: i32,
        ) -> libc::kern_return_t;
        fn mach_vm_deallocate(
            target: libc::mach_port_t,
            address: u64,
            size: u64,
        ) -> libc::kern_return_t;
    }

    #[test]
    fn native_errors_never_report_success_or_trigger_elevation() {
        assert!(result(libc::KERN_SUCCESS).is_ok());
        assert_eq!(
            result(libc::KERN_NOT_SUPPORTED).unwrap_err().code(),
            PlatformErrorCode::Unsupported
        );
        assert_eq!(
            result(libc::KERN_PROTECTION_FAILURE).unwrap_err().code(),
            PlatformErrorCode::OperationFailed
        );
    }

    #[test]
    fn native_reclamation_discards_volatile_pages_but_preserves_live_memory() {
        const VM_FLAGS_ANYWHERE: i32 = 1;
        const VM_FLAGS_PURGABLE: i32 = 2;
        const VM_PURGABLE_SET_STATE: i32 = 0;
        const VM_PURGABLE_NONVOLATILE: i32 = 0;
        const VM_PURGABLE_VOLATILE: i32 = 1;
        const VM_PURGABLE_EMPTY: i32 = 2;
        let live = vec![0x5au8; 1024 * 1024];
        let mut address = 0;
        let size = 16 * 1024 * 1024;
        unsafe {
            assert_eq!(
                mach_vm_allocate(
                    mach_task_self_,
                    &mut address,
                    size,
                    VM_FLAGS_ANYWHERE | VM_FLAGS_PURGABLE,
                ),
                libc::KERN_SUCCESS
            );
        }
        struct Allocation(u64, u64);
        impl Drop for Allocation {
            fn drop(&mut self) {
                unsafe { mach_vm_deallocate(mach_task_self_, self.0, self.1) };
            }
        }
        let _allocation = Allocation(address, size);
        unsafe { std::ptr::write_bytes(address as *mut u8, 0x42, size as usize) };
        let mut state = VM_PURGABLE_VOLATILE;
        assert_eq!(
            unsafe {
                mach_vm_purgable_control(
                    mach_task_self_,
                    address,
                    VM_PURGABLE_SET_STATE,
                    &mut state,
                )
            },
            libc::KERN_SUCCESS
        );
        release().expect("the public native operation must work without authorization");
        state = VM_PURGABLE_NONVOLATILE;
        assert_eq!(
            unsafe {
                mach_vm_purgable_control(
                    mach_task_self_,
                    address,
                    VM_PURGABLE_SET_STATE,
                    &mut state,
                )
            },
            libc::KERN_SUCCESS
        );
        assert_eq!(state, VM_PURGABLE_EMPTY);
        assert!(live.iter().all(|byte| *byte == 0x5a));
    }
}
