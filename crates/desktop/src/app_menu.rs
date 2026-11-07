//! The app menu in the UI's language.
//!
//! On macOS winit makes the app menu (About, Services, Hide, Hide Others,
//! Show All, Quit) with English titles; [`follow_language`] names its
//! items as macOS names them in its own apps in the language the UI
//! speaks (`ddi_l10n::msg::shell`), and again whenever that changes (an
//! Options choice). Items are found by what they do, so the order winit
//! makes them in, and `crate::quit`'s pointing Quit at the shell, do not
//! matter. Elsewhere there is no app menu.

/// Names the app menu's items in the UI's language, when it is not
/// named in it already (cheap: called from every turn of the loop).
pub fn follow_language() {
    #[cfg(target_os = "macos")]
    imp::follow_language();
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::Cell;

    use ddi_l10n::msg::shell;
    use objc2::{MainThreadMarker, sel};
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    thread_local! {
        /// The language the menu is named in.
        static NAMED: Cell<Option<&'static str>> = const { Cell::new(None) };
    }

    pub fn follow_language() {
        let tag = ddi_l10n::current().tag;
        if NAMED.get() == Some(tag) {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        // None before the app has finished launching: named on a later turn.
        let Some(menu) = NSApplication::sharedApplication(mtm).mainMenu() else {
            return;
        };
        for i in 0..menu.numberOfItems() {
            let Some(submenu) = menu.itemAtIndex(i).and_then(|item| item.submenu()) else {
                continue;
            };
            for j in 0..submenu.numberOfItems() {
                let Some(item) = submenu.itemAtIndex(j) else {
                    continue;
                };
                // The one item with a submenu (whose action AppKit makes
                // `submenuAction:`) is Services.
                let title = match item.action() {
                    _ if item.submenu().is_some() => shell::menu_services(),
                    Some(a) if a == sel!(orderFrontStandardAboutPanel:) => shell::menu_about(),
                    Some(a) if a == sel!(hide:) => shell::menu_hide(),
                    Some(a) if a == sel!(hideOtherApplications:) => shell::menu_hide_others(),
                    Some(a) if a == sel!(unhideAllApplications:) => shell::menu_show_all(),
                    // `quit:` once `crate::quit` points it at the shell.
                    Some(a) if a == sel!(terminate:) || a == sel!(quit:) => shell::menu_quit(),
                    _ => continue,
                };
                item.setTitle(&NSString::from_str(&title));
            }
        }
        NAMED.set(Some(tag));
    }
}
