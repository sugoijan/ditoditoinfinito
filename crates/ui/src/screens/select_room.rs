//! Song select as the multiplayer room's main view (M7b, the maintainer's
//! match flow; `docs/plans/multiplayer-prototype-progress.md`).
//!
//! - **The host** (an operator) sees their own song list, the real song
//!   select with the room around it: Start on a song picks it for the next
//!   (or first) stage and starts the match or goes on with it
//!   (`RoomClient::pick`); the song it rests on is told to the room once it
//!   rested (`RoomClient::hover`, `SPOTLIGHT_REST_MS`). Nothing is added,
//!   edited, managed or shared from here.
//! - **The others** follow: the song side shows the host's resting song
//!   (its card at once, its banner and preview once the room's share of it
//!   is read), and the list's place holds the room's panel: the status and
//!   the match's picks (on a phone the players too), scrolling with Up and
//!   Down, the wheel or its marks. They cannot pick.
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

/// Song select's room: the client, the shell's calls and what was drawn.
pub(super) struct RoomMode {
    pub(super) room: SharedRoom,
    pub(super) shell: RoomShell,
    pump: Pump,
    /// The room's version and the seconds left the parts were built for.
    built: Option<(u64, Option<u64>, bool)>,
    /// The others' panel: its first row.
    panel_first: usize,
    /// The song the others' side shows (its content's chart key, read or
    /// not), to set it again only when it changes.
    following: Option<(String, bool)>,
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
        self.room = Some(RoomMode {
            room,
            shell,
            pump: Pump::default(),
            built: None,
            panel_first: 0,
            // Never a chart's key: the first update sets the follower's
            // song (none yet, or the host's), not their own list.
            following: Some((String::from("\u{0}"), false)),
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

    /// The local player picks here (an operator); `None` outside a room.
    pub(super) fn room_host(&self) -> Option<bool> {
        self.room
            .as_ref()
            .map(|rm| rm.room.borrow().may(ddi_room::Capability::ChooseContent))
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
            (Some(rm), Some(rect)) => room::players_panel(&self.env, &rm.room.borrow(), rect),
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

    /// Replaces the room's parts in `root` (the room changed).
    fn refill_room(&mut self, root: &mut Actor, cx: &mut Cx) {
        let fresh = self.room_parts(cx);
        if let Some(old) = hit::find_mut(root, "RoomParts") {
            *old = fresh;
            old.finish_tweening();
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
        let host = self.room_host() == Some(true);
        if host {
            self.tell_hover();
        } else {
            self.follow_host(root);
        }
        let seconds = room::seconds_left(&room.borrow());
        let key = (room.borrow().version(), seconds, host);
        // The header names the room: its name comes after the room.
        let heading = String::from(room::heading(&room.borrow()));
        if self.room_built_heading.as_deref() != Some(heading.as_str()) {
            self.room_built_heading = Some(heading);
            self.room_built_host = None;
            if let Some(rm) = &mut self.room {
                rm.built = None;
            }
        }
        let rebuilt = self.room.as_ref().is_some_and(|rm| rm.built != Some(key));
        if changed || rebuilt {
            if let Some(rm) = &mut self.room {
                rm.built = Some(key);
            }
            // The host's view changed hands (a role given or taken): the
            // whole screen again.
            if self.room_built_host != Some(host) {
                self.room_built_host = Some(host);
                let left = root.tween_time_left();
                *root = self.build_tree(cx);
                self.reload();
                crate::screens::relaid(root, left, cx);
            } else {
                self.refill_room(root, cx);
            }
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

    /// The others' side follows the host's resting song: its card first,
    /// the song as the room's share has it once read.
    fn follow_host(&mut self, root: &mut Actor) {
        let Some(rm) = &mut self.room else {
            return;
        };
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
        self.cancel_navigation();
        self.set_data(data, false);
        self.fill_tabs(root);
        self.fill_charts(root);
        self.left_dirty = true;
    }

    /// Start on a song in the room: the host's pick (the shell's play
    /// function then does what the press must, as keeping the audio
    /// context for the prompt); the others' does nothing.
    pub(super) fn room_start(&mut self, song: &SongEntry, chart: &ChartEntry, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        if self.room_host() != Some(true) {
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

    /// The footer's Start hint in a room: the host's pick, or none.
    pub(super) fn room_play_hint(&self) -> Option<Option<UiText>> {
        let rm = self.room.as_ref()?;
        let r = rm.room.borrow();
        Some(
            r.may(ddi_room::Capability::ChooseContent)
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
