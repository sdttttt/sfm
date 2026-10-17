//! 系统指标采样。
//!
//! 读取全部经 `sysinfo` 完成，平台相关的细节都收敛在本模块内，上层只见
//! [`Snapshot`]。

mod cpu;
mod mem;

use sysinfo::System;

/// 状态栏项与菜单中所显示数值的统一视图。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    /// 非空闲时间的占比，取值 `0.0..=1.0`。
    pub cpu: f64,
    /// 已用的物理内存，单位为字节。
    pub mem_used: u64,
    /// 已安装的物理内存总量，单位为字节。
    pub mem_total: u64,
    /// 已使用的交换空间，单位为字节。
    pub swap_used: u64,
}

/// 持有唯一的 [`System`] 并把它刷新结果汇总成一份 [`Snapshot`]。
///
/// 占用率靠两次刷新的差值算出来，因此 `System` 必须跨 tick 存活；采样又只发生在
/// 主线程，所以无需任何同步。
pub struct Sampler {
    sys: System,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        Sampler { sys: System::new() }
    }

    /// 刷新全部指标并返回一份快照。
    ///
    /// 首次调用时没有上一次的基线，`sysinfo` 会把 CPU 占用率报成 0%；这是它的
    /// 固有行为，两次刷新之间还需要 `MINIMUM_CPU_UPDATE_INTERVAL` 的间隔。
    pub fn refresh(&mut self) -> Snapshot {
        cpu::refresh(&mut self.sys);
        mem::refresh(&mut self.sys);
        Snapshot {
            cpu: cpu::usage(&self.sys),
            mem_used: mem::used(&self.sys),
            mem_total: mem::total(&self.sys),
            swap_used: mem::swap_used(&self.sys),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_reports_plausible_memory() {
        let mut sampler = Sampler::new();
        let snap = sampler.refresh();
        assert!(snap.mem_total > 0);
        assert!(snap.mem_used <= snap.mem_total);
    }

    #[test]
    fn refresh_keeps_cpu_within_range() {
        let mut sampler = Sampler::new();
        let snap = sampler.refresh();
        assert!((0.0..=1.0).contains(&snap.cpu), "{}", snap.cpu);
    }
}
