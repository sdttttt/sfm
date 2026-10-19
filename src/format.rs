//! 状态栏标题与菜单各行的文本格式化。
//!
//! 纯字符串逻辑，不依赖运行中的应用，因此可在 `cargo test` 中直接单测。

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;

/// 状态栏标题里内存数值的显示风格。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemStyle {
    /// 显示已占用的具体大小，例如 `8.4G`。想知道「到底占了多少 G」时切到它。
    Size,
    /// 显示已用内存占总内存的百分比，例如 `53%`。默认风格 —— 菜单栏宽度紧张，
    /// 而百分比在任何机器上都稳定占三位宽，比 `8.4G` / `512M` / `10.7G` 这类
    /// 随负载跳动的绝对值更容易一眼看出轻重。
    #[default]
    Percent,
}

impl MemStyle {
    /// 界面提供的全部风格，顺序即持久化用的序号。
    pub const ALL: [MemStyle; 2] = [MemStyle::Size, MemStyle::Percent];

    /// 由持久化值还原风格；负值或越界时回退到默认风格。
    ///
    /// 参数类型与 `NSUserDefaults.integerForKey` 的返回值一致（`isize`），
    /// 免去调用方做无谓的转换。
    pub fn from_index(value: isize) -> Self {
        match value {
            0 => MemStyle::Size,
            1 => MemStyle::Percent,
            _ => MemStyle::default(),
        }
    }

    /// 持久化用的序号。
    pub fn index(self) -> u64 {
        match self {
            MemStyle::Size => 0,
            MemStyle::Percent => 1,
        }
    }
}

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

/// 输出形如 ` 340K` 的速率字段（右对齐到 5 字符宽）。
///
/// 档位与 [`format_bytes`] 一致 —— 秒级窗口里 0.1M 的精度同样是噪声。多出来的
/// 填充让 ↓/↑ 两个速率字段的起止位置不随读数跳动，与百分比 `{">3}` 是同一个
/// 防抖思路；左侧填充还顺带充当了箭头与数字之间的间隔。
pub fn format_rate(bytes_per_sec: u64) -> String {
    format!("{:>5}", format_bytes(bytes_per_sec))
}

/// 把 `0.0..=1.0` 的负载四舍五入为状态栏所显示的整数百分比。
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

/// 已用内存占总内存的百分比。总量读不到时回退为 0。
pub fn mem_percent(used: u64, total: u64) -> u32 {
    if total == 0 {
        return 0;
    }
    percent(used as f64 / total as f64)
}

/// 输出形如 `"CPU= 11% MEM= 53%"`（百分比风格）或 `"CPU= 11% MEM=8.4G"`（大小风格）。
///
/// 两个百分比字段都右对齐到 3 字符宽，这样 CPU 从 9% 变到 10%、或从 99% 变到
/// 100% 时，后面的内容不会左右抖动；百分比风格下两个字段的宽度也都是固定的。
///
/// `net` 为 `Some((下行, 上行))` 时在末尾追加 `" ↓ 340K ↑1.2M"` 形式的网速段
/// （字节/秒）。`None` 时输出与不带网速的旧版标题完全一致。
pub fn status_title(
    cpu: f64,
    mem_used: u64,
    mem_total: u64,
    style: MemStyle,
    net: Option<(u64, u64)>,
) -> String {
    let mem = match style {
        MemStyle::Size => format_bytes(mem_used),
        MemStyle::Percent => format!("{:>3}%", mem_percent(mem_used, mem_total)),
    };
    let base = format!("CPU={:>3}% MEM={}", percent(cpu), mem);
    match net {
        None => base,
        // format_rate 自带的左侧填充正好充当箭头与数字之间的间隔。
        Some((rx, tx)) => format!("{} ↓{} ↑{}", base, format_rate(rx), format_rate(tx)),
    }
}

/// 输出形如 `"CPU 12%"`。
pub fn cpu_row(cpu: f64) -> String {
    format!("CPU {}%", percent(cpu))
}

/// 输出形如 `"内存 8.4G / 16.0G"`。
///
/// 菜单里始终同时给出已用量与总量，因此不受状态栏的显示风格影响 —— 即便标题
/// 切到百分比，这里仍能给出绝对数值，便于换算。
pub fn mem_row(used: u64, total: u64) -> String {
    format!("内存 {} / {}", format_bytes(used), format_bytes(total))
}

/// 输出形如 `"Swap 1.2G"`。
pub fn swap_row(used: u64) -> String {
    format!("Swap {}", format_bytes(used))
}

/// 输出形如 `"网速 ↓ 340K/s ↑1.2M/s"`。
///
/// 与标题的网速段共用同一套定宽字段，菜单里只是多了 `/s` 后缀把「这是速率」
/// 说破。
pub fn net_row(rx: u64, tx: u64) -> String {
    format!("网速 ↓{}/s ↑{}/s", format_rate(rx), format_rate(tx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Snapshot;

    const G: u64 = GIB;

    #[test]
    fn byte_formatting_boundaries() {
        assert_eq!(format_bytes(0), "0K");
        assert_eq!(format_bytes(KIB), "1K");
        assert_eq!(format_bytes(MIB - 1), "1023K");
        assert_eq!(format_bytes(MIB), "1M");
        assert_eq!(format_bytes(GIB - 1), "1023M");
        assert_eq!(format_bytes(GIB), "1.0G");
        assert_eq!(format_bytes(16 * G), "16.0G");
        assert_eq!(format_bytes(128 * G), "128.0G");
    }

    #[test]
    fn byte_formatting_matches_activity_monitor_style() {
        assert_eq!(format_bytes(9 * G + 200 * MIB), "9.2G");
        assert_eq!(format_bytes(512 * MIB), "512M");
    }

    #[test]
    fn rate_formatting_matches_byte_tiers_and_stays_five_wide() {
        assert_eq!(format_rate(0), "   0K");
        assert_eq!(format_rate(340 * KIB), " 340K");
        assert_eq!(format_rate(MIB), "   1M");
        assert_eq!(format_rate(1023 * MIB), "1023M");
        assert_eq!(format_rate(GIB + 200 * MIB), " 1.2G");
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
    fn mem_percent_guards_against_zero_total() {
        assert_eq!(mem_percent(8 * G, 16 * G), 50);
        assert_eq!(mem_percent(0, 0), 0);
        assert_eq!(mem_percent(1, 0), 0);
        // 读数异常时（已用大于总量）钳到 100%，而不是显示出 400% 这种鬼话。
        assert_eq!(mem_percent(32 * G, 16 * G), 100);
    }

    #[test]
    fn mem_style_index_round_trips() {
        for style in MemStyle::ALL {
            assert_eq!(MemStyle::from_index(style.index() as isize), style);
        }
    }

    #[test]
    fn mem_style_from_unknown_index_falls_back_to_percent() {
        // 未知序号回退到默认风格（百分比）。注意这与持久化序号 0 代表的
        // 「占用大小」是两回事：序号是存储格式，默认值是用户偏好。
        assert_eq!(MemStyle::from_index(-1), MemStyle::Percent);
        assert_eq!(MemStyle::from_index(2), MemStyle::Percent);
        assert_eq!(MemStyle::default(), MemStyle::Percent);
    }

    #[test]
    fn mem_style_index_mapping_is_stable() {
        // 序号即持久化格式。改动会让已存过设置的用户看到另一种风格，因此锁定，
        // 与「默认风格是哪个」无关。
        assert_eq!(MemStyle::Size.index(), 0);
        assert_eq!(MemStyle::Percent.index(), 1);
        assert_eq!(MemStyle::from_index(0), MemStyle::Size);
        assert_eq!(MemStyle::from_index(1), MemStyle::Percent);
    }

    #[test]
    fn title_content() {
        assert_eq!(
            status_title(0.12, 8 * G + 4 * MIB, 16 * G, MemStyle::Size, None),
            "CPU= 12% MEM=8.0G"
        );
        assert_eq!(
            status_title(0.004, 0, 0, MemStyle::Size, None),
            "CPU=  0% MEM=0K"
        );
        assert_eq!(
            status_title(1.0, 16 * G, 16 * G, MemStyle::Size, None),
            "CPU=100% MEM=16.0G"
        );
    }

    #[test]
    fn title_percent_style() {
        assert_eq!(
            status_title(0.11, 8 * G + 4 * MIB, 16 * G, MemStyle::Percent, None),
            "CPU= 11% MEM= 50%"
        );
        // 总量读不到时退化为 0%，而不是除以零。
        assert_eq!(
            status_title(0.5, 4 * G, 0, MemStyle::Percent, None),
            "CPU= 50% MEM=  0%"
        );
    }

    #[test]
    fn title_appends_net_segment() {
        assert_eq!(
            status_title(
                0.11,
                8 * G,
                16 * G,
                MemStyle::Percent,
                Some((340 * KIB, 2 * GIB))
            ),
            "CPU= 11% MEM= 50% ↓ 340K ↑ 2.0G"
        );
    }

    #[test]
    fn title_width_is_stable_across_percentages() {
        // 大小风格：CPU 位数变化时后缀位置不动。
        let sizes = [0.4f64, 9.6, 99.5, 1.0]
            .iter()
            .map(|cpu| status_title(*cpu, 8 * G + 4 * MIB, 16 * G, MemStyle::Size, None).len())
            .collect::<Vec<_>>();
        assert!(sizes.windows(2).all(|w| w[0] == w[1]), "{sizes:?}");

        // 百分比风格：两个字段都是定宽，整个标题长度恒定。
        let pcts = [0.4f64, 9.6, 99.5, 1.0]
            .iter()
            .map(|cpu| {
                let used = (*cpu * 0.5 * 16.0 * G as f64) as u64;
                status_title(*cpu, used, 16 * G, MemStyle::Percent, None).len()
            })
            .collect::<Vec<_>>();
        assert!(pcts.windows(2).all(|w| w[0] == w[1]), "{pcts:?}");
    }

    #[test]
    fn title_width_is_stable_across_rates() {
        // 网速段两个速率字段都是定宽，速率跳动时标题长度恒定 —— 空闲时的
        // `0K` 与满速时的 `1023M` 占位完全一样。
        let rates = [0, 512, 340 * KIB, 512 * MIB, 1023 * MIB, 2 * GIB];
        let widths = rates
            .iter()
            .map(|&rate| {
                status_title(0.11, 8 * G, 16 * G, MemStyle::Percent, Some((rate, 0))).len()
            })
            .collect::<Vec<_>>();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
    }

    #[test]
    fn menu_rows() {
        assert_eq!(cpu_row(0.126), "CPU 13%");
        assert_eq!(mem_row(8 * G + 4 * MIB, 16 * G), "内存 8.0G / 16.0G");
        assert_eq!(swap_row(0), "Swap 0K");
        assert_eq!(swap_row(GIB + MIB * 200), "Swap 1.2G");
        assert_eq!(net_row(340 * KIB, 0), "网速 ↓ 340K/s ↑   0K/s");
    }

    #[test]
    fn rows_are_derived_from_a_snapshot() {
        let snap = Snapshot {
            cpu: 0.5,
            mem_used: 2 * G,
            mem_total: 8 * G,
            swap_used: MIB,
            net_rx: 340 * KIB,
            net_tx: 1023 * MIB,
        };
        assert_eq!(cpu_row(snap.cpu), "CPU 50%");
        assert_eq!(mem_row(snap.mem_used, snap.mem_total), "内存 2.0G / 8.0G");
        assert_eq!(swap_row(snap.swap_used), "Swap 1M");
        assert_eq!(net_row(snap.net_rx, snap.net_tx), "网速 ↓ 340K/s ↑1023M/s");
        assert_eq!(
            status_title(
                snap.cpu,
                snap.mem_used,
                snap.mem_total,
                MemStyle::Size,
                None
            ),
            "CPU= 50% MEM=2.0G"
        );
        assert_eq!(
            status_title(
                snap.cpu,
                snap.mem_used,
                snap.mem_total,
                MemStyle::Percent,
                Some((snap.net_rx, snap.net_tx)),
            ),
            "CPU= 50% MEM= 25% ↓ 340K ↑1023M"
        );
    }
}
