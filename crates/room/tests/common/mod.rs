//! What the room's integration tests share: a song's content, a battle
//! plan of fixed sections, a competitor's scripted play. Each test file
//! takes what it uses (`mod common;`).

#![allow(dead_code)]

use ddi_room::attacks::{Judged, POWER_CLASSES_V0};
use ddi_room::live::plan::BattlePlan;
use ddi_room::live::stream::{Moment, MomentKind};
use ddi_room::{ChartKey, ContentId, SongKey};

/// Song `n`'s content.
pub fn song(n: u32) -> ContentId {
    ContentId {
        song: SongKey::new(format!("song-{n}")),
        chart: ChartKey::new(format!("digest-{n}")),
    }
}

/// A plan of `n` slots of 2 s under the attack rules named `rules`, each
/// with four shocks (a new room's Shocks per attack: the first, third,
/// sixth and eighth of eight a quarter second apart, so a section clears
/// when a section of eight did) where `room(j)`; checkpoints every 2 s
/// from 2 s.
pub fn plan_under(rules: &str, n: usize, seed: u64, room: impl Fn(usize) -> bool) -> BattlePlan {
    plan_of(rules, n, seed, 4, room)
}

/// [`plan_under`] for attacks of `shocks` shocks (2, 4, 6 or 8 of the
/// eight a quarter second apart, the first and the last always).
pub fn plan_of(
    rules: &str,
    n: usize,
    seed: u64,
    shocks: u32,
    room: impl Fn(usize) -> bool,
) -> BattlePlan {
    let kept: &[i64] = match shocks {
        2 => &[0, 7],
        4 => &[0, 2, 5, 7],
        6 => &[0, 1, 3, 4, 6, 7],
        _ => &[0, 1, 2, 3, 4, 5, 6, 7],
    };
    use ddi_room::attacks::{Cell, Schedule, Section, Slot};
    const S: i64 = 1_000_000;
    let slots = (0..n)
        .map(|j| {
            let start_us = j as i64 * 2 * S;
            Slot {
                start_us,
                section: room(j).then(|| {
                    let cells: Vec<Cell> = kept
                        .iter()
                        .map(|&i| Cell {
                            tick: (j as i64 * 4 * 48) + i * 24,
                            lane: (i % 4) as u8,
                            at_us: start_us + i * S / 4,
                        })
                        .collect();
                    let last = cells.last().unwrap().at_us;
                    Section {
                        clear_us: last + 100_000,
                        cells,
                    }
                }),
            }
        })
        .collect();
    BattlePlan::new(
        rules,
        "placement-v2",
        POWER_CLASSES_V0.id,
        "ddi",
        seed,
        (1..=n as i64).map(|k| k * 2 * S).collect(),
        Schedule { slots },
    )
    .with_shocks(shocks)
}

/// A competitor's play, scripted: its moments in time order.
#[derive(Clone, Debug)]
pub struct Script {
    pub moments: Vec<Moment>,
    /// Out at this song ms (its life emptied), if it goes out.
    pub out_ms: Option<i32>,
    /// Plays to this song ms.
    pub end_ms: i32,
}

/// A seeded play: a moment every `gap_ms` from about 1 s to `end_ms`,
/// out at `out_ms` if given; some of its moments holds' ends where
/// `holds` (each draws once more, so the two kinds of script differ).
pub fn script(seed: u64, gap_ms: i32, end_ms: i32, out_ms: Option<i32>, holds: bool) -> Script {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let (mut life, mut combo, mut score) = (1_000_000i64, 0u32, 0i64);
    let mut moments = Vec::new();
    let mut at = 1_000 + (seed % 7) as i32 * 13;
    while at < out_ms.unwrap_or(end_ms).min(end_ms) {
        let r = next() % 100;
        let tier = match r {
            0..=79 => 0u8,
            80..=93 => 1,
            94..=97 => 2,
            _ => 5,
        };
        if tier == 5 {
            life -= 40_000;
            combo = 0;
        } else {
            life += 2_000;
            combo += 1;
            score += 6_000 - i64::from(tier) * 1_000;
        }
        life = life.clamp(1, 1_000_000);
        let kind = if holds && next() % 29 == 0 {
            MomentKind::HoldEnd { ok: tier < 5 }
        } else {
            MomentKind::Tap {
                tier,
                offset_ms: if tier == 5 {
                    0
                } else {
                    (next() % 41) as i16 - 20
                },
                w0: tier == 0 && next() % 2 == 0,
            }
        };
        moments.push(Moment {
            at_ms: at,
            tick: i64::from(at) * 48 * 2 / 1_000,
            lane: (next() % 4) as u8,
            kind,
            life_ppm: life as u32,
            combo,
            score,
        });
        at += gap_ms;
    }
    if let Some(out) = out_ms {
        moments.push(Moment {
            at_ms: out,
            tick: i64::from(out) * 48 * 2 / 1_000,
            lane: 0,
            kind: MomentKind::Tap {
                tier: 5,
                offset_ms: 0,
                w0: false,
            },
            life_ppm: 0,
            combo: 0,
            score,
        });
    }
    Script {
        moments,
        out_ms,
        end_ms,
    }
}

impl Script {
    /// The judgments of the chart's notes, as the ledger counts them.
    pub fn judged(&self) -> Vec<Judged> {
        self.moments
            .iter()
            .filter_map(|m| match m.kind {
                MomentKind::Tap { tier, .. } => Some(Judged {
                    at_us: i64::from(m.at_ms) * 1_000,
                    tier,
                }),
                _ => None,
            })
            .collect()
    }

    /// Life just before song µs `at`.
    pub fn life(&self, at: i64) -> u32 {
        self.moments
            .iter()
            .take_while(|m| i64::from(m.at_ms) * 1_000 < at)
            .last()
            .map_or(1_000_000, |m| m.life_ppm)
    }

    /// Its score just before song µs `at` (the last moment's before it):
    /// what its checkpoint reports carry (room model revision 20).
    pub fn score(&self, at: i64) -> i64 {
        self.moments
            .iter()
            .take_while(|m| i64::from(m.at_ms) * 1_000 < at)
            .last()
            .map_or(0, |m| m.score)
    }
}

/// A room run through the reducer at a time of its own: every accepted
/// revision's events applied to a mirror, which must stay equal to the
/// authority's state, and every refused command checked to change
/// nothing. Each test file adds the helpers its own story needs.
pub struct Room {
    pub s: ddi_room::RoomState,
    pub mirror: ddi_room::RoomState,
    pub now: u64,
    pub host: ddi_room::ParticipantId,
}

impl Room {
    /// A room owned by `twitch:<owner>` named `name`.
    pub fn open(
        owner: &str,
        name: &str,
        policy: ddi_room::RoomPolicy,
        rules: ddi_room::MatchRules,
        source: ddi_room::ContentSource,
    ) -> Self {
        let s = ddi_room::RoomState::new(
            ddi_room::RoomId::new("room"),
            ddi_room::Identity::new(format!("twitch:{owner}")),
            name.into(),
            policy,
            rules,
            source,
        );
        Self {
            mirror: s.clone(),
            s,
            now: 0,
            host: ddi_room::ParticipantId(1),
        }
    }

    pub fn follow(&mut self, events: &[ddi_room::Event]) {
        if !events.is_empty() {
            self.mirror.commit(events);
        }
        assert_eq!(self.mirror, self.s, "the mirror follows the events");
    }

    /// `who` issues `cmd` against their current view.
    pub fn by(
        &mut self,
        who: ddi_room::ParticipantId,
        cmd: ddi_room::Command,
    ) -> Result<Vec<ddi_room::Event>, ddi_room::Rejection> {
        self.based(who, Some(self.s.revision), cmd)
    }

    /// `who` issues `cmd` against the room as it was at `base`.
    pub fn based(
        &mut self,
        who: ddi_room::ParticipantId,
        base: Option<ddi_room::Revision>,
        cmd: ddi_room::Command,
    ) -> Result<Vec<ddi_room::Event>, ddi_room::Rejection> {
        self.apply(ddi_room::Actor::Member(who), base, cmd)
    }

    /// The authority's `cmd`.
    pub fn sys(
        &mut self,
        cmd: ddi_room::Command,
    ) -> Result<Vec<ddi_room::Event>, ddi_room::Rejection> {
        self.apply(ddi_room::Actor::Authority, None, cmd)
    }

    fn apply(
        &mut self,
        actor: ddi_room::Actor,
        base: Option<ddi_room::Revision>,
        cmd: ddi_room::Command,
    ) -> Result<Vec<ddi_room::Event>, ddi_room::Rejection> {
        let before = self.s.clone();
        let r = self.s.apply(actor, base, &cmd, ddi_room::Millis(self.now));
        match &r {
            Ok(events) => self.follow(&events.clone()),
            Err(_) => assert_eq!(before, self.s, "a refused command changes nothing"),
        }
        r
    }

    /// The authority's join of `twitch:<name>`.
    pub fn join_as(
        &mut self,
        name: &str,
        intent: ddi_room::Intent,
        admin: bool,
    ) -> Result<ddi_room::ParticipantId, ddi_room::Rejection> {
        let identity = ddi_room::Identity::new(format!("twitch:{name}"));
        self.sys(ddi_room::Command::Join {
            identity: identity.clone(),
            name: name.into(),
            admin,
            intent,
        })?;
        Ok(self.s.member_by_identity(&identity).unwrap().id)
    }

    /// `ms` pass: what the room's deadlines decide then.
    pub fn wait(&mut self, ms: u64) -> Vec<ddi_room::Event> {
        self.now += ms;
        let events = self.s.advance(ddi_room::Millis(self.now));
        self.follow(&events);
        events
    }

    /// Time passes to `ms` after the start: the authority's alarm at
    /// every deadline the room names on the way, as a server's is.
    pub fn wait_to(&mut self, ms: u64) {
        loop {
            match self.s.next_deadline() {
                Some(at) if at.0 <= ms && at.0 > self.now => {
                    self.now = at.0;
                    let events = self.s.advance(ddi_room::Millis(self.now));
                    self.follow(&events);
                }
                _ => break,
            }
        }
        self.now = ms;
        let events = self.s.advance(ddi_room::Millis(self.now));
        self.follow(&events);
    }

    /// The readiness revision of `stage`.
    pub fn rev(&self, stage: ddi_room::StageId) -> u32 {
        self.s.stage(stage).unwrap().readiness_rev
    }

    /// `who` reports `loading` for `stage`.
    pub fn load_for(
        &mut self,
        who: ddi_room::ParticipantId,
        stage: ddi_room::StageId,
        loading: ddi_room::Loading,
    ) -> Result<Vec<ddi_room::Event>, ddi_room::Rejection> {
        let readiness_rev = self.rev(stage);
        self.by(
            who,
            ddi_room::Command::ReportLoading {
                stage,
                readiness_rev,
                loading,
            },
        )
    }

    /// `who` says they are ready (or not) for `stage`.
    pub fn ready_for(
        &mut self,
        who: ddi_room::ParticipantId,
        stage: ddi_room::StageId,
        ready: bool,
    ) -> Result<Vec<ddi_room::Event>, ddi_room::Rejection> {
        let readiness_rev = self.rev(stage);
        self.by(
            who,
            ddi_room::Command::SetReady {
                stage,
                readiness_rev,
                ready,
            },
        )
    }
}

/// A member's scripted play as their device streams it: its moments, its
/// checkpoint reports where it has a reporter, and its end.
#[derive(Default)]
pub struct Stream {
    pub play: Option<Script>,
    pub sent: usize,
    pub reporter: Option<ddi_room::attacks::Reporter>,
    pub judged: Vec<Judged>,
    pub judged_sent: usize,
    pub ended: bool,
}

impl Stream {
    /// The play to stream, reported by `reporter` where given.
    pub fn give(&mut self, play: Script, reporter: Option<ddi_room::attacks::Reporter>) {
        self.judged = play.judged();
        self.play = Some(play);
        self.reporter = reporter;
    }

    /// Whether there is anything left to stream.
    pub fn streaming(&self) -> bool {
        self.play.is_some() && !self.ended
    }

    /// The play judged to song µs `t_us`, streamed on `live` for `stage`;
    /// at its end the run the member reports.
    pub fn feed(
        &mut self,
        live: &mut ddi_room::live::LiveLink,
        stage: ddi_room::StageId,
        t_us: i64,
    ) -> Option<ddi_room::Command> {
        use ddi_room::live::stream::{LiveReport, StreamEnd};
        use ddi_room::{Command, ReportedRun};
        let play = self.play.clone()?;
        if self.ended || t_us < -800_000 {
            return None;
        }
        let out = live.stream(stage);
        while let Some(m) = play.moments.get(self.sent) {
            if i64::from(m.at_ms) * 1_000 > t_us {
                break;
            }
            out.moment(*m);
            self.sent += 1;
        }
        let judged_to = t_us.min(i64::from(play.out_ms.unwrap_or(play.end_ms)) * 1_000);
        out.judged_to(judged_to);
        if let Some(r) = self.reporter.as_mut() {
            while let Some(j) = self.judged.get(self.judged_sent) {
                if j.at_us > t_us {
                    break;
                }
                r.judged(*j);
                self.judged_sent += 1;
            }
            if let Some(o) = play.out_ms
                && i64::from(o) * 1_000 <= t_us
            {
                r.out_at(i64::from(o) * 1_000);
            }
            for (k, rep) in r.due_scored(judged_to, &|at| play.life(at), &|at| play.score(at)) {
                out.report(LiveReport::from_report(k, rep));
            }
        }
        if let Some(o) = play.out_ms
            && i64::from(o) * 1_000 <= t_us
        {
            out.end(StreamEnd::Out {
                at_us: i64::from(o) * 1_000,
            });
            self.ended = true;
            Some(Command::ReportOutcome {
                stage,
                run: ReportedRun::Out { at_ms: o as u64 },
                detail: None,
            })
        } else if t_us >= i64::from(play.end_ms) * 1_000 {
            out.end(StreamEnd::Finished);
            self.ended = true;
            let last = play.moments.last();
            Some(Command::ReportOutcome {
                stage,
                run: ReportedRun::Live {
                    life_ppm: last.map_or(1_000_000, |m| m.life_ppm),
                    score: last.map_or(0, |m| m.score),
                    steps: 0,
                },
                detail: None,
            })
        } else {
            None
        }
    }
}
