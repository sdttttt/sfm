//! 系统指标采样。
//!
//! 读取全部经 `sysinfo` 完成，平台相关的细节都收敛在本模块内，上层只见
//! [`Snapshot`]。

mod cpu;
mod mem;
mod net;

use std::collections::HashSet;
use std::time::Instant;

use sysinfo::{Networks, System};

pub(crate) use net::is_default_enabled;

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
    /// 「端口范围」勾选接口的下行速率，单位为字节/秒。首次刷新时为 0。
    pub net_rx: u64,
    /// 「端口范围」勾选接口的上行速率，单位为字节/秒。首次刷新时为 0。
    pub net_tx: u64,
}

/// 持有唯一的 [`System`] 与 [`Networks`] 并把刷新结果汇总成一份 [`Snapshot`]。
///
/// 占用率与网速都靠两次刷新的差值算出来，因此两个句柄必须跨 tick 存活；采样
/// 又只发生在主线程，所以无需任何同步。
pub struct Sampler {
    sys: System,
    networks: Networks,
    /// 上次网络采样的时刻。sysinfo 只给字节差，速率要靠真实间隔换算 —— 而刷新
    /// 间隔是用户可改的（1/2/5 秒），还有「立即刷新」这类不规则触发。
    net_window: Option<Instant>,
    /// 「端口范围」勾选的接口集合。这是采样层的配置：`refresh` 直接按它过滤，
    /// App 只负责通过 [`Sampler::set_net_ifaces`] 改动。
    net_ifaces: HashSet<String>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        Sampler {
            sys: System::new(),
            networks: Networks::new(),
            net_window: None,
            net_ifaces: HashSet::new(),
        }
    }

    /// 刷新全部指标并返回一份快照。
    ///
    /// 首次调用时没有上一次的基线，`sysinfo` 会把 CPU 占用率与网速都报成 0；
    /// 这是它的固有行为，两次刷新之间 CPU 还需要 `MINIMUM_CPU_UPDATE_INTERVAL`
    /// 的间隔。
    pub fn refresh(&mut self) -> Snapshot {
        cpu::refresh(&mut self.sys);
        mem::refresh(&mut self.sys);
        net::refresh(&mut self.networks);
        let (net_rx, net_tx) = self.net_rates();
        Snapshot {
            cpu: cpu::usage(&self.sys),
            mem_used: mem::used(&self.sys),
            mem_total: mem::total(&self.sys),
            swap_used: mem::swap_used(&self.sys),
            net_rx,
            net_tx,
        }
    }

    /// 当前「端口范围」的勾选集合。
    pub fn net_ifaces(&self) -> &HashSet<String> {
        &self.net_ifaces
    }

    /// 更新「端口范围」勾选集合，立即影响下一次刷新的统计范围。
    pub fn set_net_ifaces(&mut self, ifaces: HashSet<String>) {
        self.net_ifaces = ifaces;
    }

    /// 刷新一次接口列表并返回排序后的全部接口名，供「端口范围」子菜单展示。
    /// 这次刷新同时把各接口的基线归零，随后的首个快照仍然是 0 速率。
    pub fn net_interface_names(&mut self) -> Vec<String> {
        net::interface_names(&mut self.networks)
    }

    /// 把勾选接口的字节差换算成字节/秒。
    ///
    /// 首帧没有上一次的采样时刻，报 0 —— 这与 sysinfo 首次见到新接口时基线
    /// 就地归零的行为互相印证，双重保证首帧不会飙升。
    fn net_rates(&mut self) -> (u64, u64) {
        let Some(started) = self.net_window.replace(Instant::now()) else {
            return (0, 0);
        };
        // 下限防止两次刷新贴得过近（如连点「立即刷新」）时除零、读数爆炸。
        let secs = started.elapsed().as_secs_f64().max(0.05);
        let rate = |delta: u64| (delta as f64 / secs).round() as u64;
        (
            rate(net::received(&self.networks, &self.net_ifaces)),
            rate(net::transmitted(&self.networks, &self.net_ifaces)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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

    #[test]
    fn refresh_starts_network_at_zero() {
        let mut sampler = Sampler::new();
        let snap = sampler.refresh();
        assert_eq!(snap.net_rx, 0);
        assert_eq!(snap.net_tx, 0);
    }

    #[test]
    fn refresh_keeps_network_rates_bounded() {
        let mut sampler = Sampler::new();
        sampler.refresh();
        std::thread::sleep(Duration::from_millis(300));
        let snap = sampler.refresh();
        // 宽裕上界：消费级机器没有能跑出 20 GiB/s 的网卡，越界只可能是换算 bug。
        let cap = 20 * 1024 * 1024 * 1024;
        assert!(snap.net_rx <= cap, "{}", snap.net_rx);
        assert!(snap.net_tx <= cap, "{}", snap.net_tx);
    }

    #[test]
    fn empty_selection_reports_zero_network() {
        let mut sampler = Sampler::new();
        sampler.refresh();
        sampler.set_net_ifaces(HashSet::new());
        let snap = sampler.refresh();
        assert_eq!(snap.net_rx, 0);
        assert_eq!(snap.net_tx, 0);
    }
}
