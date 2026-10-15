//! Sfm — a menu bar CPU / memory monitor.
//!
//! Everything runs on the main thread: the sampling calls are microsecond-scale
//! kernel reads, so a main-queue timer is simpler and cheaper than a dedicated
//! sampling thread plus cross-thread plumbing.

mod format;
mod menu;
mod metrics;
mod prefs;

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
    NSControlStateValueOn, NSFont, NSFontAttributeName, NSForegroundColorAttributeName,
    NSStatusBar, NSStatusBarButton, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableDictionary, NSString,
};

use metrics::{CpuSampler, Snapshot};

const NANOS_PER_SEC: u64 = 1_000_000_000;

// `dispatch_source_create` is a plain C function that `dispatch2` does not wrap
// yet, so it is declared here. The returned source comes back with a +1 retain
// count, which `DispatchRetained::from_raw` takes ownership of.
extern "C" {
    fn dispatch_source_create(
        type_: *const dispatch_source_type_s,
        handle: usize,
        mask: usize,
        queue: *const c_void,
    ) -> *mut c_void;
}

/// Reinterprets a messageable object as `AnyObject`, which AppKit signatures
/// (`id`) use for attribute values and targets.
fn any<T: Message>(object: &T) -> &AnyObject {
    let ptr: *const T = object;
    // SAFETY: every `Message` is an Objective-C object pointer, so the two
    // types have identical layout and validity.
    unsafe { &*ptr.cast::<AnyObject>() }
}

fn make_timer(secs: u64) -> DispatchRetained<DispatchSource> {
    let queue = DispatchQueue::main();
    let queue_ptr = DispatchObject::as_raw(queue).as_ptr().cast::<c_void>();
    // SAFETY: `_dispatch_source_type_timer` is the timer source type; a timer
    // uses neither a handle nor a mask, and `queue_ptr` is the main queue.
    let raw = unsafe { dispatch_source_create(&_dispatch_source_type_timer, 0, 0, queue_ptr) };
    let raw = NonNull::new(raw).expect("dispatch_source_create returned NULL");
    // SAFETY: the pointer came from `dispatch_source_create` and is a live timer
    // source owned with a +1 retain count.
    let source = unsafe { DispatchRetained::from_raw(raw.cast::<DispatchSource>()) };
    source.set_timer(DispatchTime::NOW, secs * NANOS_PER_SEC, NANOS_PER_SEC / 10);
    source
}

/// Builds an attributed string in `font`, optionally tinted with `color`.
fn attributed(text: &str, font: &NSFont, color: Option<&NSColor>) -> Retained<NSAttributedString> {
    let attrs: Retained<NSMutableDictionary<NSAttributedStringKey, AnyObject>> =
        NSMutableDictionary::new();
    // SAFETY: the keys are `NSString`s and the values are objects, which is what
    // `-setObject:forKeyedSubscript:` expects.
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
    cpu: CpuSampler,
    snapshot: Snapshot,
    interval_secs: u64,
    /// Kept alive so AppKit keeps the status item (and this process) around.
    _status_item: Retained<NSStatusItem>,
    button: Retained<NSStatusBarButton>,
    title_font: Retained<NSFont>,
    info_font: Retained<NSFont>,
    secondary: Retained<NSColor>,
    /// `None` only between allocation and menu installation at startup.
    parts: Option<menu::MenuParts>,
    timer: DispatchRetained<DispatchSource>,
    /// Set by `SFM_DEBUG=1`: log every frame to stderr.
    debug: bool,
}

impl App {
    /// Samples every metric and rewrites the status item and the three info
    /// rows. A failed sample keeps the previous value instead of dropping to
    /// zero, which would read as "the machine went idle".
    fn refresh(&mut self) {
        if let Some(cpu) = self.cpu.sample(self.interval_secs) {
            self.snapshot.cpu = cpu;
        }
        if let Some(used) = metrics::memory_used() {
            self.snapshot.mem_used = used;
        }
        if let Some(total) = metrics::total_memory() {
            self.snapshot.mem_total = total;
        }
        if let Some(swap) = metrics::swap_used() {
            self.snapshot.swap_used = swap;
        }

        let title = attributed(
            &format::status_title(self.snapshot.cpu, self.snapshot.mem_used),
            &self.title_font,
            None,
        );
        self.button.setAttributedTitle(&title);

        let Some(parts) = self.parts.as_ref() else {
            return;
        };
        if self.debug {
            eprintln!(
                "sfm: {} | {} | {} | {}",
                format::status_title(self.snapshot.cpu, self.snapshot.mem_used),
                format::cpu_row(self.snapshot.cpu),
                format::mem_row(self.snapshot.mem_used, self.snapshot.mem_total),
                format::swap_row(self.snapshot.swap_used),
            );
        }

        let snap = self.snapshot;
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
    }

    /// Switches the refresh period in place: the timer source is reused, so the
    /// change takes effect immediately without a restart.
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

    fn quit(&self) {
        prefs::set_refresh_secs(self.interval_secs);
        prefs::synchronize();
        NSApplication::sharedApplication(MainThreadMarker::new().expect("main thread"))
            .terminate(None);
    }
}

/// Instance variable holding the address of the [`App`] that owns the run loop.
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

        #[unsafe(method(quit:))]
        fn on_quit(&self, _sender: *mut AnyObject) {
            self.app().quit();
        }
    }
);

impl MenuHandler {
    // Rust-side view of the menu actions, used by the self test to invoke the
    // very selectors AppKit sends when a menu item is clicked.
    extern_methods!(
        #[unsafe(method(refreshNow:))]
        fn refresh_now(&self, sender: &AnyObject);

        #[unsafe(method(setInterval1:))]
        fn set_interval_1(&self, sender: &AnyObject);

        #[unsafe(method(setInterval2:))]
        fn set_interval_2(&self, sender: &AnyObject);

        #[unsafe(method(setInterval5:))]
        fn set_interval_5(&self, sender: &AnyObject);

        #[unsafe(method(quit:))]
        fn quit_menu(&self, sender: &AnyObject);
    );

    fn new(app: *mut App) -> Retained<Self> {
        // SAFETY: `alloc` returns a fresh, zeroed instance; the ivar holds a
        // pointer that is valid for the rest of the process.
        unsafe {
            let mtm = MainThreadMarker::new().expect("main thread");
            let this = Self::alloc(mtm).set_ivars(HandlerIvars { app: app as usize });
            objc2::msg_send![super(this), init]
        }
    }

    /// The handler only exists on the main thread, where the leaked [`App`] is
    /// alive for the whole run loop.
    fn app(&self) -> &'static mut App {
        // SAFETY: the ivar was set at construction time to a `Box::leak`ed
        // pointer that is never freed, and actions arrive on the main thread.
        unsafe { &mut *(self.ivars().app as *mut App) }
    }
}

/// Timer callback. Runs on the main queue once per refresh interval, with the
/// `App` pointer that was installed as the source's context.
extern "C" fn on_tick(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: the context is the `Box::leak`ed `App` set up in `main`, and the
    // timer runs on the main queue, where nothing else mutates it.
    unsafe { (*ctx.cast::<App>()).refresh() };
}

fn main() {
    let mtm = MainThreadMarker::new().expect("Sfm must run on the main thread");
    let run_selftest = std::env::args().any(|arg| arg == "--selftest");
    let debug = std::env::var_os("SFM_DEBUG").is_some();

    let ns_app = NSApplication::sharedApplication(mtm);
    // Accessory policy is the runtime equivalent of `LSUIElement=YES`: no Dock
    // icon and no app menu.
    ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let interval_secs = prefs::refresh_secs();
    let status_item =
        NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    let button: Retained<NSStatusBarButton> = status_item
        .button(mtm)
        .expect("a status item always has a button");

    let title_font = NSFont::monospacedDigitSystemFontOfSize_weight(0.0, 0.0);
    let info_font = NSFont::menuFontOfSize(0.0);
    let secondary = NSColor::secondaryLabelColor();

    let app_ptr: *mut App = Box::into_raw(Box::new(App {
        cpu: CpuSampler::new(),
        snapshot: Snapshot {
            cpu: 0.0,
            mem_used: 0,
            mem_total: 0,
            swap_used: 0,
        },
        interval_secs,
        _status_item: status_item.clone(),
        button: button.clone(),
        title_font,
        info_font,
        secondary,
        parts: None,
        timer: make_timer(interval_secs),
        debug,
    }));

    let handler = MenuHandler::new(app_ptr);
    let parts = menu::build(mtm, &handler, &prefs::INTERVALS, interval_secs);
    let menu = parts.menu.clone();
    // SAFETY: `app_ptr` was just created and is not yet shared with the run
    // loop; the reference below is the only one that exists.
    let app = unsafe { &mut *app_ptr };
    app.parts = Some(parts);

    // Both left and right clicks open this same menu — AppKit's default.
    status_item.setMenu(Some(&menu));

    if run_selftest {
        selftest(app, &handler);
        return;
    }

    // Sample once up front so the very first frame carries real data, then hand
    // the app over to AppKit.
    app.refresh();
    // SAFETY: the context is only read by `on_tick` on the main queue.
    unsafe {
        app.timer.set_context(app_ptr.cast::<c_void>());
    }
    app.timer.set_event_handler_f(on_tick);
    app.timer.resume();

    ns_app.run();
}

/// Exercises the menu wiring without a human clicking: builds the real status
/// item and menu, then sends the exact selectors AppKit would send.
///
/// Run with `sfm --selftest`. Prints one line per step and exits.
fn selftest(app: &mut App, handler: &Retained<MenuHandler>) {
    println!("interval loaded from prefs: {}", app.interval_secs);
    println!("status item visible: {}", app._status_item.isVisible());

    // Two frames one second apart, so the CPU number is a real measurement.
    app.refresh();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    app.refresh();
    let snap = app.snapshot;
    println!("title: {}", format::status_title(snap.cpu, snap.mem_used));
    // Read the text back out of the real status bar button, so this proves the
    // value reached AppKit and not just our own formatting code.
    println!(
        "status item title: {}",
        app.button.attributedTitle().string()
    );
    println!("{}", format::cpu_row(snap.cpu));
    println!("{}", format::mem_row(snap.mem_used, snap.mem_total));
    println!("{}", format::swap_row(snap.swap_used));

    let Some(parts) = app.parts.as_ref() else {
        panic!("menu was not installed");
    };
    let sender = parts.cpu.clone();
    // These are the same selectors AppKit sends when the items are clicked.
    handler.refresh_now(&sender);
    println!(
        "after refreshNow: {}",
        format::status_title(app.snapshot.cpu, app.snapshot.mem_used)
    );

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

    // The quit action persists the interval and asks AppKit to terminate.
    handler.quit_menu(&sender);
    println!("quit returned (process is still alive)");
}
