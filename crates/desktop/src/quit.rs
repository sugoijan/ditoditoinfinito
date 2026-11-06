//! Quitting with the window left open for the system to take down.
//!
//! The shell ends the process itself (`Shell::exit`), from inside the
//! event loop once what quits with the game is done, with its window
//! still open, as AppKit's own `terminate:` ends an app: the system takes
//! the window down with the process, and its fullscreen Space with it.
//! winit closes every window as its loop ends, and on macOS a window
//! closed in its fullscreen Space starts the Space's way out, which the
//! app runs on its run loop; a process ending right after left the
//! window's last frame on screen, frozen, for a while after it had gone.
//!
//! winit answers `terminate:` by closing the windows too, so on macOS the
//! ways to it are pointed at the shell's quit ([`route`], [`asked`]): the
//! app menu's Quit (⌘Q) and the quit Apple Event (the Dock's Quit,
//! logging out). Elsewhere there is nothing to route.

/// Points macOS's ways to quit at the shell's quit ([`asked`]) instead of
/// `terminate:`. Once the app is up (from `resumed`); a second call
/// changes nothing.
pub fn route() {
    #[cfg(target_os = "macos")]
    imp::route();
}

/// The app was asked to quit (macOS's menu or Apple Event) since the
/// last call.
pub fn asked() -> bool {
    #[cfg(target_os = "macos")]
    {
        imp::ASKED.replace(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::{Cell, RefCell};

    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSAppleEventManager;

    /// `kCoreEventClass` and `kAEQuitApplication`.
    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
    const QUIT_APPLICATION: u32 = u32::from_be_bytes(*b"quit");

    thread_local! {
        /// A quit was asked ([`super::asked`]).
        pub static ASKED: Cell<bool> = const { Cell::new(false) };
        /// The target, which neither the menu item nor the Apple Event
        /// manager keeps.
        static TARGET: RefCell<Option<Retained<Target>>> = const { RefCell::new(None) };
    }

    define_class!(
        // SAFETY: `NSObject` has no subclassing requirements, and
        // `Target` implements no `Drop`.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "DDIQuitTarget"]
        pub struct Target;

        impl Target {
            /// The app menu's Quit (⌘Q).
            #[unsafe(method(quit:))]
            fn quit(&self, _sender: Option<&AnyObject>) {
                ASKED.set(true);
            }

            /// The quit Apple Event (the Dock's Quit, logging out),
            /// answered at once: the shell quits from its loop.
            #[unsafe(method(quitEvent:withReplyEvent:))]
            fn quit_event(&self, _event: Option<&AnyObject>, _reply: Option<&AnyObject>) {
                ASKED.set(true);
            }
        }
    );

    impl Target {
        fn new(mtm: MainThreadMarker) -> Retained<Target> {
            let this = Target::alloc(mtm).set_ivars(());
            // SAFETY: `NSObject`'s `init` on a freshly allocated object.
            unsafe { msg_send![super(this), init] }
        }
    }

    pub fn route() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if TARGET.with_borrow(Option::is_some) {
            return;
        }
        let target = Target::new(mtm);
        if let Some(menu) = NSApplication::sharedApplication(mtm).mainMenu() {
            for i in 0..menu.numberOfItems() {
                let Some(submenu) = menu.itemAtIndex(i).and_then(|item| item.submenu()) else {
                    continue;
                };
                for j in 0..submenu.numberOfItems() {
                    if let Some(item) = submenu.itemAtIndex(j)
                        && item.action() == Some(sel!(terminate:))
                    {
                        // SAFETY: `target` answers `quit:`, taking the
                        // sender, and is kept for the app's life (`TARGET`).
                        unsafe {
                            item.setTarget(Some(&target));
                            item.setAction(Some(sel!(quit:)));
                        }
                    }
                }
            }
        }
        // In place of AppKit's handler, which calls `terminate:`.
        // SAFETY: `target` answers `quitEvent:withReplyEvent:`, taking the
        // event and its reply, and is kept for the app's life (`TARGET`);
        // the two codes are `FourCharCode`s.
        unsafe {
            let _: () = msg_send![
                &*NSAppleEventManager::sharedAppleEventManager(),
                setEventHandler: &*target,
                andSelector: sel!(quitEvent:withReplyEvent:),
                forEventClass: CORE_EVENT_CLASS,
                andEventID: QUIT_APPLICATION
            ];
        }
        TARGET.with_borrow_mut(|kept| *kept = Some(target));
    }
}
