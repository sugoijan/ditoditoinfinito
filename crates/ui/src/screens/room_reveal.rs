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

use std::rc::Rc;

use ddi_actor::Actor;
use ddi_l10n::{UiText, msg::room as m};
use ddi_room::{DrawEntry, DrawRecord};

use super::dialog::{dialog_root, shell};
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

/// The reel's place at `progress` (0 to 1 of the reveal): how many rows
/// have passed the middle, as a fraction, stopping on the winner. The
/// same for every device given the same record.
pub fn reel_at(record: &DrawRecord, progress: f64, reduced: bool) -> f64 {
    let n = record.entries.len().max(1);
    let winner = usize::from(record.winner).min(n - 1);
    let turns = PASS.div_ceil(n).max(1);
    let total = (turns * n + winner) as f64;
    if reduced {
        return total;
    }
    let t = (progress / SPIN).clamp(0.0, 1.0);
    // Ease out (a cubic): fast, then slowing to a stop.
    total * (1.0 - (1.0 - t).powi(3))
}

/// The draw's reveal over the room's song select.
pub struct RevealDialog {
    env: Env,
    room: crate::state::room::SharedRoom,
    record: DrawRecord,
    /// The UI's clock at the last frame, for the next.
    time: f64,
    /// The UI's clock when it first ran, and whether the room's client was
    /// revealing then: it closes once that reveal is over (one opened
    /// with none, as a test's, after the reveal's own length).
    opened: Option<f64>,
    saw: bool,
}

impl RevealDialog {
    pub fn new(env: Env, room: crate::state::room::SharedRoom, record: DrawRecord) -> RevealDialog {
        RevealDialog {
            env,
            room,
            record,
            time: 0.0,
            opened: None,
            saw: false,
        }
    }

    /// An entry's words: whose choice (you, or their name), the song's
    /// title and its chart.
    fn words(r: &RoomClient, e: &DrawEntry) -> (UiText, UiText, UiText) {
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
        let screen = [vp.width, vp.height];
        let env = self.env.clone();
        let r = self.room.borrow();
        let now = r.now_ms();
        let progress = r.reveal().map_or(1.0, |v| v.progress(now));
        let reduced = env.reduced();
        let number = r
            .state()
            .current
            .as_ref()
            .and_then(|m| m.stage_index(self.record.stage))
            .map_or(1, |i| i + 1);
        let w = 560.0_f32.min(screen[0] - 32.0);
        let h = 64.0 + SHOWN as f32 * ROW_H + 72.0;
        let title: String = m::draw_heading(number).into();
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &title);
        let entries = &self.record.entries;
        let n = entries.len().max(1);
        let at = reel_at(&self.record, progress, reduced);
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
            let Some(e) = entries.get(index) else { continue };
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
            let (who, title, chart) = Self::words(&r, e);
            let line: String = m::draw_entry(who, title, chart).into();
            let mut t = env.text(&format!("Entry{k}"), &line, 15.0, if won { 700 } else { 500 }, color);
            look::ellipsized(&mut t, w - 64.0);
            c.push(left(t, x + 32.0, row_y));
        }
        // Whose choice it took, once the reel stops.
        if stopped && let Some(won) = self.record.won() {
            let (_, title, chart) = Self::words(&r, won);
            let line = if won.participant == r.me() {
                m::draw_won_you(title, chart)
            } else {
                let who = r.member_of(won.participant).map(super::room::person_name).unwrap_or_default();
                m::draw_won(UiText::data(who), title, chart)
            };
            let mut t = env.text("Result", &String::from(line), 15.0, 700, "Text");
            look::ellipsized(&mut t, w - 48.0);
            c.push(left(t, x + 24.0, y + h - 36.0));
        }
        drop(r);
        dialog_root(c)
    }

    fn focusables(&self) -> Vec<Focusable> {
        Vec::new()
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        self.time = cx.time();
        let opened = *self.opened.get_or_insert(self.time);
        let revealing = self.room.borrow().reveal().is_some();
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
    let record = room.borrow().reveal().map(|v| v.record.clone());
    let Some(record) = record else {
        return false;
    };
    let key = (record.stage, record.roll);
    if *shown == Some(key) {
        return false;
    }
    *shown = Some(key);
    cx.overlay(RevealDialog::new(env.clone(), Rc::clone(room), record), true);
    true
}
