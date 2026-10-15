//! 系统指标采样。所有对内核的裸 FFI 调用都收敛在本模块内。

mod cpu;
mod mem;

pub use cpu::CpuSampler;
pub use mem::{memory_used, swap_used, total_memory};

/// 状态栏项与菜单中所显示数值的统一视图。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    /// 非空闲时间的占比，取值 `0.0..=1.0`。
    pub cpu: f64,
    /// 已用的物理内存（active + wired + compressed），单位为字节。
    pub mem_used: u64,
    /// 已安装的物理内存总量，单位为字节。
    pub mem_total: u64,
    /// 已使用的交换空间，单位为字节。
    pub swap_used: u64,
}
