use std::mem::MaybeUninit;
use std::time::Instant;

// 官方推荐用 `mach2` crate 替代已废弃的 `mach_host_self`；但仅为一次端口
// 查询就引入第三个依赖并不划算。
#[allow(deprecated)]
use libc::mach_host_self;
use libc::{host_statistics, HOST_CPU_LOAD_INFO};

/// 间隔小于此值的采样会被丢弃。tick 计数粒度较粗，几个毫秒的窗口就可能报出
/// 100%，那会让点「立即刷新」时菜单栏的数字乱跳。
const MIN_SAMPLE_SECS: f64 = 0.2;

/// 内核上报的 tick 计数，顺序为 `[user, system, idle, nice]`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticks([u64; 4]);

impl Ticks {
    #[cfg(test)]
    pub(crate) fn new(ticks: [u64; 4]) -> Self {
        Ticks(ticks)
    }
}

/// 由两次 tick 采样计算 CPU 负载；数据不可用时返回 `None`。
fn utilization(prev: Ticks, curr: Ticks) -> Option<f64> {
    let delta = [
        curr.0[0].checked_sub(prev.0[0])?,
        curr.0[1].checked_sub(prev.0[1])?,
        curr.0[2].checked_sub(prev.0[2])?,
        curr.0[3].checked_sub(prev.0[3])?,
    ];
    let total: u64 = delta.iter().sum();
    if total == 0 {
        return None;
    }
    let busy = delta[0] + delta[1] + delta[3];
    Some(busy as f64 / total as f64)
}

#[allow(deprecated)]
fn read_ticks() -> Option<Ticks> {
    // SAFETY: `HOST_CPU_LOAD_INFO` 恰好会填满 `HOST_CPU_LOAD_INFO_COUNT` 个
    // 整数；缓冲区按该大小分配，并传入其真实长度。
    unsafe {
        let mut info =
            MaybeUninit::<[libc::integer_t; libc::HOST_CPU_LOAD_INFO_COUNT as usize]>::uninit();
        let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
        let host = mach_host_self();
        if host == 0 {
            return None;
        }
        let kr = host_statistics(
            host,
            HOST_CPU_LOAD_INFO,
            info.as_mut_ptr().cast(),
            &mut count,
        );
        if kr != libc::KERN_SUCCESS || count < libc::HOST_CPU_LOAD_INFO_COUNT {
            return None;
        }
        let info = info.assume_init();
        Some(Ticks([
            info[0].max(0) as u64,
            info[1].max(0) as u64,
            info[2].max(0) as u64,
            info[3].max(0) as u64,
        ]))
    }
}

/// 保存上一次的 tick 基线，使每次调用都能报告「自上次调用以来」的负载。
pub struct CpuSampler {
    prev: Option<Ticks>,
    last_at: Option<Instant>,
}

impl Default for CpuSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuSampler {
    pub fn new() -> Self {
        CpuSampler {
            prev: None,
            last_at: None,
        }
    }

    /// 返回自上次调用以来测得的负载；当没有可用测量时返回 `None`
    ///（首帧、内核读取失败，或间隔长到足以说明机器曾休眠、进程曾被挂起）。
    pub fn sample(&mut self, refresh_secs: u64) -> Option<f64> {
        let now = Instant::now();
        let elapsed = self
            .last_at
            .map(|last| now.duration_since(last).as_secs_f64())
            .unwrap_or(0.0);
        self.advance(refresh_secs, read_ticks(), elapsed)
    }

    fn advance(&mut self, refresh_secs: u64, ticks: Option<Ticks>, elapsed: f64) -> Option<f64> {
        let Some(ticks) = ticks else {
            // 保留原基线：丢掉一帧不该污染下一次测量。
            return None;
        };
        let Some(prev) = self.prev else {
            // 首帧：只记住基线，等下一帧再来比较。
            self.prev = Some(ticks);
            self.last_at = Some(Instant::now());
            return None;
        };
        if elapsed < MIN_SAMPLE_SECS {
            // 间隔太短，测不出有意义的结果。保留基线，让下一个 tick 仍能测量
            // 一个完整窗口。
            return None;
        }
        if elapsed > 3.0 * refresh_secs as f64 {
            // 休眠/唤醒，或调试器长时间暂停：此时 tick 差值毫无意义，但刚读到
            // 的这一帧可以作为新基线。
            self.prev = Some(ticks);
            self.last_at = Some(Instant::now());
            return None;
        }
        self.prev = Some(ticks);
        self.last_at = Some(Instant::now());
        utilization(prev, ticks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_is_dropped() {
        let mut sampler = CpuSampler::new();
        assert_eq!(sampler.sample(1), None);
    }

    #[test]
    fn utilization_matches_hand_computation() {
        // 4000 tick 中有 1000 个是忙 => 25%。
        let prev = Ticks::new([0, 0, 0, 0]);
        let curr = Ticks::new([500, 400, 3000, 100]);
        let load = utilization(prev, curr).unwrap();
        assert!((load - 0.25).abs() < 1e-12, "{load}");
    }

    #[test]
    fn nice_time_counts_as_busy() {
        let load = utilization(Ticks::new([0, 0, 0, 0]), Ticks::new([0, 0, 900, 100])).unwrap();
        assert!((load - 0.1).abs() < 1e-12, "{load}");
    }

    #[test]
    fn sampler_computes_delta_between_frames() {
        let mut sampler = CpuSampler::new();
        assert_eq!(
            sampler.advance(1, Some(Ticks::new([0, 0, 0, 0])), 0.0),
            None
        );
        let load = sampler
            .advance(1, Some(Ticks::new([100, 100, 800, 0])), 1.0)
            .unwrap();
        assert!((load - 0.2).abs() < 1e-12, "{load}");
    }

    #[test]
    fn long_gap_resets_baseline_instead_of_spiking() {
        let mut sampler = CpuSampler::new();
        sampler.advance(1, Some(Ticks::new([0, 0, 0, 0])), 0.0);
        // 休眠数分钟后醒来：这一帧必须被丢弃。
        assert_eq!(
            sampler.advance(1, Some(Ticks::new([900, 0, 100, 0])), 600.0),
            None
        );
        // 被丢弃的那一帧成了新基线，于是下一帧又是正常的差值：2000 tick 中
        // 有 100 个是忙。
        let load = sampler
            .advance(1, Some(Ticks::new([1000, 0, 2000, 0])), 1.0)
            .unwrap();
        assert!((load - 0.05).abs() < 1e-12, "{load}");
    }

    #[test]
    fn long_gap_is_relative_to_refresh_interval() {
        let mut sampler = CpuSampler::new();
        sampler.advance(5, Some(Ticks::new([0, 0, 0, 0])), 0.0);
        // 10 秒仅为 5 秒间隔的 2 倍，因此这一帧予以保留。
        let load = sampler
            .advance(5, Some(Ticks::new([100, 0, 900, 0])), 10.0)
            .unwrap();
        assert!((load - 0.1).abs() < 1e-12, "{load}");
    }

    #[test]
    fn back_to_back_refresh_keeps_the_previous_value() {
        let mut sampler = CpuSampler::new();
        sampler.advance(1, Some(Ticks::new([0, 0, 0, 0])), 0.0);
        let load = sampler
            .advance(1, Some(Ticks::new([100, 0, 900, 0])), 1.0)
            .unwrap();
        // 几毫秒后点「立即刷新」不应报出 100%。
        assert_eq!(
            sampler.advance(1, Some(Ticks::new([200, 0, 1000, 0])), 0.01),
            None
        );
        // 而且基线未被改动，因此下一个定时器 tick 测的是从
        // [100, 0, 900] 到 [300, 0, 1900] 的完整窗口：1200 tick 中有 200 个是忙。
        let load2 = sampler
            .advance(1, Some(Ticks::new([300, 0, 1900, 0])), 1.0)
            .unwrap();
        assert!((load2 - 200.0 / 1200.0).abs() < 1e-12, "{load2}");
        assert!((load - 0.1).abs() < 1e-12, "{load}");
    }

    #[test]
    fn failed_read_keeps_previous_frame() {
        let mut sampler = CpuSampler::new();
        sampler.advance(1, Some(Ticks::new([0, 0, 0, 0])), 0.0);
        assert_eq!(sampler.advance(1, None, 1.0), None);
        let load = sampler
            .advance(1, Some(Ticks::new([100, 0, 900, 0])), 1.0)
            .unwrap();
        assert!((load - 0.1).abs() < 1e-12, "{load}");
    }
}
