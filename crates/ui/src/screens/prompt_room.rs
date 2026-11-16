//! The start prompt of a multiplayer room's stage (M7b, the maintainer's
//! match flow; `docs/plans/multiplayer-prototype-progress.md`): the
//! readiness check. Its Start is **Ready** (one press), then **Cancel
//! ready**, which, as Back then, takes the game's deliberate gesture, held
//! or pressed twice ([`Undo`], feedback round O); Ready is
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
use crate::gesture::{CANCEL_HINT, HoldOrTwice};
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
    /// Ready taken back, or the prompt left, while ready: the game's
    /// deliberate gesture (feedback round O).
    taking: RefCell<Taking>,
}

/// What the deliberate gesture does on a room's stage's prompt once the
/// player is ready (feedback round O: "too easy to cancel a ready"): one
/// press of Ready's button (Enter, Space, a controller's confirm, a click
/// or a tap) or of Back (Esc, Backspace, the Back button, a controller's
/// Back, the browser's Back) only shows its hint; held
/// [`crate::gesture::CANCEL_HOLD`] or pressed twice within
/// [`crate::gesture::CANCEL_DOUBLE_TAP`], the timings of a play's quit, it
/// is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Undo {
    /// Ready taken back (`SetReady(false)`; refused once the room's
    /// countdown to the start is on, as it always was: the status line
    /// says so).
    Unready,
    /// The prompt left: Back, the stage left for good (withdrawn, as Back
    /// always did).
    Leave,
}

impl Undo {
    fn index(self) -> usize {
        match self {
            Undo::Unready => 0,
            Undo::Leave => 1,
        }
    }

    /// The button whose background fills as it is held.
    pub(super) fn button(self) -> &'static str {
        match self {
            Undo::Unready => "Start",
            Undo::Leave => "Back",
        }
    }
}

/// What holds a gesture's control down: a menu button (a key, a
/// controller's button) by its source, or a pointer on the button.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Holder {
    Menu(crate::nav::Source),
    Pointer,
}

/// How long a released hold's fill runs back from full, seconds.
const RUN_BACK: f32 = 0.2;
/// How long the hint's pulse after a single press lasts, seconds.
const PULSE: f64 = 0.25;

/// The two gestures' state ([`Undo`]) on the UI's clock.
#[derive(Debug, Default)]
struct Taking {
    gestures: [HoldOrTwice; 2],
    holders: [Option<Holder>; 2],
    /// The one pressed last (its hint shows), and when.
    last: Option<(Undo, f64)>,
    /// A pointer's press on a button made while ready: the rest of its
    /// gesture (its tap, its release) is the gesture's, never a press of
    /// the button, even once Ready is taken back.
    pointer: Option<Undo>,
    /// The fills shown on the buttons (`Undo::index`), 0 to 1.
    fill: [f32; 2],
}

impl Taking {
    fn reset(&mut self) {
        let pointer = self.pointer;
        *self = Taking {
            pointer,
            ..Taking::default()
        };
    }

    fn idle(&self) -> bool {
        self.last.is_none() && self.fill == [0.0; 2] && self.holders.iter().all(Option::is_none)
    }
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
            taking: RefCell::default(),
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
        } else if r.ready() && !r.may_take_ready_back() {
            // An online room's start is committed: nothing to take back
            // (round P1b: the button no longer offers it).
            msg::room::prompt_starting()
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
        // This device stopped for another of the person's (round P1b).
        if cx.is_current_screen() && rm.room.borrow().take_opened_elsewhere() {
            room::tell_opened_elsewhere(&self.env, &rm.room, cx);
        }
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
                    room::sat_out_status(&r, stage, reason)
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

    /// Whether a press of Ready's button or of Back is the deliberate
    /// gesture's ([`Undo`]): the player is ready for this stage, which the
    /// room has neither started nor let go of for them (its countdown to
    /// the start included).
    pub(super) fn room_armed(&self) -> bool {
        self.room.as_ref().is_some_and(|rm| {
            !rm.started.get() && rm.gone.get().is_none() && rm.quit_at.get().is_none() && {
                let r = rm.room.borrow();
                r.in_stage(rm.stage) && r.ready()
            }
        })
    }

    /// A press of `undo`'s control, held by `holder` (none: an activation,
    /// which has no release): while ready, its hint, or the gesture done
    /// on a second press in time. Whether it was the gesture's (not
    /// ready, the press does what it always did: one readies).
    pub(super) fn room_press(&self, undo: Undo, holder: Option<Holder>, cx: &mut Cx) -> bool {
        let Some(rm) = &self.room else {
            return false;
        };
        if !self.room_armed() && !self.room_loading_armed() {
            return false;
        }
        // Ready past taking back (an online room's countdown): the press
        // does nothing, and shows no hint that would say otherwise.
        if undo == Undo::Unready && !self.room_may_take_back() {
            return true;
        }
        let now = cx.time();
        let twice = {
            let mut t = rm.taking.borrow_mut();
            let i = undo.index();
            let twice = t.gestures[i].edge(true, now);
            t.gestures[i].held = holder.is_some();
            t.holders[i] = holder;
            t.last = Some((undo, now));
            twice
        };
        if twice {
            self.room_undo(undo, cx);
        }
        true
    }

    /// Whether Ready may be taken back now (`RoomClient::may_take_ready_back`):
    /// CPU practice's countdown is canceled by it, an online room's
    /// committed start is not.
    pub(super) fn room_may_take_back(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| rm.room.borrow().may_take_ready_back())
    }

    /// A control let go: the holds it held end, their fills running back.
    pub(super) fn room_release(&self, holder: &Holder) {
        let Some(rm) = &self.room else {
            return;
        };
        let mut t = rm.taking.borrow_mut();
        for i in 0..2 {
            if t.holders[i].as_ref() == Some(holder) {
                t.holders[i] = None;
                t.gestures[i].edge(false, 0.0);
            }
        }
    }

    /// Every hold let go (the window lost the focus: a release may never
    /// come).
    pub(super) fn room_release_all(&self) {
        if let Some(rm) = &self.room {
            let mut t = rm.taking.borrow_mut();
            for i in 0..2 {
                t.holders[i] = None;
                t.gestures[i].edge(false, 0.0);
            }
        }
    }

    /// The gesture done: Ready taken back (the room refuses it once its
    /// countdown to the start is on, and the status line says so), or the
    /// prompt left as Back always left it.
    fn room_undo(&self, undo: Undo, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        rm.taking.borrow_mut().reset();
        match undo {
            Undo::Unready => {
                let _ = rm.room.borrow_mut().set_ready(false);
            }
            Undo::Leave => self.choose(PromptChoice::Back, cx),
        }
    }

    /// What a pointer, a menu button or the focus's loss does to the
    /// gesture (`None`: the input goes on as ever). A pointer pressed on
    /// Ready's button or on Back while ready begins its gesture there,
    /// the rest of that pointer's gesture its own (a release, a slide off
    /// the button: the hold ends; its tap never presses the button); a tap
    /// on the card's background, which readies, does nothing while ready.
    pub(super) fn room_gesture_input(&self, input: &Input, root: &Actor, cx: &mut Cx) -> Option<Handled> {
        let rm = self.room.as_ref()?;
        let owned = rm.taking.borrow().pointer;
        if let (Some(undo), Input::Pointer { gesture, pos, .. }) = (owned, input)
            && *gesture != Gesture::Press
        {
            match gesture {
                Gesture::Tap
                | Gesture::Release
                | Gesture::DragEnd { .. }
                | Gesture::Flick { .. }
                | Gesture::Cancel => {
                    rm.taking.borrow_mut().pointer = None;
                    self.room_release(&Holder::Pointer);
                }
                // A finger (or the mouse) slid off the button: no hold.
                Gesture::Drag { .. }
                    if !crate::hit::target_at(root, &crate::hit::IDENTITY, *pos)
                        .is_some_and(|t| t.within(undo.button())) =>
                {
                    self.room_release(&Holder::Pointer);
                }
                _ => {}
            }
            return Some(Handled::Yes);
        }
        match input {
            Input::MenuRelease { source } => {
                self.room_release(&Holder::Menu(source.clone()));
                Some(Handled::Yes)
            }
            Input::CancelDirections => {
                self.room_release_all();
                None
            }
            _ if !self.room_armed() => None,
            Input::Menu {
                action: action @ (MenuAction::Start | MenuAction::Back),
                repeat: false,
            } => {
                let undo = match (action, cx.focused()) {
                    (MenuAction::Back, _) | (_, Some("Back")) => Undo::Leave,
                    // The focus's own button (Save, the timing line's link).
                    (_, Some("SaveOptions" | "Calibrate")) => return None,
                    _ => Undo::Unready,
                };
                let holder = cx.menu_source().cloned().map(Holder::Menu);
                self.room_press(undo, holder, cx).then_some(Handled::Yes)
            }
            Input::Activate { name } if name == "Start" || name == "Back" => {
                let undo = if name == "Start" { Undo::Unready } else { Undo::Leave };
                self.room_press(undo, None, cx).then_some(Handled::Yes)
            }
            Input::Pointer {
                target,
                gesture: Gesture::Press,
                ..
            } => {
                let undo = [Undo::Unready, Undo::Leave]
                    .into_iter()
                    .find(|u| target.as_ref().is_some_and(|t| t.within(u.button())))?;
                rm.taking.borrow_mut().pointer = Some(undo);
                self.room_press(undo, Some(Holder::Pointer), cx);
                Some(Handled::Yes)
            }
            Input::Pointer {
                target,
                gesture: Gesture::Tap,
                ..
            } if !self.on_control(target.as_ref()) => Some(Handled::Yes),
            _ => None,
        }
    }

    /// The gestures' part of a frame: a hold done once held long enough;
    /// the fills on the buttons (following a hold, running back from a
    /// release; under reduced motion in halves, never running back); the
    /// hint under the buttons, brightened by a press (a short pulse but
    /// under reduced motion) and dimming back over [`CANCEL_HINT`] (under
    /// reduced motion at once then).
    pub(super) fn room_gesture_update(&self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        let now = cx.time();
        if !self.room_armed() && !self.room_loading_armed() {
            if !rm.taking.borrow().idle() {
                rm.taking.borrow_mut().reset();
                self.update_countdown_fill(root);
            }
            self.room_hint_shown(root, now);
            return;
        }
        let done = {
            let t = rm.taking.borrow();
            [Undo::Unready, Undo::Leave].into_iter().find(|u| {
                let i = u.index();
                t.holders[i].is_some() && t.gestures[i].progress(now) >= 1.0
            })
        };
        if let Some(undo) = done {
            self.room_undo(undo, cx);
            self.update_countdown_fill(root);
            return;
        }
        let reduced = self.env.reduced();
        {
            let mut t = rm.taking.borrow_mut();
            for i in 0..2 {
                let held = t.holders[i].is_some();
                let progress = if held {
                    t.gestures[i].progress(now) as f32
                } else {
                    0.0
                };
                t.fill[i] = if reduced {
                    (progress * 2.0).floor() / 2.0
                } else if held {
                    progress
                } else {
                    (t.fill[i] - dt / RUN_BACK).max(0.0)
                };
            }
            if let Some((undo, at)) = t.last
                && now - at > CANCEL_HINT
                && t.holders[undo.index()].is_none()
            {
                t.last = None;
            }
        }
        self.update_countdown_fill(root);
        self.room_hint_shown(root, now);
    }

    /// The fill shown on `undo`'s button, if any.
    pub(super) fn room_fill(&self, undo: Undo) -> Option<f32> {
        let rm = self.room.as_ref()?;
        let f = rm.taking.borrow().fill[undo.index()];
        (f > 0.0).then_some(f)
    }

    /// Frames while a gesture shows something (its hint brightened, a
    /// hold, a fill running back).
    pub(super) fn room_gesture_animating(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| !rm.taking.borrow().idle())
    }

    /// The hint's lines under the buttons (`ready-hint-v0`): Ready's
    /// ("Hold or press twice to cancel") while ready, Back's ("… to
    /// leave") for [`CANCEL_HINT`] after Back was pressed once or while it
    /// is held; their row kept for a player in the stage whether ready or
    /// not, so that nothing moves as Ready changes. Its actors at the
    /// row's middle `y` (right-aligned to `right`, or centered on
    /// `center`), and the row's height; none: no row.
    pub(super) fn room_hint_row(&self, y: f32, at: HintAt) -> Option<(Vec<Actor>, f32)> {
        let rm = self.room.as_ref()?;
        if rm.gone.get().is_some() || !rm.room.borrow().in_stage(rm.stage) {
            return None;
        }
        let place = |mut a: Actor| match at {
            HintAt::Right(x) => right(a, x, y),
            HintAt::Center(x) => {
                call(&mut a, "xy", &[x, y]);
                a
            }
        };
        let lines = [
            ("Room.UnreadyHint", msg::room::prompt_unready_hint()),
            ("Room.LeaveHint", msg::room::prompt_leave_hint()),
        ]
        .into_iter()
        .map(|(name, text)| {
            let mut a = self.env.text(name, &text, SMALL, 400, "Muted");
            a.visible = false;
            place(a)
        })
        .collect();
        Some((lines, 18.0))
    }

    /// The hint's lines as the gesture has them now ([`Self::room_hint_row`]).
    pub(super) fn room_hint_shown(&self, root: &mut Actor, now: f64) {
        let Some(rm) = &self.room else {
            return;
        };
        let armed = self.room_armed() || self.room_loading_armed();
        let (last, held) = {
            let t = rm.taking.borrow();
            (t.last, [t.holders[0].is_some(), t.holders[1].is_some()])
        };
        let leave = held[1] || matches!(last, Some((Undo::Leave, at)) if now - at <= CANCEL_HINT);
        // The loading screen has Back's hint alone, after a press.
        let loading = self.room_loading_armed();
        let shown = if leave { Undo::Leave } else { Undo::Unready };
        // How bright: 1 just pressed or held, fading over the hint's time
        // (under reduced motion, bright for it, then at once not).
        let age = last
            .filter(|(u, _)| *u == shown)
            .map(|(_, at)| now - at);
        let bright = if held[shown.index()] {
            1.0
        } else {
            match age {
                Some(a) if self.env.reduced() => f32::from(a <= CANCEL_HINT),
                Some(a) => (1.0 - a / CANCEL_HINT).clamp(0.0, 1.0) as f32,
                None => 0.0,
            }
        };
        let pulse = match age {
            Some(a) if !self.env.reduced() && a < PULSE => 1.0 + 0.08 * (1.0 - a / PULSE) as f32,
            _ => 1.0,
        };
        let [mr, mg, mb, ma] = self.env.color("Muted");
        let [tr, tg, tb, ta] = self.env.color("Text");
        let mix = |m: f32, t: f32| m + (t - m) * bright;
        let color = [mix(mr, tr), mix(mg, tg), mix(mb, tb), mix(ma, ta)];
        for undo in [Undo::Unready, Undo::Leave] {
            let name = match undo {
                Undo::Unready => "Room.UnreadyHint",
                Undo::Leave => "Room.LeaveHint",
            };
            if let Some(a) = crate::hit::find_mut(root, name) {
                // Ready's hint only where Ready may still be taken back;
                // the loading screen has Back's alone, after a press.
                a.visible = armed
                    && undo == shown
                    && (undo == Undo::Leave || self.room_may_take_back())
                    && (leave || !loading);
                if a.visible {
                    call(a, "diffuse", &color);
                    call(a, "zoom", &[pulse]);
                }
            }
        }
    }

    /// Whether the room's prompt hears every menu button's release
    /// ([`crate::Screen::menu_releases`]): for its holds.
    pub(super) fn room_holds(&self) -> bool {
        self.room.is_some()
    }

    /// Whether Back on the loading screen is the deliberate gesture's
    /// ([`Undo::Leave`], feedback round P2): the room started the stage
    /// with the player in it, so leaving now leaves a stage that has begun.
    pub(super) fn room_loading_armed(&self) -> bool {
        self.state.borrow().step == PromptStep::Loading
            && self.room.as_ref().is_some_and(|rm| {
                rm.started.get()
                    && rm.gone.get().is_none()
                    && rm.quit_at.get().is_none()
                    && rm.room.borrow().in_stage(rm.stage)
            })
    }

    /// The loading screen's input where its Back is the gesture's
    /// ([`Self::room_loading_armed`]): Back pressed (its hint, or done on
    /// a second press in time) and let go (a hold ended); `None`: the
    /// loading screen's own.
    pub(super) fn room_loading_input(&self, input: &Input, cx: &mut Cx) -> Option<Handled> {
        if !self.room_loading_armed() {
            return None;
        }
        match input {
            Input::Menu {
                action: MenuAction::Back,
                repeat,
            } => {
                if !repeat {
                    let holder = cx.menu_source().cloned().map(Holder::Menu);
                    self.room_press(Undo::Leave, holder, cx);
                }
                Some(Handled::Yes)
            }
            Input::MenuRelease { source } => {
                self.room_release(&Holder::Menu(source.clone()));
                Some(Handled::Yes)
            }
            Input::CancelDirections => {
                self.room_release_all();
                None
            }
            _ => None,
        }
    }
}

/// Where the hint's lines go ([`PromptScreen::room_hint_row`]): right-aligned
/// to `x` (the buttons in a row from the right), or centered on it.
#[derive(Clone, Copy, Debug)]
pub(super) enum HintAt {
    Right(f32),
    Center(f32),
}

#[cfg(test)]
#[path = "prompt_room_tests.rs"]
mod tests;
