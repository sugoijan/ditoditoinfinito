//! DEVELOPMENT TOOL: simulated players for a private online room
//! (`docs/plans/multiplayer-prototype-progress.md`, M7d), `ddi --mp-bots`.
//! Headless: no window, no sound device.
//!
//! Each bot is a player of its own kind, `bot:<name>` (labeled Bot on every
//! screen), signed in by the server only for an operator's admin share
//! token (`POST /auth/bots`, `../ddi-server`'s README), never an
//! administrator. A bot is the game's own room client
//! ([`RoomClient`] over desktop's HTTP and socket, `crate::online`) driven
//! as a player's screens drive it, so it follows the room's protocol as
//! the game does: Rules, the host's resting song (shared ahead), the pick,
//! the stage's song read from the room's share (`crate::room`'s threads),
//! Ready on the stage's prompt, the stage played and the run reported
//! (`ReportOutcome`, self-reported as everyone's is). Its play is
//! `ddi_engine::cpu`'s [`Opponent`] (the chart's tap-only copy, a level
//! relative to the chart and a personality from the CPU roster or the
//! flags) judged on a real-time song clock: never a skill-tuned player.
//!
//! A host bot (`--host`) makes a room, prints its invite, accepts its
//! Rules, rests on each song before picking it (so that the others see and
//! hear it), and plays too; guest bots (`--guests N`) join the host bot's
//! room or `--join`'s invite. They stay for the next stages and matches
//! until stopped (`--matches N`, or Ctrl-C: each leaves the room).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ddi_engine::cpu::{Opponent, Skills};
use ddi_room::{
    Capability, ClientId, Connection, Intent, LinkState, Phase, PhaseKind, Run, StageId,
};
use ddi_ui::screens::data::{ChartEntry, SongEntry};
use ddi_ui::state::online::{Account, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::room::{
    Inbox, LoadProblem, RoomClient, RoomView, SPOTLIGHT, SharedRoom, ShellNews, StageOrder,
    eligible,
};

/// Bots in one run at most (the server keeps a room to eight).
pub const MAX_BOTS: usize = 8;
/// A bot's song clock begins this long before the song's second 0, as a
/// play's lead-in does.
const LEAD_S: f64 = 2.0;
/// After its last note is judged, a bot reports this much later.
const TAIL_S: f64 = 2.0;
/// The host's pause after a stage's (or match's) results, for the
/// standings to be read, before it rests on the next song.
const PAUSE_MS: u64 = 15_000;
/// A refused pick is tried again after this long.
const RETRY_MS: u64 = 5_000;
/// Leaving on Ctrl-C waits for the room's answers this long at most.
const LEAVE_MS: u64 = 4_000;

/// What `--mp-bots` was asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// A host bot makes a room and prints its invite.
    pub host: bool,
    /// Guest bots' invite (`--join`; none: the host bot's room).
    pub join: Option<String>,
    pub guests: usize,
    /// The server (`--api`), else `DDI_API_URL`, else the production one.
    pub api: Option<String>,
    /// The game's page invite links open on (`--site`).
    pub site: Option<String>,
    /// Bots' names (`--names`), else the CPU roster's.
    pub names: Vec<String>,
    /// A level, from 1, and a personality for every bot, over the roster's.
    pub level: Option<u8>,
    pub personality: Option<String>,
    /// The host's songs: these folders' (`--songs`), else the library's
    /// (the bundled songs and the settings' folders).
    pub songs: Vec<PathBuf>,
    /// The host's fixed list (`--pick ID[:CHART],…`).
    pub picks: Vec<(String, Option<usize>)>,
    /// How long the host rests on a song before picking it.
    pub rest_ms: u64,
    /// The host's Rules: stages a match.
    pub stages: Option<u16>,
    /// Stop after this many matches (none: until Ctrl-C).
    pub matches: Option<u32>,
    /// Players (people and bots) the host waits for before it picks.
    pub wait_for: usize,
    /// Seed of the names' order and the bots' plays (default: the time).
    pub seed: Option<u64>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            host: false,
            join: None,
            guests: 0,
            api: None,
            site: None,
            names: Vec::new(),
            level: None,
            personality: None,
            songs: Vec::new(),
            picks: Vec::new(),
            rest_ms: 8_000,
            stages: None,
            matches: None,
            wait_for: 2,
            seed: None,
        }
    }
}

pub const USAGE: &str = "\
ddi --mp-bots [--host] [--join LINK|INVITE] [--guests N] [options]
  Simulated players (DEVELOPMENT TOOL) in a private online room, played by
  the CPU; the server signs them in for an admin share token (DDI_SHARE_TOKEN,
  else the settings' share token). Ctrl-C leaves.
  --host               a host bot: makes a room, prints its invite, picks songs
  --join LINK|INVITE   guest bots join this room (else the host bot's)
  --guests N           guest bots (default 3 with --join, else 0; at most 8 bots)
  --api URL            the server (else DDI_API_URL, else the production one)
  --site URL           the game's page for invite links
  --names A,B,…        the bots' names (else the CPU roster's)
  --level 1-5          every bot's CPU level (else the roster's)
  --personality P      every bot's personality: a preset or seven multipliers
  --songs DIR          the host's songs from this folder (repeatable)
  --pick ID[:N],…      the host's songs in this order (chart index N)
  --rest SECONDS       the host's rest on a song before picking it (default 8)
  --stages N           the host's Rules: stages a match
  --matches N          stop after N matches
  --wait-for N         players the host waits for before picking (default 2)
  --seed N             names' order and plays";

impl Options {
    /// `args` (the command line after the program) read.
    pub fn parse(args: &[String]) -> Result<Options, String> {
        let mut o = Options::default();
        let mut guests = None;
        let mut i = 0;
        let value = |i: usize, flag: &str| -> Result<&String, String> {
            args.get(i + 1)
                .ok_or_else(|| format!("{flag} takes a value"))
        };
        while i < args.len() {
            let a = args[i].as_str();
            let mut took = true;
            match a {
                "--mp-bots" => took = false,
                "--host" => {
                    o.host = true;
                    took = false;
                }
                "--join" => {
                    let v = value(i, a)?;
                    o.join = Some(
                        ddi_ui::state::online::api::invite_in(v)
                            .ok_or("--join takes an invite link or an invite")?,
                    );
                }
                "--guests" => {
                    guests = Some(
                        value(i, a)?
                            .parse::<usize>()
                            .map_err(|_| "--guests takes a number")?,
                    )
                }
                "--api" => o.api = Some(value(i, a)?.clone()),
                "--site" => o.site = Some(value(i, a)?.clone()),
                "--names" => {
                    o.names = value(i, a)?
                        .split(',')
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                        .map(String::from)
                        .collect()
                }
                "--level" => {
                    let l = value(i, a)?.parse::<u8>().ok();
                    let levels = ddi_engine::cpu::STRENGTHS.len() as u8;
                    o.level = Some(
                        l.filter(|l| (1..=levels).contains(l))
                            .ok_or(format!("--level takes 1 to {levels}"))?,
                    );
                }
                "--personality" => {
                    let p = value(i, a)?;
                    Skills::parse(p).map_err(|e| format!("--personality: {e}"))?;
                    o.personality = Some(p.clone());
                }
                "--songs" => o.songs.push(PathBuf::from(value(i, a)?)),
                "--pick" => {
                    for p in value(i, a)?
                        .split(',')
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                    {
                        let (id, chart) = match p.rsplit_once(':') {
                            Some((id, n)) if n.parse::<usize>().is_ok() => {
                                (id.to_string(), n.parse().ok())
                            }
                            _ => (p.to_string(), None),
                        };
                        o.picks.push((id, chart));
                    }
                }
                "--rest" => {
                    let s = value(i, a)?
                        .parse::<f64>()
                        .ok()
                        .filter(|s| s.is_finite() && *s >= 0.0)
                        .ok_or("--rest takes seconds")?;
                    o.rest_ms = (s * 1000.0) as u64;
                }
                "--stages" => {
                    o.stages = Some(
                        value(i, a)?
                            .parse::<u16>()
                            .ok()
                            .filter(|n| *n >= 1)
                            .ok_or("--stages takes a number from 1")?,
                    )
                }
                "--matches" => {
                    o.matches = Some(
                        value(i, a)?
                            .parse::<u32>()
                            .ok()
                            .filter(|n| *n >= 1)
                            .ok_or("--matches takes a number from 1")?,
                    )
                }
                "--wait-for" => {
                    o.wait_for = value(i, a)?
                        .parse::<usize>()
                        .ok()
                        .filter(|n| (2..=9).contains(n))
                        .ok_or("--wait-for takes 2 to 9")?
                }
                "--seed" => {
                    o.seed = Some(
                        value(i, a)?
                            .parse::<u64>()
                            .map_err(|_| "--seed takes a number")?,
                    )
                }
                other => return Err(format!("--mp-bots does not take {other}")),
            }
            i += if took { 2 } else { 1 };
        }
        o.guests = guests.unwrap_or(if o.join.is_some() { 3 } else { 0 });
        if !o.host && o.join.is_none() {
            return Err("--mp-bots needs --host or --join".into());
        }
        if !o.host && o.guests == 0 {
            return Err("--join needs at least one guest (--guests N)".into());
        }
        let bots = o.guests + usize::from(o.host);
        if bots > MAX_BOTS {
            return Err(format!("at most {MAX_BOTS} bots in a room"));
        }
        if !o.names.is_empty() && o.names.len() < bots {
            return Err(format!(
                "--names gives {} names for {bots} bots",
                o.names.len()
            ));
        }
        Ok(o)
    }
}

/// A bot's CPU: its name, its level (an index of the engine's strengths)
/// and its personality (`ddi_engine::cpu::Skills`).
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub level: usize,
    pub skills: String,
}

/// The run's bots' profiles: the names given, else the CPU roster's in an
/// order drawn from `seed`; each its roster level and personality unless
/// the options set them (a named bot not on the roster: level 3,
/// balanced).
pub fn profiles(o: &Options, count: usize, seed: u64) -> Vec<Profile> {
    let roster = ddi_ui::state::cpu_roster::catalog();
    let mut order: Vec<usize> = (0..roster.len()).collect();
    // A small seeded shuffle: the same seed, the same bots.
    let mut x = seed | 1;
    for i in (1..order.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        order.swap(i, (x % (i as u64 + 1)) as usize);
    }
    (0..count)
        .map(|n| {
            let from_roster = if o.names.is_empty() {
                roster.get(order[n % order.len()])
            } else {
                roster.iter().find(|c| c.name == o.names[n])
            };
            let name = o
                .names
                .get(n)
                .cloned()
                .or_else(|| from_roster.map(|c| c.name.clone()))
                .unwrap_or_else(|| format!("Bot {}", n + 1));
            let level = o
                .level
                .or_else(|| from_roster.map(|c| c.level))
                .unwrap_or(3);
            Profile {
                name,
                level: usize::from(level.saturating_sub(1)),
                skills: o
                    .personality
                    .clone()
                    .or_else(|| from_roster.map(|c| c.skills.clone()))
                    .unwrap_or_else(|| "balanced".into()),
            }
        })
        .collect()
}

/// A song the host may pick: one original chart one player plays on four
/// lanes with its digest (the CPU plays it; everyone checks it by its
/// digest), closest to meter 7 unless `chart` names one.
pub fn choose_chart(song: &SongEntry, chart: Option<usize>) -> Option<ChartEntry> {
    let ok = |c: &&ChartEntry| eligible(c).is_ok() && c.digest.is_some() && c.version.is_none();
    match chart {
        Some(i) => song
            .charts
            .iter()
            .filter(ok)
            .find(|c| c.index == i)
            .cloned(),
        None => song
            .charts
            .iter()
            .filter(ok)
            .min_by_key(|c| (c.meter as i64 - 7).abs())
            .cloned(),
    }
}

/// The stage played by a bot: its CPU on the stage's chart, judged on a
/// real-time song clock from the stage's start.
pub struct StagePlay {
    pub stage: StageId,
    pub number: u16,
    opponent: Opponent,
    started_ms: u64,
}

impl StagePlay {
    /// Chart `chart` of `song` (as read) played by `profile` with `seed`,
    /// under the match's ruleset (`StageOrder::ruleset`, everyone's; its
    /// fail immediate: out of the stage), from `now_ms`.
    pub fn new(
        order: &StageOrder,
        song: &ddi_chart::Song,
        chart: usize,
        profile: &Profile,
        seed: u64,
        now_ms: u64,
    ) -> Result<StagePlay, String> {
        let mut opponent = cpu_for(song, chart, &order.ruleset, profile, seed)?;
        opponent.start(0.0);
        Ok(StagePlay {
            stage: order.stage,
            number: order.number,
            opponent,
            started_ms: now_ms,
        })
    }

    /// The song's second at `now_ms`.
    pub fn song_time(&self, now_ms: u64) -> f64 {
        now_ms.saturating_sub(self.started_ms) as f64 / 1000.0 - LEAD_S
    }

    /// Played up to `now_ms`; its run once it is over (out, or the song
    /// played to its last note and a little more).
    pub fn advance(&mut self, now_ms: u64) -> Option<Run> {
        let t = self.song_time(now_ms);
        self.opponent.advance(t);
        if let Some(at) = self.opponent.out_at() {
            return Some(Run::Out { at });
        }
        let end = self.opponent.player().end_time();
        let over = (self.opponent.finished() && t >= end + TAIL_S) || t >= end + 30.0;
        over.then(|| Run::Live {
            life: f64::from(self.opponent.life()),
            accuracy: ddi_session::battle::accuracy(&self.opponent.score()),
        })
    }
}

/// `profile`'s CPU on chart `chart` of `song` under the match's ruleset
/// `ruleset` (`ddi_session::battle::stage_ruleset`), or the engine's reason
/// why it cannot play it.
fn cpu_for(
    song: &ddi_chart::Song,
    chart: usize,
    ruleset: &str,
    profile: &Profile,
    seed: u64,
) -> Result<Opponent, String> {
    let c = song.charts.get(chart).ok_or("no such chart")?;
    let rules = ddi_session::battle::stage_ruleset(ruleset, c);
    let skills = Skills::parse(&profile.skills).unwrap_or_default();
    let level = profile.level.min(ddi_engine::cpu::STRENGTHS.len() - 1);
    Opponent::with_skills(song, chart, rules, level, seed, skills)
}

/// A run in a line of the log.
fn run_text(run: &Run) -> String {
    match run {
        Run::Live { life, accuracy } => {
            format!(
                "life {:.0}%, accuracy {:.2}%",
                life * 100.0,
                accuracy * 100.0
            )
        }
        Run::Out { at } => format!("out at {at:.1} s"),
        Run::Forfeit => "forfeit".into(),
    }
}

fn mix(a: u64, b: u64) -> u64 {
    let mut x = a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^ (x >> 29)
}

fn now_ms() -> u64 {
    crate::clock::epoch_ms().max(0.0) as u64
}

fn clock_text() -> String {
    let s = now_ms() / 1000;
    format!("{:02}:{:02}:{:02}", (s / 3600) % 24, (s / 60) % 60, s % 60)
}

/// A bot's session from the server, for the admin `token`: its token, or
/// why not.
fn sign_in(base: &str, token: &str, name: &str) -> Result<String, String> {
    let url = format!("{}/auth/bots", base.trim_end_matches('/'));
    let body = serde_json::json!({ "name": name }).to_string();
    let mut resp = crate::share::agent()
        .post(&url)
        .header("authorization", &format!("Bearer {token}"))
        .header("content-type", "application/json")
        .send(body)
        .map_err(|e| format!("{url}: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    if status == 201
        && let Some(t) = doc.get("session_token").and_then(|t| t.as_str())
    {
        return Ok(t.to_string());
    }
    let why = doc
        .get("error")
        .and_then(|e| e.as_str())
        .map_or_else(|| text.chars().take(120).collect(), String::from);
    Err(format!("HTTP {status} {why}"))
}

/// The host's songs and where it is in them.
struct Host {
    songs: Vec<(SongEntry, ChartEntry)>,
    next: usize,
    resting: Option<u64>,
    rules: bool,
    stages: Option<u16>,
    rest_ms: u64,
    wait_for: usize,
    not_before: u64,
    waiting_said: bool,
}

enum Role {
    Host(Box<Host>),
    /// A guest; its invite once known (the host bot's comes later).
    Guest(Option<String>),
}

/// What the log said last, so that it says each change once.
#[derive(Default)]
struct Said {
    link: Option<LinkState>,
    phase: Option<PhaseKind>,
    spotlight: Option<String>,
    invite: bool,
    refusal: Option<String>,
}

struct Bot {
    profile: Profile,
    role: Role,
    seed: u64,
    online: SharedOnline,
    asked: bool,
    room: Option<SharedRoom>,
    news: Option<crate::room::NewsTx>,
    order: Option<StageOrder>,
    ready_at: u64,
    play: Option<StagePlay>,
    matches: u32,
    said: Said,
    /// Over (refused, closed, left): nothing more to do.
    done: bool,
}

impl Bot {
    fn say(&self, text: impl AsRef<str>) {
        println!(
            "{} {:<18} {}",
            clock_text(),
            self.profile.name,
            text.as_ref()
        );
    }

    fn host_invite(&self) -> Option<String> {
        let r = self.room.as_ref()?.borrow();
        let link = r.online_link()?;
        ddi_ui::state::online::api::invite_in(&link.invite_link()?)
    }

    fn tick(&mut self, now: u64, base: &str, matches: Option<u32>) {
        if self.done {
            return;
        }
        if self.room.is_none() {
            self.enter(now, base);
            return;
        }
        let room = self.room.clone().expect("in a room");
        room.borrow_mut().tick(now);
        self.links(&room);
        if self.done {
            return;
        }
        let connected = room.borrow().link_state() == LinkState::Connected;
        // The room's own work, as its screens do it: the invite for its
        // operators, songs to prepare and to share.
        if connected
            && room.borrow().may(Capability::ChooseContent)
            && room.borrow().invite_link().is_none()
        {
            room.borrow_mut().fetch_invite();
        }
        if !self.said.invite
            && let Some(link) = room.borrow().invite_link()
        {
            self.said.invite = true;
            if matches!(self.role, Role::Host(_)) {
                self.say(format!("invite: {link}"));
            }
        }
        let jobs = room.borrow_mut().take_load_jobs();
        for job in jobs {
            if job.stage != SPOTLIGHT {
                self.say(format!("loading stage {}", stage_number(&room, job.stage)));
            }
            crate::room::prepare_online_with(self.news.as_ref(), &room, job);
        }
        let uploads = room.borrow_mut().take_upload_jobs();
        for job in uploads {
            let what = if job.stage == SPOTLIGHT {
                "sharing its resting song".to_string()
            } else {
                format!("sharing stage {}'s song", stage_number(&room, job.stage))
            };
            self.say(what);
            if let Some(tx) = &self.news {
                crate::room::upload_with(tx, job);
            }
        }
        self.notes(&room, matches);
        if self.done {
            return;
        }
        self.stage(&room, now);
        if let Role::Host(host) = &mut self.role {
            let host = host.as_mut();
            Self::host(
                host,
                &room,
                now,
                &self.profile,
                self.order.is_some(),
                &mut |t| println!("{} {:<18} {t}", clock_text(), self.profile.name),
            );
        }
    }

    /// Signed in, then into the room: the host's made, the guest's joined.
    fn enter(&mut self, now: u64, base: &str) {
        let mut o = self.online.borrow_mut();
        o.poll(now);
        match o.account().clone() {
            Account::Unknown => o.check(),
            Account::SignedIn(me) if !self.asked => {
                match &self.role {
                    Role::Host(_) => {
                        self.asked = true;
                        o.private_room();
                    }
                    Role::Guest(Some(invite)) => {
                        self.asked = true;
                        o.open_invite(invite);
                        o.join_invite();
                    }
                    Role::Guest(None) => return,
                }
                drop(o);
                self.say(format!("signed in as {}", me.player_id));
                return;
            }
            Account::SignedOut | Account::Unreachable => {
                drop(o);
                self.say("the server did not take the bot's session");
                self.done = true;
                return;
            }
            _ => {}
        }
        if let Some(p) = o.create_problem().cloned() {
            drop(o);
            self.say(format!("making the room was refused: {}", p.code));
            self.done = true;
            return;
        }
        if let Some(p) = o.invite().and_then(|v| v.refused.clone()) {
            drop(o);
            self.say(format!("joining was refused: {}", p.code));
            self.done = true;
            return;
        }
        let Some(id) = o.take_entered() else {
            return;
        };
        drop(o);
        let client = ClientId::new(format!(
            "bot-{}-{:x}",
            self.profile.name.to_lowercase().replace(' ', "-"),
            mix(self.seed, now)
        ));
        let mut room = RoomClient::online(
            self.online.clone(),
            &id,
            Box::new(crate::online::SocketTransport::with_base(base.to_string())),
            client,
            now,
        );
        let (tx, inbox) = crate::room::news_inbox();
        room.set_inbox(checked(inbox, self.profile.clone()));
        // The Rules pop-up is a screen's; a bot has read them.
        room.set_rules_seen(true);
        self.news = Some(tx);
        self.room = Some(Rc::new(RefCell::new(room)));
        self.say(format!("in room {id}"));
    }

    /// The link's changes said; an ended link ends the bot.
    fn links(&mut self, room: &SharedRoom) {
        let link = room.borrow().link_state();
        if self.said.link == Some(link) {
            return;
        }
        let first = self.said.link.is_none();
        self.said.link = Some(link);
        match link {
            LinkState::Connected => {
                let r = room.borrow();
                let me = r.state().member(r.me());
                let who = me.map_or(String::new(), |m| {
                    format!(" as player {} ({:?})", m.id, m.role).to_lowercase()
                });
                drop(r);
                self.say(format!("connected{who}"));
                if first && !matches!(self.role, Role::Host(_)) {
                    self.say("Rules: OK (read only)");
                }
            }
            LinkState::Joining => {}
            LinkState::Reconnecting { attempt } => self.say(format!("reconnecting ({attempt})")),
            LinkState::SignInAgain => {
                self.say("the session ended; leaving");
                self.done = true;
            }
            LinkState::Closed => {
                self.say("the room closed");
                self.done = true;
            }
            LinkState::Left => {
                self.say("left the room");
                self.done = true;
            }
        }
    }

    /// What the room did, said once: the host's resting song, refusals,
    /// the standings after each stage, the match's end.
    fn notes(&mut self, room: &SharedRoom, matches: Option<u32>) {
        let (spot, refusal, phase) = {
            let r = room.borrow();
            let s = r.state();
            let spot = s.spotlight.as_ref().filter(|sp| sp.by != r.me()).map(|sp| {
                format!(
                    "{} ({} {})",
                    sp.card.title, sp.card.difficulty, sp.card.meter
                )
            });
            (spot, r.refusal().map(|f| format!("{f:?}")), s.phase.kind())
        };
        if spot.is_some() && spot != self.said.spotlight {
            self.say(format!(
                "following the host's song: {}",
                spot.clone().unwrap_or_default()
            ));
        }
        self.said.spotlight = spot;
        if refusal.is_some() && refusal != self.said.refusal {
            self.say(format!("refused: {}", refusal.clone().unwrap_or_default()));
            room.borrow_mut().clear_refusal();
        }
        self.said.refusal = refusal;
        if self.said.phase == Some(phase) {
            return;
        }
        self.said.phase = Some(phase);
        if matches!(phase, PhaseKind::Intermission | PhaseKind::MatchResults) {
            let r = room.borrow();
            let s = r.state();
            let standings = s.standings().unwrap_or_default();
            if let Some(mine) = standings.iter().find(|st| st.participant == r.me()) {
                let what = if phase == PhaseKind::MatchResults {
                    "match over"
                } else {
                    "standings"
                };
                let line = format!(
                    "{what}: place {} of {}, {} points",
                    mine.rank,
                    standings.len(),
                    mine.total
                );
                drop(r);
                self.say(line);
            }
        }
        if phase == PhaseKind::MatchResults {
            self.matches += 1;
            if matches.is_some_and(|m| self.matches >= m) {
                self.say(format!("{} match(es) played; leaving", self.matches));
                room.borrow_mut().leave();
            }
        }
    }

    /// The stage this bot is in: Ready on its prompt, its play once the
    /// room starts it, its run reported.
    fn stage(&mut self, room: &SharedRoom, now: u64) {
        if self.order.is_none() && room.borrow().view() != RoomView::Select {
            let order = room.borrow_mut().take_stage_order();
            if let Some(order) = order {
                // A person's moment to press Ready.
                self.ready_at = now + 700 + mix(self.seed, now) % 1800;
                self.order = Some(order);
            }
        }
        let Some(order) = self.order.clone() else {
            return;
        };
        if let Some(play) = &mut self.play {
            let Some(run) = play.advance(now) else {
                return;
            };
            let number = play.number;
            self.play = None;
            self.order = None;
            self.say(format!("stage {number}: {}", run_text(&run)));
            if let Err(why) = room.borrow_mut().report_run(order.stage, run) {
                self.say(format!("the report was refused: {why:?}"));
            }
            return;
        }
        let started = room.borrow().stage_started(order.stage);
        if started {
            let seed = mix(self.seed, order.stage.0 ^ (u64::from(self.matches) << 32));
            let read = crate::play::load::read_chart(&order.song.id)
                .map_err(|e| format!("{e:?}"))
                .and_then(|song| {
                    StagePlay::new(&order, &song, order.chart.index, &self.profile, seed, now)
                });
            match read {
                Ok(play) => {
                    self.say(format!(
                        "stage {} of {} started: {}",
                        order.number, order.of, order.song.title
                    ));
                    self.play = Some(play);
                }
                Err(why) => {
                    self.say(format!(
                        "stage {}: cannot play it ({why}); forfeit",
                        order.number
                    ));
                    self.order = None;
                    let _ = room.borrow_mut().report_run(order.stage, Run::Forfeit);
                }
            }
            return;
        }
        let view = room.borrow().view();
        if view == RoomView::Prompt(order.stage) {
            let (ready, failed) = {
                let r = room.borrow();
                (r.ready(), r.load_problem(order.stage))
            };
            if let Some(p) = failed {
                if now >= self.ready_at {
                    self.say(format!(
                        "stage {}: cannot play its song ({p:?})",
                        order.number
                    ));
                    self.ready_at = u64::MAX;
                }
            } else if !ready && now >= self.ready_at {
                match room.borrow_mut().set_ready(true) {
                    Ok(()) => self.say(format!("ready for stage {}", order.number)),
                    Err(why) => self.say(format!("Ready was refused: {why:?}")),
                }
                self.ready_at = u64::MAX;
            }
        } else if view == RoomView::Select {
            // The stage went on without this bot (or the pick was undone).
            let sat_out = room.borrow().state().stage(order.stage).is_some_and(|st| {
                st.play
                    .as_ref()
                    .is_some_and(|p| !p.lineup.contains(&room.borrow().me()))
            });
            if sat_out {
                self.say(format!("sitting out stage {}", order.number));
            }
            room.borrow_mut().leave_prompt(order.stage);
            self.order = None;
        }
    }

    /// The host's part: its Rules accepted once, then a song rested on and
    /// picked whenever the room waits for one and someone else is there to
    /// play it.
    fn host(
        host: &mut Host,
        room: &SharedRoom,
        now: u64,
        _profile: &Profile,
        busy: bool,
        say: &mut dyn FnMut(String),
    ) {
        let (connected, operator, phase) = {
            let r = room.borrow();
            (
                r.link_state() == LinkState::Connected,
                r.may(Capability::ChooseContent),
                r.state().phase.clone(),
            )
        };
        if !connected || !operator {
            return;
        }
        if !host.rules {
            host.rules = true;
            let mut rules = room.borrow().rules().clone();
            if let Some(n) = host.stages {
                rules.stages = n;
            }
            let stages = rules.stages;
            match room.borrow_mut().save_rules(rules) {
                Ok(()) => say(format!("Rules: OK ({stages} stages a match)")),
                Err(why) => say(format!("Rules refused: {why:?}")),
            }
            host.not_before = now + 2_000;
        }
        let waiting = matches!(
            phase,
            Phase::Lobby
                | Phase::Intermission { .. }
                | Phase::MatchResults
                | Phase::Interrupted { .. }
        );
        if !waiting {
            // A stage gathering, counting down or played: its standings
            // are read after it before the next song.
            host.not_before = now + PAUSE_MS;
            host.resting = None;
            return;
        }
        if busy || room.borrow().view() != RoomView::Select {
            return;
        }
        if now < host.not_before || host.songs.is_empty() {
            return;
        }
        let players = {
            let r = room.borrow();
            r.state()
                .members
                .values()
                .filter(|m| {
                    !m.left
                        && !m.is_cpu()
                        && m.connection == Connection::Connected
                        && m.intent == Intent::Play
                })
                .count()
        };
        if players < host.wait_for {
            if !host.waiting_said {
                host.waiting_said = true;
                say(format!(
                    "waiting for players ({players} of {})",
                    host.wait_for
                ));
            }
            room.borrow_mut().hover(None);
            host.resting = None;
            return;
        }
        host.waiting_said = false;
        let (song, chart) = host.songs[host.next % host.songs.len()].clone();
        room.borrow_mut().hover(Some((&song, &chart)));
        let Some(since) = host.resting else {
            host.resting = Some(now);
            say(format!(
                "resting on {} ({} {})",
                song.title,
                chart.difficulty.name(),
                chart.meter
            ));
            return;
        };
        if now < since + host.rest_ms {
            return;
        }
        let number = room.borrow().next_pick_number();
        match room.borrow_mut().pick(&song, &chart) {
            Ok(()) => {
                say(format!("picked {} for stage {number}", song.title));
                host.next += 1;
                host.resting = None;
                // Nothing more until the room has moved on (its stage then
                // holds it back until after the standings).
                host.not_before = now + PAUSE_MS;
            }
            Err(why) => {
                say(format!("the pick was refused: {why:?}"));
                room.borrow_mut().clear_refusal();
                host.not_before = now + RETRY_MS;
            }
        }
    }
}

/// Stage `stage`'s number in its match (0: unknown).
fn stage_number(room: &SharedRoom, stage: StageId) -> usize {
    let r = room.borrow();
    let s = r.state();
    s.current
        .as_ref()
        .and_then(|m| m.stage_index(stage))
        .or_else(|| s.playlist.iter().position(|st| st.id == stage))
        .map_or(0, |i| i + 1)
}

/// `inbox`, with each stage's shared song checked for the bot's CPU: a
/// chart it cannot plan fails to load (the bot sits the stage out) rather
/// than being readied for.
fn checked(mut inbox: Inbox, profile: Profile) -> Inbox {
    Box::new(move || {
        inbox()
            .into_iter()
            .map(|news| match news {
                ShellNews::PreparedShared {
                    job,
                    result: Ok(picked),
                } if job.stage != SPOTLIGHT => {
                    let playable = crate::play::load::read_chart(&picked.song.id)
                        .map_err(|e| format!("{e:?}"))
                        .and_then(|song| {
                            cpu_for(
                                &song,
                                picked.chart.index,
                                ddi_room::MatchRules::DEFAULT_RULESET,
                                &profile,
                                1,
                            )
                            .map(|_| ())
                        });
                    ShellNews::PreparedShared {
                        job,
                        result: match playable {
                            Ok(()) => Ok(picked),
                            Err(e) => {
                                eprintln!("bots: {}: the CPU cannot play it: {e}", profile.name);
                                Err(LoadProblem::CpuCannotPlay)
                            }
                        },
                    }
                }
                other => other,
            })
            .collect()
    })
}

/// The host's songs: the fixed list, else every song of the library (or
/// of `--songs`' folders alone) with a chart the CPU plays, in an order
/// drawn from `seed`.
fn host_songs(o: &Options, seed: u64) -> Vec<(SongEntry, ChartEntry)> {
    let songs = crate::songs::Songs::new(o.songs.clone());
    songs.start(std::sync::Arc::new(|| {}));
    if let Some(l) = songs.library.borrow_mut().as_mut() {
        l.wait();
    }
    let roots: Vec<PathBuf> = o
        .songs
        .iter()
        .map(|d| d.canonicalize().unwrap_or_else(|_| d.clone()))
        .collect();
    // The bundled songs (unless `--songs` names the folders), then the
    // folders' songs.
    let mut library: Vec<ddi_library::manifest::ManifestEntry> = if roots.is_empty() {
        crate::library::bundled()
    } else {
        Vec::new()
    };
    if let Some(l) = songs.library.borrow().as_ref() {
        library.extend(
            l.songs
                .iter()
                .filter(|l| {
                    roots.is_empty() || {
                        let r = l.root.canonicalize().unwrap_or_else(|_| l.root.clone());
                        roots.iter().any(|d| r.starts_with(d))
                    }
                })
                .map(|l| l.entry.clone()),
        );
    }
    let profile = Profile {
        name: String::new(),
        level: 2,
        skills: "balanced".into(),
    };
    let playable = |song: &SongEntry, chart: &ChartEntry| {
        crate::play::load::read_chart(&song.id)
            .map_err(|e| format!("{e:?}"))
            .and_then(|s| {
                cpu_for(
                    &s,
                    chart.index,
                    ddi_room::MatchRules::DEFAULT_RULESET,
                    &profile,
                    1,
                )
                .map(|_| ())
            })
            .is_ok()
    };
    let entry = ddi_ui::state::select_view::plain_song_entry;
    if !o.picks.is_empty() {
        return o
            .picks
            .iter()
            .filter_map(|(id, chart)| {
                let Some(lib) = library.iter().find(|l| l.id == *id) else {
                    eprintln!("bots: no song {id} in the library");
                    return None;
                };
                let song = entry(lib);
                let chart = choose_chart(&song, *chart).filter(|c| playable(&song, c));
                if chart.is_none() {
                    eprintln!(
                        "bots: song {id} has no chart the CPU plays (one player, four lanes)"
                    );
                }
                Some((song, chart?))
            })
            .collect();
    }
    let mut out: Vec<(SongEntry, ChartEntry)> = library
        .iter()
        .filter_map(|l| {
            let song = entry(l);
            let chart = choose_chart(&song, None).filter(|c| playable(&song, c))?;
            Some((song, chart))
        })
        .collect();
    out.sort_by_key(|(s, _)| mix(seed, s.id.bytes().fold(0u64, |h, b| mix(h, u64::from(b)))));
    out
}

/// Ctrl-C's count: once leaves the rooms, twice ends at once.
static STOP: AtomicUsize = AtomicUsize::new(0);

#[cfg(unix)]
fn catch_stop() {
    extern "C" fn on_signal(_: libc::c_int) {
        if STOP.fetch_add(1, Ordering::SeqCst) >= 1 {
            // SAFETY: `_exit` is async-signal-safe.
            unsafe { libc::_exit(130) };
        }
    }
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    // SAFETY: the handler only touches an atomic and `_exit`.
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
}

#[cfg(windows)]
fn catch_stop() {
    unsafe extern "system" fn on_ctrl(_: u32) -> windows_sys::core::BOOL {
        if STOP.fetch_add(1, Ordering::SeqCst) >= 1 {
            std::process::exit(130);
        }
        1
    }
    // SAFETY: the handler only touches an atomic and exits.
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(on_ctrl), 1);
    }
}

#[cfg(not(any(unix, windows)))]
fn catch_stop() {}

/// `ddi --mp-bots …`: the bots run until stopped; the process's exit code.
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return 0;
    }
    let o = match Options::parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            return 2;
        }
    };
    if let Some(api) = &o.api {
        crate::share::set_base(api);
    }
    let token = std::env::var("DDI_SHARE_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| ddi_ui::state::settings::Settings::load().share_token);
    let token = token.trim().to_string();
    if token.is_empty() {
        eprintln!("bots: no admin share token (DDI_SHARE_TOKEN, or the settings' share token)");
        return 2;
    }
    run_with(o, &token)
}

/// The bots of `o`, signed in with the admin `token`, until stopped or
/// their matches are played; the exit code.
pub fn run_with(o: Options, token: &str) -> i32 {
    let base = crate::share::base();
    let site = o.site.clone().unwrap_or_else(crate::online::site);
    let seed = o.seed.unwrap_or_else(now_ms);
    let count = o.guests + usize::from(o.host);
    let profiles = profiles(&o, count, seed);
    let mut slugs: Vec<String> = profiles.iter().map(|p| p.name.to_lowercase()).collect();
    slugs.sort();
    slugs.dedup();
    if slugs.len() < count {
        eprintln!("bots: two bots with one name");
        return 2;
    }
    println!(
        "{} bots: {} against {base} (DEVELOPMENT TOOL; Ctrl-C leaves)",
        clock_text(),
        profiles
            .iter()
            .map(|p| format!("{} (level {}, {})", p.name, p.level + 1, p.skills))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let songs = if o.host {
        let songs = host_songs(&o, seed);
        if songs.is_empty() {
            eprintln!("bots: the host has no song the CPU plays (one player, four lanes)");
            return 1;
        }
        println!("{} bots: the host's songs: {}", clock_text(), songs.len());
        songs
    } else {
        Vec::new()
    };
    let mut bots = Vec::new();
    for (n, profile) in profiles.into_iter().enumerate() {
        let session = match sign_in(&base, token, &profile.name) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("bots: {} could not sign in: {e}", profile.name);
                return 1;
            }
        };
        let online = Online::new(
            Box::new(crate::online::ThreadHttp::with_base(base.clone())),
            OnlineConfig {
                platform: Platform::Desktop,
                site: site.clone(),
            },
        )
        .with_token(Some(session), Rc::new(|_: Option<&str>| {}))
        .shared();
        let role = if o.host && n == 0 {
            Role::Host(Box::new(Host {
                songs: songs.clone(),
                next: 0,
                resting: None,
                rules: false,
                stages: o.stages,
                rest_ms: o.rest_ms,
                wait_for: o.wait_for,
                not_before: 0,
                waiting_said: false,
            }))
        } else {
            Role::Guest(o.join.clone())
        };
        bots.push(Bot {
            seed: mix(seed, n as u64 + 1),
            profile,
            role,
            online,
            asked: false,
            room: None,
            news: None,
            order: None,
            ready_at: 0,
            play: None,
            matches: 0,
            said: Said::default(),
            done: false,
        });
    }
    catch_stop();
    let mut leaving: Option<u64> = None;
    loop {
        let now = now_ms();
        // Guests of the host bot's room: its invite once it has one.
        let invite = bots.first().and_then(|b| match &b.role {
            Role::Host(_) => b.host_invite(),
            Role::Guest(_) => None,
        });
        for b in &mut bots {
            if let (Role::Guest(slot @ None), Some(i)) = (&mut b.role, &invite) {
                *slot = Some(i.clone());
            }
            b.tick(now, &base, o.matches);
        }
        if STOP.load(Ordering::SeqCst) > 0 && leaving.is_none() {
            leaving = Some(now);
            for b in &mut bots {
                b.stop();
            }
        }
        let over = bots.iter().all(|b| b.done);
        if over || leaving.is_some_and(|t| now > t + LEAVE_MS) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let left = bots.iter().filter(|b| b.done).count();
    println!("{} bots: {left} of {} left", clock_text(), bots.len());
    i32::from(leaving.is_none() && o.matches.is_none())
}

impl Bot {
    /// Ctrl-C: a stage being played forfeited, then the room left.
    fn stop(&mut self) {
        let Some(room) = self.room.clone() else {
            self.done = true;
            return;
        };
        if let (Some(play), Some(order)) = (self.play.take(), self.order.take()) {
            self.say(format!("stage {}: forfeit (stopped)", play.number));
            let _ = room.borrow_mut().report_run(order.stage, Run::Forfeit);
        }
        if room.borrow().link_state().ended() {
            self.done = true;
            return;
        }
        self.say("leaving");
        room.borrow_mut().leave();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn the_flags_are_read_and_refused_where_they_make_no_sense() {
        let o = Options::parse(&args("--mp-bots --host --guests 3 --stages 2 --rest 4.5")).unwrap();
        assert!(o.host && o.join.is_none());
        assert_eq!((o.guests, o.stages, o.rest_ms), (3, Some(2), 4_500));
        let o = Options::parse(&args(
            "--mp-bots --join http://localhost:8080/#/mp/join/Room000001.abcdefghijABCDEFGHIJ01?mp=1",
        ))
        .unwrap();
        assert_eq!(o.join.as_deref(), Some("Room000001.abcdefghijABCDEFGHIJ01"));
        assert_eq!(o.guests, 3, "three guests by default with an invite");
        let o = Options::parse(&args(
            "--mp-bots --host --pick beatucada:1,sweeteners --level 2 --personality jacks --names A,B --guests 1",
        ))
        .unwrap();
        assert_eq!(
            o.picks,
            vec![("beatucada".into(), Some(1)), ("sweeteners".into(), None)]
        );
        assert_eq!(
            (o.level, o.personality.as_deref()),
            (Some(2), Some("jacks"))
        );
        for bad in [
            "--mp-bots",
            "--mp-bots --guests 2",
            "--mp-bots --join nonsense",
            "--mp-bots --host --guests 8",
            "--mp-bots --host --level 9",
            "--mp-bots --host --personality wobbly",
            "--mp-bots --host --stages 0",
            "--mp-bots --host --guests 2 --names A,B",
            "--mp-bots --host --frobnicate",
            "--mp-bots --host --rest",
        ] {
            assert!(Options::parse(&args(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn profiles_come_from_the_roster_or_the_flags() {
        let o = Options::parse(&args("--mp-bots --host --guests 3")).unwrap();
        let a = profiles(&o, 4, 7);
        assert_eq!(a, profiles(&o, 4, 7), "the same seed, the same bots");
        let roster = ddi_ui::state::cpu_roster::catalog();
        for p in &a {
            let c = roster
                .iter()
                .find(|c| c.name == p.name)
                .expect("a roster CPU");
            assert_eq!(p.level + 1, usize::from(c.level));
            assert_eq!(p.skills, c.skills);
        }
        let mut names: Vec<&str> = a.iter().map(|p| p.name.as_str()).collect();
        names.dedup();
        assert_eq!(names.len(), 4);
        let o = Options::parse(&args(
            "--mp-bots --host --guests 1 --names Alpha,Beta --level 5 --personality stream",
        ))
        .unwrap();
        let b = profiles(&o, 2, 7);
        assert_eq!(b[0].name, "Alpha");
        assert_eq!((b[1].level, b[1].skills.as_str()), (4, "stream"));
    }

    fn beatucada() -> ddi_chart::Song {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
        let path = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "ssc"))
            .unwrap();
        ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap()
    }

    #[test]
    fn a_stage_is_played_on_the_song_clock_and_reported_once_over() {
        let song = beatucada();
        let chart = (0..song.charts.len())
            .find(|&i| {
                song.layout_of(&song.charts[i])
                    .is_some_and(|l| l.lanes.len() == 4 && l.players == 1)
            })
            .unwrap();
        let order = StageOrder {
            stage: StageId(3),
            number: 1,
            of: 2,
            song: SongEntry::default(),
            chart: ChartEntry::default(),
            battle: ddi_room::BattlePreset::GaugeSurvivalV0,
            ruleset: "itg".into(),
            dev_mines_at_beat: None,
            me: ddi_room::ParticipantId(2),
            cpus: Vec::new(),
            others: 3,
        };
        let profile = Profile {
            name: "T".into(),
            level: 4,
            skills: "balanced".into(),
        };
        let mut play = StagePlay::new(&order, &song, chart, &profile, 9, 1_000).unwrap();
        assert_eq!(play.song_time(1_000), -LEAD_S);
        assert_eq!(play.advance(1_000), None, "nothing at the start");
        assert_eq!(play.advance(31_000), None, "the song goes on");
        // Frame by frame to its end: one run, its life and accuracy.
        let mut t = 31_000;
        let run = loop {
            t += 16;
            if let Some(run) = play.advance(t) {
                break run;
            }
            assert!(t < 1_000_000, "the song ends");
        };
        match run {
            Run::Live { life, accuracy } => {
                assert!((0.0..=1.0).contains(&life) && life > 0.0);
                assert!(accuracy > 0.5 && accuracy <= 1.0, "{accuracy}");
            }
            Run::Out { at } => assert!(at > 0.0),
            Run::Forfeit => panic!("never a forfeit"),
        }
        // A chart the CPU cannot plan is refused.
        assert!(cpu_for(&song, 99, "itg", &profile, 1).is_err());
    }

    #[test]
    fn the_host_picks_one_original_four_lane_chart_a_song() {
        let chart = |index, style: &str, meter, digest: bool, version| ChartEntry {
            index,
            style: style.into(),
            meter,
            digest: digest.then(|| "0".repeat(32).parse().unwrap()),
            version,
            ..ChartEntry::default()
        };
        let song = SongEntry {
            charts: vec![
                chart(0, "dance-single", 3, true, None),
                chart(1, "dance-single", 8, true, None),
                chart(2, "dance-double", 7, true, None),
                chart(3, "dance-single", 7, false, None),
            ],
            ..SongEntry::default()
        };
        assert_eq!(choose_chart(&song, None).map(|c| c.index), Some(1));
        assert_eq!(choose_chart(&song, Some(0)).map(|c| c.index), Some(0));
        assert_eq!(choose_chart(&song, Some(2)), None, "not four lanes");
        assert_eq!(choose_chart(&song, Some(3)), None, "no digest");
    }
}
