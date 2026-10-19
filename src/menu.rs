//! 构造状态栏项的下拉菜单。

use std::collections::HashSet;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSControlStateValueOff, NSControlStateValueOn, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

use crate::format::{self, MemStyle};

/// 「刷新间隔 ▸ 1/2/5 秒」各菜单项对应的 selector。
pub fn interval_selector(secs: u64) -> Sel {
    match secs {
        1 => sel!(setInterval1:),
        2 => sel!(setInterval2:),
        _ => sel!(setInterval5:),
    }
}

/// 「内存显示 ▸ 占用大小/百分比」各菜单项对应的 selector。
pub fn mem_style_selector(style: MemStyle) -> Sel {
    match style {
        MemStyle::Size => sel!(setMemStyleSize:),
        MemStyle::Percent => sel!(setMemStylePercent:),
    }
}

/// 风格选项的标题，供菜单与自检共用。
pub fn mem_style_title(style: MemStyle) -> &'static str {
    match style {
        MemStyle::Size => "占用大小",
        MemStyle::Percent => "百分比",
    }
}

/// 「网速显示 ▸ 显示/隐藏」各菜单项对应的 selector 与标题。
pub fn net_shown_selector(shown: bool) -> Sel {
    if shown {
        sel!(setNetShownOn:)
    } else {
        sel!(setNetShownOff:)
    }
}

pub fn net_shown_title(shown: bool) -> &'static str {
    if shown {
        "显示"
    } else {
        "隐藏"
    }
}

/// 需要在每个 tick 重写文本的菜单项，以及各间隔、风格选项本身。
pub struct MenuParts {
    pub menu: Retained<NSMenu>,
    pub cpu: Retained<NSMenuItem>,
    pub mem: Retained<NSMenuItem>,
    pub swap: Retained<NSMenuItem>,
    pub net: Retained<NSMenuItem>,
    pub intervals: Vec<(u64, Retained<NSMenuItem>)>,
    pub mem_styles: Vec<(MemStyle, Retained<NSMenuItem>)>,
    pub net_shown: Vec<(bool, Retained<NSMenuItem>)>,
    /// 「端口范围 ▸」各接口项，顺序与启动时采集的接口名列表一致；
    /// `toggleNetIface:` 靠菜单项的 tag（= 此处的下标）反查是哪个接口。
    pub net_ifaces: Vec<(String, Retained<NSMenuItem>)>,
}

/// 装配菜单所需的网速初始状态。
pub struct NetMenuState {
    /// 状态栏标题是否带网速段。
    pub shown: bool,
    /// 启动时采集的接口名列表，顺序即「端口范围」勾选项的下标。
    pub ifaces: Vec<String>,
    /// 其中被勾选的集合。
    pub enabled: HashSet<String>,
}

fn new_item(mtm: MainThreadMarker, title: &str, action: Option<Sel>) -> Retained<NSMenuItem> {
    // SAFETY: 该菜单项是用有效的标题与 action selector 初始化出来的。
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

/// 灰显、不可点击、只用来展示数字的一行。
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
    // SAFETY: target 是 app 的 handler，其生命周期长于持有该菜单项的菜单。
    unsafe {
        item.setTarget(Some(target));
    }
    item
}

/// 构建菜单。它由状态栏项保活。
pub fn build(
    mtm: MainThreadMarker,
    target: &AnyObject,
    intervals: &[u64],
    selected: u64,
    mem_style: MemStyle,
    net: NetMenuState,
) -> MenuParts {
    let NetMenuState {
        shown: net_shown,
        ifaces: net_ifaces,
        enabled: net_enabled,
    } = &net;
    let menu: Retained<NSMenu> =
        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Sfm"));

    let cpu = info_item(mtm, &format::cpu_row(0.0));
    let mem = info_item(mtm, &format::mem_row(0, 0));
    let swap = info_item(mtm, &format::swap_row(0));
    let net = info_item(mtm, &format::net_row(0, 0));
    menu.addItem(&cpu);
    menu.addItem(&mem);
    menu.addItem(&swap);
    menu.addItem(&net);
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

    let style_parent = new_item(mtm, "内存显示", None);
    style_parent.setEnabled(true);
    let style_menu: Retained<NSMenu> =
        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("内存显示"));
    let mut style_items = Vec::with_capacity(MemStyle::ALL.len());
    for style in MemStyle::ALL {
        let item = action_item(
            mtm,
            target,
            mem_style_title(style),
            mem_style_selector(style),
        );
        item.setState(if style == mem_style {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        style_menu.addItem(&item);
        style_items.push((style, item));
    }
    style_parent.setSubmenu(Some(&style_menu));
    menu.addItem(&style_parent);

    let net_shown_parent = new_item(mtm, "网速显示", None);
    net_shown_parent.setEnabled(true);
    let net_shown_menu: Retained<NSMenu> =
        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("网速显示"));
    let mut net_shown_items = Vec::with_capacity(2);
    for shown in [true, false] {
        let item = action_item(
            mtm,
            target,
            net_shown_title(shown),
            net_shown_selector(shown),
        );
        item.setState(if shown == *net_shown {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        net_shown_menu.addItem(&item);
        net_shown_items.push((shown, item));
    }
    net_shown_parent.setSubmenu(Some(&net_shown_menu));
    menu.addItem(&net_shown_parent);

    // 接口名是运行时数据，没法为每个值各配一个编译期 selector，因此全部走
    // `toggleNetIface:`，用 tag 存下标。
    let iface_parent = new_item(mtm, "端口范围", None);
    iface_parent.setEnabled(true);
    let iface_menu: Retained<NSMenu> =
        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("端口范围"));
    let mut iface_items = Vec::with_capacity(net_ifaces.len());
    for (index, name) in net_ifaces.iter().enumerate() {
        let item = action_item(mtm, target, name, sel!(toggleNetIface:));
        item.setTag(index as isize);
        item.setState(if net_enabled.contains(name) {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        iface_menu.addItem(&item);
        iface_items.push((name.clone(), item));
    }
    iface_parent.setSubmenu(Some(&iface_menu));
    menu.addItem(&iface_parent);

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&action_item(mtm, target, "退出 Sfm", sel!(quit:)));

    MenuParts {
        menu,
        cpu,
        mem,
        swap,
        net,
        intervals: interval_items,
        mem_styles: style_items,
        net_shown: net_shown_items,
        net_ifaces: iface_items,
    }
}
