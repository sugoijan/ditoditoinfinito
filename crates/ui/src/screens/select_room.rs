//! Song select as the multiplayer room's main view (M7b, the maintainer's
//! match flow; `docs/plans/multiplayer-prototype-progress.md`).
//!
//! - **An online room (room model revision 17, round RL1)**: two tabs over
//!   the list, **Room songs** (the room's own song list, the same for
//!   everyone and updated as it changes, where the screen opens) and **My
//!   songs** (the player's own library). The one who chooses the room's
//!   songs picks on Room songs (Enter: "Pick for stage N",
//!   `RoomClient::pick_listed`), from there on as before; on My songs
//!   Enter only adds the song to the room's list, for who may add
//!   (`RoomClient::add_song`: the hosts in this stage), and the others
//!   browse it. Nobody follows the picker's resting song any more: its
//!   entry is marked in the list (`RoomClient::spotlight_entry`). Each
//!   row says the entry's state for this player and who added it; an
//!   empty list says what to do. M switches the tabs (More too: a
//!   controller's West, a phone), and each tab keeps its own place.
//! - **CPU practice** has no list: the player's own list, the real song
//!   select with the room around it: Start on a song picks it for the
//!   next (or first) stage and starts the match or goes on with it
//!   (`RoomClient::pick`).
//! - **Taking the pick over** (2026-11-12): an operator who does not
//!   choose has Pick songs (`RoomClient::may_take_pick`): a button at the
//!   status line's end (`room::TAKE_PICK`), the room's More, and T with
//!   its hint in the footer.
//! - **The players, always** (feedback round B; round SLB): a room's wide
//!   layout is in three parts, the song's side where solo has it, the
//!   list, and the players panel at the right (`select_players.rs`),
//!   foldable to a rail. A phone has no room beside the song and the list:
//!   the players are one line over the list (`room::players_strip`),
//!   which opens the full list.
//! - **The room around it**: the header's Rules and Players (in More on a
//!   phone), the room's More (invite, stop, leave), and the status line at
//!   the top of the list's column, under the header (and the tabs). A
//!   room opens straight on this screen with the default rules (the Rules
//!   pop-up opens by itself on entering only under the hidden option
//!   `Settings::multiplayer_rules_first`, off by default); a stage's
//!   standings pop up once back from its results where they were not seen
//!   there; the stage's start prompt opens when a pick gathers readiness.
//!
//! - **One view, coming in once** (2026-11-12): what the screen is built
//!   as is its [`Side`]. Before the room says who this player is (an
//!   online room's first answer comes after the screen is built) nothing
//!   role-specific shows: the bars and, if it takes a moment, one quiet
//!   line (`RoomJoining`). Once known, the view comes in with the screen's
//!   own entrance, once, the bars kept as they are.
//!
//! A child module of `select` (it reads the screen's own fields).

use super::*;
use crate::screens::room::{self, Pump, RoomShell};
use crate::screens::room_dialogs::{NoticeDialog, PlayersDialog, RulesDialog, StandingsDialog};
use crate::state::room::{EntryState, RoomView, SharedRoom};
use ddi_room::RoomSongId;

/// The status line's height above the list.
const STATUS_H: f32 = 40.0;
/// The tabs' row over it (an online room's).
const TABS_H: f32 = 34.0;
/// The tabs' buttons.
pub(super) const TAB_ROOM: &str = "Room.Tab.Room";
pub(super) const TAB_MINE: &str = "Room.Tab.Mine";
/// More's items for the list (revision 17).
const MORE_TAB: &str = "room-tab";
const MORE_REMOVE: &str = "room-remove-song";
const MORE_RETRY_UPLOAD: &str = "room-retry-upload";
const MORE_ADD_PACK: &str = "room-add-pack";
const MORE_FOLD: &str = "room-fold-players";
/// A listed song this device does not have, as the Room songs tab names
/// it (its entry's id after it).
use crate::screens::data::ROOM_SONG_ID;
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
    /// CPU practice: this player's own list to pick from.
    Picker,
    /// An online room (revision 17): the room's song list and the
    /// player's own, as two tabs ([`Tab`]).
    Listed,
}

impl Side {
    /// The side the room says this player is on now.
    fn of(r: &crate::state::room::RoomClient) -> Side {
        if r.state().member(r.me()).is_none() {
            Side::Joining
        } else if r.has_song_list() {
            Side::Listed
        } else {
            Side::Picker
        }
    }
}

/// An online room's song select's tabs (revision 17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    /// The room's song list: the picker picks here.
    Room,
    /// The player's own library: added to the room from here.
    Mine,
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
const JOINING_AFTER: f32 = 1.0;

/// Song select's room: the client, the shell's calls and what was drawn.
pub(super) struct RoomMode {
    pub(super) room: SharedRoom,
    pub(super) shell: RoomShell,
    /// What the screen is built as.
    side: Side,
    /// An online room's tab shown (its data with it: the room's list, or
    /// the player's own).
    tab: Tab,
    pump: Pump,
    /// The room's version and the seconds left the parts were built for.
    built: Option<(u64, Option<u64>, bool)>,
    /// The group's last draw revealed here (revision 22): by stage and
    /// roll.
    revealed: Option<(ddi_room::StageId, u64)>,
    /// The room's version the Room songs tab's list was made for.
    listed_for: Option<u64>,
    /// The Room songs tab's songs' ids and the entries they are.
    entries: HashMap<String, RoomSongId>,
    /// The player's own list, kept while the Room songs tab shows: put
    /// back on My songs.
    pub(super) library: Option<SelectData>,
    /// Where they were in it when they left it.
    place: Option<Place>,
    /// Where they were on Room songs (the song and its chart).
    room_place: Option<(String, Option<usize>)>,
    /// They chose the room's songs at the last update: the song shown is
    /// told the room again when the pick comes (back) to them.
    choosing: bool,
    /// What was told the room the song select rests on.
    hovered: Option<(String, usize)>,
    /// The results whose standings were opened (a stage's number, or the
    /// match's), until shown.
    standings: Option<Option<u16>>,
    /// The UI's clock at the last update ([`SelectScreen::room_wake_at`]).
    time: f64,
    /// The game is quitting once the room has answered the player's
    /// leaving, or at this time on the UI's clock.
    quit_at: Option<f64>,
    /// The player left the room from this screen (asked or not): nothing
    /// guards it any more.
    left: bool,
    /// The room's name the header was built with.
    heading: Option<String>,
    /// What the debug view was last published for: published again only
    /// when it changes ([`SelectScreen::room_publish`]).
    published: std::cell::RefCell<Option<Published>>,
    /// The wide layout's players panel folded to its rail (this device's
    /// `Settings::players_folded`).
    pub(super) folded: bool,
    /// The panel's row whose words show in a pop-over (hovered, or tapped
    /// on a touch screen).
    pub(super) popover: Option<usize>,
}

/// What the room's debug view shows depends on: the room's version, the
/// seconds left, the last refusal, the view and the screen's controls.
type Published = (
    u64,
    Option<u64>,
    Option<crate::state::room::Refusal>,
    &'static str,
    Vec<String>,
    Option<&'static str>,
    (Option<&'static str>, Option<usize>),
);

impl SelectScreen {
    /// Song select as room `room`'s main view (M7b), its shell's calls
    /// through `shell`: the host's list to pick from, or the others'
    /// following view. The data's list is the player's library (the
    /// others' is replaced by the host's resting song).
    pub fn with_room(mut self, room: SharedRoom, shell: RoomShell) -> SelectScreen {
        // The player's library as the room's client knows it: what a
        // listed song this device has is uploaded and loaded from.
        if room.borrow().has_song_list() {
            room.borrow_mut().set_library(&self.data.songs);
        }
        // Built as joining: an online room's view comes in at the first
        // update that knows the player (its data then).
        let side = match Side::of(&room.borrow()) {
            Side::Listed => Side::Joining,
            side => side,
        };
        self.room = Some(RoomMode {
            room,
            shell,
            side,
            tab: Tab::Room,
            pump: Pump::default(),
            built: None,
            revealed: None,
            listed_for: None,
            entries: HashMap::new(),
            heading: None,
            published: std::cell::RefCell::new(None),
            library: None,
            place: None,
            room_place: None,
            choosing: false,
            hovered: None,
            standings: None,
            time: 0.0,
            quit_at: None,
            left: false,
            folded: crate::state::settings::Settings::load().players_folded,
            popover: None,
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

    /// The screen shows the local player's own list (practice's, or an
    /// online room's My songs); `None` outside a room.
    pub(super) fn room_host(&self) -> Option<bool> {
        self.room
            .as_ref()
            .map(|rm| rm.side == Side::Picker || (rm.side == Side::Listed && rm.tab == Tab::Mine))
    }

    /// The screen shows an online room's Room songs tab.
    pub(super) fn room_listing(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| rm.side == Side::Listed && rm.tab == Tab::Room)
    }

    /// The listed song shown on Room songs, if one.
    fn shown_entry(&self) -> Option<RoomSongId> {
        let rm = self.room.as_ref()?;
        if !self.room_listing() {
            return None;
        }
        rm.entries.get(&self.song()?.id).copied()
    }

    /// The room has not said who this player is yet: the screen shows
    /// nothing of a view.
    /// Whether the room's status line has Retry beside it (a failed
    /// download, round P1b).
    pub(super) fn room_retry_shown(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| rm.side != Side::Joining && room::retry_shown(&rm.room.borrow()))
    }

    /// Round FX: the status line's button for chat's permission shows.
    pub(super) fn room_allow_chat_shown(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| rm.side != Side::Joining && room::allow_chat_shown(&rm.room.borrow()))
    }

    pub(super) fn room_joining(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| rm.side == Side::Joining)
    }

    /// Whether this player may take the pick over now (an operator who
    /// does not choose): Pick songs is offered.
    pub(super) fn room_may_take(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| rm.side == Side::Listed && rm.room.borrow().may_take_pick())
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
            NoticeDialog::show_refusal(self.env.clone(), &rm.room, &why, cx);
        }
    }

    /// Song select without a list of its own to show, the room's panel in
    /// its place: never since revision 17 (a member who does not choose
    /// browses the room's songs and their own).
    pub(super) fn following(&self) -> bool {
        false
    }

    /// The layout with the list's column under the status line (and an
    /// online room's tabs) and, on a phone, the players strip.
    pub(super) fn room_layout(&self, mut l: Layout) -> Layout {
        if self.room.is_some() {
            let top = STATUS_H + self.tabs_height() + self.strip_height(&l);
            l.wheel[1] += top;
            l.wheel[3] -= top;
        }
        l
    }

    /// The tabs' row's height: an online room's, once the room is known.
    fn tabs_height(&self) -> f32 {
        match &self.room {
            Some(rm) if rm.side == Side::Listed => TABS_H,
            _ => 0.0,
        }
    }

    /// The room a phone's players strip takes over the list.
    fn strip_height(&self, l: &Layout) -> f32 {
        if l.compact && !self.room_joining() && self.room.is_some() {
            STRIP_H + STRIP_GAP
        } else {
            0.0
        }
    }

    /// The layout for a viewport: a room's has the song's side short and
    /// the players under it (`Layout::for_room`).
    pub(super) fn fit_layout(&self, vp: [f32; 2], compact: bool) -> Layout {
        let mut l = Layout::fit(&self.env, vp, compact);
        if self.room.is_some() {
            return l.for_room();
        }
        // The skill radar in the ladder's place (round Z1): the wide
        // layout's alone (a phone's and a room's short side keep theirs).
        l.radar = !l.compact && crate::state::ratings::radar_shown();
        l
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
    /// layout's three parts, at the right; empty elsewhere: a phone has
    /// its strip).
    pub(super) fn room_players(&self) -> Actor {
        match self.players_column() {
            Some(c) if self.room.as_ref().is_some_and(|rm| rm.side != Side::Joining) => self.players_panel(c),
            _ => frame("RoomPlayers", Vec::new()),
        }
    }

    /// The room's own parts (`RoomParts`): an online room's tabs, the
    /// status line (with Retry or Pick songs at its end), a phone's
    /// players strip, and an empty Room songs tab's words in the list's
    /// place.
    pub(super) fn room_parts(&mut self, cx: &mut Cx) -> Actor {
        let Some(rm) = &self.room else {
            return frame("RoomParts", Vec::new());
        };
        // Nothing of a view before the room is known.
        if rm.side == Side::Joining {
            return frame("RoomParts", Vec::new());
        }
        let env = self.env.clone();
        let l = self.room_layout(self.layout);
        let r = rm.room.borrow();
        let mut children = Vec::new();
        let [wx, wy, ww, wh] = l.wheel;
        let inset = env.metric("Select.Wheel.Inset", 0.0);
        let (sx, sw) = (wx + inset + 8.0, (ww - inset - 24.0).max(80.0));
        let strip = self.strip_height(&l);
        let status_y = wy - strip - STATUS_H / 2.0;
        // An online room's tabs (revision 17): Room songs, My songs.
        if rm.side == Side::Listed {
            let ty = wy - strip - STATUS_H - TABS_H / 2.0;
            let mut x = sx;
            for (name, label, on) in [
                (TAB_ROOM, ddi_l10n::msg::room::tab_room_songs(), rm.tab == Tab::Room),
                (TAB_MINE, ddi_l10n::msg::room::tab_my_songs(), rm.tab == Tab::Mine),
            ] {
                let w = (look::mono_width(&label, 13.0) + 28.0).max(96.0);
                let mut tab = env.component(
                    "Tab",
                    "Button",
                    &Spec::new([w, TABS_H - 4.0], label.as_str()),
                    name,
                    [x + w / 2.0, ty],
                );
                crate::widgets::play(&mut tab, if on { "Selected" } else { "Available" });
                // The whole tab is the target, its plate around the words
                // too (a look may leave it unpainted, round SF).
                if let Kind::Frame(parts) = &mut tab.kind {
                    parts.insert(
                        0,
                        look::quad_at("", [w, TABS_H - 4.0], [0.0, 0.0, 0.0, 0.004], [0.0, 0.0]),
                    );
                }
                children.push(tab);
                x += w + 4.0;
            }
        }
        // The status line over the list's column (on My songs, what
        // Enter does there where it would say to pick).
        let text: String = match rm.tab {
            Tab::Mine if rm.side == Side::Listed => {
                room::status_on_my_songs(&r).unwrap_or_else(|| room::status(&r))
            }
            _ => room::status(&r),
        }
        .into();
        let mut status = env.text("Room.Status", &text, 14.0, 600, "Secondary");
        // At its right: Retry for a download that failed after its own
        // retries (round P1b), else Pick songs for an operator who does
        // not choose.
        let button = if room::retry_shown(&r) {
            Some((ddi_l10n::msg::room::retry_load(), room::RETRY_LOAD))
        } else if room::allow_chat_shown(&r) {
            // Round FX: the line says the Twitch access lacks chat's
            // permission: the step itself beside it.
            Some((ddi_l10n::msg::room::twitch_chat_allow(), room::ALLOW_CHAT))
        } else if rm.side == Side::Listed && r.may_take_pick() {
            Some((ddi_l10n::msg::room::take_pick(), room::TAKE_PICK))
        } else {
            None
        };
        let sw_text = match button {
            Some((label, name)) => {
                let w = (look::mono_width(&label, 14.0) + 24.0).max(64.0);
                children.push(env.component(
                    "Button",
                    "Button",
                    &Spec::new([w, 28.0], label.as_str()),
                    name,
                    [sx + sw - w / 2.0, status_y],
                ));
                (sw - w - 8.0).max(40.0)
            }
            None => sw,
        };
        look::ellipsized(&mut status, sw_text);
        children.push(left(status, sx, status_y));
        if strip > 0.0 {
            let rows = room::player_rows(&r);
            children.push(room::players_strip(&env, &r, rows, [sx, wy - strip, sw, STRIP_H]));
        }
        // An empty Room songs tab says what to do (revision 17).
        if rm.side == Side::Listed && rm.tab == Tab::Room && r.room_songs().is_empty() {
            let words = if r.may_add() {
                ddi_l10n::msg::room::songs_empty_adder()
            } else {
                ddi_l10n::msg::room::songs_empty_member()
            };
            let lines = crate::screens::dialog::wrap_lines(&words, (sw - 32.0).max(80.0), 15.0);
            for (i, text) in lines.iter().enumerate() {
                let mut line = env.text("Room.SongsEmpty", text, 15.0, 600, "Secondary");
                call(&mut line, "xy", &[sx + sw / 2.0, wy + wh / 3.0 + i as f32 * 22.0]);
                children.push(line);
            }
        }
        drop(r);
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
            Side::Picker => {}
            Side::Listed => self.show_room_songs(root, cx),
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
        // The joining line goes as the view comes, for good: nothing
        // plays its entrance again (a screen over the room and back plays
        // `On` over the whole tree, round SF), and it leaves the tree once
        // faded ([`SelectScreen::drop_joining_line`]).
        if from == Side::Joining
            && let Some(at) = old.iter().position(|c| c.name == JOINING)
        {
            let mut line = old.remove(at);
            line.play("Off", &mut problems);
            line.set_command("On", Vec::new());
            now.push(line);
        }
        if enter {
            let backdrop = (from == Side::Joining).then_some("Backdrop");
            play_parts(now, SIDE_PARTS.into_iter().chain(backdrop), "On");
        }
    }

    /// The joining line faded out after the view came in
    /// ([`SelectScreen::swap_side`]) leaves the tree: a part kept unseen
    /// would come in again with its last words wherever `On` is played
    /// over the screen (uncovered on desktop, round SF).
    fn drop_joining_line(&self, root: &mut Actor) {
        if self.room.as_ref().is_none_or(|rm| rm.side == Side::Joining) {
            return;
        }
        if let Kind::Frame(children) = &mut root.kind {
            children.retain(|c| c.name != JOINING || c.tweening());
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
        if self.layout.compact {
            self.room_play_button(root);
        }
        self.room_song_side(root);
        self.publish_dirty = true;
    }

    /// The room's per-frame part of `update` (before anything else: the
    /// songs to prepare and upload go to the shell whatever covers the
    /// screen). Where it is the current screen: the Rules pop-up once,
    /// results not seen yet, the stage's prompt; the others' song kept on
    /// the host's; the host's resting song told the room.
    pub(super) fn room_update(&mut self, root: &mut Actor, cx: &mut Cx) {
        self.drop_joining_line(root);
        let Some(rm) = &mut self.room else {
            return;
        };
        let (room, shell) = (rm.room.clone(), rm.shell.clone());
        rm.time = cx.time();
        let changed = rm.pump.run(&room, &shell, cx);
        // Quitting (the window closed, the leaving confirmed): once the
        // room has answered, or after a moment.
        if room::quit_due(&room, rm.quit_at, cx.time()) {
            rm.quit_at = None;
            cx.request(crate::UiRequest::Quit);
            return;
        }
        if !cx.is_current_screen() {
            // Under a screen opened over the room (Options, Calibrate):
            // a stage due is told there, with a way to its Ready screen
            // ([`room::DueDialog`]); never over a play (a stage's results
            // tell it themselves, `results_stage.rs`), and not while a
            // control is being bound there: it is told once that step
            // is saved or canceled.
            if cx.covered_by_yielding() && !cx.notices_wait() {
                let go: room::DueGo =
                    Rc::new(|cx| cx.broadcast(Message::new(room::DUE_GO)));
                room::DueDialog::tell(&self.env, &room, go, cx);
            }
            return;
        }
        // This device stopped for another of the person's (round P1b):
        // said once, with Use this device.
        if room.borrow().take_opened_elsewhere() {
            self.cancel_navigation();
            room::tell_opened_elsewhere(&self.env, &room, cx);
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
        // The group's draw (revision 22): its reveal first, the stage's
        // prompt once it is over.
        if let Some(rm) = &mut self.room
            && crate::screens::room_reveal::open_reveal(&self.env, &room, &mut rm.revealed, cx)
        {
            self.cancel_navigation();
            return;
        }
        if Pump::open_prompt(&room, &shell, cx) {
            return;
        }
        // The header names the room: its name comes after the room, and
        // changes in place (the bars stay put).
        let heading = String::from(room::heading(&room.borrow()));
        let built = self.room.as_ref().and_then(|rm| rm.heading.as_deref());
        if built != Some(heading.as_str()) {
            if built.is_some()
                && !self.layout.compact
                && let Some(old) = hit::find_mut(root, "Header")
            {
                let fresh = header(&self.env, cx, self.layout.screen[0], &heading);
                crate::screens::refill(old, fresh);
            }
            if let Some(rm) = &mut self.room {
                rm.heading = Some(heading);
            }
            self.left_dirty = true;
        }
        // The side the room says this player is on, against the one
        // shown: known at last (an online room's first answer).
        let now = Side::of(&room.borrow());
        let shown = match &self.room {
            Some(rm) => rm.side,
            None => return,
        };
        if now != shown && now != Side::Joining {
            if let Some(rm) = &mut self.room {
                rm.built = None;
            }
            let enter = shown == Side::Joining || !self.env.reduced();
            self.swap_side(now, enter, root, cx);
        }
        let side = self.room.as_ref().map_or(Side::Joining, |rm| rm.side);
        // The difficulty this player usually plays (round OC): their start
        // where they chose nothing for a song.
        if let Some(rm) = &self.room
            && let Ok(mut r) = rm.room.try_borrow_mut()
        {
            r.set_preferred(self.preferred);
        }
        match side {
            Side::Picker => self.tell_hover(),
            Side::Listed => {
                // The room's list as it is now, on its tab; the picker's
                // resting song told the room.
                self.sync_room_songs(root, cx);
                self.tell_hover_listed();
            }
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
            // The footer's Enter follows the song's place in the room.
            self.hints = None;
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

    /// Under Charts: Adaptive (round OC) the chart remembered for `song`
    /// this session (chosen on its side or on a stage's Ready card): what
    /// its side shows where nothing was picked on it since it was shown.
    pub(super) fn room_remembered_pick(&self, song: &SongEntry, charts: &[&ChartEntry]) -> Option<usize> {
        remembered_pick(&self.room.as_ref()?.room, song, charts)
    }

    /// The chart picked on the song's side under Charts: Adaptive (round
    /// OC): this player's own for that song, remembered for the room's
    /// session and their start when it is played.
    pub(super) fn room_remember_chart(&self) {
        let Some(rm) = &self.room else {
            return;
        };
        let (charts, pick) = self.charts();
        let (Some(song), Some(chart)) = (self.song(), pick.and_then(|p| charts.get(p).copied())) else {
            return;
        };
        if let Ok(mut r) = rm.room.try_borrow_mut()
            && r.songs_without_charts()
        {
            r.remember_start(song, chart);
        }
    }

    /// The picker's song and chart shown on Room songs, told the room when
    /// they change (its entry marked for the others once it rested).
    fn tell_hover_listed(&mut self) {
        let Some(rm) = &mut self.room else {
            return;
        };
        let chooses = rm.room.borrow().chooses();
        if chooses != rm.choosing {
            // The spotlight went with the last picker: told again.
            rm.choosing = chooses;
            rm.hovered = None;
        }
        if rm.tab != Tab::Room || !chooses {
            return;
        }
        let (charts, pick) = self.charts();
        let chart = pick.and_then(|p| charts.get(p).copied()).cloned();
        let entry = self.shown_entry();
        let key = entry
            .zip(chart.as_ref())
            .map(|(e, c)| (format!("{ROOM_SONG_ID}{}", e.0), c.index));
        let digest = chart.as_ref().and_then(|c| c.digest).map(|d| ddi_room::ChartKey::new(d.to_string()));
        let Some(rm) = &mut self.room else {
            return;
        };
        if rm.hovered == key {
            return;
        }
        rm.hovered = key;
        rm.room
            .borrow_mut()
            .hover_listed(entry.zip(digest.as_ref()));
    }

    /// The Room songs tab's list: a row an entry, oldest first, each the
    /// player's own copy of the song where they have it (its banner, its
    /// preview, their bests and NEW, as their library's row: round DB)
    /// with only the listed charts, else the copy downloaded ahead (no
    /// best, never NEW: `SongEntry::room_only`), else made from the
    /// entry; its line the artist, the entry's state for this player and
    /// who added it.
    pub(crate) fn room_songs_data(&self) -> (SelectData, HashMap<String, RoomSongId>) {
        let mut data = SelectData {
            mods: self.data.mods.clone(),
            preferred: self.data.preferred,
            autoplay: self.data.autoplay,
            ..SelectData::default()
        };
        let mut ids = HashMap::new();
        let Some(rm) = &self.room else {
            return (data, ids);
        };
        let r = rm.room.borrow();
        let spot = r.spotlight_entry();
        let picker = r
            .state()
            .spotlight
            .as_ref()
            .and_then(|s| r.member_of(s.by))
            .map(room::person_name);
        for e in r.room_songs() {
            // The player's own copy, else the one downloaded ahead.
            let mine = r.library_song(e).cloned().or_else(|| {
                r.ahead_song(e).map(|mut s| {
                    s.room_only = true;
                    s
                })
            });
            let mut song = match &mine {
                Some(s) => {
                    let mut s = s.clone();
                    s.charts.retain(|c| {
                        c.version.is_none()
                            && c.digest
                                .is_some_and(|d| e.chart(&ddi_room::ChartKey::new(d.to_string())).is_some())
                    });
                    s
                }
                None => SongEntry {
                    id: format!("{ROOM_SONG_ID}{}", e.id.0),
                    title: e.card.title.clone(),
                    charts: e
                        .charts
                        .iter()
                        .enumerate()
                        .map(|(i, c)| ChartEntry {
                            style: c.style.clone(),
                            difficulty: ddi_chart::Difficulty::from_name(&c.difficulty)
                                .unwrap_or(ddi_chart::Difficulty::Edit),
                            meter: c.meter,
                            index: i,
                            digest: c.chart.as_str().parse().ok(),
                            ..ChartEntry::default()
                        })
                        .collect(),
                    // Its banner as soon as it is here, before the song.
                    banner: r.ahead_banner(e),
                    ..SongEntry::default()
                },
            };
            song.pack = (!e.card.pack.is_empty()).then(|| e.card.pack.clone());
            let state = match r.entry_state(e) {
                EntryState::Uploading { done, total } if total > 0.0 => {
                    ddi_l10n::msg::room::song_uploading_percent((done / total * 100.0).floor())
                }
                EntryState::Uploading { .. } => ddi_l10n::msg::room::song_uploading(),
                EntryState::Failed => ddi_l10n::msg::room::song_failed(),
                EntryState::Ready => ddi_l10n::msg::room::song_ready(),
                EntryState::Loading => ddi_l10n::msg::room::song_loading(),
                EntryState::Downloading { done, total } if total > 0.0 => {
                    ddi_l10n::msg::room::song_downloading_percent((done / total * 100.0).floor())
                }
                EntryState::Downloading { .. } => ddi_l10n::msg::room::song_downloading(),
                EntryState::NotLoaded => ddi_l10n::msg::room::song_not_loaded(),
            };
            // Who has it, while not everyone does (stage 2).
            let state = match r.have_count(e) {
                Some((have, of)) if have < of => {
                    ddi_l10n::msg::room::song_state_have(state, have, of)
                }
                _ => state,
            };
            let by = if e.by == r.me() {
                ddi_l10n::msg::room::song_added_by_you()
            } else {
                let name = r.member_of(e.by).map(room::person_name).unwrap_or_default();
                ddi_l10n::msg::room::song_added_by(name)
            };
            // The picker's resting song is no news under Song choice:
            // Group (revision 22): the players choose.
            let spot = spot.filter(|_| !r.group_choice());
            let line = match (&picker, spot == Some(e.id)) {
                (Some(who), true) => ddi_l10n::msg::room::song_line_spotlight(
                    e.card.artist.clone(),
                    state,
                    by,
                    who.clone(),
                ),
                _ if e.card.artist.is_empty() => ddi_l10n::msg::room::song_line_no_artist(state, by),
                _ => ddi_l10n::msg::room::song_line(e.card.artist.clone(), state, by),
            };
            // Song choice: Group (revision 22): who chose it, first; a song
            // the Repeats rule excludes said so.
            let line = if r.group_choice() {
                let line = match r.repeat_refused(e.id) {
                    Some(rule) => {
                        ddi_l10n::msg::room::song_line_repeat(room::repeats_id(rule), line)
                    }
                    None => line,
                };
                match chosen_words(&r, e.id) {
                    Some(who) => ddi_l10n::msg::room::song_line_chosen(who, line),
                    None => line,
                }
            } else {
                line
            };
            song.artist = line.into();
            ids.insert(song.id.clone(), e.id);
            data.songs.push(song);
        }
        // The styles the list's songs have, in the player's library's order
        // (Single first, as song select has them), the others after.
        let order: Vec<String> = rm
            .library
            .as_ref()
            .map_or(&self.data.styles, |l| &l.styles)
            .iter()
            .map(|st| st.id.clone())
            .collect();
        drop(r);
        data.styles = data
            .songs
            .iter()
            .flat_map(|s| song_styles(s).into_iter())
            .fold(Vec::new(), |mut all, st| {
                if !all.iter().any(|a: &crate::screens::data::StyleDef| a.id == st.id) {
                    all.push(st);
                }
                all
            });
        data.styles
            .sort_by_key(|st| order.iter().position(|o| *o == st.id).unwrap_or(usize::MAX));
        data.style = Some(self.style_id().to_string());
        (data, ids)
    }

    /// The shell's library sent again (the history read after the list
    /// showed, a play kept, an import) in a room with a song list: the
    /// room's client told (a listed song's row takes its best and NEW
    /// from it, round DB); on Room songs it is kept for My songs and the
    /// room's list made again from it, never shown in its place. The data
    /// back where the screen shows it as its own list.
    pub(super) fn room_library_feed(&mut self, d: SelectData) -> Option<SelectData> {
        let Some(rm) = &mut self.room else {
            return Some(d);
        };
        if !rm.room.borrow().has_song_list() {
            return Some(d);
        }
        rm.room.borrow_mut().set_library(&d.songs);
        if rm.tab == Tab::Room && rm.library.is_some() {
            rm.library = Some(d);
            rm.listed_for = None;
            None
        } else {
            Some(d)
        }
    }

    /// The Room songs tab's list made again where the room's version moved
    /// (an entry added, removed or uploaded, a load), the song shown kept.
    fn sync_room_songs(&mut self, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        let version = rm.room.borrow().version();
        if rm.tab != Tab::Room || rm.listed_for == Some(version) {
            return;
        }
        let (charts, pick) = self.charts();
        let chart = pick.and_then(|p| charts.get(p)).map(|c| c.index);
        let shown = self.song().map(|s| (s.id.clone(), chart));
        let (data, ids) = self.room_songs_data();
        if let Some(rm) = &mut self.room {
            rm.listed_for = Some(version);
            rm.entries = ids;
        }
        if self.data.songs == data.songs {
            return;
        }
        self.resume = shown;
        self.cancel_navigation();
        let _ = cx;
        self.set_data(data, false);
        self.fill_tabs(root);
        self.fill_charts(root);
        self.left_dirty = true;
        self.room_song_side(root);
    }

    /// Room songs shown (an online room's first view, or its tab chosen):
    /// the player's own list kept aside with where they were in it, the
    /// room's list in its place where they left it.
    fn show_room_songs(&mut self, root: &mut Actor, cx: &mut Cx) {
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
        let (data, ids) = self.room_songs_data();
        let Some(rm) = &mut self.room else {
            return;
        };
        rm.tab = Tab::Room;
        rm.entries = ids;
        rm.listed_for = Some(rm.room.borrow().version());
        rm.hovered = None;
        self.resume = rm.room_place.take();
        self.cancel_navigation();
        self.set_data(data, false);
        self.fill_tabs(root);
        self.fill_charts(root);
        self.left_dirty = true;
        self.hints = None;
    }

    /// My songs shown: the player's own list back where they left it (the
    /// song and chart shown, the open pack, the style, the search).
    fn show_my_songs(&mut self, root: &mut Actor) {
        let (charts, pick) = self.charts();
        let chart = pick.and_then(|p| charts.get(p)).map(|c| c.index);
        let shown = self.song().map(|s| (s.id.clone(), chart));
        let Some(rm) = &mut self.room else {
            return;
        };
        rm.tab = Tab::Mine;
        rm.room_place = shown;
        let place = rm.place.take();
        let Some(mut library) = rm.library.take() else {
            return;
        };
        if let Some(p) = &place {
            // The favorites' pack open again where it was.
            if p.open.as_ref().is_some_and(|n| is_favorites(n.as_deref())) {
                self.resume_pack = Some(FAVORITES_KEY.to_string());
            }
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
        if let Some(p) = place {
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
        self.fill_tabs(root);
        self.fill_charts(root);
        self.left_dirty = true;
        self.hints = None;
    }

    /// The other tab shown, at once (M, the tabs' buttons, More).
    fn switch_tab(&mut self, to: Tab, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        if rm.side != Side::Listed || rm.tab == to {
            return;
        }
        match to {
            Tab::Room => self.show_room_songs(root, cx),
            Tab::Mine => self.show_my_songs(root),
        }
        self.refill_room(root, cx);
        cx.focus("Wheel");
    }

    /// Start on a song in the room: practice's pick, or an online room's
    /// on Room songs (the shell's play function then does what the press
    /// must, as keeping the audio context for the prompt); on My songs, the
    /// song added to the room's list. Anyone else's does nothing.
    pub(super) fn room_start(&mut self, song: &SongEntry, chart: &ChartEntry, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        let room = rm.room.clone();
        match (rm.side, rm.tab) {
            (Side::Picker, _) => {
                if !room.borrow().chooses() {
                    return;
                }
                let picked = room.borrow_mut().pick(song, chart);
                match picked {
                    Ok(()) => {
                        let env = self.env.clone();
                        (self.play)(song, chart, &env, cx);
                    }
                    Err(why) => NoticeDialog::show_refusal(self.env.clone(), &room, &why, cx),
                }
            }
            (Side::Listed, Tab::Room) => {
                let Some(entry) = rm.entries.get(&song.id).copied() else {
                    return;
                };
                let Some(key) = chart.digest.map(|d| ddi_room::ChartKey::new(d.to_string())) else {
                    return;
                };
                // Song choice: Group (revision 22): Enter is this player's
                // choice (again: taken back), a host's starting the draw;
                // one who may not choose is told why.
                if room.borrow().group_choice() {
                    let chosen = room.borrow_mut().choose(entry, &key);
                    if let Err(why) = chosen {
                        room.borrow_mut().clear_refusal();
                        NoticeDialog::show_refusal(self.env.clone(), &room, &why, cx);
                    }
                    self.hints = None;
                    return;
                }
                if !room.borrow().chooses() {
                    return;
                }
                let picked = room.borrow_mut().pick_listed(entry, &key);
                match picked {
                    Ok(()) => {
                        let env = self.env.clone();
                        (self.play)(song, chart, &env, cx);
                    }
                    Err(why) => NoticeDialog::show_refusal(self.env.clone(), &room, &why, cx),
                }
            }
            (Side::Listed, Tab::Mine) => {
                let (may, listed) = {
                    let r = room.borrow();
                    (r.may_add(), r.listed(song).is_some())
                };
                if !may || listed {
                    return;
                }
                let added = room.borrow_mut().add_song(song);
                if let Err(why) = added {
                    NoticeDialog::show_refusal(self.env.clone(), &room, &why, cx);
                }
                self.hints = None;
            }
            (Side::Joining, _) => {}
        }
    }

    /// Back from the room's song select leaves the room: asked first
    /// where leaving costs something (`room::ask_leave`), else at once.
    /// An online room is told and forgotten; practice between matches is
    /// kept for the session, as it was.
    pub(super) fn room_back(&mut self, cx: &mut Cx) -> bool {
        self.room_leave(room::LeaveBy::Back, cx)
    }

    /// The player sets about leaving the room.
    fn room_leave(&mut self, by: room::LeaveBy, cx: &mut Cx) -> bool {
        let Some(rm) = &self.room else {
            return false;
        };
        if !room::ask_leave(&self.env, &rm.room, by, cx) {
            self.room_left(by, cx);
        }
        true
    }

    /// The room is left, asked or at no cost: told, then the home; a
    /// quitting game waits a moment for the room's answer first.
    fn room_left(&mut self, by: room::LeaveBy, cx: &mut Cx) {
        if self.room.is_none() {
            return;
        }
        // The room's preview stops as song select's does.
        self.told_shown = false;
        self.call(SelectCall::Shown(false), cx);
        let Some(rm) = &mut self.room else {
            return;
        };
        rm.left = true;
        if by == room::LeaveBy::Quit {
            // An online room is told first; nothing else to wait for.
            rm.quit_at = Some(room::quit_leaving(&rm.room, cx.time()));
        } else {
            room::leave_now(&rm.room, &rm.shell, by, cx);
        }
    }

    /// Whether leaving the room's song select asks first
    /// ([`crate::Screen::guards_leaving`]).
    pub(super) fn room_guards(&self) -> bool {
        self.room
            .as_ref()
            .is_some_and(|rm| !rm.left && rm.room.borrow().leave_cost().is_some())
    }

    /// The room's More menu in song select's place.
    pub(super) fn room_more(&self, cx: &mut Cx) -> bool {
        let Some(rm) = &self.room else {
            return false;
        };
        // The picker's song's heart and a phone's volume first, the room's
        // own, then the other tab and what the list offers of the song
        // shown (revision 17).
        let first: Vec<(String, String)> =
            self.favorite_item().into_iter().chain(self.volume_item()).collect();
        let mut last: Vec<(String, String)> = Vec::new();
        // The wide layout's panel folded or opened (a controller's way to
        // it: More is West).
        if self.panel_shown() {
            let label = if rm.folded {
                ddi_l10n::msg::room::panel_unfold()
            } else {
                ddi_l10n::msg::room::panel_fold()
            };
            last.push((MORE_FOLD.into(), label.into()));
        }
        if rm.side == Side::Listed {
            let label = match rm.tab {
                Tab::Room => ddi_l10n::msg::room::tab_my_songs(),
                Tab::Mine => ddi_l10n::msg::room::tab_room_songs(),
            };
            last.push((MORE_TAB.into(), label.into()));
            // My songs: the shown song's whole pack added at once.
            if rm.tab == Tab::Mine
                && let Some(pack) = self.song().and_then(|s| s.pack.clone())
            {
                let songs: Vec<SongEntry> = self
                    .data
                    .songs
                    .iter()
                    .filter(|s| s.pack.as_deref() == Some(pack.as_str()))
                    .cloned()
                    .collect();
                let n = rm.room.borrow().pack_addable(&songs).len();
                if n > 0 {
                    last.push((
                        MORE_ADD_PACK.into(),
                        ddi_l10n::msg::room::add_pack(n).into(),
                    ));
                }
            }
            if let Some(entry) = self.shown_entry() {
                let r = rm.room.borrow();
                if r.may_retry_song(entry) {
                    last.push((MORE_RETRY_UPLOAD.into(), ddi_l10n::msg::room::retry_upload().into()));
                }
                if r.may_remove(entry) {
                    last.push((MORE_REMOVE.into(), ddi_l10n::msg::room::remove_song().into()));
                }
            }
        }
        room::open_more_around(&self.env, &rm.room, self.layout.compact, first, last, cx);
        true
    }

    /// Whether the wide layout's players panel is shown (the room's three
    /// parts, its view known).
    pub(super) fn panel_shown(&self) -> bool {
        self.players_column().is_some()
            && self.room.as_ref().is_some_and(|rm| rm.side != Side::Joining)
    }

    /// The panel folded or unfolded: kept in this device's settings, the
    /// screen laid out again, the panel sliding from where it was (at
    /// once under reduced motion).
    pub(super) fn toggle_panel(&mut self, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &mut self.room else {
            return;
        };
        rm.folded = !rm.folded;
        rm.popover = None;
        let folded = rm.folded;
        let mut s = crate::state::settings::Settings::load();
        if s.players_folded != folded {
            s.players_folded = folded;
            s.save();
        }
        let [w, _] = self.layout.screen;
        let from = super::players::line_x(w, self.layout.margin, !folded);
        let to = super::players::line_x(w, self.layout.margin, folded);
        self.relayout(root, cx);
        if !self.reduced
            && let Some(panel) = hit::find_mut(root, "RoomPlayers")
        {
            look::command_of(
                panel,
                "Slide",
                &format!("stoptweening;x,{};decelerate,{};x,0", from - to, super::players::SLIDE),
            );
            crate::widgets::play(panel, "Slide");
        }
        self.status(if folded { "players folded".into() } else { "players open".into() });
    }

    /// The pop-over shows row `k`'s words (none: no pop-over).
    fn set_popover(&mut self, k: Option<usize>, root: &mut Actor) {
        let Some(rm) = &mut self.room else {
            return;
        };
        if rm.popover == k {
            return;
        }
        rm.popover = k;
        self.publish_dirty = true;
        let players = self.room_players();
        if let Some(old) = hit::find_mut(root, "RoomPlayers") {
            crate::screens::refill(old, players);
        }
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

    /// The hand over an online room's tabs, as over a button (they are
    /// pressed, never focused).
    pub(super) fn room_cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        let listed = self.room.as_ref().is_some_and(|rm| rm.side == Side::Listed);
        (listed && tab_of(target).is_some()).then_some(crate::request::Cursor::Pointer)
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
            room::ALLOW_CHAT => {
                if let Some(rm) = &self.room {
                    room::allow_chat(&self.env, &rm.room, cx);
                }
            }
            room::RETRY_LOAD => {
                if let Some(rm) = &self.room {
                    room::retry_load(&rm.room);
                }
            }
            TAB_ROOM => self.switch_tab(Tab::Room, root, cx),
            TAB_MINE => self.switch_tab(Tab::Mine, root, cx),
            _ => return None,
        }
        Some(Handled::Yes)
    }

    /// A room's keys and taps: the players (a tap on them, L), the tabs
    /// (M), Pick songs (T).
    pub(super) fn room_input(
        &mut self,
        input: &Input,
        root: &mut Actor,
        cx: &mut Cx,
    ) -> Option<Handled> {
        // The wide layout's panel: its handle and rail fold and unfold it,
        // its head and its last line open the Players pop-up, a row
        // clicked opens it too, a row tapped shows its words first (a
        // second tap opens the pop-up).
        if let Input::Pointer {
            target: Some(t),
            gesture: crate::Gesture::Tap,
            kind,
            ..
        } = input
            && self.panel_shown()
        {
            let folded = self.room.as_ref().is_some_and(|rm| rm.folded);
            if t.within(super::players::HANDLE) || (folded && t.within(super::players::RAIL_NAME)) {
                self.toggle_panel(root, cx);
                return Some(Handled::Yes);
            }
            if t.within(super::players::HEAD) || t.within(room::PLAYERS_MORE) {
                self.open_players(cx);
                return Some(Handled::Yes);
            }
            if let Some(k) = t.chain.iter().find_map(|n| super::players::row_of(n)) {
                let shown = self.room.as_ref().and_then(|rm| rm.popover);
                if *kind == crate::PointerKind::Mouse || shown == Some(k) {
                    self.set_popover(None, root);
                    self.open_players(cx);
                } else {
                    self.set_popover(Some(k), root);
                }
                return Some(Handled::Yes);
            }
            if t.within("RoomPlayers") {
                self.set_popover(None, root);
                return Some(Handled::Yes);
            }
        }
        // A click or a tap on a tab shows it (round SF); the focus stays
        // where it was (the tabs are the pointer's and M's, not a stop of
        // the arrows).
        if let Input::Pointer {
            target: Some(t),
            gesture: crate::Gesture::Tap,
            ..
        } = input
            && let Some(rm) = &self.room
            && rm.side == Side::Listed
            && let Some(to) = tab_of(t)
        {
            self.switch_tab(to, root, cx);
            return Some(Handled::Yes);
        }
        // A mouse over a row: its words.
        if let Input::Hover { target } = input
            && self.panel_shown()
        {
            let k = target
                .as_ref()
                .and_then(|t| t.chain.iter().find_map(|n| super::players::row_of(n)));
            self.set_popover(k, root);
        }
        // A tap on the players (a phone's strip, the short side's panel)
        // opens the full list.
        if let Input::Pointer {
            target: Some(t),
            gesture: crate::Gesture::Tap,
            ..
        } = input
            && self.room.is_some()
            && !self.panel_shown()
            && (t.within("RoomPlayers") || t.within(room::PLAYERS_STRIP))
        {
            self.open_players(cx);
            return Some(Handled::Yes);
        }
        // L: the wide layout's panel folded or unfolded; elsewhere the
        // players (the strip and More open them too).
        if let Input::Key(k) = input
            && self.letter_shortcut(k)
            && k.key.eq_ignore_ascii_case("l")
            && self.room.as_ref().is_some_and(|rm| rm.side != Side::Joining)
        {
            if self.panel_shown() {
                self.toggle_panel(root, cx);
            } else {
                self.open_players(cx);
            }
            return Some(Handled::Yes);
        }
        // M: the other tab (revision 17).
        if let Input::Key(k) = input
            && self.letter_shortcut(k)
            && k.key.eq_ignore_ascii_case("m")
            && let Some(rm) = &self.room
            && rm.side == Side::Listed
        {
            let to = match rm.tab {
                Tab::Room => Tab::Mine,
                Tab::Mine => Tab::Room,
            };
            self.switch_tab(to, root, cx);
            return Some(Handled::Yes);
        }
        // T: an operator who does not choose takes the pick over.
        if let Input::Key(k) = input
            && self.letter_shortcut(k)
            && k.key.eq_ignore_ascii_case("t")
            && self.room_may_take()
        {
            self.take_pick(cx);
            return Some(Handled::Yes);
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
        None
    }

    /// The room's messages: More's picks, rules applied.
    pub(super) fn room_message(&mut self, msg: &Message, root: &mut Actor, cx: &mut Cx) {
        let Some(rm) = &self.room else {
            return;
        };
        let room = rm.room.clone();
        if msg.name == crate::screens::room_dialogs::RULES_SAVED {
            self.refill_room(root, cx);
        }
        // The due notice's button: the screens over the room leave, and
        // the stage's prompt opens once this one is shown again.
        if msg.name == room::DUE_GO && cx.covered_by_yielding() {
            cx.surface();
        }
        // The window asked to close while leaving costs something: asked
        // here, where the room's screen is the one shown.
        if msg.name == crate::ui::CLOSE_REQUESTED && cx.is_current_screen() {
            self.room_leave(room::LeaveBy::Quit, cx);
        }
        // The question's Leave: no second question.
        if let Some(by) = room::left_by(msg)
            && cx.is_current_screen()
        {
            self.room_left(by, cx);
        }
        if crate::screens::menu_pick(msg, room::MORE) == Some(MORE_FOLD) {
            self.toggle_panel(root, cx);
            cx.focus("Wheel");
            return;
        }
        // The list's items (revision 17).
        if let Some(id) = crate::screens::menu_pick(msg, room::MORE)
            && matches!(id, MORE_TAB | MORE_REMOVE | MORE_RETRY_UPLOAD | MORE_ADD_PACK)
        {
            let entry = self.shown_entry();
            match id {
                MORE_TAB => {
                    let to = match self.room.as_ref().map(|rm| rm.tab) {
                        Some(Tab::Room) => Tab::Mine,
                        _ => Tab::Room,
                    };
                    self.switch_tab(to, root, cx);
                }
                MORE_ADD_PACK => {
                    let pack = self.song().and_then(|s| s.pack.clone());
                    let songs: Vec<SongEntry> = self
                        .data
                        .songs
                        .iter()
                        .filter(|s| pack.is_some() && s.pack == pack)
                        .cloned()
                        .collect();
                    if let Err(why) = room.borrow_mut().add_pack(&songs) {
                        NoticeDialog::show_refusal(self.env.clone(), &room, &why, cx);
                    }
                    cx.focus("Wheel");
                }
                MORE_REMOVE => {
                    if let Some(entry) = entry
                        && let Err(why) = room.borrow_mut().remove_song(entry)
                    {
                        NoticeDialog::show_refusal(self.env.clone(), &room, &why, cx);
                    }
                    cx.focus("Wheel");
                }
                _ => {
                    if let Some(entry) = entry {
                        room.borrow_mut().retry_song_upload(entry);
                    }
                    cx.focus("Wheel");
                }
            }
            return;
        }
        if let Some(id) = crate::screens::menu_pick(msg, room::MORE)
            && id == super::favorites_mode::FAVORITE
        {
            self.toggle_favorite(root, cx);
            cx.focus("Wheel");
        } else if crate::screens::menu_pick(msg, room::MORE) == Some(MORE_VOLUME) {
            self.open_volume(cx);
        } else if let Some(id) = crate::screens::menu_pick(msg, room::MORE) {
            match room::more_picked(id, &room, cx) {
                room::MoreAction::Rules => self.open_rules(cx),
                room::MoreAction::Players => self.open_players(cx),
                room::MoreAction::ReplaceInvite => {
                    room::ask_replace_invite(&self.env, &room, cx);
                }
                room::MoreAction::Leave => {
                    self.room_leave(room::LeaveBy::More, cx);
                }
                room::MoreAction::AllowChat => room::allow_chat(&self.env, &room, cx),
                room::MoreAction::Done => self.refill_room(root, cx),
            }
        }
    }

    /// The footer's Start hint in a room: the picker's pick (practice, or
    /// an online room's Room songs), on My songs the song added to the
    /// room (or said to be in it already), or none.
    pub(super) fn room_play_hint(&self) -> Option<Option<UiText>> {
        let rm = self.room.as_ref()?;
        let r = rm.room.borrow();
        Some(match (rm.side, rm.tab) {
            (Side::Listed, Tab::Mine) if r.may_add() => Some(
                match self.song().and_then(|s| r.listed(s)) {
                    Some(_) => ddi_l10n::msg::room::song_in_room(),
                    None => ddi_l10n::msg::room::add_song_use(),
                },
            ),
            (Side::Listed, Tab::Mine) => None,
            // Song choice: Group (revision 22): what this player's Enter
            // does to their choice; nothing for one who may not choose.
            (Side::Listed, Tab::Room) if r.group_choice() => {
                if r.choice_blocked().is_some() {
                    None
                } else {
                    let shown = self.shown_entry().zip(self.shown_chart_key());
                    let mine = r.my_choice().map(|c| (c.song, c.chart.clone()));
                    // A song the Repeats rule excludes says why it can't
                    // be chosen (round RL4b).
                    let excluded = self.shown_entry().and_then(|e| r.repeat_refused(e));
                    Some(if shown.is_some() && shown == mine && r.choice_waits() {
                        // Chosen while a stage was played: Enter starts
                        // the draw now (round FX).
                        ddi_l10n::msg::room::hint_start_draw()
                    } else if shown.is_some() && shown == mine {
                        ddi_l10n::msg::room::hint_clear_choice()
                    } else if let Some(rule) = excluded {
                        ddi_l10n::msg::room::hint_repeated(room::repeats_id(rule))
                    } else if r.starts_draw_now() {
                        ddi_l10n::msg::room::hint_choose_draw()
                    } else {
                        ddi_l10n::msg::room::hint_choose()
                    })
                }
            }
            _ => r.chooses().then(|| {
                // Who does not have the song shown yet, said before the
                // pick (stage 2).
                let missing = self
                    .shown_entry()
                    .and_then(|id| r.state().songs.get(id))
                    .map_or(0, |e| r.missing_count(e));
                if missing > 0 {
                    ddi_l10n::msg::room::pick_use_missing(r.next_pick_number(), missing)
                } else {
                    ddi_l10n::msg::room::pick_use(r.next_pick_number())
                }
            }),
        })
    }

    /// The chart shown's digest as the room names it, if it has one.
    fn shown_chart_key(&self) -> Option<ddi_room::ChartKey> {
        let (charts, pick) = self.charts();
        let chart = pick.and_then(|p| charts.get(p))?;
        chart.digest.map(|d| ddi_room::ChartKey::new(d.to_string()))
    }

    /// A phone's Play button in a room: hidden where Start does nothing
    /// here (a member who does not choose, on Room songs; one who may not
    /// add, on My songs), and on My songs saying what it does there.
    pub(super) fn room_play_button(&self, root: &mut Actor) {
        let Some(rm) = &self.room else {
            return;
        };
        let hint = self.room_play_hint();
        if let Some(play) = hit::find_mut(root, "Play") {
            play.visible = hint != Some(None);
        }
        // On My songs, and on Room songs under Song choice: Group (revision
        // 22), it says what it does there.
        let says = rm.tab == Tab::Mine || rm.room.borrow().group_choice();
        if rm.side == Side::Listed
            && says
            && let Some(Some(words)) = hint
        {
            crate::screens::set(root, "Play", "Label", &String::from(words));
        }
    }

    /// The song's side hidden on an empty Room songs tab (no song to show:
    /// the list's place says what to do).
    pub(super) fn room_song_side(&self, root: &mut Actor) {
        if self.room.is_none() || self.room_joining() {
            return;
        }
        let empty = self.room_listing() && self.data.songs.is_empty();
        if let Some(song) = hit::find_mut(root, "Song") {
            song.visible = !empty;
        }
    }

    /// The footer's M hint: the other tab's name (an online room's).
    pub(super) fn room_tab_hint(&self) -> Option<UiText> {
        let rm = self.room.as_ref()?;
        (rm.side == Side::Listed).then(|| match rm.tab {
            Tab::Room => ddi_l10n::msg::room::tab_my_songs(),
            Tab::Mine => ddi_l10n::msg::room::tab_room_songs(),
        })
    }

    /// Says what the room shows (`ThemeState::room`), when anything it
    /// says changed since it was last said.
    pub(super) fn room_publish(&self, items: Vec<String>) {
        if let Some(rm) = &self.room {
            // The tab shown (revision 17).
            let tab = (rm.side == Side::Listed).then_some(match rm.tab {
                Tab::Room => "room",
                Tab::Mine => "mine",
            });
            let panel = (
                self.panel_shown().then_some(if rm.folded { "folded" } else { "open" }),
                rm.popover,
            );
            let key = {
                let r = rm.room.borrow();
                let view = match r.view() {
                    RoomView::Select => "select",
                    RoomView::Prompt(_) => "prompt",
                };
                (r.version(), room::seconds_left(&r), r.refusal().cloned(), view, items, tab, panel)
            };
            if rm.published.borrow().as_ref() == Some(&key) {
                return;
            }
            room::publish(&self.env, &rm.room, key.3, key.4.clone());
            room::publish_dialog(&self.env, |v| {
                v.tab = tab.map(str::to_string);
                v.panel = panel.0.map(str::to_string);
                v.popover = panel.1;
            });
            *rm.published.borrow_mut() = Some(key);
        }
    }

    /// The room's debug view is said again at the next publish (another
    /// screen said its own meanwhile).
    pub(super) fn room_republish(&self) {
        if let Some(rm) = &self.room {
            *rm.published.borrow_mut() = None;
        }
    }
}

/// Plays `command` on the parts named `names` among `parts`.
/// The tab a target is on, if one.
/// Under Charts: Adaptive (round OC) the chart of `charts` (`song`'s, in
/// the style shown) remembered for `song` this session, chosen on its
/// side or on a stage's Ready card: what its side and its row show where
/// nothing was picked on it since it was shown (round LO, the row).
pub(super) fn remembered_pick(
    room: &crate::state::room::SharedRoom,
    song: &SongEntry,
    charts: &[&ChartEntry],
) -> Option<usize> {
    let r = room.try_borrow().ok()?;
    if !r.songs_without_charts() {
        return None;
    }
    let key = r.remembered_start(song)?;
    charts
        .iter()
        .position(|c| c.digest.is_some_and(|d| d.to_string() == key.as_str()))
}

/// Counts the changes to the charts remembered for songs: the rows are
/// filled again for one ([`remembered_pick`]).
pub(super) fn remembered_version(room: Option<&crate::state::room::SharedRoom>) -> u64 {
    room.and_then(|r| r.try_borrow().ok()).map_or(0, |r| r.remembered_version())
}

/// Who chose listed song `song` (revision 22), as its row says it: this
/// player as "you" first, then the others in the draw's order; two names
/// at most, else the first and how many more. None: nobody.
fn chosen_words(r: &crate::state::room::RoomClient, song: RoomSongId) -> Option<UiText> {
    let mut who = r.chosen_by(song);
    if who.is_empty() {
        return None;
    }
    if let Some(i) = who.iter().position(|id| *id == r.me()) {
        who.remove(i);
        who.insert(0, r.me());
    }
    let name = |id: ddi_room::ParticipantId| -> UiText {
        if id == r.me() {
            ddi_l10n::msg::room::chosen_you()
        } else {
            UiText::data(r.member_of(id).map(room::person_name).unwrap_or_default())
        }
    };
    Some(match who.as_slice() {
        [a] => ddi_l10n::msg::room::song_chosen_one(name(*a)),
        [a, b] => ddi_l10n::msg::room::song_chosen_two(name(*a), name(*b)),
        [a, rest @ ..] => ddi_l10n::msg::room::song_chosen_more(name(*a), rest.len()),
        [] => return None,
    })
}

fn tab_of(target: &crate::hit::Target) -> Option<Tab> {
    if target.within(TAB_ROOM) {
        Some(Tab::Room)
    } else if target.within(TAB_MINE) {
        Some(Tab::Mine)
    } else {
        None
    }
}

fn play_parts<'a>(parts: &mut [Actor], names: impl IntoIterator<Item = &'a str>, command: &str) {
    let mut problems = ddi_actor::Problems::default();
    for name in names {
        if let Some(part) = parts.iter_mut().find(|c| c.name == name) {
            part.play(command, &mut problems);
        }
    }
}

/// The styles of `song`'s charts, in their order, each named as the
/// library names it (its layout's own name: song select translates a
/// built-in one as it shows it, `SelectScreen::style_label`).
fn song_styles(song: &SongEntry) -> Vec<crate::screens::data::StyleDef> {
    let mut styles: Vec<crate::screens::data::StyleDef> = Vec::new();
    for c in &song.charts {
        if !styles.iter().any(|s| s.id == c.style) {
            let name = ddi_chart::Layout::builtin(&c.style).map_or(c.style.clone(), |l| l.name);
            styles.push(crate::screens::data::StyleDef {
                id: c.style.clone(),
                name,
            });
        }
    }
    styles
}

#[cfg(test)]
#[test]
fn a_followed_songs_styles_are_named_as_the_library_names_them() {
    assert!(ddi_l10n::set_language("qps-ploc"));
    let chart = |style: &str| ChartEntry {
        style: style.into(),
        ..ChartEntry::default()
    };
    let song = SongEntry {
        charts: vec![chart("dance-single"), chart("my-layout"), chart("dance-single")],
        ..SongEntry::default()
    };
    let styles = song_styles(&song);
    ddi_l10n::set_language("en");
    let single = ddi_chart::Layout::builtin("dance-single").unwrap().name;
    let names: Vec<(&str, &str)> = styles.iter().map(|s| (s.id.as_str(), s.name.as_str())).collect();
    assert_eq!(names, [("dance-single", single.as_str()), ("my-layout", "my-layout")]);
}
