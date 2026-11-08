//! The frame the announcer studio shows a scene in
//! (`docs/plans/announcer-studio.md`, step 3): the chrome
//! ([`crate::Ui::push_chrome`]) over the game's own screens, which it
//! shows smaller, as a picture in a frame ([`Screen::stage`]), so that
//! a scene reads as something shown rather than the game itself. Around
//! the picture: the line heard and whose voice says it, what happens
//! (each step's caption), how far along the scene is, the way back to
//! the studio, and over the picture the line's words as it is said.
//!
//! The shell tells it what happens by messages ([`CAPTION`],
//! [`UNTIL`], [`SAY`]), and [`END`] takes it away: the picture grows
//! back to the whole screen as the frame fades, and it closes.

use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_l10n::UiText;
use ddi_l10n::msg::studio as m;
use ddi_library::announcer::Comment;

use super::{Env, frame, left, right};
use crate::look::{Spec, call, mono_width, quad, tracked};
use crate::message::{Message, Param};
use crate::screen::{Cx, Handled, Input, Screen, Stage};

/// What a step shows: [`Param::Str`] `Text`, the caption.
pub const CAPTION: &str = "Stage.Caption";
/// The scene ends in [`Param::Num`] `Seconds` (the progress bar's end).
pub const UNTIL: &str = "Stage.Until";
/// A line said now: [`Param::Str`] `Text` (its words, or its name), for
/// [`Param::Num`] `Seconds`.
pub const SAY: &str = "Stage.Say";
/// The scene is over: the frame goes.
pub const END: &str = "Stage.End";

/// How long the frame takes to come in or go, seconds.
const MOTION: f32 = 0.35;
/// The bezel around the picture, virtual pixels.
const BEZEL: f32 = 10.0;

/// The way back: the shell leaves the scene.
pub type BackFn = Rc<dyn Fn(&mut Cx)>;

/// What happens in a step of a scene (`ddi_session::announcer::scene`),
/// the frame's caption: the facts, worded by [`Caption::text`] in the
/// UI's language. Scores, grades and judgments are as the play's ruleset
/// writes them.
#[derive(Clone, Debug, PartialEq)]
pub enum Caption {
    /// The start menu, Singleplayer chosen on it (the welcome's scene).
    Start,
    /// Song select comes in (the welcome's).
    SongSelect,
    /// A chart is chosen on song select that gets this comment.
    Chosen(Comment),
    /// The player quits partway through the song.
    Quit,
    /// After the play: the chart cleared for the first time.
    FirstClear,
    /// After the play: its score, better than ever on the chart.
    NewRecord { score: String },
    /// A full combo, every note at `judgment` (`best`: the best one;
    /// else that one or better).
    FullCombo { judgment: String, best: bool },
    /// The life runs out.
    Failed,
    /// A play graded `grade`.
    Graded { grade: String },
    /// A play graded `grade`, the cheer after its line (`also`: what its
    /// line is for besides the grade).
    Cheer { grade: String, also: CheerAlso },
    /// Song select before the play: the chart was only ever failed.
    BeforeFailed,
    /// Song select before the play: the chart's best.
    BeforeBest { score: String },
    /// A play begins, a few seconds before its first note.
    PlayBegins,
    /// A play begins just before its first note.
    FirstNote,
    /// The player restarts the song (R twice).
    Restart,
    /// The play goes on at full life.
    FullLife,
    /// The play goes on below full life.
    BelowFull,
    /// The play goes on, the life in danger.
    Danger,
    /// The play goes on, on a battery gauge.
    Battery,
    /// The combo reaches `count` (`past`: past 1000, by another hundred).
    Combo { count: u32, past: bool },
    /// A combo of more than 50 is broken.
    ComboBroken,
    /// A life is lost on a battery gauge.
    LifeLost,
    /// One life is left on a battery gauge.
    LastLife,
    /// The song ends on a full combo, every note at `judgment` (`best`:
    /// the best one; else that one or better).
    EndsFullCombo { judgment: String, best: bool },
    /// The last note is judged: the song is cleared.
    Cleared,
}

/// What the line before a cheer is for, besides its play's grade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheerAlso {
    Nothing,
    NewRecord,
    FirstClear,
}

impl Caption {
    pub fn text(&self) -> UiText {
        match self {
            Caption::Start => m::caption_start(),
            Caption::SongSelect => m::caption_select_intro(),
            Caption::Chosen(Comment::New) => m::caption_select_new(),
            Caption::Chosen(Comment::Hard) => m::caption_select_hard(),
            Caption::Chosen(Comment::Repeat) => m::caption_select_repeat(),
            Caption::Chosen(Comment::General) => m::caption_select_general(),
            Caption::Quit => m::caption_quit(),
            Caption::FirstClear => m::caption_first_clear(),
            Caption::NewRecord { score } => m::caption_new_record(score),
            Caption::FullCombo {
                judgment,
                best: true,
            } => m::caption_full_combo_best(judgment),
            Caption::FullCombo {
                judgment,
                best: false,
            } => m::caption_full_combo(judgment),
            Caption::Failed => m::caption_failed(),
            Caption::Graded { grade } => m::caption_graded(grade),
            Caption::Cheer { grade, also } => match also {
                CheerAlso::Nothing => m::caption_cheer(grade),
                CheerAlso::NewRecord => m::caption_cheer_record(grade),
                CheerAlso::FirstClear => m::caption_cheer_first_clear(grade),
            },
            Caption::BeforeFailed => m::caption_before_failed(),
            Caption::BeforeBest { score } => m::caption_before_best(score),
            Caption::PlayBegins => m::caption_play_begins(),
            Caption::FirstNote => m::caption_first_note(),
            Caption::Restart => m::caption_restart(),
            Caption::FullLife => m::caption_full_life(),
            Caption::BelowFull => m::caption_below_full(),
            Caption::Danger => m::caption_danger(),
            Caption::Battery => m::caption_battery(),
            Caption::Combo { count, past: false } => m::caption_combo(*count),
            Caption::Combo { count, past: true } => m::caption_combo_past(*count),
            Caption::ComboBroken => m::caption_combo_broken(),
            Caption::LifeLost => m::caption_life_lost(),
            Caption::LastLife => m::caption_last_life(),
            Caption::EndsFullCombo {
                judgment,
                best: true,
            } => m::caption_ends_full_combo_best(judgment),
            Caption::EndsFullCombo {
                judgment,
                best: false,
            } => m::caption_ends_full_combo(judgment),
            Caption::Cleared => m::caption_cleared(),
        }
    }
}

pub struct StageFrame {
    env: Env,
    /// The line heard (its name), and whose voice says it.
    line: String,
    voice: String,
    caption: String,
    back: BackFn,
    /// How far in it is, 0 to 1; going once the scene is over.
    shown: f32,
    going: bool,
    closed: bool,
    /// Seconds since it came, and when the scene ends (seconds on the same
    /// count), once the shell has said.
    elapsed: f64,
    until: Option<f64>,
    /// The words said now, until when.
    saying: Option<(String, f64)>,
    screen: [f32; 2],
}

/// Where the picture goes on a screen `size` big: its scale and its top
/// left corner, and the rooms above and below it.
fn geometry([w, h]: [f32; 2]) -> (f32, [f32; 2], f32, f32) {
    let compact = h < 480.0;
    let (top, bottom) = if compact { (40.0, 58.0) } else { (64.0, 86.0) };
    let room_h = h - top - bottom - 2.0 * BEZEL;
    let room_w = w - 2.0 * (BEZEL + if compact { 12.0 } else { 32.0 });
    let scale = (room_h / h).min(room_w / w).clamp(0.3, 1.0);
    let at = [(w - w * scale) / 2.0, top + BEZEL];
    (scale, at, top, bottom)
}

fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    1.0 - (1.0 - p).powi(3)
}

impl StageFrame {
    /// The frame of a scene of `line` (its name) said by `voice` ("Your
    /// recording", "An example by Mr. Clank"), its first step's
    /// `caption`; `back` leaves the scene.
    pub fn new(env: Env, line: &str, voice: &str, caption: &str, back: BackFn) -> StageFrame {
        StageFrame {
            env,
            line: line.into(),
            voice: voice.into(),
            caption: caption.into(),
            back,
            shown: 0.0,
            going: false,
            closed: false,
            elapsed: 0.0,
            until: None,
            saying: None,
            screen: [0.0; 2],
        }
    }

    /// In at once (no entrance): a frame shown on after another's (a
    /// scene going on from the play route's prompt to its play and its
    /// results).
    pub fn at_once(mut self) -> StageFrame {
        self.shown = 1.0;
        self
    }

    /// The picture's place on the screen now (the bezel's inside),
    /// `[left, top, right, bottom]`.
    fn picture(&self) -> [f32; 4] {
        let [w, h] = self.screen;
        let (scale, at, _, _) = geometry(self.screen);
        let e = ease(self.shown);
        let s = 1.0 + (scale - 1.0) * e;
        let at = [at[0] * e, at[1] * e];
        [at[0], at[1], at[0] + w * s, at[1] + h * s]
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        let env = &self.env;
        let [w, h] = self.screen;
        let (scale, at, top, _) = geometry(self.screen);
        let compact = h < 480.0;
        let ground = env.color("Ground");
        // The ground around the picture: four bands, placed each frame.
        let mut children: Vec<Actor> = [
            "Stage.BandTop",
            "Stage.BandBottom",
            "Stage.BandLeft",
            "Stage.BandRight",
        ]
        .into_iter()
        .map(|n| quad(n, [1.0, 1.0], ground))
        .collect();
        // The bezel: a rounded ring, and its edge.
        let mut bezel = quad("Stage.Bezel", [1.0, 1.0], env.color("Raised"));
        call(&mut bezel, "outline", &[BEZEL]);
        call(&mut bezel, "cornerradius", &[BEZEL + 4.0]);
        let mut edge = quad("Stage.Edge", [1.0, 1.0], env.color("Line2"));
        call(&mut edge, "outline", &[1.0]);
        call(&mut edge, "cornerradius", &[BEZEL + 4.0]);
        // Rings, drawn only: a tap inside them is the picture's.
        bezel.decoration = true;
        edge.decoration = true;
        children.extend([bezel, edge]);
        // Above: what is heard, and who says it.
        let x0 = at[0] - BEZEL;
        let x1 = w - x0;
        let (kicker_y, line_y) = if compact { (11.0, 27.0) } else { (20.0, 42.0) };
        children.push(left(
            tracked(
                env.text("Stage.Kicker", &m::stage_kicker(), 11.0, 600, "Muted"),
                200,
            ),
            x0,
            kicker_y,
        ));
        let line = m::stage_line(&self.line);
        let voice_w = mono_width(&self.voice, 13.0);
        let mut heard = left(
            env.text(
                "Stage.Line",
                &line,
                if compact { 15.0 } else { 18.0 },
                700,
                "Text",
            ),
            x0,
            line_y,
        );
        cut(&mut heard, x1 - x0 - voice_w - 24.0);
        children.push(heard);
        children.push(right(
            env.text("Stage.Voice", &self.voice, 13.0, 400, "Muted"),
            x1,
            line_y,
        ));
        // Below: how far along, what happens, the way back.
        let below = top + 2.0 * BEZEL + h * scale;
        let bar_w = x1 - x0;
        let mut track = quad("Stage.Track", [bar_w, 3.0], env.color("Line2"));
        call(&mut track, "xy", &[x0 + bar_w / 2.0, below + 8.0]);
        let mut fill = quad("Stage.Progress", [bar_w, 3.0], env.color("Accent"));
        call(&mut fill, "xy", &[x0, below + 8.0]);
        call(&mut fill, "halign", &[0.0]);
        call(&mut fill, "zoomx", &[0.0]);
        children.extend([track, fill]);
        let (button_w, button_h) = if compact {
            (150.0, 32.0)
        } else {
            (190.0, 36.0)
        };
        let row_y = below + 12.0 + (if compact { 22.0 } else { 34.0 });
        children.push(env.component(
            "Button",
            "Button",
            &Spec::new([button_w, button_h], m::stage_back()),
            "Stage.Back",
            [x1 - button_w / 2.0, row_y],
        ));
        let mut caption = left(
            env.text(
                "Stage.Caption",
                &self.caption,
                if compact { 13.0 } else { 15.0 },
                400,
                "Secondary",
            ),
            x0,
            row_y,
        );
        cut(&mut caption, bar_w - button_w - 24.0);
        children.push(caption);
        // Over the picture: the words said.
        let mut plate = quad("Stage.SubtitlePlate", [1.0, 1.0], [0.0, 0.0, 0.0, 0.72]);
        call(&mut plate, "cornerradius", &[6.0]);
        let mut words = env.text(
            "Stage.Subtitle",
            "",
            if compact { 15.0 } else { 20.0 },
            600,
            "Text",
        );
        cut(&mut words, w * scale - 48.0);
        let mut subtitle = frame("Stage.SubtitleFrame", vec![plate, words]);
        subtitle.visible = false;
        subtitle.decoration = true;
        children.push(subtitle);
        let mut root = Actor::new(Kind::Frame(children));
        self.place(&mut root);
        root
    }

    /// Everything that moves with the picture or the time, in place.
    fn place(&self, root: &mut Actor) {
        let [w, h] = self.screen;
        let r = self.picture();
        let e = ease(self.shown);
        let b = BEZEL * e;
        let o = [r[0] - b, r[1] - b, r[2] + b, r[3] + b];
        let mut set = |name: &str, rect: [f32; 4]| {
            if let Some(a) = crate::hit::find_mut(root, name) {
                let size = [(rect[2] - rect[0]).max(0.0), (rect[3] - rect[1]).max(0.0)];
                call(a, "setsize", &size);
                call(
                    a,
                    "xy",
                    &[(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0],
                );
                a.visible = size[0] > 0.0 && size[1] > 0.0;
            }
        };
        // The bands overlap the bezel a little: no seam.
        set("Stage.BandTop", [0.0, 0.0, w, o[1] + 1.0]);
        set("Stage.BandBottom", [0.0, o[3] - 1.0, w, h]);
        set("Stage.BandLeft", [0.0, o[1], o[0] + 1.0, o[3]]);
        set("Stage.BandRight", [o[2] - 1.0, o[1], w, o[3]]);
        set("Stage.Bezel", o);
        set("Stage.Edge", o);
        if let Some(a) = crate::hit::find_mut(root, "Stage.Bezel") {
            call(a, "outline", &[b.max(0.01)]);
        }
        let progress = match self.until {
            Some(u) if u > 0.0 => (self.elapsed / u).clamp(0.0, 1.0) as f32,
            _ => 0.0,
        };
        if let Some(a) = crate::hit::find_mut(root, "Stage.Progress") {
            call(a, "zoomx", &[progress]);
        }
        // The words, centered near the picture's foot.
        let words = self.saying.as_ref().map(|(t, _)| t.as_str());
        if let Some(sub) = crate::hit::find_mut(root, "Stage.SubtitleFrame") {
            sub.visible = words.is_some_and(|t| !t.is_empty());
            let size = if h < 480.0 { 15.0 } else { 20.0 };
            let text_w = mono_width(words.unwrap_or(""), size).min(r[2] - r[0] - 48.0);
            call(
                sub,
                "xy",
                &[(r[0] + r[2]) / 2.0, r[3] - (r[3] - r[1]) * 0.12],
            );
            if let Some(p) = crate::widgets::part(sub, "Stage.SubtitlePlate") {
                call(p, "setsize", &[text_w + 28.0, size * 1.9]);
            }
        }
        call(root, "diffusealpha", &[e]);
    }

    /// Whether it moves: coming in, going, the bar filling.
    fn busy(&self) -> bool {
        let filling = self
            .until
            .is_some_and(|u| self.elapsed < u && self.shown >= 1.0);
        (self.shown < 1.0 && !self.going) || (self.going && !self.closed) || filling
    }
}

/// `a` (a run of text) cut to `width`, with an ellipsis.
fn cut(a: &mut Actor, width: f32) {
    if let Kind::Text(t) = &mut a.kind {
        t.max_width = width.max(40.0);
        t.ellipsis = true;
    }
}

fn text_of(m: &Message, key: &str) -> String {
    match m.get(key) {
        Some(Param::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn seconds_of(m: &Message) -> Option<f64> {
    match m.get("Seconds") {
        Some(Param::Num(n)) if n.is_finite() => Some(*n),
        Some(Param::Int(n)) => Some(*n as f64),
        _ => None,
    }
}

impl Screen for StageFrame {
    fn name(&self) -> &str {
        "stage"
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.tree(cx)
    }

    fn stage(&self) -> Option<Stage> {
        if self.shown <= 0.0 {
            return None;
        }
        let r = self.picture();
        Some(Stage {
            scale: (r[2] - r[0]) / self.screen[0].max(1.0),
            at: [r[0], r[1]],
        })
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        let step = if self.env.reduced() { 1.0 } else { dt / MOTION };
        if self.going {
            self.shown = (self.shown - step).max(0.0);
            if self.shown <= 0.0 && !self.closed {
                self.closed = true;
                cx.close(false);
            }
        } else {
            self.shown = (self.shown + step).min(1.0);
        }
        if self.shown >= 1.0 || self.going {
            self.elapsed += f64::from(dt);
        }
        if self
            .saying
            .as_ref()
            .is_some_and(|(_, until)| self.elapsed >= *until)
        {
            self.saying = None;
            if let Some(t) = crate::hit::find_mut(root, "Stage.Subtitle")
                && let Kind::Text(run) = &mut t.kind
            {
                run.set_text("");
            }
        }
        let vp = cx.viewport().screen;
        if [vp.width, vp.height] != self.screen {
            *root = self.tree(cx);
        }
        self.place(root);
    }

    fn animating(&self) -> bool {
        self.busy() || self.saying.is_some()
    }

    /// The bands and the bezel move as it comes in and goes.
    fn moving(&self) -> bool {
        self.shown < 1.0
    }

    fn message(&mut self, m: &Message, root: &mut Actor, _cx: &mut Cx) {
        match m.name.as_str() {
            CAPTION => {
                self.caption = text_of(m, "Text");
                if let Some(t) = crate::hit::find_mut(root, "Stage.Caption")
                    && let Kind::Text(run) = &mut t.kind
                {
                    run.set_text(&self.caption);
                }
            }
            UNTIL => {
                if let Some(s) = seconds_of(m) {
                    self.until = Some(self.elapsed + s.max(0.0));
                }
            }
            SAY => {
                let text = text_of(m, "Text");
                let until = self.elapsed + seconds_of(m).unwrap_or(2.0).max(0.5);
                if let Some(t) = crate::hit::find_mut(root, "Stage.Subtitle")
                    && let Kind::Text(run) = &mut t.kind
                {
                    run.set_text(&text);
                }
                self.saying = Some((text, until));
            }
            END => self.going = true,
            _ => return,
        }
        self.place(root);
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Pointer {
                target: Some(t),
                gesture: crate::Gesture::Tap,
                ..
            } if t.within("Stage.Back") && !self.going => {
                (self.back)(cx);
                Handled::Yes
            }
            // The rest of the frame takes its pointers: nothing under it
            // (the studio, covered) is pressed through it.
            Input::Pointer { .. } => Handled::Yes,
            _ => Handled::No,
        }
    }
}

/// What the shell draws each frame filling the screen, a texture under
/// `source` (a staged play, drawn as on the whole screen): the screen the
/// frame shows it in.
pub struct Picture {
    source: String,
    screen: [f32; 2],
}

impl Picture {
    pub fn new(source: &str) -> Picture {
        Picture {
            source: source.into(),
            screen: [0.0; 2],
        }
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        let [w, h] = self.screen;
        let mut picture = quad("Picture", [w, h], [1.0; 4]);
        let texture = cx.texture(&self.source, w.max(h));
        if let Kind::Sprite(s) = &mut picture.kind {
            s.texture = texture;
        }
        call(&mut picture, "xy", &[w / 2.0, h / 2.0]);
        frame("", vec![picture])
    }
}

impl Screen for Picture {
    fn name(&self) -> &str {
        "picture"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.tree(cx)
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        let vp = cx.viewport().screen;
        if [vp.width, vp.height] != self.screen {
            *root = self.tree(cx);
        }
    }
}

#[cfg(test)]
mod tests;
