//! Downloading a room's listed songs ahead of the picks
//! (`docs/plans/multiplayer-room-songs.md`, stage 2, `room-preload-v1`,
//! PROVISIONAL): each member's device fetches the room's songs it does not
//! have into a store of the room's own ([`PreloadJob`]: the shell's to do,
//! answered by [`RoomClient::preload_listed`], [`RoomClient::preload_progress`]
//! and [`RoomClient::preload_done`]), so that a pick is ready at once and
//! the readiness window's loading wait ends by itself
//! (`hosted-deadline-v2` ends it once everyone connected has loaded).
//!
//! - **What**: every listed song by default (Options → Data, "Download
//!   room songs ahead": Everything, Likely next, Off; a phone's or a
//!   data-saving browser's default is Likely next,
//!   `crate::state::settings::device_room_ahead`): its chart, music and
//!   pictures, never a movie (it streams when a stage plays it, as before).
//!   A song this device's own library has (the same original charts) needs
//!   nothing: it is ready at once.
//! - **Banners first**: each song's share listing and banner (small) before
//!   any song's bytes, so the rows fill in.
//! - **In what order**: the song a stage picked and not yet played first,
//!   then the one the picker is on, then the newest added, then the rest;
//!   [`PRELOAD_V1`]'s bounded number at once.
//! - **When**: never from a stage's countdown to its end (`Countdown`,
//!   `Stage`, `SongCompletion`: a download in flight is stopped, taken up
//!   again after), while a stage gathers readiness only its song, and not
//!   while this device uploads a song of its own. A hidden page is not
//!   paused (a browser does not slow a download under way; the next song
//!   starts at the hidden page's next run of the room).
//! - **Retries** as a stage's song's (`load-retry-v0`'s waits); spent, the
//!   song waits for the next connection.
//! - **Gone**: a song removed from the list is deleted from the store
//!   ([`PreloadJob::Forget`]); the whole store when the room is left or
//!   ends (the shells' own, [`RoomClient::store`]).
//! - **Who has what** (`ddi_room::live::have`): what this device has is
//!   said to the room over the live side, and the room says everyone's
//!   ([`RoomClient::have_count`]).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ddi_room::live::HaveSet;
use ddi_room::{ContentStatus, LinkState, Phase, RoomSong, RoomSongId};

use super::{EntryState, LOAD_RETRY_V0_MS, RoomClient, Sharing};
use crate::screens::data::SongEntry;
use crate::state::settings::RoomAhead;

/// The numbers of the plan (PROVISIONAL, `room-preload-v1`; `-v0`, four
/// songs within 64 MB for Likely next, retired in round DB).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreloadRules {
    pub id: &'static str,
    /// Songs downloaded at once.
    pub at_once: usize,
    /// Listings (a share's list and its banner) fetched at once.
    pub listings_at_once: usize,
    /// Likely next: the newest songs downloaded beyond the picked one and
    /// the picker's, at most this many …
    pub likely_newest: usize,
    /// … and this many bytes in all.
    pub likely_bytes: u64,
    /// A song's size where its listing was not read (bytes).
    pub unknown_bytes: u64,
}

/// Likely next is eight songs within 128 MB (round DB, the maintainer,
/// 2026-11-20).
pub const PRELOAD_V1: PreloadRules = PreloadRules {
    id: "room-preload-v1",
    at_once: 2,
    listings_at_once: 4,
    likely_newest: 8,
    likely_bytes: 128 * 1024 * 1024,
    unknown_bytes: 8 * 1024 * 1024,
};

/// Stops a download in flight: the shell looks between its chunks and
/// answers [`PreloadProblem::Stopped`].
#[derive(Clone, Debug, Default)]
pub struct Stop(Arc<AtomicBool>);

impl Stop {
    pub fn stopped(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl PartialEq for Stop {
    fn eq(&self, other: &Stop) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// What the shell is asked to do for the room's store (`store`: its name,
/// [`RoomClient::store`]).
#[derive(Clone, Debug, PartialEq)]
pub enum PreloadJob {
    /// Share `share`'s list and its song's banner, into the store: answered
    /// by [`RoomClient::preload_listed`].
    Listing {
        song: RoomSongId,
        share: String,
        store: String,
    },
    /// The song's chart, music and pictures (never a movie) into the store,
    /// told as it goes ([`RoomClient::preload_progress`]) and at its end
    /// ([`RoomClient::preload_done`]); given up where `stop` says so.
    Song {
        song: RoomSongId,
        share: String,
        store: String,
        stop: Stop,
    },
    /// Share `share`'s files deleted from the store (its song left the
    /// list).
    Forget { share: String, store: String },
}

/// A listed song's share as read: its record's entry, its pack, its banner
/// as the shell's texture source (from the store) and what downloading it
/// would take (bytes, movies left out).
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub entry: ddi_library::manifest::ManifestEntry,
    pub pack: String,
    pub banner: Option<String>,
    pub bytes: u64,
}

/// Why a download ahead did not end with the song here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreloadProblem {
    /// The share or a file could not be fetched (tried again).
    Download,
    /// Stopped by the plan ([`Stop`]): taken up again later.
    Stopped,
    /// The device's storage is full (the web's quota): the plan falls back
    /// to Likely next, said once.
    Quota,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Step<T> {
    #[default]
    Not,
    Asked,
    Done(T),
    Failed,
}

impl<T> Step<T> {
    fn asked(&self) -> bool {
        matches!(self, Step::Asked)
    }
}

/// What this device has of one listed song.
#[derive(Clone, Debug, Default)]
struct Got {
    share: String,
    listing: Step<Box<Listing>>,
    song: Step<()>,
    progress: Option<(f64, f64)>,
    tries: u32,
    retry_at: Option<u64>,
    listing_tries: u32,
    listing_retry_at: Option<u64>,
    stop: Option<Stop>,
}

/// The client's part ([`RoomClient`]'s `ahead`).
#[derive(Debug, Default)]
pub(super) struct Ahead {
    /// The store's name the shell set (none: the room's id).
    store: Option<String>,
    got: BTreeMap<RoomSongId, Got>,
    jobs: Vec<PreloadJob>,
    /// The plan for tests and the bots (none: the settings').
    rule: Option<RoomAhead>,
    /// The device's storage was full: Likely next at most from then, and
    /// whether that was said.
    quota: bool,
    quota_said: bool,
    /// What this device has, as last set on the live link.
    have: Option<HaveSet>,
}

impl RoomClient {
    /// The name of the room's store on this device: what the shell keeps
    /// the room's songs under and empties when the room is left (the
    /// room's id, unless the shell named one: a bot's own).
    pub fn store(&self) -> String {
        self.ahead
            .store
            .clone()
            .unwrap_or_else(|| self.state().room.to_string())
    }

    /// The shell names the room's store (`crate::bots`: each bot its own).
    pub fn set_store(&mut self, store: impl Into<String>) {
        self.ahead.store = Some(store.into());
    }

    /// The plan this client follows: `rule`, in place of the settings'
    /// (tests; the bots' `--no-ahead`).
    pub fn set_ahead(&mut self, rule: Option<RoomAhead>) {
        self.ahead.rule = rule;
        self.version += 1;
    }

    /// How much of the list this device downloads ahead now.
    pub fn ahead_rule(&self) -> RoomAhead {
        let rule = self
            .ahead
            .rule
            .unwrap_or_else(|| crate::state::settings::Settings::cached().room_ahead());
        match rule {
            RoomAhead::Everything if self.ahead.quota => RoomAhead::LikelyNext,
            r => r,
        }
    }

    /// The store's jobs for the shell, once each.
    pub fn take_preload_jobs(&mut self) -> Vec<PreloadJob> {
        std::mem::take(&mut self.ahead.jobs)
    }

    /// Whether the device's storage ran out (the web's quota): Likely
    /// next at most from then.
    pub fn ahead_quota(&self) -> bool {
        self.ahead.quota
    }

    /// The device's storage ran out: Likely next at most from now, said
    /// once on the status line.
    fn out_of_room(&mut self) {
        self.ahead.quota = true;
        if !self.ahead.quota_said {
            self.ahead.quota_said = true;
            if let Some(l) = self.online_link_mut() {
                l.set_notice(Some(crate::state::online::link::LinkNotice::AheadQuota));
            }
        }
    }

    /// Listing `song` read (its banner in the store), or why not.
    pub fn preload_listed(&mut self, song: RoomSongId, result: Result<Listing, PreloadProblem>) {
        let now = self.now_ms();
        if result == Err(PreloadProblem::Quota) {
            self.out_of_room();
        }
        let Some(g) = self.ahead.got.get_mut(&song).filter(|g| g.listing.asked()) else {
            return;
        };
        g.listing = match result {
            Ok(l) => Step::Done(Box::new(l)),
            Err(PreloadProblem::Stopped) => Step::Not,
            Err(PreloadProblem::Quota) => Step::Failed,
            Err(PreloadProblem::Download) => match LOAD_RETRY_V0_MS.get(g.listing_tries as usize) {
                Some(wait) => {
                    g.listing_tries += 1;
                    g.listing_retry_at = Some(now + wait);
                    Step::Not
                }
                None => Step::Failed,
            },
        };
        self.version += 1;
    }

    /// The download of `song` moved on: `done` bytes of `total`.
    pub fn preload_progress(&mut self, song: RoomSongId, done: f64, total: f64) {
        if let Some(g) = self.ahead.got.get_mut(&song).filter(|g| g.song.asked()) {
            let before = g.progress.map(|(d, t)| percent(d, t));
            g.progress = Some((done, total));
            if before != Some(percent(done, total)) {
                self.version += 1;
            }
        }
    }

    /// The download of `song` ended: here, or why not.
    pub fn preload_done(&mut self, song: RoomSongId, result: Result<(), PreloadProblem>) {
        let now = self.now_ms();
        // Out of room once more where the plan is Likely next already: not
        // tried again.
        let again = self.ahead.quota || self.ahead_rule() != RoomAhead::Everything;
        if result == Err(PreloadProblem::Quota) {
            self.out_of_room();
        }
        let Some(g) = self.ahead.got.get_mut(&song).filter(|g| g.song.asked()) else {
            return;
        };
        g.stop = None;
        g.progress = None;
        g.song = match result {
            Ok(()) => Step::Done(()),
            Err(PreloadProblem::Stopped) => Step::Not,
            Err(PreloadProblem::Quota) if again => Step::Failed,
            Err(PreloadProblem::Quota) => Step::Not,
            Err(PreloadProblem::Download) => match LOAD_RETRY_V0_MS.get(g.tries as usize) {
                Some(wait) => {
                    g.tries += 1;
                    g.retry_at = Some(now + wait);
                    Step::Not
                }
                None => Step::Failed,
            },
        };
        self.version += 1;
        self.sync_have();
    }

    /// Listed song `entry` as downloaded here (its record's entry, the
    /// listed charts alone), for its row and its song side: none until it
    /// is here.
    pub fn ahead_song(&self, entry: &RoomSong) -> Option<SongEntry> {
        let g = self.ahead.got.get(&entry.id)?;
        if g.song != Step::Done(()) {
            return None;
        }
        let Step::Done(l) = &g.listing else {
            return None;
        };
        let picked = super::picked_of(&l.entry, 0, &l.pack, l.banner.clone())?;
        Some(picked.song)
    }

    /// Listed song `entry`'s banner, as soon as it is here (before its
    /// song).
    pub fn ahead_banner(&self, entry: &RoomSong) -> Option<String> {
        match &self.ahead.got.get(&entry.id)?.listing {
            Step::Done(l) => l.banner.clone(),
            _ => None,
        }
    }

    /// The download of `entry` under way: bytes so far of all.
    pub(super) fn ahead_progress(&self, entry: &RoomSong) -> Option<(f64, f64)> {
        let g = self.ahead.got.get(&entry.id)?;
        g.song.asked().then(|| g.progress.unwrap_or((0.0, 0.0)))
    }

    /// Whether `entry` was downloaded here ahead.
    pub(super) fn ahead_here(&self, entry: &RoomSong) -> bool {
        self.ahead
            .got
            .get(&entry.id)
            .is_some_and(|g| g.song == Step::Done(()))
    }

    /// A stage's song read from the room's share (`finish_shared_found`):
    /// its entry here as if downloaded ahead (its files are in the store),
    /// its listing from what was read.
    pub(super) fn ahead_read(&mut self, share: &str, listing: Listing) {
        let Some(id) = self
            .room_songs()
            .iter()
            .find(|e| e.share.as_deref() == Some(share))
            .map(|e| e.id)
        else {
            return;
        };
        let g = self.ahead.got.entry(id).or_default();
        g.share = share.to_string();
        if let Some(stop) = g.stop.take() {
            stop.stop();
        }
        g.listing = Step::Done(Box::new(listing));
        g.song = Step::Done(());
        g.progress = None;
        self.version += 1;
        self.sync_have();
    }

    /// How many of the players have `entry`, of how many (people and bots
    /// connected who play; this device's own as it has it): none in a
    /// room that says nothing of it (practice).
    pub fn have_count(&self, entry: &RoomSong) -> Option<(u32, u32)> {
        if !self.plays_live() {
            return None;
        }
        let link = self.live_link()?;
        let haves = link.haves();
        let me = self.me;
        let mut of = 0;
        let mut have = 0;
        for m in self.state().members.values() {
            if m.left
                || m.is_cpu()
                || m.connection != ddi_room::Connection::Connected
                || m.intent != ddi_room::Intent::Play
            {
                continue;
            }
            of += 1;
            let has = if m.id == me {
                self.entry_state(entry) == EntryState::Ready
            } else {
                haves.get(&m.id).is_some_and(|s| s.has(entry.id))
            };
            if has {
                have += 1;
            }
        }
        (of > 0).then_some((have, of))
    }

    /// How many players do not have `entry` yet (the picker is told before
    /// a pick).
    pub fn missing_count(&self, entry: &RoomSong) -> u32 {
        self.have_count(entry).map_or(0, |(have, of)| of - have)
    }

    /// The plan, run with each tick of an online room: what left the list
    /// forgotten, what this device has said to the room, the downloads
    /// stopped where a stage plays, else the next ones asked for.
    pub(super) fn sync_preload(&mut self) {
        if !self.is_online() || self.mirror.state().is_none() {
            return;
        }
        let ended = matches!(
            self.link_state(),
            LinkState::Closed | LinkState::Left
        );
        self.forget_unlisted(ended);
        self.sync_have();
        if ended {
            return;
        }
        let now = self.now_ms();
        let paused = matches!(
            self.state().phase,
            Phase::Countdown { .. } | Phase::Stage { .. } | Phase::SongCompletion { .. }
        ) || self.uploading_here();
        let gathering = matches!(self.state().phase, Phase::Preparing { .. });
        let rule = self.ahead_rule();
        // Every song that could be downloaded, best first.
        let mut wanted: Vec<(u8, RoomSongId, String)> = self
            .room_songs()
            .iter()
            .filter(|e| e.status == ContentStatus::Available && e.share.is_some())
            // (Here already: in the library, downloaded, or read for a
            // stage; one downloaded is kept in the plan, done.)
            .filter(|e| self.ahead_here(e) || self.entry_state(e) != EntryState::Ready)
            .map(|e| (self.ahead_rank(e), e.id, e.share.clone().unwrap_or_default()))
            .collect();
        wanted.sort_by_key(|(rank, id, _)| (*rank, std::cmp::Reverse(*id)));
        // Stopped: what a stage plays over, what gathering readiness for
        // another song wants the link for, what the plan no longer wants.
        let planned = self.planned(&wanted, rule);
        // A stage's own load reads into the same store: what it reads is
        // not downloaded ahead a second time at once (review 2026-11-22,
        // room-client 5).
        let read_for_stage: std::collections::BTreeSet<RoomSongId> = self
            .room_songs()
            .iter()
            .filter(|e| self.stage_reads(e))
            .map(|e| e.id)
            .collect();
        let mut stopped = false;
        for (id, g) in &mut self.ahead.got {
            let keep = !paused
                && planned.contains(id)
                && (!gathering || ranked_first(&wanted, *id))
                && !read_for_stage.contains(id);
            if g.song.asked() && !keep {
                if let Some(stop) = g.stop.take() {
                    stop.stop();
                }
                g.song = Step::Not;
                g.progress = None;
                stopped = true;
            }
        }
        if stopped {
            self.version += 1;
        }
        if paused || rule == RoomAhead::Off {
            return;
        }
        let store = self.store();
        // Listings first: every song's banner before any song's bytes.
        let in_flight = self.ahead.got.values().filter(|g| g.listing.asked()).count();
        let mut room = PRELOAD_V1.listings_at_once.saturating_sub(in_flight);
        let mut listings_left = false;
        for (_, id, share) in &wanted {
            if gathering && !ranked_first(&wanted, *id) {
                break;
            }
            let g = self.ahead.got.entry(*id).or_default();
            g.share.clone_from(share);
            let due = g.listing == Step::Not && g.listing_retry_at.is_none_or(|t| now >= t);
            if g.listing == Step::Not {
                listings_left = true;
            }
            if due && room > 0 {
                room -= 1;
                g.listing = Step::Asked;
                g.listing_retry_at = None;
                self.ahead.jobs.push(PreloadJob::Listing {
                    song: *id,
                    share: share.clone(),
                    store: store.clone(),
                });
                self.version += 1;
            }
        }
        let listing_now = self.ahead.got.values().any(|g| g.listing.asked());
        if listings_left || listing_now {
            return;
        }
        let in_flight = self.ahead.got.values().filter(|g| g.song.asked()).count();
        let mut room = PRELOAD_V1.at_once.saturating_sub(in_flight);
        for (_, id, share) in &wanted {
            if room == 0 {
                break;
            }
            if !planned.contains(id) || (gathering && !ranked_first(&wanted, *id)) {
                continue;
            }
            // A stage's own load reads it already.
            if read_for_stage.contains(id) {
                continue;
            }
            let e = self.room_songs().iter().find(|e| e.id == *id).cloned();
            if e.as_ref()
                .is_none_or(|e| self.entry_state(e) == EntryState::Loading)
            {
                continue;
            }
            let g = self.ahead.got.entry(*id).or_default();
            let due = g.song == Step::Not && g.retry_at.is_none_or(|t| now >= t);
            if !due {
                continue;
            }
            room -= 1;
            let stop = Stop::default();
            g.song = Step::Asked;
            g.retry_at = None;
            g.stop = Some(stop.clone());
            self.ahead.jobs.push(PreloadJob::Song {
                song: *id,
                share: share.clone(),
                store: store.clone(),
                stop,
            });
            self.version += 1;
        }
    }

    /// Every download under way stopped: the room left, or its client let
    /// go (review 2026-11-22, room-client 6: none goes on writing into the
    /// store the shell deletes with the room).
    pub(super) fn stop_downloads(&mut self) {
        for g in self.ahead.got.values_mut() {
            if let Some(stop) = g.stop.take() {
                stop.stop();
            }
        }
    }

    /// Songs whose download failed for good are tried again (the link back).
    pub(super) fn preload_again(&mut self) {
        for g in self.ahead.got.values_mut() {
            if g.song == Step::Failed {
                g.song = Step::Not;
                g.tries = 0;
            }
            if g.listing == Step::Failed {
                g.listing = Step::Not;
                g.listing_tries = 0;
            }
        }
    }

    /// The songs the plan downloads under `rule`, of `wanted` (best
    /// first).
    fn planned(
        &self,
        wanted: &[(u8, RoomSongId, String)],
        rule: RoomAhead,
    ) -> std::collections::BTreeSet<RoomSongId> {
        match rule {
            RoomAhead::Off => Default::default(),
            RoomAhead::Everything => wanted.iter().map(|(_, id, _)| *id).collect(),
            RoomAhead::LikelyNext => {
                let mut out = std::collections::BTreeSet::new();
                let (mut newest, mut bytes) = (0, 0u64);
                for (rank, id, _) in wanted {
                    if *rank < 2 {
                        out.insert(*id);
                        continue;
                    }
                    let size = match self.ahead.got.get(id).map(|g| &g.listing) {
                        Some(Step::Done(l)) => l.bytes,
                        _ => PRELOAD_V1.unknown_bytes,
                    };
                    if newest >= PRELOAD_V1.likely_newest || bytes + size > PRELOAD_V1.likely_bytes {
                        break;
                    }
                    newest += 1;
                    bytes += size;
                    out.insert(*id);
                }
                out
            }
        }
    }

    /// Whether a stage not yet played reads `entry`'s song here now: its
    /// load handed to the shell for this readiness and not done (it may
    /// not show in the room yet, its Loading still on the way).
    fn stage_reads(&self, entry: &RoomSong) -> bool {
        let me = self.me;
        self.stages().iter().any(|st| {
            st.play.is_none()
                && st.content.as_ref().is_some_and(|c| {
                    c.id.song == entry.key && !self.picks.contains_key(&c.id)
                })
                && self.asked.contains(&(st.id, st.readiness_rev))
                && !matches!(st.prep(me).loading, ddi_room::Loading::Failed { .. })
        })
    }

    /// Where `entry` comes in the plan: 0 a stage's song not yet played, 1
    /// the one the picker is on, 2 the rest (the newest first).
    fn ahead_rank(&self, entry: &RoomSong) -> u8 {
        let picked = self.stages().iter().any(|st| {
            st.play.is_none() && st.content.as_ref().is_some_and(|c| c.id.song == entry.key)
        });
        if picked {
            0
        } else if self.spotlight_entry() == Some(entry.id) {
            1
        } else {
            2
        }
    }

    /// Whether this device uploads a song now (nothing downloads then).
    fn uploading_here(&self) -> bool {
        self.song_sharing
            .values()
            .chain(self.sharing.values().map(|(_, s)| s))
            .any(|s| matches!(s, Sharing::Uploading { .. }))
    }

    /// What left the list (or the whole room, `all`) is forgotten: a
    /// download stopped, its files deleted from the store.
    fn forget_unlisted(&mut self, all: bool) {
        let listed: BTreeMap<RoomSongId, Option<String>> = self
            .room_songs()
            .iter()
            .map(|e| (e.id, e.share.clone()))
            .collect();
        let gone: Vec<RoomSongId> = self
            .ahead
            .got
            .iter()
            .filter(|(id, g)| all || listed.get(id).is_none_or(|s| s.as_deref() != Some(&g.share)))
            .map(|(id, _)| *id)
            .collect();
        if gone.is_empty() {
            return;
        }
        let store = self.store();
        for id in gone {
            let Some(g) = self.ahead.got.remove(&id) else {
                continue;
            };
            if let Some(stop) = g.stop {
                stop.stop();
            }
            // (The whole store goes with the room: the shell's.)
            if !all && !g.share.is_empty() {
                self.ahead.jobs.push(PreloadJob::Forget {
                    share: g.share,
                    store: store.clone(),
                });
            }
        }
        self.version += 1;
    }

    /// What this device has, said to the room where it changed.
    pub(super) fn sync_have(&mut self) {
        if !self.is_online() {
            return;
        }
        let have = HaveSet::of(
            self.room_songs()
                .iter()
                .filter(|e| self.entry_state(e) == EntryState::Ready)
                .map(|e| e.id),
        );
        if self.ahead.have.as_ref() == Some(&have) {
            return;
        }
        self.ahead.have = Some(have.clone());
        if let Some(l) = self.online_link_mut() {
            l.authority.live_mut().set_have(have);
        }
    }

    /// The plan's state for the debug state and the tests: each listed
    /// song's (`listing`, `song`: `not`, `asked`, `done`, `failed`).
    pub fn preload_debug(&self) -> Vec<(RoomSongId, &'static str, &'static str)> {
        fn word<T>(s: &Step<T>) -> &'static str {
            match s {
                Step::Not => "not",
                Step::Asked => "asked",
                Step::Done(_) => "done",
                Step::Failed => "failed",
            }
        }
        self.ahead
            .got
            .iter()
            .map(|(id, g)| (*id, word(&g.listing), word(&g.song)))
            .collect()
    }
}

/// Whether `id` is the first of `wanted` and a stage's song (rank 0): what
/// a stage gathering readiness lets download.
fn ranked_first(wanted: &[(u8, RoomSongId, String)], id: RoomSongId) -> bool {
    wanted.iter().any(|(rank, i, _)| *rank == 0 && *i == id)
}

/// A whole percent of `done` of `total` (0 while the whole is not known).
fn percent(done: f64, total: f64) -> u32 {
    if total > 0.0 {
        ((done / total).clamp(0.0, 1.0) * 100.0).floor() as u32
    } else {
        0
    }
}
