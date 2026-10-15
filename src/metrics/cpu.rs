use std::mem::MaybeUninit;
use std::time::Instant;

// `mach_host_self` is deprecated in favour of the `mach2` crate; a third
// dependency is not worth one port lookup.
#[allow(deprecated)]
use libc::mach_host_self;
use libc::{host_statistics, HOST_CPU_LOAD_INFO};

/// Samples closer together than this are dropped. Tick counters are coarse
/// enough that a few-millisecond window can report 100%, which would make the
/// menu bar jump on 「立即刷新」.
const MIN_SAMPLE_SECS: f64 = 0.2;

/// Tick counts reported by the kernel, ordered `[user, system, idle, nice]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticks([u64; 4]);

impl Ticks {
    #[cfg(test)]
    pub(crate) fn new(ticks: [u64; 4]) -> Self {
        Ticks(ticks)
    }
}

/// Computes CPU load from two tick samples, or `None` if they are unusable.
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
    // SAFETY: `HOST_CPU_LOAD_INFO` fills exactly `HOST_CPU_LOAD_INFO_COUNT`
    // integers; the buffer is sized for that and passed with its real count.
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

/// Keeps the previous tick baseline so each call can report the load since the
/// last one.
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

    /// Returns the load measured since the previous call, or `None` when there
    /// is no usable measurement (first frame, kernel failure, or a gap so wide
    /// that the machine was asleep or the process was suspended).
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
            // Keep the previous baseline: a dropped frame should not poison the
            // next measurement.
            return None;
        };
        let Some(prev) = self.prev else {
            // First frame: remember the baseline and wait for the next one.
            self.prev = Some(ticks);
            self.last_at = Some(Instant::now());
            return None;
        };
        if elapsed < MIN_SAMPLE_SECS {
            // Too soon to measure anything meaningful. Keep the baseline so the
            // next tick measures a full window.
            return None;
        }
        if elapsed > 3.0 * refresh_secs as f64 {
            // Sleep/wake or a long debugger pause: the tick delta is meaningless,
            // but the frame we just read becomes the new baseline.
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
        // 1000 busy ticks out of 4000 total => 25%.
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
        // Woke from sleep after minutes: this frame must be discarded.
        assert_eq!(
            sampler.advance(1, Some(Ticks::new([900, 0, 100, 0])), 600.0),
            None
        );
        // The discarded frame became the new baseline, so the next frame is a
        // normal delta again: 100 busy ticks out of 2000 total.
        let load = sampler
            .advance(1, Some(Ticks::new([1000, 0, 2000, 0])), 1.0)
            .unwrap();
        assert!((load - 0.05).abs() < 1e-12, "{load}");
    }

    #[test]
    fn long_gap_is_relative_to_refresh_interval() {
        let mut sampler = CpuSampler::new();
        sampler.advance(5, Some(Ticks::new([0, 0, 0, 0])), 0.0);
        // 10s is only 2x a 5s interval, so the frame is kept.
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
        // 「立即刷新」a few milliseconds later must not report 100%.
        assert_eq!(
            sampler.advance(1, Some(Ticks::new([200, 0, 1000, 0])), 0.01),
            None
        );
        // And the baseline is untouched, so the next timer tick measures the
        // full window from [100, 0, 900] to [300, 0, 1900]: 200 of 1200 ticks.
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
