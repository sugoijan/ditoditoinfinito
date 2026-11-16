//! TESTING ONLY: a room watched with its audience without a server, for
//! the headless browsers (`docs/plans/multiplayer-prototype-progress.md`,
//! "The web watches an audience's stage"). Asked for in the address the
//! page loads on (`#/?mpfakeseat=1`, any route, and
//! `mpfakeend=S` to close the audience S seconds into the stage), never
//! from a menu: the next audience seat (`crate::online::audience`) is then
//! a seat in the fake room's audience (`ddi_room::fake`, in the page),
//! whose three members stream a bundled song's chart as scripted hits
//! (each missing every few notes), on the wall clock, pumped twenty times
//! a second. `mpfakematch=STAGES,CUT,WAIT` (each optional) makes it a
//! match of STAGES stages (one otherwise), each cut to its first CUT
//! seconds (the whole song otherwise), the first picked WAIT seconds
//! after the members connected (at once otherwise); the members then
//! say how they performed, so the stages' Standings have their lines,
//! and the owner picks again after each stage: the audience's page
//! before a stage, between stages and after the match (round D3). The viewer is signed out (no session, as a fresh browser):
//! the audience link's page (`#/mp/audience/<link>`) reads the fake room's
//! preview and its Watch joins the audience anonymously, as the server
//! admits a join without a session. The seat, its pages, the stage handed
//! over, the play route and the spectator view are the real ones; only
//! the server is not.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_room::fake::{FakeAudienceSocket, FakeServer, FakeSocket, SharedFake};
use ddi_room::live::{Moment, MomentKind, StreamEnd};
use ddi_room::{
    ClientId, Clock, Command, ContentStatus, Identity, Intent, ManualClock, RoomPolicy, StageId,
};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::audience::AudienceClient;
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Http, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::room::{LoadJob, Picked, RoomClient, SharedRoom, prepare_stage};

/// The bundled song the fake room plays.
pub(crate) const SONG: &str = "beatucada";
pub(crate) const ROOM: &str = "Room000001";
const MEMBERS: [&str; 3] = ["alice", "bob", "cody"];

thread_local! {
    /// Asked for (`mpfakeseat=1`), and when to close the audience.
    static WANTED: Cell<bool> = const { Cell::new(false) };
    static END_AFTER: Cell<Option<f64>> = const { Cell::new(None) };
    /// The match asked for (`mpfakematch`).
    static MATCH: Cell<MatchPlan> = const {
        Cell::new(MatchPlan {
            stages: 1,
            cut: None,
            wait: 0.0,
        })
    };
    static FAKE: RefCell<Option<Rc<RefCell<Fake>>>> = const { RefCell::new(None) };
    /// The viewer's online state over the fake room (signed out).
    static VIEWER: RefCell<Option<SharedOnline>> = const { RefCell::new(None) };
}

/// The fake room's match (`mpfakematch=STAGES,CUT,WAIT`): its stages,
/// the seconds each stage's song is cut to (none: the whole song) and the
/// seconds before the first pick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MatchPlan {
    pub stages: u16,
    pub cut: Option<f64>,
    pub wait: f64,
}

impl MatchPlan {
    /// `STAGES,CUT,WAIT` as the address has it (parts left out or
    /// unreadable: one stage, the whole song, at once).
    pub(crate) fn parse(text: &str) -> MatchPlan {
        let mut parts = text.split(',').map(|p| p.trim().parse::<f64>().ok());
        let mut next = || parts.next().flatten().filter(|v| v.is_finite() && *v > 0.0);
        MatchPlan {
            stages: next().map_or(1, |v| v.clamp(1.0, 8.0) as u16),
            cut: next(),
            wait: next().unwrap_or(0.0),
        }
    }
}

/// The next audience seat is the fake room's (`end_after`: the audience
/// closed that many seconds into the stage; `plan`: its match).
pub(crate) fn want(end_after: Option<f64>, plan: Option<MatchPlan>) {
    WANTED.set(true);
    END_AFTER.set(end_after);
    if let Some(plan) = plan {
        MATCH.set(plan);
    }
}

/// Whether the audience is the fake room's: asked for, and only on an
/// audience link's pages (its invite page, the audience's page, a stage
/// watched; review3 web-8): the rest of the session (the multiplayer
/// home, invites, the person's own rooms) stays the real one's.
pub(crate) fn on() -> bool {
    let hash = || {
        web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default()
    };
    WANTED.get()
        && matches!(
            crate::router::Route::parse(&hash()),
            crate::router::Route::MpAudience { .. }
                | crate::router::Route::MpWatch { .. }
                | crate::router::Route::MpWatchStage { .. }
        )
}

/// The rooms' steady clock (`crate::room::now_ms`).
fn now_ms() -> u64 {
    crate::room::now_ms()
}

/// The fake room and its pump.
struct Fake {
    clock: ManualClock,
    server: SharedFake,
    rooms: Vec<SharedRoom>,
    song: Option<Rc<ddi_chart::Song>>,
    chart: usize,
    step: Step,
    scripts: Vec<Script>,
    closed: bool,
    /// When every member was first seen connected (ms).
    connected_at: Option<u64>,
    /// When the stage played last was first seen settled (ms).
    settled_at: Option<u64>,
    _pump: Option<gloo::timers::callback::Interval>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Connecting,
    Picked,
    Readied(StageId),
    Playing(StageId, ddi_room::Millis),
    /// The stage's streams ended and the members said how they performed:
    /// the room settles it, then the owner picks the next one.
    Played(StageId),
    /// The match is over.
    Done,
}

/// HTTP answered from the fake room: who `who` is (none: the viewer,
/// signed out), the room's summary, its tickets and grants; the invite's
/// preview and the audience join as the server answers them (the viewer's
/// anonymous).
struct FakeHttp {
    server: SharedFake,
    who: Option<&'static str>,
    out: Vec<(u64, ApiResponse)>,
}

fn answer(status: u16, body: String) -> ApiResponse {
    ApiResponse {
        status,
        body,
        session: None,
    }
}

impl Http for FakeHttp {
    fn start(&mut self, id: u64, req: ApiRequest) {
        let Some(who) = self.who else {
            // The viewer: no session.
            let a = match (req.method, req.path.as_str()) {
                (Method::Get, p) if p.starts_with("/invites/") => {
                    match self.server.borrow().invite_preview(&p["/invites/".len()..]) {
                        Ok(b) => answer(200, b),
                        Err((s, code)) => answer(s, format!(r#"{{"error":"{code}"}}"#)),
                    }
                }
                (Method::Post, p) if p == format!("/rooms/{ROOM}/audience") => {
                    let body: serde_json::Value =
                        serde_json::from_str(req.body.as_deref().unwrap_or("{}"))
                            .unwrap_or_default();
                    let (s, b) = self.server.borrow_mut().audience_join_answer(
                        None,
                        body["invite"].as_str().unwrap_or_default(),
                        None,
                    );
                    answer(s, b)
                }
                _ => answer(401, r#"{"error":"signed_out"}"#.into()),
            };
            self.out.push((id, a));
            return;
        };
        let identity = Identity::new(format!("dev:{who}"));
        let path = format!("/rooms/{ROOM}");
        let a = match (req.method, req.path.as_str()) {
            (Method::Get, "/me") => answer(
                200,
                format!(
                    r#"{{"player_id":"dev:{who}","login":"{who}","display_name":"{who}","admin":false,"expires_at_ms":1}}"#
                ),
            ),
            (Method::Get, p) if p.starts_with("/invites/") => {
                match self.server.borrow().invite_preview(&p["/invites/".len()..]) {
                    Ok(b) => answer(200, b),
                    Err((s, code)) => answer(s, format!(r#"{{"error":"{code}"}}"#)),
                }
            }
            (Method::Post, p) if p == format!("{path}/audience") => {
                let body: serde_json::Value =
                    serde_json::from_str(req.body.as_deref().unwrap_or("{}")).unwrap_or_default();
                let (s, b) = self.server.borrow_mut().audience_join_answer(
                    Some(&format!("dev:{who}")),
                    body["invite"].as_str().unwrap_or_default(),
                    None,
                );
                answer(s, b)
            }
            (Method::Get, p) if p == path => answer(
                200,
                format!(
                    r#"{{"room_id":"{ROOM}","name":"Fake","revision":1,"phase":"lobby","admission":"open","participant":1}}"#
                ),
            ),
            (Method::Post, p) if p == format!("{path}/ticket") => {
                match self.server.borrow_mut().ticket_answer(&identity) {
                    Ok(body) => answer(200, body),
                    Err(_) => answer(403, r#"{"error":"not_a_member"}"#.into()),
                }
            }
            (Method::Post, p) if p == format!("{path}/grant") => answer(
                200,
                format!(r#"{{"grant":"grant-{who}","expires_at_ms":9,"max_bytes":100.0}}"#),
            ),
            _ => answer(404, r#"{"error":"not_found"}"#.into()),
        };
        self.out.push((id, a));
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        std::mem::take(&mut self.out)
    }
}

pub(crate) fn online(server: &SharedFake, who: Option<&'static str>) -> SharedOnline {
    let o = Online::new(
        Box::new(FakeHttp {
            server: server.clone(),
            who,
            out: Vec::new(),
        }),
        OnlineConfig {
            platform: Platform::Web,
            site: "http://localhost/".into(),
        },
    )
    .shared();
    o.borrow_mut().check();
    o.borrow_mut().poll(0);
    o
}

/// A member's scripted stream: every tap of the chart, every `miss`th
/// missed.
struct Script {
    moments: Vec<Moment>,
    end_ms: i64,
    sent: usize,
    ended: bool,
}

impl Script {
    /// How the member performed, as their device would say it for the
    /// stage's Standings: counted from what the script sent.
    fn detail(&self) -> ddi_room::RunDetail {
        let sent = &self.moments[..self.sent];
        let missed = sent
            .iter()
            .filter(|m| matches!(m.kind, MomentKind::Tap { tier: 5, .. }))
            .count() as u32;
        let hit = sent.len() as u32 - missed;
        let last = sent.last();
        ddi_room::RunDetail {
            score: last.map(|m| m.score),
            percent: Some((i64::from(hit) * 10_000 / i64::from((hit + missed).max(1))) as i32),
            grade: if missed * 12 < hit { "A" } else { "B" }.into(),
            life_ppm: last.map_or(1_000_000, |m| m.life_ppm),
            max_combo: sent.iter().map(|m| m.combo).max().unwrap_or(0),
            tiers: vec![hit, 0, 0, 0, 0, 0, missed],
            ..Default::default()
        }
    }

    fn new(song: &ddi_chart::Song, chart: usize, miss: u32, cut: Option<f64>) -> Script {
        let c = &song.charts[chart];
        let timing = c.timing(song);
        let (mut combo, mut life, mut k, mut score) = (0u32, 1_000_000i64, 0u32, 0i64);
        let mut end_ms = 0;
        let moments = c
            .notes
            .iter()
            .filter(|n| matches!(n.kind, ddi_chart::NoteKind::Tap))
            .filter(|n| cut.is_none_or(|cut| timing.seconds_at(n.tick) <= cut))
            .map(|n| {
                k += 1;
                let missed = k % miss == 0;
                if missed {
                    combo = 0;
                    life -= 60_000;
                } else {
                    combo += 1;
                    life += 4_000;
                    score += 10_100;
                }
                life = life.clamp(1, 1_000_000);
                let at = (timing.seconds_at(n.tick) * 1_000.0).round() as i32;
                end_ms = i64::from(at) + 500;
                Moment {
                    at_ms: at,
                    tick: n.tick.0,
                    lane: n.lane,
                    kind: MomentKind::Tap {
                        tier: if missed { 5 } else { 1 },
                        offset_ms: 0,
                        w0: !missed,
                    },
                    life_ppm: life as u32,
                    combo,
                    score,
                }
            })
            .collect();
        let end_ms = cut.map_or(end_ms, |cut| end_ms.max((cut * 1_000.0) as i64));
        Script {
            moments,
            end_ms,
            sent: 0,
            ended: false,
        }
    }

    fn stream(&mut self, room: &SharedRoom, stage: StageId, now: i64) {
        if self.ended {
            return;
        }
        let mut r = room.borrow_mut();
        let Some(out) = r.live_stream(stage) else {
            return;
        };
        while let Some(m) = self.moments.get(self.sent).filter(|m| i64::from(m.at_ms) <= now) {
            out.moment(*m);
            self.sent += 1;
        }
        out.judged_to(now.clamp(0, self.end_ms) * 1_000);
        if now >= self.end_ms {
            out.end(StreamEnd::Finished);
            self.ended = true;
        }
    }
}

/// The bundled song's entry as song select would give it, its chart
/// `chart`.
pub(crate) fn entry(song: &ddi_chart::Song, chart: usize) -> SongEntry {
    SongEntry {
        id: SONG.into(),
        title: song.title.clone(),
        artist: song.artist.clone(),
        // The bundled song's banner, as song select names it.
        banner: Some(format!("songs/{SONG}/Banner.png")),
        charts: vec![ChartEntry {
            style: song.charts[chart].layout.clone(),
            difficulty: Difficulty::Hard,
            index: chart,
            digest: Some(song.chart_digest(chart)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    }
}

/// The viewer's online state over the fake room (made with it at first):
/// signed out, its invite pages and audience join the fake room's.
pub(crate) fn viewer() -> SharedOnline {
    if let Some(o) = VIEWER.with_borrow(|v| v.clone()) {
        return o;
    }
    let server = room().borrow().server.clone();
    let o = online(&server, None);
    VIEWER.with_borrow_mut(|v| *v = Some(o.clone()));
    o
}

/// A seat in the fake room's audience by `link` (`ticket`: the one its
/// page's Watch was given; none: asked for at once), the viewer's.
pub(crate) fn seat(link: &str, ticket: Option<String>) -> AudienceClient {
    let online = viewer();
    let server = room().borrow().server.clone();
    AudienceClient::new(
        online,
        link,
        Box::new(FakeAudienceSocket::new(server)),
        ticket,
        now_ms(),
    )
}

/// The fake room, made and its pump started (the song read first) at
/// first.
fn room() -> Rc<RefCell<Fake>> {
    if let Some(f) = FAKE.with_borrow(|f| f.clone()) {
        return f;
    }
    let now = now_ms();
    let clock = ManualClock::new(now);
    let server = FakeServer::hosted(
        ROOM,
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default().with_live().with_audience(),
        clock.clone(),
    )
    .shared();
    server.borrow_mut().set_room_name("Fake room (testing)");
    let mut rooms = Vec::new();
    for (i, who) in MEMBERS.into_iter().enumerate() {
        if i > 0 {
            let _ = server
                .borrow_mut()
                .join(Identity::new(format!("dev:{who}")), who, Intent::Play);
        }
        rooms.push(Rc::new(RefCell::new(RoomClient::online(
            online(&server, Some(who)),
            ROOM,
            Box::new(FakeSocket::new(server.clone())),
            ClientId::new(format!("{who}-fake")),
            now,
        ))));
    }
    let fake = Rc::new(RefCell::new(Fake {
        clock,
        server: server.clone(),
        rooms,
        song: None,
        chart: 0,
        step: Step::Connecting,
        scripts: Vec::new(),
        closed: false,
        connected_at: None,
        settled_at: None,
        _pump: None,
    }));
    {
        let fake = fake.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match crate::songs::load_chart(SONG).await {
                Ok(song) => {
                    let chart = (0..song.charts.len())
                        .find(|&c| song.charts[c].layout == "dance-single")
                        .unwrap_or(0);
                    let mut f = fake.borrow_mut();
                    f.chart = chart;
                    f.song = Some(Rc::new(song));
                }
                Err(e) => web_sys::console::warn_1(&format!("fake seat: {e:?}").into()),
            }
        });
    }
    let pumped = fake.clone();
    fake.borrow_mut()._pump = Some(gloo::timers::callback::Interval::new(50, move || {
        if let Ok(mut f) = pumped.try_borrow_mut() {
            f.pump();
        }
    }));
    FAKE.with_borrow_mut(|f| *f = Some(fake.clone()));
    fake
}

/// A watched stage's song, read as the bundled song (the fake room's
/// share is nothing to fetch).
pub(crate) fn prepare(job: LoadJob) {
    wasm_bindgen_futures::spawn_local(async move {
        let result = match crate::songs::load_chart(SONG).await {
            Ok(song) => prepare_stage(&job, &song, &mut |e| {
                web_sys::console::warn_1(&format!("fake seat: {e}").into())
            })
            .map(|p| {
                let e = entry(&song, p.index);
                (
                    Picked {
                        song: e.clone(),
                        chart: e.charts[0].clone(),
                    },
                    p.live,
                )
            }),
            Err(_) => Err(ddi_ui::state::room::LoadProblem::Unreadable),
        };
        if let Some(a) = crate::online::kept_audience() {
            // The bundled song's background, as a share's listing would
            // name the stage's.
            let background = Some(format!("songs/{SONG}/Background.png"));
            a.borrow_mut().finish_load_with(&job, result, background);
        }
    });
}

impl Fake {
    /// Each member prepares what its client asks for (the song as read
    /// here) and takes its stage's order, as its screens would: the
    /// stage, once one was handed over.
    fn load(&mut self, song: &ddi_chart::Song) -> Option<StageId> {
        let mut stage = None;
        for room in &self.rooms {
            let jobs = room.borrow_mut().take_load_jobs();
            for job in jobs {
                let Ok(p) = prepare_stage(&job, song, &mut |_| {}) else {
                    continue;
                };
                let e = entry(song, p.index);
                let picked = Picked {
                    song: e.clone(),
                    chart: e.charts[0].clone(),
                };
                if job.share.is_some() {
                    room.borrow_mut()
                        .finish_shared_load_live(&job, Ok((picked, p.live)));
                } else {
                    room.borrow_mut().finish_load_live(&job, Ok(p.live));
                }
            }
            if let Some(o) = room.borrow_mut().take_stage_order() {
                stage = Some(o.stage);
            }
        }
        stage
    }

    fn pump(&mut self) {
        let now = now_ms();
        self.clock.set(now.max(self.clock.now().0));
        for r in &self.rooms {
            if let Ok(mut r) = r.try_borrow_mut() {
                r.tick(now);
            }
        }
        self.server.borrow_mut().pump();
        let Some(song) = self.song.clone() else {
            return;
        };
        match self.step {
            Step::Connecting => {
                if !self.rooms.iter().all(|r| r.borrow().room_now().is_some()) {
                    return;
                }
                let plan = MATCH.get();
                let since = *self.connected_at.get_or_insert(now);
                if (now.saturating_sub(since) as f64) < plan.wait * 1_000.0 {
                    return;
                }
                let owner = &self.rooms[0];
                let mut rules = owner.borrow().rules().clone();
                rules.set_attacks(false);
                rules.stages = plan.stages;
                let _ = owner.borrow_mut().save_rules(rules);
                let e = entry(&song, self.chart);
                let _ = owner.borrow_mut().pick(&e, &e.charts[0]);
                self.step = Step::Picked;
            }
            Step::Picked => {
                let uploads = self.rooms[0].borrow_mut().take_upload_jobs();
                for j in uploads {
                    let _ = self.server.borrow_mut().authority(Command::SetContentStatus {
                        stage: j.stage,
                        status: ContentStatus::Available,
                        share: Some("abc12345".into()),
                    });
                    self.rooms[0]
                        .borrow_mut()
                        .upload_finished(j.stage, Ok("abc12345".into()));
                }
                if let Some(stage) = self.load(&song) {
                    self.step = Step::Readied(stage);
                }
            }
            Step::Readied(stage) => {
                self.load(&song);
                for room in &self.rooms {
                    if room.borrow().in_stage(stage) {
                        let _ = room.borrow_mut().set_ready(true);
                    }
                }
                if self.rooms[0].borrow().stage_started(stage)
                    && let Some(zero) = self.rooms[0].borrow().song_zero_at(stage)
                {
                    self.scripts = (0..self.rooms.len())
                        .map(|i| Script::new(&song, self.chart, 9 + 4 * i as u32, MATCH.get().cut))
                        .collect();
                    self.step = Step::Playing(stage, zero);
                }
            }
            Step::Playing(stage, zero) => {
                let mut song_s = 0.0;
                for (s, room) in self.scripts.iter_mut().zip(&self.rooms) {
                    let t = room
                        .borrow()
                        .room_now()
                        .map_or(0, |n| n.0 as i64 - zero.0 as i64);
                    song_s = t as f64 / 1_000.0;
                    s.stream(room, stage, t);
                }
                if !self.closed
                    && let Some(end) = END_AFTER.get()
                    && song_s >= end
                {
                    self.closed = true;
                    self.rooms[0].borrow_mut().set_audience_open(false);
                }
                // A match asked for: each member says how they performed
                // once their stream ended, as their device would.
                if MATCH.get() != MatchPlan::parse("") && self.scripts.iter().all(|s| s.ended) {
                    for (s, room) in self.scripts.iter().zip(&self.rooms) {
                        let d = s.detail();
                        let run = ddi_room::Run::Live {
                            life: f64::from(d.life_ppm) / 1e6,
                            score: d.score.unwrap_or(0),
                            steps: 0,
                        };
                        let _ = room.borrow_mut().report_run(stage, run, Some(d));
                    }
                    self.step = Step::Played(stage);
                }
            }
            Step::Played(stage) => {
                let owner = &self.rooms[0];
                let phase = owner.borrow().state().phase.clone();
                match phase {
                    ddi_room::Phase::MatchResults => self.step = Step::Done,
                    // Between stages, a moment for its results: the next
                    // pick.
                    ddi_room::Phase::Intermission { .. }
                        if owner.borrow().stage_confirmed(stage) =>
                    {
                        let wait = (MATCH.get().wait * 1_000.0) as u64;
                        if self.settled_at.get_or_insert(now).saturating_add(wait) > now {
                            return;
                        }
                        self.settled_at = None;
                        let e = entry(&song, self.chart);
                        let _ = owner.borrow_mut().pick(&e, &e.charts[0]);
                        self.step = Step::Picked;
                    }
                    _ => {}
                }
            }
            Step::Done => {}
        }
    }
}
