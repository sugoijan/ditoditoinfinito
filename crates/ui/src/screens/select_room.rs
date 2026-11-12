//! Song select as the multiplayer room's main view (M7b, the maintainer's
//! match flow; `docs/plans/multiplayer-prototype-progress.md`).
//!
//! - **The picker** (one at a time, `RoomClient::chooses`: the room's
//!   owner at first; "the host" in this module's names) sees their own song
//!   list, the real song select with the room around it: Start on a song
//!   picks it for the next (or first) stage and starts the match or goes on
//!   with it (`RoomClient::pick`); the song it rests on is told to the room
//!   once it rested (`RoomClient::hover`, `SPOTLIGHT_REST_MS`). Nothing is
//!   added, edited, managed or shared from here.
//! - **The others** follow, co-hosts and the owner included when they are
//!   not the picker: the song side shows the picker's resting song (its
//!   card at once, its banner and preview once the room's share of it is
//!   read), and the list's place holds the room's panel: the status and the
//!   match's picks (on a phone the players too), scrolling with Up and
//!   Down, the wheel or its marks. They cannot pick.
//! - **Taking the pick over** (2026-11-12): an operator who follows has
//!   Pick songs (`RoomClient::may_take_pick`): a button in the header where
//!   the picker's search is (`room::TAKE_PICK`), the room's More, and T
//!   with its hint in the footer. It is immediate: this player's own list
//!   comes back (kept aside while they followed, with their place, style
//!   and search: [`RoomMode::library`], [`Place`]) and the previous picker's
//!   screen becomes the following one, with the same action to take it
//!   back.
//! - **The players, always** (feedback round B): in a room the song's side
//!   is short (`Layout::for_room`: a phone's banner with the title and a
//!   line, the style button only where the room can play more than one
//!   style, one row of chart buttons and the chart chosen in words), and
//!   the players panel (`room::players_panel`) takes the room under it,
//!   the same for the host and the others. A phone has no room beside the
//!   song and the list: the host's shows them as one line over the list
//!   (`room::players_strip`), which opens the full list.
//! - **The room around it**: the header's Rules and Players (in More on a
//!   phone), the room's More (invite, stop, leave), and the status line at
//!   the top of the list's column, under the header: beside what it talks
//!   about (the pick) and never under the footer's hints, where M2's room
//!   had it. A room opens straight on this screen with the default rules
//!   (the Rules pop-up opens by itself on entering only under the hidden
//!   option `Settings::multiplayer_rules_first`, off by default; its
//!   button is always there); a stage's standings pop up once back from
//!   its results where they were not seen there; the stage's start prompt
//!   opens when a pick gathers readiness.
//!
//! - **One view, coming in once** (2026-11-12): what the screen is built
//!   as is its [`Side`], changed only by [`SelectScreen::swap_side`].
//!   Before the room says who this player is (an online room's first
//!   answer comes after the screen is built) nothing role-specific shows:
//!   the bars and, if it takes a moment, one quiet line (`RoomJoining`).
//!   Once known, the picker's or the follower's view comes in with the
//!   screen's own entrance, once, the bars kept as they are. A later change
//!   of hands is one swap: the view's parts leave, then the other view's
//!   come in (at once under reduced motion).
//!
//! A child module of `select` (it reads the screen's own fields).

use super::*;
use crate::screens::room::{self, Pump, RoomShell};
use crate::screens::room_dialogs::{NoticeDialog, PlayersDialog, RulesDialog, StandingsDialog};
use crate::state::room::{RoomView, SharedRoom};

/// The status line's height above the list.
const STATUS_H: f32 = 40.0;
/// A phone's players strip's height under it, and the room after it.
const STRIP_H: f32 = 28.0;
const STRIP_GAP: f32 = 8.0;

/// Where a player was in their own list when the others' view replaced it:
/// put back with the list when they pick again.
struct Place {
    /// The song shown and its chart (the chart's index in the song).
    song: Option<(String, Option<usize>)>,
    /// The open pack's name (`None`: the bundled songs'); none: no pack
    /// open.
    open: Option<Option<String>>,
    /// The style shown (its id) and the search.
    style: String,
    search: String,
}

/// What a room's song select shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    /// The room has not said who this player is yet (an online room
    /// before its first answer): nothing role-specific.
    Joining,
    /// This player's own list: they choose the room's songs.
    Picker,
    /// The picker's resting song and the room's panel.
    Follower,
}

impl Side {
    /// The side the room says this player is on now.
    fn of(r: &crate::state::room::RoomClient) -> Side {
        if r.state().member(r.me()).is_none() {
            Side::Joining
        } else if r.chooses() {
            Side::Picker
        } else {
            Side::Follower
        }
    }
}

/// The view's own parts, which leave and come in when it changes hands
/// (the bars stay; the backdrop follows the song shown).
const SIDE_PARTS: [&str; 6] = [
    "Wheel",
    "Wheel.Track",
    "Wheel.Thumb",
    "RoomParts",
    "Song",
    "RoomPlayers",
];

/// The quiet line shown while the room is not known yet.
const JOINING: &str = "RoomJoining";
/// How long joining goes on before the line shows: a room that answers
/// at once shows none.
const JOINING_AFTER: f32 = 0.4;

/// Song select's room: the client, the shell's calls and what was drawn.
pub(super) struct RoomMode {
    pub(super) room: SharedRoom,
    pub(super) shell: RoomShell,
    /// What the screen is built as (its data with it: the player's own
    /// list, or the song followed).
    side: Side,
    /// A change of hands under way: when the leaving view's parts are
    /// out and the other view is built.
    leaving: Option<f64>,
    pump: Pump,
    /// The room's version and the seconds left the parts were built for.
    built: Option<(u64, Option<u64>, bool)>,
    /// The others' panel: its first row.
    panel_first: usize,
    /// The song the others' side shows (its content's chart key, read or
    /// not), to set it again only when it changes.
    following: Option<(String, bool)>,
    /// The player's own list, kept while the others' view replaces it
    /// with the picker's resting song: put back when this player picks
    /// (the pick taken or come back to them, or an online room's first
    /// snapshot saying they pick, which comes after the screen is built).
    pub(super) library: Option<SelectData>,
    /// Where they were in it when they began to follow.
    place: Option<Place>,
    /// What was told the room the song select rests on.
    hovered: Option<(String, usize)>,
    /// The results whose standings were opened (a stage's number, or the
    /// match's), until shown.
    standings: Option<Option<u16>>,
    /// The UI's clock at the last update ([`SelectScreen::room_wake_at`]).
    time: f64,
}

impl SelectScreen {
    /// Song select as room `room`'s main view (M7b), its shell's calls
    /// through `shell`: the host's list to pick from, or the others'
    /// following view. The data's list is the player's library (the
    /// others' is replaced by the host's resting song).
    pub fn with_room(mut self, room: SharedRoom, shell: RoomShell) -> SelectScreen {
        let side = Side::of(&room.borrow());
        self.room = Some(RoomMode {
            room,
            shell,
            side,
            leaving: None,
            pump: Pump::default(),
            built: None,
            panel_first: 0,
            // Never a chart's key: the first update sets the follower's
            // song (none yet, or the host's), not their own list.
            following: Some((String::from("\u{0}"), false)),
            library: None,
            place: None,
            hovered: None,
            standings: None,
            time: 0.0,
        });
        // The styles the room can play, the one shown kept where it is
        // among them.
        let shown = self.style_id().to_string();
        let mut styles = self.data.styles.clone();
        self.room_styles(&mut styles);
        if styles.len() != self.data.styles.len() {
            self.style = styles.iter().position(|st| st.id == shown).unwrap_or(0);
            Rc::make_mut(&mut self.data).styles = styles;
            self.tabs_for = None;
            self.pick = None;
            self.reload();
        }
        self
    }

    /// The screen shows the local player's own list to pick from (they
    /// are the room's picker); `None` outside a room.
    pub(super) fn room_host(&self) -> Option<bool> {
        self.room.as_ref().map(|rm| rm.side == Side::Picker)
    }

    /// The room has not said who this player is yet: the screen shows
    /// nothing of a view.
    pub(super) fn room_joining(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| rm.side == Side::Joining)
    }

    /// Whether this player may take the pick over now (an operator who
    /// follows): Pick songs is offered.
    pub(super) fn room_may_take(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| rm.side == Side::Follower && rm.room.borrow().may_take_pick())
    }

    /// Takes the pick over: at once, this player's own list back when the
    /// room says so.
    fn take_pick(&mut self, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        // The button goes with the following view: the focus to the list.
        cx.focus("Wheel");
        let taken = rm.room.borrow_mut().take_pick();
        if let Err(why) = taken {
            rm.room.borrow_mut().clear_refusal();
            cx.overlay(NoticeDialog::refusal(self.env.clone(), &why), true);
        }
    }

    /// The room's follower: song select shows the host's song, no list.
    pub(super) fn following(&self) -> bool {
        self.room_host() == Some(false)
    }

    /// The layout with the list's column under the status line and, on
    /// a phone, the host's players strip.
    pub(super) fn room_layout(&self, mut l: Layout) -> Layout {
        if self.room.is_some() {
            let top = STATUS_H + self.strip_height(&l);
            l.wheel[1] += top;
            l.wheel[3] -= top;
        }
        l
    }

    /// The room a phone's players strip takes over the host's list (the
    /// others' list's place holds the panel, the players in it).
    fn strip_height(&self, l: &Layout) -> f32 {
        if l.compact && self.room_host() == Some(true) {
            STRIP_H + STRIP_GAP
        } else {
            0.0
        }
    }

    /// The layout for a viewport: a room's has the song's side short and
    /// the players under it (`Layout::for_room`).
    pub(super) fn fit_layout(&self, vp: [f32; 2], compact: bool) -> Layout {
        let l = Layout::fit(&self.env, vp, compact);
        if self.room.is_some() { l.for_room() } else { l }
    }

    /// Whether the short song's side shows its style button: always on a
    /// phone; in a room's wide layout only where there is a style to
    /// change to.
    pub(super) fn style_button_shown(&self) -> bool {
        self.layout.compact || self.data.styles.len() > 1
    }

    /// Where the short song's side's chart buttons go: under the style
    /// button, or in its place where it is not shown.
    pub(super) fn charts_y(&self) -> f32 {
        if self.style_button_shown() {
            self.layout.charts_y
        } else {
            self.layout.chart_h / 2.0
        }
    }

    /// The styles a room's song select shows: those the room can play
    /// (practice's CPUs play one-player charts with four lanes), all of
    /// them where it could play none.
    pub(super) fn room_styles(&self, styles: &mut Vec<crate::screens::data::StyleDef>) {
        let Some(rm) = &self.room else {
            return;
        };
        let r = rm.room.borrow();
        if styles.iter().any(|st| r.style_playable(&st.id)) {
            styles.retain(|st| r.style_playable(&st.id));
        }
    }

    /// The room's players panel beside the song (`RoomPlayers`; the wide
    /// layout's, empty elsewhere).
    pub(super) fn room_players(&self) -> Actor {
        match (&self.room, self.layout.players) {
            (Some(rm), Some(rect)) if rm.side != Side::Joining => {
                room::players_panel(&self.env, &rm.room.borrow(), rect)
            }
            _ => frame("RoomPlayers", Vec::new()),
        }
    }

    /// The header's room buttons' width (wide layouts): Rules and Players.
    pub(super) fn room_buttons_width(&self) -> f32 {
        if self.room.is_some() && !self.layout.compact {
            let w = |t: &str| (look::mono_width(t, 15.0) + 24.0).max(72.0);
            w(&ddi_l10n::msg::room::rules()) + w(&ddi_l10n::msg::room::players()) + 24.0
        } else {
            0.0
        }
    }

    /// The room's own parts (`RoomParts`): the header's Rules and Players,
    /// the status line, and the others' panel in the list's place.
    pub(super) fn room_parts(&mut self, cx: &mut Cx) -> Actor {
        let Some(rm) = &self.room else {
            return frame("RoomParts", Vec::new());
        };
        // Nothing of a view before the room is known.
        if rm.side == Side::Joining {
            return frame("RoomParts", Vec::new());
        }
        let follower = rm.side == Side::Follower;
        let env = self.env.clone();
        let l = self.room_layout(self.layout);
        let r = rm.room.borrow();
        let mut children = Vec::new();
        if !l.compact {
            // Left of the search, where the mods were.
            let mut x = l.field[0] - 16.0;
            for (name, label) in [
                ("Room.Players", ddi_l10n::msg::room::players()),
                ("Room.Rules", ddi_l10n::msg::room::rules()),
            ] {
                let w = (look::mono_width(&label, 15.0) + 24.0).max(72.0);
                children.push(env.component(
                    "Button",
                    "Button",
                    &Spec::new([w, 40.0], label.as_str()),
                    name,
                    [x - w / 2.0, 32.0],
                ));
                x -= w + 12.0;
            }
            // An operator who follows: Pick songs, where the picker's
            // search is (a follower has none).
            if follower && r.may_take_pick() {
                let label = ddi_l10n::msg::room::take_pick();
                let w = (look::mono_width(&label, 15.0) + 24.0).max(72.0);
                children.push(env.component(
                    "Button",
                    "Button",
                    &Spec::new([w, 40.0], label.as_str()),
                    room::TAKE_PICK,
                    [l.field[0] + w / 2.0, 32.0],
                ));
            }
        }
        // The status line over the list's column.
        let [wx, wy, ww, wh] = l.wheel;
        let inset = env.metric("Select.Wheel.Inset", 0.0);
        let (sx, sw) = (wx + inset + 8.0, (ww - inset - 24.0).max(80.0));
        let text: String = room::status(&r).into();
        let mut status = env.text("Room.Status", &text, 14.0, 600, "Secondary");
        look::ellipsized(&mut status, sw);
        let strip = self.strip_height(&l);
        children.push(left(status, sx, wy - strip - STATUS_H / 2.0));
        if strip > 0.0 {
            children.push(room::players_strip(&env, &r, [sx, wy - strip, sw, STRIP_H]));
        }
        if self.following() {
            // The players are beside the song where the layout has room
            // for them; a phone's are here, under the picks.
            let rows = if l.players.is_some() {
                room::pick_panel_rows(&r)
            } else {
                room::panel_rows(&r)
            };
            drop(r);
            let (p, first) = room::panel(
                &env,
                &rows,
                self.room.as_ref().map_or(0, |rm| rm.panel_first),
                [sx, wy + 8.0, sw, wh - 16.0],
            );
            if let Some(rm) = &mut self.room {
                rm.panel_first = first;
            }
            children.push(p);
        } else {
            drop(r);
        }
        let _ = cx;
        frame("RoomParts", children)
    }

    /// The quiet line of a room not known yet (`RoomJoining`), centered
    /// under the header: unseen unless joining takes a moment, and faded
    /// out (from wherever it is) when the view comes in or the screen
    /// goes.
    pub(super) fn room_joining_line(&self) -> Option<Actor> {
        if !self.room_joining() {
            return None;
        }
        let env = &self.env;
        let [w, h] = self.layout.screen;
        // The link's own words: joining, or why it ended before the room
        // was known (closed, signed out).
        let text: String = room::status(&self.room.as_ref()?.room.borrow()).into();
        let mut line = env.text(JOINING, &text, 14.0, 600, "Secondary");
        look::ellipsized(&mut line, (w - 64.0).max(80.0));
        call(&mut line, "xy", &[w / 2.0, h / 2.0]);
        let t_in = env.metric("Motion.Screen", 0.2);
        let t_out = env.metric("Motion.Screen.Out", 0.15);
        let off = format!("stoptweening;linear,{t_out};diffusealpha,0");
        look::command_of(
            &mut line,
            "On",
            &format!("diffusealpha,0;sleep,{JOINING_AFTER};linear,{t_in};diffusealpha,1"),
        );
        look::command_of(&mut line, "Off", &off);
        look::command_of(&mut line, "Cancel", &off);
        Some(line)
    }

    /// A tree built for a room not known yet: nothing of a view shows
    /// (the list, the song's side, its backdrop, the search), only the
    /// bars and the joining line.
    pub(super) fn room_joining_view(&self, root: &mut Actor) {
        if !self.room_joining() {
            return;
        }
        for name in [
            "Wheel",
            "Wheel.Track",
            "Wheel.Thumb",
            "Song",
            "Backdrop",
            "SearchIcon",
            "AddIcon",
        ] {
            if let Some(part) = root.children_mut().iter_mut().find(|c| c.name == name) {
                part.visible = false;
            }
        }
        if let Some(line) = self.room_joining_line()
            && let Kind::Frame(c) = &mut root.kind
        {
            c.push(line);
        }
    }

    /// The room says this player is on side `to` now: the view built
    /// again as that side's, its data with it (their own list back where
    /// they pick, the song followed otherwise). The bars are kept as they
    /// are, in motion or at rest; the new view's parts come in when
    /// `enter` (else they stand as they are).
    fn swap_side(&mut self, to: Side, enter: bool, root: &mut Actor, cx: &mut Cx) {
        let from = match &mut self.room {
            Some(rm) => std::mem::replace(&mut rm.side, to),
            None => return,
        };
        match to {
            Side::Picker => self.take_back_library(),
            Side::Follower => self.follow_host(root, cx),
            Side::Joining => {}
        }
        let left = root.tween_time_left();
        let old = std::mem::replace(root, self.build_tree(cx));
        self.reload();
        // What had the focus may be gone (the search, Pick songs).
        let names = self.focusables();
        if cx
            .focused()
            .is_some_and(|f| !names.iter().any(|n| n.name == f))
            || (to != Side::Picker
                && cx
                    .focused()
                    .is_some_and(|f| f == self.search.name || f == "ClearSearch"))
        {
            cx.focus("Wheel");
        }
        if cx.holding() {
            // Under the web's loader: its entrances are held for the fade.
            crate::screens::relaid(root, left, cx);
            return;
        }
        // The bars as they were (coming in still, or at rest): only what
        // is inside them changes, at once, as between two screens.
        let (Kind::Frame(mut old), Kind::Frame(now)) = (old.kind, &mut root.kind) else {
            return;
        };
        for (name, edge) in super::SELECT_PARTS {
            if !matches!(
                edge,
                crate::screens::Edge::HeaderBar | crate::screens::Edge::FooterBar
            ) {
                continue;
            }
            let Some(at) = old.iter().position(|c| c.name == name) else {
                continue;
            };
            let mut kept = old.remove(at);
            match now.iter_mut().find(|c| c.name == name) {
                Some(fresh) => {
                    kept.visible = fresh.visible;
                    kept.kind = std::mem::replace(&mut fresh.kind, Kind::Frame(Vec::new()));
                    *fresh = kept;
                }
                // The footer comes after a tree is built: the same one.
                None if name == "Footer" => now.push(kept),
                None => {}
            }
        }
        let mut problems = ddi_actor::Problems::default();
        // The joining line goes as the view comes.
        if from == Side::Joining
            && let Some(at) = old.iter().position(|c| c.name == JOINING)
        {
            let mut line = old.remove(at);
            line.play("Off", &mut problems);
            now.push(line);
        }
        if enter {
            let backdrop = (from == Side::Joining).then_some("Backdrop");
            for name in SIDE_PARTS.into_iter().chain(backdrop) {
                if let Some(part) = now.iter_mut().find(|c| c.name == name) {
                    part.play("On", &mut problems);
                }
            }
        }
    }

    /// Replaces the room's parts in `root` (the room changed).
    fn refill_room(&mut self, root: &mut Actor, cx: &mut Cx) {
        if self.room_joining() {
            return;
        }
        // Only what is inside changes: a part coming in goes on coming in.
        let fresh = self.room_parts(cx);
        if let Some(old) = hit::find_mut(root, "RoomParts") {
            crate::screens::refill(old, fresh);
        }
        let players = self.room_players();
        if let Some(old) = hit::find_mut(root, "RoomPlayers") {
            crate::screens::refill(old, players);
        }
        if let Some(w) = hit::find_mut(root, "Wheel") {
            w.visible = !self.following();
        }
        self.publish_dirty = true;
    }

    /// The room's per-frame part of `update` (before anything else: the
    /// songs to prepare and upload go to the shell whatever covers the
    /// screen). Where it is the current screen: the Rules pop-up once,
    /// results not seen yet, the stage's prompt; the others' song kept on
    /// the host's; the host's resting song told the room.
    pub(super) fn room_update(&mut self, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &mut self.room else {
            return;
        };
        let (room, shell) = (rm.room.clone(), rm.shell.clone());
        rm.time = cx.time();
        let changed = rm.pump.run(&room, &shell, cx);
        if !cx.is_current_screen() {
            return;
        }
        // The Rules once the room is known (an online room's first
        // snapshot: the player's role, its rules).
        let known = {
            let r = room.borrow();
            r.link_state() == ddi_room::LinkState::Connected && r.state().member(r.me()).is_some()
        };
        if known && !room.borrow().rules_seen() {
            room.borrow_mut().set_rules_seen(true);
            // Only under the hidden option (`Settings::multiplayer_rules_first`,
            // off by default): a room opens straight on its song select,
            // its Rules behind their button.
            if crate::state::settings::Settings::cached().multiplayer_rules_first {
                self.cancel_navigation();
                cx.overlay(RulesDialog::first(self.env.clone(), room.clone()), true);
                return;
            }
        }
        let unseen = room.borrow().unseen_results();
        if let Some(rm) = &mut self.room
            && unseen != rm.standings
        {
            rm.standings = unseen;
            if let Some(stage) = unseen {
                self.cancel_navigation();
                cx.overlay(
                    StandingsDialog::new(self.env.clone(), room.clone(), stage),
                    true,
                );
                return;
            }
        }
        if Pump::open_prompt(&room, &shell, cx) {
            return;
        }
        // The header names the room: its name comes after the room, and
        // changes in place (the bars stay put).
        let heading = String::from(room::heading(&room.borrow()));
        if self.room_built_heading.as_deref() != Some(heading.as_str()) {
            if self.room_built_heading.is_some()
                && !self.layout.compact
                && let Some(old) = hit::find_mut(root, "Header")
            {
                let fresh = header(&self.env, cx, self.layout.screen[0], &heading);
                crate::screens::refill(old, fresh);
            }
            self.room_built_heading = Some(heading);
            self.left_dirty = true;
        }
        // The side the room says this player is on, against the one
        // shown: known at last, or the pick changed hands.
        let now = Side::of(&room.borrow());
        let (shown, leaving) = match &self.room {
            Some(rm) => (rm.side, rm.leaving),
            None => return,
        };
        if now != shown && now != Side::Joining {
            // From nothing the view comes in; from the other view, its
            // parts leave first, then this one's come in: one swap (at
            // once under reduced motion, and under the web's loader).
            let at_once = shown == Side::Joining || self.env.reduced() || cx.holding();
            if !at_once {
                match leaving {
                    None => {
                        let mut problems = ddi_actor::Problems::default();
                        for name in SIDE_PARTS {
                            if let Some(part) =
                                root.children_mut().iter_mut().find(|c| c.name == name)
                            {
                                part.play("Off", &mut problems);
                            }
                        }
                        let out = f64::from(self.env.metric("Motion.Screen.Out", 0.15));
                        if let Some(rm) = &mut self.room {
                            rm.leaving = Some(cx.time() + out);
                        }
                        self.cancel_navigation();
                        return;
                    }
                    Some(at) if cx.time() < at => return,
                    Some(_) => {}
                }
            }
            if let Some(rm) = &mut self.room {
                rm.leaving = None;
                rm.built = None;
            }
            let enter = shown == Side::Joining || !self.env.reduced();
            self.swap_side(now, enter, root, cx);
        } else if leaving.is_some() {
            // Changed back before the swap (taken and returned at once):
            // the same view's parts come back in.
            if let Some(rm) = &mut self.room {
                rm.leaving = None;
            }
            let mut problems = ddi_actor::Problems::default();
            for name in SIDE_PARTS {
                if let Some(part) = root.children_mut().iter_mut().find(|c| c.name == name) {
                    part.play("On", &mut problems);
                }
            }
        }
        let side = self.room.as_ref().map_or(Side::Joining, |rm| rm.side);
        match side {
            Side::Picker => self.tell_hover(),
            Side::Follower => self.follow_host(root, cx),
            // Nothing before the room says who this player is, but what
            // the link says of itself (it may end before the room is
            // known: closed, signed out).
            Side::Joining => {
                if changed {
                    let text: String = room::status(&room.borrow()).into();
                    set_text(root, JOINING, &text);
                }
                return;
            }
        }
        let seconds = room::seconds_left(&room.borrow());
        let key = (room.borrow().version(), seconds, side == Side::Picker);
        let rebuilt = self.room.as_ref().is_some_and(|rm| rm.built != Some(key));
        if changed || rebuilt {
            if let Some(rm) = &mut self.room {
                rm.built = Some(key);
            }
            self.refill_room(root, cx);
        }
    }

    /// A room's messages come while nothing moves (an online room's
    /// socket, the window's seconds, the spotlight's rest): looked for ten
    /// times a second, as M2's room screen did.
    pub(super) fn room_wake_at(&self) -> Option<f64> {
        self.room.as_ref().map(|rm| rm.time + 0.1)
    }

    /// The host's song and chart shown, told the room when they change.
    fn tell_hover(&mut self) {
        let (charts, pick) = self.charts();
        let chart = pick.and_then(|p| charts.get(p).copied()).cloned();
        let song = self.song().cloned();
        let key = song
            .as_ref()
            .zip(chart.as_ref())
            .map(|(s, c)| (s.id.clone(), c.index));
        let Some(rm) = &mut self.room else {
            return;
        };
        if rm.hovered == key {
            // Still the same: the client counts its rest.
            return;
        }
        rm.hovered = key;
        rm.room
            .borrow_mut()
            .hover(song.as_ref().zip(chart.as_ref()));
    }

    /// The others' side follows the picker's resting song: its card first,
    /// the song as the room's share has it once read.
    fn follow_host(&mut self, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &mut self.room else {
            return;
        };
        if rm.side != Side::Follower {
            return;
        }
        let r = rm.room.borrow();
        let now = r.state().spotlight.as_ref().map(|s| {
            let read = r.spotlight().is_some_and(|(_, p)| p.is_some());
            (s.content.chart.as_str().to_string(), read)
        });
        if rm.following == now {
            return;
        }
        rm.following = now;
        let (song, chart) = match r.spotlight() {
            Some((_, Some(p))) => (p.song.clone(), p.chart.index),
            Some((card, None)) => (placeholder(card), 0),
            None => (SongEntry::default(), 0),
        };
        drop(r);
        let mut data = SelectData {
            mods: self.data.mods.clone(),
            preferred: self.data.preferred,
            autoplay: self.data.autoplay,
            ..SelectData::default()
        };
        if !song.id.is_empty() {
            let mut styles: Vec<String> = Vec::new();
            for c in &song.charts {
                if !styles.contains(&c.style) {
                    styles.push(c.style.clone());
                }
            }
            data.styles = styles
                .into_iter()
                .map(|id| {
                    let name = ddi_chart::Layout::builtin(&id).map_or(id.clone(), |l| l.name);
                    crate::screens::data::StyleDef {
                        name: crate::screens::data::style_name(&id, &name).into(),
                        id,
                    }
                })
                .collect();
            data.style = song
                .charts
                .iter()
                .find(|c| c.index == chart)
                .map(|c| c.style.clone());
            data.resume = Some((song.id.clone(), Some(chart)));
            data.songs = vec![song];
        }
        // The player's own list is kept for when they pick, with where
        // they were in it; its search does not filter the song followed.
        if self.room.as_ref().is_some_and(|rm| rm.library.is_none()) {
            if self.typing {
                self.leave_typing(root, cx);
            }
            let (charts, pick) = self.charts();
            let chart = pick.and_then(|p| charts.get(p)).map(|c| c.index);
            let place = Place {
                song: self.song().map(|s| (s.id.clone(), chart)),
                open: self
                    .open_pack
                    .and_then(|p| self.packs.get(p))
                    .map(|(name, _)| name.clone()),
                style: self.style_id().to_string(),
                search: std::mem::take(&mut self.search.text),
            };
            if let Some(rm) = &mut self.room {
                rm.library = Some((*self.data).clone());
                rm.place = Some(place);
            }
        }
        self.cancel_navigation();
        self.set_data(data, false);
        self.fill_tabs(root);
        self.fill_charts(root);
        self.left_dirty = true;
    }

    /// This player picks now (the room said so at last, or the pick was
    /// taken or came back): their own list back in the place of the song
    /// they followed, where they left it (the song and chart shown, the
    /// open pack, the style, the search).
    fn take_back_library(&mut self) {
        let Some(rm) = &mut self.room else {
            return;
        };
        // Followed again from scratch should the pick go.
        rm.following = Some((String::from("\u{0}"), false));
        // The song shown is told the room again: its spotlight went with
        // the last picker.
        rm.hovered = None;
        let place = rm.place.take();
        let Some(mut library) = rm.library.take() else {
            return;
        };
        if let Some(p) = &place {
            // Every pack closed but the one that was open.
            let mut closed: Vec<Option<String>> = Vec::new();
            for song in &library.songs {
                if p.open.as_ref() != Some(&song.pack) && !closed.contains(&song.pack) {
                    closed.push(song.pack.clone());
                }
            }
            library.closed = closed;
            self.resume = p.song.clone();
            self.search.text = p.search.clone();
        }
        self.cancel_navigation();
        self.set_data(library, false);
        let Some(p) = place else {
            return;
        };
        if let Some(i) = self.data.styles.iter().position(|st| st.id == p.style)
            && i != self.style
        {
            self.style = i;
            self.tabs_for = None;
            self.pick = None;
            self.reload();
        }
        if let Some((id, Some(chart))) = &p.song
            && self.song().is_some_and(|song| song.id == *id)
        {
            let (charts, _) = self.charts();
            if let Some(k) = charts.iter().position(|c| c.index == *chart) {
                self.preferred = charts[k].difficulty;
                self.pick = Some(k);
            }
        }
    }

    /// Start on a song in the room: the host's pick (the shell's play
    /// function then does what the press must, as keeping the audio
    /// context for the prompt); the others' does nothing.
    pub(super) fn room_start(&mut self, song: &SongEntry, chart: &ChartEntry, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        // Theirs to pick as the room has it now, whatever the screen
        // still shows while a change of hands is under way.
        if self.room_host() != Some(true) || !rm.room.borrow().chooses() {
            return;
        }
        let picked = rm.room.borrow_mut().pick(song, chart);
        match picked {
            Ok(()) => {
                let env = self.env.clone();
                (self.play)(song, chart, &env, cx);
            }
            Err(why) => {
                rm.room.borrow_mut().clear_refusal();
                cx.overlay(NoticeDialog::refusal(self.env.clone(), &why), true);
            }
        }
    }

    /// Back from the room's song select: to the multiplayer home, the room
    /// kept; its Rules open again when the player comes back.
    pub(super) fn room_back(&mut self, cx: &mut Cx) -> bool {
        let Some(rm) = &self.room else {
            return false;
        };
        rm.room.borrow_mut().set_rules_seen(false);
        (rm.shell)(room::RoomCall::Leave, cx);
        true
    }

    /// The room's More menu in song select's place.
    pub(super) fn room_more(&self, cx: &mut Cx) -> bool {
        let Some(rm) = &self.room else {
            return false;
        };
        room::open_more(&self.env, &rm.room, self.layout.compact, cx);
        true
    }

    fn open_rules(&self, cx: &mut Cx) {
        if let Some(rm) = &self.room {
            cx.overlay(RulesDialog::new(self.env.clone(), rm.room.clone()), true);
        }
    }

    fn open_players(&self, cx: &mut Cx) {
        if let Some(rm) = &self.room {
            cx.overlay(PlayersDialog::new(self.env.clone(), rm.room.clone()), true);
        }
    }

    /// The room's buttons and the panel's marks.
    pub(super) fn room_activate(
        &mut self,
        name: &str,
        root: &mut Actor,
        cx: &mut Cx,
    ) -> Option<Handled> {
        self.room.as_ref()?;
        match name {
            // The list keeps the focus under them, as the mods' Options:
            // Enter after them picks, not opens them again.
            "Room.Rules" => {
                cx.focus("Wheel");
                self.open_rules(cx)
            }
            "Room.Players" => {
                cx.focus("Wheel");
                self.open_players(cx)
            }
            room::TAKE_PICK => self.take_pick(cx),
            "Panel.Up" => self.scroll_panel(-3, root, cx),
            "Panel.Down" => self.scroll_panel(3, root, cx),
            _ => return None,
        }
        Some(Handled::Yes)
    }

    fn scroll_panel(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) {
        if let Some(rm) = &mut self.room {
            rm.panel_first = (rm.panel_first as i64 + i64::from(by)).max(0) as usize;
        }
        self.refill_room(root, cx);
    }

    /// The others' Up and Down and wheel scroll the panel (there is no
    /// list to move).
    pub(super) fn room_input(
        &mut self,
        input: &Input,
        root: &mut Actor,
        cx: &mut Cx,
    ) -> Option<Handled> {
        // A tap on the players (the panel's rows, its line of those that
        // do not fit, a phone's strip) opens the full list.
        if let Input::Pointer {
            target: Some(t),
            gesture: crate::Gesture::Tap,
            ..
        } = input
            && self.room.is_some()
            && (t.within("RoomPlayers") || t.within(room::PLAYERS_STRIP))
        {
            self.open_players(cx);
            return Some(Handled::Yes);
        }
        if !self.following() {
            return None;
        }
        // Nothing to move or pick before the room is known: Back and More
        // alone.
        if self.room_joining() {
            return match input {
                Input::Key(k) if !matches!(k.key.as_str(), "Escape" | "Backspace") => {
                    Some(Handled::Yes)
                }
                Input::Menu { action, .. }
                    if !matches!(action, MenuAction::Back | MenuAction::More) =>
                {
                    Some(Handled::Yes)
                }
                Input::Wheel { .. } => Some(Handled::Yes),
                _ => None,
            };
        }
        // T: an operator who follows takes the pick over.
        if let Input::Key(k) = input
            && self.letter_shortcut(k)
            && k.key.eq_ignore_ascii_case("t")
            && self.room_may_take()
        {
            self.take_pick(cx);
            return Some(Handled::Yes);
        }
        let by = match input {
            Input::Menu {
                action: MenuAction::Up,
                ..
            } => -1,
            Input::Menu {
                action: MenuAction::Down,
                ..
            } => 1,
            Input::Wheel { delta, .. } if delta[1] != 0.0 => {
                if delta[1] > 0.0 {
                    1
                } else {
                    -1
                }
            }
            _ => return None,
        };
        self.scroll_panel(by, root, cx);
        Some(Handled::Yes)
    }

    /// The room's messages: More's picks, rules applied.
    pub(super) fn room_message(&mut self, msg: &Message, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        let (room, shell) = (rm.room.clone(), rm.shell.clone());
        if msg.name == crate::screens::room_dialogs::RULES_SAVED {
            self.refill_room(root, cx);
        }
        if let Some(id) = crate::screens::menu_pick(msg, room::MORE) {
            match room::more_picked(id, &room, &shell, cx) {
                room::MoreAction::Rules => self.open_rules(cx),
                room::MoreAction::Players => self.open_players(cx),
                room::MoreAction::Done => self.refill_room(root, cx),
            }
        }
    }

    /// The footer's Start hint in a room: the picker's pick, or none.
    pub(super) fn room_play_hint(&self) -> Option<Option<UiText>> {
        let rm = self.room.as_ref()?;
        let r = rm.room.borrow();
        Some(
            r.chooses()
                .then(|| ddi_l10n::msg::room::pick_use(r.next_pick_number())),
        )
    }

    /// Says what the room shows (`ThemeState::room`).
    pub(super) fn room_publish(&self, items: Vec<String>) {
        if let Some(rm) = &self.room {
            let view = match rm.room.borrow().view() {
                RoomView::Select => "select",
                RoomView::Prompt(_) => "prompt",
            };
            room::publish(&self.env, &rm.room, view, items);
        }
    }
}

/// A song the others know only by the host's card: its title and artist,
/// its chart's difficulty and meter.
fn placeholder(card: &ddi_room::ContentCard) -> SongEntry {
    let difficulty =
        ddi_chart::Difficulty::from_name(&card.difficulty).unwrap_or(ddi_chart::Difficulty::Edit);
    SongEntry {
        id: "room:spotlight".into(),
        title: card.title.clone(),
        artist: card.artist.clone(),
        charts: vec![ChartEntry {
            style: card.style.clone(),
            difficulty,
            meter: card.meter,
            index: 0,
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    }
}
