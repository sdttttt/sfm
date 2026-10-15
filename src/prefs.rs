//! Persisted preferences, stored as a single `NSUserDefaults` key.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSNumber, NSString, NSUserDefaults};

const KEY_REFRESH_SECS: &str = "refresh_secs";

/// The only refresh intervals the UI offers.
pub const INTERVALS: [u64; 3] = [1, 2, 5];

pub fn default_interval() -> u64 {
    INTERVALS[0]
}

/// Returns the stored interval, falling back to the default when the value is
/// missing or was written by some other version of the app.
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
    // SAFETY: `value` is an `NSNumber` and `key` an `NSString`, which is what
    // `-setObject:forKey:` expects.
    unsafe { defaults.setObject_forKey(Some(&*value as &AnyObject), &key) };
}

/// Flushes the defaults so the interval survives an immediate quit or reboot.
pub fn synchronize() {
    NSUserDefaults::standardUserDefaults().synchronize();
}
