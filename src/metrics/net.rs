//! 网络上下行速率采样，基于 `sysinfo`。
//!
//! sysinfo 的 `Networks` 独立于 `System`：`received()` / `transmitted()` 给出的是
//! **距上次刷新的字节差**，首次见到的接口基线就地归零（`old == current`），因此
//! 首帧不会把开机以来的累计量误报成速率；换算成字节/秒所需的两次刷新真实间隔
//! 由 [`Sampler`](super::Sampler) 记录。

use std::collections::HashSet;

use sysinfo::Networks;

/// 「端口范围」的默认勾选判定：物理网卡（`en` + 非空纯数字后缀）。
///
/// 只决定首次启动（还没有持久化勾选）时哪些接口被勾上；之后的统计范围完全
/// 以持久化的勾选集合为准。回环、VPN（utun*）、桥接等虚拟接口默认不计 ——
/// 空集合与勾选了不存在的接口都只是求不到字节差，无害。
pub(crate) fn is_default_enabled(name: &str) -> bool {
    match name.strip_prefix("en") {
        Some(rest) => !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

/// 刷新全部网卡的计数器。
pub(crate) fn refresh(networks: &mut Networks) {
    // 传 true：本次刷新没有出现的接口（如已断开的 VPN 隧道）直接从列表移除。
    networks.refresh(true);
}

/// 当前可见的全部接口名，排序后返回。
///
/// 「端口范围」子菜单与自检都要求顺序稳定，而 sysinfo 返回的是 HashMap。
pub(crate) fn interface_names(networks: &mut Networks) -> Vec<String> {
    refresh(networks);
    let mut names: Vec<String> = networks.list().keys().cloned().collect();
    names.sort();
    names
}

/// 勾选集合内接口「距上次刷新」的下行字节差之和。
pub(crate) fn received(networks: &Networks, enabled: &HashSet<String>) -> u64 {
    networks
        .list()
        .iter()
        .filter(|(name, _)| enabled.contains(*name))
        .map(|(_, data)| data.received())
        .sum()
}

/// 勾选集合内接口「距上次刷新」的上行字节差之和。
pub(crate) fn transmitted(networks: &Networks, enabled: &HashSet<String>) -> u64 {
    networks
        .list()
        .iter()
        .filter(|(name, _)| enabled.contains(*name))
        .map(|(_, data)| data.transmitted())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_selection_is_physical_only() {
        assert!(is_default_enabled("en0"));
        assert!(is_default_enabled("en5"));
        // 空后缀与带字母的伪前缀都不算物理网卡。
        assert!(!is_default_enabled("en"));
        assert!(!is_default_enabled("entropy"));
        // 常见虚拟接口全部排除。
        assert!(!is_default_enabled("lo0"));
        assert!(!is_default_enabled("utun3"));
        assert!(!is_default_enabled("bridge0"));
        assert!(!is_default_enabled("awdl0"));
    }

    #[test]
    fn empty_selection_sums_to_zero() {
        let mut networks = Networks::new();
        assert!(!interface_names(&mut networks).is_empty());
        assert_eq!(received(&networks, &HashSet::new()), 0);
        assert_eq!(transmitted(&networks, &HashSet::new()), 0);
    }
}
