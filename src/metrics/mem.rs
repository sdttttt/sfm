use std::mem::MaybeUninit;

// 官方推荐用 `mach2` crate 替代已废弃的 `mach_host_self`；但仅为一次端口
// 查询就引入第三个依赖并不划算。
#[allow(deprecated)]
use libc::mach_host_self;
use libc::{
    host_statistics64, sysctlbyname, vm_statistics64, xsw_usage, HOST_VM_INFO64, KERN_SUCCESS,
};

fn page_size() -> u64 {
    let mut size: libc::c_uint = 0;
    let mut len = std::mem::size_of::<libc::c_uint>() as libc::size_t;
    // SAFETY: size 是一个存活的 c_uint，len 则是它的精确大小。
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
    // SAFETY: value 是一个存活的 u64，len 是它的精确大小。
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

/// 已用的物理内存：active + wired + compressed。
///
/// 压缩页数必须计入 —— 在典型机器上漏掉它会少报好几个 GB。
#[allow(deprecated)]
pub fn memory_used() -> Option<u64> {
    // SAFETY: `HOST_VM_INFO64` 会填充一个 `vm_statistics64`；长度按指针传入，
    // 缓冲区在使用前已置零。
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

/// 已安装的物理内存总量，单位为字节。
pub fn total_memory() -> Option<u64> {
    sysctl_u64(b"hw.memsize\0")
}

/// 已使用的交换空间，单位为字节。
pub fn swap_used() -> Option<u64> {
    // 较新的 SDK 会给 `xsw_usage` 追加字段，因此读入一个更大的缓冲区，
    // 并且只解释开头那段 `xsw_usage` 布局。
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    // SAFETY: buf 是 64 字节的存活缓冲区，len 是它的真实大小。
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
    // SAFETY: `xsw_usage` 是纯数据，除对齐外没有其他填充要求，且内核已写入
    // `size_of::<xsw_usage>()` 个有效字节。
    let usage: xsw_usage = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<xsw_usage>()) };
    Some(usage.xsu_used)
}
