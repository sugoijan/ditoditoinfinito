//! Guided binding: press each arrow in turn on the device being bound.
//!
//! Works the same for the keyboard and any controller, whatever its layout.
//! Controllers without the standard mapping (dance pads, pad adapters, many
//! generic USB pads) number their buttons arbitrarily and may report arrows
//! on axes or on a hat switch, so the only reliable way to learn the layout
//! is to ask for each arrow. Each step records the first press edge of the
//! chosen device; the gamepad hub's edges already treat axes and hats at
//! rest as unpressed, so only a deliberate movement counts.
//!
//! Two rules keep stray input out: once a device is chosen, presses from
//! every other device are ignored (a drifting stick on another pad cannot
//! fill a step), and a recorded control must be released before the next
//! step listens (one press cannot fill two steps). Escape cancels from any
//! step, whichever device is being bound.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ddi_platform::gamepad::Control;
use ddi_platform::{DeviceId, HostTime, RawInput};
use gloo::events::{EventListener, EventListenerOptions};
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};
use yew::prelude::*;

use crate::settings::PadBindings;
use crate::web::gamepad::Gamepads;

/// Steps in order; the keyboard stops after the four arrows.
const STEPS: [&str; 6] = ["Left", "Down", "Up", "Right", "Start", "Back"];
const LANES: usize = 4;

/// What a finished flow binds; it replaces the device's earlier bindings.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Bound {
    /// One key per lane (`KeyboardEvent.code`).
    Keyboard(Vec<Vec<String>>),
    Pad(PadBindings),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// Device to bind; `None` asks for a press on it first.
    pub(crate) device: Option<DeviceId>,
    /// The page's hub; the flow takes its edge listener while open.
    pub(crate) gamepads: Option<Gamepads>,
    pub(crate) on_save: Callback<Bound>,
    pub(crate) on_cancel: Callback<()>,
}

pub(crate) enum Msg {
    Edge(RawInput),
    /// The page lost focus: keys released elsewhere send no keyup.
    Blur,
    Skip,
    Redo,
    Save,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Waiting for a press that picks the device.
    Choose,
    /// Waiting for the control of `STEPS[n]`.
    Step(usize),
    /// Everything recorded: live test, then save.
    Review,
}

/// Which keydowns the window listener swallows, kept in step with the
/// component state (the listener cannot read the component).
#[derive(Default)]
struct KeyPolicy {
    /// Every key (the keyboard is, or may become, the device being bound).
    all: bool,
    /// Only these codes (the new keys, during the live test).
    keys: Vec<String>,
}

pub(crate) struct BindFlow {
    device: Option<DeviceId>,
    phase: Phase,
    recorded: [Option<String>; 6],
    /// A control that must be released before the next step listens.
    waiting: Option<String>,
    message: Option<String>,
    /// Controls of the chosen device held right now.
    held: HashSet<String>,
    policy: Rc<RefCell<KeyPolicy>>,
    dialog: NodeRef,
    _keys: Vec<EventListener>,
}

impl Component for BindFlow {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let device = ctx.props().device.clone();
        let policy = Rc::new(RefCell::new(KeyPolicy::default()));
        if let Some(g) = &ctx.props().gamepads {
            let link = ctx.link().clone();
            g.set_listener(Some(Box::new(move |edge: &RawInput| {
                link.send_message(Msg::Edge(edge.clone()))
            })));
        }
        // A pad bound from its row may already hold something.
        let held = match (&device, &ctx.props().gamepads) {
            (Some(DeviceId::Gamepad(id)), Some(g)) => g
                .pads()
                .into_iter()
                .filter(|p| &p.id == id)
                .flat_map(|p| p.active)
                .map(|c| c.to_string())
                .collect(),
            _ => HashSet::new(),
        };
        let flow = BindFlow {
            phase: if device.is_some() {
                Phase::Step(0)
            } else {
                Phase::Choose
            },
            device,
            recorded: Default::default(),
            waiting: None,
            message: None,
            held,
            policy: policy.clone(),
            dialog: NodeRef::default(),
            _keys: key_listeners(ctx, policy),
        };
        flow.sync_policy();
        flow
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let redraw = match msg {
            Msg::Edge(edge) => self.edge(edge),
            Msg::Blur => {
                if self.device == Some(DeviceId::Keyboard) {
                    self.held.clear();
                    self.waiting = None;
                }
                true
            }
            Msg::Skip => {
                if let Phase::Step(step) = self.phase
                    && step >= LANES
                {
                    self.recorded[step] = None;
                    self.message = None;
                    self.advance();
                }
                true
            }
            Msg::Redo => {
                self.recorded = Default::default();
                self.message = None;
                self.phase = Phase::Step(0);
                true
            }
            Msg::Save => {
                if let Some(bound) = self.bound() {
                    ctx.props().on_save.emit(bound);
                }
                false
            }
            Msg::Cancel => {
                ctx.props().on_cancel.emit(());
                false
            }
        };
        self.sync_policy();
        redraw
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        // Focus the dialog so Enter or Space cannot click the button that
        // opened it, and Tab starts inside it.
        if first_render && let Some(el) = self.dialog.cast::<HtmlElement>() {
            let _ = el.focus();
        }
    }

    fn destroy(&mut self, ctx: &Context<Self>) {
        if let Some(g) = &ctx.props().gamepads {
            g.set_listener(None);
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let title = match &self.device {
            None => "Bind a device".to_string(),
            Some(DeviceId::Keyboard) => "Bind the keyboard".to_string(),
            Some(DeviceId::Gamepad(id)) => format!("Bind controller “{id}”"),
            Some(_) => "Bind a device".to_string(),
        };
        let body = match self.phase {
            Phase::Choose => html! {
                <>
                    <p class="bind-prompt">{ "Press any key or controller button on the device you want to bind" }</p>
                    <p class="muted">{ "Controllers appear once one of their buttons is pressed." }</p>
                </>
            },
            Phase::Step(step) => self.step_view(step),
            Phase::Review => self.review_view(),
        };
        let skip = matches!(self.phase, Phase::Step(s) if s >= LANES);
        let review = self.phase == Phase::Review;
        html! {
            <div class="bind-backdrop">
                <div class="bind-dialog" role="dialog" aria-modal="true" aria-labelledby="bind-title" tabindex="-1" ref={self.dialog.clone()}>
                    <h2 id="bind-title">{ title }</h2>
                    { body }
                    { for self.message.as_ref().map(|m| html! { <p class="bind-message">{ m }</p> }) }
                    <div class="bind-actions">
                        { if review { html! {
                            <>
                                <button class="primary" onclick={link.callback(|_| Msg::Save)}>{ "save" }</button>
                                <button onclick={link.callback(|_| Msg::Redo)}>{ "redo" }</button>
                            </>
                        } } else { html! {} } }
                        { if skip { html! {
                            <button onclick={link.callback(|_| Msg::Skip)}>{ "skip" }</button>
                        } } else { html! {} } }
                        <button onclick={link.callback(|_| Msg::Cancel)}>{ "cancel" }</button>
                        <span class="muted">{ "Esc cancels" }</span>
                    </div>
                </div>
            </div>
        }
    }
}

impl BindFlow {
    fn is_pad(&self) -> bool {
        matches!(self.device, Some(DeviceId::Gamepad(_)))
    }

    fn steps(&self) -> usize {
        if self.is_pad() { STEPS.len() } else { LANES }
    }

    fn label(&self, control: &str) -> String {
        if self.is_pad() {
            control
                .parse::<Control>()
                .map(|c| c.label())
                .unwrap_or_else(|_| control.to_string())
        } else {
            control.to_string()
        }
    }

    /// Handles one press or release; returns whether to redraw.
    fn edge(&mut self, edge: RawInput) -> bool {
        match &self.device {
            None => {
                // The first press picks the device; it still has to be
                // released before the first step listens.
                if !edge.pressed || self.phase != Phase::Choose {
                    return false;
                }
                self.device = Some(edge.device);
                self.held = HashSet::from([edge.control.clone()]);
                self.waiting = Some(edge.control);
                self.phase = Phase::Step(0);
                return true;
            }
            Some(d) if *d != edge.device => return false,
            Some(_) => {}
        }
        if !edge.pressed {
            self.held.remove(&edge.control);
            if self.waiting.as_ref() == Some(&edge.control) {
                self.waiting = None;
                self.message = None;
            }
            return true;
        }
        self.held.insert(edge.control.clone());
        let Phase::Step(step) = self.phase else {
            return true;
        };
        if let Some(w) = &self.waiting {
            self.message = Some(format!("Release {} first.", self.label(w)));
            return true;
        }
        if edge.device == DeviceId::Keyboard && edge.control == "Backspace" {
            // Play treats it as quit before any lane sees it.
            self.message = Some("Backspace leaves a song; press a different key.".into());
            return true;
        }
        if let Some(i) = self
            .recorded
            .iter()
            .position(|r| r.as_deref() == Some(edge.control.as_str()))
        {
            self.message = Some(format!(
                "{} is already {}. Press a different one.",
                self.label(&edge.control),
                STEPS[i]
            ));
            return true;
        }
        self.recorded[step] = Some(edge.control.clone());
        self.waiting = Some(edge.control);
        self.message = None;
        self.advance();
        true
    }

    fn advance(&mut self) {
        if let Phase::Step(step) = self.phase {
            self.phase = if step + 1 < self.steps() {
                Phase::Step(step + 1)
            } else {
                Phase::Review
            };
        }
    }

    fn sync_policy(&self) {
        let mut p = self.policy.borrow_mut();
        let keyboard = matches!(self.device, None | Some(DeviceId::Keyboard));
        p.all = keyboard && self.phase != Phase::Review;
        p.keys = if keyboard && self.phase == Phase::Review {
            self.recorded.iter().flatten().cloned().collect()
        } else {
            Vec::new()
        };
    }

    fn bound(&self) -> Option<Bound> {
        let lanes: Vec<Vec<String>> = self.recorded[..LANES]
            .iter()
            .map(|c| c.iter().cloned().collect())
            .collect();
        match &self.device {
            Some(DeviceId::Keyboard) => Some(Bound::Keyboard(lanes)),
            Some(DeviceId::Gamepad(id)) => Some(Bound::Pad(PadBindings {
                id: id.clone(),
                single: lanes,
                start: self.recorded[4].clone(),
                back: self.recorded[5].clone(),
                last_seen: 0.0,
            })),
            _ => None,
        }
    }

    fn step_view(&self, step: usize) -> Html {
        let what = match step {
            4 => "the button for Start (starts a song)".to_string(),
            5 => "the button for Back (leaves a song)".to_string(),
            _ => STEPS[step].to_string(),
        };
        let optional = |i: usize| if i >= LANES { " (optional)" } else { "" };
        html! {
            <>
                <p class="bind-prompt">{ format!("Press {what}") }</p>
                { for self.waiting.as_ref().map(|w| html! {
                    <p class="muted">{ format!("Waiting for {} to be released.", self.label(w)) }</p>
                }) }
                <ol class="bind-steps">
                    { for (0..self.steps()).map(|i| {
                        let class = classes!((i == step).then_some("current"), (i < step).then_some("done"));
                        let value = match (&self.recorded[i], i < step) {
                            (Some(c), _) => self.label(c),
                            (None, true) => "skipped".to_string(),
                            (None, false) => String::new(),
                        };
                        html! {
                            <li {class}>
                                <span>{ format!("{}{}", STEPS[i], optional(i)) }</span>
                                <span class="bind-value">{ value }</span>
                            </li>
                        }
                    }) }
                </ol>
            </>
        }
    }

    fn review_view(&self) -> Html {
        let cell = |i: usize| {
            let control = self.recorded[i].as_ref();
            let lit = control.is_some_and(|c| self.held.contains(c));
            html! {
                <div class={classes!("bind-lane", lit.then_some("lit"))}>
                    <strong>{ STEPS[i] }</strong>
                    <span>{ control.map(|c| self.label(c)).unwrap_or_else(|| "—".into()) }</span>
                </div>
            }
        };
        let controls: Vec<Option<Control>> = self.recorded[..LANES]
            .iter()
            .map(|c| c.as_ref().and_then(|c| c.parse().ok()))
            .collect();
        let mut warnings = Vec::new();
        for i in 0..LANES {
            for j in i + 1..LANES {
                if let (Some(a), Some(b)) = (&controls[i], &controls[j])
                    && a.exclusive_with(b)
                {
                    warnings.push(format!(
                        "{} and {} are opposite ends of one axis: they cannot be pressed together.",
                        STEPS[i], STEPS[j]
                    ));
                }
            }
        }
        html! {
            <>
                <p>{ "Press the arrows to test them: each lights while held." }</p>
                <div class="bind-lanes">{ for (0..LANES).map(cell) }</div>
                { if self.is_pad() { html! {
                    <div class="bind-lanes bind-extra">{ cell(4) }{ cell(5) }</div>
                } } else { html! {} } }
                { for warnings.iter().map(|w| html! { <p class="bind-warning">{ w }</p> }) }
                { if warnings.is_empty() { html! {} } else { html! {
                    <p class="muted">{ "Jumps on those arrows will not register. Some pads have a mode switch that reports the arrows as buttons instead." }</p>
                } } }
            </>
        }
    }
}

/// Window key listeners: keydown/keyup as edges of the keyboard, Escape as
/// cancel, focus loss as a release of everything.
fn key_listeners(ctx: &Context<BindFlow>, policy: Rc<RefCell<KeyPolicy>>) -> Vec<EventListener> {
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    let opts = EventListenerOptions::enable_prevent_default();
    let handler = |pressed: bool| {
        let link = ctx.link().clone();
        let policy = policy.clone();
        move |event: &web_sys::Event| {
            let Some(e) = event.dyn_ref::<KeyboardEvent>() else {
                return;
            };
            // Browser and system shortcuts (reload, devtools, tab switching)
            // stay usable while the flow listens; they are never bound.
            if e.ctrl_key() || e.meta_key() || e.alt_key() || is_function_key(&e.code()) {
                return;
            }
            let code = e.code();
            let swallow = {
                let p = policy.borrow();
                code == "Escape" || p.all || p.keys.contains(&code)
            };
            if swallow {
                e.prevent_default();
            }
            if e.repeat() {
                return;
            }
            if code == "Escape" {
                if pressed {
                    link.send_message(Msg::Cancel);
                }
                return;
            }
            link.send_message(Msg::Edge(RawInput {
                device: DeviceId::Keyboard,
                control: code,
                pressed,
                host_time: HostTime(0.0),
            }));
        }
    };
    let blur = {
        let link = ctx.link().clone();
        EventListener::new(&window, "blur", move |_| link.send_message(Msg::Blur))
    };
    vec![
        EventListener::new_with_options(&window, "keydown", opts, handler(true)),
        EventListener::new_with_options(&window, "keyup", opts, handler(false)),
        blur,
    ]
}

/// `F1`…`F24`.
fn is_function_key(code: &str) -> bool {
    code.strip_prefix('F')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}
