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
//! song time ([`MUSIC_DELAY_V0`], within the audience's ceiling).
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
    BattlePreset, ChartKey, Clock, Finisher, ManualClock, Millis, ParticipantId, Phase, ReconnectPolicy,
    RoomId, StageId, Transport,
};

use super::online::SharedOnline;
use super::online::api::{self, AudienceTicketDoc, Problem};
use super::room::{
    LiveAsk, LiveOrder, LivePrep, LiveRoom, LoadJob, LoadProblem, Picked, StageOrder, StageRules,
    ranking_of,
};

/// How far behind the room's song time an audience device plays a
/// stage's music and draws its fields, as the stage starts: within the
/// audience's ceiling (`AudienceRules::presentation_delay_ms`, two
/// seconds), above what the fake room measured a moment to take to a seat
/// (0.37 s at the median, 0.66 s at most, at 20 to 120 ms each way).
/// Fixed for a stage (the music cannot move); the fields fall a little
/// further behind it where a frame comes later still (their own delay
/// from what the frames measure, `ddi_room::live::ViewDelay`).
/// PROVISIONAL (`audience-view-v0`).
pub const MUSIC_DELAY_V0: AudienceView = AudienceView {
    id: "audience-view-v0",
    music_delay_ms: 800,
};

/// The audience's view's numbers, named and versioned. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudienceView {
    pub id: &'static str,
    pub music_delay_ms: u64,
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
    /// The session ended: sign in again.
    SignInAgain,
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
        [
            AudienceRefusal::Full,
            AudienceRefusal::Closed,
            AudienceRefusal::RoomClosed,
            AudienceRefusal::TooManySeats,
            AudienceRefusal::BadLink,
            AudienceRefusal::NoAudience,
        ]
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
    view_rules: AudienceView,
    ticket_req: Option<u64>,
    attempt: u32,
    retry_at: u64,
    refused: Option<Refused>,
    signed_out: bool,
    version: u64,
    live_news: Vec<LiveNews>,
    live_reader: bool,
    jobs: Vec<LoadJob>,
    asked: BTreeSet<StageId>,
    prepared: BTreeMap<StageId, Prepared>,
    problems: BTreeMap<StageId, LoadProblem>,
    /// Stages whose battle plan this device computed differently from the
    /// room's: watched without shocks, said quietly.
    mismatched: BTreeSet<StageId>,
    launched: BTreeSet<StageId>,
    /// The music's delay for each stage handed to the shell.
    delays: BTreeMap<StageId, u64>,
    /// Frames taken, sockets opened (the debug state's).
    sockets: u32,
    /// The shell's threads' preparations (desktop's).
    inbox: Option<AudienceInbox>,
}

/// A shell's threads' prepared songs since the last call (desktop's
/// share downloads), drained each tick ([`AudienceClient::set_inbox`]).
pub type AudienceInbox = Box<dyn FnMut() -> Vec<(LoadJob, LoadResult)>>;

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
            view_rules: MUSIC_DELAY_V0,
            ticket_req: None,
            attempt: 0,
            retry_at: 0,
            refused: None,
            signed_out: false,
            version: 0,
            live_news: Vec::new(),
            live_reader: false,
            jobs: Vec::new(),
            asked: BTreeSet::new(),
            prepared: BTreeMap::new(),
            problems: BTreeMap::new(),
            mismatched: BTreeSet::new(),
            launched: BTreeSet::new(),
            delays: BTreeMap::new(),
            sockets: 0,
            inbox: None,
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

    /// The room as its audience is shown it, once said.
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
        if self.signed_out {
            return SeatState::SignInAgain;
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

    /// Sign in again (the session ended): the page returns to this link.
    pub fn sign_in_again(&mut self) {
        self.online
            .borrow_mut()
            .sign_in(super::online::Dest::Audience(self.link.clone()));
    }

    /// Where a shell's threads answer [`Self::take_load_jobs`].
    pub fn set_inbox(&mut self, inbox: AudienceInbox) {
        self.inbox = Some(inbox);
    }

    /// Time passes (the shell's wall ms): answers read, a ticket asked for
    /// where the socket was lost, the socket read and written.
    pub fn tick(&mut self, now_ms: u64) {
        if let Some(inbox) = self.inbox.as_mut() {
            for (job, result) in inbox() {
                self.finish_load(&job, result);
            }
        }
        self.clock.set(now_ms.max(self.clock.now().0));
        self.online.borrow_mut().poll(now_ms);
        if self.signed_out && self.online.borrow().me().is_some() {
            // Signed in again: asked anew.
            self.signed_out = false;
            self.retry_at = 0;
            self.version += 1;
        }
        if let Some(id) = self.ticket_req
            && let Some(r) = self.online.borrow_mut().take_answer(id)
        {
            self.ticket_req = None;
            match api::read::<AudienceTicketDoc>(&r) {
                Ok(t) => {
                    self.seat.ticket(&t.ticket);
                }
                Err(p) if p.signed_out() => self.signed_out = true,
                Err(p) if p.status == 0 || p.status >= 500 => {
                    self.attempt += 1;
                    self.retry_at =
                        now_ms + ReconnectPolicy::RECONNECT_BACKOFF_V0.wait_ms(self.attempt);
                }
                Err(p) => self.refused = Some(Refused::of(&p)),
            }
            self.version += 1;
        }
        if self.seat.wants_ticket()
            && self.ticket_req.is_none()
            && self.refused.is_none()
            && !self.signed_out
            && now_ms >= self.retry_at
        {
            self.ticket_req = Some(
                self.online
                    .borrow_mut()
                    .request(api::audience_join(&self.room, &self.link)),
            );
        }
        let was_on = self.seat.link().is_on();
        let news = self.seat.poll();
        let on = self.seat.link().is_on();
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
                AudienceNews::Frame(f) => {
                    if self.live_reader {
                        self.live_news.push(LiveNews::Frame(f));
                    }
                }
                AudienceNews::Snapshot(s) => {
                    if self.live_reader {
                        self.live_news.push(LiveNews::Snapshot(s));
                    }
                }
                AudienceNews::View(_) | AudienceNews::Count(_) | AudienceNews::Ended(_) => {
                    self.version += 1;
                }
            }
        }
        self.sync_loading();
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
        self.plays_live() && self.view().is_some_and(|v| v.rules.attacks)
    }

    /// The stage's rules (everyone's).
    pub fn stage_rules(&self) -> StageRules {
        self.view().map_or_else(StageRules::default, |v| StageRules {
            ruleset: v.rules.ruleset.clone(),
            difficulty: v.rules.difficulty,
            boos: v.rules.boos,
            points: v.rules.points,
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
        if !self.plays_live() || self.asked.contains(&stage) {
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
        if st.content != PublicContent::Available {
            return;
        }
        let settings = crate::state::settings::Settings::load();
        let live = LiveAsk {
            seed: ddi_room::live::plan::seed_for(&RoomId::new(self.room.clone()), stage, &chart),
            rules: self.stage_rules(),
            attacks: self.attacks(),
            options: settings.play_options(None),
            speed_source: settings.speed_source,
        };
        self.asked.insert(stage);
        self.jobs.push(LoadJob {
            stage,
            readiness_rev: 0,
            song: ddi_library::share::song_id(&share, 0),
            chart: 0,
            digest: chart.as_str().parse().ok(),
            share: Some(share),
            cpus: false,
            live: Some(live),
        });
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
        self.delays
            .get(&stage)
            .copied()
            .unwrap_or(self.view_rules.music_delay_ms)
            .min(self.ceiling_ms())
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
        let stage = self.current_stage()?;
        if self.launched.contains(&stage) || !self.plays_live() {
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
        let people: Vec<(ParticipantId, String)> = play
            .lineup
            .iter()
            .map(|id| (*id, self.player_name(*id)))
            .collect();
        let order = StageOrder {
            stage,
            number: u16::try_from(index + 1).unwrap_or(u16::MAX),
            of: u16::try_from(v.stages.len()).unwrap_or(u16::MAX),
            song: prepared.picked.song,
            chart: prepared.picked.chart,
            battle: if attacks {
                BattlePreset::GaugeAttacksV0
            } else {
                BattlePreset::GaugeSurvivalV0
            },
            attacks_offered: true,
            rules: self.stage_rules(),
            me: AUDIENCE_ME,
            targeting: Default::default(),
            cpus: Vec::new(),
            dev_mines_at_beat: None,
            others: u16::try_from(people.len()).unwrap_or(u16::MAX),
            watching: true,
            live: Some(LiveOrder { prep, people }),
        };
        self.launched.insert(stage);
        self.delays.insert(stage, self.view_rules.music_delay_ms);
        self.version += 1;
        Some(order)
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
        let rules = ViewDelayRules {
            id: "audience-view-delay-v0",
            floor_ms: 0,
            ..VIEW_DELAY_V0
        };
        let music = self.view_rules.music_delay_ms.min(self.ceiling_ms());
        ViewDelay::new(rules, (self.ceiling_ms() - music) as f64, 0.0)
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

    fn song_zero_in(&self, stage: StageId) -> Option<f64> {
        let zero = LiveRoom::song_zero_at(self, stage)?;
        let now = self.seat.link().room_ms(self.clock.now().0)?;
        Some((zero.0 as f64 - now.0 as f64) / 1_000.0)
    }

    fn tick(&mut self, now_ms: u64) {
        AudienceClient::tick(self, now_ms);
    }

    fn take_live_news(&mut self) -> Vec<LiveNews> {
        self.live_reader = true;
        std::mem::take(&mut self.live_news)
    }

    fn stop_live_news(&mut self) {
        self.live_reader = false;
        self.live_news.clear();
    }

    fn live_stream(&mut self, _stage: StageId) -> Option<&mut ddi_room::live::Outbox> {
        None
    }

    fn forget_live(&mut self, _stage: StageId) {}

    fn decision(&self, stage: StageId) -> Option<(Vec<Vec<Finisher>>, bool)> {
        let ranking = self.stage_ranking(stage)?;
        let early = matches!(
            self.view()?.phase,
            Phase::SongCompletion { stage: s } if s == stage
        );
        Some((ranking, early))
    }

    fn live_prep(&self, stage: StageId) -> Option<LivePrep> {
        let mut prep = self.prepared.get(&stage)?.prep.clone()?;
        if self.mismatched.contains(&stage) {
            prep.plan = None;
        }
        Some(prep)
    }
}

/// The view's chart key as a digest's text (for tests and logs).
pub fn chart_of(v: &PublicView, stage: StageId) -> Option<ChartKey> {
    v.stages.iter().find(|s| s.id == stage)?.chart.clone()
}

#[cfg(test)]
pub(crate) mod tests;
