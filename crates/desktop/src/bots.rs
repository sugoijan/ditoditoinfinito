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
//! flags: `--level N` puts every bot at level N, `--level A-B` spreads
//! them over that range, [`spread_levels`]) judged on a real-time song
//! clock: never a skill-tuned player.
//! In a room that plays live (`docs/plans/multiplayer-online-play.md`) a
//! bot plays as a person's device does: it offers or compares the battle
//! plan, says it plays live, starts its song clock at the room's instant,
//! streams its moments and checkpoint reports and takes the room's attacks
//! into its CPU's play (`ddi_session::battle::live::LivePlay`).
//!
//! A host bot (`--host`) makes a room, prints its invite, accepts its
//! Rules, rests on each song before picking it (so that the others see and
//! hear it), and plays too. It picks while the pick is its own (a room's
//! owner has it at first, `ddi_room::Picking`): a person who takes the
//! pick over chooses the songs until they give it up, the bot following
//! like anyone and never taking it back from them; it takes it only when
//! nobody has it; guest bots (`--guests N`) join the host bot's
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

/// Bots in one run at most: a room's whole field (the cap, 73; the
/// server keeps a room to as many bots).
pub const MAX_BOTS: usize = ddi_room::RoomPolicy::INITIAL_COMPETITOR_CAP as usize;
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
    /// The bots' levels, from 1, over the roster's: an inclusive range
    /// (one level: every bot at it), spread over the bots in an order
    /// drawn from the seed ([`spread_levels`]).
    pub level: Option<(u8, u8)>,
    /// A personality for every bot, over the roster's.
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
    /// Seats in the room's audience that only watch (`--audience N`,
    /// `docs/plans/multiplayer-audience.md`): no play, nothing sent but
    /// what a seat sends; each reports what it received.
    pub audience: usize,
    /// The host's Rules: CPUs of the match's roster (`--cpus N[:LEVEL]`,
    /// `online-cpus-v0`: no socket plays them; every device, the bots'
    /// included, computes them), at one level from 1 where given.
    pub cpus: Option<(usize, Option<u8>)>,
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
            audience: 0,
            cpus: None,
        }
    }
}

pub const USAGE: &str = "\
ddi --mp-bots [--host] [--join LINK|INVITE] [--guests N] [options]
  Simulated players (DEVELOPMENT TOOL) in a private online room, played by
  the CPU; the server signs them in for an admin share token (DDI_SHARE_TOKEN,
  else the settings' share token). In a room that plays live they play live:
  their plays streamed, the room's attacks taken. Ctrl-C leaves.
  --host               a host bot: makes a room, prints its invite, picks songs
  --join LINK|INVITE   guest bots join this room (else the host bot's)
  --guests N           guest bots (default 3 with --join, else 0; at most 73 bots)
  --api URL            the server (else DDI_API_URL, else the production one)
  --site URL           the game's page for invite links
  --names A,B,…        the bots' names (else the CPU roster's)
  --level N | A-B      every bot's CPU level N (1 to 10), or levels spread over
                       A to B inclusive in an order drawn from --seed (else
                       the roster's)
  --personality P      every bot's personality: a preset or seven multipliers
  --songs DIR          the host's songs from this folder (repeatable)
  --pick ID[:N],…      the host's songs in this order (chart index N)
  --rest SECONDS       the host's rest on a song before picking it (default 8)
  --stages N           the host's Rules: stages a match
  --matches N          stop after N matches
  --wait-for N         players the host waits for before picking (default 2)
  --seed N             names' order and plays
  --cpus N[:LEVEL]     the host's Rules: N CPUs of the roster beside the bots
                       (CPUs, never bots: every device computes them and
                       reports for them), all at LEVEL (1 to 10) where given
  --audience N         N seats in the room's audience by --join's link (its
                       audience link, or its invite at the member bound): they
                       only watch (no play) and report what they received,
                       two seats a signed-in bot (at most 5000)";

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
                "--level" => o.level = Some(parse_levels(value(i, a)?)?),
                "--cpus" => {
                    let v = value(i, a)?;
                    let (n, level) = match v.split_once(':') {
                        Some((n, l)) => (n, Some(l)),
                        None => (v.as_str(), None),
                    };
                    let n = n.parse::<usize>().map_err(|_| "--cpus takes N or N:LEVEL")?;
                    let level = match level {
                        Some(l) => Some(
                            l.parse::<u8>()
                                .ok()
                                .filter(|l| (1..=10).contains(l))
                                .ok_or("--cpus: LEVEL is 1 to 10")?,
                        ),
                        None => None,
                    };
                    o.cpus = Some((n, level));
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
                        .filter(|n| (2..=MAX_BOTS).contains(n))
                        .ok_or("--wait-for takes 2 to 73")?
                }
                "--audience" => {
                    o.audience = value(i, a)?
                        .parse::<usize>()
                        .ok()
                        .filter(|n| (1..=MAX_AUDIENCE).contains(n))
                        .ok_or("--audience takes 1 to 5000")?
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
        o.guests = guests.unwrap_or(if o.join.is_some() && o.audience == 0 {
            3
        } else {
            0
        });
        if o.audience > 0 && o.join.is_none() {
            return Err("--audience needs --join".into());
        }
        if o.audience > 0 && !o.host && o.guests == 0 {
            return Ok(o);
        }
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

/// A bot's CPU: its name, its level (an index of the engine's ten levels)
/// and its personality (`ddi_engine::cpu::Skills`).
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    pub level: usize,
    pub skills: String,
}

/// `--level`'s value: one level (`7`, every bot at it) or an inclusive
/// range (`5-10`), each from 1 to the engine's ten.
fn parse_levels(v: &str) -> Result<(u8, u8), String> {
    let levels = ddi_engine::cpu::LEVELS as u8;
    let bad = || format!("--level takes a level from 1 to {levels}, or a range such as 5-{levels}");
    let level = |s: &str| {
        s.trim()
            .parse::<u8>()
            .ok()
            .filter(|l| (1..=levels).contains(l))
    };
    let (lo, hi) = match v.split_once('-') {
        Some((a, b)) => (level(a).ok_or_else(bad)?, level(b).ok_or_else(bad)?),
        None => {
            let l = level(v).ok_or_else(bad)?;
            (l, l)
        }
    };
    if lo > hi {
        return Err(format!("--level {v}: the range goes from the lower level to the higher"));
    }
    Ok((lo, hi))
}

/// `count` bots' levels over `lo..=hi`, spread rather than clumped: with
/// at least as many bots as levels every level is used (none more than
/// once more than another), with fewer the bots take distinct levels
/// evenly apart; which bot gets which, and with fewer bots where the
/// levels sit, drawn from `seed`.
pub fn spread_levels(lo: u8, hi: u8, count: usize, seed: u64) -> Vec<u8> {
    let span = usize::from(hi - lo) + 1;
    let mut x = mix(seed, 0x6c65_7665_6c73) | 1;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut levels: Vec<u8> = if count >= span {
        (0..count).map(|n| lo + (n % span) as u8).collect()
    } else {
        let off = (next() % count.max(1) as u64) as usize;
        (0..count)
            .map(|n| lo + ((n * span + off) / count) as u8)
            .collect()
    };
    for i in (1..levels.len()).rev() {
        levels.swap(i, (next() % (i as u64 + 1)) as usize);
    }
    levels
}

/// The run's bots' profiles: the names given, else the CPU roster's in an
/// order drawn from `seed`; each its roster level and personality unless
/// the options set them (a named bot not on the roster: level 8,
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
    let levels = o
        .level
        .map(|(lo, hi)| spread_levels(lo, hi, count, seed));
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
                .or_else(|| {
                    // Past the roster's 24, its names again, numbered
                    // plainly ("Lumen Kite 2"): each bot its own identity.
                    from_roster.map(|c| match n / roster.len() {
                        0 => c.name.clone(),
                        round => format!("{} {}", c.name, round + 1),
                    })
                })
                .unwrap_or_else(|| format!("Bot {}", n + 1));
            let level = levels
                .as_ref()
                .and_then(|l| l.get(n).copied())
                .or_else(|| from_roster.map(|c| c.level))
                .unwrap_or(8);
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
    /// The most the whole chart gives (DDI): the scale its score is
    /// reported on (`ddi_session::battle::cpu_score`, `CPU_SCORE_V0`: a
    /// bot plays the CPU's tap-only copy beside people on the chart).
    most: Option<i64>,
    started_ms: u64,
    /// The stage played live (`ddi_session::battle::live`): its stream to
    /// the room and the room's attacks into its CPU's play.
    live: Option<ddi_session::battle::live::LivePlay>,
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
        let mut opponent = cpu_for(song, chart, &order.rules, profile, seed)?;
        opponent.start(0.0);
        let most = ddi_session::battle::chart_most(
            song,
            chart,
            &ddi_session::battle::stage_ruleset(&order.rules),
        );
        Ok(StagePlay {
            stage: order.stage,
            number: order.number,
            opponent,
            most,
            started_ms: now_ms,
            live: None,
        })
    }

    /// Played live in `room` (its stream sent, the room's attacks taken),
    /// its song clock on the room's: song time zero at the room's instant.
    pub fn go_live(
        &mut self,
        room: &SharedRoom,
        order: &StageOrder,
        song: &ddi_chart::Song,
        now_ms: u64,
    ) -> Result<(), String> {
        let handle = ddi_session::battle::live::LiveHandle {
            room: room.clone(),
            wall_ms: Rc::new(now_ms_fn),
        };
        self.live =
            ddi_session::battle::live::LivePlay::new(handle, order, song, order.chart.index)?;
        if self.live.is_some()
            && let Some(zero_in) = room.borrow().song_zero_in(order.stage)
        {
            // `song_time` reads `LEAD_S` before the start: the start put
            // there so that song time zero is the room's instant.
            let start = now_ms as f64 + zero_in * 1_000.0 - LEAD_S * 1_000.0;
            self.started_ms = start.max(0.0) as u64;
        }
        Ok(())
    }

    /// The song's second at `now_ms`.
    pub fn song_time(&self, now_ms: u64) -> f64 {
        (now_ms as f64 - self.started_ms as f64) / 1000.0 - LEAD_S
    }

    /// Played up to `now_ms`; its run once it is over (out, or the song
    /// played to its last note and a little more).
    pub fn advance(&mut self, now_ms: u64) -> Option<Run> {
        let t = self.song_time(now_ms);
        self.opponent.advance(t);
        if let Some(live) = &mut self.live {
            // Its stream reads the play as judged; the room's attacks go
            // into it.
            let judged = self.opponent.take_judged();
            live.send(t, self.opponent.player(), &judged);
            let opponent = &mut self.opponent;
            let mut deliver = |cells: &[ddi_engine::battle::Hazard]| {
                opponent.receive_hazards(cells).is_ok()
            };
            live.receive(t, &mut deliver);
        }
        if let Some(at) = self.opponent.out_at() {
            return Some(Run::Out { at });
        }
        let end = self.opponent.player().end_time();
        let over = (self.opponent.finished() && t >= end + TAIL_S) || t >= end + 30.0;
        if over && let Some(live) = &mut self.live {
            live.end(ddi_room::live::StreamEnd::Finished, t);
        }
        over.then(|| {
            ddi_session::battle::run_of(
                None,
                self.opponent.life(),
                &ddi_session::battle::cpu_score(&self.opponent.score(), self.most),
                self.opponent.player().steps().len(),
            )
        })
    }
}

impl StagePlay {
    /// How the bot performed so far (`ddi_room::RunDetail`), as judged by
    /// its own play.
    pub fn detail(&self) -> ddi_room::RunDetail {
        let player = self.opponent.player();
        let mut results = player.results_brief();
        results.score = ddi_session::battle::cpu_score(&results.score, self.most);
        ddi_ui::state::room::run_detail(&results, player.ruleset(), self.opponent.life(), None)
    }
}

/// `profile`'s CPU on chart `chart` of `song` under the match's rules
/// `rules` (`ddi_session::battle::stage_ruleset`), or the engine's reason
/// why it cannot play it.
fn cpu_for(
    song: &ddi_chart::Song,
    chart: usize,
    rules: &ddi_session::battle::StageRules,
    profile: &Profile,
    seed: u64,
) -> Result<Opponent, String> {
    song.charts.get(chart).ok_or("no such chart")?;
    let rules = ddi_session::battle::stage_ruleset(rules);
    let skills = Skills::parse(&profile.skills).unwrap_or_default();
    let level = profile.level.min(ddi_engine::cpu::LEVELS - 1);
    Opponent::with_skills(song, chart, rules, level, seed, skills)
}

/// A run in a line of the log.
fn run_text(run: &Run) -> String {
    match run {
        Run::Live { life, accuracy, .. } => {
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

fn now_ms_fn() -> u64 {
    now_ms()
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
    cpus: Option<(usize, Option<u8>)>,
    rest_ms: u64,
    wait_for: usize,
    not_before: u64,
    waiting_said: bool,
    /// Someone else chooses the songs (they took the pick over): who, as
    /// last said.
    following: Option<Option<ddi_room::ParticipantId>>,
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
            LinkState::UpdateRequired => {
                self.say("this room needs a newer version of the game");
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
                "following the picker's song: {}",
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
            let run = play.advance(now);
            // What became of the attacks against it (taken, or refused and
            // why), and its live part as it started.
            let lines = play.live.as_mut().map(|l| l.take_log()).unwrap_or_default();
            for line in lines {
                self.say(line);
            }
            let Some(play) = &mut self.play else {
                return;
            };
            let Some(run) = run else {
                return;
            };
            let number = play.number;
            // How it performed, for the stage's standings, as a player's
            // report says it.
            let detail = play.detail();
            self.play = None;
            self.order = None;
            self.say(format!("stage {number}: {}", run_text(&run)));
            if let Err(why) = room.borrow_mut().report_run(order.stage, run, Some(detail)) {
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
                    let mut play =
                        StagePlay::new(&order, &song, order.chart.index, &self.profile, seed, now)?;
                    play.go_live(room, &order, &song, now)?;
                    Ok(play)
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
                    let _ = room
                        .borrow_mut()
                        .report_run(order.stage, Run::Forfeit, None);
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
            if let Some((n, level)) = host.cpus {
                if room.borrow().cpus_offered() {
                    let mut roster =
                        ddi_ui::state::cpu_roster::starting(n, mix(now, n as u64));
                    // The roster's names run out before the cap: the rest
                    // are its common profile's.
                    while roster.len() < n {
                        roster.push(ddi_room::OpponentSpec {
                            level: 4,
                            seed: mix(now, roster.len() as u64),
                            personality: None,
                            name: None,
                            targeting: None,
                        });
                    }
                    if let Some(l) = level {
                        for c in &mut roster {
                            c.level = l - 1;
                        }
                    }
                    let seats = rules.seats;
                    let humans = room.borrow().humans();
                    rules = ddi_ui::state::cpu_roster::with_roster(rules, roster, humans);
                    rules.seats = rules.seats.max(seats);
                    say(format!("Rules: {n} CPUs of the roster"));
                } else {
                    say("Rules: this room's server cannot seat CPUs online".into());
                }
            }
            let stages = rules.stages;
            match room.borrow_mut().save_rules(rules) {
                Ok(()) => say(format!("Rules: OK ({stages} stages a match)")),
                Err(why) => say(format!("Rules refused: {why:?}")),
            }
            host.not_before = now + 2_000;
        }
        // One picker at a time: the bot picks while the pick is its own.
        // Someone who took it over chooses until they give it up (they
        // leave, lose the role, are gone too long: the room hands it back
        // to its owner, the bot); the bot never takes it from anyone, only
        // when nobody has it.
        let (chooses, picker, may_take) = {
            let r = room.borrow();
            (r.chooses(), r.picker(), r.may_take_pick())
        };
        if !chooses {
            if host.following != Some(picker) {
                host.following = Some(picker);
                let name = picker
                    .and_then(|id| {
                        room.borrow().member_of(id).map(|m| match &m.kind {
                            ddi_room::MemberKind::Human { name, .. } => name.clone(),
                            ddi_room::MemberKind::Cpu { ordinal, .. } => format!("CPU {ordinal}"),
                        })
                    })
                    .unwrap_or_else(|| "nobody".into());
                say(format!("{name} chooses the songs now"));
            }
            if picker.is_none() && may_take {
                let _ = room.borrow_mut().take_pick();
            }
            room.borrow_mut().hover(None);
            host.resting = None;
            return;
        }
        if host.following.take().is_some() {
            say("choosing the songs again".into());
            host.not_before = host.not_before.max(now + 2_000);
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
                    live,
                } if job.stage != SPOTLIGHT => {
                    let playable = crate::play::load::read_chart(&picked.song.id)
                        .map_err(|e| format!("{e:?}"))
                        .and_then(|song| {
                            cpu_for(
                                &song,
                                picked.chart.index,
                                &Default::default(),
                                &profile,
                                1,
                            )
                            .map(|_| ())
                        });
                    ShellNews::PreparedShared {
                        job,
                        live,
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
        level: 7,
        skills: "balanced".into(),
    };
    let playable = |song: &SongEntry, chart: &ChartEntry| {
        crate::play::load::read_chart(&song.id)
            .map_err(|e| format!("{e:?}"))
            .and_then(|s| {
                cpu_for(&s, chart.index, &Default::default(), &profile, 1)
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
    if o.audience > 0 && !o.host && o.guests == 0 {
        return run_audience(&o, token);
    }
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
                cpus: o.cpus,
                rest_ms: o.rest_ms,
                wait_for: o.wait_for,
                not_before: 0,
                waiting_said: false,
                following: None,
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
            let _ = room
                .borrow_mut()
                .report_run(order.stage, Run::Forfeit, None);
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
            (Some((2, 2)), Some("jacks"))
        );
        let o = Options::parse(&args("--mp-bots --host --guests 3 --level 5-10")).unwrap();
        assert_eq!(o.level, Some((5, 10)));
        // Audience seats alone, by an audience link.
        let o = Options::parse(&args(
            "--mp-bots --audience 40 --join http://localhost:8080/#/mp/audience/Room000001.abcdefghijABCDEFGHIJ01?mp=1",
        ))
        .unwrap();
        assert_eq!((o.audience, o.guests), (40, 0), "no guests by default beside an audience");
        assert_eq!(o.join.as_deref(), Some("Room000001.abcdefghijABCDEFGHIJ01"));
        for bad in [
            "--mp-bots --audience 3",
            "--mp-bots --audience 0 --join Room000001.abcdefghijABCDEFGHIJ01",
            "--mp-bots --audience 5001 --join Room000001.abcdefghijABCDEFGHIJ01",
            "--mp-bots",
            "--mp-bots --guests 2",
            "--mp-bots --join nonsense",
            "--mp-bots --host --guests 73",
            "--mp-bots --host --level 11",
            "--mp-bots --host --level 0",
            "--mp-bots --host --level 7-3",
            "--mp-bots --host --level 0-4",
            "--mp-bots --host --level 3-11",
            "--mp-bots --host --level 3-",
            "--mp-bots --host --level 3..5",
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
        // A range: the bots spread over it, the same for the same seed.
        let o = Options::parse(&args("--mp-bots --host --guests 5 --level 5-10")).unwrap();
        let c = profiles(&o, 6, 7);
        let mut levels: Vec<usize> = c.iter().map(|p| p.level + 1).collect();
        assert_eq!(c, profiles(&o, 6, 7));
        levels.sort_unstable();
        assert_eq!(levels, vec![5, 6, 7, 8, 9, 10]);
        // A whole field of bots: every name its own, the roster's again
        // numbered past its 24.
        let o = Options::parse(&args("--mp-bots --host --guests 72")).unwrap();
        let all = profiles(&o, 73, 7);
        let names: std::collections::BTreeSet<&str> =
            all.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names.len(), 73);
        assert!(all[24..].iter().all(|p| p.name.ends_with(" 2") || p.name.ends_with(" 3") || p.name.ends_with(" 4")));
        assert!(Options::parse(&args("--mp-bots --host --guests 73")).is_err());
    }

    /// A range's levels: every one used where there are enough bots, none
    /// more than once more than another; distinct where there are fewer;
    /// always within the range; the order (and where fewer sit) by seed.
    #[test]
    fn a_level_range_is_spread_over_the_bots() {
        for (lo, hi) in [(1u8, 10u8), (5, 10), (3, 4), (7, 7)] {
            let span = usize::from(hi - lo) + 1;
            for count in 1..=8 {
                for seed in 0..20 {
                    let l = spread_levels(lo, hi, count, seed);
                    assert_eq!(l.len(), count);
                    assert!(l.iter().all(|x| (lo..=hi).contains(x)), "{l:?}");
                    let uses: Vec<usize> = (lo..=hi)
                        .map(|v| l.iter().filter(|x| **x == v).count())
                        .collect();
                    if count >= span {
                        let (min, max) = (uses.iter().min().unwrap(), uses.iter().max().unwrap());
                        assert!(*min >= 1 && max - min <= 1, "{lo}-{hi} x{count}: {l:?}");
                    } else {
                        assert!(uses.iter().all(|u| *u <= 1), "{lo}-{hi} x{count}: {l:?}");
                    }
                }
            }
        }
        assert_eq!(spread_levels(5, 10, 4, 3), spread_levels(5, 10, 4, 3));
        let orders: std::collections::BTreeSet<Vec<u8>> =
            (0..20).map(|s| spread_levels(5, 10, 6, s)).collect();
        assert!(orders.len() > 5, "the seed decides the order");
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
            attacks_offered: false,
            rules: Default::default(),
            dev_mines_at_beat: None,
            me: ddi_room::ParticipantId(2),
            targeting: Default::default(),
            cpus: Vec::new(),
            others: 3,
            watching: false,
            live: None,
        };
        let profile = Profile {
            name: "T".into(),
            level: 9,
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
            Run::Live {
                life,
                accuracy,
                absolute,
                score,
                ..
            } => {
                assert!((0.0..=1.0).contains(&life) && life > 0.0);
                assert!(accuracy > 0.5 && accuracy <= 1.0, "{accuracy}");
                // Under DDI its report carries the score, the sum on the
                // chart's own scale (`CPU_SCORE_V0`), and its absolute
                // part with that scale's most.
                let a = absolute.expect("DDI");
                assert!(a.most > 0);
                let ratio = (accuracy * 1_000_000.0).round() as i64;
                assert_eq!(score, Some(ratio + a.score));
            }
            Run::Out { at } => assert!(at > 0.0),
            Run::Forfeit => panic!("never a forfeit"),
        }
        // A chart the CPU cannot plan is refused.
        assert!(cpu_for(&song, 99, &Default::default(), &profile, 1).is_err());
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

/// `--audience`'s bound: seats one run opens.
pub const MAX_AUDIENCE: usize = 5_000;

/// What an audience seat received, for its report.
#[derive(Default)]
struct Watched {
    stages: std::collections::BTreeSet<u64>,
    last_count: Option<u32>,
}

/// `--audience N` alone (DEVELOPMENT TOOL): N seats in the room's audience
/// by `--join`'s link, two a signed-in bot, that only watch: each one's
/// seat over the shell's socket, ticked until Ctrl-C (or the audience's
/// end); a line every ten seconds (seats watching, frames received, the
/// count as said, the room's phase) and each seat's totals at the end.
fn run_audience(o: &Options, token: &str) -> i32 {
    use ddi_ui::state::audience::{AudienceClient, SeatState};
    let base = crate::share::base();
    let site = o.site.clone().unwrap_or_else(crate::online::site);
    let link = o.join.clone().unwrap_or_default();
    let seed = o.seed.unwrap_or_else(now_ms);
    let people = o.audience.div_ceil(2);
    println!(
        "{} audience: {} seats by {link} against {base}, {people} bots signed in (DEVELOPMENT TOOL; Ctrl-C leaves)",
        clock_text(),
        o.audience
    );
    let mut seats = Vec::new();
    for k in 0..people {
        let name = format!("Watcher {:x}{}", seed % 0xffff, k + 1);
        let session = match sign_in(&base, token, &name) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("audience: {name} could not sign in: {e}");
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
        online.borrow_mut().check();
        for _ in 0..2.min(o.audience - seats.len()) {
            seats.push((
                AudienceClient::new(
                    online.clone(),
                    &link,
                    Box::new(crate::online::SocketTransport::audience(Some(base.clone()))),
                    None,
                    now_ms(),
                ),
                Watched::default(),
            ));
        }
    }
    catch_stop();
    let mut said_at = 0;
    loop {
        let now = now_ms();
        for (a, w) in &mut seats {
            a.tick(now);
            // Nothing is played: the songs are never read.
            let _ = a.take_load_jobs();
            if let Some(st) = a.current_stage() {
                w.stages.insert(st.0);
            }
            w.last_count = a.count();
        }
        let ended = seats
            .iter()
            .filter(|(a, _)| matches!(a.state(), SeatState::Ended(_) | SeatState::Refused(_)))
            .count();
        if now >= said_at {
            said_at = now + 10_000;
            let watching = seats
                .iter()
                .filter(|(a, _)| a.state() == SeatState::Watching)
                .count();
            let frames: u64 = seats.iter().map(|(a, _)| u64::from(a.frames())).sum();
            let count = seats.iter().filter_map(|(a, _)| a.count()).max();
            let phase = seats
                .iter()
                .find_map(|(a, _)| a.view().map(|v| format!("{:?}", v.phase.kind())));
            println!(
                "{} audience: {watching} of {} watching, {frames} frames received, count {count:?}, phase {phase:?}",
                clock_text(),
                seats.len()
            );
        }
        if ended == seats.len() || STOP.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    for (k, (a, w)) in seats.iter_mut().enumerate() {
        println!(
            "{} audience: seat {}: {:?}, {} frames, {} sockets, stages {:?}, count {:?}",
            clock_text(),
            k + 1,
            a.state(),
            a.frames(),
            a.sockets(),
            w.stages,
            w.last_count
        );
        a.leave();
    }
    0
}
