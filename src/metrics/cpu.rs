//! CPU 占用率采样，基于 `sysinfo`。

use sysinfo::System;

/// 刷新 CPU 占用率。
///
/// 占用率是两次刷新之间的差值，`sysinfo` 内部按
/// [`MINIMUM_CPU_UPDATE_INTERVAL`](sysinfo::MINIMUM_CPU_UPDATE_INTERVAL)（200ms）
/// 节流：间隔更短时沿用它上一次算出的值。
pub(crate) fn refresh(sys: &mut System) {
    sys.refresh_cpu_usage();
}

/// 全部核心的平均占用率，归一化到 `0.0..=1.0`。
pub(crate) fn usage(sys: &System) -> f64 {
    f64::from(sys.global_cpu_usage()) / 100.0
}
