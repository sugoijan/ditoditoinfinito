//! The start prompt of a multiplayer room's stage (M7b, the maintainer's
//! match flow; `docs/plans/multiplayer-prototype-progress.md`): the
//! readiness check. Its Start is **Ready** (pressed again, Cancel ready),
//! counted down by itself where the shell asks for a countdown, as a
//! singleplayer play's auto-start is (`PromptState::countdown`: only where
//! the audio may start without another press). Beside the controls'
//! diagram the players and whether each is ready, scrolling where they do
//! not fit; under the song the room's status (the window's seconds left,
//! the countdown). The stage starts for everyone once all are ready or at
//! the room's deadline: this prompt then presses Start itself for the
//! shell (outside any gesture: the Ready press was the one that made the
//! audio context, `PromptChoice::Ready`). Whoever was not ready sits the
//! stage out: the prompt says so and goes back to the room, as it does
//! when the room cancels the start.
//!
//! A child module of `prompt` (it reads the screen's own fields).

use std::cell::{Cell, RefCell};

use super::*;
use crate::screens::room::{self, Pump, RoomShell};
use crate::state::room::{RoomView, SharedRoom};
use ddi_room::{Participation, StageId};

/// How long the prompt says why it goes before going back to the room.
const GONE_SECONDS: f32 = 2.5;
/// The players' column's width beside the diagram.
const LIST_W: f32 = 200.0;
const LIST_ROW: f32 = 22.0;
/// A CPU's level's stars at the end of its row.
const LIST_STARS: f32 = 11.0;

pub(super) struct PromptRoom {
    room: SharedRoom,
    stage: StageId,
    shell: RoomShell,
    pump: RefCell<Pump>,
    /// The room started the stage and Start was pressed for the shell.
    started: Cell<bool>,
    /// The room's version and seconds left the card was built for.
    seen: Cell<Option<(u64, Option<u64>)>>,
    /// Since when the room no longer has this prompt's stage for the
    /// player (sat out, canceled), seconds.
    gone: Cell<Option<f32>>,
    /// The players' column's first row.
    first: Cell<usize>,
    /// The UI's clock at the last update.
    time: Cell<f64>,
}

impl PromptScreen {
    /// The prompt of room `room`'s stage `stage` (M7b), its shell's room
    /// calls through `shell` (what the room needs meanwhile: songs to
    /// prepare and share).
    pub fn with_room(mut self, room: SharedRoom, stage: StageId, shell: RoomShell) -> PromptScreen {
        self.room = Some(PromptRoom {
            room,
            stage,
            shell,
            pump: RefCell::new(Pump::default()),
            started: Cell::new(false),
            seen: Cell::new(None),
            gone: Cell::new(None),
            first: Cell::new(0),
            time: Cell::new(0.0),
        });
        self
    }

    /// Start's label in a room: Ready, or Cancel ready once ready.
    pub(super) fn room_start_label(&self) -> Option<UiText> {
        let rm = self.room.as_ref()?;
        Some(if rm.room.borrow().ready() {
            msg::room::unready()
        } else {
            msg::room::ready()
        })
    }

    /// Start in a room: Ready set (`Some`) or toggled (`None`), the shell
    /// told inside the press ([`PromptChoice::Ready`]). Whether the room
    /// took it (a prompt outside a room: `false`).
    pub(super) fn room_ready(&self, set: Option<bool>, cx: &mut Cx) -> bool {
        let Some(rm) = &self.room else {
            return false;
        };
        if rm.started.get() || rm.gone.get().is_some() {
            return true;
        }
        let ready = set.unwrap_or(!rm.room.borrow().ready());
        if ready != rm.room.borrow().ready() {
            let _ = rm.room.borrow_mut().set_ready(ready);
        }
        (self.act)(PromptChoice::Ready, cx);
        true
    }

    /// The room's part of a frame: its pump, the stage's start (Start
    /// pressed for the shell), the room letting go of the prompt (said,
    /// then back), and the card built again when the room changed.
    pub(super) fn room_update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        rm.time.set(cx.time());
        let changed = rm.pump.borrow_mut().run(&rm.room, &rm.shell, cx);
        if rm.started.get() {
            return;
        }
        let (started, here, version, seconds) = {
            let r = rm.room.borrow();
            (
                r.stage_started(rm.stage),
                r.view() == RoomView::Prompt(rm.stage),
                r.version(),
                room::seconds_left(&r),
            )
        };
        if started {
            rm.started.set(true);
            self.countdown = Countdown::Done;
            (self.act)(PromptChoice::Start, cx);
            return;
        }
        if !here {
            let gone = rm.gone.get().map_or(0.0, |g| g + dt);
            let first = rm.gone.get().is_none();
            rm.gone.set(Some(gone));
            if gone >= GONE_SECONDS {
                rm.gone.set(Some(f32::MAX / 2.0));
                if gone < f32::MAX / 4.0 {
                    (self.act)(PromptChoice::Back, cx);
                }
                return;
            }
            if first {
                self.countdown = Countdown::Done;
                self.rebuild(root, cx);
                self.publish();
            }
            return;
        }
        let key = Some((version, seconds));
        if changed || rm.seen.get() != key {
            rm.seen.set(key);
            self.rebuild(root, cx);
            self.publish();
        }
    }

    /// Lines under the song in a room: the room's status, or why the
    /// prompt goes.
    pub(super) fn room_lines(&self) -> Vec<(String, &'static str)> {
        let Some(rm) = &self.room else {
            return Vec::new();
        };
        let r = rm.room.borrow();
        if rm.gone.get().is_some() {
            let sat_out = matches!(
                r.state().participation(r.me()),
                Some(Participation::SittingOut { .. })
            );
            let line = if sat_out {
                msg::room::prompt_sat_out()
            } else {
                msg::room::prompt_canceled()
            };
            return vec![(line.into(), "Gold")];
        }
        vec![(room::status(&r).into(), "Accent")]
    }

    /// The width the diagram leaves the players' column in a room.
    pub(super) fn room_list_width(&self, inner: f32) -> f32 {
        if self.room.is_some() && inner >= 440.0 {
            LIST_W
        } else {
            0.0
        }
    }

    /// The players beside the diagram, from `[x, y]`, `w` wide, at most
    /// `h` tall (at least four rows): name and readiness, the local
    /// player's first; marks for the rows hidden above and below.
    pub(super) fn room_list(&self, [x, y, w, h]: [f32; 4]) -> (Vec<Actor>, f32) {
        let Some(rm) = &self.room else {
            return (Vec::new(), 0.0);
        };
        let env = &self.env;
        let mut rows = room::player_rows(&rm.room.borrow());
        rows.sort_by_key(|p| !p.me);
        let fit = ((h.max(4.0 * LIST_ROW + 18.0) - 18.0) / LIST_ROW)
            .floor()
            .max(1.0) as usize;
        let marks = rows.len() > fit;
        let shown = if marks {
            fit.saturating_sub(1).max(1)
        } else {
            fit
        };
        let first = rm.first.get().min(rows.len().saturating_sub(shown));
        rm.first.set(first);
        let mut out = Vec::new();
        let head = look::tracked(
            env.text("Room.Players", &msg::room::players(), 11.0, 600, "Faint"),
            120,
        );
        out.push(left(head, x, y + 6.0));
        let ready = rows.iter().filter(|p| p.ready && !p.cpu).count();
        let people = rows.iter().filter(|p| !p.cpu).count();
        out.push(right(
            env.text(
                "Room.Ready",
                &msg::room::players_count(ready, people),
                11.0,
                400,
                "Muted",
            ),
            x + w,
            y + 6.0,
        ));
        let mut ry = y + 18.0;
        for (k, p) in rows.iter().enumerate().skip(first).take(shown) {
            let mid = ry + LIST_ROW / 2.0;
            let color = if p.me { "Text" } else { "Secondary" };
            // A CPU: its level's stars in the state's place.
            if let Some(level) = p.level {
                let sw = crate::stars::width(LIST_STARS);
                let mut n = env.text(&format!("Room.Player{k}"), &p.name, 13.0, 600, color);
                look::ellipsized(&mut n, (w - sw - 8.0).max(30.0));
                out.push(left(n, x, mid));
                out.push(crate::stars::stars_right(
                    env,
                    &format!("Room.Player{k}.Stars"),
                    level,
                    LIST_STARS,
                    x + w,
                    mid,
                ));
                ry += LIST_ROW;
                continue;
            }
            let state: String = if p.ready {
                msg::room::state_ready().into()
            } else if p.state.is_empty() {
                msg::room::state_not_ready().into()
            } else {
                p.state.clone()
            };
            let sw = look::mono_width(&state, 12.0).min(w * 0.5);
            let mut n = env.text(&format!("Room.Player{k}"), &p.name, 13.0, 600, color);
            look::ellipsized(&mut n, (w - sw - 8.0).max(30.0));
            out.push(left(n, x, mid));
            let state_color = if p.ready && !p.cpu { "Accent" } else { "Muted" };
            let mut s = env.text(
                &format!("Room.Player{k}.State"),
                &state,
                12.0,
                600,
                state_color,
            );
            look::ellipsized(&mut s, w * 0.5);
            out.push(right(s, x + w, mid));
            ry += LIST_ROW;
        }
        if marks {
            let above = first;
            let below = rows.len() - first - shown;
            let text: String = if below > 0 {
                msg::room::panel_more_below(below).into()
            } else {
                msg::room::panel_more_above(above).into()
            };
            let name = if below > 0 { "Room.Down" } else { "Room.Up" };
            out.push(env.component(
                "Link",
                "Button",
                &Spec::new([w, 20.0], &text),
                name,
                [x + w / 2.0, ry + 10.0],
            ));
            ry += 20.0;
        }
        (out, ry - y)
    }

    /// The players' column scrolled by `by` rows (its marks, the wheel).
    pub(super) fn room_scroll(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) -> bool {
        let Some(rm) = &self.room else {
            return false;
        };
        let n = room::player_rows(&rm.room.borrow()).len();
        let first = (rm.first.get() as i64 + i64::from(by)).clamp(0, n as i64) as usize;
        rm.first.set(first);
        self.rebuild(root, cx);
        true
    }

    /// What the debug state says of the room.
    pub(super) fn room_publish(&self, items: Vec<String>) {
        if let Some(rm) = &self.room {
            room::publish(&self.env, &rm.room, "prompt", items);
        }
    }

    /// The room is looked at ten times a second while nothing moves.
    pub(super) fn room_wake_at(&self) -> Option<f64> {
        self.room.as_ref().map(|rm| rm.time.get() + 0.1)
    }

    /// The room's stage this prompt is of, if any.
    pub fn room_stage(&self) -> Option<StageId> {
        self.room.as_ref().map(|rm| rm.stage)
    }
}
