//! The start prompt (`docs/plans/ui-core.md`, step 6.4), one screen for
//! both shells: the song, its chart and the mods it will play with over
//! its picture, until the player starts or goes back; or what is still
//! loading, the linked folder the browser asks about again, the devices
//! not calibrated yet, an interrupted play or why the song cannot play.
//!
//! The shell says what to show through [`PromptState`] (shared, its
//! `version` bumped on every change: the screen builds itself again) and
//! hears what the player chose through a [`PromptFn`], called inside the
//! input's handler: on the web the key, click or tap that starts the song
//! is still the user gesture the audio context must be made in
//! (`AGENTS.md`), and asking for a linked folder needs one too
//! ([`Cx::in_gesture`] says whether this one is: a pad's press is not in
//! WebKit).
//!
//! The picture is drawn by the screen itself through the 2D pass, as the
//! play draws it before the first change ([`PromptPicture`]): cover-fitted
//! into the background frame, dimmed by the backgrounds' brightness, with
//! a blurred, dimmer copy over the rest of the screen and the field's
//! filter where the lanes will be. Drawn by the UI and not by the play's
//! renderer, the prompt is the same on both shells and on every surface
//! (the 2D pass cannot draw over the play's sRGB surface on WebGL2), and
//! the transition from song select's blurred backdrop (`docs/plans/
//! theme.md`) can later sharpen the one into the other: the blurred copy
//! is song select's backdrop's way of drawing ([`super::Backdrop`]).
//!
//! Components: `Dialog` (the card, `Panel`'s fallback), `PlayButton` and
//! `Button`; colours `Play.Field` (the play's own, under the picture),
//! `Text`, `Muted`, `Faint`, `Accent`, `Danger`,
//! `Difficulty.<name>`; metric `Prompt.Width` (the card's), and
//! `Backdrop.Side` and `Backdrop.Spread` for the blurred copy.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind, Sprite};

use super::select_dialogs::{wrap, wrap_lines};
use super::{Env, footer, frame, left};
use crate::gesture::Gesture;
use crate::look::{self, Spec, call, named, quad};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};

/// What the prompt names: the song and its chart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptInfo {
    pub title: String,
    /// The artist (or a calibration's hint).
    pub artist: String,
    /// "Hard 13 · Single" ([`chart_line`]); empty for none.
    pub chart: String,
    /// The difficulty's name for its colour (`Difficulty.Hard`); empty:
    /// the text's.
    pub difficulty: String,
    /// The mods it will be played with ([`mods_line`]); empty for none.
    pub mods: String,
    /// "AUTOPLAY" when autoplayed, else empty.
    pub badge: String,
}

/// The song's picture behind the prompt, as the play shows it before the
/// first change.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptPicture {
    /// The image's texture source (the shell's: a file it keeps).
    pub source: String,
    /// Its width over its height.
    pub aspect: f32,
    /// The background frame's shape (width over height), `None`: the
    /// whole screen.
    pub frame: Option<f32>,
    /// The backgrounds' perceived brightness, `0..=1`.
    pub brightness: f32,
    /// The field's filter: its band's middle and width (fractions of the
    /// screen's width) and how dark, `0..=1` of the brightness removed.
    pub filter: Option<([f32; 2], f32)>,
}

/// The play's blurred copy around the frame is this bright against the
/// frame (`ddi_render::background`'s `WIDE_DIM`).
const EXTENSION_DIM: f32 = 0.6;

/// What the prompt is at.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum PromptStep {
    /// Something still loads ("loading…", "preparing audio…"); Back
    /// leaves.
    Loading(String),
    /// Start plays.
    #[default]
    Ready,
    /// A linked song's folder needs the browser's permission again
    /// (asked from a gesture: [`PromptChoice::Allow`]); why the last ask
    /// failed.
    Access {
        folder: String,
        refused: Option<String>,
    },
    /// The audio path or the display has no offsets yet
    /// ([`new_device_step`]): its lines, and its actions (`calibrate`,
    /// `defaults`, `copy-audio:<id>`, `copy-display:<id>`, as
    /// [`super::notices::register_devices`] reads them) with their
    /// labels.
    NewDevice {
        lines: Vec<String>,
        actions: Vec<(String, String)>,
    },
    /// The play was cut off (the tab hidden, the audio suspended).
    Interrupted,
    /// What came of it (a calibration's outcome): a heading, lines with
    /// their colours' names, and actions (id, label), the first focused.
    Card {
        heading: String,
        lines: Vec<(String, String)>,
        actions: Vec<(String, String)>,
    },
    /// The song cannot play, and why.
    Error(String),
}

/// What the player chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptChoice {
    /// Start the song (also while loading: a press meant to start it).
    Start,
    Back,
    /// Ask for the linked folder.
    Allow,
    /// After an interruption.
    Retry,
    /// A new device's action, by its id.
    Device(String),
    /// A card's action, by its id.
    Action(String),
}

/// What the shell shows; `version` goes up with every change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptState {
    /// The song (its name may come after the prompt is up).
    pub info: PromptInfo,
    pub step: PromptStep,
    /// Lines under the song (the video decoder's download, "this browser
    /// needs a tap…").
    pub notes: Vec<String>,
    /// A touch screen: taps, not keys, and the ✕ button to quit.
    pub touch: bool,
    pub picture: Option<PromptPicture>,
    pub version: u64,
}

impl PromptState {
    pub fn set_info(&mut self, info: PromptInfo) {
        if self.info != info {
            self.info = info;
            self.version += 1;
        }
    }

    pub fn set_step(&mut self, step: PromptStep) {
        if self.step != step {
            self.step = step;
            self.version += 1;
        }
    }

    pub fn set_notes(&mut self, notes: Vec<String>) {
        if self.notes != notes {
            self.notes = notes;
            self.version += 1;
        }
    }

    pub fn set_touch(&mut self, touch: bool) {
        if self.touch != touch {
            self.touch = touch;
            self.version += 1;
        }
    }

    pub fn set_picture(&mut self, picture: Option<PromptPicture>) {
        if self.picture != picture {
            self.picture = picture;
            self.version += 1;
        }
    }
}

pub type SharedPrompt = Rc<RefCell<PromptState>>;

/// The shell's answer to a choice, inside the input's handler (the
/// gesture, when it is one: [`Cx::in_gesture`]). It may close the screen
/// ([`Cx::close`]) or leave it to show what comes next.
pub type PromptFn = Rc<dyn Fn(PromptChoice, &mut Cx)>;

/// "Hard 13 · Single": a chart's difficulty, meter and style.
pub fn chart_line(difficulty: ddi_chart::Difficulty, meter: u32, style: &str) -> String {
    format!(
        "{} {meter} · {style}",
        super::data::difficulty_name(difficulty)
    )
}

/// The mods a play will run with, as the settings have them: the speed,
/// Reverse, then every option off its default ([`super::play::mods_summary`]).
pub fn mods_line(settings: &crate::state::settings::Settings) -> String {
    let speed = format!("×{}", (settings.speed * 100.0).round() / 100.0);
    let mut parts = vec![speed];
    if settings.reverse {
        parts.push("reverse".into());
    }
    parts.extend(super::play::mods_summary(
        &settings.transform,
        settings.appearance,
        settings.scroll_action,
        None,
    ));
    parts.join(" · ")
}

/// The new-device step for devices `profile`, as the web's card had it:
/// a line for each, new or known, an offer of each calibrated device's
/// offset for a new one, then calibrating now or playing with the
/// defaults.
pub fn new_device_step(
    settings: &crate::state::settings::Settings,
    profile: &ddi_platform::DeviceProfile,
    new_audio: bool,
    new_display: bool,
) -> PromptStep {
    let mut lines = vec![
        "Offsets are kept per audio path and per display. These have not been calibrated yet:"
            .to_string(),
    ];
    // One offer a kind (the calibrated device seen last); both new and
    // both offered: one action for the two, so neither is lost.
    let mut offers: Vec<(String, String)> = Vec::new();
    for (what, kind, fp, new, list) in [
        (
            "audio path",
            "audio",
            &profile.audio,
            new_audio,
            &settings.audio_profiles,
        ),
        (
            "display",
            "display",
            &profile.display,
            new_display,
            &settings.display_profiles,
        ),
    ] {
        if !new {
            lines.push(format!("{}: {} (known)", capital(what), fp.label));
            continue;
        }
        lines.push(format!("New {what}: {}", fp.label));
        let best = list
            .iter()
            .filter(|p| p.calibrated)
            .max_by(|a, b| a.last_seen.total_cmp(&b.last_seen));
        if let Some(p) = best {
            offers.push((
                format!("copy-{kind}:{}", p.id),
                format!("{:+.0} ms from {}", p.offset * 1000.0, p.label),
            ));
        }
    }
    let mut actions = Vec::new();
    if !offers.is_empty() {
        let id = offers
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>()
            .join("+");
        let label = offers
            .iter()
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join(" and ");
        actions.push((id, format!("Use {label}")));
    }
    actions.push(("calibrate".into(), "Calibrate now".into()));
    actions.push(("defaults".into(), "Play with defaults".into()));
    PromptStep::NewDevice { lines, actions }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The prompt's screen.
pub struct PromptScreen {
    env: Env,
    state: SharedPrompt,
    act: PromptFn,
    /// The state's version built.
    built: u64,
    /// The kind of step built.
    built_step: Option<std::mem::Discriminant<PromptStep>>,
    screen: [f32; 2],
    /// The viewport it was built for, and the zoom onto it (a phone
    /// upright: laid out 480 wide, zoomed to the width).
    vp: [f32; 2],
    zoom: f32,
    /// The buttons of the step built, the one focused first first.
    buttons: Vec<String>,
    /// Over song select (desktop): counted in [`super::ThemeState::over`].
    over: bool,
    /// The fade in started (at the first frame).
    faded: bool,
}

/// The card's text sizes.
const TITLE: f32 = 28.0;
const BODY: f32 = 15.0;
const SMALL: f32 = 13.0;

impl PromptScreen {
    pub fn new(env: Env, state: SharedPrompt, act: PromptFn) -> PromptScreen {
        PromptScreen {
            env,
            state,
            act,
            built: u64::MAX,
            built_step: None,
            screen: super::DESIGN,
            vp: [0.0, 0.0],
            zoom: 1.0,
            buttons: Vec::new(),
            over: false,
            faded: false,
        }
    }

    /// Shown over song select, as its other screens are (it says what it
    /// does again once none is).
    pub fn over_select(mut self) -> PromptScreen {
        self.over = true;
        self
    }

    fn choose(&self, choice: PromptChoice, cx: &mut Cx) {
        (self.act)(choice, cx);
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.vp = [vp.width, vp.height];
        // A phone upright: 480 wide (as options, results and song select
        // are), zoomed to the width; else the screen as it is.
        let narrow = self.env.metric("Options.Narrow.Width", 480.0);
        (self.screen, self.zoom) = if super::is_compact(&*self.env.look, vp) && vp.height > vp.width
        {
            let z = (vp.width / narrow).max(1.0);
            ([vp.width / z, vp.height / z], z)
        } else {
            ([vp.width, vp.height], 1.0)
        };
        let state = self.state.borrow().clone();
        self.built = state.version;
        self.built_step = Some(std::mem::discriminant(&state.step));
        let env = self.env.clone();
        let screen = self.screen;
        // The play's own field colour (`Play.Field`): a song without a
        // picture opens on it.
        let mut field = quad(
            "Ground",
            [screen[0] + 8.0, screen[1] + 8.0],
            env.color("Play.Field"),
        );
        call(&mut field, "xy", &[screen[0] / 2.0, screen[1] / 2.0]);
        let mut children = vec![field];
        if let Some(p) = &state.picture {
            children.push(picture(&env, screen, p, cx));
        }
        let (card, buttons) = self.card(&state);
        self.buttons = buttons;
        children.push(card);
        let keys = !state.touch && matches!(state.step, PromptStep::Ready | PromptStep::Loading(_));
        if keys {
            let hints: &[(&[&str], &str)] = if state.step == PromptStep::Ready {
                &[(&["ENTER"], "Start"), (&["ESC"], "Back")]
            } else {
                &[(&["ESC"], "Back")]
            };
            let (f, texts) = footer(&env, screen, hints, "", &[]);
            env.state.borrow_mut().hints = texts;
            children.push(f);
        } else {
            env.state.borrow_mut().hints = Vec::new();
        }
        let mut root = Actor::new(Kind::Frame(children));
        if self.zoom != 1.0 {
            call(&mut root, "zoom", &[self.zoom]);
        }
        root
    }

    /// The card in the middle, and its buttons (the first focused).
    fn card(&self, state: &PromptState) -> (Actor, Vec<String>) {
        let env = &self.env;
        let [sw, sh] = self.screen;
        // A phone sideways: wider, so fewer lines and buttons lie on it.
        let w = if sh < 480.0 {
            (sw - 32.0).min(780.0)
        } else {
            env.metric("Prompt.Width", 560.0).min(sw - 32.0)
        };
        let inner = w - 48.0;
        let mut c: Vec<Actor> = Vec::new();
        let mut y = 24.0;
        let line = |c: &mut Vec<Actor>,
                    name: &str,
                    text: &str,
                    size: f32,
                    weight: u16,
                    color: &str,
                    y: &mut f32| {
            for (i, l) in wrap_lines(text, inner, size).into_iter().enumerate() {
                let mut t = env.text(&format!("{name}{i}"), &l, size, weight, color);
                wrap(&mut t, inner);
                *y += size * 0.65;
                c.push(left(t, 24.0, *y));
                *y += size * 0.65;
            }
        };
        let i = &state.info;
        // The song, on every step (once its name is known; asking for its
        // folder before it is read, the folder's name).
        let title = match &state.step {
            PromptStep::Access { folder, .. } if i.title.is_empty() => folder.clone(),
            _ => i.title.clone(),
        };
        if !title.is_empty() {
            let mut title = env.text("SongTitle", &title, TITLE, 700, "Text");
            wrap(&mut title, inner);
            y += TITLE * 0.6;
            c.push(left(title, 24.0, y));
            y += TITLE * 0.6 + 4.0;
        }
        if !i.artist.is_empty() {
            line(&mut c, "Artist", &i.artist, BODY, 400, "Muted", &mut y);
        }
        if !i.chart.is_empty() {
            y += 6.0;
            let color = if i.difficulty.is_empty() {
                "Text".to_string()
            } else {
                format!("Difficulty.{}", i.difficulty)
            };
            line(&mut c, "Chart", &i.chart, BODY, 700, &color, &mut y);
        }
        if !i.badge.is_empty() {
            line(&mut c, "Badge", &i.badge, SMALL, 700, "Accent", &mut y);
        }
        if !i.mods.is_empty() {
            line(&mut c, "Mods", &i.mods, SMALL, 400, "Muted", &mut y);
        }
        y += 14.0;
        // The step.
        let mut buttons: Vec<(String, String, &str)> = Vec::new();
        match &state.step {
            PromptStep::Loading(what) => {
                line(&mut c, "Status", what, BODY + 3.0, 700, "Text", &mut y);
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::Ready => {
                let status = if state.touch {
                    "Tap to start"
                } else {
                    "Press Enter, or Start on a controller, or click to start"
                };
                line(&mut c, "Status", status, BODY + 3.0, 700, "Text", &mut y);
                buttons.push(("Start".into(), "Start".into(), "PlayButton"));
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::Access { folder, refused } => {
                let text = format!(
                    "This song plays from the folder “{folder}”, which the browser asks about again."
                );
                line(&mut c, "Status", &text, BODY, 400, "Secondary", &mut y);
                if let Some(r) = refused {
                    line(&mut c, "Refused", r, BODY, 600, "Danger", &mut y);
                }
                buttons.push((
                    "Allow".into(),
                    "Allow reading the folder".into(),
                    "PlayButton",
                ));
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::NewDevice { lines, actions } => {
                line(
                    &mut c,
                    "Status",
                    "New device",
                    BODY + 3.0,
                    700,
                    "Text",
                    &mut y,
                );
                for (k, l) in lines.iter().enumerate() {
                    let color = if k == 0 { "Muted" } else { "Secondary" };
                    line(&mut c, &format!("Device{k}."), l, BODY, 400, color, &mut y);
                }
                // Playing on is focused first (a pad's Start plays on),
                // then the rest as listed (the audio's offer above the
                // display's).
                let order = actions
                    .iter()
                    .filter(|(id, _)| id == "defaults")
                    .chain(actions.iter().filter(|(id, _)| id != "defaults"));
                for (id, label) in order {
                    let kind = if id == "defaults" {
                        "PlayButton"
                    } else {
                        "Button"
                    };
                    buttons.push((format!("Device.{id}"), label.clone(), kind));
                }
            }
            PromptStep::Interrupted => {
                line(
                    &mut c,
                    "Status",
                    "Interrupted",
                    BODY + 3.0,
                    700,
                    "Text",
                    &mut y,
                );
                line(
                    &mut c,
                    "Why",
                    "The tab was hidden or audio was suspended.",
                    BODY,
                    400,
                    "Muted",
                    &mut y,
                );
                buttons.push(("Retry".into(), "Retry".into(), "PlayButton"));
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::Error(e) => {
                line(&mut c, "Status", e, BODY, 600, "Danger", &mut y);
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::Card {
                heading,
                lines,
                actions,
            } => {
                line(&mut c, "Status", heading, BODY + 3.0, 700, "Text", &mut y);
                for (k, (l, color)) in lines.iter().enumerate() {
                    line(&mut c, &format!("Line{k}."), l, BODY, 400, color, &mut y);
                }
                for (k, (id, label)) in actions.iter().enumerate() {
                    let kind = if k == 0 { "PlayButton" } else { "Button" };
                    buttons.push((format!("Action.{id}"), label.clone(), kind));
                }
            }
        }
        for (k, n) in state.notes.iter().enumerate() {
            line(&mut c, &format!("Note{k}."), n, SMALL, 400, "Muted", &mut y);
        }
        if matches!(state.step, PromptStep::Ready) {
            let quit = if state.touch {
                "During play, hold or double-tap the ✕ button to quit."
            } else {
                "During play, hold or double-tap Esc (or Back) to quit."
            };
            line(&mut c, "Quit", quit, SMALL, 400, "Faint", &mut y);
        }
        y += 18.0;
        // Taller on a phone upright (a thumb's).
        let bh = if self.zoom > 1.0 { 48.0 } else { 40.0 };
        // The buttons: in one row from the right when they fit, else one
        // a row (a new device's offers).
        let widths: Vec<f32> = buttons
            .iter()
            .map(|(_, l, kind)| {
                // The play button's label is larger (the theme's 18).
                let size = if *kind == "PlayButton" { 18.0 } else { 15.0 };
                // Its letter spacing too (the play button's is 0.08 em).
                let spacing = if *kind == "PlayButton" {
                    l.chars().count() as f32 * 0.08 * size
                } else {
                    0.0
                };
                (look::mono_width(l, size) + spacing + 36.0).max(104.0)
            })
            .collect();
        let one_row = widths.iter().sum::<f32>() + 10.0 * (buttons.len() as f32 - 1.0) <= inner;
        if one_row {
            // The first (focused) rightmost, as results put Retry.
            let mut x = w - 24.0;
            for ((name, label, kind), bw) in buttons.iter().zip(&widths) {
                c.push(pbutton(
                    env,
                    bh,
                    name,
                    label,
                    [x - bw / 2.0, y + bh / 2.0],
                    *bw,
                    kind,
                ));
                x -= bw + 10.0;
            }
            y += bh;
        } else {
            // In rows: the short ones two a row (the first, focused, on
            // top), then the long ones one a row.
            let half = (inner - 10.0) / 2.0;
            let (short, long): (Vec<usize>, Vec<usize>) =
                (0..buttons.len()).partition(|&k| widths[k] <= half);
            let mut rows: Vec<Vec<usize>> = short.chunks(2).map(<[usize]>::to_vec).collect();
            rows.extend(long.into_iter().map(|k| vec![k]));
            for row in rows {
                if let [a, b] = row[..] {
                    for (j, x) in [(a, 24.0 + half / 2.0), (b, w - 24.0 - half / 2.0)] {
                        let (name, label, kind) = &buttons[j];
                        c.push(pbutton(env, bh, name, label, [x, y + bh / 2.0], half, kind));
                    }
                } else {
                    let (name, label, kind) = &buttons[row[0]];
                    let bw = if widths[row[0]] <= half { half } else { inner };
                    let x = if bw < inner {
                        24.0 + half / 2.0
                    } else {
                        w / 2.0
                    };
                    c.push(pbutton(env, bh, name, label, [x, y + bh / 2.0], bw, kind));
                }
                y += bh + 10.0;
            }
            y -= 10.0;
        }
        let h = y + 24.0;
        let mut children = vec![env.component(
            "Dialog",
            "Panel",
            &Spec::new([w, h], ""),
            "Card",
            [0.0, 0.0],
        )];
        children.extend(c);
        let mut card = frame("Prompt", children);
        // Too tall for the screen (a phone sideways, many devices):
        // smaller.
        let room = sh - 16.0 - if state.touch { 0.0 } else { 40.0 };
        let zoom = (room / h).min(1.0);
        call(&mut card, "zoom", &[zoom]);
        let top = ((sh - if state.touch { 0.0 } else { 40.0 }) - h * zoom) / 2.0;
        call(&mut card, "xy", &[(sw - w * zoom) / 2.0, top.max(8.0)]);
        (card, buttons.into_iter().map(|(n, _, _)| n).collect())
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx) {
        let focused = cx.focused().map(str::to_string);
        let before = std::mem::discriminant(&self.state.borrow().step);
        let step = self.built_step;
        *root = self.tree(cx);
        root.finish_tweening();
        // The same step (a resize, a line changed): the focus stays; a new
        // one focuses its own first button.
        let keep = focused.filter(|f| step == Some(before) && self.buttons.contains(f));
        if let Some(f) = keep.or_else(|| self.buttons.first().cloned()) {
            cx.focus(f);
        }
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "prompt".into();
        let info = self.state.borrow().info.clone();
        st.song = info.title.clone();
        st.chart = (!info.chart.is_empty()).then_some(info.chart);
        st.status = match &self.state.borrow().step {
            PromptStep::Loading(s) => s.clone(),
            PromptStep::Ready => "ready".into(),
            PromptStep::Access { .. } => "access".into(),
            PromptStep::NewDevice { .. } => "new device".into(),
            PromptStep::Interrupted => "interrupted".into(),
            PromptStep::Error(e) => format!("error: {e}"),
            PromptStep::Card { heading, .. } => format!("card: {heading}"),
        };
        st.backdrop = self
            .state
            .borrow()
            .picture
            .as_ref()
            .map(|p| p.source.clone());
        st.page_items = self.buttons.clone();
    }
}

impl Drop for PromptScreen {
    /// Gone (closed by the shell, or the stack let go): song select says
    /// what it does again once nothing is over it.
    fn drop(&mut self) {
        if self.over && self.built != u64::MAX {
            let mut st = self.env.state.borrow_mut();
            st.over = st.over.saturating_sub(1);
        }
    }
}

/// A button `h` high (the dialogs' are 40).
fn pbutton(
    env: &Env,
    h: f32,
    name: &str,
    label: &str,
    centre: [f32; 2],
    w: f32,
    kind: &str,
) -> Actor {
    let mut b = look::component_or(&*env.look, kind, "Button", &Spec::new([w, h], label));
    b.name = name.into();
    call(&mut b, "xy", &centre);
    b
}

/// The song's picture: the blurred copy over the screen, the picture in
/// its frame, the field's filter.
fn picture(env: &Env, [w, h]: [f32; 2], p: &PromptPicture, cx: &mut Cx) -> Actor {
    let b = p.brightness.clamp(0.0, 1.0);
    let aspect = if p.aspect.is_finite() && p.aspect > 0.0 {
        p.aspect
    } else {
        w / h
    };
    // The frame: centred, as large as fits.
    let [fx, fy, fw, fh] = match p.frame.filter(|a| a.is_finite() && *a > 0.0) {
        Some(a) if w / h > a => [(w - h * a) / 2.0, 0.0, h * a, h],
        Some(a) => [0.0, (h - w / a) / 2.0, w, w / a],
        None => [0.0, 0.0, w, h],
    };
    let cover = |tw: f32, th: f32| -> [f32; 2] {
        if aspect > tw / th {
            [th * aspect, th]
        } else {
            [tw, tw / aspect]
        }
    };
    let mut children = Vec::new();
    let framed = fw < w - 1.0 || fh < h - 1.0;
    if framed {
        // Blurred: the image asked for tiny and drawn large, several
        // times a little apart (song select's backdrop's way).
        let side = env.metric("Backdrop.Side", 64.0);
        let spread = env.metric("Backdrop.Spread", 14.0);
        let tex = cx.texture(&p.source, side);
        let size = cover(w + 4.0 * spread, h + 4.0 * spread);
        let copies: [[f32; 2]; 5] = [
            [0.0, 0.0],
            [-1.0, -0.6],
            [1.0, -0.6],
            [-0.6, 1.0],
            [0.6, 1.0],
        ];
        let dim = b * EXTENSION_DIM;
        for (i, [dx, dy]) in copies.iter().enumerate() {
            let mut s = named(Kind::Sprite(Sprite::quad(size)), &format!("Wide{i}"));
            if let Kind::Sprite(sp) = &mut s.kind {
                sp.texture = tex;
            }
            call(
                &mut s,
                "xy",
                &[w / 2.0 + dx * spread, h / 2.0 + dy * spread],
            );
            // Each over the last: the first opaque, the rest blending in,
            // so all of them come to their average.
            let alpha = 1.0 / (i as f32 + 1.0);
            call(&mut s, "diffuse", &[dim, dim, dim, alpha]);
            children.push(s);
        }
    }
    let side = fw.max(fh);
    let tex = cx.texture(&p.source, side);
    let size = cover(fw, fh);
    let mut s = named(Kind::Sprite(Sprite::quad(size)), "Picture");
    if let Kind::Sprite(sp) = &mut s.kind {
        sp.texture = tex;
    }
    call(&mut s, "xy", &[fw / 2.0, fh / 2.0]);
    call(&mut s, "diffuse", &[b, b, b, 1.0]);
    let mut clip = frame("Frame", vec![s]);
    clip.clip = Some([0.0, 0.0, fw, fh]);
    call(&mut clip, "xy", &[fx, fy]);
    children.push(clip);
    if let Some(([mid, width], dark)) = p.filter
        && dark > 0.0
    {
        let mut f = quad(
            "Filter",
            [width * w, h + 2.0],
            [0.0, 0.0, 0.0, dark.clamp(0.0, 1.0)],
        );
        call(&mut f, "xy", &[mid * w, h / 2.0]);
        children.push(f);
    }
    frame("Backdrop", children)
}

impl Screen for PromptScreen {
    fn name(&self) -> &str {
        "play-prompt"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        if self.over {
            self.env.state.borrow_mut().over += 1;
        }
        let root = self.tree(cx);
        self.publish();
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        self.buttons.iter().map(Focusable::new).collect()
    }

    /// Frames while something loads (the shell's lines change), so they
    /// are read.
    fn animating(&self) -> bool {
        // Changed by the shell since built (a picture, a line, a step):
        // a frame builds it, whatever the step.
        let st = self.state.borrow();
        matches!(st.step, PromptStep::Loading(_)) || st.version != self.built
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        // Fades in from the first frame, taking keys at once: built into
        // the tree, the fade would be the screen's entrance, which holds
        // input back (a quick Enter or Escape was lost). None under
        // reduced motion.
        if !std::mem::replace(&mut self.faded, true) && !self.env.reduced() {
            let t = self.env.metric("Motion.Screen", 0.2);
            look::run(root, &format!("diffusealpha,0;linear,{t};diffusealpha,1"));
        }
        let vp = cx.viewport().screen;
        let resized = [vp.width, vp.height] != self.vp;
        if resized || self.state.borrow().version != self.built {
            self.rebuild(root, cx);
            self.publish();
        }
        if self.env.state.borrow().screen != "prompt" {
            self.publish();
        }
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        let step = self.state.borrow().step.clone();
        match input {
            Input::Menu { repeat: true, .. } => Handled::Yes,
            Input::Menu {
                action: MenuAction::Back,
                ..
            } => {
                self.choose(PromptChoice::Back, cx);
                Handled::Yes
            }
            // A press while the audio readies: the shell may need it (a
            // pad's start the browser did not count). Ready, Start starts
            // whatever has the focus (the step may have turned Ready since
            // the last frame, Back still focused).
            Input::Menu {
                action: MenuAction::Start,
                ..
            } if matches!(step, PromptStep::Loading(_) | PromptStep::Ready) => {
                self.choose(PromptChoice::Start, cx);
                Handled::Yes
            }
            Input::Activate { name } => {
                let choice = match name.as_str() {
                    "Start" => Some(PromptChoice::Start),
                    "Back" => Some(PromptChoice::Back),
                    "Allow" => Some(PromptChoice::Allow),
                    "Retry" => Some(PromptChoice::Retry),
                    n => n
                        .strip_prefix("Device.")
                        .map(|id| PromptChoice::Device(id.to_string()))
                        .or_else(|| {
                            n.strip_prefix("Action.")
                                .map(|id| PromptChoice::Action(id.to_string()))
                        }),
                };
                match choice {
                    Some(c) => {
                        self.choose(c, cx);
                        Handled::Yes
                    }
                    None => Handled::No,
                }
            }
            // A tap anywhere but a button starts (the web's prompt was one
            // big button), or is the press meant to start while loading.
            Input::Pointer {
                target,
                gesture: Gesture::Tap,
                ..
            } if matches!(step, PromptStep::Ready | PromptStep::Loading(_))
                && !target
                    .as_ref()
                    .is_some_and(|t| self.buttons.iter().any(|b| t.within(b))) =>
            {
                self.choose(PromptChoice::Start, cx);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{MenuEvent, UiEvent};
    use crate::testing::Harness;
    use crate::{PointerKind, TextMode};
    use ddi_platform::{DeviceFingerprint, DeviceProfile};

    type Seen = Rc<RefCell<Vec<(PromptChoice, bool)>>>;

    fn prompt(state: &SharedPrompt) -> (Harness, Seen) {
        let env = Env::new(Rc::new(look::Plain), Rc::new(|s: &str| s.to_string()));
        let seen: Seen = Rc::default();
        let act: PromptFn = {
            let seen = seen.clone();
            Rc::new(move |c, cx: &mut Cx| seen.borrow_mut().push((c, cx.in_gesture())))
        };
        let info = PromptInfo {
            title: "Some Things Must".into(),
            artist: "Sevish".into(),
            chart: chart_line(ddi_chart::Difficulty::Hard, 13, "Single"),
            difficulty: "Hard".into(),
            mods: "×3 · reverse".into(),
            badge: String::new(),
        };
        let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        state.borrow_mut().set_info(info);
        h.push(PromptScreen::new(env, state.clone(), act));
        h.run(0.3);
        (h, seen)
    }

    #[test]
    fn start_and_back_reach_the_shell_inside_the_gesture() {
        let state = SharedPrompt::default();
        let (mut h, seen) = prompt(&state);
        assert!(h.texts().iter().any(|t| t == "Hard 13 · Single"));
        h.key("Enter", "Enter");
        // A pad's Start the browser does not count as a gesture.
        h.send(UiEvent::Menu(MenuEvent {
            action: MenuAction::Start,
            down: true,
            source: 0,
            gesture: false,
            time: h.now,
        }));
        // A tap anywhere but a button starts; the Back button goes back.
        h.tap_at(PointerKind::Touch, [100.0, 100.0]);
        h.tap(PointerKind::Mouse, "Back");
        h.key("Escape", "Escape");
        let got = seen.borrow().clone();
        assert_eq!(
            got[..4],
            [
                (PromptChoice::Start, true),
                (PromptChoice::Start, false),
                (PromptChoice::Start, true),
                (PromptChoice::Back, true),
            ]
        );
        // Escape too (going back needs no gesture).
        assert_eq!(got[4].0, PromptChoice::Back);
    }

    #[test]
    fn the_shells_steps_show_as_they_change() {
        let state = SharedPrompt::default();
        state
            .borrow_mut()
            .set_step(PromptStep::Loading("preparing audio…".into()));
        let (mut h, seen) = prompt(&state);
        assert!(h.texts().iter().any(|t| t == "preparing audio…"));
        assert!(h.ui.actor("Start").is_none());
        // A press while it loads still reaches the shell.
        h.key("Enter", "Enter");
        assert_eq!(seen.borrow().last(), Some(&(PromptChoice::Start, true)));
        state.borrow_mut().set_step(PromptStep::Access {
            folder: "Pack".into(),
            refused: Some("Reading the folder was not allowed.".into()),
        });
        h.run(0.1);
        assert!(h.texts().iter().any(|t| t.contains("“Pack”")));
        h.key("Enter", "Enter");
        assert_eq!(seen.borrow().last(), Some(&(PromptChoice::Allow, true)));
        state.borrow_mut().set_step(PromptStep::Interrupted);
        h.run(0.1);
        h.key("Enter", "Enter");
        assert_eq!(seen.borrow().last(), Some(&(PromptChoice::Retry, true)));
        state
            .borrow_mut()
            .set_notes(vec!["loading the video decoder… 40 %".into()]);
        state.borrow_mut().set_step(PromptStep::Ready);
        h.run(0.1);
        assert!(h.texts().iter().any(|t| t.contains("40 %")));
    }

    #[test]
    fn keys_work_from_the_first_frame() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_step(PromptStep::Ready);
        let env = Env::new(Rc::new(look::Plain), Rc::new(|s: &str| s.to_string()));
        let seen: Seen = Rc::default();
        let act: PromptFn = {
            let seen = seen.clone();
            Rc::new(move |c, cx: &mut Cx| seen.borrow_mut().push((c, cx.in_gesture())))
        };
        let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        h.push(PromptScreen::new(env, state, act));
        // No time for an entrance: Escape at once goes back.
        assert_eq!(h.ui.layers()[0].phase, "active");
        h.key("Escape", "Escape");
        assert_eq!(
            seen.borrow().last().map(|s| &s.0),
            Some(&PromptChoice::Back)
        );
    }

    #[test]
    fn start_starts_when_ready_whatever_has_the_focus() {
        let state = SharedPrompt::default();
        state
            .borrow_mut()
            .set_step(PromptStep::Loading("loading…".into()));
        let (mut h, seen) = prompt(&state);
        assert_eq!(h.ui.focused(), Some("Back"));
        // Ready, and Start before a frame has built it: Start, not Back.
        state.borrow_mut().set_step(PromptStep::Ready);
        h.key("Enter", "Enter");
        assert_eq!(
            seen.borrow().last().map(|s| &s.0),
            Some(&PromptChoice::Start)
        );
        // Built: the focus is on Start.
        h.run(0.1);
        assert_eq!(h.ui.focused(), Some("Start"));
    }

    #[test]
    fn both_new_devices_are_offered_together() {
        use crate::state::settings::Settings;
        let mut s = Settings::default();
        Settings::set_profile_offset(
            &mut s.audio_profiles,
            &DeviceFingerprint::new("audio:44100:5:40", "Headphones"),
            0.03,
        );
        Settings::set_profile_offset(
            &mut s.display_profiles,
            &DeviceFingerprint::new("display:2560x1440@2", "2560×1440 (2×)"),
            0.008,
        );
        let p = DeviceProfile {
            audio: DeviceFingerprint::new("audio:48000:5:20", "48 kHz · ~20 ms"),
            display: DeviceFingerprint::new("display:1280x720@1", "1280×720 (1×)"),
        };
        let PromptStep::NewDevice { actions, .. } = new_device_step(&s, &p, true, true) else {
            panic!()
        };
        assert_eq!(
            actions[0].0,
            "copy-audio:audio:44100:5:40+copy-display:display:2560x1440@2"
        );
        assert_eq!(
            actions[0].1,
            "Use +30 ms from Headphones and +8 ms from 2560×1440 (2×)"
        );
        // Taken: both kept with the offsets copied.
        super::super::notices::register_devices(&mut s, &p, &actions[0].0);
        assert_eq!(s.audio_profile("audio:48000:5:20").unwrap().offset, 0.03);
        assert_eq!(
            s.display_profile("display:1280x720@1").unwrap().offset,
            0.008
        );
    }

    #[test]
    fn a_new_device_offers_the_calibrated_ones_and_plays_on_first() {
        use crate::state::settings::Settings;
        let mut s = Settings::default();
        Settings::set_profile_offset(
            &mut s.audio_profiles,
            &DeviceFingerprint::new("audio:44100:5:40", "Headphones"),
            0.03,
        );
        let p = DeviceProfile {
            audio: DeviceFingerprint::new("audio:48000:5:20", "48 kHz · ~20 ms"),
            display: DeviceFingerprint::new("display:1280x720@1", "1280×720 (1×)"),
        };
        let step = new_device_step(&s, &p, true, false);
        let PromptStep::NewDevice { lines, actions } = &step else {
            panic!("{step:?}")
        };
        assert_eq!(lines[1], "New audio path: 48 kHz · ~20 ms");
        assert_eq!(lines[2], "Display: 1280×720 (1×) (known)");
        let ids: Vec<&str> = actions.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            ["copy-audio:audio:44100:5:40", "calibrate", "defaults"]
        );
        assert_eq!(actions[0].1, "Use +30 ms from Headphones");
        let state = SharedPrompt::default();
        state.borrow_mut().set_step(step);
        let (mut h, seen) = prompt(&state);
        h.key("Enter", "Enter");
        h.tap(PointerKind::Mouse, "Device.copy-audio:audio:44100:5:40");
        assert_eq!(
            *seen.borrow(),
            [
                (PromptChoice::Device("defaults".into()), true),
                (
                    PromptChoice::Device("copy-audio:audio:44100:5:40".into()),
                    true
                ),
            ]
        );
    }

    #[test]
    fn a_card_offers_its_actions_the_first_focused() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_step(PromptStep::Card {
            heading: "+12.4 ms".into(),
            lines: vec![("Converged.".into(), "Text".into())],
            actions: vec![
                ("save".into(), "Save".into()),
                ("again".into(), "Again".into()),
            ],
        });
        let (mut h, seen) = prompt(&state);
        assert!(h.texts().iter().any(|t| t == "Converged."));
        h.key("Enter", "Enter");
        h.tap(PointerKind::Mouse, "Action.again");
        assert_eq!(
            *seen.borrow(),
            [
                (PromptChoice::Action("save".into()), true),
                (PromptChoice::Action("again".into()), true)
            ]
        );
    }

    #[test]
    fn the_mods_line_says_the_speed_and_what_is_off_its_default() {
        use crate::state::settings::Settings;
        let mut s = Settings {
            speed: 2.5,
            reverse: true,
            ..Settings::default()
        };
        s.transform.turn = ddi_engine::Turn::Mirror;
        assert_eq!(mods_line(&s), "×2.5 · reverse · mirror");
    }
}
