//! Results (`docs/plans/theme.md`, drawn in layout A;
//! `docs/plans/ui-core.md`, step 6.1):
//!
//! - **Left**: the song and the chart, the mods, the grade, the score
//!   (money, percent or "—", as the ruleset scores) and EX where the
//!   ruleset has it (`Results.Score`), the lamp or FAILED, the ruleset's
//!   tiers with bars (`Results.Judgement`; tiers it does not name are
//!   left out), then holds (under the ruleset's names), hold ticks, mines,
//!   stray presses, max combo, fast and slow, the mean and σ.
//! - **Right**: the main set of the panels the ruleset offers
//!   ([`super::data::PanelSet`]; the other sets wait for a picker): the
//!   timing histogram colored by the
//!   ruleset's windows, life over the song with the gauge's danger line,
//!   offsets over the song with misses marked, and hits short of the best
//!   tier by lane.
//! - **Bottom**: the song offset editor (a millisecond either way, this
//!   play's mean error when it is worth taking, reset), then Retry and
//!   Song list, plain buttons both.
//! - **Keys**: Start activates the focused button (Song list at first,
//!   as the mockup and StepMania's evaluation screen have it), Back
//!   (Escape, Backspace, a pad's Back) goes to the songs too; `R` and a
//!   pad's Select (Y; not Tab) retry; Left and Right move the focus
//!   along the bar's buttons. A quit play's grade is QUIT.
//!   Menu input and taps in the first [`SETTLE`] seconds, and repeats of
//!   held keys, are let go. The footer says notes worth saying
//!   (a device changed, the videos stopped).
//!
//! What the buttons do is the shell's ([`ResultsActions`]); without them
//! (the developer route) Song list leaves the screen.
//!
//! Replays (`docs/plans/play-history.md`): "Watch replay" (W) where the
//! shell offers it; a replay's results ([`ResultsKind::Replay`]) offer
//! "Watch again" for Retry, a kept play's ([`ResultsKind::Kept`]) its
//! replay and no Retry; both are marked REPLAY.

use ddi_engine::MenuInput as HintAction;
use std::rc::Rc;

use ddi_actor::{Actor, Kind, Sprite};

use super::data::{PanelKind, ResultsData, ResultsKind};
use super::{Backdrop, Env, footer, frame, ground, header, left, right};
use crate::graph;
use crate::hit;
use crate::look::{self, Spec, call, named, quad};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::widgets::{part, set_image, set_text};

/// What Retry or Song list does (the play route: play again, go to the
/// songs).
pub type ActionFn = Rc<dyn Fn(&Env, &mut Cx)>;
/// The song offset set (seconds).
pub type OffsetFn = Rc<dyn Fn(f64)>;

/// The shell's part of the screen.
#[derive(Clone, Default)]
pub struct ResultsActions {
    pub retry: Option<ActionFn>,
    pub song_list: Option<ActionFn>,
    /// Watch the play's replay (offered once the play is kept).
    pub replay: Option<ActionFn>,
    pub offset: Option<OffsetFn>,
}

/// The life panel's largest drop is the largest fall within this many
/// seconds (the maintainer's choice: one bad burst, not the lowest point).
pub const DROP_WINDOW: f32 = 4.0;

/// The offset editor's buttons focus moves through after the bar's.
const OFFSET_BUTTONS: [&str; 4] = ["OffsetDown", "OffsetUp", "UseMean", "ResetOffset"];

/// Menu input in the first this many seconds is let go: keys and pads
/// still held or mashed as the song ends must not leave or retry.
pub const SETTLE: f32 = 0.6;

pub struct ResultsScreen {
    /// The header's wordmark's clock.
    live: super::Live,
    env: Env,
    data: ResultsData,
    /// Seconds shown.
    age: f32,
    backdrop: Backdrop,
    screen: [f32; 2],
    actions: ResultsActions,
    /// On a phone: one column (two sideways) that scrolls between a bar
    /// on top and the buttons at the bottom; the zoom from the layout
    /// to the screen (upright, 480 wide).
    compact: bool,
    zoom: f32,
    scroll: f32,
    /// The compact content's height, as last built.
    content_h: f32,
    /// The banner's texture, kept for the content built again.
    banner: Option<u32>,
}

impl ResultsScreen {
    pub fn new(env: Env, data: ResultsData, actions: ResultsActions) -> ResultsScreen {
        ResultsScreen {
            live: Default::default(),
            env,
            data,
            age: 0.0,
            backdrop: Backdrop::with_alpha("Backdrop.Results"),
            screen: super::DESIGN,
            actions,
            compact: false,
            zoom: 1.0,
            scroll: 0.0,
            content_h: 0.0,
            banner: None,
        }
    }

    /// The layout's size and zoom ([`super::compact_fit`]).
    fn place(&mut self, vp: ddi_actor::screen::VirtualScreen) -> bool {
        let (screen, zoom, compact) = super::compact_fit(&self.env, vp);
        let changed = (screen, zoom, compact) != (self.screen, self.zoom, self.compact);
        self.screen = screen;
        self.zoom = zoom;
        self.compact = compact;
        changed
    }

    /// The root for this layout: built, zoomed.
    fn make_root(&mut self, cx: &mut Cx) -> Actor {
        self.banner = self
            .data
            .song
            .banner
            .as_deref()
            .map(|b| cx.texture(b, self.env.metric("Results.Score.Width", 400.0)));
        let mut root = if self.compact {
            self.build_compact(cx)
        } else {
            self.build_tree(cx)
        };
        if self.compact && self.zoom != 1.0 {
            call(&mut root, "zoom", &[self.zoom]);
        }
        root
    }

    /// The compact parts: the bar's height on top, the buttons' at the
    /// bottom, the margin.
    fn compact_bars(&self) -> (f32, f32, f32) {
        let upright = self.screen[0] < self.screen[1];
        (52.0, if upright { 112.0 } else { 60.0 }, 12.0)
    }

    /// A phone's results: the bar (back, the title, the panel sets), the
    /// content (the score column, then the panels, one under another;
    /// side by side when sideways) scrolling, the buttons.
    fn build_compact(&mut self, cx: &mut Cx) -> Actor {
        let env = self.env.clone();
        let screen = self.screen;
        let [w, h] = screen;
        let (top, bottom, m) = self.compact_bars();
        let mut children = vec![ground(&env, screen), self.backdrop.build(&env, screen)];
        let view = [m, top, w - 2.0 * m, h - top - bottom];
        let (mut content, content_h) = self.compact_content(view[2]);
        self.content_h = content_h;
        call(&mut content, "y", &[-self.scroll]);
        let mut clip = frame("ResultsClip", vec![content]);
        clip.clip = Some([-4.0, 0.0, view[2] + 4.0, view[3]]);
        call(&mut clip, "xy", &[view[0], view[1]]);
        children.push(clip);
        children.push(env.component(
            "IconButton",
            "Button",
            &Spec::new([40.0, 40.0], "‹"),
            "Back",
            [m + 20.0, top / 2.0],
        ));
        children.push(super::more_button(&env, [m + 84.0, top / 2.0]));
        if let Some(b) = self.top_replay([m + 130.0, top / 2.0]) {
            children.push(b);
        }

        children.push(self.bottom_bar([m, h - bottom, w - 2.0 * m, bottom]));
        env.state.borrow_mut().hints = Vec::new();
        self.finish_root(children, cx)
    }

    /// The root over `children`, entering, the song's backdrop on it.
    fn finish_root(&mut self, children: Vec<Actor>, cx: &mut Cx) -> Actor {
        let env = self.env.clone();
        let mut root = Actor::new(Kind::Frame(children));
        // The header's row and the tabs from above, the footer and the
        // buttons from below, the score from the left, the panels from
        // the right, the backdrop fading.
        {
            use super::Edge::{Bottom, Left, None, Right, Top};
            super::transitions(
                &mut root,
                &env,
                &[
                    ("Header", Top),
                    ("Back", Top),
                    ("Title", Top),
                    ("WatchReplay", Top),
                    ("Footer", Bottom),
                    ("Bottom", Bottom),
                    ("ScoreColumn", Left),
                    ("Panels", Right),
                    ("ResultsClip", Right),
                    ("Backdrop", None),
                ],
            );
        }
        if let Some(src) = self.data.song.backdrop().map(str::to_string) {
            self.backdrop.set(Some(&src), &mut root, cx, &env);
            // At once: the backdrop fades in with the screen, not after
            // it.
            if let Some(b) = hit::find_mut(&mut root, "Backdrop") {
                b.finish_tweening();
                for c in b.children_mut() {
                    c.finish_tweening();
                }
            }
        }
        root
    }

    /// The scrolling content `w` wide, and its height.
    fn compact_content(&self, w: f32) -> (Actor, f32) {
        let env = &self.env;
        let sideways = self.screen[0] >= self.screen[1];
        let gap = 16.0;
        let score_w = if sideways { (w * 0.44).min(380.0) } else { w };
        let (score, score_h) = self.score_column_sized([0.0, 0.0, score_w]);
        let mut children = vec![score];
        let kinds = self
            .data
            .panel_sets
            .get(self.data.panel_set)
            .map(|s| s.panels.clone())
            .unwrap_or_default();
        let (px, pw, mut py) = if sideways {
            (score_w + gap, w - score_w - gap, 0.0)
        } else {
            (0.0, w, score_h + gap)
        };
        let ph = if sideways { 190.0 } else { 200.0 };
        for kind in kinds {
            children.push(self.panel_of(kind, [px, py, pw, ph]));
            py += ph + gap;
        }
        let mut end = py.max(score_h);
        // What is worth saying, under it all.
        let mut notes = self.data.notes.clone();
        if !self.data.note.is_empty() {
            notes.push(self.data.note.clone());
        }
        for (i, line) in notes
            .iter()
            .flat_map(|n| super::select_dialogs::wrap_lines(n, w, 13.0))
            .enumerate()
        {
            let t = env.text(&format!("Note{i}"), &line, 13.0, 400, "Muted");
            children.push(left(t, 0.0, end + 10.0));
            end += 18.0;
        }
        let height = end + 16.0;
        (frame("ResultsContent", children), height)
    }

    /// How far the content scrolls: its height (as last built; it
    /// changes only with what builds it again) past the view's.
    fn max_scroll(&self) -> f32 {
        let (top, bottom, _) = self.compact_bars();
        (self.content_h - (self.screen[1] - top - bottom)).max(0.0)
    }

    fn scroll_by(&mut self, dy: f32, root: &mut Actor) {
        if !self.compact {
            return;
        }
        let to = (self.scroll + dy).clamp(0.0, self.max_scroll());
        if to != self.scroll {
            self.scroll = to;
            if let Some(c) = hit::find_mut(root, "ResultsContent") {
                call(c, "y", &[-self.scroll]);
            }
            self.env.state.borrow_mut().scroll = self.scroll;
        }
    }

    /// The data shown (the offset and the panels as changed).
    pub fn data(&self) -> &ResultsData {
        &self.data
    }

    /// The design's places: the score column's left, the panels' rect, the
    /// bottom bar's top.
    fn places(&self) -> ([f32; 3], [f32; 4], f32) {
        let env = &self.env;
        let [w, h] = self.screen;
        let design_w = env.metric("Design.Width", 1280.0);
        let panels_x = w - (design_w - env.metric("Results.Panels.X", 464.0));
        let panels_w = env.metric("Results.Panels.Width", 784.0);
        let bar_y = h - (env.metric("Design.Height", 720.0) - env.metric("Results.Bar.Y", 584.0));
        (
            [
                env.metric("Margin", 32.0),
                84.0,
                env.metric("Results.Score.Width", 400.0),
            ],
            [panels_x, 84.0, panels_w, bar_y - 24.0 - 84.0],
            bar_y,
        )
    }

    fn build_tree(&mut self, cx: &mut Cx) -> Actor {
        let env = self.env.clone();
        let screen = self.screen;
        let (left_col, panels, bar_y) = self.places();
        let mut children = vec![ground(&env, screen), self.backdrop.build(&env, screen)];
        children.push(self.score_column(left_col, cx));
        children.push(self.panels_actor(panels));
        children.push(self.bottom_bar([panels[0], bar_y, panels[2], 76.0]));
        children.push(header(&env, cx, screen[0], "RESULTS"));
        children.push(super::more_button(&env, [screen[0] - 64.0, 32.0]));
        if let Some(b) = self.top_replay([panels[0], 32.0]) {
            children.push(b);
        }
        let mut note = self.data.notes.join(" · ");
        if !self.data.note.is_empty() {
            if !note.is_empty() {
                note.push_str(" · ");
            }
            note.push_str(&self.data.note);
        }
        let mut hints: Vec<(&[&str], &str)> = vec![(&["ENTER"], "Song list")];
        match self.data.kind {
            ResultsKind::Play => hints.push((&["R"], "Retry")),
            ResultsKind::Replay => hints.push((&["R"], "Watch again")),
            ResultsKind::Kept => {}
        }
        if self.actions.replay.is_some() && self.data.kind != ResultsKind::Replay {
            hints.push((&["W"], "Watch replay"));
        }
        let mut controller: Vec<(&[HintAction], &str)> = vec![
            (&[HintAction::Confirm, HintAction::Start], "Activate"),
            (&[HintAction::More], "More"),
            (&[HintAction::Back], "Song list"),
        ];
        controller.push((
            &[HintAction::Select],
            if self.data.kind == ResultsKind::Kept {
                "Watch replay"
            } else {
                "Retry"
            },
        ));
        let (f, texts) = footer(&env, screen, &hints, &controller, &note, &[], cx);
        env.state.borrow_mut().hints = texts;
        children.push(f);
        self.finish_root(children, cx)
    }

    fn score_column(&self, [x, y, w]: [f32; 3], _cx: &mut Cx) -> Actor {
        self.score_column_sized([x, y, w]).0
    }

    /// The score column at `[x, y]`, `w` wide, and its height.
    fn score_column_sized(&self, [x, y, w]: [f32; 3]) -> (Actor, f32) {
        let env = &self.env;
        let d = &self.data;
        let mut children = Vec::new();
        // The banner, with its placeholder under it.
        let bh = (w * 100.0 / 400.0).round();
        let mut ph = quad("BannerPlaceholder", [w, bh], env.color("BannerEmpty"));
        call(&mut ph, "xy", &[w / 2.0, bh / 2.0]);
        call(&mut ph, "cornerradius", &[6.0]);
        let mut banner = named(Kind::Sprite(Sprite::quad([w, bh])), "Banner");
        call(&mut banner, "xy", &[w / 2.0, bh / 2.0]);
        call(&mut banner, "cornerradius", &[6.0]);
        let mut holder = frame("BannerHolder", vec![banner]);
        set_image(&mut holder, "Banner", self.banner);
        children.push(ph);
        children.push(holder);
        let cut = |mut a: Actor, w: f32| {
            if let Kind::Text(t) = &mut a.kind {
                t.max_width = w;
                t.ellipsis = true;
            }
            a
        };
        let mut ty = bh + 16.0;
        let title = if d.song.subtitle.is_empty() {
            d.song.title.clone()
        } else {
            format!("{} {}", d.song.title, d.song.subtitle)
        };
        children.push(left(
            cut(env.text("SongTitle", &title, 20.0, 700, "Text"), w),
            0.0,
            ty,
        ));
        ty += 22.0;
        let line = if d.song.artist.is_empty() {
            d.line.clone()
        } else {
            format!("{} · {}", d.song.artist, d.line)
        };
        children.push(left(
            cut(env.text("SongLine", &line, 14.0, 400, "Secondary"), w),
            0.0,
            ty,
        ));
        ty += 19.0;
        if !d.mods.is_empty() {
            children.push(left(
                cut(env.text("Mods", &d.mods, 13.0, 400, "Muted"), w),
                0.0,
                ty,
            ));
            ty += 18.0;
        }
        ty += 2.0;
        // Grade, score, EX.
        let mut score = env.component(
            "Results.Score",
            "Panel",
            &Spec::new([w, 66.0], ""),
            "ScoreBox",
            [0.0, ty],
        );
        set_text(&mut score, "Grade", &d.grade);
        // A quit's grade is QUIT, in the danger color.
        if d.quit
            && let Some(g) = part(&mut score, "Grade")
        {
            call(g, "diffuse", &env.color("Danger"));
        }
        // One run, the percent sign in it (how wide the number is is not
        // known here).
        set_text(&mut score, "Score", &d.score);
        if let Some(p) = part(&mut score, "Percent") {
            p.visible = false;
        }
        set_text(&mut score, "Title", &format!("{} {}", d.grade, d.score));
        // A long grade ("AAAA", "★★★★") smaller, the score after it; EX
        // goes on the line under them.
        // The grade's box sized to the grade (a letter about 0.58 em in
        // the bold face), no wider than 120; the score after it, made
        // smaller if it would pass the column.
        // Stars come from the system's fonts (the theme's face leaves
        // them out): about an em each, not a letter's 0.6.
        let full = d
            .grade
            .chars()
            .map(|c| {
                if matches!(c, '★' | '☆') {
                    64.0
                } else {
                    look::mono_width(&c.to_string(), 64.0) * 1.15
                }
            })
            .sum::<f32>();
        let zoom = (120.0 / full.max(1.0)).min(1.0);
        if let Some(g) = part(&mut score, "Grade") {
            call(g, "zoom", &[zoom]);
        }
        let grade_w = full * zoom;
        if let Some(sc) = part(&mut score, "Score") {
            let x = sc.current().pos[0].max(grade_w + 20.0);
            call(sc, "x", &[x]);
            // The numbers face: 0.6 em a figure.
            let score_w = d.score.chars().count() as f32 * 0.6 * 56.0;
            if score_w > w - x {
                call(sc, "zoom", &[(w - x) / score_w]);
            }
        }
        if let Some(ex) = part(&mut score, "Ex") {
            ex.visible = false;
        }
        children.push(score);
        ty += 66.0 + 10.0;
        // The lamp, FAILED.
        let mut marks = Vec::new();
        if d.kind != ResultsKind::Play {
            marks.push(("Replay", "REPLAY".to_string(), "Accent"));
        }
        if d.failed {
            marks.push(("Failed", "FAILED".to_string(), "Danger"));
        }
        if let Some(l) = &d.lamp {
            marks.push(("Lamp", l.clone(), "Gold"));
        }
        if !marks.is_empty() || d.ex.is_some() {
            let mut mx = 0.0;
            for (name, text, color) in marks {
                let t = look::tracked(env.text(name, &text, 13.0, 700, color), 80);
                children.push(left(t, mx, ty));
                mx += look::mono_width(&text, 13.0) * 1.1 + 16.0;
            }
            if let Some(ex) = &d.ex {
                children.push(right(env.numbers("Ex", ex, 14.0, "Muted"), w, ty));
            }
            ty += 22.0;
        }
        // Judgements.
        let max = d
            .judgements
            .iter()
            .map(|j| j.count)
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        let row_h = if d.judgements.len() > 6 { 22.0 } else { 26.0 };
        for (i, j) in d.judgements.iter().enumerate() {
            let mut row = env.component(
                "Results.Judgement",
                "List.Row",
                &Spec::new([w, row_h], &j.name),
                &format!("Judgement{i}"),
                [0.0, ty],
            );
            set_text(&mut row, "Name", &j.name);
            set_text(&mut row, "Title", &j.name);
            set_text(&mut row, "Count", &j.count.to_string());
            set_text(&mut row, "Subtitle", &j.count.to_string());
            crate::widgets::play(&mut row, &j.color);
            if let Some(b) = part(&mut row, "Bar") {
                call(b, "zoomx", &[j.count as f32 / max]);
                b.visible = j.count > 0;
            }
            row.finish_tweening();
            children.push(row);
            ty += row_h;
        }
        ty += 8.0;
        // The stats, two columns.
        let mut line = quad("StatsLine", [w, 1.0], env.color("Line"));
        call(&mut line, "xy", &[w / 2.0, ty]);
        children.push(line);
        ty += 14.0;
        // Two a row; one with a long value takes a row of its own.
        let (mut col, mut yy) = (0, ty);
        for (i, (label, value)) in d.stats.iter().enumerate() {
            let wide = value.chars().count() > 9;
            if wide && col == 1 {
                col = 0;
                yy += 20.0;
            }
            let (x0, cw) = if wide {
                (0.0, w)
            } else {
                (col as f32 * (w / 2.0 + 8.0), w / 2.0 - 8.0)
            };
            children.push(left(
                cut(
                    env.text(&format!("Stat{i}"), label, 13.0, 400, "Muted"),
                    if wide { w * 0.45 } else { cw - 70.0 },
                ),
                x0,
                yy,
            ));
            children.push(right(
                env.numbers(&format!("StatValue{i}"), value, 13.0, "Text"),
                x0 + cw,
                yy,
            ));
            if wide || col == 1 {
                col = 0;
                yy += 20.0;
            } else {
                col = 1;
            }
        }
        let height = if col == 1 { yy + 20.0 } else { yy };
        let mut f = frame("ScoreColumn", children);
        call(&mut f, "xy", &[x, y]);
        (f, height)
    }

    /// A panel with its title and note.
    fn panel(
        &self,
        name: &str,
        title: &str,
        note: &str,
        rect: [f32; 4],
        content: Vec<Actor>,
    ) -> Actor {
        let [x, y, w, h] = rect;
        let mut p = self.env.component(
            "Panel",
            "Panel",
            &Spec::new([w, h], title),
            &format!("Panel.{name}"),
            [x, y],
        );
        set_text(&mut p, "Note", note);
        if let Kind::Frame(c) = &mut p.kind {
            c.extend(content);
        }
        p
    }

    /// The chosen set's panels: two side by side a row, the last of an
    /// odd count across.
    fn panels_actor(&self, [x, y, w, h]: [f32; 4]) -> Actor {
        let gap = 16.0;
        let kinds = self
            .data
            .panel_sets
            .get(self.data.panel_set)
            .map(|s| s.panels.clone())
            .unwrap_or_default();
        let mut children = Vec::new();
        let n = kinds.len();
        let rects: Vec<[f32; 4]> = match n {
            0 => Vec::new(),
            1 => vec![[x, y, w, h]],
            2 => {
                let top = (h - gap) * 0.62;
                vec![[x, y, w, top], [x, y + top + gap, w, h - top - gap]]
            }
            _ => {
                let rows = n.div_ceil(2);
                let (pw, ph) = ((w - gap) / 2.0, (h - gap * (rows - 1) as f32) / rows as f32);
                (0..n)
                    .map(|i| {
                        let (c, r) = ((i % 2) as f32, (i / 2) as f32);
                        let wide = i == n - 1 && n % 2 == 1;
                        [
                            x + c * (pw + gap),
                            y + r * (ph + gap),
                            if wide { w } else { pw },
                            ph,
                        ]
                    })
                    .collect()
            }
        };
        for (kind, rect) in kinds.into_iter().zip(rects) {
            children.push(self.panel_of(kind, rect));
        }
        frame("Panels", children)
    }

    /// A panel of `kind` in `rect`.
    fn panel_of(&self, kind: PanelKind, rect: [f32; 4]) -> Actor {
        match kind {
            PanelKind::Timing => self.timing_panel(rect, rect[2] > 600.0),
            PanelKind::Life => self.life_panel(rect),
            PanelKind::Offsets => self.offsets_panel(rect),
            PanelKind::Lanes => self.lanes_panel(rect),
            PanelKind::Beats => self.beats_panel(rect),
        }
    }

    /// How far either way the timing panels reach (ms): the hits' spread,
    /// at least twice the best window, at most the widest.
    fn range(&self) -> f32 {
        let widest = self
            .data
            .windows
            .iter()
            .map(|w| w.early.max(w.late))
            .fold(0.0f32, f32::max);
        let best = self
            .data
            .windows
            .first()
            .map_or(22.0, |w| w.early.max(w.late));
        let mut spread: Vec<f32> = self
            .data
            .hits
            .iter()
            .filter_map(|h| h.offset.map(f32::abs))
            .collect();
        spread.sort_by(f32::total_cmp);
        let p98 = spread
            .get(spread.len().saturating_sub(1) * 98 / 100)
            .copied()
            .unwrap_or(0.0);
        let r = p98.max(best * 2.0);
        let r = if widest > 0.0 { r.min(widest) } else { r };
        (r / 5.0).ceil().max(2.0) * 5.0
    }

    /// The timing histogram, bins colored by the ruleset's windows, the
    /// best window's band behind them.
    fn timing_panel(&self, rect: [f32; 4], large: bool) -> Actor {
        let env = &self.env;
        let [_, _, w, h] = rect;
        let (mean, sd) = self.data.timing();
        let note = format!("mean {} ms · σ {sd:.1}", signed(mean, 1));
        let inner = [14.0, 40.0, w - 28.0, h - 40.0 - 30.0];
        let range = self.range();
        // An odd count: a bin centred on 0, so a perfect play's bar
        // stands on the axis.
        let bins = if large { 61 } else { 31 };
        let bin = 2.0 * range / bins as f32;
        let offsets: Vec<f32> = self.data.hits.iter().filter_map(|h| h.offset).collect();
        let counts = graph::histogram(&offsets, [-range, range], bins);
        let max = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
        let [ix, iy, iw, ih] = inner;
        let mut c = Vec::new();
        let px = |ms: f32| ix + iw * ((ms + range) / (2.0 * range)).clamp(0.0, 1.0);
        // The best window's band.
        if let Some(win) = self.data.windows.first() {
            let (a, b) = (px(-win.early), px(win.late));
            let mut band = quad("Window", [b - a, ih], env.color("Window"));
            call(&mut band, "xy", &[(a + b) / 2.0, iy + ih / 2.0]);
            c.push(band);
        }
        // The zero line behind the bars, so a bar on it is not cut.
        c.push(graph::segment(
            [px(0.0), iy],
            [px(0.0), iy + ih],
            1.0,
            env.color("Graph.Axis"),
        ));
        let slot = iw / bins as f32;
        for (i, &n) in counts.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let centre = -range + bin * (i as f32 + 0.5);
            let color = self
                .data
                .windows
                .iter()
                .find(|w| {
                    if centre < 0.0 {
                        -centre <= w.early
                    } else {
                        centre <= w.late
                    }
                })
                .map_or("Judgement.Miss", |w| w.color.as_str());
            let bh = n as f32 / max * ih;
            let mut q = quad("", [(slot - 1.0).max(1.0), bh], env.color(color));
            call(
                &mut q,
                "xy",
                &[ix + slot * (i as f32 + 0.5), iy + ih - bh / 2.0],
            );
            c.push(q);
        }
        c.push(left(
            env.text(
                "Early",
                &format!("−{range:.0} ms early"),
                12.0,
                400,
                "Faint",
            ),
            ix,
            iy + ih + 14.0,
        ));
        c.push(right(
            env.text("Late", &format!("late +{range:.0} ms"), 12.0, 400, "Faint"),
            ix + iw,
            iy + ih + 14.0,
        ));
        self.panel("Timing", "Timing", &note, rect, c)
    }

    /// The song's length, at least the last thing that happened.
    fn length(&self) -> f32 {
        let last_hit = self.data.hits.last().map_or(0.0, |h| h.time);
        let last_life = self.data.life.last().map_or(0.0, |l| l.0);
        self.data.song.length.max(last_hit).max(last_life).max(1.0)
    }

    fn life_panel(&self, rect: [f32; 4]) -> Actor {
        let env = &self.env;
        let [_, _, w, h] = rect;
        // The largest fall in a few seconds: a bad burst, even where life
        // came back after it (a lowest would mostly say where it began).
        let drop = self
            .data
            .steepest_drop(DROP_WINDOW)
            .filter(|&(d, _, _)| d >= 0.005);
        let note = match drop {
            Some((d, from, _)) => format!(
                "largest drop {:.0}% at {}",
                d * 100.0,
                super::data::clock(from)
            ),
            None => "no drop".to_string(),
        };
        let len = self.length();
        let inner = [14.0, 40.0, w - 28.0, h - 40.0 - 30.0];
        let [ix, iy, iw, ih] = inner;
        // Where it fell, faintly behind the line.
        let mut band = Vec::new();
        if let Some((_, from, to)) = drop {
            let px = |t: f32| ix + iw * (t / len).clamp(0.0, 1.0);
            let (a, b) = (px(from), px(to).max(px(from) + 2.0));
            let mut color = env.color("Danger");
            color[3] *= 0.18;
            let mut q = quad("Life.Drop", [b - a, ih], color);
            call(&mut q, "xy", &[(a + b) / 2.0, iy + ih / 2.0]);
            band.push(q);
        }
        // As steps: life holds until it changes.
        let mut steps = Vec::with_capacity(self.data.life.len() * 2 + 1);
        for (i, &(t, v)) in self.data.life.iter().enumerate() {
            if i > 0 {
                steps.push((t, self.data.life[i - 1].1));
            }
            steps.push((t, v));
        }
        if let Some(&(_, v)) = self.data.life.last() {
            steps.push((len, v));
        }
        let mut c = band;
        c.push(graph::line_graph(
            "Life.Graph",
            &*env.look,
            &steps,
            [0.0, len],
            [0.0, 1.0],
            inner,
        ));
        // The gauge's danger line, dashed.
        if let Some(d) = self.data.danger.filter(|d| *d > 0.0 && *d < 1.0) {
            let dy = iy + ih * (1.0 - d);
            let mut x = ix;
            while x < ix + iw {
                c.push(graph::segment(
                    [x, dy],
                    [(x + 6.0).min(ix + iw), dy],
                    1.0,
                    env.color("Danger"),
                ));
                x += 10.0;
            }
        }
        c.push(left(
            env.numbers("LifeStart", "0:00", 12.0, "Faint"),
            ix,
            iy + ih + 14.0,
        ));
        c.push(right(
            env.numbers("LifeEnd", &super::data::clock(len), 12.0, "Faint"),
            ix + iw,
            iy + ih + 14.0,
        ));
        self.panel("Life", "Life", &note, rect, c)
    }

    /// Each hit's offset over the song (early above), misses as lines.
    fn offsets_panel(&self, rect: [f32; 4]) -> Actor {
        let env = &self.env;
        let [_, _, w, h] = rect;
        let len = self.length();
        let [ix, iy, iw, ih] = [14.0, 40.0, w - 28.0, h - 40.0 - 14.0];
        let range = self.range();
        let mut c = vec![graph::segment(
            [ix, iy + ih / 2.0],
            [ix + iw, iy + ih / 2.0],
            1.0,
            env.color("Graph.Axis"),
        )];
        let mut pts = Vec::new();
        let mut misses = 0;
        for hit in &self.data.hits {
            let x = ix + (hit.time / len).clamp(0.0, 1.0) * iw;
            match hit.offset {
                Some(o) => pts.push([x, iy + ih / 2.0 + (o / range).clamp(-1.0, 1.0) * ih / 2.0]),
                None => {
                    misses += 1;
                    c.push(graph::segment(
                        [x, iy],
                        [x, iy + ih],
                        1.5,
                        env.color("Judgement.Miss"),
                    ));
                }
            }
        }
        c.push(graph::points(
            "Offsets.Points",
            &pts,
            2.0,
            env.color("Graph.Point"),
        ));
        let note = match misses {
            0 => format!("early above · ±{range:.0} ms"),
            1 => "early above · 1 miss".to_string(),
            n => format!("early above · {n} misses"),
        };
        self.panel("Offsets", "Offsets over the song", &note, rect, c)
    }

    /// Per lane, the hits short of the best tier.
    fn lanes_panel(&self, rect: [f32; 4]) -> Actor {
        let env = &self.env;
        let [_, _, w, h] = rect;
        let best = self.data.windows.first();
        let lanes = self.data.lanes.len().max(1);
        let mut counts = vec![0u32; lanes];
        let mut sums = vec![0f32; lanes];
        for hit in &self.data.hits {
            let l = usize::from(hit.lane).min(lanes - 1);
            match hit.offset {
                Some(o) if best.is_none_or(|b| o < -b.early || o > b.late) => {
                    counts[l] += 1;
                    sums[l] += o;
                }
                None => counts[l] += 1,
                _ => {}
            }
        }
        let max = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
        let [ix, iy, iw, ih] = [14.0, 40.0, w - 28.0, h - 40.0 - 44.0];
        let slot = iw / lanes as f32;
        // Room for a name and a mean under each lane, else the symbol and
        // the count.
        let roomy = slot >= 84.0;
        let mut c = Vec::new();
        for l in 0..lanes {
            let bh = counts[l] as f32 / max * ih;
            let cxl = ix + slot * (l as f32 + 0.5);
            if counts[l] > 0 {
                let mut q = quad("", [slot * 0.5, bh], env.color("Graph.Bar"));
                call(&mut q, "xy", &[cxl, iy + ih - bh / 2.0]);
                call(&mut q, "cornerradius", &[2.0]);
                c.push(q);
            }
            let lane = self.data.lanes.get(l).cloned().unwrap_or_default();
            let name = if roomy {
                format!("{} {}", lane.symbol, lane.label)
            } else {
                lane.symbol.clone()
            };
            let mut t = env.text(&format!("Lane{l}"), &name, 13.0, 600, "Secondary");
            if let Kind::Text(tx) = &mut t.kind {
                tx.max_width = slot - 4.0;
                tx.ellipsis = true;
            }
            call(&mut t, "xy", &[cxl, iy + ih + 14.0]);
            c.push(t);
            let note = if roomy {
                let mean = if counts[l] > 0 {
                    sums[l] / counts[l] as f32
                } else {
                    0.0
                };
                format!("{} · {} ms", counts[l], signed(mean, 0))
            } else {
                counts[l].to_string()
            };
            let mut n = env.numbers(&format!("LaneNote{l}"), &note, 12.0, "Muted");
            call(&mut n, "xy", &[cxl, iy + ih + 32.0]);
            c.push(n);
        }
        let best_name = self
            .data
            .judgements
            .first()
            .map_or("the best", |j| j.name.as_str());
        self.panel("Lanes", &format!("By lane · not {best_name}"), "", rect, c)
    }

    /// The hits by their notes' places in the beat: a row each (the rare
    /// ones together), its count, its mean and spread on the timing
    /// panels' scale, the best window behind them, and the numbers.
    fn beats_panel(&self, rect: [f32; 4]) -> Actor {
        let env = &self.env;
        let [_, _, w, h] = rect;
        let [ix, iy, iw, ih] = [14.0, 40.0, w - 28.0, h - 40.0 - 12.0];
        let row = 26.0;
        let rows = ((ih / row).floor() as usize).clamp(1, 7);
        let known = self.data.hits.iter().filter(|h| h.beat.is_some()).count() as u32;
        let groups = self.data.beat_groups(rows, (known / 50).max(5));
        let mut c = Vec::new();
        if groups.is_empty() {
            let why = if self.data.hits.is_empty() {
                "No notes were judged."
            } else {
                "This play was kept before notes' places in the beat were."
            };
            let mut t = env.text("Beats.Empty", why, 13.0, 400, "Muted");
            if let Kind::Text(tx) = &mut t.kind {
                tx.max_width = iw;
                tx.ellipsis = true;
            }
            c.push(left(t, ix, iy + ih / 2.0));
            return self.panel("Beats", "By beat", "", rect, c);
        }
        let range = self.range();
        let (track_x, track_w) = (ix + 104.0, (iw - 104.0 - 104.0).max(40.0));
        let px = |ms: f32| track_x + track_w * ((ms + range) / (2.0 * range)).clamp(0.0, 1.0);
        // The rows centered in the room, the best window's band and the
        // zero line behind them all.
        let top = iy + (ih - row * groups.len() as f32) / 2.0;
        let span = row * groups.len() as f32;
        if let Some(win) = self.data.windows.first() {
            let (a, b) = (px(-win.early), px(win.late));
            let mut band = quad("Window", [b - a, span], env.color("Window"));
            call(&mut band, "xy", &[(a + b) / 2.0, top + span / 2.0]);
            c.push(band);
        }
        c.push(graph::segment(
            [px(0.0), top],
            [px(0.0), top + span],
            1.0,
            env.color("Graph.Axis"),
        ));
        for (i, g) in groups.iter().enumerate() {
            let y = top + row * (i as f32 + 0.5);
            c.push(left(
                env.text(&format!("Beat{i}"), &g.name, 13.0, 600, "Secondary"),
                ix,
                y,
            ));
            c.push(right(
                env.numbers(
                    &format!("BeatCount{i}"),
                    &g.count.to_string(),
                    12.0,
                    "Muted",
                ),
                track_x - 12.0,
                y,
            ));
            if g.count > g.misses {
                // The spread (a standard deviation either way), the mean
                // across it.
                let (a, b) = (
                    px(g.mean - g.sd),
                    px(g.mean + g.sd).max(px(g.mean - g.sd) + 2.0),
                );
                let mut color = env.color("Graph.Point");
                color[3] *= 0.45;
                let mut spread = quad("", [b - a, 6.0], color);
                call(&mut spread, "xy", &[(a + b) / 2.0, y]);
                call(&mut spread, "cornerradius", &[3.0]);
                c.push(spread);
                let mut mean = quad("", [2.0, 14.0], env.color("Graph.Point"));
                call(&mut mean, "xy", &[px(g.mean), y]);
                c.push(mean);
            }
            let stats = if g.count > g.misses {
                format!("{} · σ {:.1}", signed(g.mean, 1), g.sd)
            } else {
                "—".to_string()
            };
            c.push(right(
                env.numbers(&format!("BeatStats{i}"), &stats, 12.0, "Muted"),
                ix + iw,
                y,
            ));
        }
        self.panel("Beats", "By beat", "mean · σ ms", rect, c)
    }

    fn bottom_bar(&self, [x, y, w, h]: [f32; 4]) -> Actor {
        let env = &self.env;
        // Upright on a phone: the editor on a row of its own, the buttons
        // under it.
        let two_rows = self.compact && self.screen[0] < self.screen[1];
        let (mid, edit_mid) = if two_rows {
            (h * 0.75, h * 0.27)
        } else {
            (h / 2.0, h / 2.0)
        };
        let mut c = Vec::new();
        // Right to left: the main button, then the others.
        let mut right_edge = w;
        for (name, label, kind) in self.bar_buttons().into_iter().rev() {
            let bw = (look::mono_width(label, 15.0) + 40.0).max(112.0);
            c.push(env.component(
                kind,
                "Button",
                &Spec::new([bw, 48.0], label),
                name,
                [right_edge - bw / 2.0, mid],
            ));
            right_edge -= bw + 12.0;
        }
        if let Some(o) = &self.data.offset {
            c.push(left(
                env.text(
                    "OffsetLabel",
                    if self.compact {
                        "Offset"
                    } else {
                        "Song offset"
                    },
                    15.0,
                    600,
                    "Secondary",
                ),
                0.0,
                edit_mid,
            ));
            let bx = if self.compact { 64.0 } else { 118.0 };
            c.push(env.component(
                "Button",
                "Button",
                &Spec::new([44.0, 44.0], "−1"),
                "OffsetDown",
                [bx + 22.0, edit_mid],
            ));
            let mut v = env.numbers(
                "OffsetValue",
                &format!("{} ms", signed((o.current * 1000.0) as f32, 0)),
                20.0,
                "Text",
            );
            call(&mut v, "xy", &[bx + 44.0 + 45.0, edit_mid]);
            c.push(v);
            c.push(env.component(
                "Button",
                "Button",
                &Spec::new([44.0, 44.0], "+1"),
                "OffsetUp",
                [bx + 134.0 + 22.0, edit_mid],
            ));
            let mut bx = bx + 178.0 + 12.0;
            // This play's mean as an offset, while it is not the offset
            // set (as the old results offered it).
            if let Some(s) = o.suggested.filter(|s| (s - o.current).abs() >= 0.001) {
                let label = format!("Use {} ms", signed((s * 1000.0) as f32, 0));
                let uw = look::mono_width(&label, 15.0) + 28.0;
                c.push(env.component(
                    "Button",
                    "Button",
                    &Spec::new([uw, 44.0], &label),
                    "UseMean",
                    [bx + uw / 2.0, edit_mid],
                ));
                bx += uw + 12.0;
            }
            if o.current != 0.0 {
                let rw = look::mono_width("Reset", 15.0) + 28.0;
                c.push(env.component(
                    "Button",
                    "Button",
                    &Spec::new([rw, 44.0], "Reset"),
                    "ResetOffset",
                    [bx + rw / 2.0, edit_mid],
                ));
            }
        }
        let mut f = frame("Bottom", c);
        call(&mut f, "xy", &[x, y]);
        f
    }

    /// A play's "Watch replay" goes in the bar only on an upright phone
    /// (its own row): elsewhere the offset editor needs the bar's room,
    /// and it goes on top, by the title ([`Self::top_replay`]).
    fn replay_in_bar(&self) -> bool {
        self.data.kind != ResultsKind::Play || (self.compact && self.screen[0] < self.screen[1])
    }

    /// A play's "Watch replay" on top, where the bar has no room for it.
    fn top_replay(&self, at: [f32; 2]) -> Option<Actor> {
        if self.replay_in_bar() || self.actions.replay.is_none() {
            return None;
        }
        let label = "Watch replay";
        let bw = look::mono_width(label, 15.0) + 40.0;
        Some(self.env.component(
            "Button",
            "Button",
            &Spec::new([bw, 40.0], label),
            "WatchReplay",
            [at[0] + bw / 2.0, at[1]],
        ))
    }

    /// The bar's buttons, left to right: name, label, component. Retry
    /// first, Song list last, all plain buttons (the accent's look read
    /// as a focus beside the focus outline on Song list).
    fn bar_buttons(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        let mut v = Vec::new();
        let replay = self.actions.replay.is_some();
        match self.data.kind {
            ResultsKind::Play => {
                v.push(("Retry", "Retry", "Button"));
                if replay && self.replay_in_bar() {
                    v.push(("WatchReplay", "Watch replay", "Button"));
                }
            }
            ResultsKind::Replay => v.push(("Retry", "Watch again", "Button")),
            ResultsKind::Kept => {
                if replay {
                    v.push(("WatchReplay", "Watch replay", "Button"));
                }
            }
        }
        v.push(("SongList", "Song list", "Button"));
        v
    }

    /// Left and Right: the focus along the bar's buttons (from anywhere
    /// else, to its first or last).
    fn bar_focus(&self, action: MenuAction, root: &Actor, cx: &mut Cx) {
        let names: Vec<&str> = self
            .bar_buttons()
            .into_iter()
            .map(|(n, _, _)| n)
            .filter(|n| hit::find(root, n).is_some())
            .collect();
        if names.is_empty() {
            return;
        }
        let left = matches!(action, MenuAction::Left | MenuAction::Previous);
        let next = match names.iter().position(|n| Some(*n) == cx.focused()) {
            Some(i) if left => i.saturating_sub(1),
            Some(i) => (i + 1).min(names.len() - 1),
            None if left => names.len() - 1,
            None => 0,
        };
        cx.focus(names[next]);
    }

    /// A new song offset (seconds): kept by the shell, the bar drawn
    /// again (its buttons depend on it).
    fn set_offset(&mut self, seconds: f64, root: &mut Actor, cx: &mut Cx) {
        let Some(o) = &mut self.data.offset else {
            return;
        };
        let seconds = ((seconds * 1000.0).round() / 1000.0).clamp(-1.0, 1.0);
        o.current = seconds;
        if let Some(f) = &self.actions.offset {
            f(seconds);
        }
        let fresh = if self.compact {
            let (_, bottom, m) = self.compact_bars();
            let [w, h] = self.screen;
            self.bottom_bar([m, h - bottom, w - 2.0 * m, bottom])
        } else {
            let (_, panels, bar_y) = self.places();
            self.bottom_bar([panels[0], bar_y, panels[2], 76.0])
        };
        if let Some(b) = hit::find_mut(root, "Bottom") {
            super::refill(b, fresh);
        }
        // Focus stays where it was, or on Retry when its button went.
        if let Some(f) = cx.focused()
            && hit::find(root, f).is_none()
        {
            cx.focus("Retry");
        }
        self.publish();
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "results".into();
        st.compact = self.compact;
        st.scroll = self.scroll;
        st.panels = self
            .data
            .panel_sets
            .get(self.data.panel_set)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        st.song_offset = self
            .data
            .offset
            .map_or(0, |o| (o.current * 1000.0).round() as i32);
        st.song = self.data.song.title.clone();
        st.backdrop = self.backdrop.source().map(str::to_string);
    }

    fn retry(&mut self, cx: &mut Cx) {
        // A kept play has no Retry: Start or Select there is its replay.
        if self.data.kind == ResultsKind::Kept {
            return self.watch_replay(cx);
        }
        self.env.state.borrow_mut().status = "retry".into();
        if let Some(r) = self.actions.retry.clone() {
            let env = self.env.clone();
            r(&env, cx);
        }
    }

    fn watch_replay(&mut self, cx: &mut Cx) {
        if let Some(r) = self.actions.replay.clone() {
            self.env.state.borrow_mut().status = "watch replay".into();
            let env = self.env.clone();
            r(&env, cx);
        }
    }

    fn open_more(&self, cx: &mut Cx) {
        let mut items: Vec<_> = self
            .bar_buttons()
            .into_iter()
            .map(|(id, label, _)| (id.into(), label.into()))
            .collect();
        if !self.replay_in_bar() && self.actions.replay.is_some() {
            items.push(("WatchReplay".into(), "Watch replay".into()));
        }
        if let Some(offset) = self.data.offset {
            items.extend([
                ("OffsetDown".into(), "Offset −1 ms".into()),
                ("OffsetUp".into(), "Offset +1 ms".into()),
                ("ResetOffset".into(), "Reset offset".into()),
            ]);
            if offset.suggested.is_some() {
                items.push(("UseMean".into(), "Use mean offset".into()));
            }
        }
        super::action_menu(&self.env, "Results", items, "Results.More", cx);
    }

    fn controller_focus(&mut self, action: MenuAction, root: &mut Actor, cx: &mut Cx) {
        let controls: Vec<_> = self
            .focusables()
            .into_iter()
            .filter(|f| hit::find(root, &f.name).is_some_and(|p| hit::shown(root, &p)))
            .collect();
        if controls.is_empty() {
            return;
        }
        let at = controls
            .iter()
            .position(|f| Some(f.name.as_str()) == cx.focused())
            .unwrap_or(0);
        if action == MenuAction::Up && at == 0 {
            cx.enter_chrome();
            return;
        }
        let next = if matches!(action, MenuAction::Up | MenuAction::Left) {
            at.saturating_sub(1)
        } else {
            (at + 1).min(controls.len() - 1)
        };
        let name = &controls[next].name;
        if self.compact
            && let Some(path) = hit::find(root, name)
            && hit::target_of(root, &path).is_some_and(|t| t.within("ResultsClip"))
            && let Some(rect) = hit::bounds(root, &hit::IDENTITY, &path)
        {
            let (top, bottom, _) = self.compact_bars();
            let y0 = rect[1] / self.zoom;
            let y1 = rect[3] / self.zoom;
            let delta = if y0 < top {
                y0 - top
            } else {
                (y1 - (self.screen[1] - bottom)).max(0.0)
            };
            self.scroll_by(delta, root);
        }
        cx.focus(name.clone());
    }

    fn song_list(&mut self, cx: &mut Cx) {
        self.env.state.borrow_mut().status = "song list".into();
        match self.actions.song_list.clone() {
            Some(f) => {
                let env = self.env.clone();
                f(&env, cx);
            }
            None => super::leave(&self.env, cx, true),
        }
    }
}

/// "−2.1", "+3": a signed number with a real minus sign. (An `f32`
/// widens exactly, and its digits are those of its exact value: the text
/// is the same as of the `f32` itself.)
pub fn signed(v: impl Into<f64>, decimals: usize) -> String {
    let v: f64 = v.into();
    let s = format!("{:.*}", decimals, v.abs());
    let zero = s.chars().all(|c| c == '0' || c == '.');
    if zero {
        s
    } else if v < 0.0 {
        format!("−{s}")
    } else {
        format!("+{s}")
    }
}

impl Screen for ResultsScreen {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "results"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.env.state.borrow_mut().over += 1;
        self.place(cx.viewport().screen);
        let root = self.make_root(cx);
        self.publish();
        root
    }

    /// Frames until it has settled (its clock is the frames').
    fn animating(&self) -> bool {
        self.age < SETTLE || self.live.animating(&self.env)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v: Vec<Focusable> = self
            .bar_buttons()
            .into_iter()
            .map(|(name, _, _)| Focusable::new(name))
            .chain(OFFSET_BUTTONS.into_iter().map(Focusable::new))
            .collect();
        if !self.replay_in_bar() && self.actions.replay.is_some() {
            v.insert(1, Focusable::new("WatchReplay"));
        }
        v.insert(1, Focusable::new("More"));
        v.push(Focusable::new("Back"));
        v
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        self.live.tick(root, "Wordmark", &self.env, cx);
        let settling = self.age < SETTLE;
        self.age += dt;
        // Settled: focus back on Song list, whatever a tap meanwhile moved
        // it to.
        if settling && self.age >= SETTLE {
            cx.focus("SongList");
        }
        if self.place(cx.viewport().screen) {
            self.scroll = 0.0;
            let left = super::time_left(root);
            *root = self.make_root(cx);
            super::relaid(root, left, cx);
            self.publish();
        }
        if self.env.state.borrow().screen != "results" {
            self.publish();
        }
    }

    fn message(&mut self, m: &crate::Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == crate::controller_hints::CHANGED {
            let left = super::time_left(root);
            *root = self.make_root(cx);
            super::relaid(root, left, cx);
            self.publish();
        }
        if let Some(id) = super::menu_pick(m, "Results.More") {
            self.input(&Input::Activate { name: id.into() }, root, cx);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        // Keys and pads held or mashed at the song's end, and any held
        // key's repeat, are let go.
        let settling = self.age < SETTLE;
        match input {
            Input::Menu { repeat, .. } if settling || *repeat => return Handled::Yes,
            Input::Key(k) if k.down && (settling || k.repeat) => return Handled::Yes,
            // Taps too: on a phone the play's touch lanes covered the
            // screen, and a tap mashed at the end lands on a button.
            Input::Activate { .. } if settling => return Handled::Yes,
            _ => {}
        }
        match input {
            Input::Menu { action, .. } if action.is_direction() && !cx.menu_from_keyboard() => {
                self.controller_focus(*action, root, cx);
                Handled::Yes
            }
            // A phone's column scrolls: by Up and Down, a wheel, a drag.
            Input::Menu {
                action: MenuAction::Up | MenuAction::Down,
                ..
            } if self.compact => {
                let down = matches!(
                    input,
                    Input::Menu {
                        action: MenuAction::Down,
                        ..
                    }
                );
                let before = self.scroll;
                self.scroll_by(if down { 80.0 } else { -80.0 }, root);
                if self.scroll != before {
                    Handled::Yes
                } else {
                    Handled::No
                }
            }
            Input::Wheel {
                target: Some(t),
                delta,
                ..
            } if self.compact && t.within("ResultsClip") => {
                self.scroll_by(delta[1] / self.zoom, root);
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture: crate::Gesture::Drag { delta },
                ..
            } if self.compact && t.within("ResultsClip") => {
                self.scroll_by(-delta[1] / self.zoom, root);
                Handled::Yes
            }
            Input::Menu { action, .. } => match action {
                MenuAction::More => {
                    self.open_more(cx);
                    Handled::Yes
                }
                MenuAction::Left | MenuAction::Previous | MenuAction::Right | MenuAction::Next => {
                    self.bar_focus(*action, root, cx);
                    Handled::Yes
                }
                MenuAction::Back => {
                    self.song_list(cx);
                    Handled::Yes
                }
                // A pad's Select (Y) retries, as the old results' Start
                // did; not Tab (it changes the style on song select: a
                // key that does that must not restart a song here).
                MenuAction::Select => {
                    if !cx.menu_from_keyboard() {
                        self.retry(cx);
                    }
                    Handled::Yes
                }
                // Start activates what has focus (Song list at first).
                _ => Handled::No,
            },
            Input::Key(k) if k.down && !k.repeat && !k.composing => {
                if k.key.eq_ignore_ascii_case("r")
                    && !k.modifiers.command()
                    && self.data.kind != ResultsKind::Kept
                {
                    self.retry(cx);
                    Handled::Yes
                } else if k.key.eq_ignore_ascii_case("w")
                    && !k.modifiers.command()
                    && self.data.kind != ResultsKind::Replay
                {
                    self.watch_replay(cx);
                    Handled::Yes
                } else if k.key == "Backspace" {
                    self.song_list(cx);
                    Handled::Yes
                } else {
                    Handled::No
                }
            }
            Input::Activate { name } => {
                let current = self.data.offset.map_or(0.0, |o| o.current);
                match name.as_str() {
                    "More" => self.open_more(cx),
                    "SongList" | "Back" => self.song_list(cx),
                    "Retry" => self.retry(cx),
                    "WatchReplay" => self.watch_replay(cx),
                    "OffsetDown" => self.set_offset(current - 0.001, root, cx),
                    "OffsetUp" => self.set_offset(current + 0.001, root, cx),
                    "UseMean" => {
                        if let Some(s) = self.data.offset.and_then(|o| o.suggested) {
                            self.set_offset(s, root, cx);
                        }
                    }
                    "ResetOffset" => self.set_offset(0.0, root, cx),
                    _ => {}
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}
