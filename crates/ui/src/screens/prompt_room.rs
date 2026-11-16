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
/// PROVISIONAL (`ready-card-v1`, the feedback round I): the room the
/// card leaves the screen's edges on a desktop (a share of the width
/// within these bounds) and on a phone (its safe area); the players'
/// part is `room::players_part`.
pub(super) const CARD_MARGIN: (f32, f32, f32) = (0.03, 24.0, 64.0);
pub(super) const CARD_PHONE_MARGIN: f32 = 8.0;
/// The players' part beside the card's other parts from this width of
/// its own on (else under the diagram).
pub(super) const LIST_BESIDE: f32 = 270.0;

/// What changes the attack policy during play, as the prompt's hint shows
/// it: the active controller's Back (a short press of it, D8), else the
/// first policy key the chart's keyboard bindings leave free (Shift; Tab
/// where both Shifts are a lane's, `controller_hints::policy_key`); none
/// on a touch screen, or where every policy key is a lane's.
pub(super) fn targeting_keys(state: &PromptState, cx: &Cx) -> Option<Vec<crate::keycaps::Keycap>> {
    if state.touch {
        return None;
    }
    let hints = cx.hints();
    if hints.active() {
        let caps = hints.actions(&[ddi_engine::MenuInput::Back]);
        return (!caps.is_empty()).then_some(caps);
    }
    let table = state
        .bindings
        .as_ref()
        .map(|c| c.settings.controls.keyboard(&c.layout));
    let code = crate::controller_hints::policy_key(|k| {
        table
            .as_ref()
            .is_some_and(|t| t.iter().any(|lane| lane.iter().any(|c| c == k)))
    })?;
    Some(vec![crate::keycaps::Keycap::Key(
        crate::controller_hints::policy_keycap(code),
    )])
}

/// The attack policy's row: its name as a control (one of the play
/// options' rows, so the focus and the lighting are theirs) and its id.
pub(super) const TARGETING: &str = "Opt.room.targeting";
pub(super) const TARGETING_ID: &str = "room.targeting";

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
    /// The players' part's first row.
    first: Cell<usize>,
    /// Its rows, those shown and a column's rows when last laid out.
    rows_shown: Cell<(usize, usize, usize)>,
    /// The UI's clock at the last update.
    time: Cell<f64>,
    /// The game is quitting once the room has answered the player's
    /// leaving, or at this time on the UI's clock.
    quit_at: Cell<Option<f64>>,
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
            rows_shown: Cell::new((0, 0, 0)),
            time: Cell::new(0.0),
            quit_at: Cell::new(None),
        });
        self
    }

    /// Start's label in a room: Ready, or Cancel ready once ready.
    pub(super) fn room_start_label(&self) -> Option<UiText> {
        let rm = self.room.as_ref()?;
        let r = rm.room.borrow();
        Some(if !r.in_stage(rm.stage) {
            // A watcher has nothing to be ready for.
            msg::room::watching()
        } else if r.ready() {
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
        // A watcher's press readies nothing: the shell's sound alone.
        if rm.room.borrow().in_stage(rm.stage) {
            let ready = set.unwrap_or(!rm.room.borrow().ready());
            if ready != rm.room.borrow().ready() {
                let _ = rm.room.borrow_mut().set_ready(ready);
            }
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
        if room::quit_due(&rm.room, rm.quit_at.get(), cx.time()) {
            rm.quit_at.set(None);
            cx.request(crate::UiRequest::Quit);
            return;
        }
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

    /// Whether closing the game from the stage's prompt asks first
    /// ([`crate::Screen::guards_leaving`]): leaving the room costs
    /// something (Back here only leaves the prompt, for the room).
    pub(super) fn room_guards(&self) -> bool {
        self.room.as_ref().is_some_and(|rm| {
            rm.quit_at.get().is_none() && rm.room.borrow().leave_cost().is_some()
        })
    }

    /// The window asked to close on the stage's prompt: asked, and the
    /// game quit once the room is told if the player says Leave.
    pub(super) fn room_message(&mut self, msg: &crate::Message, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        if !cx.is_current_screen() {
            return;
        }
        if msg.name == crate::ui::CLOSE_REQUESTED
            && !room::ask_leave(&self.env, &rm.room, room::LeaveBy::Quit, cx)
        {
            cx.request(crate::UiRequest::Quit);
        }
        if room::left_by(msg) == Some(room::LeaveBy::Quit) {
            rm.quit_at.set(Some(room::quit_leaving(&rm.room, cx.time())));
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
            // Why the player sits the stage out, as the room's status
            // line says it.
            let line = match r.state().participation(r.me()) {
                Some(Participation::SittingOut { stage, reason }) => {
                    room::sat_out_status(room::number_of(&r, stage), reason)
                }
                _ => msg::room::prompt_canceled(),
            };
            return vec![(line.into(), "Gold")];
        }
        vec![(room::status_line(&r).into(), "Accent")]
    }

    /// Whether the card is a room's stage's Ready card (`ready-card-v1`):
    /// larger, translucent, its players' part in columns.
    pub(super) fn room_card(&self) -> bool {
        self.room.is_some()
    }

    /// The field's filter band is drawn on the backdrop: not behind a
    /// room's Ready card (its plate is translucent), only once the play
    /// is on its way (the loading screen).
    pub(super) fn band(&self, state: &PromptState) -> bool {
        !self.room_card() || state.step == PromptStep::Loading
    }

    /// The players in `[x, y, w, h]` (`h` at least four rows), each the
    /// room's players panel's line (`room::player_line`: the name and its
    /// badge, a CPU's stars or a role, the policy, the state, the points),
    /// in the room's order (`room::panel_order`), in as many columns as
    /// they need and `w` holds (one for a few, several for many: each at
    /// least [`LIST_COL_MIN`] wide), down each column then the next; past
    /// what the part holds a window of them, the local player's line
    /// always in view (`room::window_keeping`, as the Standings pop-up
    /// has it), a mark under it for the rows hidden. Its height.
    pub(super) fn room_list(&self, area: [f32; 4]) -> (Vec<Actor>, f32) {
        let Some(rm) = &self.room else {
            return (Vec::new(), 0.0);
        };
        let rows = room::panel_order(room::player_rows(&rm.room.borrow()));
        // Of the people who play the stage ([`room::plays`]): a watcher or
        // one waiting for the next match is never waited for.
        let ready = rows.iter().filter(|p| p.plays && p.ready && !p.cpu).count();
        let people = rows.iter().filter(|p| p.plays && !p.cpu).count();
        let part = room::players_part(
            &self.env,
            &rows,
            &room::PartNames {
                heading: "Room.Players",
                count: "Room.Ready",
                row: "Room.Player",
                down: "Room.Down",
                up: "Room.Up",
            },
            (&msg::room::players(), &msg::room::players_count(ready, people)),
            rm.first.get(),
            area,
        );
        rm.first.set(part.first);
        rm.rows_shown.set((rows.len(), part.shown, part.per));
        (part.actors, part.height)
    }

    /// The player's attack policy, where the stage's match has attacks
    /// and the room still has this prompt for them (M8 round 3, D8): a
    /// row `inner` wide from `y` (a label, ◀ the policy ▶) with a line
    /// under it saying what the policy does, and under that how to change
    /// it during play (the maintainer, 2026-11-13: Shift, shown on the
    /// ready screen): the key's cap, or the controller's Back where a
    /// controller is the hints' source, and "during play"; nothing on a
    /// touch screen (the label is tapped). Its control's name and its
    /// height.
    pub(super) fn room_targeting_row(
        &self,
        inner: f32,
        y: f32,
        state: &PromptState,
        cx: &mut Cx,
    ) -> Option<(Actor, String, f32)> {
        let rm = self.room.as_ref()?;
        if rm.gone.get().is_some() {
            return None;
        }
        let policy = {
            let r = rm.room.borrow();
            // A watcher sends no attacks: no policy to choose.
            (r.attacks_on() && r.in_stage(rm.stage)).then(|| r.my_targeting())?
        };
        let env = &self.env;
        let name = TARGETING;
        let x0 = 24.0;
        let mid = OPTION_ROW / 2.0;
        let label_text = msg::room::targeting();
        let label_w = look::mono_width(&label_text, 14.0).min(inner * 0.4);
        // Wide enough for the longest policy's word, so the arrows stay.
        let vw = ddi_room::attacks::Targeting::ALL
            .iter()
            .map(|t| look::mono_width(&room::policy_word(*t), 14.0))
            .fold(60.0, f32::max)
            .min(inner - label_w - 70.0)
            + 8.0;
        let vx = x0 + label_w + 14.0;
        let row_w = label_w + 14.0 + 44.0 + vw;
        let mut light = quad(
            &format!("{name}.Light"),
            [row_w + 12.0, OPTION_ROW - 2.0],
            [1.0, 1.0, 1.0, 0.004],
        );
        call(&mut light, "cornerradius", &[6.0]);
        call(&mut light, "xy", &[x0 + row_w / 2.0, mid]);
        let mut label = env.text(&format!("{name}.Label"), &label_text, 14.0, 400, "Muted");
        call(&mut label, "maxwidth", &[label_w]);
        let mut value = env.text(
            &format!("{name}.Value"),
            &room::policy_word(policy),
            14.0,
            600,
            "Text",
        );
        call(&mut value, "maxwidth", &[vw]);
        call(&mut value, "xy", &[vx + 22.0 + vw / 2.0, mid]);
        // An arrow and the box a press on it lands in (drawn all but
        // clear: a clear box takes no press).
        let arrow = |part: &str, glyph: &str, x: f32| {
            let mut hit = quad(
                &format!("{name}.{part}"),
                [26.0, OPTION_ROW],
                [1.0, 1.0, 1.0, 0.004],
            );
            call(&mut hit, "xy", &[x, mid]);
            let mut g = env.text("", glyph, 12.0, 400, "Accent");
            call(&mut g, "xy", &[x, mid]);
            frame(&format!("{name}.{part}"), vec![hit, g])
        };
        let mut parts = vec![
            light,
            left(label, x0, mid),
            arrow("Down", "◀", vx + 9.0),
            value,
            arrow("Up", "▶", vx + 22.0 + vw + 13.0),
        ];
        // What it does: beside the row where the card is wide, else
        // under it.
        let about = room::policy_about(policy);
        let beside = x0 + row_w + 18.0;
        let room_beside = inner + 24.0 - beside;
        let mut height = OPTION_ROW;
        if look::mono_width(&about, 12.0) <= room_beside {
            let text = env.text(&format!("{name}.About"), &about, 12.0, 400, "Muted");
            parts.push(left(text, beside, mid));
        } else {
            for (i, l) in wrap_lines(&about, inner, 12.0).into_iter().enumerate() {
                let mut text = env.text(&format!("{name}.About{i}"), &l, 12.0, 400, "Muted");
                look::ellipsized(&mut text, inner);
                parts.push(left(text, x0, height + 8.0));
                height += 16.0;
            }
        }
        if let Some(keys) = targeting_keys(state, cx) {
            let (mut hint, w) = crate::screens::key_hint(
                env,
                &format!("{name}.Key"),
                &keys,
                &msg::room::targeting_during_play(),
                cx,
            );
            call(&mut hint, "xy", &[x0 + w / 2.0, height + 15.0]);
            parts.push(hint);
            height += 28.0;
        }
        let mut row = frame(name, parts);
        call(&mut row, "y", &[y]);
        Some((row, name.into(), height))
    }

    /// The policy stepped by `by` in its ring (Left, Right, ◀ ▶, a tap or
    /// Enter on the row): the room is told, and the card built again.
    pub(super) fn room_targeting_step(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        if rm.started.get() || rm.gone.get().is_some() || !rm.room.borrow().in_stage(rm.stage) {
            return;
        }
        let all = ddi_room::attacks::Targeting::ALL;
        let now = rm.room.borrow().my_targeting();
        let i = all.iter().position(|t| *t == now).unwrap_or(0) as i32;
        let next = all[(i + by).rem_euclid(all.len() as i32) as usize];
        let _ = rm.room.borrow_mut().set_targeting(next);
        self.rebuild(root, cx);
        self.publish();
    }

    /// A column's rows in the players' part as last laid out: what its
    /// marks and Page Up and Page Down scroll it by.
    pub(super) fn room_page(&self) -> i32 {
        self.room
            .as_ref()
            .map_or(0, |rm| rm.rows_shown.get().2.max(1) as i32)
    }

    /// The players' part scrolled by `by` rows (its marks, the wheel,
    /// Page Up and Page Down).
    pub(super) fn room_scroll(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) -> bool {
        let Some(rm) = &self.room else {
            return false;
        };
        // Within the part as last laid out (its rows counted then).
        let (n, shown, _) = rm.rows_shown.get();
        rm.first.set(crate::screens::scrolled(rm.first.get(), by, n, shown));
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
