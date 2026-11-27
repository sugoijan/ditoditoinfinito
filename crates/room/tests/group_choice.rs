//! Choosing by the group (room model revision 22, round RL4), through the
//! reducer: who may choose, one choice each, cleared and moved, a host's
//! own choice opening the countdown, its early end, a countdown canceled
//! when its choices all go, the draw (its entries, its winner, the
//! distribution over many rolls), the choices cleared after, each Repeats
//! rule, a chosen song kept from removal, a chooser leaving, kicked or
//! turning to Watch, and a room stored by revision 21. Every revision's
//! events are applied to a mirror, which must stay equal.

mod common;

use common::{Room, chart, chart_line, new_song};
use ddi_room::*;

/// A room under Song choice: Group with `repeats`, songs 1 to `songs` there
/// to load (the host's), Ana and Bob who play, Wes who watches.
struct Group {
    r: Room,
    ana: ParticipantId,
    bob: ParticipantId,
    wes: ParticipantId,
    ids: Vec<RoomSongId>,
}

fn group(repeats: Repeats, songs: u32) -> Group {
    let mut rules = MatchRules::hosted();
    rules.stages = 3;
    rules.choice = SongChoice::Group;
    rules.repeats = repeats;
    let mut r = Room::open(
        "host",
        "Host",
        RoomPolicy::default().with_live().with_audience(),
        rules,
        ContentSource::Shared,
    );
    let host = r.host;
    let ana = r.join_as("ana", Intent::Play, false).unwrap();
    let bob = r.join_as("bob", Intent::Play, false).unwrap();
    let wes = r.join_as("wes", Intent::Watch, false).unwrap();
    let mut ids = Vec::new();
    for n in 1..=songs {
        let mut song = new_song(n);
        song.charts = vec![chart_line(n, 0), chart_line(n, 1)];
        r.by(host, Command::AddSong { song: Box::new(song) }).unwrap();
        let id = r.s.songs.by_key(&new_song(n).key).unwrap().id;
        r.sys(Command::SetSongContent {
            song: id,
            status: ContentStatus::Available,
            share: Some(format!("share{:03}", id.0)),
            charts: None,
        })
        .unwrap();
        ids.push(id);
    }
    Group { r, ana, bob, wes, ids }
}

impl Group {
    /// `who` chooses song `n` (its Hard chart).
    fn choose(&mut self, who: ParticipantId, n: u32) -> Result<Vec<Event>, Rejection> {
        let song = self.ids[(n - 1) as usize];
        self.r.by(
            who,
            Command::Choose {
                song: Some(song),
                chart: Some(chart(n, 1)),
            },
        )
    }

    fn clear(&mut self, who: ParticipantId) -> Result<Vec<Event>, Rejection> {
        self.r.by(who, Command::Choose { song: None, chart: None })
    }

    fn chosen(&self, who: ParticipantId) -> Option<RoomSongId> {
        self.r.s.choices.get(&who).map(|c| c.song)
    }

    /// The countdown runs to its end and the authority draws with `roll`.
    fn draw(&mut self, roll: u64) -> Vec<Event> {
        let at = self.r.s.draw_ends_at().expect("a countdown");
        self.r.wait_to(at.0);
        assert!(self.r.s.draw_due(Millis(self.r.now)));
        self.r.sys(Command::Draw { roll }).unwrap()
    }

    /// The stage just drawn is played to its end by the host, Ana and Bob.
    fn play(&mut self) {
        let stage = self.r.s.upcoming_stage().unwrap();
        let host = self.r.host;
        for who in [host, self.ana, self.bob] {
            self.r.load_for(who, stage, Loading::Loaded).unwrap();
            self.r.ready_for(who, stage, true).unwrap();
        }
        let start = match self.r.s.phase {
            Phase::Countdown { start_at, .. } => start_at.0,
            ref p => panic!("not counting down: {p:?}"),
        };
        self.r.wait_to(start);
        for who in [host, self.ana, self.bob] {
            self.r
                .by(
                    who,
                    Command::ReportOutcome {
                        stage,
                        run: ReportedRun::Live {
                            life_ppm: 500_000,
                            score: 1000 + who.0 as i64,
                            steps: 0,
                        },
                        detail: None,
                    },
                )
                .unwrap();
        }
    }

    fn picked(&self) -> RoomSongId {
        let stage = self.r.s.upcoming_stage().unwrap();
        self.r.s.stage(stage).unwrap().content.as_ref().unwrap().song.unwrap()
    }
}

/// Only those who will play choose: a watcher is told why; under Host
/// nobody chooses; CPU practice cannot have Group at all.
#[test]
fn only_those_who_will_play_choose_and_only_under_group() {
    let mut g = group(Repeats::Allowed, 3);
    let host = g.r.host;
    let wes = g.wes;
    assert_eq!(
        g.choose(wes, 1),
        Err(Rejection::CannotChoose {
            why: NoChoice::Watching
        })
    );
    let ana = g.ana;
    g.choose(ana, 1).unwrap();
    assert_eq!(g.chosen(ana), Some(g.ids[0]));
    // A chart that is not the song's, a song not listed.
    let id = g.ids[0];
    assert_eq!(
        g.r.by(ana, Command::Choose { song: Some(id), chart: Some(chart(2, 1)) }),
        Err(Rejection::ChartNotInSong)
    );
    assert_eq!(
        g.r.by(ana, Command::Choose { song: Some(RoomSongId(99)), chart: Some(chart(1, 1)) }),
        Err(Rejection::NotListed)
    );
    // Under Host nobody chooses, and the standing choices go.
    let mut host_rules = g.r.s.next.rules.clone();
    host_rules.choice = SongChoice::Host;
    g.r.by(host, Command::SaveNextRules { rules: host_rules.clone() }).unwrap();
    assert!(g.r.s.choices.is_empty());
    assert_eq!(g.choose(ana, 1), Err(Rejection::NotGroupChoice));
    // The host's pick is the pick again; under Group it is refused.
    let mut group_rules = host_rules;
    group_rules.choice = SongChoice::Group;
    g.r.by(host, Command::SaveNextRules { rules: group_rules }).unwrap();
    assert_eq!(
        g.r.by(host, Command::PickListed { song: id, chart: chart(1, 1) }),
        Err(Rejection::GroupChooses)
    );
    // Practice has no list: Group is refused.
    let mut p = Room::open(
        "solo",
        "Solo",
        RoomPolicy::default(),
        MatchRules::hosted(),
        ContentSource::LocalLibrary,
    );
    let mut rules = MatchRules::hosted();
    rules.choice = SongChoice::Group;
    let solo = p.host;
    assert_eq!(
        p.by(solo, Command::SaveNextRules { rules }),
        Err(Rejection::InvalidRules(RulesProblem::GroupNeedsList))
    );
}

/// Round LO: with more newcomers waiting than places for the next stage,
/// only those who will be seated choose (in arrival order, as they are
/// seated); the rest are refused as waiting for the next match and their
/// state says every place is taken. A place that frees lets the next one
/// choose.
#[test]
fn only_the_newcomers_who_will_be_seated_choose() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.choose(host, 1).unwrap();
    g.draw(0);
    g.play();
    assert!(matches!(g.r.s.phase, Phase::Intermission { .. }));
    let cy = g.r.join_as("cy", Intent::Play, false).unwrap();
    let di = g.r.join_as("di", Intent::Play, false).unwrap();
    // One place left beside the host, Ana and Bob. (A join counts the
    // people who want to play against the cap, a place counts the
    // match's CPUs too: a match with CPUs seats fewer newcomers than join
    // to play; the cap lowered here stands for them.)
    g.r.s.policy.max_competitors = 4;
    g.r.mirror.policy.max_competitors = 4;
    assert_eq!(g.r.s.late_places(), 1);
    assert_eq!(g.r.s.participation(cy), Some(Participation::WaitingNextStage));
    assert_eq!(g.r.s.participation(di), Some(Participation::WaitingPlace));
    g.choose(cy, 2).unwrap();
    assert_eq!(
        g.choose(di, 3),
        Err(Rejection::CannotChoose {
            why: NoChoice::NextMatch
        })
    );
    let choosers = g.r.s.choosers();
    assert!(choosers.contains(&cy) && !choosers.contains(&di), "{choosers:?}");
    assert!([host, ana, bob].iter().all(|p| choosers.contains(p)));
    // Bob leaves: a place for Di too.
    g.r.by(bob, Command::Leave).unwrap();
    assert_eq!(g.r.s.participation(di), Some(Participation::WaitingNextStage));
    g.choose(di, 3).unwrap();
}

/// One choice each: Enter on another song moves it, clearing takes it
/// back, the same again changes nothing; a member's choice opens no
/// countdown, the host's own does, named as theirs.
#[test]
fn a_hosts_own_choice_opens_the_countdown() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.r.now = 5_000;
    g.choose(ana, 1).unwrap();
    g.choose(ana, 2).unwrap();
    assert_eq!(g.chosen(ana), Some(g.ids[1]));
    let rev = g.r.s.revision;
    g.choose(ana, 2).unwrap();
    assert_eq!(g.r.s.revision, rev, "the same choice: nothing changed");
    g.clear(ana).unwrap();
    assert_eq!(g.chosen(ana), None);
    g.choose(ana, 2).unwrap();
    g.choose(bob, 3).unwrap();
    assert!(g.r.s.draw.is_none(), "members alone: no countdown");
    let events = g.choose(host, 1).unwrap();
    let draw = g.r.s.draw.clone().unwrap();
    assert!(events.contains(&Event::DrawOpened { draw: draw.clone() }));
    assert_eq!((draw.by, draw.opened_at, draw.ends_at), (host, Millis(5_000), Millis(15_000)));
    assert_eq!(draw.stage, g.r.s.pick_stage().unwrap());
    // Everyone who may choose has: the draw comes three seconds after
    // its opening at the earliest.
    assert_eq!(g.r.s.draw_ends_at(), Some(Millis(8_000)));
    assert_eq!(g.r.s.next_deadline(), Some(Millis(8_000)));
    // A choice changed in it counts until it ends.
    g.choose(bob, 1).unwrap();
    assert_eq!(g.r.s.chosen_by(g.ids[0]), vec![host, bob]);
}

/// Not everyone chose: the countdown runs its ten seconds; at its end the
/// authority draws; before it, a draw is refused.
#[test]
fn the_countdown_runs_its_length_until_everyone_chose() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana) = (g.r.host, g.ana);
    g.r.now = 1_000;
    g.choose(host, 1).unwrap();
    assert_eq!(g.r.s.draw_ends_at(), Some(Millis(11_000)));
    g.r.wait_to(9_000);
    g.choose(ana, 2).unwrap();
    assert_eq!(g.r.s.draw_ends_at(), Some(Millis(11_000)), "Bob has not chosen");
    assert_eq!(g.r.sys(Command::Draw { roll: 1 }), Err(Rejection::NoDrawDue));
    g.r.wait_to(11_000);
    assert!(g.r.s.draw_due(Millis(11_000)));
}

/// The host clears a lone choice: the countdown is canceled; a co-host's
/// choice opens it again; a member's alone left keeps it running.
#[test]
fn a_countdown_whose_choices_all_go_is_canceled() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana) = (g.r.host, g.ana);
    g.choose(host, 1).unwrap();
    assert!(g.r.s.draw.is_some());
    let events = g.clear(host).unwrap();
    assert!(events.contains(&Event::DrawCanceled { why: DrawEnd::Empty }));
    assert!(g.r.s.draw.is_none());
    // Ana's choice alone opens nothing; a co-host's does, and goes on
    // with Ana's once the co-host clears theirs.
    g.r.by(host, Command::SetCoHost { participant: ana, on: true }).unwrap();
    g.choose(ana, 2).unwrap();
    assert_eq!(g.r.s.draw.as_ref().map(|d| d.by), Some(ana));
    g.r.by(host, Command::SetCoHost { participant: ana, on: false }).unwrap();
    assert!(g.r.s.draw.is_some(), "losing the role keeps the countdown");
    let bob = g.bob;
    g.choose(bob, 3).unwrap();
    g.clear(ana).unwrap();
    assert!(g.r.s.draw.is_some(), "Bob's choice still stands");
    g.clear(bob).unwrap();
    assert!(g.r.s.draw.is_none());
}

/// The draw: one entry a choosing member, by id, the winner the roll's
/// remainder; the drawn song picked (available at once, its chart the
/// winner's), the choices cleared, the record kept; the stage plays and
/// the next stage's choices start empty.
#[test]
fn the_draw_picks_one_members_choice_and_clears_them_all() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.choose(ana, 2).unwrap();
    g.choose(bob, 3).unwrap();
    g.choose(host, 1).unwrap();
    // Three entries by id: the host's, Ana's, Bob's; roll 4 is Ana's.
    let events = g.draw(4);
    let record = g.r.s.drawn.clone().unwrap();
    assert!(matches!(events[0], Event::Drawn { .. }));
    assert_eq!(
        record.entries.iter().map(|e| e.participant).collect::<Vec<_>>(),
        vec![host, ana, bob]
    );
    assert_eq!((record.winner, record.roll), (1, 4));
    assert_eq!(record.won().unwrap().song, g.ids[1]);
    assert!(g.r.s.choices.is_empty() && g.r.s.draw.is_none());
    assert_eq!(g.picked(), g.ids[1]);
    let stage = g.r.s.upcoming_stage().unwrap();
    let c = g.r.s.stage(stage).unwrap().content.clone().unwrap();
    assert_eq!((c.status, c.id.chart.clone()), (ContentStatus::Available, chart(2, 1)));
    assert!(matches!(g.r.s.phase, Phase::Preparing { window: Some(_), .. }));
    // A pick may not be made while the window is open; choices for the
    // next stage may.
    g.choose(ana, 3).unwrap();
    assert!(g.r.s.draw.is_none(), "no pick now: nothing counts down");
    g.choose(host, 1).unwrap();
    assert!(g.r.s.draw.is_none());
    g.play();
    assert!(matches!(g.r.s.phase, Phase::Intermission { .. }));
    // Round FX: the host's choice made during the stage waits (before,
    // the stage's end opened the countdown by itself); made again between
    // stages, it opens the next countdown.
    assert!(g.r.s.draw.is_none());
    assert_eq!(g.r.s.choices.len(), 2);
    g.choose(host, 1).unwrap();
    assert!(g.r.s.draw.is_some());
    assert_eq!(g.r.s.choices.len(), 2);
    // The audience sees the choices with what they name, the countdown
    // and the last draw.
    let view = ddi_room::audience::PublicView::of(&g.r.s, "Room");
    assert_eq!(view.rules.choice, SongChoice::Group);
    assert_eq!(view.choices.len(), 2);
    assert_eq!(view.choices[0].card.title, "Song 1");
    assert!(view.draw.is_some());
    let drawn = view.drawn.unwrap();
    assert_eq!((drawn.winner, drawn.entries[1].card.title.as_str()), (1, "Song 2"));
}

/// A song two members chose has two chances: over a thousand rolls, the
/// draw lands on it about twice as often as on the song chosen once.
#[test]
fn a_song_chosen_twice_has_two_chances() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.choose(ana, 1).unwrap();
    g.choose(bob, 2).unwrap();
    g.choose(host, 1).unwrap();
    let at = g.r.s.draw_ends_at().unwrap();
    g.r.wait_to(at.0);
    let mut once = 0;
    let mut twice = 0;
    for seed in 0..1200u64 {
        let mut s = g.r.s.clone();
        let roll = ddi_room::ids::splitmix64(seed);
        s.apply(Actor::Authority, None, &Command::Draw { roll }, at).unwrap();
        let stage = s.upcoming_stage().unwrap();
        let song = s.stage(stage).unwrap().content.as_ref().unwrap().song.unwrap();
        if song == g.ids[0] {
            twice += 1;
        } else {
            assert_eq!(song, g.ids[1]);
            once += 1;
        }
    }
    assert_eq!(once + twice, 1200);
    assert!((740..=860).contains(&twice), "{twice} of 1200 for the song chosen twice");
}

/// Repeats: Not in a row refuses the song just played and gives way when
/// it is the only one; Not in a match refuses every song played in the
/// match, and gives way when all have been.
#[test]
fn the_repeats_rules_refuse_in_words_and_give_way() {
    let mut g = group(Repeats::NotInARow, 2);
    let (host, ana) = (g.r.host, g.ana);
    g.choose(host, 1).unwrap();
    g.draw(0);
    g.play();
    assert_eq!(g.choose(ana, 1), Err(Rejection::Repeated { rule: Repeats::NotInARow }));
    assert_eq!(g.r.s.excluded_songs().1.into_iter().collect::<Vec<_>>(), vec![g.ids[0]]);
    g.choose(ana, 2).unwrap();
    // Song 2 removed: song 1 alone is there, and may be chosen again.
    let mut g = group(Repeats::NotInARow, 1);
    let host = g.r.host;
    g.choose(host, 1).unwrap();
    g.draw(0);
    g.play();
    assert!(g.r.s.excluded_songs().1.is_empty(), "the rule gives way");
    let ana = g.ana;
    g.choose(ana, 1).unwrap();

    let mut g = group(Repeats::NotInAMatch, 3);
    let (host, ana) = (g.r.host, g.ana);
    for n in [1, 2] {
        g.choose(host, n).unwrap();
        g.draw(0);
        g.play();
    }
    for n in [1, 2] {
        assert_eq!(g.choose(ana, n), Err(Rejection::Repeated { rule: Repeats::NotInAMatch }));
    }
    g.choose(ana, 3).unwrap();
    // A list of two, both played: any may be chosen again.
    let mut g = group(Repeats::NotInAMatch, 2);
    let host = g.r.host;
    for n in [1, 2] {
        g.choose(host, n).unwrap();
        g.draw(0);
        g.play();
    }
    assert!(g.r.s.excluded_songs().1.is_empty());
}

/// A standing choice keeps its song in the list: its removal is refused
/// and a full list under Remove the oldest passes it over.
#[test]
fn a_chosen_song_is_kept_from_removal() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana) = (g.r.host, g.ana);
    g.choose(ana, 1).unwrap();
    let id = g.ids[0];
    assert_eq!(g.r.by(host, Command::RemoveSong { song: id }), Err(Rejection::SongInUse));
    g.r.by(host, Command::SetSongList { rules: ListRules { size: 3, when_full: WhenFull::RemoveOldest } })
        .unwrap();
    let events = g.r.by(host, Command::AddSong { song: Box::new(new_song(9)) }).unwrap();
    assert!(events.contains(&Event::SongRemoved {
        song: g.ids[1],
        why: Removal::ListFull,
        share: Some(format!("share{:03}", g.ids[1].0)),
    }));
    assert!(g.r.s.songs.get(id).is_some());
}

/// The authority's removal of a chosen song (round AT: the server's
/// operator, a reported song) goes through where a host's is refused:
/// the choices of it go, each chooser told as for any song gone
/// (`ChoiceGone::Song`), a countdown with no choice left canceled, one
/// with another choice still standing going on; a song a stage not yet
/// over plays is still refused, to a host and to the authority.
#[test]
fn the_authoritys_removal_takes_the_choices_of_its_song() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    let (one, two) = (g.ids[0], g.ids[1]);
    g.choose(ana, 1).unwrap();
    g.choose(host, 1).unwrap();
    assert!(g.r.s.draw.is_some(), "the host's choice opened the countdown");
    assert_eq!(g.r.by(host, Command::RemoveSong { song: one }), Err(Rejection::SongInUse));
    let events = g.r.sys(Command::RemoveSong { song: one }).unwrap();
    assert_eq!(
        events[0],
        Event::SongRemoved {
            song: one,
            why: Removal::Removed,
            share: Some(format!("share{:03}", one.0)),
        }
    );
    for who in [host, ana] {
        assert!(events.contains(&Event::ChoiceChanged {
            participant: who,
            choice: None,
            why: Some(ChoiceGone::Song),
        }));
        assert_eq!(g.chosen(who), None);
    }
    assert!(events.contains(&Event::DrawCanceled { why: DrawEnd::Empty }));
    assert!(g.r.s.draw.is_none());
    assert!(g.r.s.songs.get(one).is_none());
    // Another choice still standing: the countdown goes on.
    g.choose(bob, 3).unwrap();
    g.choose(host, 2).unwrap();
    g.choose(ana, 2).unwrap();
    assert!(g.r.s.draw.is_some());
    let events = g.r.sys(Command::RemoveSong { song: two }).unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::DrawCanceled { .. })));
    assert!(g.r.s.draw.is_some(), "Bob's choice still stands");
    assert_eq!(g.chosen(bob), Some(g.ids[2]));
    assert_eq!((g.chosen(host), g.chosen(ana)), (None, None));
    // The draw's song, its stage picked: refused to both.
    g.draw(0);
    let three = g.ids[2];
    assert!(g.r.s.song_in_stage(g.r.s.songs.get(three).unwrap()));
    assert_eq!(g.r.by(host, Command::RemoveSong { song: three }), Err(Rejection::SongInUse));
    assert_eq!(g.r.sys(Command::RemoveSong { song: three }), Err(Rejection::SongInUse));
}

/// A chooser who leaves, is kicked or turns to Watch takes their choice
/// with them; the countdown goes on while a choice stands and is
/// canceled when none does. One away past the reconnect grace too.
#[test]
fn a_chooser_gone_takes_their_choice() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    let cy = g.r.join_as("cy", Intent::Play, false).unwrap();
    let di = g.r.join_as("di", Intent::Play, false).unwrap();
    for (who, n) in [(ana, 1), (bob, 2), (cy, 3), (di, 1)] {
        g.choose(who, n).unwrap();
    }
    g.choose(host, 1).unwrap();
    g.r.by(ana, Command::Leave).unwrap();
    assert_eq!(g.chosen(ana), None);
    g.r.by(host, Command::Remove { participant: bob }).unwrap();
    assert_eq!(g.chosen(bob), None);
    g.r.by(cy, Command::SetIntent { intent: Intent::Watch }).unwrap();
    assert_eq!(g.chosen(cy), None);
    g.r.sys(Command::SetConnection { participant: di, connection: Connection::Disconnected })
        .unwrap();
    assert_eq!(g.chosen(di), None);
    assert!(g.r.s.draw.is_some(), "the host's own still stands");
    // The host leaves too: the room passes to... nobody who chose: canceled.
    g.r.by(host, Command::Leave).unwrap();
    assert!(g.r.s.choices.is_empty());
    assert!(g.r.s.draw.is_none());
}

/// The draw's pick needs what a pick needs: with too few players at its
/// end, the countdown is canceled and the choices stand; it opens again
/// once someone is back.
#[test]
fn a_draw_that_cannot_start_is_canceled_and_the_choices_stand() {
    let mut g = group(Repeats::Allowed, 2);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.r.by(bob, Command::SetIntent { intent: Intent::Watch }).unwrap();
    g.choose(ana, 2).unwrap();
    g.choose(host, 1).unwrap();
    assert!(g.r.s.draw.is_some());
    g.r.now += 1;
    g.r.by(ana, Command::SetIntent { intent: Intent::Watch }).unwrap();
    // Ana's choice went with her Play; the host's alone stands, and the
    // countdown with it, but a draw now could not start a match of one.
    assert!(g.r.s.draw.is_none(), "{:?}", g.r.s.draw);
    assert!(g.r.s.draw_refusal(host, Millis(g.r.now)).is_some());
    assert_eq!(g.chosen(host), Some(g.ids[0]));
    g.r.by(ana, Command::SetIntent { intent: Intent::Play }).unwrap();
    assert!(g.r.s.draw.is_some(), "opened again once a draw could start");
}

/// Song choice changes from a later stage like the other per-stage rules:
/// a match under Host turned to Group from its next stage.
#[test]
fn song_choice_changes_from_a_later_stage() {
    let mut rules = MatchRules::hosted();
    rules.stages = 3;
    let mut r = Room::open("host", "Host", RoomPolicy::default(), rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join_as("ana", Intent::Play, false).unwrap();
    r.by(host, Command::AddSong { song: Box::new(new_song(1)) }).unwrap();
    let id = r.s.songs.by_key(&new_song(1).key).unwrap().id;
    r.sys(Command::SetSongContent {
        song: id,
        status: ContentStatus::Available,
        share: Some("share001".into()),
        charts: None,
    })
    .unwrap();
    r.by(host, Command::PickListed { song: id, chart: chart(1, 1) }).unwrap();
    let from = r.s.rules_land_on().unwrap();
    let m = r.s.current.as_ref().unwrap();
    let play = PlayRules {
        choice: SongChoice::Group,
        repeats: Repeats::NotInARow,
        ..m.play_rules_for(from)
    };
    r.by(host, Command::ChangeStageRules { from, rules: play }).unwrap();
    // The stage under way is the host's pick; the next is the group's.
    assert_eq!(r.s.choosing_for(), Some(from));
    assert_eq!(r.s.choice_rules(), (SongChoice::Group, Repeats::NotInARow));
    // The only song is the one under way: Not in a row gives way.
    r.by(ana, Command::Choose { song: Some(id), chart: Some(chart(1, 1)) }).unwrap();
    r.by(host, Command::AddSong { song: Box::new(new_song(2)) }).unwrap();
    let two = r.s.songs.by_key(&new_song(2).key).unwrap().id;
    r.sys(Command::SetSongContent {
        song: two,
        status: ContentStatus::Available,
        share: Some("share002".into()),
        charts: None,
    })
    .unwrap();
    // A second song there: the first is excluded again, Ana's choice gone.
    assert!(r.s.choices.is_empty());
    assert_eq!(
        r.by(ana, Command::Choose { song: Some(id), chart: Some(chart(1, 1)) }),
        Err(Rejection::Repeated { rule: Repeats::NotInARow })
    );
}

/// A room stored by revision 21 (`room-revision-21.json`, written by
/// revision 21's own reducer: a match between stages, a list under Who
/// can add songs: Everyone) reads as Host with no choices, writes back
/// what it read, and takes Group from its next stage.
#[test]
fn a_room_stored_by_revision_21_reads_as_host() {
    let text = include_str!("room-revision-21.json");
    let mut state: RoomState = serde_json::from_str(text).unwrap();
    let back = serde_json::to_value(&state).unwrap();
    assert_eq!(back, serde_json::from_str::<serde_json::Value>(text).unwrap(), "written as it was read");
    assert_eq!(state.choice_rules(), (SongChoice::Host, Repeats::Allowed));
    assert!(state.choices.is_empty() && state.draw.is_none() && state.drawn.is_none());
    let host = ParticipantId(1);
    let ana = state.member_by_identity(&Identity::new("twitch:ana")).unwrap().id;
    let song = state.songs.entries[1].id;
    let chart = state.songs.entries[1].charts[0].chart.clone();
    assert_eq!(
        state.choose_refusal(ana, song, &chart),
        Some(Rejection::NotGroupChoice)
    );
    // It plays as it did: the picker's pick of a listed song (round RL4b).
    let mut played = state.clone();
    played
        .apply(
            Actor::Member(host),
            Some(played.revision),
            &Command::PickListed { song, chart: chart.clone() },
            Millis(90_000),
        )
        .unwrap();
    assert!(matches!(played.phase, Phase::Preparing { window: Some(_), .. }));
    let from = state.rules_land_on().unwrap();
    let m = state.current.as_ref().unwrap();
    let play = PlayRules {
        choice: SongChoice::Group,
        ..m.play_rules_for(from)
    };
    state
        .apply(Actor::Member(host), Some(state.revision), &Command::ChangeStageRules { from, rules: play }, Millis(90_000))
        .unwrap();
    state
        .apply(Actor::Member(ana), Some(state.revision), &Command::Choose { song: Some(song), chart: Some(chart) }, Millis(90_000))
        .unwrap();
    assert_eq!(state.choices.len(), 1);
}

/// Round RL4b: the Ready countdown of a stage the draw picked counts from
/// the end of the draw's reveal, which every device shows before the
/// stage's Ready card: the players have the whole window to press Ready.
#[test]
fn the_ready_countdown_counts_from_the_reveals_end() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.choose(ana, 2).unwrap();
    g.choose(host, 1).unwrap();
    g.choose(bob, 3).unwrap();
    g.draw(1);
    let at = g.r.s.drawn.as_ref().unwrap().at;
    let stage = g.r.s.upcoming_stage().unwrap();
    // Everyone has the song at once (downloaded ahead): the loading
    // grace ends at once.
    for who in [host, ana, bob] {
        g.r.load_for(who, stage, Loading::Loaded).unwrap();
    }
    let window = g.r.s.policy.readiness.window_ms;
    match &g.r.s.phase {
        Phase::Preparing { window: Some(w), .. } => {
            assert!(w.loading_until.is_none(), "the loading is over");
            assert_eq!(w.deadline, at.after(DRAW_V0.reveal_ms).after(window));
        }
        p => panic!("not gathering readiness: {p:?}"),
    }
}

/// Round RL4b: a match stopped while the draw counts down takes the
/// choices made for its next stage with it, each maker told why; nothing
/// opens again by itself (before, the next change of any kind opened a
/// countdown for a new match's first stage).
#[test]
fn a_match_stopped_during_the_countdown_takes_the_choices_with_it() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana) = (g.r.host, g.ana);
    g.choose(host, 1).unwrap();
    g.draw(0);
    g.play();
    assert!(matches!(g.r.s.phase, Phase::Intermission { .. }));
    g.choose(ana, 2).unwrap();
    g.choose(host, 3).unwrap();
    assert!(g.r.s.draw.is_some());
    let events = g.r.by(host, Command::StopMatch).unwrap();
    // Ana is told why; the host, who stopped it, knows.
    for (who, why) in [(host, None), (ana, Some(ChoiceGone::Stage))] {
        assert!(
            events.contains(&Event::ChoiceChanged {
                participant: who,
                choice: None,
                why,
            }),
            "{events:?}"
        );
    }
    assert!(events.contains(&Event::DrawCanceled { why: DrawEnd::Rules }));
    assert!(g.r.s.choices.is_empty() && g.r.s.draw.is_none());
    // Time passes and someone else acts: no countdown opens.
    g.r.wait(1_000);
    let bob = g.bob;
    g.r.by(bob, Command::SetIntent { intent: Intent::Watch }).unwrap();
    assert!(g.r.s.draw.is_none());
    // The host's choice for the new match opens it again.
    g.choose(host, 3).unwrap();
    g.r.by(bob, Command::SetIntent { intent: Intent::Play }).unwrap();
    assert!(g.r.s.draw.is_some());
}

/// Round RL4b: a choice the room takes away says why (Song choice turned
/// to Host, the Repeats rule tightened), so its maker is told; one's own
/// clearing says nothing.
#[test]
fn a_choice_the_room_takes_away_says_why() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.choose(host, 1).unwrap();
    g.draw(0);
    g.play();
    // Song 1 was played; Ana chooses it again (Repeats: Allowed).
    g.choose(ana, 1).unwrap();
    let events = g.clear(ana).unwrap();
    assert!(events.contains(&Event::ChoiceChanged { participant: ana, choice: None, why: None }));
    g.choose(ana, 1).unwrap();
    g.choose(bob, 2).unwrap();
    let change = |g: &mut Group, f: &dyn Fn(&mut PlayRules)| {
        let from = g.r.s.rules_land_on().unwrap();
        let mut play = g.r.s.current.as_ref().unwrap().play_rules_for(from);
        f(&mut play);
        let host = g.r.host;
        g.r.by(host, Command::ChangeStageRules { from, rules: play }).unwrap()
    };
    let events = change(&mut g, &|p| p.repeats = Repeats::NotInARow);
    assert!(events.contains(&Event::ChoiceChanged {
        participant: ana,
        choice: None,
        why: Some(ChoiceGone::Repeats),
    }));
    assert_eq!(g.chosen(bob), Some(g.ids[1]), "Bob's song was not played");
    g.choose(host, 3).unwrap();
    let events = change(&mut g, &|p| p.choice = SongChoice::Host);
    assert!(events.contains(&Event::ChoiceChanged {
        participant: bob,
        choice: None,
        why: Some(ChoiceGone::Host),
    }));
    // The host's own change takes the host's choice without a why.
    assert!(events.contains(&Event::ChoiceChanged { participant: host, choice: None, why: None }));
}

/// Round RL4b: a full list whose every song somebody chose takes no new
/// song, even under Remove the oldest (no song may make room).
#[test]
fn a_full_list_of_chosen_songs_refuses_an_addition() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.r.by(host, Command::SetSongList { rules: ListRules { size: 3, when_full: WhenFull::RemoveOldest } })
        .unwrap();
    for (who, n) in [(ana, 1), (bob, 2), (host, 3)] {
        g.choose(who, n).unwrap();
    }
    assert_eq!(g.r.s.add_refusal(host, None), Some(Rejection::ListFull));
    assert_eq!(
        g.r.by(host, Command::AddSong { song: Box::new(new_song(9)) }),
        Err(Rejection::ListFull)
    );
    // One choice taken back: its song makes room.
    g.clear(ana).unwrap();
    g.r.by(host, Command::AddSong { song: Box::new(new_song(9)) }).unwrap();
    assert!(g.r.s.songs.get(g.ids[0]).is_none(), "the oldest not chosen went");
}

/// Round RL4b: Repeats counts what was played or is picked, never a pick
/// whose start was called off: a drawn song whose stage a host canceled
/// may be chosen again for that same stage under Not in a match and Not
/// in a row, while the stage before it stays excluded under Not in a row.
#[test]
fn repeats_pass_over_a_canceled_start() {
    for repeats in [Repeats::NotInAMatch, Repeats::NotInARow] {
        let mut g = group(repeats, 3);
        let (host, ana) = (g.r.host, g.ana);
        g.choose(host, 1).unwrap();
        g.draw(0);
        g.play();
        // Stage 2: song 2 drawn, then its start canceled by the host.
        g.choose(host, 2).unwrap();
        g.draw(0);
        let stage = g.r.s.upcoming_stage().unwrap();
        g.r.by(host, Command::CancelStart { stage }).unwrap();
        assert!(matches!(g.r.s.phase, Phase::Preparing { window: None, .. }));
        assert_eq!(g.r.s.choosing_for(), Some(stage));
        let excluded = g.r.s.excluded_songs().1;
        assert!(!excluded.contains(&g.ids[1]), "{repeats:?}: the canceled pick was never played");
        assert!(excluded.contains(&g.ids[0]), "{repeats:?}: stage 1's song");
        g.choose(ana, 2).unwrap();
    }
}

/// Round FX: a choice made while the room cannot pick (a stage readied or
/// played) is a choice all the same (shown, counted, drawn among) but a
/// host's or co-host's opens no countdown, at the stage's end or ever by
/// itself; made again once the room can pick (here another song; Enter on
/// the same song in `a_hosts_choice_made_during_a_stage_waits_through_both_authorities`),
/// it does. A member's choice never starts it either way.
#[test]
fn a_choice_made_while_the_room_cannot_pick_waits_for_its_maker() {
    let mut g = group(Repeats::Allowed, 3);
    let (host, ana, bob) = (g.r.host, g.ana, g.bob);
    g.r.by(host, Command::SetCoHost { participant: ana, on: true }).unwrap();
    g.choose(host, 1).unwrap();
    g.draw(0);
    // The drawn stage readied: the room cannot pick.
    assert!(g.r.s.pick_stage().is_none());
    g.choose(ana, 2).unwrap();
    g.choose(bob, 3).unwrap();
    assert!(g.r.s.choices.values().all(|c| c.waits));
    let stage = g.r.s.upcoming_stage().unwrap();
    for who in [host, ana, bob] {
        g.r.load_for(who, stage, Loading::Loaded).unwrap();
        g.r.ready_for(who, stage, true).unwrap();
    }
    let start = match g.r.s.phase {
        Phase::Countdown { start_at, .. } => start_at.0,
        ref p => panic!("not counting down: {p:?}"),
    };
    g.r.wait_to(start);
    // During the play the host chooses too.
    assert!(matches!(g.r.s.phase, Phase::Stage { .. }));
    g.choose(host, 3).unwrap();
    assert!(g.r.s.choices[&host].waits);
    for who in [host, ana, bob] {
        g.r.by(
            who,
            Command::ReportOutcome {
                stage,
                run: ReportedRun::Live {
                    life_ppm: 500_000,
                    score: 1000 + who.0 as i64,
                    steps: 0,
                },
                detail: None,
            },
        )
        .unwrap();
    }
    // The stage ended: every choice stands, nothing counts down, however
    // long the room waits.
    let now = g.r.now;
    g.r.wait_to(now + 60_000);
    assert!(g.r.s.pick_stage().is_some(), "{:?}", g.r.s.phase);
    assert!(g.r.s.draw.is_none(), "no countdown at a moment nobody chose");
    assert_eq!(g.r.s.choices.len(), 3);
    assert_eq!(g.r.s.chosen_by(g.ids[2]), vec![host, bob]);
    assert!(g.r.s.choice_waits_for(host) && g.r.s.choice_waits_for(ana));
    assert!(!g.r.s.choice_waits_for(bob), "a member starts nothing");
    assert_eq!(g.r.s.draw_starter(), None);
    // Bob moves his: still nothing.
    g.choose(bob, 2).unwrap();
    assert!(g.r.s.draw.is_none());
    // Ana, a co-host, moves hers to another song: made now, it opens the
    // countdown, named as hers; the host's still waits.
    let events = g.choose(ana, 1).unwrap();
    let draw = g.r.s.draw.clone().unwrap();
    assert!(events.contains(&Event::DrawOpened { draw: draw.clone() }));
    assert_eq!(draw.by, ana);
    assert!(!g.r.s.choices[&ana].waits && g.r.s.choices[&host].waits);
    // The draw takes every choice, a waiting one too.
    g.draw(0);
    assert_eq!(g.r.s.drawn.as_ref().unwrap().entries.len(), 3);
}
