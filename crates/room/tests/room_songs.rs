//! The room's song list (room model revision 17, round RL1), through the
//! reducer: adding (the hosts), the same song twice, removing, a full list
//! under both rules, a person's share of it, uploads done and failed, an
//! adder gone, a pick from the list and a room stored by revision 16.
//! Every revision's events are applied to a mirror, which must stay equal.

mod common;

use common::Room;
use ddi_room::*;

/// A new online room's: the songs' shares, live play.
fn online() -> Room {
    let mut rules = MatchRules::hosted();
    rules.stages = 3;
    Room::open(
        "host",
        "Host",
        RoomPolicy::default().with_live().with_audience(),
        rules,
        ContentSource::Shared,
    )
}

/// Song `n`, as its adder's device describes it: two charts.
fn new_song(n: u32) -> NewSong {
    NewSong {
        key: SongKey::new(format!("s{n:015x}")),
        card: SongCard {
            title: format!("Song {n}"),
            artist: "Artist".into(),
            pack: "Pack".into(),
        },
        charts: vec![chart_line(n, 0), chart_line(n, 1)],
        bundled: false,
    }
}

fn chart_line(n: u32, k: u32) -> ChartLine {
    ChartLine {
        chart: ChartKey::new(format!("d{n}-{k}")),
        style: "dance-single".into(),
        difficulty: if k == 0 { "Easy" } else { "Hard" }.into(),
        meter: 3 + 6 * k,
    }
}

impl Room {
    fn add(&mut self, who: ParticipantId, n: u32) -> Result<RoomSongId, Rejection> {
        self.by(
            who,
            Command::AddSong {
                song: Box::new(new_song(n)),
            },
        )?;
        Ok(self.listed(n).expect("listed").id)
    }

    fn listed(&self, n: u32) -> Option<&RoomSong> {
        self.s.songs.by_key(&new_song(n).key)
    }

    /// The server's word on entry `song`'s upload: there, with its share.
    fn uploaded(&mut self, song: RoomSongId) {
        self.sys(Command::SetSongContent {
            song,
            status: ContentStatus::Available,
            share: Some(format!("share{:03}", song.0)),
            charts: None,
        })
        .unwrap();
    }

    fn cohost(&mut self, name: &str) -> ParticipantId {
        let id = self.join_as(name, Intent::Play, false).unwrap();
        let host = self.host;
        self.by(host, Command::SetCoHost { participant: id, on: true }).unwrap();
        id
    }

    fn keys(&self) -> Vec<String> {
        self.s
            .songs
            .entries
            .iter()
            .map(|e| e.card.title.clone())
            .collect()
    }
}

/// Added by a host: in the list for everyone at once (uploading), there to
/// load once the server says its upload is done, its charts then the
/// record's; a member may not add; the same song again is the same entry.
#[test]
fn a_host_adds_a_song_and_it_is_there_once_uploaded() {
    let mut r = online();
    let host = r.host;
    let ana = r.join_as("ana", Intent::Play, false).unwrap();
    assert!(r.s.may_add(host) && !r.s.may_add(ana));
    let id = r.add(host, 1).unwrap();
    let e = r.listed(1).unwrap().clone();
    assert_eq!((e.id, e.by, e.status.clone(), e.share.clone()), (id, host, ContentStatus::Pending, None));
    assert_eq!(e.added_at, r.s.revision);
    assert_eq!(r.s.songs.changed_at, r.s.revision);
    // The server read the record: two charts, the second's level another.
    let mut charts = new_song(1).charts;
    charts[1].meter = 10;
    r.sys(Command::SetSongContent {
        song: id,
        status: ContentStatus::Available,
        share: Some("share001".into()),
        charts: Some(charts.clone()),
    })
    .unwrap();
    let e = r.listed(1).unwrap();
    assert!(e.available());
    assert_eq!((e.share.as_deref(), e.charts.clone()), (Some("share001"), charts));
    // Told again the same: nothing changes, no revision.
    let rev = r.s.revision;
    r.uploaded(id);
    assert_eq!(r.s.revision, rev, "the same: nothing changed");
    // A member may not add; nor may anyone add it twice.
    assert_eq!(
        r.add(ana, 2),
        Err(Rejection::Unauthorized {
            capability: Capability::AddSongs
        })
    );
    assert_eq!(r.add(host, 1), Err(Rejection::AlreadyListed { song: id }));
    // A co-host adds; the client may only add a song within the bounds.
    let cy = r.cohost("cy");
    r.add(cy, 2).unwrap();
    let mut big = new_song(3);
    big.charts = (0..33).map(|k| chart_line(3, k)).collect();
    assert_eq!(
        r.by(cy, Command::AddSong { song: Box::new(big) }),
        Err(Rejection::InvalidSong)
    );
    let mut twice = new_song(3);
    twice.charts.push(chart_line(3, 0));
    assert_eq!(
        r.by(cy, Command::AddSong { song: Box::new(twice) }),
        Err(Rejection::InvalidSong)
    );
    // Only the authority says what an upload made.
    assert_eq!(
        r.by(
            host,
            Command::SetSongContent {
                song: id,
                status: ContentStatus::Available,
                share: None,
                charts: None
            }
        ),
        Err(Rejection::AuthorityOnly)
    );
    assert_eq!(r.keys(), ["Song 1", "Song 2"]);
}

/// A bundled song everyone has: listed by its content, there at once.
#[test]
fn a_bundled_song_is_there_at_once() {
    let mut r = online();
    let host = r.host;
    let mut s = new_song(7);
    s.bundled = true;
    r.by(host, Command::AddSong { song: Box::new(s) }).unwrap();
    let e = r.listed(7).unwrap();
    assert!(e.available() && e.bundled && e.share.is_none());
}

/// Removing: who added it or a host; never a song a stage not yet over
/// names; its share in the event, for the server to delete.
#[test]
fn removing_is_the_adders_or_a_hosts_and_never_a_song_in_use() {
    let mut r = online();
    let host = r.host;
    let cy = r.cohost("cy");
    let ana = r.join_as("ana", Intent::Play, false).unwrap();
    let a = r.add(cy, 1).unwrap();
    let b = r.add(cy, 2).unwrap();
    r.uploaded(a);
    r.uploaded(b);
    assert_eq!(
        r.by(ana, Command::RemoveSong { song: a }),
        Err(Rejection::Unauthorized {
            capability: Capability::AddSongs
        })
    );
    assert_eq!(r.by(host, Command::RemoveSong { song: RoomSongId(99) }), Err(Rejection::NotListed));
    // Picked: in use until its stage is over.
    r.by(
        host,
        Command::PickListed {
            song: b,
            chart: chart_line(2, 1).chart,
        },
    )
    .unwrap();
    assert_eq!(r.by(cy, Command::RemoveSong { song: b }), Err(Rejection::SongInUse));
    let events = r.by(host, Command::RemoveSong { song: a }).unwrap();
    assert!(events.contains(&Event::SongRemoved {
        song: a,
        why: Removal::Removed,
        share: Some("share001".into()),
    }));
    assert_eq!(r.keys(), ["Song 2"]);
    // Its window canceled, the stage still names it: still in use.
    let stage = r.s.upcoming_stage().unwrap();
    r.by(host, Command::CancelStart { stage }).unwrap();
    assert_eq!(r.by(cy, Command::RemoveSong { song: b }), Err(Rejection::SongInUse));
}

/// A full list under Remove the oldest (a new room's): the oldest song no
/// stage not yet over names goes; under Refuse new songs, refused. A
/// person adds at most half the list's size.
#[test]
fn a_full_list_removes_its_oldest_or_refuses_and_a_person_adds_half() {
    let mut r = online();
    let host = r.host;
    let cy = r.cohost("cy");
    let admin = r.join_as("admin", Intent::Watch, true).unwrap();
    assert_eq!(r.s.songs.rules, ListRules::default());
    assert_eq!(r.s.songs.rules.size, 100);
    // Only the owner sets the list's rules, within the bound.
    let small = ListRules {
        size: 4,
        when_full: WhenFull::RemoveOldest,
    };
    assert_eq!(
        r.by(cy, Command::SetSongList { rules: small }),
        Err(Rejection::Unauthorized {
            capability: Capability::ChangeAccess
        })
    );
    for size in [0, 101] {
        assert_eq!(
            r.by(
                host,
                Command::SetSongList {
                    rules: ListRules { size, ..small }
                }
            ),
            Err(Rejection::InvalidSong)
        );
    }
    r.by(host, Command::SetSongList { rules: small }).unwrap();
    let ids: Vec<RoomSongId> = [(host, 1), (host, 2), (cy, 3), (cy, 4)]
        .into_iter()
        .map(|(who, n)| r.add(who, n).unwrap())
        .collect();
    for id in &ids {
        r.uploaded(*id);
    }
    // Half the list's size a person: the host's third is refused.
    assert_eq!(r.add(host, 5), Err(Rejection::TooManySongs { limit: 2 }));
    // The oldest is picked: in use, the next oldest goes for the fifth.
    r.by(
        host,
        Command::PickListed {
            song: ids[0],
            chart: chart_line(1, 0).chart,
        },
    )
    .unwrap();
    let events = r
        .by(admin, Command::AddSong { song: Box::new(new_song(5)) })
        .unwrap();
    assert_eq!(
        events[0],
        Event::SongRemoved {
            song: ids[1],
            why: Removal::ListFull,
            share: Some("share002".into()),
        }
    );
    assert_eq!(r.keys(), ["Song 1", "Song 3", "Song 4", "Song 5"]);
    // Refuse new songs: the next is refused, nothing removed.
    r.by(
        host,
        Command::SetSongList {
            rules: ListRules {
                when_full: WhenFull::Refuse,
                ..small
            },
        },
    )
    .unwrap();
    assert_eq!(r.add(admin, 6), Err(Rejection::ListFull));
    // A smaller size removes nothing by itself; the next addition makes
    // room down to it (Remove the oldest), the song in use kept.
    r.by(
        host,
        Command::SetSongList {
            rules: ListRules {
                size: 2,
                when_full: WhenFull::RemoveOldest,
            },
        },
    )
    .unwrap();
    assert_eq!(r.s.songs.len(), 4);
    // (Half of two: one a person, and the admin has one.)
    assert_eq!(r.add(admin, 6), Err(Rejection::TooManySongs { limit: 1 }));
    let late = r.cohost("dee");
    r.add(late, 6).unwrap();
    assert_eq!(r.keys(), ["Song 1", "Song 6"]);
}

/// The picker picks a chart of a listed song: the stage's song there at
/// once with the entry's share, its card the entry's; not before the song
/// is uploaded, never a chart it does not have, never in practice.
#[test]
fn a_pick_from_the_list_is_available_at_once() {
    let mut r = online();
    let host = r.host;
    let ana = r.join_as("ana", Intent::Play, false).unwrap();
    let id = r.add(host, 1).unwrap();
    let pick = |r: &mut Room, who, chart: ChartKey| r.by(who, Command::PickListed { song: id, chart });
    assert_eq!(pick(&mut r, host, chart_line(1, 1).chart), Err(Rejection::SongNotReady));
    r.uploaded(id);
    assert_eq!(pick(&mut r, host, chart_line(2, 1).chart), Err(Rejection::ChartNotInSong));
    assert!(matches!(
        pick(&mut r, ana, chart_line(1, 1).chart),
        Err(Rejection::Unauthorized { .. })
    ));
    pick(&mut r, host, chart_line(1, 1).chart).unwrap();
    let stage = r.s.upcoming_stage().unwrap();
    let c = r.s.stage(stage).unwrap().content.clone().unwrap();
    assert_eq!(c.status, ContentStatus::Available);
    assert_eq!(c.share.as_deref(), Some("share001"));
    assert_eq!(c.song, Some(id));
    assert_eq!(c.id.song, new_song(1).key);
    assert_eq!(c.id.chart, chart_line(1, 1).chart);
    let card = c.card.unwrap();
    assert_eq!((card.title.as_str(), card.difficulty.as_str(), card.meter), ("Song 1", "Hard", 9));
    // As any pick from there on: everyone loads, readies, it starts.
    for p in [host, ana] {
        r.load_for(p, stage, Loading::Loaded).unwrap();
        r.ready_for(p, stage, true).unwrap();
    }
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    // The audience is told the list's count, no more.
    let view = audience::PublicView::of(&r.s, "room");
    assert_eq!(view.songs, 1);
    let text = serde_json::to_string(&view).unwrap();
    assert!(!text.contains("share001\",\"bundled") && !text.contains("\"by\""));
    // CPU practice has no list.
    let mut p = Room::open(
        "solo",
        "Solo",
        RoomPolicy::default(),
        MatchRules::hosted(),
        ContentSource::LocalLibrary,
    );
    let me = p.host;
    assert_eq!(p.add(me, 1), Err(Rejection::NoSongList));
    assert_eq!(
        p.by(
            me,
            Command::PickListed {
                song: id,
                chart: chart_line(1, 0).chart
            }
        ),
        Err(Rejection::NoSongList)
    );
}

/// An adder who goes before their upload is done takes the entry with
/// them (nobody else can finish it); their songs that are there stay.
#[test]
fn an_adder_gone_takes_their_unfinished_uploads() {
    let mut r = online();
    let host = r.host;
    let cy = r.cohost("cy");
    let done = r.add(cy, 1).unwrap();
    r.uploaded(done);
    let unfinished = r.add(cy, 2).unwrap();
    // Away for a moment: kept.
    r.sys(Command::SetConnection {
        participant: cy,
        connection: Connection::Reconnecting,
    })
    .unwrap();
    assert!(r.s.songs.get(unfinished).is_some());
    let events = r
        .sys(Command::SetConnection {
            participant: cy,
            connection: Connection::Disconnected,
        })
        .unwrap();
    assert!(events.contains(&Event::SongRemoved {
        song: unfinished,
        why: Removal::AdderGone,
        share: None,
    }));
    assert!(r.s.songs.get(done).is_some());
    // The host's own leaving the same way.
    let mine = r.add(host, 3).unwrap();
    r.by(host, Command::Leave).unwrap();
    assert!(r.s.songs.get(mine).is_none() || r.s.closed.is_some());
}

/// A room stored by revision 16 loads under revision 17 (round RL1):
/// the JSON revision 16's own reducer wrote (`room-revision-16.json`: an
/// online room whose host's resting song was shared ahead and picked for
/// stage 1, played; a co-host took the pick, rests on another song and
/// picked a third for stage 2, waiting for its share) reads with an empty
/// list, writes back what it reads, and plays on as it was; a host then
/// adds a song and picks it for the stage after.
#[test]
fn a_room_stored_by_revision_16_loads_with_an_empty_list() {
    let text = include_str!("room-revision-16.json");
    let mut state: RoomState = serde_json::from_str(text).unwrap();
    let back = serde_json::to_value(&state).unwrap();
    assert_eq!(
        back,
        serde_json::from_str::<serde_json::Value>(text).unwrap(),
        "written as it was read"
    );
    assert!(state.songs.is_empty());
    assert_eq!(state.songs.rules, ListRules::default());
    let stage2 = StageId(2);
    assert!(state.content_pending(stage2), "its pick waits for its share");
    // Its share comes as at revision 16, and the stage plays on.
    state
        .apply(
            Actor::Authority,
            None,
            &Command::SetContentStatus {
                stage: stage2,
                status: ContentStatus::Available,
                share: Some("abc12345".into()),
            },
            Millis(60_000),
        )
        .unwrap();
    assert!(!state.content_pending(stage2));
    // A host adds a song: the list's first entry.
    let host = ParticipantId(1);
    let revision = state.revision;
    state
        .apply(
            Actor::Member(host),
            Some(revision),
            &Command::AddSong {
                song: Box::new(new_song(1)),
            },
            Millis(61_000),
        )
        .unwrap();
    assert_eq!(state.songs.entries[0].id, RoomSongId(1));
    assert!(serde_json::to_value(&state).unwrap().get("songs").is_some());
}

/// What a list costs (round RL1's measure, `--ignored --nocapture` in a
/// release build): the state's JSON with a hundred typical entries (five
/// charts) and at the list's worst (every text at its bound, 32 charts),
/// and a command decided on it (the reducer copies the state for every
/// command; the list behind its shared pointer is not copied unless the
/// command changes it).
#[test]
#[ignore]
fn room_songs_measure() {
    use std::time::Instant;
    let fill = |worst: bool| {
        let mut r = online();
        let host = r.host;
        let cy = r.cohost("cy");
        r.join_as("ana", Intent::Play, false).unwrap();
        for n in 0..100u32 {
            let who = if n % 2 == 0 { host } else { cy };
            let long = |c: char, n: usize| std::iter::repeat_n(c, n).collect::<String>();
            let song = if worst {
                NewSong {
                    key: SongKey::new(format!("{n:0>64}")),
                    card: SongCard {
                        title: "😀".repeat(200),
                        artist: "😀".repeat(200),
                        pack: "😀".repeat(200),
                    },
                    charts: (0..32)
                        .map(|k| ChartLine {
                            chart: ChartKey::new(format!("{n:0>32}{k:0>32}")),
                            style: long('s', 32),
                            difficulty: long('d', 32),
                            meter: 99,
                        })
                        .collect(),
                    bundled: false,
                }
            } else {
                NewSong {
                    key: SongKey::new(format!("{:016x}", u64::from(n) * 0x9E37_79B9)),
                    card: SongCard {
                        title: format!("A Song Title Number {n}"),
                        artist: "Some Artist feat. Another".into(),
                        pack: "A Community Pack Vol. 3".into(),
                    },
                    charts: (0..5u32)
                        .map(|k| ChartLine {
                            chart: ChartKey::new(format!("{:032x}", u128::from(n) << 8 | u128::from(k))),
                            style: "dance-single".into(),
                            difficulty: ["Beginner", "Easy", "Medium", "Hard", "Challenge"]
                                [k as usize]
                                .into(),
                            meter: 2 + 3 * k,
                        })
                        .collect(),
                    bundled: false,
                }
            };
            r.by(who, Command::AddSong { song: Box::new(song) }).unwrap();
            let id = r.s.songs.entries.last().unwrap().id;
            r.uploaded(id);
        }
        r
    };
    for (what, worst) in [("typical (5 charts)", false), ("worst (32 charts, texts at their bounds)", true)] {
        let r = fill(worst);
        let list = serde_json::to_string(&*r.s.songs).unwrap().len();
        let state = serde_json::to_string(&r.s).unwrap().len();
        let mut no_list = r.s.clone();
        no_list.songs = Default::default();
        let bare = serde_json::to_string(&no_list).unwrap().len();
        // A command that leaves the list alone (a member's Ready flip
        // stands for it: Intent back and forth), and one that adds.
        let n = 2_000u32;
        let mut s = r.s.clone();
        let ana = ParticipantId(3);
        let t = Instant::now();
        for i in 0..n {
            let intent = if i % 2 == 0 { Intent::Watch } else { Intent::Play };
            let rev = s.revision;
            s.apply(Actor::Member(ana), Some(rev), &Command::SetIntent { intent }, Millis(1))
                .unwrap();
        }
        let other = t.elapsed().as_secs_f64() / f64::from(n) * 1e6;
        let mut s = r.s.clone();
        let t = Instant::now();
        let rounds = 200u32;
        for i in 0..rounds {
            let id = s.songs.entries[0].id;
            let rev = s.revision;
            s.apply(Actor::Member(r.host), Some(rev), &Command::RemoveSong { song: id }, Millis(1))
                .unwrap();
            let mut song = new_song(1_000 + i);
            song.charts = (0..5).map(|k| chart_line(1_000 + i, k)).collect();
            let rev = s.revision;
            let who = s.songs.entries.iter().find(|e| e.id == id).map_or(r.host, |e| e.by);
            let _ = s.apply(Actor::Member(who), Some(rev), &Command::AddSong { song: Box::new(song) }, Millis(1));
        }
        let changing = t.elapsed().as_secs_f64() / f64::from(rounds * 2) * 1e6;
        let mut copy = r.s.clone();
        let t = Instant::now();
        for _ in 0..n {
            copy = std::hint::black_box(r.s.clone());
            std::sync::Arc::make_mut(&mut copy.songs);
        }
        let deep = t.elapsed().as_secs_f64() / f64::from(n) * 1e6;
        drop(copy);
        println!(
            "{what}: list {list} B ({} B an entry), state {state} B (without the list {bare} B); \
             a command leaving the list alone {other:.1} µs, one changing it {changing:.1} µs; \
             a deep copy of state and list {deep:.1} µs",
            list / 100
        );
    }
}
