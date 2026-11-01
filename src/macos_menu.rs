//! Make "About Priogator" in the macOS app menu open the app's own About
//! dialog instead of the standard panel.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::NSApplication;

static REQUESTED: AtomicBool = AtomicBool::new(false);
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PriogatorAboutTarget"]
    struct AboutTarget;

    impl AboutTarget {
        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            REQUESTED.store(true, Ordering::Relaxed);
            if let Some(ctx) = CONTEXT.get() {
                ctx.request_repaint();
            }
        }
    }
);

impl AboutTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

/// Point the menu item to us. Call once the window exists.
pub fn install(ctx: &egui::Context) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let _ = CONTEXT.set(ctx.clone());
    let app = NSApplication::sharedApplication(mtm);
    let Some(items) = app
        .mainMenu()
        .and_then(|menu| menu.itemAtIndex(0))
        .and_then(|item| item.submenu())
        .map(|menu| menu.itemArray())
    else {
        return;
    };
    let standard = sel!(orderFrontStandardAboutPanel:);
    let Some(about) = items.iter().find(|item| item.action() == Some(standard)) else {
        return;
    };
    let target = AboutTarget::new(mtm);
    unsafe {
        about.setTarget(Some(&target));
        about.setAction(Some(sel!(showAbout:)));
    }
    // The menu item doesn't keep its target alive.
    std::mem::forget(target);
}

/// Whether the menu item was chosen since the last call.
pub fn take_request() -> bool {
    REQUESTED.swap(false, Ordering::Relaxed)
}
