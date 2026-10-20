//! The app's notices, between the device watcher
//! (`components::device_watcher`, which decides them) and the canvas pages
//! that draw them (`ddi_ui::screens::notices::NoticeBar`, the UI's
//! chrome on every theme page: `components::ui_dev::start_ui`). A page
//! subscribes while it runs; an action pressed on its banner comes back
//! to the watcher.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_ui::screens::notices::{AppNotices, NoticesFeed};

type Listener = Rc<dyn Fn()>;
/// Does a banner's action (`later`, `defaults`, …).
type ActionFn = Rc<dyn Fn(&str)>;

#[derive(Default)]
struct Hub {
    now: AppNotices,
    listeners: Vec<(u64, Listener)>,
    next: u64,
    action: Option<ActionFn>,
}

thread_local! {
    static HUB: RefCell<Hub> = RefCell::new(Hub::default());
}

/// Shows `notices` on every page that draws them.
pub(crate) fn set(notices: AppNotices) {
    let listeners: Vec<Listener> = HUB.with(|h| {
        let mut h = h.borrow_mut();
        if h.now == notices {
            return Vec::new();
        }
        h.now = notices;
        h.listeners.iter().map(|(_, l)| l.clone()).collect()
    });
    for l in listeners {
        l();
    }
}

/// What is shown now.
pub(crate) fn now() -> AppNotices {
    HUB.with(|h| h.borrow().now.clone())
}

/// Who acts on a banner's buttons (the device watcher).
pub(crate) fn on_action(action: Option<ActionFn>) {
    HUB.with(|h| h.borrow_mut().action = action);
}

/// A banner's button `id` was pressed.
pub(crate) fn act(id: &str) {
    let action = HUB.with(|h| h.borrow().action.clone());
    if let Some(a) = action {
        a(id);
    }
}

/// Keeps `feed` current while the subscription lives; `changed` hears
/// each change (to tell the screen).
pub(crate) struct Subscription(u64);

impl Drop for Subscription {
    fn drop(&mut self) {
        HUB.with(|h| h.borrow_mut().listeners.retain(|(id, _)| *id != self.0));
    }
}

pub(crate) fn subscribe(feed: NoticesFeed, changed: Rc<dyn Fn()>) -> Subscription {
    *feed.borrow_mut() = now();
    HUB.with(|h| {
        let mut h = h.borrow_mut();
        let id = h.next;
        h.next += 1;
        h.listeners.push((
            id,
            Rc::new(move || {
                *feed.borrow_mut() = now();
                changed();
            }),
        ));
        Subscription(id)
    })
}
