//! The web shell's components, without a framework: each page is a
//! [`Component`] whose state changes by messages, handled one after
//! another in a microtask after they are sent (as Yew handled them, which
//! the shell used until every screen moved onto the canvas, step 6.4),
//! and whose few DOM elements it keeps itself under its root element
//! ([`Context::root`]), brought up to date by [`Component::view`] after
//! messages that ask for it.
//!
//! A component lives while its [`Mounted`] handle does: dropping it calls
//! [`Component::destroy`], drops the state, removes the root and lets the
//! messages still queued go.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::rc::{Rc, Weak};

use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

pub(crate) trait Component: Sized + 'static {
    type Message: 'static;
    type Properties: 'static;

    fn create(ctx: &Context<Self>) -> Self;

    /// Handles a message; whether the view must be brought up to date.
    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    /// Brings the component's DOM under [`Context::root`] up to date
    /// (after it is made, and after messages that ask for it).
    fn view(&self, _ctx: &Context<Self>) {}

    /// After a view; `first` after the first.
    fn rendered(&mut self, _ctx: &Context<Self>, _first: bool) {}

    fn destroy(&mut self, _ctx: &Context<Self>) {}
}

/// What a component is given: its properties, its scope, its root.
pub(crate) struct Context<C: Component> {
    props: C::Properties,
    link: Scope<C>,
    root: HtmlElement,
}

impl<C: Component> Context<C> {
    pub(crate) fn props(&self) -> &C::Properties {
        &self.props
    }

    pub(crate) fn link(&self) -> &Scope<C> {
        &self.link
    }

    /// The element the component's DOM goes in.
    pub(crate) fn root(&self) -> &HtmlElement {
        &self.root
    }
}

struct Shared<C: Component> {
    ctx: Context<C>,
    state: RefCell<Option<C>>,
    queue: RefCell<VecDeque<C::Message>>,
    scheduled: Cell<bool>,
}

/// A component's way to send itself messages (a weak handle: sending to a
/// component gone does nothing).
pub(crate) struct Scope<C: Component> {
    shared: Weak<Shared<C>>,
}

impl<C: Component> Clone for Scope<C> {
    fn clone(&self) -> Self {
        Scope {
            shared: self.shared.clone(),
        }
    }
}

impl<C: Component> Scope<C> {
    /// Queues `msg`; it is handled in a microtask (after the code sending
    /// it, still within the task: a key's or a click's handler's).
    pub(crate) fn send_message(&self, msg: impl Into<C::Message>) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        shared.queue.borrow_mut().push_back(msg.into());
        if !shared.scheduled.replace(true) {
            let weak = self.shared.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(shared) = weak.upgrade() {
                    run(&shared);
                }
            });
        }
    }

    pub(crate) fn send_message_batch(&self, msgs: Vec<C::Message>) {
        for m in msgs {
            self.send_message(m);
        }
    }

    /// Runs `future` and sends what it gives (unless the component is
    /// gone by then).
    pub(crate) fn send_future<F>(&self, future: F)
    where
        F: Future<Output = C::Message> + 'static,
    {
        let link = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let msg = future.await;
            link.send_message(msg);
        });
    }

    pub(crate) fn send_future_batch<F>(&self, future: F)
    where
        F: Future<Output = Vec<C::Message>> + 'static,
    {
        let link = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            link.send_message_batch(future.await);
        });
    }

    /// A callback sending `f`'s message.
    pub(crate) fn callback<T: 'static, M: Into<C::Message>>(
        &self,
        f: impl Fn(T) -> M + 'static,
    ) -> Callback<T> {
        let link = self.clone();
        Callback::from(move |t| link.send_message(f(t)))
    }
}

/// Handles the queued messages, then views once if any asked for it.
fn run<C: Component>(shared: &Rc<Shared<C>>) {
    let mut render = false;
    loop {
        let Some(msg) = shared.queue.borrow_mut().pop_front() else {
            break;
        };
        let Ok(mut state) = shared.state.try_borrow_mut() else {
            // Handled from inside its own update: put back, for the run
            // in progress.
            shared.queue.borrow_mut().push_front(msg);
            return;
        };
        let Some(c) = state.as_mut() else {
            shared.queue.borrow_mut().clear();
            break;
        };
        render |= c.update(&shared.ctx, msg);
    }
    shared.scheduled.set(false);
    if render {
        view(shared, false);
    }
}

fn view<C: Component>(shared: &Rc<Shared<C>>, first: bool) {
    if let Ok(state) = shared.state.try_borrow()
        && let Some(c) = state.as_ref()
    {
        c.view(&shared.ctx);
    } else {
        return;
    }
    if let Ok(mut state) = shared.state.try_borrow_mut()
        && let Some(c) = state.as_mut()
    {
        c.rendered(&shared.ctx, first);
    }
}

/// A component in the page: dropping it takes it down.
pub(crate) struct Mounted<C: Component> {
    shared: Rc<Shared<C>>,
}

impl<C: Component> Mounted<C> {
    pub(crate) fn scope(&self) -> Scope<C> {
        self.shared.ctx.link.clone()
    }
}

/// Makes a `C` with `props` in a new element `tag` at the end of
/// `parent`, and views it.
pub(crate) fn mount<C: Component>(parent: &Element, tag: &str, props: C::Properties) -> Mounted<C> {
    let root: HtmlElement = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.create_element(tag).ok())
        .and_then(|e| e.dyn_into().ok())
        .expect("an element");
    let _ = parent.append_child(&root);
    let shared = Rc::new_cyclic(|weak: &Weak<Shared<C>>| Shared {
        ctx: Context {
            props,
            link: Scope {
                shared: weak.clone(),
            },
            root,
        },
        state: RefCell::new(None),
        queue: RefCell::new(VecDeque::new()),
        scheduled: Cell::new(false),
    });
    let c = C::create(&shared.ctx);
    *shared.state.borrow_mut() = Some(c);
    view(&shared, true);
    Mounted { shared }
}

impl<C: Component> Drop for Mounted<C> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.try_borrow_mut()
            && let Some(mut c) = state.take()
        {
            c.destroy(&self.shared.ctx);
        }
        self.shared.queue.borrow_mut().clear();
        self.shared.ctx.root.remove();
    }
}

/// A function to call with a value, cheap to clone.
pub(crate) struct Callback<T> {
    f: Rc<dyn Fn(T)>,
}

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Callback { f: self.f.clone() }
    }
}

impl<T> PartialEq for Callback<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.f, &other.f)
    }
}

impl<T: 'static> Default for Callback<T> {
    fn default() -> Self {
        Callback::from(|_| {})
    }
}

impl<T, F: Fn(T) + 'static> From<F> for Callback<T> {
    fn from(f: F) -> Self {
        Callback { f: Rc::new(f) }
    }
}

impl<T> Callback<T> {
    pub(crate) fn emit(&self, value: T) {
        (self.f)(value)
    }
}

/// A new element `tag` with `class`, at the end of `parent`.
pub(crate) fn element(parent: &Element, tag: &str, class: &str) -> HtmlElement {
    let el: HtmlElement = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.create_element(tag).ok())
        .and_then(|e| e.dyn_into().ok())
        .expect("an element");
    if !class.is_empty() {
        el.set_class_name(class);
    }
    let _ = parent.append_child(&el);
    el
}

/// A handle on an element a component made or was given, read later
/// (`cast`), as the views' references were.
#[derive(Clone, Default)]
pub(crate) struct NodeRef(Rc<RefCell<Option<Element>>>);

impl PartialEq for NodeRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl NodeRef {
    pub(crate) fn set(&self, el: &Element) {
        *self.0.borrow_mut() = Some(el.clone());
    }

    pub(crate) fn get(&self) -> Option<Element> {
        self.0.borrow().clone()
    }

    pub(crate) fn cast<T: JsCast>(&self) -> Option<T> {
        self.get().and_then(|e| e.dyn_into().ok())
    }
}

/// `root` as the page shown when the browser gives no GPU: plain DOM.
pub(crate) fn no_gpu(root: &HtmlElement, reason: &str) {
    root.set_class_name("shell no-gpu");
    root.set_inner_html("");
    element(root, "h1", "").set_text_content(Some("No graphics"));
    element(root, "p", "").set_text_content(Some(
        "This browser gave the game no usable GPU (neither WebGPU nor WebGL2), so nothing can be drawn. Menus that are plain pages still work.",
    ));
    element(root, "p", "muted").set_text_content(Some(reason));
    let p = element(root, "p", "");
    let a = element(&p, "a", "");
    let _ = a.set_attribute("href", &crate::router::Route::Home.to_hash());
    a.set_text_content(Some("← home"));
}
