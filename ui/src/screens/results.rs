//! Results (`docs/plans/theme.md`, drawn in layout A):
//!
//! - **Left**: the song, the grade, score and EX (`Results.Score`), the
//!   judgement counts with bars (`Results.Judgement`), then held and let
//!   go, max combo, mines, fast and slow.
//! - **Right**: panels the player picks (`Panel`): **Overview** (timing
//!   histogram, life, offsets over the song, misses by lane) or
//!   **Timing** (a large histogram and the offsets); "Customize…" is
//!   where a player would pick their own (not yet).
//! - **Bottom**: the song offset editor, then Song list and Retry.
//! - **Keys**: Start goes back to the songs, `R` retries, Left and Right
//!   change the panels.
//!
//! The panels read the per-hit record ([`super::data::Hit`]) that step
//! 6.1 adds to the engine; the developer route shows a made-up play.

use ddi_actor::{Actor, Kind, Sprite};

use super::data::ResultsData;
use super::{Backdrop, Env, fit, fit_root, footer, frame, ground, header, left, right};
use crate::graph;
use crate::hit;
use crate::look::{self, Spec, call, named, quad};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::widgets::{part, set_image, set_text};

/// The panel sets the header offers.
pub const PANEL_SETS: [&str; 3] = ["Overview", "Timing", "Customize…"];

/// What Retry does (step 6: play the chart again).
pub type RetryFn = std::rc::Rc<dyn Fn(&Env, &mut Cx)>;

pub struct ResultsScreen {
    env: Env,
    data: ResultsData,
    panels: usize,
    backdrop: Backdrop,
    screen: [f32; 2],
    retry: Option<RetryFn>,
}

impl ResultsScreen {
    pub fn new(env: Env, data: ResultsData, retry: Option<RetryFn>) -> ResultsScreen {
        ResultsScreen {
            env,
            data,
            panels: 0,
            backdrop: Backdrop::with_alpha("Backdrop.Results"),
            screen: super::DESIGN,
            retry,
        }
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
        children.push(header(&env, screen[0], "RESULTS"));
        children.push(self.panel_pills(screen));
        let (f, texts) = footer(
            &env,
            screen,
            &[
                (&["ENTER"], "Song list"),
                (&["R"], "Retry"),
                (&["←→"], "Panels"),
            ],
            &self.data.note,
            &[],
        );
        env.state.borrow_mut().hints = texts;
        children.push(f);
        let mut root = Actor::new(Kind::Frame(children));
        super::entrance(&mut root, &env);
        if let Some(src) = self.data.song.backdrop().map(str::to_string) {
            self.backdrop.set(Some(&src), &mut root, cx, &env);
            // At once: the screen fades in as a whole.
            if let Some(b) = hit::find_mut(&mut root, "Backdrop") {
                b.finish_tweening();
                for c in b.children_mut() {
                    c.finish_tweening();
                }
            }
        }
        root
    }

    fn score_column(&self, [x, y, w]: [f32; 3], cx: &mut Cx) -> Actor {
        let env = &self.env;
        let d = &self.data;
        let mut children = Vec::new();
        // The banner, with its placeholder under it.
        let bh = (w * 125.0 / 400.0).round();
        let mut ph = quad("BannerPlaceholder", [w, bh], env.color("BannerEmpty"));
        call(&mut ph, "xy", &[w / 2.0, bh / 2.0]);
        let mut banner = named(Kind::Sprite(Sprite::quad([w, bh])), "Banner");
        call(&mut banner, "xy", &[w / 2.0, bh / 2.0]);
        let mut holder = frame("BannerHolder", vec![banner]);
        set_image(
            &mut holder,
            "Banner",
            d.song.banner.as_deref().map(|b| cx.texture(b, w)),
        );
        children.push(ph);
        children.push(holder);
        let mut ty = bh + 12.0 + 12.0;
        children.push(left(
            env.text("SongTitle", &d.song.title, 20.0, 700, "Text"),
            0.0,
            ty,
        ));
        ty += 22.0;
        let line = format!("{} · {}", d.song.artist, d.line);
        let mut l = left(env.text("SongLine", &line, 14.0, 400, "Secondary"), 0.0, ty);
        if let Kind::Text(t) = &mut l.kind {
            t.max_width = w;
        }
        children.push(l);
        ty += 20.0;
        // Grade, score, EX.
        let mut score = env.component(
            "Results.Score",
            "Panel",
            &Spec::new([w, 70.0], ""),
            "ScoreBox",
            [0.0, ty],
        );
        set_text(&mut score, "Grade", &d.grade);
        set_text(&mut score, "Score", &format!("{:.2}", d.score));
        set_text(&mut score, "Ex", &format!("EX {:.2}%", d.ex));
        set_text(&mut score, "Title", &format!("{} {:.2}%", d.grade, d.score));
        children.push(score);
        ty += 70.0 + 12.0;
        // Judgements.
        let max = d
            .judgements
            .iter()
            .map(|j| j.count)
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        for (i, j) in d.judgements.iter().enumerate() {
            let mut row = env.component(
                "Results.Judgement",
                "List.Row",
                &Spec::new([w, 28.0], &j.name),
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
            ty += 28.0;
        }
        ty += 12.0;
        // The stats, two columns.
        let mut line = quad("StatsLine", [w, 1.0], env.color("Line"));
        call(&mut line, "xy", &[w / 2.0, ty]);
        children.push(line);
        ty += 16.0;
        for (i, (label, value)) in d.stats.iter().enumerate() {
            let (cx0, row) = ((i % 2) as f32 * (w / 2.0), (i / 2) as f32);
            let yy = ty + row * 22.0;
            children.push(left(
                env.text(&format!("Stat{i}"), label, 13.0, 400, "Muted"),
                cx0,
                yy,
            ));
            children.push(right(
                env.numbers(&format!("StatValue{i}"), value, 14.0, "Text"),
                cx0 + w / 2.0 - 16.0,
                yy,
            ));
        }
        let mut f = frame("ScoreColumn", children);
        call(&mut f, "xy", &[x, y]);
        f
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

    fn panels_actor(&self, [x, y, w, h]: [f32; 4]) -> Actor {
        let gap = 16.0;
        let children = match self.panels {
            1 => {
                let top = (h - gap) * 0.63;
                vec![
                    self.timing_panel([x, y, w, top], 1.5),
                    self.offsets_panel([x, y + top + gap, w, h - top - gap]),
                ]
            }
            2 => vec![self.panel(
                "Customize",
                "Customize…",
                "",
                [x, y, w, h],
                vec![left(
                    self.env.text(
                        "CustomizeNote",
                        "Pick your own panels here (not yet).",
                        15.0,
                        400,
                        "Muted",
                    ),
                    16.0,
                    h / 2.0,
                )],
            )],
            _ => {
                let (pw, ph) = ((w - gap) / 2.0, (h - gap) / 2.0);
                vec![
                    self.timing_panel([x, y, pw, ph], 3.0),
                    self.life_panel([x + pw + gap, y, pw, ph]),
                    self.offsets_panel([x, y + ph + gap, pw, ph]),
                    self.lanes_panel([x + pw + gap, y + ph + gap, pw, ph]),
                ]
            }
        };
        frame("Panels", children)
    }

    /// The timing histogram: bins of `bin` ms over ±45, coloured by
    /// window, the best window's band behind them.
    fn timing_panel(&self, rect: [f32; 4], bin: f32) -> Actor {
        let env = &self.env;
        let [_, _, w, h] = rect;
        let (mean, sd) = self.data.timing();
        let note = format!("mean {} ms · σ {sd:.1}", signed(mean, 1));
        let inner = [14.0, 40.0, w - 28.0, h - 40.0 - 30.0];
        let range = 45.0;
        let bins = (2.0 * range / bin).round() as usize;
        let offsets: Vec<f32> = self.data.hits.iter().filter_map(|h| h.offset).collect();
        let counts = graph::histogram(&offsets, [-range, range], bins);
        let max = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
        let [ix, iy, iw, ih] = inner;
        let mut c = Vec::new();
        // The best window's band.
        if let Some((edge, _)) = self.data.windows.first() {
            let bw = iw * (edge * 2.0 / (2.0 * range)).min(1.0);
            let mut band = quad("Window", [bw, ih], env.color("Window"));
            call(&mut band, "xy", &[ix + iw / 2.0, iy + ih / 2.0]);
            c.push(band);
        }
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
                .find(|(edge, _)| centre.abs() <= *edge)
                .map_or("Judgement.Miss", |(_, c)| c.as_str());
            let bh = n as f32 / max * ih;
            let mut q = quad("", [(slot - 1.0).max(1.0), bh], env.color(color));
            call(
                &mut q,
                "xy",
                &[ix + slot * (i as f32 + 0.5), iy + ih - bh / 2.0],
            );
            c.push(q);
        }
        c.push(graph::segment(
            [ix + iw / 2.0, iy],
            [ix + iw / 2.0, iy + ih],
            1.0,
            env.color("Graph.Axis"),
        ));
        c.push(left(
            env.text("Early", "−45 ms early", 12.0, 400, "Faint"),
            ix,
            iy + ih + 14.0,
        ));
        c.push(right(
            env.text("Late", "late +45 ms", 12.0, 400, "Faint"),
            ix + iw,
            iy + ih + 14.0,
        ));
        self.panel("Timing", "Timing", &note, rect, c)
    }

    fn life_panel(&self, rect: [f32; 4]) -> Actor {
        let env = &self.env;
        let [_, _, w, h] = rect;
        let lowest = self
            .data
            .life
            .iter()
            .map(|&(_, v)| v)
            .fold(1.0f32, f32::min);
        let note = format!("lowest {:.0}%", lowest * 100.0);
        let len = self.data.song.length.max(1.0);
        let inner = [14.0, 40.0, w - 28.0, h - 40.0 - 30.0];
        let [ix, iy, iw, ih] = inner;
        let mut c = vec![graph::line_graph(
            "Life.Graph",
            &*env.look,
            &self.data.life,
            [0.0, len],
            [0.0, 1.0],
            inner,
        )];
        // The danger line, dashed.
        let dy = iy + ih * 0.75;
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
        let len = self.data.song.length.max(1.0);
        let [ix, iy, iw, ih] = [14.0, 40.0, w - 28.0, h - 40.0 - 14.0];
        let range = 50.0;
        let mut c = vec![graph::segment(
            [ix, iy + ih / 2.0],
            [ix + iw, iy + ih / 2.0],
            1.0,
            env.color("Graph.Axis"),
        )];
        let mut pts = Vec::new();
        for hit in &self.data.hits {
            let x = ix + (hit.time / len).clamp(0.0, 1.0) * iw;
            match hit.offset {
                Some(o) => pts.push([x, iy + ih / 2.0 + (o / range).clamp(-1.0, 1.0) * ih / 2.0]),
                None => c.push(graph::segment(
                    [x, iy],
                    [x, iy + ih],
                    1.5,
                    env.color("Judgement.Miss"),
                )),
            }
        }
        c.push(graph::points(
            "Offsets.Points",
            &pts,
            2.0,
            env.color("Graph.Point"),
        ));
        self.panel("Offsets", "Offsets over the song", "early above", rect, c)
    }

    /// Per lane, the hits that were not the best judgement.
    fn lanes_panel(&self, rect: [f32; 4]) -> Actor {
        let env = &self.env;
        let [_, _, w, h] = rect;
        let best = self.data.windows.first().map_or(21.5, |w| w.0);
        let lanes = self.data.lanes.len().max(1);
        let mut counts = vec![0u32; lanes];
        let mut sums = vec![0f32; lanes];
        for hit in &self.data.hits {
            let l = usize::from(hit.lane).min(lanes - 1);
            match hit.offset {
                Some(o) if o.abs() > best => {
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
        let mut c = Vec::new();
        for l in 0..lanes {
            let bh = counts[l] as f32 / max * ih;
            let cxl = ix + slot * (l as f32 + 0.5);
            if counts[l] > 0 {
                let mut q = quad("", [slot * 0.5, bh], env.color("Graph.Bar"));
                call(&mut q, "xy", &[cxl, iy + ih - bh / 2.0]);
                c.push(q);
            }
            let mean = if counts[l] > 0 {
                sums[l] / counts[l] as f32
            } else {
                0.0
            };
            let mut name = env.text(
                &format!("Lane{l}"),
                &self.data.lanes[l],
                13.0,
                600,
                "Secondary",
            );
            call(&mut name, "xy", &[cxl, iy + ih + 14.0]);
            c.push(name);
            let mut n = env.numbers(
                &format!("LaneNote{l}"),
                &format!("{} · {} ms", counts[l], signed(mean, 0)),
                12.0,
                "Muted",
            );
            call(&mut n, "xy", &[cxl, iy + ih + 32.0]);
            c.push(n);
        }
        self.panel("Lanes", "By lane · not Fantastic", "", rect, c)
    }

    fn bottom_bar(&self, [x, y, w, h]: [f32; 4]) -> Actor {
        let env = &self.env;
        let mid = h / 2.0;
        let mut c = vec![left(
            env.text("OffsetLabel", "Song offset", 15.0, 600, "Secondary"),
            0.0,
            mid,
        )];
        let bx = 118.0;
        c.push(env.component(
            "Button",
            "Button",
            &Spec::new([44.0, 44.0], "−"),
            "OffsetDown",
            [bx + 22.0, mid],
        ));
        let mut v = env.numbers(
            "OffsetValue",
            &format!("{:+} ms", self.data.song_offset),
            20.0,
            "Text",
        );
        call(&mut v, "xy", &[bx + 44.0 + 51.0, mid]);
        c.push(v);
        c.push(env.component(
            "Button",
            "Button",
            &Spec::new([44.0, 44.0], "+"),
            "OffsetUp",
            [bx + 146.0 + 22.0, mid],
        ));
        let (mean, _) = self.data.timing();
        let label = format!("Use the mean, {} ms", signed(mean, 0));
        let uw = look::mono_width(&label, 15.0) + 28.0;
        c.push(env.component(
            "Button",
            "Button",
            &Spec::new([uw, 44.0], &label),
            "UseMean",
            [bx + 190.0 + 12.0 + uw / 2.0, mid],
        ));
        let retry_w = 112.0;
        let list_w = 128.0;
        c.push(env.component(
            "PlayButton",
            "Button",
            &Spec::new([retry_w, 48.0], "Retry"),
            "Retry",
            [w - retry_w / 2.0, mid],
        ));
        c.push(env.component(
            "Button",
            "Button",
            &Spec::new([list_w, 48.0], "Song list"),
            "SongList",
            [w - retry_w - 12.0 - list_w / 2.0, mid],
        ));
        let mut f = frame("Bottom", c);
        call(&mut f, "xy", &[x, y]);
        f
    }

    fn panel_pills(&self, screen: [f32; 2]) -> Actor {
        let env = &self.env;
        let mut x = screen[0] - env.metric("Margin", 32.0);
        let mut c = Vec::new();
        for (i, name) in PANEL_SETS.iter().enumerate().rev() {
            let w = look::mono_width(name, 13.0) + 24.0;
            let mut pill =
                look::component_or(&*env.look, "Pill", "Button", &Spec::new([w, 30.0], *name));
            pill.name = format!("PanelSet{i}");
            let w = super::extent(&mut pill, [w, 30.0])[0];
            call(&mut pill, "xy", &[x - w / 2.0, 32.0]);
            crate::widgets::play(
                &mut pill,
                if i == self.panels {
                    "Selected"
                } else {
                    "Available"
                },
            );
            pill.finish_tweening();
            x -= w + 8.0;
            c.push(pill);
        }
        c.push(right(
            env.text("PanelsLabel", "Panels", 13.0, 400, "Muted"),
            x,
            32.0,
        ));
        frame("PanelSets", c)
    }

    fn set_panels(&mut self, i: usize, root: &mut Actor) {
        let i = i.min(PANEL_SETS.len() - 1);
        if i == self.panels {
            return;
        }
        self.panels = i;
        let (_, rect, _) = self.places();
        let fresh = self.panels_actor(rect);
        if let Some(p) = hit::find_mut(root, "Panels") {
            *p = fresh;
        }
        let pills = self.panel_pills(self.screen);
        if let Some(p) = hit::find_mut(root, "PanelSets") {
            *p = pills;
        }
        self.publish();
    }

    fn set_offset(&mut self, ms: i32, root: &mut Actor) {
        self.data.song_offset = ms.clamp(-250, 250);
        if let Some(v) = hit::find_mut(root, "OffsetValue")
            && let Kind::Text(t) = &mut v.kind
        {
            t.set_text(&format!("{:+} ms", self.data.song_offset));
        }
        self.publish();
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "results".into();
        st.panels = PANEL_SETS[self.panels].to_string();
        st.song_offset = self.data.song_offset;
        st.song = self.data.song.title.clone();
        st.backdrop = self.backdrop.source().map(str::to_string);
    }

    fn retry(&mut self, cx: &mut Cx) {
        self.env.state.borrow_mut().status = "retry".into();
        if let Some(r) = self.retry.clone() {
            let env = self.env.clone();
            r(&env, cx);
        }
    }
}

/// "−2.1", "+3": a signed number with a real minus sign.
pub fn signed(v: f32, decimals: usize) -> String {
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
    fn name(&self) -> &str {
        "results"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.env.state.borrow_mut().over += 1;
        let vp = cx.viewport().screen;
        self.screen = fit(&*self.env.look, vp);
        let mut root = self.build_tree(cx);
        fit_root(&*self.env.look, &mut root, vp);
        self.publish();
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v: Vec<Focusable> = ["SongList", "Retry", "OffsetDown", "OffsetUp", "UseMean"]
            .into_iter()
            .map(Focusable::new)
            .collect();
        v.extend((0..PANEL_SETS.len()).map(|i| Focusable::new(format!("PanelSet{i}"))));
        v
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        let vp = cx.viewport().screen;
        if fit(&*self.env.look, vp) != self.screen {
            self.screen = fit(&*self.env.look, vp);
            *root = self.build_tree(cx);
            fit_root(&*self.env.look, root, vp);
            root.finish_tweening();
        }
        if self.env.state.borrow().screen != "results" {
            self.publish();
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Menu { action, .. } => match action {
                MenuAction::Left => {
                    self.set_panels(self.panels.saturating_sub(1), root);
                    Handled::Yes
                }
                MenuAction::Right => {
                    self.set_panels(self.panels + 1, root);
                    Handled::Yes
                }
                MenuAction::Start if cx.focused().is_none_or(|f| f == "SongList") => {
                    super::leave(&self.env, cx, false);
                    Handled::Yes
                }
                MenuAction::Back => {
                    super::leave(&self.env, cx, true);
                    Handled::Yes
                }
                _ => Handled::No,
            },
            Input::Key(k) if k.down && !k.repeat && k.key.eq_ignore_ascii_case("r") => {
                self.retry(cx);
                Handled::Yes
            }
            Input::Activate { name } => {
                match name.as_str() {
                    "SongList" => super::leave(&self.env, cx, false),
                    "Retry" => self.retry(cx),
                    "OffsetDown" => self.set_offset(self.data.song_offset - 1, root),
                    "OffsetUp" => self.set_offset(self.data.song_offset + 1, root),
                    "UseMean" => {
                        let (mean, _) = self.data.timing();
                        self.set_offset(self.data.song_offset - mean.round() as i32, root);
                    }
                    other => {
                        if let Some(i) = other.strip_prefix("PanelSet").and_then(|n| n.parse().ok())
                        {
                            self.set_panels(i, root);
                        }
                    }
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}
