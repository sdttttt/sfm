//! 状态栏标题与菜单各行的文本格式化。
//!
//! 纯字符串逻辑，不依赖运行中的应用，因此可在 `cargo test` 中直接单测。

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;

/// 输出形如 `8.4G`、`512M`、`0K`。
///
/// 1 GiB 以下取整：在菜单栏里，0.1M 的精度纯属噪声；1 GiB 以上保留一位小数，
/// 其分辨率已足够有用。
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= GIB {
        format!("{:.1}G", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{}M", bytes / MIB)
    } else {
        format!("{}K", bytes / KIB)
    }
}

/// 把 `0.0..=1.0` 的负载四舍五入为菜单栏所显示的整数百分比。
pub fn percent(cpu: f64) -> u32 {
    if !cpu.is_finite() || cpu <= 0.0 {
        return 0;
    }
    let pct = (cpu * 100.0).round();
    if pct >= 100.0 {
        100
    } else {
        pct as u32
    }
}

/// 输出形如 `" 12% 8.4G"`。
///
/// 百分比右对齐到 3 字符宽，这样当 CPU 从 9% 变到 10%、或从 99% 变到 100% 时，
/// 后面的内存数值不会左右抖动。
pub fn status_title(cpu: f64, mem_used: u64) -> String {
    format!("{:>3}% {}", percent(cpu), format_bytes(mem_used))
}

/// 输出形如 `"CPU 12%"`。
pub fn cpu_row(cpu: f64) -> String {
    format!("CPU {}%", percent(cpu))
}

/// 输出形如 `"内存 8.4G / 16.0G"`。
pub fn mem_row(used: u64, total: u64) -> String {
    format!("内存 {} / {}", format_bytes(used), format_bytes(total))
}

/// 输出形如 `"Swap 1.2G"`。
pub fn swap_row(used: u64) -> String {
    format!("Swap {}", format_bytes(used))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Snapshot;

    #[test]
    fn byte_formatting_boundaries() {
        assert_eq!(format_bytes(0), "0K");
        assert_eq!(format_bytes(KIB), "1K");
        assert_eq!(format_bytes(MIB - 1), "1023K");
        assert_eq!(format_bytes(MIB), "1M");
        assert_eq!(format_bytes(GIB - 1), "1023M");
        assert_eq!(format_bytes(GIB), "1.0G");
        assert_eq!(format_bytes(16 * GIB), "16.0G");
        assert_eq!(format_bytes(128 * GIB), "128.0G");
    }

    #[test]
    fn byte_formatting_matches_activity_monitor_style() {
        assert_eq!(format_bytes(9 * GIB + 200 * MIB), "9.2G");
        assert_eq!(format_bytes(512 * MIB), "512M");
    }

    #[test]
    fn percent_rounds_and_clamps() {
        assert_eq!(percent(0.0), 0);
        assert_eq!(percent(0.004), 0);
        assert_eq!(percent(0.096), 10);
        assert_eq!(percent(0.995), 100);
        assert_eq!(percent(1.0), 100);
        assert_eq!(percent(f64::NAN), 0);
    }

    #[test]
    fn title_width_is_stable_across_percentages() {
        let widths = [0.4f64, 9.6, 99.5, 1.0]
            .iter()
            .map(|cpu| status_title(*cpu, 8 * GIB + 4 * MIB).len())
            .collect::<Vec<_>>();
        assert_eq!(widths[0], widths[1], "{widths:?}");
        assert_eq!(widths[1], widths[2], "{widths:?}");
        assert_eq!(widths[2], widths[3], "{widths:?}");
    }

    #[test]
    fn title_content() {
        assert_eq!(status_title(0.12, 8 * GIB + 4 * MIB), " 12% 8.0G");
        assert_eq!(status_title(0.004, 0), "  0% 0K");
        assert_eq!(status_title(1.0, 16 * GIB), "100% 16.0G");
    }

    #[test]
    fn menu_rows() {
        assert_eq!(cpu_row(0.126), "CPU 13%");
        assert_eq!(mem_row(8 * GIB + 4 * MIB, 16 * GIB), "内存 8.0G / 16.0G");
        assert_eq!(swap_row(0), "Swap 0K");
        assert_eq!(swap_row(GIB + MIB * 200), "Swap 1.2G");
    }

    #[test]
    fn rows_are_derived_from_a_snapshot() {
        let snap = Snapshot {
            cpu: 0.5,
            mem_used: 2 * GIB,
            mem_total: 8 * GIB,
            swap_used: MIB,
        };
        assert_eq!(cpu_row(snap.cpu), "CPU 50%");
        assert_eq!(mem_row(snap.mem_used, snap.mem_total), "内存 2.0G / 8.0G");
        assert_eq!(swap_row(snap.swap_used), "Swap 1M");
        assert_eq!(status_title(snap.cpu, snap.mem_used), " 50% 2.0G");
    }
}
