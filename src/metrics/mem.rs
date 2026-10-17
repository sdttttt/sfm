//! 内存与交换空间采样，基于 `sysinfo`。

use sysinfo::System;

/// 刷新 RAM 与交换空间的用量。
pub(crate) fn refresh(sys: &mut System) {
    sys.refresh_memory();
}

/// 已用的物理内存，单位为字节。
///
/// macOS 上 `sysinfo` 的口径是 `internal − purgeable + wired + compressed`，
/// 与活动监视器的「内存已用」一致。
pub(crate) fn used(sys: &System) -> u64 {
    sys.used_memory()
}

/// 已安装的物理内存总量，单位为字节。
pub(crate) fn total(sys: &System) -> u64 {
    sys.total_memory()
}

/// 已使用的交换空间，单位为字节。
pub(crate) fn swap_used(sys: &System) -> u64 {
    sys.used_swap()
}
