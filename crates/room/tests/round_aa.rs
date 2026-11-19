//! Feedback round AA (room model revision 15), through the reducer: people
//! who join during a match play its next song not begun; open-ended
//! matches, a last song and an early end; and under Charts: Adaptive's
//! Play on, a failed competitor's place after every survivor by score.
//! Every revision's events are applied to a mirror, which must stay
//! equal.

mod common;

use common::{Room, song};
use ddi_room::*;

impl Room {
    /// A room under `rules` with `hosted-deadline-v1`'s window (no loading
    /// grace), its readiness as the walkthroughs have it.
    fn under(rules: MatchRules) -> Self {
        let policy = RoomPolicy {
            readiness: ReadinessPolicy::v1(),
            ..RoomPolicy::default()
        };
        Self::open("host", "Host", policy, rules, ContentSource::LocalLibrary)
    }

    fn join(&mut self, name: &str) -> ParticipantId {
        self.join_as(name, Intent::Play, false).unwrap()
    }

    fn pick(&mut self, n: u32) -> Result<Vec<Event>, Rejection> {
        let host = self.host;
        self.by(
            host,
            Command::Pick {
                content: song(n),
                card: None,
            },
        )
    }

    fn upcoming(&self) -> StageId {
        self.s.upcoming_stage().expect("a stage gathers readiness")
    }

    fn ready(&mut self, who: ParticipantId) {
        let stage = self.upcoming();
        self.load_for(who, stage, Loading::Loaded).unwrap();
        self.ready_for(who, stage, true).unwrap();
    }

    /// Everyone named readied, then the countdown into the song.
    fn play_with(&mut self, who: &[ParticipantId]) -> StageId {
        for p in who {
            self.ready(*p);
        }
        let Phase::Countdown { stage, .. } = self.s.phase else {
            panic!("not counting down: {:?}", self.s.phase)
        };
        self.wait(self.s.policy.readiness.countdown_ms);
        stage
    }

    fn end(&mut self, stage: StageId, groups: &[&[ParticipantId]]) {
        let ranking = groups
            .iter()
            .map(|g| {
                g.iter()
                    .map(|&participant| Finisher {
                        participant,
                        finish: Finish::Survived,
                    })
                    .collect()
            })
            .collect();
        self.sys(Command::EndSong {
            stage,
            ranking: Some(ranking),
        })
        .unwrap();
    }

    fn part(&self, who: ParticipantId) -> Participation {
        self.s.participation(who).unwrap()
    }

    fn standing(&self, who: ParticipantId) -> Standing {
        self.s
            .standings()
            .unwrap()
            .into_iter()
            .find(|s| s.participant == who)
            .unwrap()
    }
}

fn rules(stages: u16) -> MatchRules {
    let mut rules = MatchRules::hosted();
    rules.stages = stages;
    rules
}

/// Part 2: joining during a match is On for a new room. Someone who joins
/// while a stage's song is picked and not started is seated in it at once
/// (its song to load like anyone's) and plays it; someone who joins
/// during a stage's play waits for the next pick and plays the stage
/// after; a stage they missed stands as not played, their total the
/// stages they played.
#[test]
fn a_newcomer_during_a_match_plays_its_next_song_not_begun() {
    assert!(MatchRules::hosted().mid_match_entry, "On for a new room");
    let mut r = Room::under(rules(3));
    let host = r.host;
    let ana = r.join("ana");
    r.pick(1).unwrap();
    let s1 = r.upcoming();
    // Picked, not started: Bo is seated in it at once.
    let bo = r.join("bo");
    assert_eq!(r.part(bo), Participation::NotReady { stage: s1 });
    assert_eq!(
        r.s.current.as_ref().unwrap().roster.get(&bo).unwrap().first_stage,
        s1
    );
    assert!(r.s.admits_next_stage());
    let stage = r.play_with(&[host, ana, bo]);
    assert_eq!(stage, s1);
    assert!(r.s.stage(s1).unwrap().play.as_ref().unwrap().lineup.contains(&bo));
    // During the play: Cy waits for the next song.
    let cy = r.join("cy");
    assert_eq!(r.part(cy), Participation::WaitingNextStage);
    assert_eq!(r.ready_for(cy, s1, true), Err(Rejection::NotUpcoming));
    r.end(s1, &[&[bo], &[ana], &[host]]);
    assert_eq!(r.part(cy), Participation::WaitingNextStage);
    r.pick(2).unwrap();
    let s2 = r.upcoming();
    assert_eq!(r.part(cy), Participation::NotReady { stage: s2 });
    let s = r.play_with(&[host, ana, bo, cy]);
    r.end(s, &[&[cy], &[bo], &[ana], &[host]]);
    // Cy missed stage 1: not played there, the total stage 2's alone.
    let row = r.standing(cy);
    assert_eq!(row.stages[0], StageStanding::NotPlayed);
    assert!(matches!(row.stages[1], StageStanding::Placed(p) if p.place == 1));
    assert_eq!(
        row.total,
        match row.stages[1] {
            StageStanding::Placed(p) => p.points,
            _ => unreachable!(),
        }
    );
}

/// Part 2: newcomers are seated up to the room's competitor cap, never
/// the seats the match began with, and never in a CPU's place: with every
/// place taken (CPUs counted) a newcomer waits and is told why; a place
/// that frees seats them at the next stage.
#[test]
fn a_full_match_seats_nobody_more_and_never_in_a_cpus_place() {
    let opponent = OpponentSpec {
        level: 3,
        seed: 7,
        personality: None,
        name: None,
        targeting: None,
    };
    let mut rules = MatchRules::practice(opponent);
    rules.stages = 3;
    rules.seats = 5;
    let mut r = Room::under(rules);
    r.s.policy.max_competitors = 5;
    r.mirror.policy.max_competitors = 5;
    let host = r.host;
    let ana = r.join("ana");
    r.pick(1).unwrap();
    // Host, Ana and three CPUs: five places, the cap.
    assert_eq!(r.s.current.as_ref().unwrap().roster.len(), 5);
    let bo = r.join("bo");
    assert_eq!(r.part(bo), Participation::WaitingPlace);
    assert_eq!(r.s.late_places(), 0);
    let cpus: Vec<ParticipantId> = r.s.members.values().filter(|m| m.is_cpu()).map(|m| m.id).collect();
    let s1 = r.play_with(&[host, ana]);
    let mut field = vec![host, ana];
    field.extend(&cpus);
    let groups: Vec<&[ParticipantId]> = field.iter().map(std::slice::from_ref).collect();
    r.end(s1, &groups);
    // Ana leaves: a place frees, and Bo is seated at the next pick.
    r.by(ana, Command::Leave).unwrap();
    assert_eq!(r.part(bo), Participation::WaitingNextStage);
    r.pick(2).unwrap();
    let s2 = r.upcoming();
    assert_eq!(r.part(bo), Participation::NotReady { stage: s2 });
    assert!(cpus.iter().all(|c| r.s.current.as_ref().unwrap().roster.contains_key(c)));
}

/// Part 2: Off (a room stored before revision 15 keeps it so) waits for
/// the next match as before; Off is the only way persistent elimination
/// goes (the two never combine).
#[test]
fn joining_during_a_match_off_and_elimination_never_combine() {
    let mut off = rules(2);
    off.mid_match_entry = false;
    let mut r = Room::under(off);
    let host = r.host;
    let ana = r.join("ana");
    r.pick(1).unwrap();
    let bo = r.join("bo");
    assert_eq!(r.part(bo), Participation::WaitingNextMatch);
    assert!(!r.s.admits_next_stage());
    let s1 = r.play_with(&[host, ana]);
    r.end(s1, &[&[host], &[ana]]);
    r.pick(2).unwrap();
    assert_eq!(r.part(bo), Participation::WaitingNextMatch);
    let mut both = rules(2);
    both.return_each_stage = false;
    assert_eq!(
        both.validate(&RoomPolicy::default(), 1),
        Err(RulesProblem::EliminationWithLateEntry)
    );
}

fn open(stages_shown: u16) -> MatchRules {
    let mut rules = rules(stages_shown);
    rules.open_ended = true;
    rules
}

/// Part 3: an open-ended match goes on as songs are picked; a pick made
/// its last song (until its start) ends the match once it settles, as a
/// counted match ends; the next match's draft keeps Stages: Open.
#[test]
fn an_open_match_goes_on_until_its_last_song() {
    let mut r = Room::under(open(4));
    let host = r.host;
    let ana = r.join("ana");
    assert_eq!(r.s.playlist.len(), 1, "one stage made ahead");
    for n in 1..=3 {
        r.pick(n).unwrap();
        let s = r.upcoming();
        let m = r.s.current.as_ref().unwrap();
        assert_eq!(m.stage_count(), None);
        assert!(!m.is_last(s, r.s.policy.max_stages));
        let played = r.play_with(&[host, ana]);
        assert_eq!(played, s);
        // Its next stage made as it started.
        assert_eq!(r.s.current.as_ref().unwrap().stages.len(), n as usize + 1);
        r.end(s, &[&[host], &[ana]]);
        assert!(matches!(r.s.phase, Phase::Intermission { .. }));
    }
    // The fourth picked, made the last song and back, then the last.
    r.pick(4).unwrap();
    let s4 = r.upcoming();
    r.by(host, Command::SetLastStage { stage: s4, last: true }).unwrap();
    assert!(r.s.stage(s4).unwrap().last);
    r.by(host, Command::SetLastStage { stage: s4, last: false }).unwrap();
    assert!(!r.s.stage(s4).unwrap().last);
    assert_eq!(
        r.by(ana, Command::SetLastStage { stage: s4, last: true }),
        Err(Rejection::Unauthorized {
            capability: Capability::Advance
        })
    );
    r.by(host, Command::SetLastStage { stage: s4, last: true }).unwrap();
    let played = r.play_with(&[host, ana]);
    // Never once its play started (an operator advances nothing then).
    assert!(
        r.by(host, Command::SetLastStage { stage: played, last: false })
            .is_err()
    );
    assert!(r.s.stage(played).unwrap().last);
    assert_eq!(r.s.current.as_ref().unwrap().stages.len(), 4, "none after the last");
    r.end(played, &[&[ana], &[host]]);
    assert_eq!(r.s.phase, Phase::MatchResults);
    assert_eq!(r.s.standings().unwrap()[0].stages.len(), 4);
    assert_eq!(r.s.match_winners().len(), 1);
    assert!(r.s.next.rules.open_ended, "the next match open too");
    // Counted: no last song.
    let mut c = Room::under(rules(2));
    let host = c.host;
    c.join("ana");
    c.pick(1).unwrap();
    let s = c.upcoming();
    assert_eq!(
        c.by(host, Command::SetLastStage { stage: s, last: true }),
        Err(Rejection::NotOpenEnded)
    );
}

/// Part 3: between stages an operator ends an open match now, with the
/// stages played (never before one settled, never during a play); a
/// start canceled and its stage then made the last drops what was made
/// ahead of it; the room's stage bound is an open match's last by itself.
#[test]
fn an_open_match_ends_now_or_at_the_rooms_bound() {
    let mut r = Room::under(open(4));
    let host = r.host;
    let ana = r.join("ana");
    r.pick(1).unwrap();
    assert_eq!(r.by(host, Command::EndMatch), Err(Rejection::NothingPlayed));
    let s1 = r.play_with(&[host, ana]);
    // Never during a stage's play (an operator advances nothing then).
    assert!(r.by(host, Command::EndMatch).is_err());
    r.end(s1, &[&[ana], &[host]]);
    assert_eq!(
        r.by(ana, Command::EndMatch),
        Err(Rejection::Unauthorized {
            capability: Capability::Advance
        })
    );
    r.pick(2).unwrap();
    r.by(host, Command::EndMatch).unwrap();
    assert_eq!(r.s.phase, Phase::MatchResults);
    let m = r.s.current.as_ref().unwrap();
    assert_eq!(m.stages.len(), 1, "the stage not begun dropped");
    assert_eq!(r.s.match_winners(), vec![ana]);
    // Counted: no early end (Stop match is its way).
    let mut c = Room::under(rules(2));
    let host = c.host;
    let ana = c.join("ana");
    c.pick(1).unwrap();
    let s = c.play_with(&[host, ana]);
    c.end(s, &[&[host], &[ana]]);
    assert_eq!(c.by(host, Command::EndMatch), Err(Rejection::NotOpenEnded));

    // The room's bound: two stages at most.
    let mut b = Room::under(open(4));
    b.s.policy.max_stages = 2;
    b.mirror.policy.max_stages = 2;
    let host = b.host;
    let ana = b.join("ana");
    b.pick(1).unwrap();
    let s1 = b.play_with(&[host, ana]);
    b.end(s1, &[&[host], &[ana]]);
    b.pick(2).unwrap();
    let s2 = b.upcoming();
    assert!(b.s.current.as_ref().unwrap().is_last(s2, 2), "said beforehand");
    let s2 = b.play_with(&[host, ana]);
    assert_eq!(b.s.current.as_ref().unwrap().stages.len(), 2);
    b.end(s2, &[&[host], &[ana]]);
    assert_eq!(b.s.phase, Phase::MatchResults);

    // A start canceled, then the stage made the last: what was made
    // ahead is dropped at its end.
    let mut k = Room::under(open(4));
    let host = k.host;
    let ana = k.join("ana");
    k.pick(1).unwrap();
    k.ready(host);
    k.ready(ana);
    let Phase::Countdown { stage, .. } = k.s.phase else {
        panic!("{:?}", k.s.phase)
    };
    assert_eq!(k.s.current.as_ref().unwrap().stages.len(), 2);
    k.by(host, Command::CancelStart { stage }).unwrap();
    k.by(host, Command::SetLastStage { stage, last: true }).unwrap();
    // Picked again: its window opens anew, the readiness kept, so the
    // start commits at once.
    k.pick(1).unwrap();
    assert!(matches!(k.s.phase, Phase::Countdown { .. }));
    k.wait(k.s.policy.readiness.countdown_ms);
    let s = stage;
    k.end(s, &[&[host], &[ana]]);
    assert_eq!(k.s.phase, Phase::MatchResults);
    assert_eq!(k.s.current.as_ref().unwrap().stages.len(), 1);
}

fn e(id: u64, run: Run) -> Entrant {
    Entrant {
        participant: ParticipantId(id),
        run,
    }
}

/// Part 4: the failed (Charts: Adaptive's Play on) rank after everyone
/// still in, among themselves by score (equal scores tied), before those
/// out and forfeits; out for the decision, which stays the battle's
/// among those still in.
#[test]
fn failed_runs_rank_after_the_survivors_by_score() {
    let live = |life, score| Run::Live {
        life,
        score,
        steps: 0,
    };
    let entrants = [
        e(1, Run::Failed { at: 20.0, score: 5_000_000 }),
        e(2, live(0.1, 1_000_000)),
        e(3, Run::Failed { at: 40.0, score: 4_000_000 }),
        e(4, Run::Failed { at: 10.0, score: 5_000_000 }),
        e(5, Run::Forfeit),
    ];
    let ranking = battle::rank(&entrants, RankBy::Score);
    let ids: Vec<Vec<u64>> = ranking
        .iter()
        .map(|g| g.iter().map(|f| f.participant.0).collect())
        .collect();
    assert_eq!(ids, vec![vec![2], vec![1, 4], vec![3], vec![5]]);
    assert_eq!(ranking[1][0].finish, Finish::Failed);
    assert!(battle::decided(&entrants), "one still in: decided");
    let failed = ReportedRun::of(Run::Failed { at: 12.3456, score: 42 });
    assert_eq!(failed, ReportedRun::Failed { at_ms: 12_346, score: 42 });
    assert!(failed.finished_song());
    assert_eq!(
        serde_json::to_string(&failed).unwrap(),
        r#"{"failed":{"at_ms":12346,"score":42}}"#
    );
    // The rule: Play on for a new room, under both chart rules since
    // revision 16 (round AC); a rule set from before it reads under
    // Adaptive alone.
    let mut r = MatchRules::hosted();
    assert_eq!(r.easiest, AtEasiest::PlayOn);
    assert!(r.plays_on_failed(), "As picked: plays on (revision 16)");
    r.charts = Charts::Adaptive;
    assert!(r.plays_on_failed());
    r.easiest = AtEasiest::Out;
    assert!(!r.plays_on_failed());
    let mut before = MatchRules::hosted();
    before.life_out_any_chart = false;
    assert!(!before.plays_on_failed(), "revision 15's As picked: out");
    before.charts = Charts::Adaptive;
    assert!(before.plays_on_failed());
}

/// Part 4, a stage settled on its starters' reports: one survivor, two
/// failed who played on (the higher score above), the awards the same
/// table over the whole order.
#[test]
fn a_stage_with_failed_starters_awards_them_after_the_survivor() {
    let mut rules = rules(1);
    rules.charts = Charts::Adaptive;
    let policy = RoomPolicy {
        readiness: ReadinessPolicy::v1(),
        ..RoomPolicy::default()
    };
    let mut r = Room::open("host", "Host", policy, rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join("ana");
    let bo = r.join("bo");
    r.pick(1).unwrap();
    let s = r.upcoming();
    r.sys(Command::SetContentStatus {
        stage: s,
        status: ContentStatus::Available,
        share: Some("abc12345".into()),
    })
    .unwrap();
    let stage = r.play_with(&[host, ana, bo]);
    for (who, run) in [
        (ana, Run::Failed { at: 30.0, score: 3_000_000 }),
        (
            host,
            Run::Live {
                life: 0.2,
                score: 1_000_000,
                steps: 0,
            },
        ),
        (bo, Run::Failed { at: 50.0, score: 2_000_000 }),
    ] {
        r.by(
            who,
            Command::ReportOutcome {
                stage,
                run: ReportedRun::of(run),
                detail: None,
            },
        )
        .unwrap();
    }
    let play = r.s.stage(stage).unwrap().play.clone().unwrap();
    let result = play.result.expect("settled on the three reports");
    let place = |p| result.placement(p).unwrap();
    assert_eq!((place(host).place, place(host).finish), (1, Finish::Survived));
    assert_eq!((place(ana).place, place(ana).finish), (2, Finish::Failed));
    assert_eq!((place(bo).place, place(bo).finish), (3, Finish::Failed));
    assert!(place(host).points > place(ana).points && place(ana).points > place(bo).points);
}
