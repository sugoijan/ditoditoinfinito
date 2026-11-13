//! TESTING ONLY: a room watched with its audience without a server, for
//! the headless browsers (`docs/plans/multiplayer-prototype-progress.md`,
//! "The web watches an audience's stage"). Asked for on the start route
//! beside the prototype's switch (`#/?mp=1&mpfakeseat=1`, and
//! `mpfakeend=S` to close the audience S seconds into the stage), never
//! from a menu: the next audience seat (`crate::online::audience`) is then
//! a seat in the fake room's audience (`ddi_room::fake`, in the page),
//! whose three members stream a bundled song's chart as scripted hits
//! (each missing every few notes), on the wall clock, pumped twenty times
//! a second. The seat, its page, the stage handed over, the play route
//! and the spectator view are the real ones; only the server is not.

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
const SONG: &str = "beatucada";
const ROOM: &str = "Room000001";
const MEMBERS: [&str; 3] = ["alice", "bob", "cody"];

thread_local! {
    /// Asked for (`mpfakeseat=1`), and when to close the audience.
    static WANTED: Cell<bool> = const { Cell::new(false) };
    static END_AFTER: Cell<Option<f64>> = const { Cell::new(None) };
    static FAKE: RefCell<Option<Rc<RefCell<Fake>>>> = const { RefCell::new(None) };
}

/// The next audience seat is the fake room's (`end_after`: the audience
/// closed that many seconds into the stage).
pub(crate) fn want(end_after: Option<f64>) {
    WANTED.set(true);
    END_AFTER.set(end_after);
}

/// Whether the audience is the fake room's.
pub(crate) fn on() -> bool {
    WANTED.get()
}

fn now_ms() -> u64 {
    js_sys::Date::now().max(0.0) as u64
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
    _pump: Option<gloo::timers::callback::Interval>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Connecting,
    Picked,
    Readied(StageId),
    Playing(StageId, ddi_room::Millis),
}

/// HTTP answered from the fake room: who `who` is, the room's summary,
/// its tickets and grants; for the seat, the audience join as the server
/// answers it.
struct FakeHttp {
    server: SharedFake,
    who: &'static str,
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
        let who = self.who;
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
                    &format!("dev:{who}"),
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
                match self.server.borrow_mut().ticket(&identity) {
                    Ok(t) => answer(200, format!(r#"{{"ticket":"{t}","expires_at_ms":9}}"#)),
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

fn online(server: &SharedFake, who: &'static str) -> SharedOnline {
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
    fn new(song: &ddi_chart::Song, chart: usize, miss: u32) -> Script {
        let c = &song.charts[chart];
        let timing = c.timing(song);
        let (mut combo, mut life, mut k, mut score) = (0u32, 1_000_000i64, 0u32, 0i64);
        let mut end_ms = 0;
        let moments = c
            .notes
            .iter()
            .filter(|n| matches!(n.kind, ddi_chart::NoteKind::Tap))
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
fn entry(song: &ddi_chart::Song, chart: usize) -> SongEntry {
    SongEntry {
        id: SONG.into(),
        title: song.title.clone(),
        artist: song.artist.clone(),
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

/// A seat in the fake room's audience by `link`, the room made and its
/// pump started (the song read first).
pub(crate) fn seat(link: &str) -> AudienceClient {
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
            online(&server, who),
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
    FAKE.with_borrow_mut(|f| *f = Some(fake));
    AudienceClient::new(
        online(&server, "dora"),
        link,
        Box::new(FakeAudienceSocket::new(server)),
        None,
        now,
    )
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
            a.borrow_mut().finish_load(&job, result);
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
                let owner = &self.rooms[0];
                let mut rules = owner.borrow().rules().clone();
                rules.set_attacks(false);
                rules.stages = 1;
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
                        .map(|i| Script::new(&song, self.chart, 9 + 4 * i as u32))
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
            }
        }
    }
}
