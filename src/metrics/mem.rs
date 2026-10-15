use std::mem::MaybeUninit;

// `mach_host_self` is deprecated in favour of the `mach2` crate; a third
// dependency is not worth one port lookup.
#[allow(deprecated)]
use libc::mach_host_self;
use libc::{
    host_statistics64, sysctlbyname, vm_statistics64, xsw_usage, HOST_VM_INFO64, KERN_SUCCESS,
};

fn page_size() -> u64 {
    let mut size: libc::c_uint = 0;
    let mut len = std::mem::size_of::<libc::c_uint>() as libc::size_t;
    // SAFETY: `size` is a live `c_uint` and `len` is its exact size.
    let rc = unsafe {
        sysctlbyname(
            c"hw.pagesize".as_ptr(),
            (&mut size as *mut libc::c_uint).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || size == 0 {
        4096
    } else {
        size as u64
    }
}

fn sysctl_u64(name: &[u8]) -> Option<u64> {
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>() as libc::size_t;
    // SAFETY: `value` is a live `u64` and `len` its exact size.
    let rc = unsafe {
        sysctlbyname(
            name.as_ptr().cast(),
            (&mut value as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len < std::mem::size_of::<u64>() {
        return None;
    }
    Some(value)
}

/// Physical memory in use: active + wired + compressed.
///
/// The compressed page count is essential — omitting it under-reports by
/// several gigabytes on a typical system.
#[allow(deprecated)]
pub fn memory_used() -> Option<u64> {
    // SAFETY: `HOST_VM_INFO64` fills a `vm_statistics64`; the count is passed by
    // pointer and the buffer is zeroed before use.
    let stats = unsafe {
        let mut stats = MaybeUninit::<vm_statistics64>::uninit();
        let mut count = libc::HOST_VM_INFO64_COUNT;
        let host = mach_host_self();
        if host == 0 {
            return None;
        }
        let kr = host_statistics64(host, HOST_VM_INFO64, stats.as_mut_ptr().cast(), &mut count);
        if kr != KERN_SUCCESS || count == 0 {
            return None;
        }
        stats.assume_init()
    };

    let pages =
        stats.active_count as u64 + stats.wire_count as u64 + stats.compressor_page_count as u64;
    Some(pages * page_size())
}

/// Installed physical memory, in bytes.
pub fn total_memory() -> Option<u64> {
    sysctl_u64(b"hw.memsize\0")
}

/// Swap in use, in bytes.
pub fn swap_used() -> Option<u64> {
    // Newer SDKs append fields to `xsw_usage`, so read into a larger buffer and
    // only interpret the leading `xsw_usage` layout.
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    // SAFETY: `buf` is 64 live bytes and `len` is its real size.
    let rc = unsafe {
        sysctlbyname(
            c"vm.swapusage".as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len < std::mem::size_of::<xsw_usage>() {
        return None;
    }
    // SAFETY: `xsw_usage` is plain data with no padding requirements beyond
    // alignment, and the kernel wrote `size_of::<xsw_usage>()` valid bytes.
    let usage: xsw_usage = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<xsw_usage>()) };
    Some(usage.xsu_used)
}
