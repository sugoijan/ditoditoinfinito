//! The group's draw shown (room model revision 22, round RL4): a reel of
//! the draw's entries (each a player's name, their song and its chart, in
//! the draw's order) that spins and slows, as a roulette does, to stop on
//! the entry drawn, then one line saying whose choice it took. Every
//! device shows the same: the reel's motion is made from the room's
//! record alone (its entries, its winner), never from a number of the
//! device's own. About three seconds (`ddi_room::DRAW_V0.reveal_ms`),
//! skippable by nothing (it is short), still under reduced motion (the
//! entry drawn centered and the line at once). It closes by itself when
//! the reveal is over on this device, the stage's Ready card following
//! (`RoomClient::reveal`, which holds the stage's prompt back meanwhile).
//! The room's audience sees the same reveal over its page (round RL4b,
//! [`open_audience_reveal`]), drawn from the public view's draw.

use std::rc::Rc;

use ddi_actor::Actor;
use ddi_l10n::{UiText, msg::room as m};
use ddi_room::{DrawEntry, DrawRecord};

use super::dialog::{fitted_dialog, shell};
use super::{Env, left};
use crate::look::{self, quad_at};
use crate::nav::Focusable;
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::room::RoomClient;


/// The reel's rows shown at once (the drawn one in the middle).
const SHOWN: usize = 5;
/// A row's height.
const ROW_H: f32 = 38.0;
/// Of the reveal, the part the reel spins (the rest holds the result).
const SPIN: f64 = 0.75;
/// The rows that pass at the least before it stops: a few turns where
/// there are few entries.
const PASS: usize = 14;

/// The reel's place at `progress` (0 to 1 of the reveal) among `n`
/// entries stopping on `winner`: how many rows have passed the middle, as
/// a fraction. The same for every device given the same record.
pub fn reel_at(n: usize, winner: usize, progress: f64, reduced: bool) -> f64 {
    let n = n.max(1);
    let winner = winner.min(n - 1);
    let turns = PASS.div_ceil(n).max(1);
    let total = (turns * n + winner) as f64;
    if reduced {
        return total;
    }
    let t = (progress / SPIN).clamp(0.0, 1.0);
    // Ease out (a cubic): fast, then slowing to a stop.
    total * (1.0 - (1.0 - t).powi(3))
}

/// A draw as the reveal shows it: each entry's line (whose choice, the
/// song, its chart) in the draw's order, the winner's index, the stage's
/// number and the line saying whose choice it took.
#[derive(Clone, Debug, PartialEq)]
pub struct Reel {
    pub lines: Vec<String>,
    pub winner: usize,
    pub number: usize,
    pub result: String,
}

impl Reel {
    /// A member's: the room's record, this player's own entry "You".
    pub fn of_room(r: &RoomClient, record: &DrawRecord) -> Reel {
        let words = |e: &DrawEntry| -> (UiText, UiText, UiText) {
            let who = if e.participant == r.me() {
                m::draw_entry_you()
            } else {
                UiText::data(r.member_of(e.participant).map(super::room::person_name).unwrap_or_default())
            };
            let (title, chart) = r
                .state()
                .songs
                .get(e.song)
                .and_then(|s| s.chart(&e.chart).map(|line| s.card_for(line)))
                .map(|c| (c.title, format!("{} {}", c.difficulty, c.meter)))
                .unwrap_or_default();
            (who, UiText::data(title), UiText::data(chart))
        };
        // Under Adaptive the song alone: each starts on their own chart.
        let song_only = r.own_starts(record.stage);
        let lines = record
            .entries
            .iter()
            .map(|e| {
                let (who, title, chart) = words(e);
                if song_only {
                    m::draw_entry_song(who, title).into()
                } else {
                    m::draw_entry(who, title, chart).into()
                }
            })
            .collect();
        let result = record
            .won()
            .map(|won| {
                let (_, title, chart) = words(won);
                let who = || {
                    UiText::data(r.member_of(won.participant).map(super::room::person_name).unwrap_or_default())
                };
                match (won.participant == r.me(), song_only) {
                    (true, true) => m::draw_won_you_song(title).into(),
                    (true, false) => m::draw_won_you(title, chart).into(),
                    (false, true) => m::draw_won_song(who(), title).into(),
                    (false, false) => m::draw_won(who(), title, chart).into(),
                }
            })
            .unwrap_or_default();
        let number = r
            .state()
            .current
            .as_ref()
            .and_then(|m| m.stage_index(record.stage))
            .map_or(1, |i| i + 1);
        Reel {
            lines,
            winner: usize::from(record.winner),
            number,
            result,
        }
    }

    /// The audience's: the public view's draw, each entry named as the
    /// page names the players.
    pub fn of_public(v: &ddi_room::audience::PublicView, drawn: &ddi_room::audience::public::PublicDrawn) -> Reel {
        let name = |id| super::audience::player_name(v, id);
        let chart = |c: &ddi_room::ContentCard| UiText::data(format!("{} {}", c.difficulty, c.meter));
        // Under Adaptive the song alone (round OC).
        let song_only = v.rules.charts == ddi_room::Charts::Adaptive;
        let lines = drawn
            .entries
            .iter()
            .map(|e| {
                let (who, title) = (UiText::data(name(e.participant)), UiText::data(&e.card.title));
                if song_only {
                    m::draw_entry_song(who, title).into()
                } else {
                    m::draw_entry(who, title, chart(&e.card)).into()
                }
            })
            .collect();
        let result = drawn
            .entries
            .get(usize::from(drawn.winner))
            .map(|won| {
                let (who, title) = (UiText::data(name(won.participant)), UiText::data(&won.card.title));
                if song_only {
                    m::draw_won_song(who, title).into()
                } else {
                    m::draw_won(who, title, chart(&won.card)).into()
                }
            })
            .unwrap_or_default();
        let number = v.stages.iter().position(|s| s.id == drawn.stage).map_or(1, |i| i + 1);
        Reel {
            lines,
            winner: usize::from(drawn.winner),
            number,
            result,
        }
    }
}

/// How far into its reveal this device is (0 to 1), while it reveals it;
/// none once over.
pub type Progress = Rc<dyn Fn() -> Option<f64>>;

/// The draw's reveal over the room's song select, or the audience's page.
pub struct RevealDialog {
    env: Env,
    reel: Reel,
    progress: Progress,
    /// The UI's clock at the last frame, for the next.
    time: f64,
    /// The UI's clock when it first ran, and whether the device was
    /// revealing then: it closes once that reveal is over (one opened
    /// with none, as a test's, after the reveal's own length).
    opened: Option<f64>,
    saw: bool,
}

impl RevealDialog {
    pub fn new(env: Env, reel: Reel, progress: Progress) -> RevealDialog {
        RevealDialog {
            env,
            reel,
            progress,
            time: 0.0,
            opened: None,
            saw: false,
        }
    }

    /// A member's, over the room's song select.
    pub fn for_room(env: Env, room: crate::state::room::SharedRoom, record: DrawRecord) -> RevealDialog {
        let reel = Reel::of_room(&room.borrow(), &record);
        let progress: Progress = Rc::new(move || {
            let r = room.borrow();
            let now = r.now_ms();
            r.reveal().map(|v| v.progress(now))
        });
        RevealDialog::new(env, reel, progress)
    }
}

impl Screen for RevealDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "room-draw"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        // A phone's narrow screen: drawn on a wider one, scaled to fit.
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let progress = (self.progress)().unwrap_or(1.0);
        let reduced = env.reduced();
        let w = 560.0_f32.min(screen[0] - 32.0);
        let h = 64.0 + SHOWN as f32 * ROW_H + 72.0;
        let title: String = m::draw_heading(self.reel.number).into();
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &title);
        let lines = &self.reel.lines;
        let n = lines.len().max(1);
        let at = reel_at(lines.len(), self.reel.winner, progress, reduced);
        let top = y + 64.0;
        let middle = top + (SHOWN / 2) as f32 * ROW_H + ROW_H / 2.0;
        // The middle row's band: the entry under it is the one the reel
        // rests on.
        c.push(quad_at(
            "Band",
            [w - 32.0, ROW_H - 4.0],
            [1.0, 1.0, 1.0, 0.08],
            [x + w / 2.0, middle],
        ));
        let stopped = reduced || progress >= SPIN;
        let base = at.floor();
        let frac = (at - base) as f32;
        let half = (SHOWN / 2) as i64;
        for k in -half - 1..=half + 1 {
            // The entry `k` rows from the middle: the reel moves up as rows
            // pass, so the next entry comes from below.
            let index = (base as i64 + k).rem_euclid(n as i64) as usize;
            let Some(line) = lines.get(index) else { continue };
            let row_y = middle + (k as f32 - frac) * ROW_H;
            // Within the reel's window: never over the title or the line
            // under it.
            if row_y < top + ROW_H * 0.3 || row_y > top + SHOWN as f32 * ROW_H - ROW_H * 0.3 {
                continue;
            }
            let distance = ((row_y - middle) / ROW_H).abs();
            let won = stopped && k == 0;
            let color = if won {
                "Gold"
            } else if distance < 0.5 {
                "Text"
            } else {
                "Faint"
            };
            let mut t = env.text(&format!("Entry{k}"), line, 15.0, if won { 700 } else { 500 }, color);
            look::ellipsized(&mut t, w - 64.0);
            c.push(left(t, x + 32.0, row_y));
        }
        // Whose choice it took, once the reel stops.
        if stopped && !self.reel.result.is_empty() {
            let mut t = env.text("Result", &self.reel.result, 15.0, 700, "Text");
            look::ellipsized(&mut t, w - 48.0);
            c.push(left(t, x + 24.0, y + h - 36.0));
        }
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        Vec::new()
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        self.time = cx.time();
        let opened = *self.opened.get_or_insert(self.time);
        let revealing = (self.progress)().is_some();
        self.saw |= revealing;
        // Over on this device: the stage's Ready card comes next.
        let length = ddi_room::DRAW_V0.reveal_ms as f64 / 1000.0;
        if !revealing && (self.saw || self.time > opened + length) {
            cx.close(false);
            return;
        }
        *root = self.build(cx);
    }

    fn wake_at(&self) -> Option<f64> {
        Some(self.time + 1.0 / 60.0)
    }

    fn input(&mut self, _input: &Input, _root: &mut Actor, _cx: &mut Cx) -> Handled {
        // Skippable by nothing: it is short, and everyone sees it alike.
        Handled::Yes
    }
}

/// Opens the draw's reveal over the room's song select where this device
/// reveals a draw it has not shown yet; whether it opened (`shown` keeps
/// the last one shown).
pub fn open_reveal(
    env: &Env,
    room: &crate::state::room::SharedRoom,
    shown: &mut Option<(ddi_room::StageId, u64)>,
    cx: &mut Cx,
) -> bool {
    let record = room.borrow().reveal().map(|v| v.what.clone());
    let Some(record) = record else {
        return false;
    };
    let key = (record.stage, record.roll);
    if *shown == Some(key) {
        return false;
    }
    *shown = Some(key);
    cx.overlay(RevealDialog::for_room(env.clone(), Rc::clone(room), record), true);
    true
}

/// Opens the draw's reveal over the audience's page where the seat
/// reveals a draw it has not shown yet (round RL4b): the same reel the
/// members see; whether it opened (`shown` keeps the last one shown).
pub fn open_audience_reveal(
    env: &Env,
    audience: &crate::state::audience::SharedAudience,
    shown: &mut Option<(ddi_room::StageId, ddi_room::Millis)>,
    cx: &mut Cx,
) -> bool {
    let reel = {
        let a = audience.borrow();
        let Some(v) = a.reveal() else {
            return false;
        };
        let key = (v.what.stage, v.what.at);
        if *shown == Some(key) {
            return false;
        }
        *shown = Some(key);
        a.view().map(|view| Reel::of_public(view, &v.what))
    };
    let Some(reel) = reel else {
        return false;
    };
    let seat = Rc::clone(audience);
    let progress: Progress = Rc::new(move || {
        let a = seat.borrow();
        let now = crate::state::room::LiveRoom::now_ms(&*a);
        a.reveal().map(|v| v.progress(now))
    });
    cx.overlay(RevealDialog::new(env.clone(), reel, progress), true);
    true
}
