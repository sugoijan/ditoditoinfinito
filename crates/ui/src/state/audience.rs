//! A room watched with its audience (`docs/plans/multiplayer-audience.md`,
//! "What the game builds"): people who watch beyond the room's members,
//! counted and not listed, read only. The device's seat is `ddi_room`'s
//! [`Audience`] over the shell's socket (the web's `WebSocket`, desktop's
//! tungstenite), its tickets from the server's audience join through
//! [`Online`](super::online::Online)'s HTTP, a new one each time the socket
//! is lost (a relay's restart is survived without one: the relay keeps its
//! seats).
//!
//! What it shows comes from the room's public view
//! ([`PublicView`]: the room's name, its host, the rules, the players,
//! each stage's song, start and result, the standings) and the count; a
//! stage is played on this device as a watched stage
//! (`ddi_session::play`, `StageSetup::watching`) fed by the audience's
//! frames through the same feed a member's view reads
//! ([`LiveRoom`]), its music and fields drawn a delay behind the room's
//! song time ([`AUDIENCE_VIEW_V1`], within the audience's ceiling).
//!
//! It sends nothing to the room but its hello, a resume after a gap and
//! its keep-alive (the seat's own). Sans-IO; natively tested over the fake
//! room's audience (`ddi_room::fake`).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use ddi_room::audience::public::{PublicContent, PublicKind, PublicView};
use ddi_room::audience::{AUDIENCE_V0, Audience, AudienceEnd, AudienceNews, AudienceRefusal};
use ddi_room::live::{LIVE_V1, LiveNews, LiveRules, VIEW_DELAY_V0, ViewDelay, ViewDelayRules};
use ddi_room::{
    ChartKey, Clock, Finisher, ManualClock, Millis, ParticipantId, Phase, ReconnectPolicy,
    RoomId, StageId, Transport,
};

use super::online::SharedOnline;
use super::online::api::{self, AudienceTicketDoc, Problem};
use super::room::{
    CpuSeat, LiveAsk, LiveOrder, LivePrep, LiveRoom, LoadJob, LoadProblem, Picked, SeatEnd, StageOrder,
    StageRules, StageTable, TableSource, ranking_of,
};

/// How far behind the room's song time an audience device plays a
/// stage's music and draws its fields: as the first stage starts,
/// `music_delay_ms`, within the audience's ceiling
/// (`AudienceRules::presentation_delay_ms`, two seconds), above what the
/// fake room measured a moment to take to a seat (0.37 s at the median,
/// 0.66 s at most, at 20 to 120 ms each way), fixed for a stage (the
/// music cannot move; the fields fall a little further behind it where a
/// frame comes later still, their own delay from what the frames measure,
/// `ddi_room::live::ViewDelay`); then moved at each stage's start by what
/// the last stage watched needed: where the fields had to fall further
/// behind the music than `rise_above_ms` for a tenth of its frames or
/// more (their own delay's 90th percentile), the next stage's music is
/// put that much further behind (rounded up to 50 ms, at most
/// `most_music_delay_ms`, which leaves the fields 400 ms of the
/// two-second ceiling); after a stage that needed less, it falls back
/// `fall_ms` toward `music_delay_ms`. Never within a stage: the music is
/// one take, never moved, sped up or slowed down, so nothing is heard;
/// within a stage the fields keep adapting on top of it, as before.
/// Resyncing at a silence was not chosen (songs have few, and a jump in
/// the sound is heard however short), nor a slow drift (a rate change of
/// the music is a pitch change, and the play's clock is the music's).
/// PROVISIONAL (`audience-view-v1`, 2026-11-13; `audience-view-v0`, the
/// same 0.8 s never moved, was retired on 2026-11-14).
pub const AUDIENCE_VIEW_V1: AudienceView = AudienceView {
    id: "audience-view-v1",
    music_delay_ms: 800,
    most_music_delay_ms: 1_600,
    rise_above_ms: 100,
    fall_ms: 200,
    lag_quantile: 0.9,
};

/// The audience's view's numbers, named and versioned. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudienceView {
    pub id: &'static str,
    /// The music's delay with nothing measured yet, and the one it falls
    /// back toward (ms).
    pub music_delay_ms: u64,
    /// The most it rises to (ms; within the audience's ceiling).
    pub most_music_delay_ms: u64,
    /// The fields' own delay beyond the music's (its `lag_quantile` over
    /// the last stage watched, ms) past which the next stage's music is
    /// put that much further behind.
    pub rise_above_ms: u64,
    /// How much it falls back after a stage that needed no rise (ms).
    pub fall_ms: u64,
    pub lag_quantile: f64,
}

impl AudienceView {
    /// The next stage's music delay after one watched at `current` ms
    /// whose fields fell `lags` ms further behind it (one a frame; none:
    /// nothing watched, kept).
    pub fn next_music_delay(&self, current: u64, lags: &[f32]) -> u64 {
        if lags.is_empty() {
            return current;
        }
        let mut sorted = lags.to_vec();
        sorted.sort_by(f32::total_cmp);
        let i = ((sorted.len() - 1) as f64 * self.lag_quantile).round() as usize;
        let lag = f64::from(sorted[i].max(0.0)).ceil() as u64;
        let next = if lag > self.rise_above_ms {
            current + lag.div_ceil(50) * 50
        } else {
            current
                .saturating_sub(self.fall_ms)
                .max(self.music_delay_ms)
        };
        next.min(self.most_music_delay_ms.max(self.music_delay_ms))
    }
}

/// A watching device's music delays behind the room's song time (a
/// member who watches, [`crate::state::room::WATCH_MUSIC_V0`], or the
/// audience, [`AUDIENCE_VIEW_V1`]): each stage's fixed as its order is
/// handed over, the next one's from how far the fields of the stage
/// watched last fell behind its music ([`AudienceView::next_music_delay`]).
#[derive(Clone, Debug)]
pub struct MusicDelays {
    pub rules: AudienceView,
    /// The next stage's (ms).
    next: u64,
    /// The stage watched last and how far behind the music its fields
    /// were drawn, a sample a frame (ms).
    lags: Option<(StageId, Vec<f32>)>,
    /// Each stage's, fixed (ms).
    fixed: BTreeMap<StageId, u64>,
}

impl MusicDelays {
    pub fn new(rules: AudienceView) -> MusicDelays {
        MusicDelays {
            rules,
            next: rules.music_delay_ms,
            lags: None,
            fixed: BTreeMap::new(),
        }
    }

    /// The next stage's delay (ms).
    pub fn next(&self) -> u64 {
        self.next
    }

    /// `stage`'s delay, where it was fixed (ms).
    pub fn of(&self, stage: StageId) -> Option<u64> {
        self.fixed.get(&stage).copied()
    }

    /// `stage`'s delay fixed as its order is handed over, from what the
    /// stage watched last needed (a stage handed over again keeps its own).
    pub fn fix(&mut self, stage: StageId) -> u64 {
        if let Some(d) = self.of(stage) {
            return d;
        }
        if let Some((_, lags)) = self.lags.take() {
            self.next = self.rules.next_music_delay(self.next, &lags);
        }
        self.fixed.insert(stage, self.next);
        self.next
    }

    /// The fields' own delay beyond the music this frame of `stage`.
    pub fn note(&mut self, stage: StageId, delay_ms: f64) {
        match &mut self.lags {
            Some((s, lags)) if *s == stage => lags.push(delay_ms as f32),
            _ => self.lags = Some((stage, vec![delay_ms as f32])),
        }
    }
}

/// The fields' own delay where the music is moved behind the room's
/// song time: what is left of the ceiling, `left_ms`, from nothing, as
/// the frames measure (rules `id`).
pub fn behind_music(id: &'static str, left_ms: f64) -> ViewDelay {
    let rules = ViewDelayRules {
        id,
        floor_ms: 0,
        ..VIEW_DELAY_V0
    };
    ViewDelay::new(rules, left_ms, 0.0)
}

/// Where the seat is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeatState {
    /// Asking for a ticket, or its socket opening.
    Joining,
    /// The socket is open.
    Watching,
    /// The socket was lost: a new ticket is asked for after a wait.
    Reconnecting,
    /// The audience join was refused (the room's audience full or closed,
    /// the link not the room's, a room without an audience, two seats of
    /// this person's already, the room ended).
    Refused(Refused),
    /// The relay closed the seat for good.
    Ended(AudienceEnd),
}

/// Why the audience join said no, as the screens word it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    Audience(AudienceRefusal),
    /// Another refusal (its code: data).
    Other(String),
}

impl Refused {
    pub fn of(p: &Problem) -> Refused {
        AudienceRefusal::ALL
            .into_iter()
            .find(|r| r.code() == p.code)
            .map_or_else(|| Refused::Other(p.code.clone()), Refused::Audience)
    }
}

/// A watched stage's song prepared (read from the room's share, found by
/// its digest) and what its live play needs: the reading horizon and,
/// with attacks, this device's battle plan (kept only where its digest is
/// the room's).
#[derive(Clone, Debug, PartialEq)]
struct Prepared {
    picked: Picked,
    prep: Option<LivePrep>,
}

/// An audience seat's client.
pub struct AudienceClient {
    online: SharedOnline,
    room: String,
    link: String,
    seat: Audience<Box<dyn Transport>, ManualClock>,
    clock: ManualClock,
    ticket_req: Option<u64>,
    attempt: u32,
    retry_at: u64,
    refused: Option<Refused>,
    version: u64,
    live_news: crate::state::room::LiveNewsTap,
    jobs: Vec<LoadJob>,
    /// Each stage's song asked for: its share and chart (a stage picked
    /// again with another song is asked for again; `prepared` and
    /// `problems` are of the song asked for, a late answer for another
    /// dropped).
    asked: BTreeMap<StageId, (String, ChartKey)>,
    prepared: BTreeMap<StageId, Prepared>,
    problems: BTreeMap<StageId, LoadProblem>,
    /// Stages whose battle plan this device computed differently from the
    /// room's: watched without shocks, said quietly.
    mismatched: BTreeSet<StageId>,
    launched: BTreeSet<StageId>,
    /// Stages the seat saw before their song began (gathering, counting
    /// down): handed over by themselves as they start. A stage the seat
    /// first saw playing (it arrived, or came back after a reload) waits
    /// for Watch ([`AudienceClient::watch_now`]), the press that starts
    /// the sound where a browser asks for one.
    seen_before: BTreeSet<StageId>,
    /// Stages asked for with Watch.
    watch_asked: BTreeSet<StageId>,
    /// The music's delay for each stage handed to the shell.
    music: MusicDelays,
    /// Frames taken, sockets opened (the debug state's).
    sockets: u32,
    /// The shell's threads' preparations (desktop's).
    inbox: Option<AudienceInbox>,
    /// The last stage watched's picture (a texture source, its shape),
    /// as its prompt had it: the audience's page's backdrop until the
    /// next stage's replaces it ([`AudienceClient::set_backdrop`]).
    backdrop: Option<(String, f32)>,
}

/// A shell's threads' news since the last call (desktop's share
/// downloads), drained each tick ([`AudienceClient::set_inbox`]): the
/// room client's kind, of which a seat reads its prepared songs.
pub type AudienceInbox = crate::state::room::Inbox;

/// A watched stage's song as prepared, or why not.
pub type LoadResult = Result<(Picked, Option<LivePrep>), LoadProblem>;

pub type SharedAudience = Rc<RefCell<AudienceClient>>;

impl std::fmt::Debug for AudienceClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudienceClient")
            .field("room", &self.room)
            .field("state", &self.state())
            .field("count", &self.count())
            .field("frames", &self.frames())
            .finish()
    }
}

impl AudienceClient {
    /// A seat by `link` (the audience link, or the room's own links where
    /// it offers its audience) for the person signed in on `online`, over
    /// `transport`; `ticket` the one the invite page's join was given
    /// (none: asked for at once).
    pub fn new(
        online: SharedOnline,
        link: &str,
        transport: Box<dyn Transport>,
        ticket: Option<String>,
        now_ms: u64,
    ) -> AudienceClient {
        let room = api::room_of(link).unwrap_or_default().to_string();
        let clock = ManualClock::new(now_ms);
        let mut seat = Audience::new(RoomId::new(room.clone()), transport, clock.clone());
        if let Some(t) = &ticket {
            seat.ticket(t);
        }
        let mut c = AudienceClient {
            online,
            room,
            link: link.into(),
            seat,
            clock,
            ticket_req: None,
            attempt: 0,
            retry_at: 0,
            refused: None,
            version: 0,
            live_news: Default::default(),
            jobs: Vec::new(),
            asked: BTreeMap::new(),
            prepared: BTreeMap::new(),
            problems: BTreeMap::new(),
            mismatched: BTreeSet::new(),
            launched: BTreeSet::new(),
            seen_before: BTreeSet::new(),
            watch_asked: BTreeSet::new(),
            music: MusicDelays::new(AUDIENCE_VIEW_V1),
            sockets: 0,
            inbox: None,
            backdrop: None,
        };
        c.tick(now_ms);
        c
    }

    pub fn online(&self) -> &SharedOnline {
        &self.online
    }

    pub fn room_id(&self) -> &str {
        &self.room
    }

    pub fn link(&self) -> &str {
        &self.link
    }

    /// Bumped by every change a screen shows.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The picture of the stage watched (its prompt's: the song's own or
    /// its generated background's still; a texture source and its
    /// shape), told by the shell as the stage's play shows it: the
    /// audience's page draws it behind its card once the stage is over,
    /// until the next stage's replaces it. None kept: the start menu's
    /// rain (before the first stage of this visit).
    pub fn set_backdrop(&mut self, picture: Option<(String, f32)>) {
        if picture.is_some() && picture != self.backdrop {
            self.backdrop = picture;
            self.version += 1;
        }
    }

    pub fn backdrop(&self) -> Option<&(String, f32)> {
        self.backdrop.as_ref()
    }

    /// The room as its audience is shown it, once said.
    /// The room's last view says the server's operator closed it
    /// (`ddi_room::Closure::ByAdministrator`): "The room was closed."
    pub fn closed_by_operator(&self) -> bool {
        self.view()
            .is_some_and(|v| v.closure == Some(ddi_room::Closure::ByAdministrator))
    }

    pub fn view(&self) -> Option<&PublicView> {
        self.seat.link().view()
    }

    /// How many watch with the audience, as last said.
    pub fn count(&self) -> Option<u32> {
        self.seat.link().count()
    }

    /// Frames taken in order so far (the debug state's).
    pub fn frames(&self) -> u32 {
        self.seat.link().frames()
    }

    /// Sockets opened so far (a relay's restart opens none).
    pub fn sockets(&self) -> u32 {
        self.sockets
    }

    pub fn state(&self) -> SeatState {
        if let Some(end) = self.seat.link().ended() {
            return SeatState::Ended(end);
        }
        if let Some(r) = &self.refused {
            return SeatState::Refused(r.clone());
        }
        if self.seat.link().is_on() {
            SeatState::Watching
        } else if self.sockets > 0 {
            SeatState::Reconnecting
        } else {
            SeatState::Joining
        }
    }

    /// The device leaves: its socket closed, nothing more asked.
    pub fn leave(&mut self) {
        self.seat.leave();
        self.version += 1;
    }

    /// Where a shell's threads answer [`Self::take_load_jobs`].
    pub fn set_inbox(&mut self, inbox: AudienceInbox) {
        self.inbox = Some(inbox);
    }

    /// Time passes (the shell's wall ms): answers read, a ticket asked for
    /// where the socket was lost, the socket read and written.
    pub fn tick(&mut self, now_ms: u64) {
        if let Some(inbox) = self.inbox.as_mut() {
            for news in inbox() {
                if let crate::state::room::ShellNews::PreparedShared { job, result } = news {
                    self.finish_found(&job, result.map(|f| *f));
                }
            }
        }
        self.clock.set(now_ms.max(self.clock.now().0));
        self.online.borrow_mut().poll(now_ms);
        let answer = self
            .ticket_req
            .and_then(|id| self.online.borrow_mut().take_answer(id));
        if let Some(r) = answer {
            self.ticket_req = None;
            match api::read_current::<AudienceTicketDoc>(&r) {
                Ok(t) => {
                    self.seat.ticket(&t.ticket);
                }
                // The audience join needs no session (2026-11-15): only a
                // server from before asks for one.
                Err(p) if p.signed_out() => {
                    self.refused = Some(Refused::Other(ddi_room::SERVER_BEHIND.into()));
                }
                // A relay not ready, or too many joins from this network
                // just now (`audience_busy`): asked again after a wait.
                Err(p) if p.transient() => {
                    self.attempt += 1;
                    self.retry_at =
                        now_ms + ReconnectPolicy::RECONNECT_BACKOFF_V0.wait_ms(self.attempt);
                }
                Err(p) => {
                    let signed_in = self.online.borrow().me().is_some();
                    self.refused = Some(Refused::of(&api::worded_for(p, signed_in)));
                }
            }
            self.version += 1;
        }
        if self.seat.wants_ticket()
            && self.ticket_req.is_none()
            && self.refused.is_none()
            && now_ms >= self.retry_at
        {
            self.ticket_req = Some(
                self.online
                    .borrow_mut()
                    .request(api::audience_join(&self.room, &self.link)),
            );
        }
        let was_on = self.seat.link().is_on();
        let wanted = self.seat.wants_ticket();
        let news = self.seat.poll();
        let on = self.seat.link().is_on();
        // A socket closed before it ever opened (a relay down, the upgrade
        // refused): the next ticket after a wait too, as for a lost one.
        if !was_on
            && !on
            && !wanted
            && self.seat.wants_ticket()
            && self.seat.link().ended().is_none()
        {
            self.attempt += 1;
            self.retry_at = now_ms + ReconnectPolicy::RECONNECT_BACKOFF_V0.wait_ms(self.attempt);
        }
        if on != was_on {
            if on {
                self.sockets += 1;
                self.attempt = 0;
            } else if self.seat.link().ended().is_none() {
                // Lost: a new ticket after a wait (a refused one says why).
                self.attempt += 1;
                self.retry_at = now_ms + ReconnectPolicy::RECONNECT_BACKOFF_V0.wait_ms(self.attempt);
            }
            self.version += 1;
        }
        for n in news {
            match n {
                AudienceNews::Frame(f) => self.live_news.extend([LiveNews::Frame(f)]),
                AudienceNews::Snapshot(s) => self.live_news.extend([LiveNews::Snapshot(s)]),
                AudienceNews::View(_) | AudienceNews::Count(_) | AudienceNews::Ended(_) => {
                    self.version += 1;
                }
            }
        }
        self.sync_loading();
        self.note_seen();
    }

    /// Notes the stage the room gathers or counts down for: seen before
    /// its song began.
    fn note_seen(&mut self) {
        if let Some(v) = self.view()
            && let Phase::Preparing { stage, .. } | Phase::Countdown { stage, .. } = v.phase
        {
            self.seen_before.insert(stage);
        }
    }

    /// The stage being played now that the seat can follow but is not
    /// watching (Watch on the audience's page): its song begun, prepared
    /// here, the room playing live, the seat not ended, and not handed
    /// over now (or handed over and left: the browser's Back, a reload).
    pub fn watchable(&self) -> Option<StageId> {
        if self.seat.link().ended().is_some() || !self.plays_live() {
            return None;
        }
        let v = self.view()?;
        let stage = match v.phase {
            Phase::Stage { stage } | Phase::SongCompletion { stage } => stage,
            _ => return None,
        };
        let st = v.stages.iter().find(|s| s.id == stage)?;
        st.play.as_ref()?.song_zero_at?;
        self.prepared.contains_key(&stage).then_some(stage)
    }

    /// Watch pressed: the stage being played, handed over as it stands
    /// (its order, joining part-way), whether it was handed over before
    /// or never (a seat that arrived during it).
    pub fn watch_now(&mut self) -> Option<StageOrder> {
        let stage = self.watchable()?;
        self.launched.remove(&stage);
        self.watch_asked.insert(stage);
        self.take_stage_order()
    }

    /// The stage the room plays (or prepares) now, as the view says.
    pub fn current_stage(&self) -> Option<StageId> {
        match self.view()?.phase {
            Phase::Preparing { stage, .. }
            | Phase::Countdown { stage, .. }
            | Phase::Stage { stage }
            | Phase::SongCompletion { stage } => Some(stage),
            _ => None,
        }
    }

    /// The room plays its stages live: the audience is sent their frames.
    pub fn plays_live(&self) -> bool {
        self.view()
            .and_then(|v| v.live.as_deref())
            .and_then(LiveRules::named)
            .is_some()
    }

    /// Attacks are played in the view's match (its rule On, the room
    /// playing live).
    pub fn attacks(&self) -> bool {
        self.attacks_of(self.current_stage())
    }

    /// The stage's rules (everyone's): the stage the room is on.
    pub fn stage_rules(&self) -> StageRules {
        self.stage_rules_of(self.current_stage())
    }

    /// Stage `stage`'s rules: the match's, or those the public view says
    /// it is played under where they were changed during the match.
    pub fn stage_rules_of(&self, stage: Option<StageId>) -> StageRules {
        self.view()
            .map(|v| StageRules::of_public(v, stage))
            .unwrap_or_default()
    }

    /// Attacks are played in stage `stage` (the room playing live).
    pub fn attacks_of(&self, stage: Option<StageId>) -> bool {
        self.view().is_some_and(|v| {
            self.plays_live() && crate::state::room::public_rules_of(v, stage).0.attacks
        })
    }

    /// A player's name as the view shows it (a CPU's from its number).
    pub fn player_name(&self, id: ParticipantId) -> String {
        self.view()
            .and_then(|v| v.players.iter().find(|p| p.id == id))
            .map(|p| match (&p.name, &p.kind) {
                (Some(n), _) => n.clone(),
                (None, PublicKind::Cpu { ordinal, .. }) => {
                    ddi_l10n::msg::room::cpu_name(*ordinal).to_string()
                }
                (None, _) => String::new(),
            })
            .unwrap_or_default()
    }

    /// The watched stage's song, prepared from the room's share.
    fn sync_loading(&mut self) {
        let Some(stage) = self.current_stage() else {
            return;
        };
        if !self.plays_live() {
            return;
        }
        let Some(v) = self.view() else {
            return;
        };
        let Some(st) = v.stages.iter().find(|s| s.id == stage) else {
            return;
        };
        let (Some(share), Some(chart)) = (st.share.clone(), st.chart.clone()) else {
            return;
        };
        if st.content != PublicContent::Available
            || self.asked.get(&stage) == Some(&(share.clone(), chart.clone()))
        {
            return;
        }
        // Another song for the stage (picked again): its own preparation.
        self.prepared.remove(&stage);
        self.problems.remove(&stage);
        let settings = crate::state::settings::Settings::load();
        let live = LiveAsk {
            seed: ddi_room::live::plan::seed_for(&RoomId::new(self.room.clone()), stage, &chart),
            rules: self.stage_rules_of(Some(stage)),
            attacks: self.attacks_of(Some(stage)),
            options: settings.play_options(None),
            speed_source: settings.speed_source,
        };
        self.asked.insert(stage, (share.clone(), chart.clone()));
        self.jobs.push(LoadJob::from_share(stage, 0, share, &chart, Some(live)));
        self.version += 1;
    }

    /// Songs to prepare from the room's share (the shell answers
    /// [`Self::finish_load`]).
    pub fn take_load_jobs(&mut self) -> Vec<LoadJob> {
        std::mem::take(&mut self.jobs)
    }

    /// The shell prepared `job`: the song read from the share with what
    /// live play needs, or why not.
    pub fn finish_load(&mut self, job: &LoadJob, result: LoadResult) {
        // The song the stage has now, or a late answer for one it had.
        let current = self.asked.get(&job.stage).is_some_and(|(share, chart)| {
            job.share.as_ref() == Some(share) && job.digest == chart.as_str().parse().ok()
        });
        if !current {
            return;
        }
        match result {
            Ok((picked, prep)) => {
                self.prepared.insert(job.stage, Prepared { picked, prep });
                self.problems.remove(&job.stage);
            }
            Err(p) => {
                self.problems.insert(job.stage, p);
            }
        }
        self.version += 1;
    }

    /// [`Self::finish_load`] from the song as the shell read it from the
    /// room's share (`crate::state::room::SharedFound`).
    pub fn finish_found(
        &mut self,
        job: &LoadJob,
        result: Result<crate::state::room::SharedFound, LoadProblem>,
    ) {
        self.finish_load(job, result.and_then(crate::state::room::SharedFound::picked));
    }

    /// Why the stage's song could not be prepared, if it could not.
    pub fn load_problem(&self, stage: StageId) -> Option<LoadProblem> {
        self.problems.get(&stage).copied()
    }

    /// This device's battle plan of `stage` differs from the room's: its
    /// fields are drawn without shocks (said quietly).
    pub fn plan_mismatch(&self, stage: StageId) -> bool {
        self.mismatched.contains(&stage)
    }

    /// The music's delay behind the room's song time for `stage` (ms):
    /// fixed once its order is handed to the shell.
    pub fn music_delay_ms(&self, stage: StageId) -> u64 {
        self.music
            .of(stage)
            .unwrap_or(self.music.next())
            .min(self.ceiling_ms())
    }

    /// The music delay the next stage handed over will have (ms).
    pub fn next_music_delay_ms(&self) -> u64 {
        self.music.next().min(self.ceiling_ms())
    }

    /// The view's rules (its id for the debug state).
    pub fn view_rules(&self) -> AudienceView {
        self.music.rules
    }

    fn ceiling_ms(&self) -> u64 {
        AUDIENCE_V0.presentation_delay_ms
    }

    /// A stage to watch now, once: the room started it (its song zero is
    /// committed), its song is prepared and the room's clock is known
    /// (from the frames' times). Its competitors are the stage's lineup;
    /// its shocks are drawn from this device's battle plan only where its
    /// digest is the room's.
    pub fn take_stage_order(&mut self) -> Option<StageOrder> {
        if self.seat.link().ended().is_some() {
            return None;
        }
        self.note_seen();
        let stage = self.current_stage()?;
        if self.launched.contains(&stage) || !self.plays_live() {
            return None;
        }
        // Begun before the seat saw it: Watch hands it over.
        if !self.seen_before.contains(&stage) && !self.watch_asked.contains(&stage) {
            return None;
        }
        let now = self.clock.now().0;
        self.seat.link().room_ms(now)?;
        let v = self.view()?.clone();
        let (index, st) = v.stages.iter().enumerate().find(|(_, s)| s.id == stage)?;
        let play = st.play.as_ref()?;
        play.song_zero_at?;
        let prepared = self.prepared.get(&stage)?.clone();
        let attacks = self.attacks();
        let room_plan = st.battle.clone();
        let mut prep = prepared.prep.clone();
        if let Some(p) = prep.as_mut()
            && let Some(plan) = &p.plan
            && room_plan.as_ref() != Some(&plan.digest)
        {
            // Never a wrong picture: no shocks drawn.
            p.plan = None;
            self.mismatched.insert(stage);
        }
        // The CPUs computed here as every member's device computes them
        // (`ddi_room::live::cpus`); the people drawn from their streams.
        let cpu_of = |id: &ParticipantId| {
            v.players.iter().find(|p| p.id == *id).and_then(|p| match &p.kind {
                PublicKind::Cpu {
                    level,
                    ordinal,
                    seed,
                    personality,
                    targeting,
                } => Some(CpuSeat::for_stage(
                    *id,
                    *ordinal,
                    *level,
                    *seed,
                    index + 1,
                    p.name.clone(),
                    personality.clone(),
                    *targeting,
                )),
                _ => None,
            })
        };
        let cpus: Vec<CpuSeat> = play.lineup.iter().filter_map(cpu_of).collect();
        let people: Vec<(ParticipantId, String)> = play
            .lineup
            .iter()
            .filter(|id| cpu_of(id).is_none())
            .map(|id| (*id, self.player_name(*id)))
            .collect();
        let seed = st.chart.as_ref().map(|c| {
            ddi_room::live::plan::seed_for(&RoomId::new(self.room.clone()), stage, c)
        });
        let order = StageOrder {
            stage,
            number: u16::try_from(index + 1).unwrap_or(u16::MAX),
            of: u16::try_from(v.stages.len()).unwrap_or(u16::MAX),
            song: prepared.picked.song,
            chart: prepared.picked.chart,
            battle: ddi_room::BattlePreset::with_attacks(attacks),
            attacks_offered: true,
            adaptive_offered: true,
            rules: self.stage_rules_of(Some(stage)),
            me: AUDIENCE_ME,
            targeting: Default::default(),
            cpus,
            dev_mines_at_beat: None,
            others: u16::try_from(people.len()).unwrap_or(u16::MAX),
            watching: true,
            live: Some(LiveOrder {
                prep,
                people,
                seed,
            }),
        };
        self.launched.insert(stage);
        // The music's delay for it, from what the stage watched last
        // needed (a stage watched again keeps its own).
        self.music.fix(stage);
        self.version += 1;
        Some(order)
    }

    /// The banner of stage `stage`'s song (a texture source), once this
    /// seat read the room's share of it ([`Self::finish_found`]: the
    /// share's listing names it, as a member's client finds a shared
    /// song's); none before, and for a song without one.
    pub fn banner(&self, stage: StageId) -> Option<&str> {
        self.prepared.get(&stage)?.picked.song.banner.as_deref()
    }

    /// The last stage of the view's match the room settled (its result
    /// confirmed), if any.
    pub fn settled_stage(&self) -> Option<StageId> {
        self.view()?
            .stages
            .iter()
            .rev()
            .find(|s| s.play.as_ref().is_some_and(|p| p.result.is_some()))
            .map(|s| s.id)
    }

    /// Stage `stage`'s standings as the room's members' results show
    /// them ([`StageTable`], the same builder: `TableSource`), from the
    /// public view: every starter with the place, finish and award the
    /// room confirmed, how they performed (the details the starters
    /// reported), the match's totals and places after it, and the match's
    /// result where it was its last. What the public view has not: a
    /// starter's raw run before the room confirmed the stage (their line
    /// waits for the stage's result), the viewer's own attacks' summary
    /// (the audience plays none), and matches before the current one.
    pub fn stage_table(&self, stage: StageId) -> Option<StageTable> {
        let v = self.view()?;
        let index = v.stages.iter().position(|s| s.id == stage)?;
        let play = v.stages[index].play.as_ref()?;
        Some(
            TableSource {
                stage,
                index,
                of: v.stages.len(),
                roster: v.standings.iter().map(|st| st.participant).collect(),
                results: v.stages[..=index]
                    .iter()
                    .map(|s| s.play.as_ref().and_then(|p| p.result.as_ref()))
                    .collect(),
                lineup: play.lineup.clone(),
                detail: &|id| {
                    play.details
                        .iter()
                        .find(|(who, _)| *who == id)
                        .map(|(_, d)| d.clone())
                },
                run: &|_| None,
                completed: v.phase == Phase::MatchResults,
                standings: &|| v.standings.clone(),
                rules: StageRules::of_public(v, Some(stage)),
                attacks: self.attacks_of(Some(stage)),
                adaptive_offered: true,
                summary: None,
            }
            .table(),
        )
    }

    /// Whether `stage` was handed to the shell to watch.
    pub fn launched(&self, stage: StageId) -> bool {
        self.launched.contains(&stage)
    }

    /// The stage's confirmed ranking, from the view.
    pub fn stage_ranking(&self, stage: StageId) -> Option<Vec<Vec<Finisher>>> {
        let v = self.view()?;
        let r = v.stages.iter().find(|s| s.id == stage)?.play.as_ref()?.result.as_ref()?;
        Some(ranking_of(&r.placements))
    }
}

/// Who an audience device is in a watched stage's setup: nobody's seat.
pub const AUDIENCE_ME: ParticipantId = ParticipantId(0);

impl LiveRoom for AudienceClient {
    fn live_rules(&self) -> LiveRules {
        self.view()
            .and_then(|v| v.live.as_deref())
            .and_then(LiveRules::named)
            .unwrap_or(LIVE_V1)
    }

    /// What is left of the audience's ceiling once the music is moved
    /// behind the room's song time: from nothing, as the frames measure.
    fn view_delay(&self, _rules: &LiveRules) -> ViewDelay {
        let music = self.music.next().min(self.ceiling_ms());
        behind_music("audience-view-delay-v0", (self.ceiling_ms() - music) as f64)
    }

    fn room_ms_at(&self, wall_ms: u64) -> Option<Millis> {
        self.seat.link().room_ms(wall_ms)
    }

    fn clock_uncertainty_us(&self) -> i64 {
        // No pings: the quickest trip down is the error, tens of ms, which
        // the music's delay covers.
        50_000
    }

    fn song_zero_at(&self, stage: StageId) -> Option<Millis> {
        let v = self.view()?;
        let zero = v
            .stages
            .iter()
            .find(|s| s.id == stage)?
            .play
            .as_ref()?
            .song_zero_at?;
        Some(Millis(zero.0 + self.music_delay_ms(stage)))
    }

    fn now_ms(&self) -> u64 {
        self.clock.now().0
    }

    fn tick(&mut self, now_ms: u64) {
        AudienceClient::tick(self, now_ms);
    }

    fn take_live_news(&mut self) -> Vec<LiveNews> {
        self.live_news.take()
    }

    fn stop_live_news(&mut self) {
        self.live_news.stop();
    }

    fn live_news_whole(&self, stage: StageId) -> bool {
        self.live_news.whole(stage)
    }

    fn read_live_again(&mut self, stage: StageId) {
        self.live_news.read_again(stage);
        self.seat.link_mut().read_again(stage);
    }

    fn live_stream(&mut self, _stage: StageId) -> Option<&mut ddi_room::live::Outbox> {
        None
    }

    fn forget_live(&mut self, _stage: StageId) {}

    fn decision(&self, stage: StageId) -> Option<(Vec<Vec<Finisher>>, bool)> {
        let v = self.view()?;
        let r = v.stages.iter().find(|s| s.id == stage)?.play.as_ref()?.result.as_ref()?;
        Some(crate::state::room::decision_of(&v.phase, stage, &r.placements))
    }

    fn live_prep(&self, stage: StageId) -> Option<LivePrep> {
        let mut prep = self.prepared.get(&stage)?.prep.clone()?;
        if self.mismatched.contains(&stage) {
            prep.plan = None;
        }
        Some(prep)
    }

    fn seat_end(&self) -> Option<SeatEnd> {
        seat_end_of(&self.state()).map(|e| match e {
            SeatEnd::RoomClosed if self.closed_by_operator() => SeatEnd::ClosedByOperator,
            e => e,
        })
    }

    /// The fields' own delay beyond the music this frame: kept for the
    /// next stage's music delay.
    fn note_view_delay(&mut self, stage: StageId, delay_ms: f64) {
        self.music.note(stage, delay_ms);
    }
}

/// Why a seat in `state` ended for a stage watched, if it did: ended by
/// its relay, or a new ticket refused (or the session gone) after its
/// socket was lost. Joining and reconnecting are not ends.
pub fn seat_end_of(state: &SeatState) -> Option<SeatEnd> {
    Some(match state {
        SeatState::Joining | SeatState::Watching | SeatState::Reconnecting => return None,
        SeatState::Ended(AudienceEnd::Closed)
        | SeatState::Refused(Refused::Audience(AudienceRefusal::Closed)) => SeatEnd::AudienceClosed,
        SeatState::Ended(AudienceEnd::RoomClosed)
        | SeatState::Refused(Refused::Audience(AudienceRefusal::RoomClosed)) => SeatEnd::RoomClosed,
        SeatState::Refused(Refused::Audience(AudienceRefusal::BadLink)) => SeatEnd::LinkReplaced,
        SeatState::Refused(Refused::Audience(r)) => SeatEnd::Refused(r.code().into()),
        SeatState::Refused(Refused::Other(code)) => SeatEnd::Refused(code.clone()),
    })
}

/// The view's chart key as a digest's text (for tests and logs).
pub fn chart_of(v: &PublicView, stage: StageId) -> Option<ChartKey> {
    v.stages.iter().find(|s| s.id == stage)?.chart.clone()
}

#[cfg(test)]
pub(crate) mod tests;
