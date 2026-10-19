//! Sfm —— 一个菜单栏 CPU / 内存监控工具。
//!
//! 全部逻辑都跑在主线程上：采样只是微秒级的内核读取，因此用主队列定时器比
//! 「专门的采样线程 + 跨线程数据搬运」更简单、也更划算。

mod format;
mod menu;
mod metrics;
mod prefs;

use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::NonNull;

use dispatch2::{
    _dispatch_source_type_timer, dispatch_source_type_s, DispatchObject, DispatchQueue,
    DispatchRetained, DispatchSource, DispatchTime,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{
    define_class, extern_methods, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
    Message,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSMenuItem,
    NSStatusBar, NSStatusBarButton, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableDictionary, NSString,
};

use metrics::{is_default_enabled, Sampler, Snapshot};

const NANOS_PER_SEC: u64 = 1_000_000_000;

// `dispatch_source_create` 是一个纯 C 函数，`dispatch2` 目前尚未包装它，
// 因此在这里手工声明。返回值带 +1 的 retain 计数，由 `DispatchRetained::from_raw`
// 接管所有权。
extern "C" {
    fn dispatch_source_create(
        type_: *const dispatch_source_type_s,
        handle: usize,
        mask: usize,
        queue: *const c_void,
    ) -> *mut c_void;
}

/// 把可发消息的对象重新解释为 `AnyObject` —— AppKit 的签名（`id`）用后者来
/// 表示属性值和 target。
fn any<T: Message>(object: &T) -> &AnyObject {
    let ptr: *const T = object;
    // SAFETY: 任何 `Message` 都是 Objective-C 对象指针，这两个类型在内存布局
    // 与有效性上完全等价。
    unsafe { &*ptr.cast::<AnyObject>() }
}

fn make_timer(secs: u64) -> DispatchRetained<DispatchSource> {
    let queue = DispatchQueue::main();
    let queue_ptr = DispatchObject::as_raw(queue).as_ptr().cast::<c_void>();
    // SAFETY: `_dispatch_source_type_timer` 即定时器 source 类型；定时器既不
    // 使用 handle 也不使用 mask，且 `queue_ptr` 就是主队列。
    let raw = unsafe { dispatch_source_create(&_dispatch_source_type_timer, 0, 0, queue_ptr) };
    let raw = NonNull::new(raw).expect("dispatch_source_create returned NULL");
    // SAFETY: 该指针来自 `dispatch_source_create`，是一个带 +1 retain 计数的
    // 存活定时器 source。
    let source = unsafe { DispatchRetained::from_raw(raw.cast::<DispatchSource>()) };
    source.set_timer(DispatchTime::NOW, secs * NANOS_PER_SEC, NANOS_PER_SEC / 10);
    source
}

/// 用 `font` 构造一段富文本字符串，可选地为它套上 `color` 着色。
fn attributed(text: &str, font: &NSFont, color: Option<&NSColor>) -> Retained<NSAttributedString> {
    let attrs: Retained<NSMutableDictionary<NSAttributedStringKey, AnyObject>> =
        NSMutableDictionary::new();
    // SAFETY: key 均为 `NSString`、value 均为对象，正是
    // `-setObject:forKeyedSubscript:` 所要求的类型。
    unsafe {
        attrs.setObject_forKeyedSubscript(
            Some(any(font)),
            ProtocolObject::from_ref(NSFontAttributeName),
        );
        if let Some(color) = color {
            attrs.setObject_forKeyedSubscript(
                Some(any(color)),
                ProtocolObject::from_ref(NSForegroundColorAttributeName),
            );
        }
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(&*attrs as &NSDictionary<NSAttributedStringKey, AnyObject>),
        )
    }
}

struct App {
    sampler: Sampler,
    snapshot: Snapshot,
    interval_secs: u64,
    mem_style: format::MemStyle,
    /// 状态栏标题是否带网速段。
    net_shown: bool,
    /// 仅为保活而持有：让 AppKit 不会释放状态栏项（以及本进程）。
    _status_item: Retained<NSStatusItem>,
    button: Retained<NSStatusBarButton>,
    title_font: Retained<NSFont>,
    info_font: Retained<NSFont>,
    secondary: Retained<NSColor>,
    /// 仅在启动时「分配 App」到「装好菜单」这一瞬间为 `None`。
    parts: Option<menu::MenuParts>,
    timer: DispatchRetained<DispatchSource>,
    /// 由 `SFM_DEBUG=1` 开启：把每一帧都记到 stderr。
    debug: bool,
}

impl App {
    /// 采样全部指标，并重写状态栏项与那四行明细。
    fn refresh(&mut self) {
        self.snapshot = self.sampler.refresh();
        let snap = self.snapshot;
        let net = self.net_shown.then_some((snap.net_rx, snap.net_tx));

        let title = attributed(
            &format::status_title(snap.cpu, snap.mem_used, snap.mem_total, self.mem_style, net),
            &self.title_font,
            None,
        );
        self.button.setAttributedTitle(&title);

        let Some(parts) = self.parts.as_ref() else {
            return;
        };
        if self.debug {
            eprintln!(
                "sfm: {} | {} | {} | {} | {}",
                format::status_title(snap.cpu, snap.mem_used, snap.mem_total, self.mem_style, net,),
                format::cpu_row(snap.cpu),
                format::mem_row(snap.mem_used, snap.mem_total),
                format::swap_row(snap.swap_used),
                format::net_row(snap.net_rx, snap.net_tx),
            );
        }

        let cpu_text = attributed(
            &format::cpu_row(snap.cpu),
            &self.info_font,
            Some(&self.secondary),
        );
        parts.cpu.setAttributedTitle(Some(&cpu_text));
        let mem_text = attributed(
            &format::mem_row(snap.mem_used, snap.mem_total),
            &self.info_font,
            Some(&self.secondary),
        );
        parts.mem.setAttributedTitle(Some(&mem_text));
        let swap_text = attributed(
            &format::swap_row(snap.swap_used),
            &self.info_font,
            Some(&self.secondary),
        );
        parts.swap.setAttributedTitle(Some(&swap_text));
        let net_text = attributed(
            &format::net_row(snap.net_rx, snap.net_tx),
            &self.info_font,
            Some(&self.secondary),
        );
        parts.net.setAttributedTitle(Some(&net_text));
    }

    /// 原地修改刷新周期：复用同一个 timer source，因此改动立即生效，无需重启。
    fn set_interval(&mut self, secs: u64) {
        if !prefs::INTERVALS.contains(&secs) || secs == self.interval_secs {
            return;
        }
        self.interval_secs = secs;
        prefs::set_refresh_secs(secs);
        self.timer
            .set_timer(DispatchTime::NOW, secs * NANOS_PER_SEC, NANOS_PER_SEC / 10);
        if let Some(parts) = self.parts.as_ref() {
            for (item_secs, item) in &parts.intervals {
                item.setState(if *item_secs == secs {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
        }
        self.refresh();
    }

    /// 切换状态栏里内存的显示风格：立即重写标题、同步勾选并持久化。
    fn set_mem_style(&mut self, style: format::MemStyle) {
        if style == self.mem_style {
            return;
        }
        self.mem_style = style;
        prefs::set_mem_style(style);
        if let Some(parts) = self.parts.as_ref() {
            for (item_style, item) in &parts.mem_styles {
                item.setState(if *item_style == style {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
        }
        self.refresh();
    }

    /// 切换状态栏里网速段的显示：立即重写标题、同步勾选并持久化。
    fn set_net_shown(&mut self, shown: bool) {
        if shown == self.net_shown {
            return;
        }
        self.net_shown = shown;
        prefs::set_net_shown(shown);
        if let Some(parts) = self.parts.as_ref() {
            for (item_shown, item) in &parts.net_shown {
                item.setState(if *item_shown == shown {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
        }
        self.refresh();
    }

    /// 翻转「端口范围」里某个接口的勾选：立即改变统计范围、同步勾选并持久化。
    /// `index` 是菜单项的 tag，即 `parts.net_ifaces` 的下标。
    fn toggle_net_iface(&mut self, index: usize) {
        let Some(parts) = self.parts.as_ref() else {
            return;
        };
        let Some((name, item)) = parts.net_ifaces.get(index) else {
            return;
        };
        let name = name.clone();
        // Retained 克隆一份，尽早结束对 parts 的借用，后面的 refresh 需要 &mut self。
        let item = item.clone();

        let was_on = self.sampler.net_ifaces().contains(&name);
        let mut enabled = self.sampler.net_ifaces().clone();
        if was_on {
            enabled.remove(&name);
        } else {
            enabled.insert(name);
        }
        self.sampler.set_net_ifaces(enabled);
        // 排序后落盘，键值在 defaults 编辑器里可读。
        let mut sorted: Vec<String> = self.sampler.net_ifaces().iter().cloned().collect();
        sorted.sort();
        prefs::set_net_ifaces(&sorted);
        item.setState(if was_on {
            NSControlStateValueOff
        } else {
            NSControlStateValueOn
        });
        self.refresh();
    }

    fn quit(&self) {
        prefs::set_refresh_secs(self.interval_secs);
        prefs::set_mem_style(self.mem_style);
        prefs::set_net_shown(self.net_shown);
        let mut ifaces: Vec<String> = self.sampler.net_ifaces().iter().cloned().collect();
        ifaces.sort();
        prefs::set_net_ifaces(&ifaces);
        prefs::synchronize();
        NSApplication::sharedApplication(MainThreadMarker::new().expect("main thread"))
            .terminate(None);
    }
}

/// ObjC 实例变量，保存拥有 run loop 的那个 [`App`] 的地址。
#[derive(Clone, Copy)]
struct HandlerIvars {
    app: usize,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = HandlerIvars]
    struct MenuHandler;

    impl MenuHandler {
        #[unsafe(method(refreshNow:))]
        fn on_refresh_now(&self, _sender: *mut AnyObject) {
            self.app().refresh();
        }

        #[unsafe(method(setInterval1:))]
        fn on_interval_1(&self, _sender: *mut AnyObject) {
            self.app().set_interval(1);
        }

        #[unsafe(method(setInterval2:))]
        fn on_interval_2(&self, _sender: *mut AnyObject) {
            self.app().set_interval(2);
        }

        #[unsafe(method(setInterval5:))]
        fn on_interval_5(&self, _sender: *mut AnyObject) {
            self.app().set_interval(5);
        }

        #[unsafe(method(setMemStyleSize:))]
        fn on_mem_style_size(&self, _sender: *mut AnyObject) {
            self.app().set_mem_style(format::MemStyle::Size);
        }

        #[unsafe(method(setMemStylePercent:))]
        fn on_mem_style_percent(&self, _sender: *mut AnyObject) {
            self.app().set_mem_style(format::MemStyle::Percent);
        }

        #[unsafe(method(setNetShownOn:))]
        fn on_net_shown_on(&self, _sender: *mut AnyObject) {
            self.app().set_net_shown(true);
        }

        #[unsafe(method(setNetShownOff:))]
        fn on_net_shown_off(&self, _sender: *mut AnyObject) {
            self.app().set_net_shown(false);
        }

        #[unsafe(method(toggleNetIface:))]
        fn on_toggle_net_iface(&self, sender: *mut AnyObject) {
            if sender.is_null() {
                return;
            }
            // SAFETY: 该 selector 只挂在「端口范围」的接口项上，sender 一定是
            // NSMenuItem，其 tag 即 parts.net_ifaces 中的下标。
            let tag = unsafe { (&*sender.cast::<NSMenuItem>()).tag() };
            if tag < 0 {
                return;
            }
            self.app().toggle_net_iface(tag as usize);
        }

        #[unsafe(method(quit:))]
        fn on_quit(&self, _sender: *mut AnyObject) {
            self.app().quit();
        }
    }
);

impl MenuHandler {
    // 从 Rust 侧调用这些菜单 action；自检正是用它来发送「点击菜单项时
    // AppKit 真正会发的那些 selector」。
    extern_methods!(
        #[unsafe(method(refreshNow:))]
        fn refresh_now(&self, sender: &AnyObject);

        #[unsafe(method(setInterval1:))]
        fn set_interval_1(&self, sender: &AnyObject);

        #[unsafe(method(setInterval2:))]
        fn set_interval_2(&self, sender: &AnyObject);

        #[unsafe(method(setInterval5:))]
        fn set_interval_5(&self, sender: &AnyObject);

        #[unsafe(method(setMemStyleSize:))]
        fn set_mem_style_size(&self, sender: &AnyObject);

        #[unsafe(method(setMemStylePercent:))]
        fn set_mem_style_percent(&self, sender: &AnyObject);

        #[unsafe(method(setNetShownOn:))]
        fn set_net_shown_on(&self, sender: &AnyObject);

        #[unsafe(method(setNetShownOff:))]
        fn set_net_shown_off(&self, sender: &AnyObject);

        #[unsafe(method(toggleNetIface:))]
        fn toggle_net_iface(&self, sender: &AnyObject);

        #[unsafe(method(quit:))]
        fn quit_menu(&self, sender: &AnyObject);
    );

    fn new(app: *mut App) -> Retained<Self> {
        // SAFETY: `alloc` 返回一个全新的零值实例；ivar 中存放的指针在整个
        // 进程生命周期内都有效。
        unsafe {
            let mtm = MainThreadMarker::new().expect("main thread");
            let this = Self::alloc(mtm).set_ivars(HandlerIvars { app: app as usize });
            objc2::msg_send![super(this), init]
        }
    }

    /// 该 handler 只存在于主线程，而被故意泄漏的 [`App`] 在整个 run loop
    /// 期间都存活。
    fn app(&self) -> &'static mut App {
        // SAFETY: 该 ivar 在构造时被设为 `Box::leak` 得到的、永不释放的指针，
        // 且所有 action 都在主线程抵达。
        unsafe { &mut *(self.ivars().app as *mut App) }
    }
}

/// 定时器回调。每个刷新周期在主队列上执行一次，`ctx` 即先前安装为该 source
/// context 的那个 `App` 指针。
extern "C" fn on_tick(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: context 是在 `main` 中设置的、被 `Box::leak` 的 `App`；定时器
    // 运行在主队列上，没有别的代码会改动它。
    unsafe { (*ctx.cast::<App>()).refresh() };
}

fn main() {
    let mtm = MainThreadMarker::new().expect("Sfm must run on the main thread");
    let run_selftest = std::env::args().any(|arg| arg == "--selftest");
    let debug = std::env::var_os("SFM_DEBUG").is_some();

    let ns_app = NSApplication::sharedApplication(mtm);
    // Accessory 策略等价于 Info.plist 里的 `LSUIElement=YES`：不显示 Dock 图标，
    // 也不显示 app 菜单。
    ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let interval_secs = prefs::refresh_secs();
    let mem_style = prefs::mem_style();
    let net_shown = prefs::net_shown();
    let status_item =
        NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    let button: Retained<NSStatusBarButton> = status_item
        .button(mtm)
        .expect("a status item always has a button");

    let title_font = NSFont::monospacedDigitSystemFontOfSize_weight(0.0, 0.0);
    let info_font = NSFont::menuFontOfSize(0.0);
    let secondary = NSColor::secondaryLabelColor();

    let app_ptr: *mut App = Box::into_raw(Box::new(App {
        sampler: Sampler::new(),
        snapshot: Snapshot {
            cpu: 0.0,
            mem_used: 0,
            mem_total: 0,
            swap_used: 0,
            net_rx: 0,
            net_tx: 0,
        },
        interval_secs,
        mem_style,
        net_shown,
        _status_item: status_item.clone(),
        button: button.clone(),
        title_font,
        info_font,
        secondary,
        parts: None,
        timer: make_timer(interval_secs),
        debug,
    }));

    // SAFETY: `app_ptr` was just created and is not yet shared with the run
    // loop; the reference below is the only one that exists.
    let app = unsafe { &mut *app_ptr };
    // 接口列表在启动时采集一次（「端口范围」子菜单的条目即来自这里），同时
    // 这次刷新把各接口的字节基线归零。
    let ifaces = app.sampler.net_interface_names();
    let enabled: HashSet<String> = match prefs::net_ifaces() {
        Some(stored) => stored.into_iter().collect(),
        // 首次启动还没有持久化勾选：默认勾上全部物理网卡。
        None => ifaces
            .iter()
            .filter(|name| is_default_enabled(name))
            .cloned()
            .collect(),
    };
    app.sampler.set_net_ifaces(enabled);

    let handler = MenuHandler::new(app_ptr);
    let parts = menu::build(
        mtm,
        &handler,
        &prefs::INTERVALS,
        interval_secs,
        mem_style,
        menu::NetMenuState {
            shown: net_shown,
            ifaces,
            enabled: app.sampler.net_ifaces().clone(),
        },
    );
    let menu = parts.menu.clone();
    app.parts = Some(parts);

    // 左右键点击都弹出这个菜单 —— 这是 AppKit 的默认行为。
    status_item.setMenu(Some(&menu));

    if run_selftest {
        selftest(app, &handler);
        return;
    }

    // 先采样一次，让内存第一帧就带上真实数据（CPU 要两次刷新才有值），
    // 之后再把 app 交给 AppKit。
    app.refresh();
    // SAFETY: 该 context 只会被主队列上的 `on_tick` 读取。
    unsafe {
        app.timer.set_context(app_ptr.cast::<c_void>());
    }
    app.timer.set_event_handler_f(on_tick);
    app.timer.resume();

    ns_app.run();
}

/// 在无人点击的情况下校验菜单接线：构建真实的状态栏项与菜单，然后发送 AppKit
/// 真正会发的那些 selector。
///
/// 用 `sfm --selftest` 运行：每一步打印一行，随后退出。
fn selftest(app: &mut App, handler: &Retained<MenuHandler>) {
    // 按当前显示设置渲染标题，免得每处 println 手工拼 status_title 的参数。
    let title = |app: &App| {
        let snap = app.snapshot;
        let net = app.net_shown.then_some((snap.net_rx, snap.net_tx));
        format::status_title(snap.cpu, snap.mem_used, snap.mem_total, app.mem_style, net)
    };

    println!("interval loaded from prefs: {}", app.interval_secs);
    println!("net shown loaded from prefs: {}", app.net_shown);
    println!("status item visible: {}", app._status_item.isVisible());

    // 取相隔一秒的两帧，这样 CPU 与网速数值才是真实测量结果。
    app.refresh();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    app.refresh();
    let snap = app.snapshot;
    println!("title: {}", title(app));
    // 把文本从真实的状态栏按钮里读回来，以此证明数值确实送达了 AppKit，
    // 而不只是在自己的格式化代码里转了一圈。
    println!(
        "status item title: {}",
        app.button.attributedTitle().string()
    );
    println!("{}", format::cpu_row(snap.cpu));
    println!("{}", format::mem_row(snap.mem_used, snap.mem_total));
    println!("{}", format::swap_row(snap.swap_used));
    println!("{}", format::net_row(snap.net_rx, snap.net_tx));

    let Some(parts) = app.parts.as_ref() else {
        panic!("menu was not installed");
    };
    let sender = parts.cpu.clone();
    // 下面这些正是 AppKit 在菜单项被点击时会发送的 selector。
    handler.refresh_now(&sender);
    println!("after refreshNow: {}", title(app));

    handler.set_interval_5(&sender);
    println!(
        "after setInterval5: interval={} prefs={}",
        app.interval_secs,
        prefs::refresh_secs()
    );
    for (secs, item) in &app.parts.as_ref().unwrap().intervals {
        println!(
            "  {secs}秒 checkmark={}",
            item.state() == NSControlStateValueOn
        );
    }

    handler.set_interval_1(&sender);
    println!(
        "after setInterval1: interval={} prefs={}",
        app.interval_secs,
        prefs::refresh_secs()
    );

    println!(
        "after load: mem_style={:?} prefs={:?}",
        app.mem_style,
        prefs::mem_style()
    );
    handler.set_mem_style_percent(&sender);
    println!(
        "after setMemStylePercent: title={} prefs={:?}",
        title(app),
        prefs::mem_style()
    );
    for (style, item) in &app.parts.as_ref().unwrap().mem_styles {
        println!(
            "  内存显示·{} checkmark={}",
            menu::mem_style_title(*style),
            item.state() == NSControlStateValueOn
        );
    }

    handler.set_mem_style_size(&sender);
    println!(
        "after setMemStyleSize: title={} prefs={:?}",
        title(app),
        prefs::mem_style()
    );

    handler.set_net_shown_off(&sender);
    println!(
        "after setNetShownOff: title={} prefs={}",
        title(app),
        prefs::net_shown()
    );
    handler.set_net_shown_on(&sender);
    println!(
        "after setNetShownOn: title={} prefs={}",
        title(app),
        prefs::net_shown()
    );

    let Some(parts) = app.parts.as_ref() else {
        panic!("menu was not installed");
    };
    for (name, item) in &parts.net_ifaces {
        println!(
            "  端口范围·{name} checkmark={}",
            item.state() == NSControlStateValueOn
        );
    }
    // 翻转第一个接口再翻回来：勾选态与 prefs 都应跟着走一个来回。
    // 注意发送方必须是接口项本身 —— handler 要从它的 tag 里读下标。
    if let Some((name, item)) = parts.net_ifaces.first() {
        let iface_sender = item.clone();
        let toggle_and_report = |step: &str| {
            handler.toggle_net_iface(&iface_sender);
            let enabled = app.sampler.net_ifaces().contains(name);
            println!(
                "after toggleNetIface({name}) {step}: enabled={enabled} prefs={:?}",
                prefs::net_ifaces()
            );
        };
        toggle_and_report("once");
        toggle_and_report("again");
    }

    // 退出动作会先落盘全部设置，再请求 AppKit 终止进程；`terminate:` 会直接
    // 结束进程，这行之后不会再有输出。
    handler.quit_menu(&sender);
}
