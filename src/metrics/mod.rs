//! System metric sampling. All `unsafe` FFI is confined to this module.

mod cpu;
mod mem;

pub use cpu::CpuSampler;
pub use mem::{memory_used, swap_used, total_memory};

/// A consistent view of the numbers shown in the status item and the menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    /// Fraction of non-idle time, `0.0..=1.0`.
    pub cpu: f64,
    /// Physical memory in use (active + wired + compressed), in bytes.
    pub mem_used: u64,
    /// Physical memory installed, in bytes.
    pub mem_total: u64,
    /// Swap in use, in bytes.
    pub swap_used: u64,
}
