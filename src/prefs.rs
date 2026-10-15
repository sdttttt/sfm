//! 持久化的配置，以单个 `NSUserDefaults` key 存储。

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSNumber, NSString, NSUserDefaults};

use crate::format::MemStyle;

const KEY_REFRESH_SECS: &str = "refresh_secs";
const KEY_MEM_STYLE: &str = "mem_style";

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

/// 将 defaults 落盘，使间隔设置在立即退出或重启后依然保留。
pub fn synchronize() {
    NSUserDefaults::standardUserDefaults().synchronize();
}
