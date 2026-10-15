//! Construction of the status item's drop-down menu.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

use crate::format;

/// Selector for the "刷新间隔 ▸ 1/2/5 秒" items.
pub fn interval_selector(secs: u64) -> Sel {
    match secs {
        1 => sel!(setInterval1:),
        2 => sel!(setInterval2:),
        _ => sel!(setInterval5:),
    }
}

/// The items whose text is rewritten on every tick, plus the interval items.
pub struct MenuParts {
    pub menu: Retained<NSMenu>,
    pub cpu: Retained<NSMenuItem>,
    pub mem: Retained<NSMenuItem>,
    pub swap: Retained<NSMenuItem>,
    pub intervals: Vec<(u64, Retained<NSMenuItem>)>,
}

fn new_item(mtm: MainThreadMarker, title: &str, action: Option<Sel>) -> Retained<NSMenuItem> {
    // SAFETY: the item is initialised with a valid title and action selector.
    let item: Retained<NSMenuItem> = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(""),
        )
    };
    item.setEnabled(action.is_some());
    item
}

/// A grey, non-actionable row that only reports a number.
fn info_item(mtm: MainThreadMarker, text: &str) -> Retained<NSMenuItem> {
    new_item(mtm, text, None)
}

fn action_item(
    mtm: MainThreadMarker,
    target: &AnyObject,
    title: &str,
    action: Sel,
) -> Retained<NSMenuItem> {
    let item = new_item(mtm, title, Some(action));
    // SAFETY: `target` is the app's handler, which outlives the menu that owns
    // this item.
    unsafe {
        item.setTarget(Some(target));
    }
    item
}

/// Builds the menu. The status item keeps it alive.
pub fn build(
    mtm: MainThreadMarker,
    target: &AnyObject,
    intervals: &[u64],
    selected: u64,
) -> MenuParts {
    let menu: Retained<NSMenu> =
        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Sfm"));

    let cpu = info_item(mtm, &format::cpu_row(0.0));
    let mem = info_item(mtm, &format::mem_row(0, 0));
    let swap = info_item(mtm, &format::swap_row(0));
    menu.addItem(&cpu);
    menu.addItem(&mem);
    menu.addItem(&swap);
    menu.addItem(&NSMenuItem::separatorItem(mtm));

    menu.addItem(&action_item(mtm, target, "立即刷新", sel!(refreshNow:)));

    let interval_parent = new_item(mtm, "刷新间隔", None);
    interval_parent.setEnabled(true);
    let interval_menu: Retained<NSMenu> =
        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("刷新间隔"));
    let mut interval_items = Vec::with_capacity(intervals.len());
    for &secs in intervals {
        let item = action_item(mtm, target, &format!("{secs} 秒"), interval_selector(secs));
        item.setState(if secs == selected {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        interval_menu.addItem(&item);
        interval_items.push((secs, item));
    }
    interval_parent.setSubmenu(Some(&interval_menu));
    menu.addItem(&interval_parent);

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&action_item(mtm, target, "退出 Sfm", sel!(quit:)));

    MenuParts {
        menu,
        cpu,
        mem,
        swap,
        intervals: interval_items,
    }
}
