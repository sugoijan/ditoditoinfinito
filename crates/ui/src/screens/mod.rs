//! The theme's screens (`docs/plans/ui-core.md`, step 5;
//! `docs/plans/theme.md`): song select, results and options, laid out as
//! the maintainer's mockups lay them out but knowing nothing of how they
//! look. Places, sizes and timings are the look's metrics
//! ([`crate::look::metric`], with the mockups' numbers as defaults);
//! everything drawn is a component by name, falling back to the plain
//! look's ([`crate::look::component_or`]). Step 6 feeds them the
//! library, the engine's records and the settings through the plain data
//! of [`data`]; the developer routes feed them [`sample`].
//!
//! Components and their parts (origin top left unless said; a part a
//! look leaves out is skipped):
//!
//! - `Screen.Header` (`Panel`): `Plate`, `Title` (the screen's name),
//!   `Subtitle`. `Screen.Footer` (`Panel`): `Plate`.
//! - `KeyHint` (centered; `Button`): its typed key caps in `Spec::keycaps`, its
//!   label as the text, optional plain prefix in `Spec::key_prefix`;
//!   `Extent` (a hidden quad as large as it is).
//!   `Pill` (centered; `Button`): `Label`, `Extent`.
//!   `Link` (centered; `Button`): `Label`, `Extent`.
//! - `SongList.Row` (`List.Row`): `Plate`, `Banner`, `Title`, `Subtitle`
//!   (the artist), `Right` (kept at the row's right edge as the wheel
//!   moves rows right) holding `Cell0`…`Cell4` with `Meter0`…`Meter4`
//!   (Beginner to Challenge) and `NoCharts`; `Pulse`. Commands
//!   `Select`, `Deselect`, `Available`, `Unavailable`; `Full` and
//!   `Empty` on a cell. `SongList.Header` (`List.Header`): `Plate`,
//!   `Chevron`, `Title`, `Count`; `Select`, `Deselect`.
//! - `SongInfo` (`Panel`): `Banner`, `Title`, `Subtitle`, `Artist`,
//!   `Bpm`, `Length`, `Pack`, `Line` (artist, BPM and length on one
//!   line, for a narrow one).
//! - `StyleTab` (centered; `Button`): `Label`, `Count`, `Extent`;
//!   commands `Selected`, `Available`, `Unavailable`.
//! - `ChartLadder.Row` (`List.Row`): `Plate`, `Bar`, `Label`, `Notes`,
//!   `Meter`, `Pulse`; `Select`, `Deselect` and the difficulty's name
//!   (`Hard`). `ChartLadder.Empty` (`Panel`): `Title`, `Hint`.
//!   `ChartButton` (centered; `Button`): `Meter`, `Assist`; the same
//!   commands, and `Assisted`, `Converted` or `Original` (a generated
//!   version's mark).
//!   `PlayButton`, `IconButton`, `StyleButton` (centered; `Button`).
//!   `Tab` (centered; `Button`): `Label`, `Underline`; `Selected`,
//!   `Available` (the results' panel sets).
//! - `Results.Judgement` (`List.Row`): `Name`, `Bar` (as long as the
//!   row's `Spec` is wide, zoomed by the share), `Count`; its color's
//!   name as a command (`Judgement.W1`). `Results.Score` (`Panel`):
//!   `Grade`, `Score`, `Ex`. `Panel`: `Plate`, `Title`, `Note`.
//! - `Options.Section` (`List.Header`): `Plate`, `Title`, `Mark`;
//!   `Select`, `Deselect`. `Options.Row` (`List.Row`): `Plate`, `Label`,
//!   `Pulse`; `Select`, `Deselect` (the focused option).
//!   `Options.Description` (`Panel`): `Title`, `Note`. `Options.Preview`
//!   (`Panel`): `Plate`, `Title`.
//!
//! Colors a screen asks for itself: `Ground`, `Text`, `Muted`,
//! `Accent`, `Difficulty.<name>`, `Judgement.<W1…Miss>`, `Graph.*`.
//!
//! Motion: selections glide, the beat pulse lights the selection once a
//! beat of the previewed song ([`Pulse`]), screens fade and slide in;
//! with reduced motion ([`Env::reduced_motion`]) the pulse and the
//! slides stop and the wheel jumps.

pub mod calibration;
pub mod credits;
pub mod data;
pub mod editor;
pub mod editor_dialogs;
pub mod editor_field;
pub mod folders_offer;
pub mod numbers;
pub mod options;
pub mod options_dialogs;
pub mod options_model;
pub mod options_shell;
#[cfg(test)]
mod options_tests;
pub mod page;
#[cfg(test)]
mod page_tests;
pub mod play;
pub mod prompt;
pub mod prompt_controls;
pub mod prompt_options;
pub mod results;
pub mod sample;
pub mod select;
pub mod select_dialogs;
mod select_navigation;
pub mod start;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wording_tests;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_actor::commands::{Call, parse};
use ddi_actor::{Actor, Kind, Rgba, Sprite};

use crate::dev_widgets::FoldFn;
use crate::hit;
use crate::look::{self, Look, Spec, call, named, quad, text_weighted};

/// The theme's design size (`docs/plans/theme.md`).
pub const DESIGN: [f32; 2] = [1280.0, 720.0];
/// The compact layout's, for small windows (a phone held sideways).
pub const COMPACT: [f32; 2] = [844.0, 390.0];
/// Below this many CSS pixels of height a window gets the compact
/// layout: phones held sideways are 360–430 high; tablets and laptop
/// windows 600 and more.
pub const COMPACT_BELOW: f32 = 480.0;
/// Windows narrower than this many CSS pixels get the compact design too
/// (a phone held upright).
pub const COMPACT_NARROWER: f32 = 600.0;

/// The design for a window `css` CSS pixels large.
pub fn design_for(look: &dyn Look, css: [f32; 2]) -> [f32; 2] {
    // Short (a phone held sideways) or narrow (held upright): the compact
    // design.
    if css[1] < look::metric(look, "Compact.Below", COMPACT_BELOW)
        || css[0] < look::metric(look, "Compact.Narrower", COMPACT_NARROWER)
    {
        [
            look::metric(look, "Compact.Width", COMPACT[0]),
            look::metric(look, "Compact.Height", COMPACT[1]),
        ]
    } else {
        [
            look::metric(look, "Design.Width", DESIGN[0]),
            look::metric(look, "Design.Height", DESIGN[1]),
        ]
    }
}

/// Where the beat is: given the song's id, seconds since it was
/// selected and its display BPM, the beat now (fractions count), or
/// `None` while nothing plays. The developer routes count from the BPM
/// ([`bpm_beat`]); step 6 reads the preview's audio time.
pub type BeatFn = Rc<dyn Fn(&str, f64, f32) -> Option<f64>>;

/// Beats from the display BPM alone.
pub fn bpm_beat() -> BeatFn {
    Rc::new(|_, t, bpm| (bpm > 0.0).then(|| t * f64::from(bpm) / 60.0))
}

/// What the screens did, shared with the shell (its debug view, tests).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeState {
    /// The top screen's name (`select`, `results`, `options`).
    pub screen: String,
    pub compact: bool,
    pub reduced_motion: bool,
    /// The selected song's title and id; a pack's name when a header is
    /// selected.
    pub song: String,
    pub song_id: String,
    pub header: Option<String>,
    /// The selected song has no chart in the style.
    pub greyed: bool,
    pub style: String,
    /// Each style's chart count for the selected song.
    pub style_counts: Vec<(String, usize)>,
    /// The chart shown ("Hard 13"), and the difficulty the player chose.
    pub chart: Option<String>,
    pub preferred: String,
    pub ladder: Vec<String>,
    /// Wheel items, and packs closed.
    pub items: usize,
    pub closed: Vec<String>,
    /// Songs greyed in the wheel's built rows (for tests).
    pub greyed_rows: usize,
    pub search: String,
    pub popup: bool,
    pub suggestions: Vec<String>,
    pub matches: usize,
    /// The footer's hints ("ENTER Play").
    pub hints: Vec<String>,
    /// The pulse now (0 at rest), and whether it runs.
    pub pulse: f32,
    pub pulsing: bool,
    /// The backdrop's image.
    pub backdrop: Option<String>,
    /// Results: the panel set; the song offset.
    pub panels: String,
    pub song_offset: i32,
    /// Options: the section, the focused option (its label and id) and
    /// its value; the section's rows' ids, how far they are scrolled,
    /// what the last action did, and where the field's preview is (the
    /// surface's pixels).
    pub section: String,
    pub option: String,
    pub option_id: String,
    pub value: String,
    pub option_rows: Vec<String>,
    pub scroll: f32,
    pub notice: String,
    pub preview: Option<[f32; 4]>,
    /// What happened last.
    pub status: String,
    /// The reduced motion setting as options show it (`Auto`, `On`,
    /// `Off`: 0, 1, 2), kept by the shell.
    pub reduced_motion_setting: usize,
    /// Screens open over song select (results, options): it says what it
    /// does again only once none is.
    pub over: usize,
    /// Song select: the selected row's actor, and where the wheel is
    /// (virtual pixels `[x, y, w, h]`), for tests.
    pub selected_row: Option<String>,
    pub wheel: [f32; 4],
    /// Song select: the selected chart's best, its notices, the import
    /// dialog's status.
    pub best: Option<String>,
    pub notices: Vec<String>,
    /// The import dialog is open (it says so while it is).
    pub import_open: bool,
    /// A page (credits, calibration): its links', buttons' and folds' ids
    /// (`link:<url>`, `button:<id>`, `fold:<id>`), the focused one's
    /// index (-1: none), its height. The start prompt: its buttons'
    /// names (`Start`, `Device.defaults`).
    pub page_items: Vec<String>,
    /// Song select: plays start autoplayed.
    pub autoplay: bool,
    pub page_focus: i32,
    pub page_height: f32,
    /// Song select's plays list, while it is open: its rows (`id`,
    /// `when`, `grade score lamp`, `mods`, a row a line, `|` between),
    /// and the chosen row's id.
    pub plays: Vec<String>,
    pub plays_selected: Option<String>,
    /// The chart editor, while it is up.
    pub editor: Option<editor::EditorView>,
    /// The start prompt, while it is up.
    pub prompt: Option<prompt::PromptView>,
}

pub type SharedTheme = Rc<RefCell<ThemeState>>;

/// What the screens are given: the look, the shell's folding (search),
/// motion, the beat, and where to say what they did.
#[derive(Clone)]
pub struct Env {
    pub look: Rc<dyn Look>,
    pub fold: FoldFn,
    /// Motion is reduced (the settings' choice with the system's
    /// preference; the shell keeps it current).
    pub reduced_motion: Rc<Cell<bool>>,
    pub beat: BeatFn,
    pub state: SharedTheme,
}

impl Env {
    pub fn new(look: Rc<dyn Look>, fold: FoldFn) -> Env {
        Env {
            look,
            fold,
            reduced_motion: Rc::new(Cell::new(false)),
            beat: bpm_beat(),
            state: SharedTheme::default(),
        }
    }

    pub fn presentation(&self) -> crate::look::Presentation<'_> {
        crate::look::Presentation {
            look: &*self.look,
            reduced_motion: self.reduced(),
        }
    }

    pub fn reduced(&self) -> bool {
        self.reduced_motion.get()
    }

    /// Follows reduced motion `setting` at once, `system` the system's
    /// preference: motion as the screens see it, and the setting as
    /// options show it ([`ThemeState::reduced_motion_setting`]).
    pub fn follow_motion(&self, setting: crate::state::settings::ReducedMotion, system: bool) {
        self.reduced_motion.set(setting.reduced(system));
        self.state.borrow_mut().reduced_motion_setting = setting as usize;
    }

    pub fn metric(&self, name: &str, default: f32) -> f32 {
        look::metric(&*self.look, name, default)
    }

    pub fn color(&self, name: &str) -> Rgba {
        look::color(&*self.look, name)
    }

    /// A run of text in the look's text face.
    pub fn text(&self, name: &str, s: &str, size: f32, weight: u16, color: &str) -> Actor {
        text_weighted(
            &look::font_for(&*self.look, "Text"),
            weight,
            name,
            s,
            size,
            self.color(color),
        )
    }

    /// A run of numbers in the look's numbers face.
    pub fn numbers(&self, name: &str, s: &str, size: f32, color: &str) -> Actor {
        let weight = self.metric("Numbers.Weight", 800.0) as u16;
        text_weighted(
            &look::font_for(&*self.look, "Numbers"),
            weight,
            name,
            s,
            size,
            self.color(color),
        )
    }

    /// Component `name`, else `fallback`, named `as_name` and placed at
    /// `at`.
    pub fn component(
        &self,
        name: &str,
        fallback: &str,
        spec: &Spec,
        as_name: &str,
        at: [f32; 2],
    ) -> Actor {
        let mut a = look::component_or(&*self.look, name, fallback, spec);
        a.name = as_name.into();
        call(&mut a, "xy", &at);
        a
    }
}

/// A frame of `children` named `name`.
pub fn frame(name: &str, children: Vec<Actor>) -> Actor {
    named(Kind::Frame(children), name)
}

/// Text placed with its left edge at `x`, its middle at `y`.
pub fn left(mut a: Actor, x: f32, y: f32) -> Actor {
    call(&mut a, "xy", &[x, y]);
    call(&mut a, "halign", &[0.0]);
    a
}

/// Text placed with its right edge at `x`.
pub fn right(mut a: Actor, x: f32, y: f32) -> Actor {
    call(&mut a, "xy", &[x, y]);
    call(&mut a, "halign", &[1.0]);
    a
}

/// How large a component says it is (its `Extent` part), else `spec`.
pub fn extent(a: &mut Actor, spec: [f32; 2]) -> [f32; 2] {
    match crate::widgets::part(a, "Extent") {
        Some(e) => {
            let s = e.size();
            let z = e.current().scale;
            [s[0] * z[0], s[1] * z[1]]
        }
        None => spec,
    }
}

/// Sets text part `part` of the actor named `owner`.
pub fn set(root: &mut Actor, owner: &str, part: &str, s: &str) {
    if let Some(a) = hit::find_mut(root, owner) {
        crate::widgets::set_text(a, part, s);
    }
}

/// Plays `command` on the actor named `owner`.
pub fn play(root: &mut Actor, owner: &str, command: &str) {
    if let Some(a) = hit::find_mut(root, owner) {
        crate::widgets::play(a, command);
    }
}

/// The screen's header across the top: its name, the game's, and what
/// is put at its right by the screen.
/// The header along the top: the Back button (`Back`), the wordmark
/// (the look's image for `Screen.Header`, bound to its `Wordmark` part's
/// sprites) over the screen's `title`.
pub fn header(env: &Env, cx: &mut crate::Cx, width: f32, title: &str) -> Actor {
    let h = env.metric("Header.Height", 64.0);
    let spec = Spec::new([width, h], title).with_items(["Dito Dito Infinito"]);
    let mut a = env.component("Screen.Header", "Panel", &spec, "Header", [0.0, 0.0]);
    if let Some(source) = env.look.image("Screen.Header")
        && let Some(mark) = crate::widgets::part(&mut a, "Wordmark")
    {
        let texture = cx.texture(source, h);
        sprites_mut(mark, &mut |s| s.texture = texture);
    }
    // A look without a Back button of its own gets the plain one.
    if crate::widgets::part(&mut a, "Back").is_none()
        && let Kind::Frame(c) = &mut a.kind
    {
        let m = env.metric("Margin", 32.0);
        c.push(back_button(env, [44.0, 44.0], [m + 22.0, h / 2.0]));
    }
    a
}

/// The vivid paint's clock for a screen's live art (the start menu's
/// title, the header's wordmark): the shell's clock ([`crate::Cx::clock`]), so
/// the paint keeps its phase from screen to screen and from the web
/// loader's title; set in `update` on the frame that holds it, reported
/// in `animating`. Reduced motion holds it still (at zero from the
/// start, as the loader holds its title).
#[derive(Default)]
pub(crate) struct Live {
    time: f64,
    /// A vivid sprite was found at the last tick.
    lit: bool,
}

impl Live {
    /// Takes the clock (unless motion is reduced) and sets it on every
    /// vivid sprite under the actor named `name` (the first of that
    /// name: the header's `Wordmark`, the start menu's `Title`, not a
    /// notice's or a dialog's part).
    pub fn tick(&mut self, root: &mut Actor, name: &str, env: &Env, cx: &crate::Cx) {
        if !env.reduced() {
            // Wrapped at a multiple of the four-second palette loop: the
            // paint's f32 keeps its precision after days of uptime.
            self.time = cx.clock().rem_euclid(4096.0);
        }
        let time = self.time as f32;
        let mut lit = false;
        if let Some(art) = hit::find_mut(root, name) {
            sprites_mut(art, &mut |s| {
                if let Some(vivid) = &mut s.shape.vivid {
                    vivid.time = time;
                    lit = true;
                }
            });
        }
        self.lit = lit;
    }

    /// Whether frames are wanted for it.
    pub fn animating(&self, env: &Env) -> bool {
        self.lit && !env.reduced()
    }
}

/// A screen's Back button, named `Back`, its middle at `at`.
pub fn back_button(env: &Env, size: [f32; 2], at: [f32; 2]) -> Actor {
    env.component("BackButton", "Button", &Spec::new(size, "‹"), "Back", at)
}

/// `f` on every sprite under `actor` (a component's art stays in the
/// look; the screen supplies its image and clock).
pub(crate) fn sprites_mut(actor: &mut Actor, f: &mut impl FnMut(&mut Sprite)) {
    if let Kind::Sprite(sprite) = &mut actor.kind {
        f(sprite);
    }
    for child in actor.children_mut() {
        sprites_mut(child, f);
    }
}

/// One key hint: key caps and what they do. A look may supply a `KeyHint`
/// image atlas; its `Cap0`, `Cap1`, … sprites receive that texture.
pub fn key_hint(
    env: &Env,
    name: &str,
    keys: &[crate::keycaps::Keycap],
    label: &str,
    cx: &mut crate::Cx,
) -> (Actor, f32) {
    key_hint_prefixed(env, name, keys, "", label, cx)
}

fn key_hint_prefixed(
    env: &Env,
    name: &str,
    keys: &[crate::keycaps::Keycap],
    prefix: &str,
    label: &str,
    cx: &mut crate::Cx,
) -> (Actor, f32) {
    let h = 22.0;
    let labels: Vec<_> = keys.iter().map(crate::keycaps::Keycap::label).collect();
    let text = format!("{prefix} {} {label}", labels.join(" "))
        .trim_start()
        .to_string();
    let w = look::mono_width(&text, 13.0) + 24.0;
    let mut spec = Spec::new([0.0, h], label).with_keycaps(keys.iter().cloned());
    spec.key_prefix = prefix.into();
    let mut a = look::component_or(&*env.look, "KeyHint", "Button", &spec);
    let mut fallback = false;
    if crate::widgets::part(&mut a, "Extent").is_none() {
        // The plain button: sized here, its label all of it.
        a = look::component(&*env.look, "Button", &Spec::new([w, h], &text));
        fallback = true;
    }
    a.name = name.into();
    if !fallback && let Some(source) = env.look.image("KeyHint") {
        let texture = cx.texture(source, 1280.0);
        for (i, key) in keys.iter().enumerate() {
            if let Some(cap) = crate::widgets::part(&mut a, &format!("Cap{i}"))
                && let Kind::Sprite(sprite) = &mut cap.kind
            {
                sprite.texture = if matches!(key, crate::keycaps::Keycap::Controller(_)) {
                    cx.texture("ui/controllers.png", 2048.0)
                } else {
                    texture
                };
            }
        }
    }
    let w = if fallback {
        w
    } else {
        extent(&mut a, [w, h])[0]
    };
    (a, w)
}

/// The footer along the bottom with key hints from the left and links
/// from the right; the hints' texts.
pub fn footer(
    env: &Env,
    screen: [f32; 2],
    hints: &[(&[&str], &str)],
    controller: &[(&[ddi_engine::MenuInput], &str)],
    note: &str,
    links: &[(&str, &str)],
    cx: &mut crate::Cx,
) -> (Actor, Vec<String>) {
    let hints: Vec<_> = hints
        .iter()
        .map(|&(keys, label)| FooterHint {
            keys,
            label,
            prefix: "",
        })
        .collect();
    footer_hints(env, screen, &hints, controller, note, links, cx)
}

/// A footer shortcut, with optional plain text outside its keycaps.
pub struct FooterHint<'a> {
    pub keys: &'a [&'a str],
    pub label: &'a str,
    pub prefix: &'a str,
}

pub fn footer_hints(
    env: &Env,
    screen: [f32; 2],
    hints: &[FooterHint<'_>],
    controller: &[(&[ddi_engine::MenuInput], &str)],
    note: &str,
    links: &[(&str, &str)],
    cx: &mut crate::Cx,
) -> (Actor, Vec<String>) {
    let h = env.metric("Footer.Height", 40.0);
    let hint_gap = env.metric("Footer.HintGap", 20.0);
    let margin = env.metric("Margin", 32.0);
    let y = screen[1] - h;
    let mut plate = look::component_or(
        &*env.look,
        "Screen.Footer",
        "Panel",
        &Spec::new([screen[0], h], ""),
    );
    plate.name = "Footer.Plate".into();
    let mut children = vec![plate];
    let mut x = margin;
    let mut texts = Vec::new();
    let resolved: Vec<(Vec<crate::keycaps::Keycap>, &str, &str)> =
        if crate::controller_hints::active() {
            controller
                .iter()
                .filter_map(|(actions, label)| {
                    let caps = if *actions
                        == [ddi_engine::MenuInput::Confirm, ddi_engine::MenuInput::Start]
                    {
                        crate::controller_hints::activate()
                    } else {
                        crate::controller_hints::actions(actions)
                    };
                    (!caps.is_empty()).then_some((caps, "", *label))
                })
                .collect()
        } else {
            hints
                .iter()
                .map(|h| {
                    (
                        h.keys
                            .iter()
                            .map(|k| crate::keycaps::Keycap::from_hint(k))
                            .collect(),
                        h.prefix,
                        h.label,
                    )
                })
                .collect()
        };
    for (i, (caps, prefix, label)) in resolved.iter().enumerate() {
        let (mut a, w) = key_hint_prefixed(env, &format!("Hint{i}"), caps, prefix, label, cx);
        call(&mut a, "xy", &[x + w / 2.0, h / 2.0]);
        x += w + hint_gap;
        children.push(a);
        let printed = if crate::controller_hints::active() {
            caps.iter()
                .map(crate::keycaps::Keycap::label)
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            hints[i].keys.join(" ")
        };
        texts.push(
            format!("{prefix} {printed} {label}")
                .trim_start()
                .to_string(),
        );
    }
    if !note.is_empty() {
        children.push(left(
            env.text("Footer.Note", note, 13.0, 400, "Muted"),
            x,
            h / 2.0,
        ));
        texts.push(note.to_string());
    }
    let mut rx = screen[0] - margin;
    for (name, label) in links.iter().rev() {
        let w = look::mono_width(label, 13.0) + 16.0;
        let mut a = look::component_or(&*env.look, "Link", "Button", &Spec::new([w, 32.0], *label));
        a.name = (*name).into();
        call(&mut a, "xy", &[rx - w / 2.0, h / 2.0]);
        rx -= w + 12.0;
        children.push(a);
    }
    let mut f = frame("Footer", children);
    call(&mut f, "xy", &[0.0, y]);
    (f, texts)
}

/// The ground: the look's darkest color over everything.
pub fn ground(env: &Env, screen: [f32; 2]) -> Actor {
    let mut g = quad(
        "Ground",
        [screen[0] + 8.0, screen[1] + 8.0],
        env.color("Ground"),
    );
    call(&mut g, "xy", &[screen[0] / 2.0, screen[1] / 2.0]);
    g
}

/// The selected song's background behind a screen, blurred and dim
/// (`docs/plans/theme.md`): a small, cached Gaussian-filtered texture
/// (`Backdrop.Side` physical pixels), drawn once at `Backdrop.Alpha`.
/// A new image cross-fades in over `Backdrop.Fade` seconds. Filtering
/// happens once on upload, never on an ordinary frame.
pub struct Backdrop {
    /// The image each of the two layers shows; which is in front.
    shown: [Option<String>; 2],
    front: usize,
    /// The metric its strength is read from.
    alpha: &'static str,
}

impl Backdrop {
    pub fn new() -> Backdrop {
        Backdrop::with_alpha("Backdrop.Alpha")
    }

    /// A backdrop as strong as metric `alpha` says (results' is fainter).
    pub fn with_alpha(alpha: &'static str) -> Backdrop {
        Backdrop {
            shown: [None, None],
            front: 0,
            alpha,
        }
    }

    /// The backdrop's actors, over `screen`.
    pub fn build(&mut self, env: &Env, screen: [f32; 2]) -> Actor {
        let front = self.front;
        let size = screen;
        let alpha = env.metric(self.alpha, 0.2).clamp(0.0, 1.0);
        let layers: Vec<Actor> = (0..2)
            .map(|l| {
                let mut image = named(Kind::Sprite(Sprite::quad(size)), "Image");
                if let Kind::Sprite(s) = &mut image.kind {
                    s.fit = ddi_actor::Fit::Cover;
                }
                call(&mut image, "xy", &[screen[0] / 2.0, screen[1] / 2.0]);
                call(&mut image, "diffuse", &[1.0, 1.0, 1.0, alpha]);
                image.visible = false;
                let mut f = frame(&format!("Backdrop.{l}"), vec![image]);
                call(
                    &mut f,
                    "diffusealpha",
                    &[if l == front { 1.0 } else { 0.0 }],
                );
                f
            })
            .collect();
        // Rebuilt (a new layout): the front layer's image comes back with
        // `restore`, the other one is let go.
        self.shown[1 - self.front] = None;
        let mut ground = quad(
            "BaseGround",
            screen,
            [
                ddi_actor::background::GROUND[0],
                ddi_actor::background::GROUND[1],
                ddi_actor::background::GROUND[2],
                1.0,
            ],
        );
        call(&mut ground, "xy", &[screen[0] / 2.0, screen[1] / 2.0]);
        let mut children = vec![ground];
        children.extend(layers);
        frame("Backdrop", children)
    }

    /// Puts the textures back on a rebuilt backdrop.
    pub fn restore(&self, root: &mut Actor, cx: &mut crate::Cx, env: &Env) {
        let side = env.metric("Backdrop.Side", 256.0);
        for (l, s) in self.shown.iter().enumerate() {
            if let Some(source) = s {
                let id = cx.blurred_texture(source, side);
                Self::show(root, l, Some(id));
            }
        }
    }

    fn show(root: &mut Actor, layer: usize, texture: Option<u32>) {
        if let Some(f) = hit::find_mut(root, &format!("Backdrop.{layer}")) {
            for c in f.children_mut() {
                if let Kind::Sprite(s) = &mut c.kind {
                    s.texture = texture.unwrap_or(ddi_actor::UNTEXTURED);
                }
                c.visible = texture.is_some();
            }
        }
    }

    /// The image now: `source`, cross-fading from the last.
    pub fn set(&mut self, source: Option<&str>, root: &mut Actor, cx: &mut crate::Cx, env: &Env) {
        if self.shown[self.front].as_deref() == source {
            return;
        }
        let side = env.metric("Backdrop.Side", 256.0);
        let fade = env.metric("Backdrop.Fade", 0.35);
        let back = 1 - self.front;
        let id = source.map(|s| cx.blurred_texture(s, side));
        Self::show(root, back, id);
        self.shown[back] = source.map(str::to_string);
        for (l, to) in [(back, 1.0), (self.front, 0.0)] {
            if let Some(f) = hit::find_mut(root, &format!("Backdrop.{l}")) {
                look::run(f, &format!("stoptweening;linear,{fade};diffusealpha,{to}"));
            }
        }
        self.front = back;
    }

    pub fn source(&self) -> Option<&str> {
        self.shown[self.front].as_deref()
    }
}

impl Default for Backdrop {
    fn default() -> Backdrop {
        Backdrop::new()
    }
}

/// The beat pulse: how lit the selection is, from 1 on the beat down to
/// 0, settling over `Pulse.Decay` seconds, times `Pulse.Amount`.
#[derive(Clone, Debug, Default)]
pub struct Pulse {
    /// Seconds since the song was selected.
    pub since: f64,
    pub value: f32,
    /// Whether it runs (a song with a tempo, motion not reduced).
    pub on: bool,
}

impl Pulse {
    /// A new song: its beat counts from now.
    pub fn restart(&mut self) {
        self.since = 0.0;
    }

    /// `dt` later, for song `id` at `bpm`.
    pub fn update(&mut self, dt: f32, env: &Env, id: &str, bpm: f32) -> f32 {
        self.since += f64::from(dt);
        let beat = if env.reduced() {
            None
        } else {
            (env.beat)(id, self.since, bpm)
        };
        self.on = beat.is_some();
        self.value = match beat {
            Some(b) if bpm > 0.0 => {
                let after = (b.rem_euclid(1.0) * 60.0 / f64::from(bpm)) as f32;
                let decay = env.metric("Pulse.Decay", 0.3).max(0.01);
                env.metric("Pulse.Amount", 0.35) * (-after / decay * 3.0).exp()
            }
            _ => 0.0,
        };
        self.value
    }
}

/// Shows the pulse on the `Pulse` part of the actor named `owner` (and
/// takes it off the one lit before).
pub fn light(root: &mut Actor, owner: Option<&str>, value: f32, last: &mut Option<String>) {
    if last.as_deref() != owner
        && let Some(old) = last.take()
        && let Some(a) = hit::find_mut(root, &old)
        && let Some(p) = crate::widgets::part(a, "Pulse")
    {
        call(p, "diffusealpha", &[0.0]);
    }
    if let Some(name) = owner
        && let Some(a) = hit::find_mut(root, name)
        && let Some(p) = crate::widgets::part(a, "Pulse")
    {
        call(p, "diffusealpha", &[value]);
        *last = Some(name.to_string());
    }
}

/// Whether the screen is on the compact design ([`design_for`]'s, from
/// the look's `Compact.*`).
pub fn is_compact(look: &dyn Look, vp: ddi_actor::screen::VirtualScreen) -> bool {
    let compact = [
        look::metric(look, "Compact.Width", COMPACT[0]),
        look::metric(look, "Compact.Height", COMPACT[1]),
    ];
    vp.design == compact
}

/// A screen's layout size, zoom and whether it is compact (results,
/// options, the pages): the full design as it is; a compact one sideways
/// (or no wider than `Options.Narrow.Width`) as it is (a phone's own
/// size); upright, that wide, zoomed to the screen's width.
pub(crate) fn compact_fit(
    env: &Env,
    vp: ddi_actor::screen::VirtualScreen,
) -> ([f32; 2], f32, bool) {
    if !is_compact(&*env.look, vp) {
        return ([vp.width, vp.height], 1.0, false);
    }
    let (size, zoom) = narrow_fit(env, vp);
    (size, zoom, true)
}

/// A screen's layout size and zoom on any design: sideways (or no wider
/// than `Options.Narrow.Width`) as it is; upright, that wide, zoomed to
/// the screen's width.
pub(crate) fn narrow_fit(env: &Env, vp: ddi_actor::screen::VirtualScreen) -> ([f32; 2], f32) {
    let narrow = env.metric("Options.Narrow.Width", 480.0);
    if vp.width >= vp.height || vp.width <= narrow {
        ([vp.width, vp.height], 1.0)
    } else {
        let z = vp.width / narrow;
        ([narrow, vp.height / z], z)
    }
}

/// The images every player's first screens draw, at the sides they are
/// drawn at (virtual pixels), for a shell to load under its loader so
/// the screens come whole (the web's `preload`): the start menu's title
/// and rain, the header's wordmark, the keycap and controller hint
/// atlases. Kept beside the screens that draw them: the same looks, the
/// same sides ([`crate::Cx::texture`]'s).
pub fn essential_images(env: &Env, vp: ddi_actor::screen::VirtualScreen) -> Vec<(String, f32)> {
    let ([w, _], _) = narrow_fit(env, vp);
    let mut v = Vec::new();
    if let Some(s) = env.look.image("TitleMenu.Title") {
        v.push((
            s.to_string(),
            env.metric("TitleMenu.TitleWidth", 520.0).min(w - 32.0),
        ));
    }
    if let Some(s) = env.look.image("TitleMenu.Background") {
        v.push((s.to_string(), 1024.0));
    }
    if let Some(s) = env.look.image("Screen.Header") {
        v.push((s.to_string(), env.metric("Header.Height", 64.0)));
    }
    if let Some(s) = env.look.image("KeyHint") {
        v.push((s.to_string(), 1280.0));
    }
    v.push(("ui/controllers.png".to_string(), 2048.0));
    v
}

/// `s` with its first letter capitalized ("audio" → "Audio").
pub(crate) fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// A screen over song select leaves (`cancel`: backing out).
pub fn leave(env: &Env, cx: &mut crate::Cx, cancel: bool) {
    let mut st = env.state.borrow_mut();
    st.over = st.over.saturating_sub(1);
    drop(st);
    cx.close(cancel);
}

/// The edge of the screen a part comes in from and leaves to
/// ([`transitions`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
    /// Fades where it is.
    None,
}

/// The screen's transitions, part by part (`docs/plans/theme.md`,
/// "Transitions"): each of `parts` (the name of a child of `root`, or of
/// `root` itself, and its edge) gets `On`, `Off` and `Cancel` commands that slide
/// it in from its edge over `Motion.Screen` seconds and out to it over
/// `Motion.Screen.Out`, fading with it, by `Motion.Slide.X` or
/// `Motion.Slide.Y` virtual pixels; the parts at the top and bottom edges
/// come first, the rest `Motion.Stagger` later. Reduced motion keeps the
/// fades alone. The root itself gets no command: what is not listed
/// (the ground) stays as it is.
pub fn transitions(root: &mut Actor, env: &Env, parts: &[(&str, Edge)]) {
    let t_in = env.metric("Motion.Screen", 0.2);
    let t_out = env.metric("Motion.Screen.Out", 0.15);
    let dx = env.metric("Motion.Slide.X", 120.0);
    let dy = env.metric("Motion.Slide.Y", 56.0);
    let stagger = env.metric("Motion.Stagger", 0.05);
    let reduced = env.reduced();
    for (name, edge) in parts {
        // The root or one of its children (not a part of a part: a
        // header's Back moves with the header).
        let part = if root.name == *name {
            Some(&mut *root)
        } else {
            root.children_mut().iter_mut().find(|c| c.name == *name)
        };
        let Some(part) = part else {
            continue;
        };
        let (axis, d) = match edge {
            Edge::Top => ("addy", -dy),
            Edge::Bottom => ("addy", dy),
            Edge::Left => ("addx", -dx),
            Edge::Right => ("addx", dx),
            Edge::None => ("addx", 0.0),
        };
        let delay = match edge {
            Edge::Top | Edge::Bottom => 0.0,
            _ => stagger,
        };
        let wait = if delay > 0.0 {
            format!("sleep,{delay};")
        } else {
            String::new()
        };
        // Out, a part goes back to its place unseen (`sleep,0`), so that
        // the next `On` (uncovered) slides it in from the same edge.
        let (on, off) = if reduced || d == 0.0 {
            (
                format!("diffusealpha,0;{wait}linear,{t_in};diffusealpha,1"),
                format!("finishtweening;linear,{t_out};diffusealpha,0"),
            )
        } else {
            (
                format!(
                    "diffusealpha,0;{axis},{d};{wait}decelerate,{t_in};diffusealpha,1;{axis},{}",
                    -d
                ),
                format!(
                    "finishtweening;accelerate,{t_out};diffusealpha,0;{axis},{d};sleep,0;{axis},{}",
                    -d
                ),
            )
        };
        look::command_of(part, "On", &on);
        look::command_of(part, "Off", &off);
        look::command_of(part, "Cancel", &off);
    }
}

/// The screen's transitions as one: the root fades, and slides a little
/// unless motion is reduced (the chart editor's; the menus move part by
/// part, [`transitions`]).
pub fn transitions_whole(root: &mut Actor, env: &Env) {
    let t_in = env.metric("Motion.Screen", 0.2);
    let t_out = env.metric("Motion.Screen.Out", 0.15);
    let (on, off) = if env.reduced() {
        (
            format!("diffusealpha,0;linear,{t_in};diffusealpha,1"),
            format!("linear,{t_out};diffusealpha,0"),
        )
    } else {
        (
            format!("diffusealpha,0;addx,24;decelerate,{t_in};diffusealpha,1;addx,-24"),
            format!("accelerate,{t_out};diffusealpha,0;addx,24;sleep,0;addx,-24"),
        )
    };
    look::command_of(root, "On", &on);
    look::command_of(root, "Off", &off);
    look::command_of(root, "Cancel", &off);
}

/// `fresh` takes the place of `old`, a part built again (its rows, its
/// hints changed): only what is inside changes, so a part in motion (the
/// screen coming in or leaving) keeps its place and its tweens.
pub(crate) fn refill(old: &mut Actor, fresh: Actor) {
    old.kind = fresh.kind;
}

/// How long the tweens anywhere under `a` have left (the longest).
pub(crate) fn time_left(a: &Actor) -> f32 {
    a.children()
        .iter()
        .map(time_left)
        .fold(a.tween_time_left(), f32::max)
}

/// A screen laid out again (a new size, the chrome's room taken at the
/// top): `fresh` took the old root's place, which had `left` seconds of
/// tweens to go ([`time_left`]). A screen still coming in goes on coming
/// in, from as far as it had come; otherwise its first frame shows the
/// new tree as it is.
pub(crate) fn relaid(fresh: &mut Actor, left: f32, cx: &crate::Cx) {
    let mut problems = ddi_actor::Problems::default();
    if cx.holding() {
        hold_entrances(fresh);
    }
    fresh.play("On", &mut problems);
    let total = time_left(fresh);
    // Longer tweens than an entrance (a backdrop fading, rows gliding)
    // are not one.
    if left <= 0.0 || left > total {
        fresh.finish_tweening();
    } else {
        fresh.update(total - left, &ddi_actor::Clocks::default(), &mut problems);
    }
}

/// Keeps the entrances under `a` for the web's loader to fade
/// (`Ui::hold_entrances`, `Ui::reveal`): a part with an `On` and an
/// `Off` ([`transitions`]) gets its `On` as its `RevealMessage` and an
/// `On` that keeps it unseen; its children move with it. An empty `On`
/// (a title standing in place) and a part already held are left alone.
pub(crate) fn hold_entrances(a: &mut Actor) {
    let reveal = crate::message::Message::new("Reveal").command();
    if a.has_command(&reveal) {
        return;
    }
    let part = a.has_command("Off") && a.command("On").is_some_and(|c| !c.is_empty());
    if part {
        let on = a.command("On").map(<[Call]>::to_vec).unwrap_or_default();
        a.set_command(&reveal, on);
        a.set_command("On", parse("diffusealpha,0").unwrap_or_default());
        return;
    }
    for child in a.children_mut() {
        hold_entrances(child);
    }
}

/// Gives the parts under `a` their `On` back after the reveal
/// (`reveal`: the reveal's command).
pub(crate) fn unhold_entrances(a: &mut Actor, reveal: &str) {
    if let Some(on) = a.command(reveal).map(<[Call]>::to_vec) {
        a.set_command("On", on);
        a.remove_command(reveal);
        return;
    }
    for child in a.children_mut() {
        unhold_entrances(child, reveal);
    }
}

/// A visible route to the same contextual actions as a controller's West button.
pub(crate) fn more_button(env: &Env, centre: [f32; 2]) -> Actor {
    env.component(
        "Button",
        "Button",
        &Spec::new([64.0, 40.0], "More"),
        "More",
        centre,
    )
}

pub(crate) fn action_menu(
    env: &Env,
    title: &str,
    items: Vec<(String, String)>,
    message: &'static str,
    cx: &mut crate::Cx,
) {
    cx.overlay(
        editor_dialogs::ActionsDialog::new(
            env.clone(),
            title,
            items,
            std::rc::Rc::new(move |id, cx| {
                cx.broadcast(
                    crate::Message::new(message)
                        .with("Action", crate::message::Param::Str(id.into())),
                )
            }),
        ),
        true,
    );
}

pub(crate) fn menu_pick<'a>(m: &'a crate::Message, name: &str) -> Option<&'a str> {
    if m.name != name {
        return None;
    }
    match m.get("Action") {
        Some(crate::message::Param::Str(s)) => Some(s),
        _ => None,
    }
}

/// Rebuilding a dialog keeps its focus name but replaces the actor carrying
/// the focus appearance. Restore that appearance without moving focus.
pub(crate) fn restore_focus(root: &mut Actor, name: Option<&str>) {
    if let Some(actor) = name.and_then(|name| crate::hit::find_mut(root, name)) {
        actor.play("GainFocus", &mut ddi_actor::Problems::default());
        actor.finish_tweening();
    }
}
