//! 持久化的配置，以单个 `NSUserDefaults` key 存储。

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSNumber, NSString, NSUserDefaults};

use crate::format::MemStyle;

const KEY_REFRESH_SECS: &str = "refresh_secs";
const KEY_MEM_STYLE: &str = "mem_style";
const KEY_NET_SHOWN: &str = "net_shown";
const KEY_NET_IFACES: &str = "net_ifaces";

/// 界面提供的全部刷新间隔。
pub const INTERVALS: [u64; 3] = [1, 2, 5];

pub fn default_interval() -> u64 {
    INTERVALS[0]
}

/// 返回已保存的间隔；当值缺失、或是由其他版本的 app 写入时，回退到默认值。
pub fn refresh_secs() -> u64 {
    let defaults = NSUserDefaults::standardUserDefaults();
    let value = defaults.integerForKey(&NSString::from_str(KEY_REFRESH_SECS));
    if value < 0 {
        return default_interval();
    }
    let value = value as u64;
    if INTERVALS.contains(&value) {
        value
    } else {
        default_interval()
    }
}

pub fn set_refresh_secs(secs: u64) {
    if !INTERVALS.contains(&secs) {
        return;
    }
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(KEY_REFRESH_SECS);
    let value: Retained<NSNumber> = NSNumber::numberWithUnsignedLongLong(secs);
    // SAFETY: value 是 `NSNumber`、key 是 `NSString`，正是 `-setObject:forKey:`
    // 所要求的类型。
    unsafe { defaults.setObject_forKey(Some(&*value as &AnyObject), &key) };
}

/// 返回已保存的内存显示风格；值缺失或越界时回退到默认风格。
pub fn mem_style() -> MemStyle {
    let defaults = NSUserDefaults::standardUserDefaults();
    let value = defaults.integerForKey(&NSString::from_str(KEY_MEM_STYLE));
    MemStyle::from_index(value)
}

pub fn set_mem_style(style: MemStyle) {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(KEY_MEM_STYLE);
    let value: Retained<NSNumber> = NSNumber::numberWithUnsignedLongLong(style.index());
    // SAFETY: value 是 `NSNumber`、key 是 `NSString`，正是 `-setObject:forKey:`
    // 所要求的类型。
    unsafe { defaults.setObject_forKey(Some(&*value as &AnyObject), &key) };
}

/// 返回网速是否显示；键缺失（首次启动）默认显示。
pub fn net_shown() -> bool {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(KEY_NET_SHOWN);
    // `integerForKey` 对缺失键返回 0，与「用户明确关掉」无法区分，而这里的
    // 默认值是「显示」，因此用 `objectForKey` 判存在性而不是读数值。
    match defaults.objectForKey(&key) {
        None => true,
        Some(_) => defaults.integerForKey(&key) != 0,
    }
}

pub fn set_net_shown(shown: bool) {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(KEY_NET_SHOWN);
    let value: Retained<NSNumber> = NSNumber::numberWithUnsignedLongLong(u64::from(shown));
    // SAFETY: value 是 `NSNumber`、key 是 `NSString`，正是 `-setObject:forKey:`
    // 所要求的类型。
    unsafe { defaults.setObject_forKey(Some(&*value as &AnyObject), &key) };
}

/// 返回持久化的「端口范围」勾选集合；键缺失（首次启动）时返回 `None`，
/// 由调用方套用「勾选全部 en*」的默认规则。
///
/// 存储格式是逗号拼接的接口名（如 `en0,en5`）：接口名不含逗号，天然可逆。
/// 集合里残留已消失的接口名无害 —— 接口可能重现，因此读取时不做修剪。
pub fn net_ifaces() -> Option<Vec<String>> {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(KEY_NET_IFACES);
    let raw = defaults.stringForKey(&key)?;
    let raw = raw.to_string();
    if raw.is_empty() {
        // 空串是「用户取消了全部勾选」，与键缺失是两种状态。
        return Some(Vec::new());
    }
    Some(raw.split(',').map(str::to_string).collect())
}

pub fn set_net_ifaces(ifaces: &[String]) {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(KEY_NET_IFACES);
    let value = NSString::from_str(&ifaces.join(","));
    // SAFETY: value 是 `NSString`、key 也是 `NSString`，正是
    // `-setObject:forKey:` 所要求的类型。
    unsafe { defaults.setObject_forKey(Some(&*value as &AnyObject), &key) };
}

/// 将 defaults 落盘，使间隔设置在立即退出或重启后依然保留。
pub fn synchronize() {
    NSUserDefaults::standardUserDefaults().synchronize();
}
