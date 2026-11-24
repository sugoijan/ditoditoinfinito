//! The room model's fixes from the code review of 2026-11-22 (round RFa):
//! each case as the review described it, reproduced first, every
//! revision's events applied to a mirror, which must stay equal.

#![allow(dead_code)]

mod common;

use common::{Room, song};
use ddi_room::*;

fn hosted() -> Room {
    let mut policy = RoomPolicy::default().with_live().with_audience();
    policy.readiness.window_ms = 15_000;
    let mut rules = MatchRules::hosted();
    rules.stages = 3;
    Room::open("host", "Host", policy, rules, ContentSource::Shared)
}

fn join(r: &mut Room, name: &str) -> ParticipantId {
    r.join_as(name, Intent::Play, false).unwrap()
}

/// Finding 1 (a): a member's loading failure is kept with a code of a
/// bounded size, whatever they send (two 700 KB codes once made the
/// running match's row 1.4 MB). An honest client's longest code,
/// `battle_plan_mismatch`, is kept as it is.
#[test]
fn a_loading_failures_code_is_kept_bounded() {
    let mut r = hosted();
    let ana = join(&mut r, "ana");
    let bob = join(&mut r, "bob");
    let host = r.host;
    r.by(host, Command::Pick { content: song(1), card: None }).unwrap();
    let s1 = r.s.upcoming_stage().unwrap();
    r.load_for(ana, s1, Loading::Failed { code: "x".repeat(700_000) }).unwrap();
    // Cut between characters, never inside one.
    r.load_for(bob, s1, Loading::Failed { code: "é".repeat(100) }).unwrap();
    for who in [ana, bob] {
        let Loading::Failed { code } = &r.s.stage(s1).unwrap().prep(who).loading else {
            panic!("failed")
        };
        assert!(code.len() <= Loading::MAX_CODE, "{}", code.len());
    }
    let bytes = serde_json::to_vec(&r.s.current).unwrap().len();
    assert!(bytes < 20_000, "{bytes}");
    // The same report again changes nothing.
    assert_eq!(r.load_for(ana, s1, Loading::Failed { code: "x".repeat(700_000) }), Ok(vec![]));
    // An honest client's code, as it is.
    let honest = "battle_plan_mismatch";
    r.load_for(ana, s1, Loading::Failed { code: honest.into() }).unwrap();
    assert_eq!(
        r.s.stage(s1).unwrap().prep(ana).loading,
        Loading::Failed { code: honest.into() }
    );
}

/// Finding 1 (c): the next match's rules an operator saves are bounded:
/// the battle's id at 64 bytes (as a stage's rules already were,
/// `PlayRules::check`) and the CPU roster at the room's competitor cap
/// whatever the CPU mode (it was read only under `CpuMode::Roster`, so
/// a co-host could park any number of entries in the draft). An honest
/// roster seats its CPUs beside one person at least: at most the cap
/// less one.
#[test]
fn the_next_rules_are_bounded() {
    let mut r = hosted();
    let host = r.host;
    let mut rules = r.s.next.rules.clone();
    rules.battle = "x".repeat(700_000);
    let got = r.by(host, Command::SaveNextRules { rules });
    assert!(
        matches!(got, Err(Rejection::InvalidRules(RulesProblem::BattleId))),
        "the battle's id refused"
    );

    let mut rules = r.s.next.rules.clone();
    rules.cpu.mode = CpuMode::Fill;
    rules.cpu.roster = vec![rules.cpu.opponent.clone(); 10_000];
    let got = r.by(host, Command::SaveNextRules { rules: rules.clone() });
    assert!(
        matches!(got, Err(Rejection::InvalidRules(RulesProblem::OverCap(_)))),
        "the roster refused"
    );
    // Within the cap it is saved as before.
    rules.cpu.roster.truncate(usize::from(r.s.policy.max_competitors) - 1);
    r.by(host, Command::SaveNextRules { rules }).unwrap();
    // A practice roster as the client makes it (`cpu_roster::with_roster`).
    let mut rules = MatchRules::practice(OpponentSpec { level: 5, seed: 1, personality: None, name: None, targeting: None });
    rules.cpu.roster = vec![rules.cpu.opponent.clone(); 3];
    rules.cpu.mode = CpuMode::Roster;
    rules.seats = 4;
    assert_eq!(rules.validate(&RoomPolicy::default(), 1), Ok(()));
}

/// Finding 1 (b): rows of people who left are kept (results name them by
/// id), and a room bounded only the people present, so a room joined
/// over its life by thousands of sign-ins grew its stored row without
/// bound. A new person is refused as a full room once the room keeps
/// `RoomPolicy::member_rows` people's rows, present or not (four times
/// its member bound and administrators' reserve: 1,552 for a room of
/// today's bound); someone with a row comes back as before. Proven with
/// a member bound of 4 (rows: 20). Since room model revision 24 a far
/// backstop: rows nothing names are forgotten (`tests/revision_24.rs`),
/// so here the people who leave are co-hosts, whose role keeps the row.
#[test]
fn a_room_keeps_a_bounded_number_of_peoples_rows() {
    let mut policy = RoomPolicy::default().with_live().with_audience();
    policy.max_members = 4;
    policy.admin_reserve = 1;
    assert_eq!(policy.member_rows(), 20);
    assert_eq!(RoomPolicy::default().member_rows(), 1_552);
    let mut r = Room::open("host", "Host", policy, MatchRules::hosted(), ContentSource::Shared);
    let host = r.host;
    let cohost = |r: &mut Room, name: &str| {
        let id = r.join_as(name, Intent::Watch, false).unwrap();
        r.by(host, Command::SetCoHost { participant: id, on: true }).unwrap();
        r.by(id, Command::Leave).unwrap();
        id
    };
    let first = cohost(&mut r, "p0");
    for i in 1..18 {
        cohost(&mut r, &format!("p{i}"));
    }
    assert_eq!(r.s.members.len(), 19, "the host and 18 who left");
    // The twentieth row is taken; a twenty-first person is refused.
    cohost(&mut r, "p18");
    assert_eq!(r.join_as("p19", Intent::Watch, false), Err(Rejection::RoomFull));
    // An administrator too: the bound is the room's stored size.
    assert_eq!(r.join_as("pat", Intent::Watch, true), Err(Rejection::RoomFull));
    // Someone who has a row comes back.
    assert_eq!(r.join_as("p0", Intent::Watch, false), Ok(first));
    assert!(!r.s.member(first).unwrap().left);
    // A room stored with more rows than that (before the bound) loads,
    // and its people come back.
    let mut big = r.s.clone();
    let json = {
        let mut room = r.s.clone();
        for i in 0..30u64 {
            let id = ParticipantId(1_000 + i);
            let mut m = room.members[&first].clone();
            m.id = id;
            m.kind = MemberKind::Human { identity: Identity::new(format!("twitch:old{i}")), name: format!("old{i}") };
            m.left = true;
            room.members.insert(id, m);
        }
        room.counters.participant = 1_030;
        serde_json::to_string(&room).unwrap()
    };
    big = serde_json::from_str(&json).unwrap_or(big);
    assert_eq!(big.members.len(), 50);
    let mut back = Room { mirror: big.clone(), s: big, now: r.now, host: r.host };
    assert_eq!(back.join_as("old3", Intent::Watch, false), Ok(ParticipantId(1_003)));
    assert_eq!(back.join_as("newcomer", Intent::Watch, false), Err(Rejection::RoomFull));
}

/// Finding 2: a co-host the owner kicks loses the role with the kick, so
/// coming back by the invite they are a member like anyone (they came
/// back a co-host, with Kick, the pick and the rules). A co-host who
/// leaves by themselves keeps it, as before.
#[test]
fn a_kicked_co_host_comes_back_a_member() {
    let mut r = hosted();
    let host = r.host;
    let cy = join(&mut r, "cy");
    let di = join(&mut r, "di");
    for p in [cy, di] {
        r.by(host, Command::SetCoHost { participant: p, on: true }).unwrap();
    }
    r.by(host, Command::Remove { participant: cy }).unwrap();
    assert_eq!(r.s.member(cy).unwrap().role, Role::Member);
    assert_eq!(join(&mut r, "cy"), cy);
    let m = r.s.member(cy).unwrap();
    assert!(!m.left);
    assert_eq!(m.role, Role::Member, "back as a member");
    assert!(r.by(cy, Command::Remove { participant: di }).is_err());
    // Leaving by oneself keeps the role.
    r.by(di, Command::Leave).unwrap();
    assert_eq!(join(&mut r, "di"), di);
    assert_eq!(r.s.member(di).unwrap().role, Role::CoHost);
}

/// Finding 3 (a): with CPUs, a seated player who left and comes back to
/// a match whose places are all taken (their own given to someone since)
/// comes back to watch: the lineup never passes the competitor cap (it
/// reached 74, a field no award rule knows, and the stage never settled).
#[test]
fn a_returning_player_never_takes_a_stage_past_the_cap_with_cpus() {
    let policy = RoomPolicy::default();
    let mut rules = MatchRules::hosted();
    rules.stages = 3;
    rules.seats = 73;
    rules.cpu.mode = CpuMode::Fixed { count: 71 };
    let mut r = Room::open("host", "Host", policy, rules, ContentSource::LocalLibrary);
    let host = r.host;
    let ana = join(&mut r, "ana");
    let w = r.join_as("w", Intent::Watch, false).unwrap();
    r.by(host, Command::Pick { content: song(1), card: None }).unwrap();
    assert_eq!(r.s.candidates().len(), 73);
    r.by(ana, Command::Leave).unwrap();
    r.by(w, Command::SetIntent { intent: Intent::Play }).unwrap();
    assert!(r.s.current.as_ref().unwrap().roster.contains_key(&w), "w seated in ana's place");
    assert_eq!(join(&mut r, "ana"), ana);
    assert_eq!(r.s.member(ana).unwrap().intent, Intent::Watch, "back to watch: no place");
    let stage = r.s.upcoming_stage().unwrap();
    for p in [host, ana, w] {
        let _ = r.load_for(p, stage, Loading::Loaded);
        let _ = r.ready_for(p, stage, true);
    }
    for _ in 0..400 {
        if matches!(r.s.phase, Phase::Stage { .. }) {
            break;
        }
        let next = r.now + 100;
        r.wait_to(next);
    }
    let play = r.s.stage(stage).unwrap().play.clone().unwrap();
    assert_eq!(play.lineup.len(), 73);
    // Choosing Play again is refused while every place is taken.
    assert!(r.by(ana, Command::SetIntent { intent: Intent::Play }).is_err());
}

/// Finding 3 (b): a stage the room ranks from reports whose result no
/// award rule can confirm interrupts the match at its report deadline
/// (it stayed in its stage, the deadline past, every alarm doing
/// nothing, until an operator stopped the match).
#[test]
fn a_stage_that_cannot_be_confirmed_interrupts_the_match() {
    let mut r = hosted();
    let host = r.host;
    let ana = join(&mut r, "ana");
    r.by(host, Command::Pick { content: song(1), card: None }).unwrap();
    let s = r.s.upcoming_stage().unwrap();
    r.sys(Command::SetContentStatus { stage: s, status: ContentStatus::Available, share: Some("sh".into()) })
        .unwrap();
    for p in [host, ana] {
        r.load_for(p, s, Loading::Loaded).unwrap();
        r.ready_for(p, s, true).unwrap();
    }
    for _ in 0..400 {
        if matches!(r.s.phase, Phase::Stage { .. }) {
            break;
        }
        let next = r.now + 100;
        r.wait_to(next);
    }
    assert!(matches!(r.s.phase, Phase::Stage { .. }));
    // A field of 74, as the lineup above once reached: no rule awards it.
    for room in [&mut r.s, &mut r.mirror] {
        let play = room
            .current
            .as_mut()
            .unwrap()
            .stages
            .iter_mut()
            .find(|x| x.id == s)
            .unwrap()
            .play
            .as_mut()
            .unwrap();
        for i in 0..72 {
            play.lineup.insert(ParticipantId(10_000 + i));
        }
    }
    assert!(r.s.report_deadline(s).is_some());
    let deadline = r.s.report_deadline(s).unwrap().0;
    r.wait_to(deadline + 1);
    assert_eq!(
        r.s.phase,
        Phase::Interrupted { reason: Interruption::AuthorityFailed }
    );
    assert_eq!(r.s.next_deadline(), None, "nothing left to wake for");
}

/// Finding 4: one who pressed Ready while still loading, counted out of
/// the start as still loading and so free to enter late, chooses their
/// own start once loaded (`start_refusal` read their readiness as having
/// entered); once entered, it is locked.
#[test]
fn a_late_entrant_who_readied_while_loading_chooses_their_start() {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    rules.charts = Charts::Adaptive;
    let mut policy = RoomPolicy::default().with_live().with_audience();
    policy.readiness.window_ms = 15_000;
    let mut r = Room::open("host", "Host", policy, rules, ContentSource::Shared);
    let host = r.host;
    let ana = join(&mut r, "ana");
    let bob = join(&mut r, "bob");
    let line = |k: u32, d: &str| ChartLine {
        chart: ChartKey::new(format!("d1-{k}")),
        style: "dance-single".into(),
        difficulty: d.into(),
        meter: 3 + k,
    };
    let song = NewSong {
        key: SongKey::new("s000000000000001"),
        card: SongCard { title: "S".into(), artist: String::new(), pack: String::new() },
        charts: vec![line(0, "Easy"), line(1, "Medium"), line(2, "Hard")],
    };
    r.by(host, Command::AddSong { song: Box::new(song.clone()) }).unwrap();
    let id = r.s.songs.by_key(&song.key).unwrap().id;
    r.sys(Command::SetSongContent { song: id, status: ContentStatus::Available, share: Some("share001".into()), charts: None }).unwrap();
    r.by(host, Command::PickListed { song: id, chart: ChartKey::new("d1-2") }).unwrap();
    let stage = r.s.upcoming_stage().unwrap();
    for p in [host, bob] {
        r.load_for(p, stage, Loading::Loaded).unwrap();
        r.ready_for(p, stage, true).unwrap();
    }
    r.load_for(ana, stage, Loading::Loading { permille: 300 }).unwrap();
    r.ready_for(ana, stage, true).unwrap();
    for _ in 0..1000 {
        if matches!(r.s.phase, Phase::Countdown { .. }) {
            break;
        }
        let next = r.now + 100;
        r.wait_to(next);
    }
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    let play = r.s.stage(stage).unwrap().play.clone().unwrap();
    assert_eq!(play.sat_out.get(&ana), Some(&SitOutReason::StillLoading));
    r.load_for(ana, stage, Loading::Loaded).unwrap();
    assert!(r.s.late_entry_open(ana, stage, Millis(r.now)).is_ok());
    let rev = r.rev(stage);
    r.by(ana, Command::SetStart { stage, readiness_rev: rev, chart: Some(ChartKey::new("d1-0")) })
        .unwrap();
    assert_eq!(r.s.start_of(stage, ana), Some(&ChartKey::new("d1-0")));
    // Entered, the start is locked.
    r.ready_for(ana, stage, true).unwrap();
    assert!(r.s.stage(stage).unwrap().play.as_ref().unwrap().lineup.contains(&ana));
    let rev = r.rev(stage);
    assert_eq!(
        r.by(ana, Command::SetStart { stage, readiness_rev: rev, chart: Some(ChartKey::new("d1-1")) }),
        Err(Rejection::StartCommitted)
    );
}

/// Finding 5: an administrator who holds the pick and comes back without
/// the flag lets the pick go, as a co-host losing the role does (they
/// stayed the picker, every pick of theirs refused).
#[test]
fn an_administrator_losing_the_flag_lets_the_pick_go() {
    let mut r = hosted();
    let pat = r.join_as("pat", Intent::Play, true).unwrap();
    r.by(pat, Command::TakePick).unwrap();
    assert_eq!(r.s.picker(), Some(pat));
    assert_eq!(r.join_as("pat", Intent::Play, false), Ok(pat));
    assert!(!r.s.member(pat).unwrap().admin);
    assert_eq!(r.s.picker(), Some(r.host), "back to the owner");
    // A co-host losing the flag keeps the pick: their role takes it.
    let cy = r.join_as("cy", Intent::Play, true).unwrap();
    let host = r.host;
    r.by(host, Command::SetCoHost { participant: cy, on: true }).unwrap();
    r.by(cy, Command::TakePick).unwrap();
    assert_eq!(r.join_as("cy", Intent::Play, false), Ok(cy));
    assert_eq!(r.s.picker(), Some(cy));
}

/// Finding 6: after the owner lowers the list's size, an addition by
/// someone held to their half of the list (under Who can add songs:
/// Everyone) pushes out one song, the oldest not in use, never many at
/// once (it removed every song down to the new size: 91 of 100). A
/// host's addition still makes room down to the size, as before
/// (`room_songs.rs`'s full list).
#[test]
fn an_addition_held_to_a_half_pushes_out_one_song() {
    let mut r = hosted();
    let host = r.host;
    let ana = join(&mut r, "ana");
    let song = |n: u32| common::one_chart_song(n, "Easy", 3);
    for n in 1..=12 {
        r.by(host, Command::AddSong { song: Box::new(song(n)) }).unwrap();
    }
    r.by(host, Command::SetWhoAdds { who: WhoAdds::Everyone }).unwrap();
    let small = ListRules { size: 4, when_full: WhenFull::RemoveOldest };
    r.by(host, Command::SetSongList { rules: small }).unwrap();
    assert_eq!(r.s.songs.len(), 12, "a smaller size removes nothing by itself");
    assert_eq!(r.s.song_quota(ana), Some(2));
    assert_eq!(r.s.add_refusal(ana, Some(&song(20))), None);
    let events = r.by(ana, Command::AddSong { song: Box::new(song(20)) }).unwrap();
    let removed = events.iter().filter(|e| matches!(e, Event::SongRemoved { .. })).count();
    assert_eq!(removed, 1, "one song out for one in");
    assert_eq!(r.s.songs.len(), 12);
    // Their half reached, refused as before.
    r.by(ana, Command::AddSong { song: Box::new(song(21)) }).unwrap();
    assert!(matches!(
        r.by(ana, Command::AddSong { song: Box::new(song(22)) }),
        Err(Rejection::TooManySongs { .. })
    ));
    // The host's addition makes room down to the size.
    r.by(host, Command::AddSong { song: Box::new(song(30)) }).unwrap();
    assert_eq!(r.s.songs.len(), 4);
}

/// Finding 7: a run's reported life is kept within a full gauge (only its
/// detail's was): `life_ppm: u32::MAX` ranked first under As picked.
#[test]
fn a_reported_runs_life_is_kept_within_a_full_gauge() {
    let mut r = hosted();
    let host = r.host;
    let ana = join(&mut r, "ana");
    r.by(host, Command::Pick { content: song(1), card: None }).unwrap();
    let s = r.s.upcoming_stage().unwrap();
    r.sys(Command::SetContentStatus { stage: s, status: ContentStatus::Available, share: Some("sh".into()) })
        .unwrap();
    for p in [host, ana] {
        r.load_for(p, s, Loading::Loaded).unwrap();
        r.ready_for(p, s, true).unwrap();
    }
    for _ in 0..400 {
        if matches!(r.s.phase, Phase::Stage { .. }) {
            break;
        }
        let next = r.now + 100;
        r.wait_to(next);
    }
    let forged = ReportedRun::Live { life_ppm: u32::MAX, score: 10, steps: 0 };
    r.by(ana, Command::ReportOutcome { stage: s, run: forged, detail: None }).unwrap();
    let kept = r.s.stage(s).unwrap().play.as_ref().unwrap().reports[&ana];
    assert_eq!(kept, ReportedRun::Live { life_ppm: 1_000_000, score: 10, steps: 0 });
    // The same report again is the same report.
    assert_eq!(r.by(ana, Command::ReportOutcome { stage: s, run: forged, detail: None }), Ok(vec![]));
    let honest = ReportedRun::Live { life_ppm: 500_000, score: 20, steps: 0 };
    r.by(host, Command::ReportOutcome { stage: s, run: honest, detail: None }).unwrap();
    // Ranked by life: the host's half gauge below ana's full one, by
    // life first; the life no longer past a full gauge.
    let result = r.s.stage(s).unwrap().play.as_ref().unwrap().result.clone().unwrap();
    assert_eq!(result.placements[0].participant, ana);
}
