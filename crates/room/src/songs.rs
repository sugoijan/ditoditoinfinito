//! The room's song list (room model revision 17, round RL1: stage 1 of
//! `docs/plans/multiplayer-room-songs.md`): the songs an online room's
//! stages are picked from, apart from each member's library and the same
//! for everyone, mirrored as the rest of the room is.
//!
//! An entry is a whole song (all its charts), named by its content
//! ([`RoomSong::key`]: a hash of its original charts' digests, made by the
//! adder's device with `ddi_share_protocol::song_key` and checked by the
//! server against the record uploaded), so the same song added twice is
//! one entry. It carries what a row needs to be drawn before anyone has
//! downloaded the song (its card, its charts' styles, difficulties and
//! levels), who added it and when, and where the room's share of it is
//! once uploaded. The room never reads a song: its key and its charts'
//! digests are opaque to it.
//!
//! Who may add (`RoomState::may_add`, the one check): the hosts (the
//! owner, co-hosts and administrators: `Capability::AddSongs`), and with
//! the owner's room setting "Who can add songs" at Everyone ([`WhoAdds`],
//! room model revision 21, round RL3: stage 3) every person in the room
//! who is not held to watching by the watch-only link (playing or
//! watching by choice; bots too; never a CPU or the audience). Removing:
//! whoever added it, or a host; or the authority (the server's operator
//! removing a reported song, round AT), with the same event: refused only
//! by a stage not yet over, it takes the choices of a chosen song away.
//! Bounds ([`MAX_SONGS`], [`ListRules`]): at most a hundred songs, the
//! list's size a host setting up to it, what happens when it is full
//! another (remove the oldest not in use, or refuse), and a person at
//! most half the list's size, hosts and co-hosts the whole list
//! (`RoomState::song_quota`, round DB). A song is uploaded when it is added (the
//! server's grant per entry), kept while the room lives and deleted when
//! it is removed or the room ends. A bundled song is uploaded as any
//! other (revision 19, round DB: the room's share the one source); a
//! device that has a listed song by its content plays its own copy. An
//! entry a room stored before listed a bundled song with nothing uploaded
//! ([`RoomSong::bundled`]), which still reads and plays.

use crate::ids::{ChartKey, ParticipantId, Revision, RoomSongId, SongKey};
use crate::state::{ContentCard, ContentStatus};
use serde::{Deserialize, Serialize};

/// The most songs a room's list holds: the hard bound under the host's
/// list size ([`ListRules::size`]).
pub const MAX_SONGS: u16 = 100;
/// The most original charts an entry lists.
pub const MAX_CHARTS: usize = 32;
/// The longest key an entry's song or chart may have (bytes).
pub const MAX_KEY: usize = 64;
/// The longest style or difficulty an entry's chart names (bytes: a
/// style's id, `dance-single`, and a difficulty's name, `Challenge`, are
/// short), so that a full list at its worst stays well inside its row.
pub const MAX_CHART_TEXT: usize = 32;

/// The room's song list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSongs {
    /// Oldest first (by id).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<RoomSong>,
    /// The next entry's id (from 1): ids are never reused.
    #[serde(default)]
    pub next: u64,
    #[serde(default, skip_serializing_if = "ListRules::is_default")]
    pub rules: ListRules,
    /// The revision of the list's last change (an entry, its content, its
    /// rules, who may add): what the server compares to write its row
    /// only when the list changed.
    #[serde(default)]
    pub changed_at: Revision,
    /// Who may add songs (revision 21, [`crate::Command::SetWhoAdds`]):
    /// the hosts until the owner opens it to everyone. Left out of the
    /// JSON at Hosts, so a room stored before reads as Hosts.
    #[serde(default, skip_serializing_if = "WhoAdds::is_hosts")]
    pub who_adds: WhoAdds,
}

impl RoomSongs {
    /// A room with no list ever: what a room stored before revision 17
    /// reads as, and is written as (nothing).
    pub fn is_empty(&self) -> bool {
        *self == RoomSongs::default()
    }

    pub fn get(&self, id: RoomSongId) -> Option<&RoomSong> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn by_key(&self, key: &SongKey) -> Option<&RoomSong> {
        self.entries.iter().find(|e| e.key == *key)
    }

    /// The entries `who` added.
    pub fn added_by(&self, who: ParticipantId) -> usize {
        self.entries.iter().filter(|e| e.by == who).count()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// What the host set of the list ([`crate::Command::SetSongList`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRules {
    /// How many songs it holds, 1 to [`MAX_SONGS`].
    pub size: u16,
    #[serde(default)]
    pub when_full: WhenFull,
}

impl Default for ListRules {
    fn default() -> Self {
        ListRules {
            size: MAX_SONGS,
            when_full: WhenFull::default(),
        }
    }
}

impl ListRules {
    pub fn is_default(&self) -> bool {
        *self == ListRules::default()
    }

    /// The most songs one person who is not a host may have added: half
    /// the list's size (the maintainer, 2026-11-19), at least one; hosts
    /// and co-hosts are exempt (`RoomState::song_quota`, 2026-11-20).
    pub fn per_person(&self) -> usize {
        (usize::from(self.size) / 2).max(1)
    }

    /// Whether these are rules a list may have.
    pub fn valid(&self) -> bool {
        (1..=MAX_SONGS).contains(&self.size)
    }
}

/// Who may add songs to the room's list: the owner's room setting "Who
/// can add songs" (room model revision 21, round RL3, stage 3 of
/// `docs/plans/multiplayer-room-songs.md`; the maintainer, 2026-11-19:
/// Hosts by default, Everyone a setting the host turns on). Hosts and
/// co-hosts may fill the whole list either way; anyone else, under
/// Everyone, half of it ([`ListRules::per_person`]). Turned back to
/// Hosts, nothing listed is removed (uploads under way finish) and only
/// new additions are refused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhoAdds {
    /// The owner, co-hosts and administrators.
    #[default]
    Hosts,
    /// Every person in the room (`RoomState::may_add`).
    Everyone,
}

impl WhoAdds {
    pub fn is_hosts(&self) -> bool {
        *self == WhoAdds::Hosts
    }
}

/// What an addition to a full list does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhenFull {
    /// The oldest songs go to make room: never one a stage not yet
    /// played names (the song picked, being readied or played). Whoever
    /// added them (round RL3: the oldest of all, not the adder's own; one
    /// who is not a host is refused at their half before anything goes,
    /// and pushes out one song an addition, round RFa, so they push out
    /// at most half the list).
    #[default]
    RemoveOldest,
    /// The song is refused until one is removed.
    Refuse,
}

/// One song of the list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSong {
    pub id: RoomSongId,
    /// The song's content (`ddi_share_protocol::song_key`).
    pub key: SongKey,
    pub card: SongCard,
    /// Its original charts (no version the game makes of them, which
    /// every device makes alike), in the song's order.
    pub charts: Vec<ChartLine>,
    /// Who added it, and when.
    pub by: ParticipantId,
    pub added_at: Revision,
    /// Being uploaded (`Pending`), there to load (`Available`), or not
    /// (`Unavailable`, why: its upload's charts were not the ones added).
    pub status: ContentStatus,
    /// The room's share of it, once uploaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// Listed before room model revision 19 as a song bundled with the
    /// game, by its content alone, nothing uploaded (no share): only a
    /// room stored then has such an entry. Every device that has the song
    /// (by its content, as for any listed song) plays its own copy; one
    /// that has not cannot load it. Never made since (round DB).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bundled: bool,
}

impl RoomSong {
    /// Its chart `chart`, if it is one of its charts.
    pub fn chart(&self, chart: &ChartKey) -> Option<&ChartLine> {
        self.charts.iter().find(|c| c.chart == *chart)
    }

    /// What a stage that plays its chart `line` shows before it is loaded.
    pub fn card_for(&self, line: &ChartLine) -> ContentCard {
        ContentCard {
            title: self.card.title.clone(),
            artist: self.card.artist.clone(),
            difficulty: line.difficulty.clone(),
            meter: line.meter,
            style: line.style.clone(),
        }
    }

    pub fn available(&self) -> bool {
        self.status == ContentStatus::Available
    }
}

/// What a list's row shows of a song before it is loaded: the words the
/// adder's library has for it (display only, never checked).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongCard {
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    /// The pack it came in (none: a bundled song, or none named).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pack: String,
}

impl SongCard {
    pub fn fits(&self) -> bool {
        [&self.title, &self.artist, &self.pack]
            .iter()
            .all(|t| t.chars().count() <= ContentCard::MAX_TEXT)
    }
}

/// One original chart of a listed song.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartLine {
    /// Its digest (what a pick names, `ContentId::chart`).
    pub chart: ChartKey,
    /// Its style's id (`dance-single`), difficulty as the chart names it
    /// (`Hard`) and level.
    pub style: String,
    pub difficulty: String,
    #[serde(default)]
    pub meter: u32,
}

impl ChartLine {
    pub fn fits(&self) -> bool {
        self.chart.as_str().len() <= MAX_KEY
            && !self.chart.as_str().is_empty()
            && self.style.len() <= MAX_CHART_TEXT
            && self.difficulty.len() <= MAX_CHART_TEXT
    }
}

/// A song as its adder's device describes it ([`crate::Command::AddSong`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSong {
    pub key: SongKey,
    pub card: SongCard,
    pub charts: Vec<ChartLine>,
}

impl NewSong {
    /// Within the list's bounds: a key, one chart at least and at most
    /// [`MAX_CHARTS`], each named once, every text within its bound.
    pub fn fits(&self) -> bool {
        let key = self.key.as_str();
        let unique = self
            .charts
            .iter()
            .enumerate()
            .all(|(i, c)| !self.charts[..i].iter().any(|d| d.chart == c.chart));
        !key.is_empty()
            && key.len() <= MAX_KEY
            && (1..=MAX_CHARTS).contains(&self.charts.len())
            && unique
            && self.card.fits()
            && self.charts.iter().all(ChartLine::fits)
    }
}

/// Why an entry left the list ([`crate::Event::SongRemoved`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Removal {
    /// Its adder or a host removed it, or the authority did (the server's
    /// operator, round AT): one reason, so no client tells them apart.
    Removed,
    /// The list was full and it was the oldest not in use.
    ListFull,
    /// Its adder went before its upload was done.
    AdderGone,
}
