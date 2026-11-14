//! The prototype handoff's acceptance walkthroughs that the room model can
//! express (`docs/plans/multiplayer-prototype-handoff.md`), run through the
//! reducer. Every accepted revision's events are also applied to a mirror,
//! which must stay equal to the authority's state.

use ddi_room::*;

struct Room {
    s: RoomState,
    mirror: RoomState,
    now: u64,
    host: ParticipantId,
}

fn song(n: u32) -> ContentId {
    ContentId {
        song: SongKey::new(format!("song-{n}")),
        chart: ChartKey::new(format!("digest-{n}")),
    }
}

fn survived(groups: &[&[ParticipantId]]) -> Vec<Vec<Finisher>> {
    groups
        .iter()
        .map(|g| {
            g.iter()
                .map(|&participant| Finisher {
                    participant,
                    finish: Finish::Survived,
                })
                .collect()
        })
        .collect()
}

impl Room {
    fn new(rules: MatchRules, source: ContentSource) -> Self {
        let s = RoomState::new(
            RoomId::new("room"),
            Identity::new("twitch:host"),
            "Host".into(),
            RoomPolicy::default(),
            rules,
            source,
        );
        Self {
            mirror: s.clone(),
            s,
            now: 0,
            host: ParticipantId(1),
        }
    }

    fn hosted(stages: u16) -> Self {
        let mut rules = MatchRules::hosted();
        rules.stages = stages;
        Self::new(rules, ContentSource::LocalLibrary)
    }

    /// The cap as a value (73 by default since 2026-11-13): the policy's,
    /// and the seats of the next match's rules with it.
    fn cap(&mut self, cap: u16) {
        for s in [&mut self.s, &mut self.mirror] {
            s.policy.max_competitors = cap;
            s.next.rules.seats = s.next.rules.seats.min(cap);
        }
    }

    fn follow(&mut self, events: &[Event]) {
        if !events.is_empty() {
            self.mirror.commit(events);
        }
        assert_eq!(self.mirror, self.s, "the mirror follows the events");
    }

    /// `who` issues `cmd` against their current view.
    fn by(&mut self, who: ParticipantId, cmd: Command) -> Result<Vec<Event>, Rejection> {
        self.based(who, Some(self.s.revision), cmd)
    }

    fn based(
        &mut self,
        who: ParticipantId,
        base: Option<Revision>,
        cmd: Command,
    ) -> Result<Vec<Event>, Rejection> {
        let before = self.s.clone();
        let r = self
            .s
            .apply(Actor::Member(who), base, &cmd, Millis(self.now));
        match &r {
            Ok(events) => self.follow(&events.clone()),
            Err(_) => assert_eq!(before, self.s, "a refused command changes nothing"),
        }
        r
    }

    fn sys(&mut self, cmd: Command) -> Result<Vec<Event>, Rejection> {
        let r = self.s.apply(Actor::Authority, None, &cmd, Millis(self.now));
        if let Ok(events) = &r {
            self.follow(&events.clone());
        }
        r
    }

    fn join(&mut self, name: &str, intent: Intent) -> ParticipantId {
        self.join_as(name, intent, false).unwrap()
    }

    fn join_as(
        &mut self,
        name: &str,
        intent: Intent,
        admin: bool,
    ) -> Result<ParticipantId, Rejection> {
        let identity = Identity::new(format!("twitch:{name}"));
        self.sys(Command::Join {
            identity: identity.clone(),
            name: name.into(),
            admin,
            intent,
        })?;
        Ok(self.s.member_by_identity(&identity).unwrap().id)
    }

    fn wait(&mut self, ms: u64) -> Vec<Event> {
        self.now += ms;
        let events = self.s.advance(Millis(self.now));
        self.follow(&events);
        events
    }

    fn upcoming(&self) -> StageId {
        self.s.upcoming_stage().expect("a stage gathers readiness")
    }

    fn rev(&self, stage: StageId) -> u32 {
        self.s.stage(stage).unwrap().readiness_rev
    }

    fn choose(&mut self, stage: StageId, n: u32) {
        self.by(
            self.host,
            Command::ChooseContent {
                stage,
                content: Some(song(n)),
            },
        )
        .unwrap();
    }

    fn load(&mut self, who: ParticipantId, loading: Loading) {
        let stage = self.upcoming();
        self.by(
            who,
            Command::ReportLoading {
                stage,
                readiness_rev: self.rev(stage),
                loading,
            },
        )
        .unwrap();
    }

    fn set_ready(&mut self, who: ParticipantId, ready: bool) -> Result<Vec<Event>, Rejection> {
        let stage = self.upcoming();
        self.by(
            who,
            Command::SetReady {
                stage,
                readiness_rev: self.rev(stage),
                ready,
            },
        )
    }

    fn ready(&mut self, who: ParticipantId) {
        self.load(who, Loading::Loaded);
        self.set_ready(who, true).unwrap();
    }

    fn start_match(&mut self) {
        self.by(self.host, Command::StartMatch).unwrap();
    }

    fn start_stage(&mut self) -> Result<Vec<Event>, Rejection> {
        let stage = self.upcoming();
        self.by(
            self.host,
            Command::StartStage {
                stage,
                readiness_rev: self.rev(stage),
            },
        )
    }

    /// Count down into the song.
    fn play(&mut self) -> StageId {
        let Phase::Countdown { stage, .. } = self.s.phase else {
            panic!("not counting down: {:?}", self.s.phase)
        };
        self.wait(self.s.policy.readiness.countdown_ms);
        assert_eq!(self.s.phase, Phase::Stage { stage });
        stage
    }

    fn end(&mut self, stage: StageId, groups: &[&[ParticipantId]]) {
        self.sys(Command::EndSong {
            stage,
            ranking: Some(survived(groups)),
        })
        .unwrap();
    }

    fn total(&self, who: ParticipantId) -> Points {
        self.s
            .standings()
            .unwrap()
            .into_iter()
            .find(|s| s.participant == who)
            .unwrap()
            .total
    }

    fn part(&self, who: ParticipantId) -> Participation {
        self.s.participation(who).unwrap()
    }
}

#[test]
fn a_new_arrival_during_a_match_waits_for_the_next_match() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let first = r.upcoming();
    r.choose(first, 1);
    r.start_match();
    let bob = r.join("bob", Intent::Play);
    assert_eq!(r.part(bob), Participation::WaitingNextMatch);
    assert_eq!(r.set_ready(bob, true), Err(Rejection::NotInMatch));
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let stage = r.play();
    // Never admitted into a running song.
    assert_eq!(r.part(bob), Participation::WaitingNextMatch);
    assert_eq!(
        r.sys(Command::EndSong {
            stage,
            ranking: Some(survived(&[&[host], &[ana], &[bob]])),
        }),
        Err(Rejection::InvalidRanking(RankingProblem::NotInLineup(bob)))
    );
    r.end(stage, &[&[host], &[ana]]);
    assert_eq!(r.s.phase, Phase::MatchResults);
    assert!(
        r.s.standings()
            .unwrap()
            .iter()
            .all(|s| s.participant != bob)
    );
    r.by(host, Command::OpenNextMatch).unwrap();
    let next = r.upcoming();
    assert_eq!(r.part(bob), Participation::NotReady { stage: next });
    r.choose(next, 2);
    r.start_match();
    assert!(r.s.current.as_ref().unwrap().roster.contains_key(&bob));
}

#[test]
fn a_missed_stage_keeps_earlier_points_and_a_later_stage_can_be_readied() {
    let mut r = Room::hosted(3);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let cara = r.join("cara", Intent::Play);
    r.start_match();
    let s1 = r.upcoming();
    r.choose(s1, 1);
    for p in [host, ana, cara] {
        r.ready(p);
    }
    r.start_stage().unwrap();
    let s1 = r.play();
    r.end(s1, &[&[cara], &[host], &[ana]]);
    let cara_after_one = r.total(cara);
    assert_eq!(cara_after_one, Points::whole(12)); // three: 12, 5, 1
    // Intermission: cara reads her results and does not ready. That blocks
    // nothing: the host's Start opens the window and the deadline commits.
    let s2 = r.upcoming();
    r.choose(s2, 2);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let Phase::Preparing {
        window: Some(window),
        ..
    } = r.s.phase
    else {
        panic!("a window opens")
    };
    // Readying does not shorten the window.
    r.wait(window.deadline.0 - r.now - 1);
    assert!(matches!(r.s.phase, Phase::Preparing { .. }));
    r.wait(1);
    let s2 = r.play();
    assert_eq!(
        r.part(cara),
        Participation::SittingOut {
            stage: s2,
            reason: SitOutReason::NotReady
        }
    );
    r.end(s2, &[&[ana], &[host]]);
    assert_eq!(r.total(cara), cara_after_one, "earlier points stay");
    let standing =
        r.s.standings()
            .unwrap()
            .into_iter()
            .find(|s| s.participant == cara)
            .unwrap();
    assert_eq!(
        standing.stages[1],
        StageStanding::SatOut(SitOutReason::NotReady)
    );
    // And she readies for stage 3 with her own record.
    let s3 = r.upcoming();
    r.choose(s3, 3);
    for p in [host, ana, cara] {
        r.ready(p);
    }
    r.start_stage().unwrap();
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    assert_eq!(r.part(cara), Participation::Committed { stage: s3 });
}

#[test]
fn no_ready_humans_cannot_start_and_a_deadline_without_them_cancels() {
    let mut rules = MatchRules::hosted();
    rules.cpu.mode = CpuMode::Fixed { count: 2 };
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    // CPUs alone are no reason to start: at least one ready person.
    assert_eq!(
        r.start_stage(),
        Err(Rejection::NotEnoughReady {
            ready_humans: 0,
            competitors: 2
        })
    );
    r.ready(host);
    r.start_stage().unwrap();
    assert_eq!(r.part(ana), Participation::NotReady { stage: s });
    // The only ready person withdraws during the window: at the deadline
    // nothing starts, and the room says why.
    r.set_ready(host, false).unwrap();
    let events = r.wait(r.s.policy.readiness.window_ms);
    assert!(events.contains(&Event::StartCanceled {
        stage: s,
        reason: CancelReason::BelowMinimum
    }));
    assert_eq!(
        r.s.phase,
        Phase::Preparing {
            stage: s,
            window: None
        }
    );
}

#[test]
fn one_forgotten_ready_sits_out_while_the_rest_start() {
    let mut r = Room::hosted(2);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let dan = r.join("dan", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    r.ready(ana);
    r.load(dan, Loading::Loaded); // loaded, never pressed Ready
    let events = r.start_stage().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::PhaseChanged {
            phase: Phase::Preparing {
                window: Some(_),
                ..
            }
        }
    )));
    // A second Start cannot stack another window.
    assert_eq!(r.start_stage(), Err(Rejection::WindowOpen));
    let events = r.wait(r.s.policy.readiness.window_ms);
    let committed = events
        .iter()
        .find_map(|e| match e {
            Event::StartCommitted {
                lineup, sat_out, ..
            } => Some((lineup.clone(), sat_out.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(committed.0, vec![host, ana]);
    assert_eq!(committed.1, vec![(dan, SitOutReason::NotReady)]);
}

#[test]
fn a_required_loading_failure_sits_out_and_unavailable_content_blocks_the_start() {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = Room::new(rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let eve = r.join("eve", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    // Shared content is pending until the authority confirms it.
    assert_eq!(r.start_stage(), Err(Rejection::ContentPending));
    r.sys(Command::SetContentStatus {
        stage: s,
        status: ContentStatus::Unavailable {
            code: "upload_failed".into(),
        },
        share: None,
    })
    .unwrap();
    assert_eq!(r.start_stage(), Err(Rejection::ContentUnavailable));
    r.sys(Command::SetContentStatus {
        stage: s,
        status: ContentStatus::Available,
        share: None,
    })
    .unwrap();
    r.ready(host);
    r.ready(ana);
    // Eve readies while loading (intent), then her load fails.
    r.set_ready(eve, true).unwrap();
    assert_eq!(r.part(eve), Participation::ReadyLoading { stage: s });
    r.load(
        eve,
        Loading::Failed {
            code: "decode".into(),
        },
    );
    assert_eq!(r.part(eve), Participation::LoadFailed { stage: s });
    r.start_stage().unwrap();
    // The content becomes unusable during the window: canceled, distinctly.
    let events = r
        .sys(Command::SetContentStatus {
            stage: s,
            status: ContentStatus::Unavailable {
                code: "share_expired".into(),
            },
            share: None,
        })
        .unwrap();
    assert!(events.contains(&Event::StartCanceled {
        stage: s,
        reason: CancelReason::ContentUnavailable
    }));
    r.sys(Command::SetContentStatus {
        stage: s,
        status: ContentStatus::Available,
        share: None,
    })
    .unwrap();
    r.start_stage().unwrap();
    let events = r.wait(r.s.policy.readiness.window_ms);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::StartCommitted { sat_out, .. } if sat_out == &vec![(eve, SitOutReason::LoadFailed)]
    )));
}

#[test]
fn a_canceled_start_returns_to_preparation_keeping_readiness() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    // The countdown is frozen: no readiness changes, no song change.
    assert_eq!(
        r.by(
            ana,
            Command::SetReady {
                stage: s,
                readiness_rev: r.rev(s),
                ready: false
            }
        ),
        Err(Rejection::StartCommitted)
    );
    assert_eq!(
        r.by(
            host,
            Command::ChooseContent {
                stage: s,
                content: Some(song(2))
            }
        ),
        Err(Rejection::StartCommitted)
    );
    let events = r.by(host, Command::CancelStart { stage: s }).unwrap();
    assert_eq!(
        events[0],
        Event::StartCanceled {
            stage: s,
            reason: CancelReason::ByOperator
        }
    );
    assert_eq!(
        r.s.phase,
        Phase::Preparing {
            stage: s,
            window: None
        }
    );
    assert_eq!(r.part(ana), Participation::ReadyLoaded { stage: s });
    assert!(r.s.stage(s).unwrap().play.is_none());
    assert_eq!(
        r.by(host, Command::CancelStart { stage: s }),
        Err(Rejection::NothingToCancel)
    );
}

#[test]
fn replacing_the_song_during_preparation_cancels_the_window_and_clears_readiness() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let dan = r.join("dan", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    r.ready(ana);
    let old_rev = r.rev(s);
    r.start_stage().unwrap();
    let events = r
        .by(
            host,
            Command::ChooseContent {
                stage: s,
                content: Some(song(2)),
            },
        )
        .unwrap();
    assert!(events.contains(&Event::StartCanceled {
        stage: s,
        reason: CancelReason::ContentReplaced
    }));
    assert_eq!(
        r.s.phase,
        Phase::Preparing {
            stage: s,
            window: None
        }
    );
    assert_eq!(r.rev(s), old_rev + 1);
    for p in [host, ana, dan] {
        assert_eq!(r.part(p), Participation::NotReady { stage: s });
    }
    // A Ready sent for the old song is refused, never applied to the new.
    assert_eq!(
        r.by(
            ana,
            Command::SetReady {
                stage: s,
                readiness_rev: old_rev,
                ready: true
            }
        ),
        Err(Rejection::StaleReadiness {
            current: old_rev + 1
        })
    );
    // Nothing passes the deadline the canceled window had.
    assert!(r.wait(r.s.policy.readiness.window_ms).is_empty());
}

#[test]
fn access_changes_keep_present_members_and_readiness() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(ana);
    let members = r.s.members.clone();
    r.by(
        host,
        Command::SetAccess {
            admission: Admission::Closed,
            rotate_invite: true,
        },
    )
    .unwrap();
    assert_eq!(r.s.members, members);
    assert_eq!(r.part(ana), Participation::ReadyLoaded { stage: s });
    assert_eq!(r.s.access.invite_generation, 2);
    assert_eq!(
        r.join_as("zed", Intent::Play, false),
        Err(Rejection::AdmissionClosed)
    );
    // A member who leaves and returns is restored, closed or not.
    r.by(ana, Command::Leave).unwrap();
    assert_eq!(r.join("ana", Intent::Play), ana);
    // Administrators may enter a closed room to help.
    assert!(r.join_as("admin", Intent::Watch, true).is_ok());
}

#[test]
fn current_rules_cannot_change_mid_match_and_saved_drafts_wait_for_the_boundary() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let s = r.upcoming();
    r.choose(s, 1);
    r.start_match();
    let locked = r.s.current.as_ref().unwrap().rules.clone();
    r.ready(host);
    r.ready(ana);
    let mut draft = MatchRules::hosted();
    draft.stages = 3;
    draft.seats = 4;
    r.by(
        host,
        Command::SaveNextRules {
            rules: draft.clone(),
        },
    )
    .unwrap();
    assert_eq!(r.s.current.as_ref().unwrap().rules, locked);
    assert_eq!(r.s.next.rules, draft);
    // The running stage's readiness is untouched by a next-match draft.
    assert_eq!(r.part(ana), Participation::ReadyLoaded { stage: s });
    assert_eq!(r.s.playlist.len(), 3);
    r.start_stage().unwrap();
    let s = r.play();
    r.end(s, &[&[host], &[ana]]);
    r.by(host, Command::OpenNextMatch).unwrap();
    assert_eq!(r.s.next.rules, draft, "the saved draft survives the match");
    assert_eq!(r.s.next.review, None);
    assert_eq!(r.s.history.len(), 1);
    assert_eq!(r.s.history[0].played.rules, locked);
    r.start_match();
    assert_eq!(r.s.current.as_ref().unwrap().rules, draft);
    assert_eq!(r.s.current.as_ref().unwrap().stages.len(), 3);
}

#[test]
fn a_stage_count_change_keeps_the_first_playlist_slots() {
    let mut r = Room::hosted(4);
    let host = r.host;
    let ids: Vec<StageId> = r.s.playlist.iter().map(|s| s.id).collect();
    r.choose(ids[0], 1);
    r.choose(ids[1], 2);
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    r.by(
        host,
        Command::SaveNextRules {
            rules: rules.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        r.s.playlist.iter().map(|s| s.id).collect::<Vec<_>>(),
        ids[..2]
    );
    assert_eq!(r.s.playlist[1].content.as_ref().unwrap().id, song(2));
    rules.stages = 3;
    r.by(host, Command::SaveNextRules { rules }).unwrap();
    assert_eq!(r.s.playlist.len(), 3);
    assert!(r.s.playlist[2].content.is_none());
    assert!(
        !ids.contains(&r.s.playlist[2].id),
        "a fresh id, never reused"
    );
}

#[test]
fn newly_reserved_seats_put_a_saved_draft_under_review_at_the_boundary() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    r.start_match();
    let mut draft = MatchRules::hosted();
    draft.stages = 1;
    draft.seats = 3;
    r.by(host, Command::SaveNextRules { rules: draft }).unwrap();
    // Two more arrive during the match and reserve seats for the next.
    let bob = r.join("bob", Intent::Play);
    let _cy = r.join("cy", Intent::Play);
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let s = r.play();
    r.end(s, &[&[ana], &[host]]);
    // The live roster was never changed by the reservations.
    assert_eq!(r.s.current.as_ref().unwrap().roster.len(), 2);
    let events = r.by(host, Command::OpenNextMatch).unwrap();
    let problem = RulesProblem::HumansExceedSeats {
        humans: 4,
        seats: 3,
    };
    assert!(events.contains(&Event::NextRulesReviewed {
        review: Some(problem)
    }));
    assert_eq!(r.s.next.review, Some(problem));
    assert_eq!(
        r.by(host, Command::StartMatch),
        Err(Rejection::RulesNeedReview(problem))
    );
    // One leaves: still under review until an operator saves rules.
    r.by(bob, Command::Leave).unwrap();
    let mut fixed = r.s.next.rules.clone();
    fixed.seats = 4;
    r.by(host, Command::SaveNextRules { rules: fixed }).unwrap();
    assert_eq!(r.s.next.review, None);
    r.start_match();
}

#[test]
fn an_edit_start_race_is_resolved_by_revision() {
    // A co-host saves while the host starts from the same view: whichever
    // lands second is stale.
    let mut r = Room::hosted(1);
    let host = r.host;
    let co = r.join("co", Intent::Watch);
    r.by(
        host,
        Command::SetCoHost {
            participant: co,
            on: true,
        },
    )
    .unwrap();
    let view = r.s.revision;
    let mut edit = MatchRules::hosted();
    edit.stages = 2;
    r.based(
        co,
        Some(view),
        Command::SaveNextRules {
            rules: edit.clone(),
        },
    )
    .unwrap();
    let saved_at = r.s.next.saved_at;
    assert_eq!(
        r.based(host, Some(view), Command::StartMatch),
        Err(Rejection::Stale {
            base: view,
            changed_at: saved_at
        })
    );

    // The other order: the start lands first; the edit made before it is
    // stale and never reaches the locked match.
    let mut r = Room::hosted(1);
    let host = r.host;
    let co = r.join("co", Intent::Watch);
    r.by(
        host,
        Command::SetCoHost {
            participant: co,
            on: true,
        },
    )
    .unwrap();
    let view = r.s.revision;
    r.based(host, Some(view), Command::StartMatch).unwrap();
    let locked = r.s.current.as_ref().unwrap().rules.clone();
    assert!(matches!(
        r.based(
            co,
            Some(view),
            Command::SaveNextRules {
                rules: edit.clone()
            }
        ),
        Err(Rejection::Stale { .. })
    ));
    // Resubmitted against the new view, it is the next match's draft.
    r.by(
        co,
        Command::SaveNextRules {
            rules: edit.clone(),
        },
    )
    .unwrap();
    assert_eq!(r.s.current.as_ref().unwrap().rules, locked);
    assert_eq!(r.s.next.rules, edit);
    // Edits without a base are refused, never applied blind.
    assert_eq!(
        r.based(co, None, Command::SaveNextRules { rules: edit }),
        Err(Rejection::MissingBase)
    );
}

#[test]
fn host_cohost_admin_observer_and_member_permissions_stay_distinct() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let co = r.join("co", Intent::Play);
    let admin = r.join_as("admin", Intent::Watch, true).unwrap();
    let mem = r.join("mem", Intent::Play);
    r.by(
        host,
        Command::SetCoHost {
            participant: co,
            on: true,
        },
    )
    .unwrap();
    let s = r.upcoming();
    let choose = |n| Command::ChooseContent {
        stage: s,
        content: Some(song(n)),
    };
    assert_eq!(
        r.by(mem, choose(1)),
        Err(Rejection::Unauthorized {
            capability: Capability::ChooseContent
        })
    );
    // One picker at a time, the owner at first: another operator is told
    // who has the pick, takes it, and chooses; a member cannot take it.
    assert_eq!(
        r.by(co, choose(2)),
        Err(Rejection::NotPicker { picker: Some(host) })
    );
    assert_eq!(
        r.by(mem, Command::TakePick),
        Err(Rejection::Unauthorized {
            capability: Capability::TakePick
        })
    );
    r.by(co, Command::TakePick).unwrap();
    r.by(co, choose(2)).unwrap();
    assert_eq!(
        r.by(admin, choose(3)),
        Err(Rejection::NotPicker { picker: Some(co) })
    );
    r.by(admin, Command::TakePick).unwrap();
    r.by(admin, choose(3)).unwrap();
    let access = Command::SetAccess {
        admission: Admission::Closed,
        rotate_invite: false,
    };
    for who in [co, admin, mem] {
        assert_eq!(
            r.by(who, access.clone()),
            Err(Rejection::Unauthorized {
                capability: Capability::ChangeAccess
            })
        );
    }
    assert_eq!(
        r.by(
            admin,
            Command::SetCoHost {
                participant: mem,
                on: true
            }
        ),
        Err(Rejection::Unauthorized {
            capability: Capability::ManageCoHosts
        })
    );
    assert_eq!(
        r.by(
            host,
            Command::SetCoHost {
                participant: host,
                on: false
            }
        ),
        Err(Rejection::OwnerRole)
    );
    // Watching operators operate without a seat or readiness.
    assert_eq!(r.part(admin), Participation::Watching);
    assert_eq!(r.set_ready(admin, true), Err(Rejection::NotPlaying));
    // No client may report the authority's facts.
    assert_eq!(
        r.by(
            host,
            Command::Join {
                identity: Identity::new("twitch:fake"),
                name: "Fake".into(),
                admin: true,
                intent: Intent::Play
            }
        ),
        Err(Rejection::AuthorityOnly)
    );
    r.start_match();
    assert!(!r.s.current.as_ref().unwrap().roster.contains_key(&admin));
    for p in [host, co, mem] {
        r.ready(p);
    }
    r.by(
        co,
        Command::StartStage {
            stage: s,
            readiness_rev: r.rev(s),
        },
    )
    .unwrap();
    let s = r.play();
    assert_eq!(
        r.by(
            host,
            Command::ConfirmStage {
                stage: s,
                ranking: survived(&[&[host], &[co], &[mem]])
            }
        ),
        Err(Rejection::AuthorityOnly)
    );
    assert_eq!(
        r.by(mem, Command::StopMatch),
        Err(Rejection::Unauthorized {
            capability: Capability::StopMatch
        })
    );
    // An unknown or departed client is refused.
    assert_eq!(
        r.by(ParticipantId(999), Command::Leave),
        Err(Rejection::NotAMember)
    );
}

#[test]
fn reconnecting_restores_the_same_row_and_points_without_a_respawn() {
    let mut rules = MatchRules::hosted();
    rules.stages = 3;
    rules.cpu.mode = CpuMode::Fixed { count: 1 };
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    r.start_match();
    let cpus = r.s.members.values().filter(|m| m.is_cpu()).count();
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let s1 = r.play();
    let cpu =
        *r.s.members
            .values()
            .find(|m| m.is_cpu())
            .map(|m| &m.id)
            .unwrap();
    r.end(s1, &[&[ana], &[cpu], &[host]]);
    let ana_points = r.total(ana);
    assert_eq!(ana_points, Points::whole(12));

    // Stage 2: Ana's connection drops mid-song. No forfeit is inferred, no
    // CPU takes her seat, and she comes back to the same row.
    let s = r.upcoming();
    r.choose(s, 2);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let s2 = r.play();
    r.sys(Command::SetConnection {
        participant: ana,
        connection: Connection::Reconnecting,
    })
    .unwrap();
    assert_eq!(r.part(ana), Participation::Competing { stage: s2 });
    r.sys(Command::SetConnection {
        participant: ana,
        connection: Connection::Disconnected,
    })
    .unwrap();
    assert_eq!(r.join("ana", Intent::Play), ana);
    assert_eq!(r.s.members[&ana].connection, Connection::Connected);
    assert_eq!(r.s.members.values().filter(|m| m.is_cpu()).count(), cpus);
    assert_eq!(r.s.current.as_ref().unwrap().roster.len(), 3);
    let play = r.s.stage(s2).unwrap().play.clone().unwrap();
    assert!(play.lineup.contains(&ana) && play.result.is_none());
    assert_eq!(r.total(ana), ana_points);
    r.sys(Command::EndSong {
        stage: s2,
        ranking: Some(vec![
            vec![Finisher {
                participant: host,
                finish: Finish::Survived,
            }],
            vec![Finisher {
                participant: cpu,
                finish: Finish::Survived,
            }],
            vec![Finisher {
                participant: ana,
                finish: Finish::Forfeit,
            }],
        ]),
    })
    .unwrap();
    // A confirmed forfeit adds zero; earlier points stay.
    assert_eq!(r.total(ana), ana_points);

    // In the intermission: readied, disconnected, back. Ready starts off.
    let s3 = r.upcoming();
    r.choose(s3, 3);
    r.ready(ana);
    r.sys(Command::SetConnection {
        participant: ana,
        connection: Connection::Disconnected,
    })
    .unwrap();
    assert_eq!(r.join("ana", Intent::Play), ana);
    assert_eq!(r.part(ana), Participation::NotReady { stage: s3 });
}

#[test]
fn a_committed_starter_leaving_cancels_the_countdown() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let bo = r.join("bo", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    for p in [host, ana, bo] {
        r.ready(p);
    }
    r.start_stage().unwrap();
    let events = r.by(bo, Command::Leave).unwrap();
    assert!(events.contains(&Event::StartCanceled {
        stage: s,
        reason: CancelReason::RosterChanged
    }));
    assert_eq!(r.part(bo), Participation::Left);
    assert_eq!(r.s.stage(s).unwrap().prep(bo), Prep::default());
    // Ana's connection drops before the next start: canceled the same way,
    // and her Ready is gone when she is back.
    r.start_stage().unwrap();
    let events = r
        .sys(Command::SetConnection {
            participant: ana,
            connection: Connection::Disconnected,
        })
        .unwrap();
    assert_eq!(
        events[0],
        Event::StartCanceled {
            stage: s,
            reason: CancelReason::RosterChanged
        }
    );
    assert_eq!(r.join("ana", Intent::Play), ana);
    assert_eq!(r.part(ana), Participation::NotReady { stage: s });
    r.ready(ana);
    // The rest start; the leaver sits out with zero, results kept.
    r.start_stage().unwrap();
    let s = r.play();
    assert_eq!(
        r.s.stage(s).unwrap().play.as_ref().unwrap().sat_out[&bo],
        SitOutReason::Left
    );
}

#[test]
fn the_winner_finishes_the_song_without_more_points() {
    let mut r = Room::hosted(2);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let s = r.play();
    r.sys(Command::ConfirmStage {
        stage: s,
        ranking: vec![
            vec![Finisher {
                participant: ana,
                finish: Finish::Survived,
            }],
            vec![Finisher {
                participant: host,
                finish: Finish::Eliminated,
            }],
        ],
    })
    .unwrap();
    assert_eq!(r.s.phase, Phase::SongCompletion { stage: s });
    assert_eq!(r.part(ana), Participation::WinnerFinishing { stage: s });
    assert_eq!(
        r.part(host),
        Participation::Placed {
            stage: s,
            place: 2,
            finish: Finish::Eliminated
        }
    );
    let won = r.total(ana);
    // The rest of the song cannot add or change points.
    assert_eq!(
        r.sys(Command::ConfirmStage {
            stage: s,
            ranking: survived(&[&[ana], &[host]])
        }),
        Err(Rejection::ResultAlreadyConfirmed)
    );
    assert_eq!(
        r.sys(Command::EndSong {
            stage: s,
            ranking: Some(survived(&[&[host], &[ana]]))
        }),
        Err(Rejection::ResultAlreadyConfirmed)
    );
    // Nobody advances past the song before its natural end.
    assert_eq!(
        r.by(host, Command::PrepareNextStage),
        Err(Rejection::Unauthorized {
            capability: Capability::Advance
        })
    );
    r.sys(Command::EndSong {
        stage: s,
        ranking: None,
    })
    .unwrap();
    assert_eq!(r.total(ana), won);
    assert!(matches!(r.s.phase, Phase::Intermission { .. }));
}

#[test]
fn an_interrupted_stage_awards_nothing_and_earlier_stages_stay_as_partial_results() {
    let mut r = Room::hosted(3);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let s1 = r.play();
    r.end(s1, &[&[host], &[ana]]);
    let s = r.upcoming();
    r.choose(s, 2);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let s2 = r.play();
    r.sys(Command::Interrupt {
        reason: Interruption::AuthorityFailed,
    })
    .unwrap();
    assert_eq!(
        r.s.phase,
        Phase::Interrupted {
            reason: Interruption::AuthorityFailed
        }
    );
    assert!(r.s.stage(s2).unwrap().play.as_ref().unwrap().interrupted);
    // A late outcome for the lost stage is refused.
    assert!(r.end_err(s2, &[&[ana], &[host]]));
    let standings = r.s.standings().unwrap();
    let ana_row = standings.iter().find(|s| s.participant == ana).unwrap();
    assert_eq!(ana_row.stages[1], StageStanding::Unconfirmed);
    assert_eq!(ana_row.stages[2], StageStanding::NotPlayed);
    assert_eq!(ana_row.total, Points::whole(1));
    assert!(r.s.match_winners().is_empty());
    r.by(host, Command::OpenNextMatch).unwrap();
    let record = r.s.history.last().unwrap();
    assert_eq!(
        record.ending,
        MatchEnding::Interrupted {
            reason: Interruption::AuthorityFailed
        }
    );
    assert!(record.winners().is_empty(), "no overall winner");
    assert_eq!(record.played.standings()[0].total, Points::whole(12));
    assert_eq!(r.s.phase, Phase::Lobby);
}

impl Room {
    fn end_err(&mut self, stage: StageId, groups: &[&[ParticipantId]]) -> bool {
        self.sys(Command::EndSong {
            stage,
            ranking: Some(survived(groups)),
        })
        .is_err()
    }
}

#[test]
fn a_join_to_a_full_room_watches_and_plays_once_a_place_frees_up() {
    let mut r = Room::hosted(1);
    // A cap of nine (a value: 73 by default since 2026-11-13).
    r.cap(9);
    r.s.policy.max_members = 11;
    r.mirror.policy.max_members = 11;
    let players: Vec<ParticipantId> = (0..8)
        .map(|i| r.join(&format!("p{i}"), Intent::Play))
        .collect();
    // Full of players: in to watch, the event saying so to every mirror.
    let identity = Identity::new("twitch:late");
    let events = r
        .sys(Command::Join {
            identity: identity.clone(),
            name: "late".into(),
            admin: false,
            intent: Intent::Play,
        })
        .unwrap();
    let late = r.s.member_by_identity(&identity).unwrap().id;
    assert!(events.iter().any(|e| matches!(
        e,
        Event::MemberJoined { member } if member.id == late && member.intent == Intent::Watch
    )));
    // Asking to watch is unaffected, and so is a member's coming back.
    let fan = r.join("fan", Intent::Watch);
    assert_eq!(r.s.members[&fan].intent, Intent::Watch);
    let refused = Err(Rejection::Cap(CapExceeded {
        requested: 10,
        cap: 9,
    }));
    let play = Command::SetIntent {
        intent: Intent::Play,
    };
    assert_eq!(r.by(late, play.clone()), refused);
    // The member limit (11 here; 384 by default) still refuses whole.
    assert_eq!(
        r.join_as("eleventh", Intent::Watch, false),
        Err(Rejection::RoomFull)
    );
    assert_eq!(
        r.join_as("twelfth", Intent::Play, false),
        Err(Rejection::RoomFull)
    );
    // A place frees up: nobody is moved to Play by it…
    r.by(players[3], Command::Leave).unwrap();
    assert!(!r.s.players_full());
    assert_eq!(r.s.members[&late].intent, Intent::Watch);
    assert_eq!(r.s.members[&fan].intent, Intent::Watch);
    // …the watcher chooses it, and the room is full of players again.
    r.by(late, play.clone()).unwrap();
    assert_eq!(r.s.members[&late].intent, Intent::Play);
    assert!(r.s.players_full());
    assert_eq!(r.by(fan, play), refused);
    // A newcomer asking to play now joins to watch again.
    r.s.policy.max_members = 12;
    r.mirror.policy.max_members = 12;
    let next = r.join_as("next", Intent::Play, false).unwrap();
    assert_eq!(r.s.members[&next].intent, Intent::Watch);
}

#[test]
fn the_competitor_cap_is_enforced() {
    let mut r = Room::hosted(1);
    // A cap of nine (a value: 73 by default since 2026-11-13).
    r.cap(9);
    for i in 0..8 {
        r.join(&format!("p{i}"), Intent::Play);
    }
    // Every player's place taken: one who asks to play joins to watch
    // (`join-watch-v0`), never refused for it.
    assert!(r.s.players_full());
    assert_eq!(r.s.arrival_intent(Intent::Play), Intent::Watch);
    assert_eq!(r.s.arrival_intent(Intent::Watch), Intent::Watch);
    let watcher = r.join_as("tenth", Intent::Play, false).unwrap();
    assert_eq!(r.s.members[&watcher].intent, Intent::Watch);
    assert_eq!(r.part(watcher), Participation::Watching);
    assert_eq!(r.s.reserved_humans(), 9);
    assert_eq!(
        r.by(
            watcher,
            Command::SetIntent {
                intent: Intent::Play
            }
        ),
        Err(Rejection::Cap(CapExceeded {
            requested: 10,
            cap: 9
        }))
    );
    let mut rules = MatchRules::hosted();
    rules.seats = 10;
    assert_eq!(
        r.by(r.host, Command::SaveNextRules { rules }),
        Err(Rejection::InvalidRules(RulesProblem::OverCap(
            CapExceeded {
                requested: 10,
                cap: 9
            }
        )))
    );
    let mut rules = MatchRules::hosted();
    rules.seats = 9;
    rules.cpu.mode = CpuMode::Fixed { count: 1 };
    assert_eq!(
        r.by(r.host, Command::SaveNextRules { rules }),
        Err(Rejection::InvalidRules(RulesProblem::CpusExceedSeats {
            humans: 9,
            cpus: 1,
            seats: 9
        }))
    );
}

#[test]
fn practice_plays_a_match_with_cpus_and_starts_another_with_fresh_readiness() {
    let opponent = OpponentSpec {
        level: 0,
        seed: 42,
        personality: None,
        name: None,
        targeting: None,
    };
    let mut rules = MatchRules::practice(opponent);
    rules.stages = 2;
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let me = r.host;
    for s in r.s.playlist.iter().map(|s| s.id).collect::<Vec<_>>() {
        r.choose(s, s.0 as u32);
    }
    // Readiness is never automatic, not even alone with CPUs.
    assert_eq!(
        r.part(me),
        Participation::NotReady {
            stage: r.upcoming()
        }
    );
    r.ready(me);
    r.start_match();
    let cpus: Vec<ParticipantId> =
        r.s.members
            .values()
            .filter(|m| m.is_cpu())
            .map(|m| m.id)
            .collect();
    assert_eq!(cpus.len(), 3);
    assert_eq!(r.s.opponent(cpus[0]).unwrap().level, 0);
    // Ready from the lobby carries into stage 1: everyone eligible at once.
    r.start_stage().unwrap();
    let s1 = r.play();
    r.end(s1, &[&[me], &[cpus[0]], &[cpus[1], cpus[2]]]);
    let result = r.s.stage(s1).unwrap().play.clone().unwrap().result.unwrap();
    assert_eq!(result.rule, AwardRule::EightPlayerScaledV1);
    // Four: 12, 6, 3, 1; the tied two share 3 and 1.
    assert_eq!(
        result.placement(cpus[1]).unwrap().points,
        Points::whole(2)
    );
    // Intermission: nothing carries over; the player readies again.
    assert_eq!(
        r.part(me),
        Participation::NotReady {
            stage: r.upcoming()
        }
    );
    r.ready(me);
    r.start_stage().unwrap();
    let s2 = r.play();
    r.end(s2, &[&[cpus[0]], &[me], &[cpus[1]], &[cpus[2]]]);
    assert_eq!(r.s.phase, Phase::MatchResults);
    // 12 + 6 each.
    assert_eq!(r.total(me), Points::whole(18));
    assert_eq!(r.total(cpus[0]), Points::whole(18));
    let mut winners = r.s.match_winners();
    winners.sort();
    assert_eq!(winners, vec![me, cpus[0]], "equal totals share the rank");
    r.by(me, Command::OpenNextMatch).unwrap();
    assert_eq!(r.s.phase, Phase::Lobby);
    assert!(r.s.members.values().all(|m| !m.is_cpu()));
    assert_eq!(r.s.history[0].cpus.len(), 3);
    let next = r.upcoming();
    assert_eq!(r.part(me), Participation::NotReady { stage: next });
    r.choose(next, 9);
    r.ready(me);
    r.start_match();
    let new_cpus: Vec<ParticipantId> =
        r.s.members
            .values()
            .filter(|m| m.is_cpu())
            .map(|m| m.id)
            .collect();
    assert!(new_cpus.iter().all(|c| !cpus.contains(c)), "fresh ids");
    assert_ne!(r.s.current.as_ref().unwrap().id, r.s.history[0].played.id);
}

#[test]
fn persistent_elimination_ends_the_match_for_the_eliminated_and_the_absent() {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    rules.return_each_stage = false;
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let bo = r.join("bo", Intent::Play);
    let cy = r.join("cy", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    for p in [host, ana, bo] {
        r.ready(p);
    }
    r.start_stage().unwrap();
    r.wait(r.s.policy.readiness.window_ms);
    let s1 = r.play();
    assert_eq!(r.part(cy), Participation::OutForMatch, "missed a stage");
    r.sys(Command::EndSong {
        stage: s1,
        ranking: Some(vec![
            vec![Finisher {
                participant: host,
                finish: Finish::Survived,
            }],
            vec![Finisher {
                participant: ana,
                finish: Finish::Survived,
            }],
            vec![Finisher {
                participant: bo,
                finish: Finish::Eliminated,
            }],
        ]),
    })
    .unwrap();
    assert_eq!(r.part(bo), Participation::OutForMatch);
    assert_eq!(r.set_ready(bo, true), Err(Rejection::OutForMatch));
}

#[test]
fn a_late_entrant_joins_at_a_stage_boundary_with_zero_for_missed_stages() {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    rules.mid_match_entry = true;
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    r.start_match();
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let s1 = r.play();
    let late = r.join("late", Intent::Play);
    assert_eq!(r.part(late), Participation::WaitingNextMatch);
    r.end(s1, &[&[host], &[ana]]);
    // Still waiting through the intermission until the boundary.
    assert_eq!(r.part(late), Participation::WaitingNextMatch);
    r.by(host, Command::PrepareNextStage).unwrap();
    let s2 = r.upcoming();
    assert_eq!(r.part(late), Participation::NotReady { stage: s2 });
    let row =
        r.s.standings()
            .unwrap()
            .into_iter()
            .find(|s| s.participant == late)
            .unwrap();
    assert_eq!(row.total, Points::ZERO);
    assert_eq!(row.stages[0], StageStanding::NotPlayed);
}

/// M5c's open question, fixed at the model: Start in the lobby starts the
/// match and its first stage as one change. A start its first stage
/// refuses leaves the lobby as it was (no match locked, a newcomer still
/// joins to play); one it accepts locks the match and counts down.
#[test]
fn start_in_the_lobby_starts_the_match_with_its_first_stage_or_nothing() {
    let mut r = Room::hosted(2);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let s = r.upcoming();
    r.choose(s, 1);
    r.ready(host);
    // Too few ready: refused whole; still the lobby, nothing locked.
    let before = r.s.clone();
    assert!(matches!(
        r.start_stage(),
        Err(Rejection::NotEnoughReady { .. })
    ));
    assert_eq!(r.s, before);
    assert_eq!(r.s.phase, Phase::Lobby);
    assert!(r.s.current.is_none());
    let late = r.join("late", Intent::Play);
    assert_eq!(
        r.s.participation(late),
        Some(Participation::NotReady { stage: s })
    );
    // Without the view it was made against, refused as StartMatch is.
    assert_eq!(
        r.based(
            host,
            None,
            Command::StartStage {
                stage: s,
                readiness_rev: r.rev(s),
            }
        ),
        Err(Rejection::MissingBase)
    );
    // Only the playlist's first stage starts a match.
    let second = r.s.playlist[1].id;
    assert!(
        r.by(
            host,
            Command::StartStage {
                stage: second,
                readiness_rev: 0,
            }
        )
        .is_err()
    );
    r.ready(ana);
    // Late is unready: the match starts with a window on its first stage.
    let events = r.start_stage().unwrap();
    assert!(matches!(events[0], Event::MatchStarted { .. }));
    assert!(matches!(
        r.s.phase,
        Phase::Preparing {
            window: Some(_),
            ..
        }
    ));
    r.wait(r.s.policy.readiness.window_ms);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    // StartMatch alone (protocol 1's two-step start) still works.
    let mut old = Room::hosted(1);
    old.join("bo", Intent::Play);
    old.start_match();
    assert!(matches!(old.s.phase, Phase::Preparing { .. }));
}

fn report(
    r: &mut Room,
    who: ParticipantId,
    stage: StageId,
    run: Run,
) -> Result<Vec<Event>, Rejection> {
    r.by(
        who,
        Command::ReportOutcome {
            stage,
            run: ReportedRun::of(run),
            detail: None,
        },
    )
}

/// Feedback round B: a report's detail is additive to protocol 2. What a
/// peer that predates it sends still reads (no `detail`, no `details`),
/// what is sent without one is byte for byte what was sent before, and a
/// peer that predates it reads what carries one (serde ignores the field
/// it does not know; shown here by the old shapes reading the new text
/// with the field taken out).
#[test]
fn a_reports_detail_is_additive_on_the_wire() {
    use ddi_room::RunDetail;
    use serde_json::{Value, json};
    let stage = StageId(3);
    let run = ReportedRun::of(Run::Out { at: 12.5 });
    // Without a detail: the shape protocol 2 has had.
    let plain = Command::ReportOutcome {
        stage,
        run,
        detail: None,
    };
    let text = serde_json::to_value(&plain).unwrap();
    assert_eq!(
        text,
        json!({"report_outcome": {"stage": 3, "run": {"out": {"at_ms": 12500}}}})
    );
    assert_eq!(serde_json::from_value::<Command>(text).unwrap(), plain);
    let event = json!({"outcome_reported": {
        "stage": 3, "participant": 2, "run": "forfeit", "at": 5000
    }});
    let Event::OutcomeReported { detail, .. } = serde_json::from_value(event).unwrap() else {
        panic!("an outcome")
    };
    assert_eq!(detail, None);
    // With one: only what is said travels.
    let detail = RunDetail {
        percent: Some(9_950),
        grade: "S".into(),
        life_ppm: 640_000,
        max_combo: 283,
        tiers: vec![200, 80, 3, 0, 0, 0, 0],
        ..RunDetail::default()
    };
    let with = Command::ReportOutcome {
        stage,
        run,
        detail: Some(detail.clone()),
    };
    let text = serde_json::to_value(&with).unwrap();
    assert_eq!(
        text["report_outcome"]["detail"],
        json!({
            "percent": 9950, "grade": "S", "life_ppm": 640000,
            "max_combo": 283, "tiers": [200, 80, 3, 0, 0, 0, 0]
        })
    );
    assert_eq!(serde_json::from_value::<Command>(text).unwrap(), with);
    // A detail from a later build with a field this one does not know,
    // and missing the ones it does: read, the rest defaulted.
    let later: RunDetail = serde_json::from_value(json!({"grade": "A", "streak": 12})).unwrap();
    assert_eq!(
        later,
        RunDetail {
            grade: "A".into(),
            ..RunDetail::default()
        }
    );
    // A stage as an older authority stores and sends it: no `details`.
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = Room::new(rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let s = r.upcoming();
    r.choose(s, 1);
    r.sys(Command::SetContentStatus {
        stage: s,
        status: ContentStatus::Available,
        share: Some("abc12345".into()),
    })
    .unwrap();
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let stage = r.play();
    let play = |r: &Room| {
        r.s.current
            .as_ref()
            .unwrap()
            .stage(stage)
            .unwrap()
            .play
            .clone()
            .unwrap()
    };
    report(&mut r, ana, stage, Run::Out { at: 20.0 }).unwrap();
    let old: Value = serde_json::to_value(play(&r)).unwrap();
    assert!(old.get("details").is_none(), "{old}");
    assert_eq!(
        serde_json::from_value::<ddi_room::state::StagePlay>(old).unwrap(),
        play(&r)
    );
    // The host's report says more: kept beside the report, the result the
    // runs' alone.
    r.by(
        host,
        Command::ReportOutcome {
            stage,
            run: ReportedRun::of(Run::Live {
                life: 0.64,
                accuracy: 0.995,
                absolute: None,
                score: None,
                steps: 0,
            }),
            detail: Some(detail.clone()),
        },
    )
    .unwrap();
    let play = play(&r);
    assert_eq!(play.details.get(&host), Some(&detail));
    assert_eq!(play.details.get(&ana), None);
    assert_eq!(
        play.result.as_ref().unwrap().placement(host).unwrap().place,
        1
    );
    let new: Value = serde_json::to_value(&play).unwrap();
    assert_eq!(new["details"][0][0], json!(host));
    assert_eq!(
        serde_json::from_value::<ddi_room::state::StagePlay>(new).unwrap(),
        play
    );
}

/// M5d's start into a common stage: an online stage (no CPUs) settles on
/// its starters' own reports, ranked as practice ranks
/// (`gauge-survival-v0`), its points awarded; then the next stage and the
/// match's results, as practice goes.
#[test]
fn an_online_stage_settles_on_its_starters_reports() {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = Room::new(rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let s = r.upcoming();
    r.choose(s, 1);
    assert_eq!(
        r.s.stage(s).unwrap().content.as_ref().unwrap().status,
        ContentStatus::Pending
    );
    r.sys(Command::SetContentStatus {
        stage: s,
        status: ContentStatus::Available,
        share: Some("abc12345".into()),
    })
    .unwrap();
    assert_eq!(
        r.s.stage(s)
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .share
            .as_deref(),
        Some("abc12345")
    );
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let stage = r.play();
    // Not before the stage, not from someone not in it.
    let eve = r.join("eve", Intent::Play);
    assert_eq!(
        report(&mut r, eve, stage, Run::Forfeit),
        Err(Rejection::NotInLineup)
    );
    let live = Run::Live {
        life: 0.5,
        accuracy: 0.9,
        absolute: None,
        score: None,
        steps: 0,
    };
    report(&mut r, ana, stage, Run::Out { at: 20.0 }).unwrap();
    assert_eq!(r.s.phase, Phase::Stage { stage }, "waits for the host");
    // The same report again changes nothing; another is refused.
    assert_eq!(
        report(&mut r, ana, stage, Run::Out { at: 20.0 }),
        Ok(vec![])
    );
    assert_eq!(
        report(&mut r, ana, stage, live),
        Err(Rejection::AlreadyReported)
    );
    report(&mut r, host, stage, live).unwrap();
    let Phase::Intermission { next } = r.s.phase else {
        panic!("{:?}", r.s.phase)
    };
    let result = r.s.current.as_ref().unwrap().stage(stage).unwrap();
    let result = result.play.as_ref().unwrap().result.as_ref().unwrap();
    assert_eq!(result.placement(host).unwrap().place, 1);
    assert_eq!(result.placement(ana).unwrap().finish, Finish::Eliminated);
    // The second stage: the host finishes the song and Ana never reports;
    // the room waits its deadline after the song's end, then ranks her a
    // forfeit.
    r.by(
        host,
        Command::ChooseContent {
            stage: next,
            content: Some(song(2)),
        },
    )
    .unwrap();
    r.sys(Command::SetContentStatus {
        stage: next,
        status: ContentStatus::Available,
        share: Some("def67890".into()),
    })
    .unwrap();
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let stage = r.play();
    let start = r.now;
    assert_eq!(
        r.s.report_deadline(stage),
        Some(Millis(start + r.s.policy.outcomes.stage_cap_ms))
    );
    r.wait(60_000);
    report(&mut r, host, stage, live).unwrap();
    assert_eq!(
        r.s.next_deadline(),
        Some(Millis(r.now + r.s.policy.outcomes.after_song_end_ms))
    );
    r.wait(r.s.policy.outcomes.after_song_end_ms - 1);
    assert_eq!(r.s.phase, Phase::Stage { stage });
    r.wait(1);
    assert_eq!(r.s.phase, Phase::MatchResults);
    let m = r.s.current.as_ref().unwrap();
    let result = m.stage(stage).unwrap().play.as_ref().unwrap();
    let result = result.result.as_ref().unwrap();
    assert_eq!(result.placement(ana).unwrap().finish, Finish::Forfeit);
    assert_eq!(r.s.match_winners(), vec![host]);
    r.by(host, Command::OpenNextMatch).unwrap();
    assert_eq!(r.s.phase, Phase::Lobby);
}

/// A stage nobody finishes settles at the cap from its start; a stage with
/// CPUs is ranked by its authority alone (practice), never from reports.
#[test]
fn self_reported_stages_have_a_cap_and_cpu_stages_take_no_reports() {
    let mut rules = MatchRules::hosted();
    rules.stages = 1;
    let mut r = Room::new(rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let s = r.upcoming();
    r.choose(s, 1);
    r.sys(Command::SetContentStatus {
        stage: s,
        status: ContentStatus::Available,
        share: Some("abc12345".into()),
    })
    .unwrap();
    r.ready(host);
    r.ready(ana);
    r.start_stage().unwrap();
    let stage = r.play();
    report(&mut r, ana, stage, Run::Out { at: 3.0 }).unwrap();
    r.wait(r.s.policy.outcomes.stage_cap_ms);
    assert_eq!(r.s.phase, Phase::MatchResults);
    let m = r.s.current.as_ref().unwrap();
    let result = m.stage(stage).unwrap().play.as_ref().unwrap();
    let result = result.result.as_ref().unwrap();
    // A run out of the stage beats no report at all.
    assert_eq!(result.placement(ana).unwrap().place, 1);
    assert_eq!(result.placement(host).unwrap().finish, Finish::Forfeit);

    let mut p = Room::new(
        MatchRules::practice(OpponentSpec {
            level: 2,
            seed: 1,
            personality: None,
            name: None,
            targeting: None,
        }),
        ContentSource::LocalLibrary,
    );
    let me = p.host;
    let s = p.upcoming();
    p.choose(s, 1);
    p.ready(me);
    p.start_stage().unwrap();
    let stage = p.play();
    assert_eq!(
        report(&mut p, me, stage, Run::Forfeit),
        Err(Rejection::AuthorityOnly)
    );
    assert_eq!(p.s.report_deadline(stage), None);
}

// ---- M7b: the maintainer's match flow (pick, spotlight, early start) ----

fn card(n: u32) -> ContentCard {
    ContentCard {
        title: format!("Song {n}"),
        artist: "Artist".into(),
        difficulty: "Hard".into(),
        meter: 9,
        style: "dance-single".into(),
    }
}

impl Room {
    fn pick(&mut self, n: u32) -> Result<Vec<Event>, Rejection> {
        self.by(
            self.host,
            Command::Pick {
                content: song(n),
                card: Some(Box::new(card(n))),
            },
        )
    }

    fn window(&self) -> ReadinessWindow {
        match self.s.phase {
            Phase::Preparing {
                window: Some(w), ..
            } => w,
            ref p => panic!("no readiness window: {p:?}"),
        }
    }
}

#[test]
fn a_pick_starts_the_match_and_the_stage_starts_once_everyone_is_ready() {
    let mut r = Room::hosted(2);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let bo = r.join("bo", Intent::Play);
    r.pick(1).unwrap();
    // The match started with its first stage's window open, its song
    // carded for the picks' history.
    let s1 = r.upcoming();
    assert!(r.s.running_match().is_some());
    let w = r.window();
    assert_eq!(
        w.deadline,
        Millis(r.now + r.s.policy.readiness.window_ms),
        "a local song is available at once: the deadline counts from the pick"
    );
    assert_eq!(
        r.s.stage(s1).unwrap().content.as_ref().unwrap().card,
        Some(card(1))
    );
    // Picking again while the window runs is refused.
    assert_eq!(r.pick(2), Err(Rejection::WindowOpen));
    r.ready(host);
    r.ready(ana);
    assert!(matches!(r.s.phase, Phase::Preparing { .. }));
    // The last one ready starts it at once, long before the deadline.
    r.ready(bo);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    let s1 = r.play();
    r.end(s1, &[&[ana], &[host], &[bo]]);
    // Between stages the host's next pick goes on with the match.
    assert_eq!(r.s.phase, Phase::Intermission { next: r.upcoming() });
    r.pick(2).unwrap();
    let s2 = r.upcoming();
    assert_ne!(s1, s2);
    r.ready(host);
    r.ready(ana);
    // Bo does not ready: at the deadline he sits the stage out.
    let w = r.window();
    r.wait(w.deadline.0 - r.now);
    let s2p = r.play();
    assert_eq!(s2p, s2);
    assert_eq!(
        r.part(bo),
        Participation::SittingOut {
            stage: s2,
            reason: SitOutReason::NotReady
        }
    );
    r.end(s2, &[&[host], &[ana]]);
    assert_eq!(r.s.phase, Phase::MatchResults);
    // After the match's results a pick opens the next match.
    let before = r.s.current.as_ref().unwrap().id;
    r.pick(3).unwrap();
    assert_eq!(r.s.history.len(), 1, "the finished match is recorded");
    assert_ne!(r.s.current.as_ref().unwrap().id, before);
    assert!(matches!(
        r.s.phase,
        Phase::Preparing {
            window: Some(_),
            ..
        }
    ));
}

#[test]
fn a_pick_is_refused_whole_for_a_field_that_could_never_start_or_a_non_operator() {
    let mut r = Room::hosted(2);
    // Alone with no CPUs: two competitors are needed.
    assert_eq!(
        r.pick(1),
        Err(Rejection::NotEnoughReady {
            ready_humans: 1,
            competitors: 1
        })
    );
    assert!(r.s.current.is_none(), "nothing locked");
    let ana = r.join("ana", Intent::Play);
    assert!(matches!(
        r.by(
            ana,
            Command::Pick {
                content: song(1),
                card: None
            }
        ),
        Err(Rejection::Unauthorized { .. })
    ));
    let long = ContentCard {
        title: "x".repeat(ContentCard::MAX_TEXT + 1),
        ..card(1)
    };
    assert_eq!(
        r.by(
            r.host,
            Command::Pick {
                content: song(1),
                card: Some(Box::new(long))
            }
        ),
        Err(Rejection::InvalidCard)
    );
    // A stale view of the lobby's rules is refused too.
    let base = Some(r.s.revision);
    let mut rules = r.s.next.rules.clone();
    rules.stages = 3;
    r.by(r.host, Command::SaveNextRules { rules }).unwrap();
    assert!(matches!(
        r.based(
            r.host,
            base,
            Command::Pick {
                content: song(1),
                card: None
            }
        ),
        Err(Rejection::Stale { .. })
    ));
}

#[test]
fn the_spotlight_is_shared_once_and_a_pick_of_it_reuses_the_share() {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = Room::new(rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    // Ana may not set the spotlight; the host may, before any match.
    assert!(matches!(
        r.by(
            ana,
            Command::Spotlight {
                content: Some(song(1)),
                card: None
            }
        ),
        Err(Rejection::Unauthorized { .. })
    ));
    r.by(
        host,
        Command::Spotlight {
            content: Some(song(1)),
            card: Some(Box::new(card(1))),
        },
    )
    .unwrap();
    let spot = r.s.spotlight.clone().unwrap();
    assert_eq!((spot.by, spot.share.as_deref()), (host, None));
    // The same again changes nothing (no revision).
    let rev = r.s.revision;
    r.by(
        host,
        Command::Spotlight {
            content: Some(song(1)),
            card: Some(Box::new(card(1))),
        },
    )
    .unwrap();
    assert_eq!(r.s.revision, rev);
    // Its upload finished: the spotlight has its share.
    r.sys(Command::SetSpotlightShare {
        chart: song(1).chart,
        share: "Share001".into(),
    })
    .unwrap();
    assert_eq!(
        r.s.spotlight.as_ref().unwrap().share.as_deref(),
        Some("Share001")
    );
    // Picking it: available at once with that share, nothing uploaded.
    r.pick(1).unwrap();
    let s1 = r.upcoming();
    let c = r.s.stage(s1).unwrap().content.clone().unwrap();
    assert_eq!(
        (c.status, c.share.as_deref()),
        (ContentStatus::Available, Some("Share001"))
    );
    r.ready(host);
    r.ready(ana);
    let s1 = r.play();
    for (who, life) in [(host, 0.5), (ana, 0.4)] {
        report(
            &mut r,
            who,
            s1,
            Run::Live {
                life,
                accuracy: 0.9,
                absolute: None,
                score: None,
                steps: 0,
            },
        )
        .unwrap();
    }
    assert!(matches!(r.s.phase, Phase::Intermission { .. }));
    // A song never in the spotlight waits for its share: the window does
    // not run out meanwhile, and counts from when it arrives.
    r.pick(2).unwrap();
    let s2 = r.upcoming();
    assert!(r.s.content_pending(s2));
    assert_eq!(r.s.next_deadline(), None);
    assert!(r.wait(r.s.policy.readiness.window_ms * 3).is_empty());
    r.sys(Command::SetSpotlightShare {
        chart: song(2).chart,
        share: "Share002".into(),
    })
    .unwrap();
    assert!(!r.s.content_pending(s2));
    assert_eq!(
        r.window().deadline,
        Millis(r.now + r.s.policy.readiness.window_ms)
    );
    // A song in another stage's record is known too.
    assert_eq!(r.s.known_share(&song(1)).as_deref(), Some("Share001"));
    // The host leaving clears their spotlight.
    r.by(host, Command::Leave).unwrap();
    assert_eq!(r.s.spotlight, None);
}

#[test]
fn a_roster_seats_its_own_named_cpus_and_bounds_their_text() {
    let mut rules = MatchRules::hosted();
    rules.stages = 1;
    rules.seats = 3;
    rules.cpu.mode = CpuMode::Roster;
    rules.cpu.roster = vec![
        OpponentSpec {
            level: 1,
            seed: 11,
            personality: Some("jacks".into()),
            name: Some("Mira".into()),
            targeting: None,
        },
        OpponentSpec {
            level: 4,
            seed: 12,
            personality: None,
            name: Some("Tobi".into()),
            targeting: None,
        },
    ];
    assert_eq!(rules.cpu_seats(1), 2);
    let mut r = Room::new(rules.clone(), ContentSource::LocalLibrary);
    r.pick(1).unwrap();
    let cpus: Vec<OpponentSpec> =
        r.s.candidates()
            .into_iter()
            .filter_map(|id| r.s.opponent(id).cloned())
            .collect();
    assert_eq!(cpus, rules.cpu.roster, "each seat its own profile");
    // Alone with CPUs: the stage starts once the player is ready.
    r.ready(r.host);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    let mut long = rules.clone();
    long.cpu.roster[0].name = Some("n".repeat(OpponentSpec::MAX_TEXT + 1));
    assert_eq!(
        long.validate(&RoomPolicy::default(), 1),
        Err(RulesProblem::CpuText)
    );
    // Over the cap: still one policy check.
    let mut many = rules;
    many.seats = 74;
    many.cpu.roster = vec![many.cpu.roster[0].clone(); 73];
    assert!(matches!(
        many.validate(&RoomPolicy::default(), 1),
        Err(RulesProblem::OverCap(_))
    ));
}

/// M7d: a bot is a person's seat of its own kind (`bot:<name>`), told apart
/// from Twitch users by its identity alone, and never an administrator,
/// whatever the authority's join says.
#[test]
fn a_bot_is_its_own_kind_of_player_and_never_an_administrator() {
    let mut r = Room::hosted(2);
    let identity = Identity::bot("lumen-kite");
    assert_eq!(identity.as_str(), "bot:lumen-kite");
    assert!(identity.is_bot());
    assert!(!Identity::new("twitch:42").is_bot());
    assert!(
        !Identity::new("dev:bot:x").is_bot(),
        "only the prefix makes a bot"
    );
    r.sys(Command::Join {
        identity: identity.clone(),
        name: "Lumen Kite".into(),
        admin: true,
        intent: Intent::Play,
    })
    .unwrap();
    let bot = r.s.member_by_identity(&identity).unwrap();
    assert!(bot.is_bot());
    assert!(!bot.admin, "a bot's join never makes it an administrator");
    assert!(!bot.is_cpu(), "a bot plays a person's seat");
    let host = r.s.member(r.host).unwrap();
    assert!(!host.is_bot());
    // A person's seat: it may play and ready up as anyone.
    let id = bot.id;
    assert!(matches!(
        r.s.participation(id),
        Some(Participation::NotReady { .. })
    ));
}

/// M8 round 3 (D8): a member's own attack policy. Set by the member
/// themselves outside a stage's play (the lobby, between stages, with
/// their readiness kept), refused while a stage is played and to a CPU,
/// the authority or someone who left; a CPU's is its opponent's (a
/// roster entry may name one), Spread where it names none.
#[test]
fn a_member_sets_their_own_attack_policy_outside_a_stage() {
    use ddi_room::attacks::Targeting;
    let roster = vec![
        OpponentSpec {
            level: 1,
            seed: 11,
            personality: None,
            name: Some("Mira".into()),
            targeting: Some(Targeting::Leader),
        },
        OpponentSpec {
            level: 4,
            seed: 12,
            personality: None,
            name: Some("Tobi".into()),
            targeting: None,
        },
    ];
    let mut rules = MatchRules::practice(roster[0].clone());
    rules.cpu.mode = CpuMode::Roster;
    rules.cpu.roster = roster;
    rules.stages = 2;
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let me = r.host;
    for s in r.s.playlist.iter().map(|s| s.id).collect::<Vec<_>>() {
        r.choose(s, s.0 as u32);
    }
    assert_eq!(r.s.member(me).unwrap().targeting(), Targeting::Spread);
    let set = |targeting| Command::SetTargeting { targeting };
    // In the lobby; the same again changes nothing.
    let e = r.by(me, set(Targeting::Retaliate)).unwrap();
    assert_eq!(
        e,
        vec![Event::TargetingChanged {
            participant: me,
            targeting: Targeting::Retaliate
        }]
    );
    assert_eq!(r.by(me, set(Targeting::Retaliate)).unwrap(), vec![]);
    // Ready, then changed: still ready (it is nobody else's rule).
    r.ready(me);
    r.by(me, set(Targeting::Leader)).unwrap();
    assert_eq!(
        r.part(me),
        Participation::ReadyLoaded {
            stage: r.upcoming()
        }
    );
    // Never the authority's to set, nor anyone's but one's own.
    assert_eq!(r.sys(set(Targeting::Spread)), Err(Rejection::NotAMember));
    r.start_match();
    let cpus: Vec<ParticipantId> =
        r.s.members
            .values()
            .filter(|m| m.is_cpu())
            .map(|m| m.id)
            .collect();
    assert!(matches!(
        r.by(cpus[0], set(Targeting::Spread)),
        Err(Rejection::Unauthorized { .. })
    ));
    // The CPUs' policies are their opponents'.
    assert_eq!(r.s.member(cpus[0]).unwrap().targeting(), Targeting::Leader);
    assert_eq!(r.s.member(cpus[1]).unwrap().targeting(), Targeting::Spread);
    r.start_stage().unwrap();
    // The stage in play: refused, nothing changed.
    let s1 = r.play();
    assert_eq!(
        r.by(me, set(Targeting::Spread)),
        Err(Rejection::WrongPhase {
            phase: PhaseKind::Stage
        })
    );
    assert_eq!(r.s.member(me).unwrap().targeting(), Targeting::Leader);
    r.end(s1, &[&[me], &[cpus[0]], &[cpus[1]]]);
    // Between stages: taken again, for the next stage.
    r.by(me, set(Targeting::Spread)).unwrap();
    assert_eq!(r.s.member(me).unwrap().targeting(), Targeting::Spread);
    // Gone from the room, the only person in it: the room ended, and
    // decides nothing more.
    r.by(me, Command::Leave).unwrap();
    assert_eq!(
        r.by(me, set(Targeting::Leader)),
        Err(Rejection::RoomClosed)
    );
}

// ---- One picker at a time (2026-11-12): taking the pick over ----

fn rest(n: u32) -> Command {
    Command::Spotlight {
        content: Some(song(n)),
        card: Some(Box::new(card(n))),
    }
}

fn pick_cmd(n: u32) -> Command {
    Command::Pick {
        content: song(n),
        card: Some(Box::new(card(n))),
    }
}

fn co_host(r: &mut Room, who: ParticipantId, on: bool) {
    r.by(
        r.host,
        Command::SetCoHost {
            participant: who,
            on,
        },
    )
    .unwrap();
}

#[test]
fn a_co_host_takes_the_pick_over_and_the_owner_takes_it_back() {
    let mut r = Room::hosted(2);
    let host = r.host;
    let co = r.join("co", Intent::Play);
    let mem = r.join("mem", Intent::Play);
    co_host(&mut r, co, true);
    // The owner picks by default: theirs to rest on, the others' refused,
    // an operator told who has the pick and a member that it is not theirs
    // to choose.
    assert_eq!(r.s.picker(), Some(host));
    assert!(r.s.chooses(host) && !r.s.chooses(co) && !r.s.chooses(mem));
    assert!(r.s.may_take_pick(co) && !r.s.may_take_pick(host) && !r.s.may_take_pick(mem));
    r.by(host, rest(1)).unwrap();
    let not_theirs = Err(Rejection::Unauthorized {
        capability: Capability::ChooseContent,
    });
    assert_eq!(
        r.by(co, rest(2)),
        Err(Rejection::NotPicker { picker: Some(host) })
    );
    assert_eq!(
        r.by(co, pick_cmd(2)),
        Err(Rejection::NotPicker { picker: Some(host) })
    );
    assert_eq!(r.by(mem, rest(2)), not_theirs);
    assert_eq!(r.by(mem, pick_cmd(2)), not_theirs);
    assert_eq!(
        r.by(mem, Command::TakePick),
        Err(Rejection::Unauthorized {
            capability: Capability::TakePick
        })
    );
    assert_eq!(r.s.spotlight.as_ref().unwrap().by, host);
    // Taken at once: one event, the last picker's resting song gone with
    // them. Taking what one has changes nothing.
    assert_eq!(
        r.by(co, Command::TakePick).unwrap(),
        vec![Event::PickerChanged { picker: Some(co) }]
    );
    assert_eq!(r.s.picker(), Some(co));
    assert_eq!(r.s.spotlight, None);
    assert!(r.s.may_take_pick(host) && !r.s.may_take_pick(co));
    let rev = r.s.revision;
    assert_eq!(r.by(co, Command::TakePick).unwrap(), vec![]);
    assert_eq!(r.s.revision, rev);
    // The owner follows now, as anyone.
    assert_eq!(
        r.by(host, rest(1)),
        Err(Rejection::NotPicker { picker: Some(co) })
    );
    assert_eq!(
        r.by(host, pick_cmd(1)),
        Err(Rejection::NotPicker { picker: Some(co) })
    );
    let s1 = r.upcoming();
    assert_eq!(
        r.by(
            host,
            Command::ChooseContent {
                stage: s1,
                content: Some(song(1)),
            }
        ),
        Err(Rejection::NotPicker { picker: Some(co) })
    );
    // The co-host rests on their song and picks it: the match starts.
    r.by(co, rest(2)).unwrap();
    assert_eq!(r.s.spotlight.as_ref().unwrap().by, co);
    r.by(co, pick_cmd(2)).unwrap();
    assert!(matches!(
        r.s.phase,
        Phase::Preparing {
            window: Some(_),
            ..
        }
    ));
    for p in [host, co, mem] {
        r.ready(p);
    }
    let s1 = r.play();
    // During the stage's play the owner takes the pick back: the stage's
    // song stays, the next choice is theirs.
    r.by(host, Command::TakePick).unwrap();
    assert_eq!(r.s.picker(), Some(host));
    assert_eq!(r.s.stage(s1).unwrap().content.as_ref().unwrap().id, song(2));
    assert_eq!(r.s.phase, Phase::Stage { stage: s1 });
    r.end(s1, &[&[host], &[co], &[mem]]);
    // It stays with whoever has it across stages: the owner's now.
    assert_eq!(
        r.by(co, pick_cmd(3)),
        Err(Rejection::NotPicker { picker: Some(host) })
    );
    r.by(host, pick_cmd(3)).unwrap();
    for p in [host, co, mem] {
        r.ready(p);
    }
    let s2 = r.play();
    // And across matches: the co-host takes it during the last stage and
    // has it for the next match's first song.
    r.by(co, Command::TakePick).unwrap();
    r.end(s2, &[&[co], &[host], &[mem]]);
    assert_eq!(r.s.phase, Phase::MatchResults);
    assert_eq!(r.s.picker(), Some(co));
    r.by(co, pick_cmd(4)).unwrap();
    assert_eq!(r.s.history.len(), 1);
    assert_eq!(r.s.picker(), Some(co));
}

#[test]
fn the_pick_goes_back_to_the_owner_when_its_holder_cannot_hold_it() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let a = r.join("a", Intent::Play);
    let b = r.join("b", Intent::Play);
    let admin = r.join_as("admin", Intent::Watch, true).unwrap();
    co_host(&mut r, a, true);
    co_host(&mut r, b, true);
    let gone = |participant, connection| Command::SetConnection {
        participant,
        connection,
    };
    // Leaving the room: the owner's again, the song they rested on gone.
    r.by(a, Command::TakePick).unwrap();
    r.by(a, rest(1)).unwrap();
    let events = r.by(a, Command::Leave).unwrap();
    assert!(events.contains(&Event::PickerChanged { picker: Some(host) }));
    assert_eq!((r.s.picker(), r.s.spotlight.clone()), (Some(host), None));
    // Losing the role.
    r.by(b, Command::TakePick).unwrap();
    co_host(&mut r, b, false);
    assert_eq!(r.s.picker(), Some(host));
    assert_eq!(
        r.by(b, Command::TakePick),
        Err(Rejection::Unauthorized {
            capability: Capability::TakePick
        })
    );
    // An administrator operates without the role: demoted from co-host,
    // they keep the pick.
    r.by(admin, Command::TakePick).unwrap();
    co_host(&mut r, admin, true);
    co_host(&mut r, admin, false);
    assert_eq!(r.s.picker(), Some(admin));
    // A dropped connection keeps everything while the authority waits
    // (Reconnecting); past its patience (Disconnected) the pick does not
    // wait for them.
    r.sys(gone(admin, Connection::Reconnecting)).unwrap();
    assert_eq!(r.s.picker(), Some(admin));
    r.sys(gone(admin, Connection::Disconnected)).unwrap();
    assert_eq!(r.s.picker(), Some(host));
    // The owner away past it: the room passes on to someone connected
    // (b, a member: a left, the administrator is away), and the pick the
    // owner held goes with it, the song they rested on gone.
    r.by(host, rest(2)).unwrap();
    r.sys(gone(host, Connection::Disconnected)).unwrap();
    assert_eq!(r.s.owner().map(|m| m.id), Some(b));
    assert_eq!((r.s.picker(), r.s.spotlight.clone()), (Some(b), None));
    assert!(r.s.one_picker());
    // The one before, back in the room, is a member (the room passed on
    // by itself: no role kept); the new owner may make them a co-host, and
    // the pick is then theirs to take, not theirs.
    r.sys(Command::Join {
        identity: Identity::new("twitch:host"),
        name: "Host".into(),
        admin: false,
        intent: Intent::Play,
    })
    .unwrap();
    assert_eq!(r.s.member(host).unwrap().connection, Connection::Connected);
    assert_eq!(r.s.member(host).unwrap().role, Role::Member);
    assert!(!r.s.may_take_pick(host));
    let mut by_b = Room { host: b, ..clone(&r) };
    co_host(&mut by_b, host, true);
    r = by_b;
    assert_eq!(r.s.picker(), Some(b));
    assert_eq!(
        r.by(host, rest(3)),
        Err(Rejection::NotPicker { picker: Some(b) })
    );
    assert!(r.s.may_take_pick(host) && !r.s.may_take_pick(b));
    r.by(host, Command::TakePick).unwrap();
    assert_eq!(r.s.picker(), Some(host));
    // A picker leaving: the owner's again.
    r.by(host, Command::Leave).unwrap();
    assert_eq!(r.s.picker(), Some(b));
    // Nobody's only while nobody is there to choose: the owner away with
    // no one connected to pass the room to keeps it, and the pick waits
    // for whoever is first back, the owner or the one the room passes to.
    r.sys(gone(b, Connection::Disconnected)).unwrap();
    assert_eq!(r.s.owner().map(|m| m.id), Some(b));
    assert_eq!(r.s.picker(), None);
    let mut other = r.s.clone();
    r.sys(gone(b, Connection::Connected)).unwrap();
    assert_eq!(r.s.picker(), Some(b), "the owner back has it again");
    // An administrator who only observes (watching, no role of the
    // room's) is never passed the room: the owner away keeps it.
    other
        .apply(
            Actor::Authority,
            None,
            &gone(admin, Connection::Connected),
            Millis(0),
        )
        .unwrap();
    assert!(other.observes(other.member(admin).unwrap()));
    assert_eq!(other.owner().map(|m| m.id), Some(b));
    assert_eq!(other.picker(), None);
}

#[test]
fn a_resting_songs_share_stays_known_when_the_pick_changes_hands() {
    let mut rules = MatchRules::hosted();
    rules.stages = 1;
    let mut r = Room::new(rules, ContentSource::Shared);
    let host = r.host;
    let co = r.join("co", Intent::Play);
    co_host(&mut r, co, true);
    r.by(host, rest(1)).unwrap();
    r.sys(Command::SetSpotlightShare {
        chart: song(1).chart,
        share: "Share001".into(),
    })
    .unwrap();
    // The spotlight goes with its picker; the room's share of it is kept.
    r.by(co, Command::TakePick).unwrap();
    assert_eq!(r.s.spotlight, None);
    assert_eq!(r.s.known_share(&song(1)).as_deref(), Some("Share001"));
    assert_eq!(r.s.known_share(&song(2)), None);
    // The co-host's own song has no share yet; the owner, back, rests on
    // theirs again and it is shared at once, and so is its pick.
    r.by(co, rest(2)).unwrap();
    assert_eq!(r.s.spotlight.as_ref().unwrap().share, None);
    r.by(host, Command::TakePick).unwrap();
    r.by(host, rest(1)).unwrap();
    let spot = r.s.spotlight.clone().unwrap();
    assert_eq!((spot.by, spot.share.as_deref()), (host, Some("Share001")));
    r.by(co, Command::TakePick).unwrap();
    r.by(co, pick_cmd(1)).unwrap();
    let c = r.s.stage(r.upcoming()).unwrap().content.clone().unwrap();
    assert_eq!(
        (c.status, c.share.as_deref()),
        (ContentStatus::Available, Some("Share001"))
    );
    // A few are kept, the newest: never without bound.
    let mut room = r.s.clone();
    for n in 10..30u32 {
        room.spotlight = Some(Spotlight {
            content: song(n),
            card: card(n),
            by: co,
            share: Some(format!("Share{n:03}")),
        });
        let to = if n % 2 == 0 { host } else { co };
        room.commit(&[Event::PickerChanged { picker: Some(to) }]);
    }
    let kept = &room.picking.as_ref().unwrap().shares;
    assert_eq!(kept.len(), Picking::KEPT_SHARES);
    assert_eq!(room.known_share(&song(29)).as_deref(), Some("Share029"));
    assert_eq!(room.known_share(&song(10)), None);
}

/// A room from before the picker (its state has no `picking`: an
/// authority that predates it made it or still runs it) reads, names its
/// owner as the picker, and plays as it did: every operator chooses, and
/// nothing says the pick could be taken.
#[test]
fn a_room_from_before_the_picker_reads_and_chooses_as_it_did() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let co = r.join("co", Intent::Play);
    let mem = r.join("mem", Intent::Play);
    co_host(&mut r, co, true);
    // A room made now says who picks, on the wire.
    let mut json = serde_json::to_value(&r.s).unwrap();
    assert_eq!(json["picking"], serde_json::json!({ "picker": 1 }));
    // The same room as an older authority has it: no such field (and one
    // this build does not know, which it passes over).
    json.as_object_mut().unwrap().remove("picking");
    json["from_a_later_build"] = serde_json::json!({ "x": 1 });
    let old: RoomState = serde_json::from_value(json).unwrap();
    assert!(!old.one_picker());
    assert_eq!(old.picker(), Some(host), "the owner, as it always was");
    assert!(old.chooses(host) && old.chooses(co) && !old.chooses(mem));
    assert!(!old.may_take_pick(co) && !old.may_take_pick(host));
    assert!(
        serde_json::to_value(&old).unwrap().get("picking").is_none(),
        "written back as it was read"
    );
    // Run by this build's reducer, it still lets every operator choose.
    r.s = old.clone();
    r.mirror = old;
    r.by(co, rest(1)).unwrap();
    r.by(host, rest(2)).unwrap();
    assert_eq!(
        r.by(mem, rest(3)),
        Err(Rejection::Unauthorized {
            capability: Capability::ChooseContent
        })
    );
    // Leaving, a demotion and a lost connection move no pick there.
    r.sys(Command::SetConnection {
        participant: co,
        connection: Connection::Disconnected,
    })
    .unwrap();
    assert!(!r.s.one_picker());
    // Taking the pick is what turns one picker at a time on.
    r.by(host, Command::TakePick).unwrap();
    assert!(r.s.one_picker());
    assert_eq!(r.s.picker(), Some(host));
    assert_eq!(r.s.spotlight, None);
}

// ---- The host's succession (2026-11-12) ----

fn role(r: &Room, who: ParticipantId) -> Role {
    r.s.member(who).unwrap().role
}

fn owner(r: &Room) -> ParticipantId {
    r.s.owner().expect("an owner").id
}

fn connection(participant: ParticipantId, connection: Connection) -> Command {
    Command::SetConnection {
        participant,
        connection,
    }
}

fn back(r: &mut Room, name: &str) {
    r.sys(Command::Join {
        identity: Identity::new(format!("twitch:{name}")),
        name: name.into(),
        admin: false,
        intent: Intent::Play,
    })
    .unwrap();
}

fn bot(r: &mut Room, name: &str) -> ParticipantId {
    let identity = Identity::new(format!("bot:{name}"));
    r.sys(Command::Join {
        identity: identity.clone(),
        name: name.into(),
        admin: false,
        intent: Intent::Play,
    })
    .unwrap();
    r.s.member_by_identity(&identity).unwrap().id
}

/// Pass it on, the default: a host who leaves hands the room to a
/// co-host, the one who joined first among them; with none, to the member
/// who joined first; those connected before those away, never a bot. The
/// new owner has everything an owner has, the pick included; the host
/// gone keeps no role (`succession-v1`).
#[test]
fn a_host_who_leaves_passes_the_room_to_a_co_host_then_to_whoever_joined_first() {
    let mut r = Room::hosted(1);
    let host = r.host;
    assert_eq!(r.s.if_host_leaves(), HostLeaves::PassOn);
    assert!(r.s.has_succession());
    let robot = bot(&mut r, "robot");
    let a = r.join("a", Intent::Play);
    let b = r.join("b", Intent::Watch);
    let c = r.join("c", Intent::Play);
    let d = r.join("d", Intent::Play);
    co_host(&mut r, c, true);
    co_host(&mut r, d, true);
    assert_eq!(r.s.if_host_left(), HostDeparture::PassTo(c));
    // The co-host who joined first, at once; the one before is a member
    // from then on, gone or back: the room never makes anyone a co-host.
    let events = r.by(host, Command::Leave).unwrap();
    assert_eq!(
        events,
        vec![
            Event::MemberLeft { participant: host },
            Event::RoleChanged {
                participant: host,
                role: Role::Member
            },
            Event::RoleChanged {
                participant: c,
                role: Role::Owner
            },
            Event::PickerChanged { picker: Some(c) },
        ]
    );
    assert_eq!((owner(&r), role(&r, host), role(&r, d)), (c, Role::Member, Role::CoHost));
    assert!(r.s.member(host).unwrap().left);
    // Everything an owner has: access, co-hosts, the setting, the pick.
    r.by(
        c,
        Command::SetAccess {
            admission: Admission::Closed,
            rotate_invite: true,
        },
    )
    .unwrap();
    co_host(&mut Room { host: c, ..clone(&r) }, a, true);
    r.by(c, rest(1)).unwrap();
    // A co-host away past the authority's patience comes after everyone
    // who is connected; with no co-host connected, the member who joined
    // first (watching or playing).
    r.sys(connection(d, Connection::Disconnected)).unwrap();
    assert_eq!(r.s.if_host_left(), HostDeparture::PassTo(a));
    r.sys(connection(a, Connection::Reconnecting)).unwrap();
    assert_eq!(r.s.if_host_left(), HostDeparture::PassTo(b));
    r.by(c, Command::Leave).unwrap();
    assert_eq!((owner(&r), role(&r, c)), (b, Role::Member));
    assert_eq!(r.s.picker(), Some(b));
    // Whoever is still coming back before whoever is away, whatever the
    // role; a bot never.
    assert_eq!(r.s.if_host_left(), HostDeparture::PassTo(a));
    r.by(b, Command::Leave).unwrap();
    assert_eq!(owner(&r), a);
    assert_eq!(r.s.if_host_left(), HostDeparture::PassTo(d));
    r.by(a, Command::Leave).unwrap();
    assert_eq!(owner(&r), d);
    assert_eq!(r.s.picker(), None, "its new owner is away");
    assert_eq!(r.s.heir(d, true), None);
    assert!(!r.s.member(robot).unwrap().left);
    assert_eq!(r.s.closed, None);
    // The first owner, back by the link, is a member; the room's owner
    // is away past the grace, so it is theirs again, with the pick, and
    // the one away a member.
    back(&mut r, "host");
    assert_eq!((owner(&r), role(&r, d)), (host, Role::Member));
    assert_eq!(r.s.picker(), Some(host));
}

fn clone(r: &Room) -> Room {
    Room {
        s: r.s.clone(),
        mirror: r.mirror.clone(),
        now: r.now,
        host: r.host,
    }
}

/// Nobody to pass it to: a host who leaves alone, or with only bots in
/// the room, ends it. A room that ended decides nothing more.
#[test]
fn a_host_who_leaves_alone_or_among_bots_closes_the_room() {
    for with_bot in [false, true] {
        let mut r = Room::hosted(1);
        let host = r.host;
        if with_bot {
            bot(&mut r, "robot");
        }
        assert_eq!(r.s.if_host_left(), HostDeparture::Close);
        let events = r.by(host, Command::Leave).unwrap();
        assert_eq!(
            events,
            vec![
                Event::MemberLeft { participant: host },
                Event::RoomClosed {
                    reason: Closure::HostLeft
                },
            ]
        );
        assert_eq!(r.s.closed, Some(Closure::HostLeft));
        let rev = r.s.revision;
        assert_eq!(
            r.sys(Command::Join {
                identity: Identity::new("twitch:late"),
                name: "late".into(),
                admin: false,
                intent: Intent::Play,
            }),
            Err(Rejection::RoomClosed)
        );
        assert_eq!(r.by(host, Command::TakePick), Err(Rejection::RoomClosed));
        assert!(r.wait(60_000).is_empty());
        assert_eq!(r.s.revision, rev);
        // On the wire, and read back.
        let json = serde_json::to_value(&r.s).unwrap();
        assert_eq!(json["closed"], serde_json::json!("host_left"));
        assert_eq!(serde_json::from_value::<RoomState>(json).unwrap(), r.s);
    }
    // A bot is not made a co-host either: it follows a room.
    let mut r = Room::hosted(1);
    let robot = bot(&mut r, "robot");
    assert_eq!(
        r.by(
            r.host,
            Command::SetCoHost {
                participant: robot,
                on: true
            }
        ),
        Err(Rejection::NotAHuman)
    );
}

/// A dropped connection costs the host nothing inside the authority's
/// patience (Reconnecting: a reload). Past it (Disconnected) the room
/// passes to someone connected; the one before comes back a member, and
/// the new owner hands it back in one command. With nobody connected to
/// pass it to, the room keeps its absent host until someone is there.
#[test]
fn a_dropped_host_keeps_the_room_inside_the_grace_and_passes_it_on_past_it() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let a = r.join("a", Intent::Play);
    let b = r.join("b", Intent::Play);
    co_host(&mut r, b, true);
    // Inside the grace, and back: nothing changed hands.
    let events = r.sys(connection(host, Connection::Reconnecting)).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!((owner(&r), r.s.picker()), (host, Some(host)));
    r.sys(connection(host, Connection::Connected)).unwrap();
    assert_eq!((owner(&r), r.s.picker()), (host, Some(host)));
    // Past it: the co-host, with the pick.
    r.sys(connection(host, Connection::Reconnecting)).unwrap();
    let events = r.sys(connection(host, Connection::Disconnected)).unwrap();
    assert_eq!(
        events,
        vec![
            Event::ConnectionChanged {
                participant: host,
                connection: Connection::Disconnected
            },
            Event::RoleChanged {
                participant: host,
                role: Role::Member
            },
            Event::RoleChanged {
                participant: b,
                role: Role::Owner
            },
            Event::PickerChanged { picker: Some(b) },
        ]
    );
    assert!(!r.s.member(host).unwrap().left);
    // Back: a member, who neither operates nor owns.
    r.sys(connection(host, Connection::Connected)).unwrap();
    assert_eq!((owner(&r), role(&r, host)), (b, Role::Member));
    assert_eq!(
        r.by(host, Command::MakeHost { participant: host }),
        Err(Rejection::Unauthorized {
            capability: Capability::MakeHost
        })
    );
    assert_eq!(
        r.by(
            host,
            Command::SetHostLeaves {
                policy: HostLeaves::Close
            }
        ),
        Err(Rejection::Unauthorized {
            capability: Capability::ChangeAccess
        })
    );
    // One command hands it back.
    let events = r.by(b, Command::MakeHost { participant: host }).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!((owner(&r), role(&r, b)), (host, Role::CoHost));
    assert_eq!(r.s.picker(), Some(b), "the pick stays with whoever has it");

    // Nobody connected to pass it to: the host away keeps the room (the
    // pick waits), and the first one there afterwards has both.
    r.sys(connection(a, Connection::Disconnected)).unwrap();
    r.sys(connection(b, Connection::Disconnected)).unwrap();
    assert_eq!(r.s.picker(), Some(host));
    r.sys(connection(host, Connection::Disconnected)).unwrap();
    assert_eq!((owner(&r), r.s.picker(), r.s.closed), (host, None, None));
    let before = clone(&r);
    r.sys(connection(a, Connection::Connected)).unwrap();
    assert_eq!((owner(&r), r.s.picker()), (a, Some(a)));
    assert_eq!(role(&r, host), Role::Member);
    // Or the host is first back: nothing passed on.
    let mut r = before;
    r.sys(connection(host, Connection::Connected)).unwrap();
    assert_eq!((owner(&r), r.s.picker()), (host, Some(host)));
    // Someone new who comes in by the link while the host is away past
    // the grace is the one there to run it.
    r.sys(connection(host, Connection::Disconnected)).unwrap();
    let new = r.join("new", Intent::Play);
    assert_eq!(owner(&r), new);
}

/// A co-host comes before any member, also one away inside the reconnect
/// grace (`Reconnecting`): the room waits for them, nobody made
/// anything meanwhile and nobody choosing songs; back, they have it; past
/// the grace (`Disconnected`), the next co-host, else the member who joined
/// first (the maintainer, 2026-11-13: "a member can inherit the host role
/// if there isn't a co-host to inherit it").
#[test]
fn a_co_host_inside_the_grace_is_waited_for_before_any_member() {
    for host_leaves in [false, true] {
        let mut r = Room::hosted(1);
        let host = r.host;
        let a = r.join("a", Intent::Play);
        let c = r.join("c", Intent::Play);
        let d = r.join("d", Intent::Play);
        co_host(&mut r, c, true);
        co_host(&mut r, d, true);
        r.sys(connection(c, Connection::Reconnecting)).unwrap();
        r.sys(connection(d, Connection::Reconnecting)).unwrap();
        assert_eq!(r.s.succession(host, true), Succession::Wait(c));
        assert_eq!(r.s.if_host_left(), HostDeparture::PassTo(c));
        // The host goes: nothing changes hands while c may come back.
        let events = if host_leaves {
            r.by(host, Command::Leave).unwrap()
        } else {
            r.sys(connection(host, Connection::Disconnected)).unwrap()
        };
        assert_eq!(events.len(), 2, "{events:?}"); // gone, and the pick freed
        assert!(events.contains(&Event::PickerChanged { picker: None }) || !host_leaves);
        assert_eq!(owner(&r), host);
        assert_eq!(r.s.closed, None);
        assert_eq!((role(&r, a), role(&r, c)), (Role::Member, Role::CoHost));
        assert_eq!(r.s.picker(), None, "nobody chooses while the room waits");
        // Another arrival makes nobody anything either.
        let e = r.join("e", Intent::Play);
        assert_eq!((owner(&r), role(&r, e)), (host, Role::Member));
        let mut back_in = clone(&r);
        // c back: the room is theirs, with the pick; the host a member.
        back_in.sys(connection(c, Connection::Connected)).unwrap();
        assert_eq!((owner(&back_in), back_in.s.picker()), (c, Some(c)));
        assert_eq!(role(&back_in, host), Role::Member);
        // c past the grace: the next co-host, still inside it, is waited for.
        r.sys(connection(c, Connection::Disconnected)).unwrap();
        assert_eq!(owner(&r), host);
        assert_eq!(r.s.succession(host, true), Succession::Wait(d));
        // d past it too: the member who joined first, never a co-host
        // made by the room.
        r.sys(connection(d, Connection::Disconnected)).unwrap();
        assert_eq!(owner(&r), a);
        assert_eq!(r.s.picker(), Some(a));
        assert_eq!((role(&r, host), role(&r, e)), (Role::Member, Role::Member));
        assert_eq!((role(&r, c), role(&r, d)), (Role::CoHost, Role::CoHost));
    }
    // A connected co-host is not made to wait for one inside the grace.
    let mut r = Room::hosted(1);
    let host = r.host;
    let c = r.join("c", Intent::Play);
    let d = r.join("d", Intent::Play);
    co_host(&mut r, c, true);
    co_host(&mut r, d, true);
    r.sys(connection(c, Connection::Reconnecting)).unwrap();
    assert_eq!(r.s.succession(host, false), Succession::To(d));
}

/// An administrator who only observes (watching, no role of the room's)
/// is never passed the room; one who plays is a member like anyone, and
/// one made a co-host a co-host.
#[test]
fn an_observing_administrator_is_skipped() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let admin = r.join_as("admin", Intent::Watch, true).unwrap();
    let a = r.join("a", Intent::Play);
    assert!(r.s.observes(r.s.member(admin).unwrap()));
    assert_eq!(r.s.if_host_left(), HostDeparture::PassTo(a));
    // Alone with the observer, the host's leaving ends the room.
    let mut alone = clone(&r);
    alone.by(a, Command::Leave).unwrap();
    assert_eq!(alone.s.if_host_left(), HostDeparture::Close);
    // Playing, the administrator is a player who joined before a.
    r.by(
        admin,
        Command::SetIntent {
            intent: Intent::Play,
        },
    )
    .unwrap();
    assert!(!r.s.observes(r.s.member(admin).unwrap()));
    assert_eq!(r.s.if_host_left(), HostDeparture::PassTo(admin));
    // Watching again but made a co-host: a co-host.
    r.by(
        admin,
        Command::SetIntent {
            intent: Intent::Watch,
        },
    )
    .unwrap();
    co_host(&mut r, admin, true);
    assert!(!r.s.observes(r.s.member(admin).unwrap()));
    r.by(host, Command::Leave).unwrap();
    assert_eq!(owner(&r), admin);
}

/// A co-host who is away or leaves while the host is connected changes no
/// role (the maintainer, 2026-11-13: "if a co-host disconnects and there's
/// still a host connected there should be no role changes"); a pick they
/// held goes back to the host.
#[test]
fn a_co_host_going_while_the_host_is_there_changes_no_role() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let a = r.join("a", Intent::Play);
    let c = r.join("c", Intent::Play);
    let d = r.join("d", Intent::Play);
    co_host(&mut r, c, true);
    co_host(&mut r, d, true);
    let roles = |r: &Room| [host, a, c, d].map(|p| role(r, p));
    let before = roles(&r);
    r.by(c, Command::TakePick).unwrap();
    for step in [Connection::Reconnecting, Connection::Disconnected] {
        let events = r.sys(connection(c, step)).unwrap();
        assert!(
            !events.iter().any(|e| matches!(e, Event::RoleChanged { .. })),
            "{events:?}"
        );
        assert_eq!(roles(&r), before);
    }
    assert_eq!(r.s.picker(), Some(host), "the pick back with the host");
    r.sys(connection(c, Connection::Connected)).unwrap();
    assert_eq!(roles(&r), before);
    r.by(d, Command::TakePick).unwrap();
    let events = r.by(d, Command::Leave).unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::RoleChanged { .. })));
    assert_eq!(roles(&r), before);
    assert_eq!((owner(&r), r.s.picker()), (host, Some(host)));
}

/// "Close the room": the setting is the owner's alone, kept on the wire;
/// under it the room ends for everyone when its host leaves or is away
/// past the grace, with others there or not.
#[test]
fn close_the_room_ends_it_for_everyone_when_its_host_goes() {
    let close = Command::SetHostLeaves {
        policy: HostLeaves::Close,
    };
    let mut r = Room::hosted(1);
    let host = r.host;
    let a = r.join("a", Intent::Play);
    co_host(&mut r, a, true);
    assert_eq!(
        r.by(a, close.clone()),
        Err(Rejection::Unauthorized {
            capability: Capability::ChangeAccess
        })
    );
    let events = r.by(host, close.clone()).unwrap();
    assert_eq!(
        events,
        vec![Event::HostLeavesChanged {
            policy: HostLeaves::Close
        }]
    );
    assert!(r.by(host, close.clone()).unwrap().is_empty());
    assert_eq!(r.s.if_host_left(), HostDeparture::Close);
    let json = serde_json::to_value(&r.s).unwrap();
    assert_eq!(
        json["hosting"],
        serde_json::json!({ "if_host_leaves": "close" })
    );
    // A drop inside the grace keeps it; past it the room ends.
    let mut dropped = clone(&r);
    dropped
        .sys(connection(host, Connection::Reconnecting))
        .unwrap();
    assert_eq!(dropped.s.closed, None);
    let events = dropped
        .sys(connection(host, Connection::Disconnected))
        .unwrap();
    assert_eq!(
        events.last(),
        Some(&Event::RoomClosed {
            reason: Closure::HostLeft
        })
    );
    assert_eq!(owner(&dropped), host, "nothing passed on");
    // Leaving: at once.
    let events = r.by(host, Command::Leave).unwrap();
    assert_eq!(
        events,
        vec![
            Event::MemberLeft { participant: host },
            Event::RoomClosed {
                reason: Closure::HostLeft
            },
        ]
    );
    assert_eq!(r.by(a, Command::TakePick), Err(Rejection::RoomClosed));
    // Back to passing it on is one command too.
    let mut r = Room::hosted(1);
    r.by(r.host, close).unwrap();
    r.by(
        r.host,
        Command::SetHostLeaves {
            policy: HostLeaves::PassOn,
        },
    )
    .unwrap();
    assert_eq!(r.s.if_host_leaves(), HostLeaves::PassOn);
}

/// A running match goes on under the new owner: the stage in play keeps
/// its lineup (the one who left a forfeit when it settles), the pick and
/// the next stage are the new owner's, and what the one before sends as
/// an owner is refused whole.
#[test]
fn a_running_match_goes_on_under_the_new_owner() {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let host = r.host;
    let a = r.join("a", Intent::Play);
    let b = r.join("b", Intent::Play);
    r.by(host, pick_cmd(1)).unwrap();
    for p in [host, a, b] {
        r.ready(p);
    }
    let s1 = r.play();
    let before = r.s.current.as_ref().unwrap().stages[0].play.clone();
    // The host leaves mid-stage: a, who joined first, has the room.
    r.by(host, Command::Leave).unwrap();
    assert_eq!((owner(&r), r.s.picker()), (a, Some(a)));
    assert_eq!(r.s.phase, Phase::Stage { stage: s1 });
    assert_eq!(r.s.current.as_ref().unwrap().stages[0].play, before);
    // The stage settles on the others' reports once its patience for the
    // missing one ends: the one who left is a forfeit.
    for (who, life) in [(a, 0.8), (b, 0.6)] {
        let run = Run::Live {
            life,
            accuracy: 0.9,
            absolute: None,
            score: None,
            steps: 0,
        };
        report(&mut r, who, s1, run).unwrap();
    }
    assert_eq!(r.s.phase, Phase::Stage { stage: s1 });
    r.wait(RoomPolicy::default().outcomes.stage_cap_ms);
    let result = r.s.current.as_ref().unwrap().stages[0]
        .play
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap();
    assert_eq!(result.placement(a).unwrap().place, 1);
    assert_eq!(result.placement(host).unwrap().finish, Finish::Forfeit);
    // The one before, back, is a member: an owner's commands are refused
    // whole, and so are an operator's.
    back(&mut r, "host");
    assert_eq!(role(&r, host), Role::Member);
    assert_eq!(
        r.by(
            host,
            Command::SetCoHost {
                participant: b,
                on: true
            }
        ),
        Err(Rejection::Unauthorized {
            capability: Capability::ManageCoHosts
        })
    );
    assert_eq!(
        r.by(host, pick_cmd(2)),
        Err(Rejection::Unauthorized {
            capability: Capability::ChooseContent
        })
    );
    // The new owner picks the next stage; the match goes on.
    r.by(a, pick_cmd(2)).unwrap();
    assert!(matches!(
        r.s.phase,
        Phase::Preparing {
            window: Some(_),
            ..
        }
    ));
    // A host away past the grace mid-countdown: the start is canceled as
    // for any starter, and the room passes on.
    for p in [host, a, b] {
        r.ready(p);
    }
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    r.sys(connection(a, Connection::Disconnected)).unwrap();
    assert_eq!(owner(&r), host, "no co-host: the member who joined first");
    assert!(matches!(
        r.s.phase,
        Phase::Preparing { window: None, .. }
    ));
}

/// Make host: the owner hands the room to someone present and connected,
/// never a bot, and is a co-host from then on. Nobody else may.
#[test]
fn the_owner_hands_the_room_over_and_becomes_a_co_host() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let a = r.join("a", Intent::Play);
    let b = r.join("b", Intent::Play);
    let robot = bot(&mut r, "robot");
    let admin = r.join_as("admin", Intent::Watch, true).unwrap();
    co_host(&mut r, b, true);
    let make = |participant| Command::MakeHost { participant };
    for who in [a, b, admin] {
        assert_eq!(
            r.by(who, make(who)),
            Err(Rejection::Unauthorized {
                capability: Capability::MakeHost
            })
        );
    }
    assert_eq!(r.by(host, make(robot)), Err(Rejection::NotAHuman));
    assert_eq!(
        r.by(host, make(ParticipantId(99))),
        Err(Rejection::UnknownParticipant)
    );
    assert!(r.by(host, make(host)).unwrap().is_empty());
    r.sys(connection(a, Connection::Reconnecting)).unwrap();
    assert_eq!(r.by(host, make(a)), Err(Rejection::NotPresent));
    r.sys(connection(a, Connection::Connected)).unwrap();
    let events = r.by(host, make(a)).unwrap();
    assert_eq!(
        events,
        vec![
            Event::RoleChanged {
                participant: host,
                role: Role::CoHost
            },
            Event::RoleChanged {
                participant: a,
                role: Role::Owner
            },
        ]
    );
    assert_eq!(r.s.picker(), Some(host), "the pick stays where it is");
    // The giver cannot take it back alone; the new owner manages the
    // co-hosts, the giver included.
    assert_eq!(
        r.by(host, make(host)),
        Err(Rejection::Unauthorized {
            capability: Capability::MakeHost
        })
    );
    r.by(
        a,
        Command::SetCoHost {
            participant: host,
            on: false,
        },
    )
    .unwrap();
    assert_eq!((role(&r, host), r.s.picker()), (Role::Member, Some(a)));
    // During a stage too: it is a room matter, not the match's.
    let mut r = Room::hosted(1);
    let a = r.join("a", Intent::Play);
    r.by(r.host, pick_cmd(1)).unwrap();
    for p in [r.host, a] {
        r.ready(p);
    }
    r.play();
    r.by(r.host, make(a)).unwrap();
    assert_eq!(owner(&r), a);
}

/// A room from before the setting (its state has no `hosting`) reads as
/// Pass it on, says nothing of it until this reducer changes something
/// there, and passes on as any room.
#[test]
fn a_room_from_before_the_setting_reads_as_pass_it_on() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let a = r.join("a", Intent::Play);
    let mut json = serde_json::to_value(&r.s).unwrap();
    assert_eq!(
        json["hosting"],
        serde_json::json!({ "if_host_leaves": "pass_on" })
    );
    assert!(json.get("closed").is_none());
    json.as_object_mut().unwrap().remove("hosting");
    let old: RoomState = serde_json::from_value(json).unwrap();
    assert!(!old.has_succession());
    assert_eq!(old.if_host_leaves(), HostLeaves::PassOn);
    assert!(
        serde_json::to_value(&old).unwrap().get("hosting").is_none(),
        "written back as it was read"
    );
    // A command that changes nothing leaves it as it was; the first
    // change writes the setting, as one more event of its revision.
    r.s = old.clone();
    r.mirror = old.clone();
    assert!(r.by(a, Command::SetIntent { intent: Intent::Play }).unwrap().is_empty());
    assert!(!r.s.has_succession());
    let events = r.by(a, Command::SetIntent { intent: Intent::Watch }).unwrap();
    assert_eq!(
        events.last(),
        Some(&Event::HostLeavesChanged {
            policy: HostLeaves::PassOn
        })
    );
    assert!(r.s.has_succession());
    // Its host leaving passes it on, whichever comes first.
    r.s = old.clone();
    r.mirror = old;
    let events = r.by(host, Command::Leave).unwrap();
    assert!(events.contains(&Event::RoleChanged {
        participant: a,
        role: Role::Owner
    }));
    assert_eq!(owner(&r), a);
    assert!(r.s.has_succession());
}

// ---- Watching (2026-11-12, `watch-v0`): Play or Watch, each member's own ----

fn intent(intent: Intent) -> Command {
    Command::SetIntent { intent }
}

/// A member who watches is no competitor: no seat, no place in the cap,
/// nothing the readiness window waits for, no placement and no award; a
/// watching host still picks and starts.
#[test]
fn a_watcher_is_no_competitor_and_a_watching_host_still_runs_the_room() {
    let mut r = Room::hosted(3);
    // A cap of nine (a value: 73 by default since 2026-11-13).
    r.cap(9);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let bo = r.join("bo", Intent::Play);
    let cy = r.join("cy", Intent::Play);
    assert_eq!(r.s.reserved_humans(), 4);
    // In the lobby; the same again changes nothing.
    assert_eq!(
        r.by(host, intent(Intent::Watch)).unwrap(),
        vec![Event::IntentChanged {
            participant: host,
            intent: Intent::Watch
        }]
    );
    assert_eq!(r.by(host, intent(Intent::Watch)).unwrap(), vec![]);
    assert_eq!(r.part(host), Participation::Watching);
    // The cap counts those who want to play.
    assert_eq!(r.s.reserved_humans(), 3);
    assert!(
        r.s.policy
            .check_competitors(r.s.reserved_humans() + 6)
            .is_ok()
    );
    assert!(
        r.s.policy
            .check_competitors(r.s.reserved_humans() + 7)
            .is_err()
    );
    // No readiness for a watcher.
    assert_eq!(r.set_ready(host, true), Err(Rejection::NotPlaying));
    // The watching host picks: the match starts without them.
    r.pick(1).unwrap();
    let s1 = r.upcoming();
    let m = r.s.running_match().unwrap();
    assert!(!m.roster.contains_key(&host));
    assert_eq!(m.roster.len(), 3);
    assert_eq!(r.part(host), Participation::Watching);
    // The window waits for the players alone: it ends once they are ready.
    r.ready(ana);
    r.ready(bo);
    assert!(matches!(r.s.phase, Phase::Preparing { .. }));
    r.ready(cy);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    assert_eq!(r.play(), s1);
    assert_eq!(r.part(host), Participation::Watching);
    r.end(s1, &[&[ana], &[bo], &[cy]]);
    // Not ranked, not awarded, not in the standings.
    let result = r.s.stage(s1).unwrap().play.clone().unwrap().result.unwrap();
    assert!(result.placement(host).is_none());
    assert!(
        r.s.standings()
            .unwrap()
            .iter()
            .all(|s| s.participant != host)
    );
    // Stage 2: Ana chooses to watch while it gathers readiness: her Ready
    // is cleared, she sits it out as watching, the others start without
    // waiting for her.
    r.pick(2).unwrap();
    let s2 = r.upcoming();
    r.ready(ana);
    r.by(ana, intent(Intent::Watch)).unwrap();
    assert_eq!(r.part(ana), Participation::Watching);
    r.ready(bo);
    r.ready(cy);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    let play = r.s.stage(s2).unwrap().play.clone().unwrap();
    assert_eq!(play.lineup.len(), 2);
    assert_eq!(play.sat_out.get(&ana), Some(&SitOutReason::Watching));
    r.play();
    r.end(s2, &[&[bo], &[cy]]);
    // She keeps what she won, with nothing for the stage she watched.
    let total = r.total(ana);
    assert!(total > Points::whole(0));
    let s2_result = r.s.stage(s2).unwrap().play.clone().unwrap().result.unwrap();
    assert!(s2_result.placement(ana).is_none_or(|p| p.points == Points::whole(0)));
    assert_eq!(r.total(ana), total);
    // Back to Play: in from the next stage (she is of the match).
    r.by(ana, intent(Intent::Play)).unwrap();
    r.pick(3).unwrap();
    let s3 = r.upcoming();
    assert_eq!(r.part(ana), Participation::NotReady { stage: s3 });
    // The host, who watched when the match began, waits for the next one.
    r.by(host, intent(Intent::Play)).unwrap();
    assert_eq!(r.part(host), Participation::WaitingNextMatch);
}

/// Where every person watches there is nothing to start, unless CPUs
/// compete: a hosted room refuses the pick whole; CPU practice plays its
/// CPUs against each other, started at once (nobody is waited for).
#[test]
fn a_stage_needs_competitors_and_practice_cpus_play_alone_for_a_watcher() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    r.by(host, intent(Intent::Watch)).unwrap();
    r.by(ana, intent(Intent::Watch)).unwrap();
    let before = r.s.clone();
    assert_eq!(r.pick(1), Err(Rejection::NoPlayingHumans));
    assert_eq!(r.s, before);
    assert_eq!(r.s.phase, Phase::Lobby);
    // One of them plays: too few to compete (the policy's two).
    r.by(ana, intent(Intent::Play)).unwrap();
    assert!(matches!(
        r.pick(1),
        Err(Rejection::NotEnoughReady {
            ready_humans: 1,
            competitors: 1
        })
    ));

    // CPU practice, its person watching.
    let opponent = OpponentSpec {
        level: 0,
        seed: 42,
        personality: None,
        name: None,
        targeting: None,
    };
    let mut rules = MatchRules::practice(opponent.clone());
    rules.stages = 2;
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let me = r.host;
    r.by(me, intent(Intent::Watch)).unwrap();
    let events = r.pick(1).unwrap();
    assert!(matches!(
        &events[0],
        Event::MatchStarted { humans, cpus, .. } if humans.is_empty() && cpus.len() == 3
    ));
    // Nobody to wait for: the start is committed with the pick.
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    let s1 = r.play();
    let cpus: Vec<ParticipantId> =
        r.s.members
            .values()
            .filter(|m| m.is_cpu())
            .map(|m| m.id)
            .collect();
    let play = r.s.stage(s1).unwrap().play.clone().unwrap();
    assert_eq!(play.lineup.iter().copied().collect::<Vec<_>>(), cpus);
    assert_eq!(r.part(me), Participation::Watching);
    r.end(s1, &[&[cpus[0]], &[cpus[1]], &[cpus[2]]]);
    assert_eq!(r.s.standings().unwrap().len(), 3);
    // Playing again mid-match: the next match's (practice admits nobody
    // into a running match).
    r.by(me, intent(Intent::Play)).unwrap();
    assert_eq!(r.part(me), Participation::WaitingNextMatch);
    r.by(me, intent(Intent::Watch)).unwrap();
    r.pick(2).unwrap();
    let s2 = r.play();
    r.end(s2, &[&[cpus[1]], &[cpus[0]], &[cpus[2]]]);
    assert_eq!(r.s.phase, Phase::MatchResults);
    assert!(!r.s.match_winners().contains(&me));

    // A single CPU has nobody to compete against.
    let mut rules = MatchRules::practice(opponent);
    rules.cpu.mode = CpuMode::Fixed { count: 1 };
    rules.seats = 2;
    let mut r = Room::new(rules, ContentSource::LocalLibrary);
    let me = r.host;
    r.by(me, intent(Intent::Watch)).unwrap();
    let before = r.s.clone();
    assert!(matches!(
        r.pick(1),
        Err(Rejection::NotEnoughReady {
            ready_humans: 0,
            competitors: 1
        })
    ));
    assert_eq!(r.s, before);
}

#[test]
fn a_member_held_to_watching_plays_only_once_the_owner_lets_them() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let fan = r.join("fan", Intent::Watch);
    let other = r.join("other", Intent::Play);
    // The authority holds the newcomer to watching (the watch-only link).
    let events = r
        .sys(Command::SetWatchOnly {
            participant: fan,
            on: true,
        })
        .unwrap();
    assert_eq!(
        events,
        vec![Event::WatchOnlyChanged {
            participant: fan,
            on: true
        }]
    );
    assert!(r.s.members[&fan].watch_only);
    let play = Command::SetIntent {
        intent: Intent::Play,
    };
    assert_eq!(r.by(fan, play.clone()), Err(Rejection::WatchOnly));
    // Nobody but the authority holds anyone to it; nobody but an
    // operator lets them go.
    assert_eq!(
        r.by(
            host,
            Command::SetWatchOnly {
                participant: other,
                on: true
            }
        ),
        Err(Rejection::AuthorityOnly)
    );
    let let_play = Command::SetWatchOnly {
        participant: fan,
        on: false,
    };
    assert!(matches!(
        r.by(fan, let_play.clone()),
        Err(Rejection::Unauthorized { .. })
    ));
    assert!(matches!(
        r.by(other, let_play.clone()),
        Err(Rejection::Unauthorized { .. })
    ));
    // Let play, by a co-host too (round B1): still watching until they
    // choose Play.
    let mut by_co = clone(&r);
    co_host(&mut by_co, other, true);
    by_co.by(other, let_play.clone()).unwrap();
    assert!(!by_co.s.members[&fan].watch_only);
    r.by(host, let_play.clone()).unwrap();
    assert!(!r.s.members[&fan].watch_only);
    assert_eq!(r.s.members[&fan].intent, Intent::Watch);
    assert_eq!(r.by(host, let_play), Ok(vec![]));
    r.by(fan, play.clone()).unwrap();
    assert_eq!(r.s.members[&fan].intent, Intent::Play);
    // An owner is never held to it.
    r.sys(Command::SetWatchOnly {
        participant: host,
        on: true,
    })
    .unwrap();
    r.by(
        host,
        Command::SetIntent {
            intent: Intent::Watch,
        },
    )
    .unwrap();
    r.by(host, play).unwrap();
}

/// Replacing the invite is an operator's, as copying it is (round B1);
/// the admission stays the owner's, also beside a replacement.
#[test]
fn a_co_host_replaces_the_invite_and_only_the_owner_changes_the_admission() {
    let mut r = Room::hosted(1);
    let host = r.host;
    let co = r.join("co", Intent::Play);
    let mem = r.join("mem", Intent::Play);
    co_host(&mut r, co, true);
    let rotate = |admission| Command::SetAccess {
        admission,
        rotate_invite: true,
    };
    let open = r.s.access.admission;
    let closed = Admission::Closed;
    assert_ne!(open, closed);
    let generation = r.s.access.invite_generation;
    assert_eq!(
        r.by(mem, rotate(open)),
        Err(Rejection::Unauthorized {
            capability: Capability::ReplaceInvite
        })
    );
    r.by(co, rotate(open)).unwrap();
    assert_eq!(r.s.access.invite_generation, generation + 1);
    assert_eq!(
        r.by(co, rotate(closed)),
        Err(Rejection::Unauthorized {
            capability: Capability::ChangeAccess
        })
    );
    r.by(host, rotate(closed)).unwrap();
    assert_eq!(r.s.access.invite_generation, generation + 2);
    assert_eq!(r.s.access.admission, closed);
}

/// The Rules pop-up's "Apply and restart" (2026-11-13): the next match's
/// rules saved and the running match stopped as one change, by an operator
/// while a match runs (in any of its phases, a stage's play included, as
/// Stop match), refused whole otherwise; the next match starts under the
/// new rules.
#[test]
fn restarting_a_match_saves_the_rules_and_stops_it_as_one_change() {
    let mut r = Room::hosted(3);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let co = r.join("co", Intent::Play);
    r.by(
        host,
        Command::SetCoHost {
            participant: co,
            on: true,
        },
    )
    .unwrap();
    let mut draft = MatchRules::hosted();
    draft.stages = 2;
    draft.difficulty = MatchDifficulty::Hard;
    draft.boos = true;
    let restart = |rules: &MatchRules| Command::RestartMatch {
        rules: rules.clone(),
    };
    // No match running: nothing to stop, nothing saved.
    assert_eq!(
        r.by(host, restart(&draft)),
        Err(Rejection::Unauthorized {
            capability: Capability::StopMatch
        })
    );
    assert_ne!(r.s.next.rules, draft);
    r.start_match();
    let locked = r.s.current.as_ref().unwrap().rules.clone();
    let s = r.upcoming();
    r.choose(s, 1);
    for p in [host, ana, co] {
        r.ready(p);
    }
    r.start_stage().unwrap();
    let s1 = r.play();
    r.end(s1, &[&[ana], &[host], &[co]]);
    assert!(matches!(r.s.phase, Phase::Intermission { .. }));
    // A member may not.
    assert_eq!(
        r.by(ana, restart(&draft)),
        Err(Rejection::Unauthorized {
            capability: Capability::EditNextRules
        })
    );
    // Rules the room cannot use: refused whole, the match still running.
    let mut bad = draft.clone();
    bad.stages = 0;
    assert!(matches!(
        r.by(host, restart(&bad)),
        Err(Rejection::InvalidRules(_))
    ));
    // Made against a draft someone saved since: stale, nothing stopped.
    let before = r.s.revision;
    let mut other = MatchRules::hosted();
    other.stages = 4;
    r.by(host, Command::SaveNextRules { rules: other }).unwrap();
    assert_eq!(
        r.based(co, Some(before), restart(&draft)),
        Err(Rejection::Stale {
            base: before,
            changed_at: r.s.next.saved_at
        })
    );
    assert!(r.s.phase.kind().match_running());
    // A co-host's, during the intermission: one revision.
    let rev = r.s.revision;
    let events = r.by(co, restart(&draft)).unwrap();
    assert_eq!(r.s.revision, Revision(rev.0 + 1), "one change");
    let saved = events
        .iter()
        .position(|e| matches!(e, Event::NextRulesSaved { .. }))
        .unwrap();
    let stopped = events
        .iter()
        .position(|e| {
            *e == Event::MatchInterrupted {
                reason: Interruption::Stopped,
            }
        })
        .unwrap();
    assert!(saved < stopped, "saved, then stopped");
    assert_eq!(
        r.s.phase,
        Phase::Interrupted {
            reason: Interruption::Stopped
        }
    );
    assert_eq!(r.s.current.as_ref().unwrap().rules, locked, "kept as played");
    assert_eq!(r.s.next.rules, draft);
    // Nothing more to stop.
    assert_eq!(
        r.by(host, restart(&draft)),
        Err(Rejection::Unauthorized {
            capability: Capability::StopMatch
        })
    );
    // The next match is played under the new rules.
    r.by(host, Command::OpenNextMatch).unwrap();
    assert_eq!(
        r.s.history.last().unwrap().ending,
        MatchEnding::Interrupted {
            reason: Interruption::Stopped
        }
    );
    r.start_match();
    let m = r.s.current.as_ref().unwrap();
    assert_eq!(m.rules, draft);
    assert_eq!(m.stages.len(), 2);
    // During a stage's play, as Stop match.
    let s = r.upcoming();
    r.choose(s, 2);
    for p in [host, ana, co] {
        r.ready(p);
    }
    r.start_stage().unwrap();
    r.play();
    let mut again = draft.clone();
    again.boos = false;
    r.by(host, restart(&again)).unwrap();
    assert_eq!(r.s.phase.kind(), PhaseKind::Interrupted);
    assert_eq!(r.s.next.rules, again);
}

/// A room's policy says whether its authority reads `RestartMatch`: every
/// room this build makes does; a policy stored or sent by an authority
/// that predates it (no field) does not, so a client never sends it
/// there.
#[test]
fn a_policy_from_before_the_restart_says_so() {
    assert!(RoomPolicy::default().restart);
    let mut json = serde_json::to_value(RoomPolicy::default()).unwrap();
    assert_eq!(json["restart"], serde_json::Value::Bool(true));
    json.as_object_mut().unwrap().remove("restart");
    let old: RoomPolicy = serde_json::from_value(json).unwrap();
    assert!(!old.restart);
    // Left out of the JSON where false, as it was.
    let back = serde_json::to_value(&old).unwrap();
    assert!(back.get("restart").is_none());
}

/// A change of the running match's per-stage rules (2026-11-13): Hard
/// timing and Boos from stage 2, Attacks off from stage 3 made in the
/// intermission after stage 2, each stage played under its own; what
/// came before keeps its rules; the mirror equal throughout.
#[test]
fn per_stage_rules_change_between_stages_and_each_stage_keeps_its_own() {
    let mut r = Room::hosted(4);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    // In the lobby there is no running match to change.
    let hard = PlayRules {
        difficulty: MatchDifficulty::Hard,
        boos: true,
        ..MatchRules::hosted().play_rules()
    };
    assert!(matches!(
        r.by(
            host,
            Command::ChangeStageRules {
                from: StageId(1),
                rules: hard.clone(),
            }
        ),
        Err(Rejection::WrongPhase { .. })
    ));
    assert_eq!(r.s.rules_land_on(), None);
    r.pick(1).unwrap();
    let s1 = r.upcoming();
    let stages: Vec<StageId> =
        r.s.current
            .as_ref()
            .unwrap()
            .stages
            .iter()
            .map(|s| s.id)
            .collect();
    // Stage 1's window is open (its prompt tells its rules): a change
    // lands on stage 2.
    assert_eq!(r.s.rules_land_on(), Some(stages[1]));
    // A member may not change them.
    assert_eq!(
        r.by(
            ana,
            Command::ChangeStageRules {
                from: stages[1],
                rules: hard.clone(),
            }
        ),
        Err(Rejection::Unauthorized {
            capability: Capability::EditNextRules
        })
    );
    // Without a base: refused, as every rules edit.
    assert_eq!(
        r.based(
            host,
            None,
            Command::ChangeStageRules {
                from: stages[1],
                rules: hard.clone(),
            }
        ),
        Err(Rejection::MissingBase)
    );
    let events = r
        .by(
            host,
            Command::ChangeStageRules {
                from: stages[1],
                rules: hard.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        events,
        vec![Event::StageRulesChanged {
            from: stages[1],
            rules: hard.clone()
        }]
    );
    let m = r.s.current.as_ref().unwrap();
    let mut began = MatchRules::hosted();
    began.stages = 4;
    assert_eq!(m.rules, began, "the match's own rules stay");
    assert_eq!(m.rules_for(s1).difficulty, MatchDifficulty::Normal);
    for &st in &stages[1..] {
        assert_eq!(m.rules_for(st).play_rules(), hard);
        assert!(m.stage_rules_changed(st));
    }
    assert!(!m.stage_rules_changed(s1));
    // The same again changes nothing.
    assert_eq!(
        r.by(
            host,
            Command::ChangeStageRules {
                from: stages[1],
                rules: hard.clone(),
            }
        ),
        Ok(vec![])
    );
    r.ready(host);
    r.ready(ana);
    let s1 = r.play();
    r.end(s1, &[&[ana], &[host]]);
    // Stage 2 picked and played under Hard timing and Boos.
    r.pick(2).unwrap();
    assert_eq!(r.upcoming(), stages[1]);
    assert_eq!(r.s.rules_land_on(), Some(stages[2]));
    r.ready(host);
    r.ready(ana);
    let s2 = r.play();
    // During stage 2's play: Attacks off from stage 3.
    let no_attacks = PlayRules {
        battle: BattlePreset::GaugeSurvivalV0.id().into(),
        ..hard.clone()
    };
    let mut on = hard.clone();
    on.battle = BattlePreset::GaugeAttacksV0.id().into();
    r.by(
        host,
        Command::ChangeStageRules {
            from: stages[2],
            rules: on.clone(),
        },
    )
    .unwrap();
    r.end(s2, &[&[host], &[ana]]);
    // In the intermission the stage after still lands on stage 3: a
    // second change from it replaces the first.
    assert_eq!(r.s.rules_land_on(), Some(stages[2]));
    r.by(
        host,
        Command::ChangeStageRules {
            from: stages[2],
            rules: no_attacks.clone(),
        },
    )
    .unwrap();
    let m = r.s.current.as_ref().unwrap();
    assert_eq!(m.changes.len(), 2, "{:?}", m.changes);
    assert_eq!(
        m.rules_for(stages[1]).play_rules(),
        hard,
        "stage 2 played as it was"
    );
    assert!(!m.rules_for(stages[2]).attacks());
    assert!(!m.rules_for(stages[3]).attacks());
    // Back to the match's own from stage 4: not marked.
    r.pick(3).unwrap();
    r.by(
        host,
        Command::ChangeStageRules {
            from: stages[3],
            rules: MatchRules::hosted().play_rules(),
        },
    )
    .unwrap();
    let m = r.s.current.as_ref().unwrap();
    assert!(!m.stage_rules_changed(stages[3]));
    assert!(m.stage_rules_changed(stages[2]));
    // On the last stage's countdown no stage is left.
    r.ready(host);
    r.ready(ana);
    let s3 = r.play();
    r.end(s3, &[&[host], &[ana]]);
    r.pick(4).unwrap();
    assert_eq!(r.s.rules_land_on(), None);
    assert_eq!(
        r.by(
            host,
            Command::ChangeStageRules {
                from: stages[3],
                rules: hard.clone(),
            }
        ),
        Err(Rejection::RulesLandLater {
            asked: stages[3],
            from: None
        })
    );
    // The record keeps each stage's rules.
    r.ready(host);
    r.ready(ana);
    let s4 = r.play();
    r.end(s4, &[&[host], &[ana]]);
    r.pick(5).unwrap();
    let played = &r.s.history.last().unwrap().played;
    assert_eq!(played.changes.len(), 3);
    assert_eq!(
        played.rules_for(stages[1]).difficulty,
        MatchDifficulty::Hard
    );
    // A new match begins with no changes.
    assert!(r.s.current.as_ref().unwrap().changes.is_empty());
}

/// A change racing a pick (or a start) is ordered by the room's
/// revision: whichever came first holds, and the change made for a stage
/// that has begun since is refused, naming where it would land now,
/// never moved silently; one made against changes saved since is stale.
#[test]
fn a_per_stage_change_racing_a_pick_or_a_countdown_lands_a_stage_later_or_is_refused() {
    let mut r = Room::hosted(3);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let co = r.join("co", Intent::Play);
    r.by(
        host,
        Command::SetCoHost {
            participant: co,
            on: true,
        },
    )
    .unwrap();
    r.pick(1).unwrap();
    let stages: Vec<StageId> =
        r.s.current
            .as_ref()
            .unwrap()
            .stages
            .iter()
            .map(|s| s.id)
            .collect();
    for p in [host, ana, co] {
        r.ready(p);
    }
    // During stage 1's countdown a change lands on stage 2.
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    assert_eq!(r.s.rules_land_on(), Some(stages[1]));
    let s1 = r.play();
    r.end(s1, &[&[ana], &[host], &[co]]);
    // In the intermission nobody readied for stage 2: it lands there.
    assert_eq!(r.s.rules_land_on(), Some(stages[1]));
    let seen = r.s.revision;
    // The pick comes first.
    r.pick(2).unwrap();
    let hard = PlayRules {
        difficulty: MatchDifficulty::Hard,
        ..MatchRules::hosted().play_rules()
    };
    assert_eq!(
        r.based(
            co,
            Some(seen),
            Command::ChangeStageRules {
                from: stages[1],
                rules: hard.clone(),
            }
        ),
        Err(Rejection::RulesLandLater {
            asked: stages[1],
            from: Some(stages[2])
        })
    );
    assert!(r.s.current.as_ref().unwrap().changes.is_empty());
    // Said again for stage 3, it holds.
    r.by(
        co,
        Command::ChangeStageRules {
            from: stages[2],
            rules: hard.clone(),
        },
    )
    .unwrap();
    // The host's own made against the view before it: stale.
    let easy = PlayRules {
        difficulty: MatchDifficulty::Easy,
        ..hard.clone()
    };
    assert!(matches!(
        r.based(
            host,
            Some(seen),
            Command::ChangeStageRules {
                from: stages[2],
                rules: easy.clone(),
            }
        ),
        Err(Rejection::Stale { .. })
    ));
    // Someone loading for a stage ahead of its pick holds it too.
    let m = r.s.current.as_ref().unwrap();
    assert_eq!(m.rules_for(stages[1]).difficulty, MatchDifficulty::Normal);
    assert_eq!(m.rules_for(stages[2]).difficulty, MatchDifficulty::Hard);
    // Adaptive only where the authority honors it.
    let mut old = r.s.clone();
    old.policy.adaptive = false;
    let stepping = PlayRules {
        charts: Charts::Adaptive,
        ..hard
    };
    let revision = old.revision;
    assert_eq!(
        old.apply(
            Actor::Member(host),
            Some(revision),
            &Command::ChangeStageRules {
                from: stages[2],
                rules: stepping,
            },
            Millis(r.now)
        ),
        Err(Rejection::InvalidRules(RulesProblem::AdaptiveNotHonored))
    );
}

/// The changes' JSON is additive: a match and a policy stored before them
/// read back as having none, a match without changes writes none, and a
/// change's event and command round-trip.
#[test]
fn per_stage_changes_are_additive_to_the_rooms_json() {
    let mut r = Room::hosted(2);
    r.join("ana", Intent::Play);
    r.pick(1).unwrap();
    let json = serde_json::to_value(&r.s).unwrap();
    assert!(
        json["current"].get("changes").is_none(),
        "{}",
        json["current"]
    );
    let mut policy = serde_json::to_value(&r.s.policy).unwrap();
    assert_eq!(policy["stage_rules"], true);
    policy.as_object_mut().unwrap().remove("stage_rules");
    let old: RoomPolicy = serde_json::from_value(policy).unwrap();
    assert!(!old.stage_rules, "an older authority's policy offers none");
    let from = r.s.rules_land_on().unwrap();
    let rules = PlayRules {
        boos: true,
        ..MatchRules::hosted().play_rules()
    };
    let cmd = Command::ChangeStageRules {
        from,
        rules: rules.clone(),
    };
    let back: Command = serde_json::from_str(&serde_json::to_string(&cmd).unwrap()).unwrap();
    assert_eq!(back, cmd);
    r.by(r.host, cmd).unwrap();
    let text = serde_json::to_string(&r.s).unwrap();
    let back: RoomState = serde_json::from_str(&text).unwrap();
    assert_eq!(back, r.s);
    // The match as an older reader stored it: no changes.
    let mut v = serde_json::to_value(&r.s).unwrap();
    v["current"].as_object_mut().unwrap().remove("changes");
    let older: RoomState = serde_json::from_value(v).unwrap();
    assert!(older.current.unwrap().changes.is_empty());
}

/// Points changeable from a later stage (the maintainer, 2026-11-13): in a
/// field of nine, Mild from stage 3 of four. Stages 1 and 2 are awarded on
/// Strong's table, 3 and 4 on Mild's; the totals sum the two (place 6
/// earns 3 on Strong and 4 on Mild at nine). A bias equal to the match's
/// is kept as none, and a change that names none keeps the match's.
#[test]
fn points_changed_from_stage_3_award_stages_3_and_4_on_their_own_curve() {
    let mut r = Room::hosted(4);
    let host = r.host;
    let mut field = vec![host];
    for i in 0..8 {
        field.push(r.join(&format!("p{i}"), Intent::Play));
    }
    let order: Vec<&[ParticipantId]> = field.iter().map(std::slice::from_ref).collect();
    let stage = |r: &mut Room, n: u32| -> StageId {
        r.pick(n).unwrap();
        for &who in &field {
            r.ready(who);
        }
        let s = r.play();
        r.end(s, &order);
        s
    };
    let s1 = stage(&mut r, 1);
    let s2 = stage(&mut r, 2);
    // In the intermission after stage 2: the change lands on stage 3,
    // which nobody has begun.
    let from = r.s.rules_land_on().expect("stage 3 still ahead");
    let m = r.s.current.as_ref().unwrap();
    assert_eq!(m.stage_index(from), Some(2));
    let mild = PlayRules {
        points: Some(PointsBias::Mild),
        ..MatchRules::hosted().play_rules()
    };
    let events = r
        .by(
            host,
            Command::ChangeStageRules {
                from,
                rules: mild.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        events,
        vec![Event::StageRulesChanged {
            from,
            rules: mild.clone()
        }]
    );
    let m = r.s.current.as_ref().unwrap();
    assert_eq!(m.rules.points, PointsBias::Strong, "the match's own stays");
    assert_eq!(m.rules_for(s2).points, PointsBias::Strong);
    assert_eq!(m.rules_for(from).points, PointsBias::Mild);
    assert!(m.stage_rules_changed(from));
    // Naming the bias the stage already has changes nothing.
    assert_eq!(
        r.by(
            host,
            Command::ChangeStageRules {
                from,
                rules: mild.clone(),
            }
        ),
        Ok(vec![])
    );
    let s3 = stage(&mut r, 3);
    let s4 = stage(&mut r, 4);
    assert_eq!(s3, from);
    let m = r.s.current.as_ref().unwrap();
    let rule = |st: StageId| {
        m.stage(st)
            .unwrap()
            .play
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .rule
    };
    assert_eq!(rule(s1), AwardRule::TopBiasedStrongV1);
    assert_eq!(rule(s2), AwardRule::TopBiasedStrongV1);
    assert_eq!(rule(s3), AwardRule::TopBiasedMildV1);
    assert_eq!(rule(s4), AwardRule::TopBiasedMildV1);
    let strong = TopBiased::awards(PointsBias::Strong, 9).unwrap();
    let mild_table = TopBiased::awards(PointsBias::Mild, 9).unwrap();
    for (place, &who) in field.iter().enumerate() {
        let expected = 2 * strong[place] + 2 * mild_table[place];
        assert_eq!(r.total(who), Points::whole(expected), "place {}", place + 1);
    }
    // Place 6 is where the two curves differ at nine.
    assert_eq!((strong[5], mild_table[5]), (3, 4));
    assert_eq!(r.total(field[5]), Points::whole(14));
}

/// A change's points bias is additive both ways: none is left out of the
/// JSON (a change without it reads as the match's), the bias equal to the
/// match's is kept as none, and an authority's policy from before it
/// (`stage_points` missing) refuses a bias and is never sent one.
#[test]
fn a_changes_points_bias_is_additive_and_honored_only_where_the_policy_says() {
    let plain = MatchRules::hosted().play_rules();
    let json = serde_json::to_value(&plain).unwrap();
    assert!(json.get("points").is_none(), "{json}");
    let mut mild = plain.clone();
    mild.points = Some(PointsBias::Mild);
    let json = serde_json::to_value(&mild).unwrap();
    assert_eq!(json["points"], "mild");
    assert_eq!(serde_json::from_value::<PlayRules>(json).unwrap(), mild);
    let mut older = serde_json::to_value(&mild).unwrap();
    older.as_object_mut().unwrap().remove("points");
    assert_eq!(serde_json::from_value::<PlayRules>(older).unwrap(), plain);

    let mut r = Room::hosted(3);
    let host = r.host;
    r.join("ana", Intent::Play);
    r.pick(1).unwrap();
    let from = r.s.rules_land_on().unwrap();
    // The match's own bias named: the same as none, so nothing changes.
    let mut same = plain.clone();
    same.points = Some(PointsBias::Strong);
    assert_eq!(
        r.by(host, Command::ChangeStageRules { from, rules: same }),
        Ok(vec![])
    );
    // With Boos too: kept without the bias.
    let mut boos = plain.clone();
    boos.boos = true;
    boos.points = Some(PointsBias::Strong);
    let events = r
        .by(host, Command::ChangeStageRules { from, rules: boos })
        .unwrap();
    let Event::StageRulesChanged { rules, .. } = &events[0] else {
        panic!("{events:?}")
    };
    assert_eq!(rules.points, None);
    // A policy from before it reads false and refuses a bias.
    let mut policy = serde_json::to_value(&r.s.policy).unwrap();
    assert_eq!(policy["stage_points"], true);
    policy.as_object_mut().unwrap().remove("stage_points");
    let old_policy: RoomPolicy = serde_json::from_value(policy).unwrap();
    assert!(!old_policy.stage_points);
    let mut old = r.s.clone();
    old.policy = old_policy;
    let revision = old.revision;
    assert_eq!(
        old.apply(
            Actor::Member(host),
            Some(revision),
            &Command::ChangeStageRules { from, rules: mild },
            Millis(r.now)
        ),
        Err(Rejection::InvalidRules(RulesProblem::PointsNotHonored))
    );
}

/// A stage counts as begun only once its song is picked (follow-up round
/// F, 2026-11-14: "There's a difference between the song being selected
/// and it being picked"): a change of rules during a running match lands
/// on stage 1 after the picker backed out of its prompt (what was loaded
/// and readied for it left as it was) and while a song merely rests under
/// the spotlight, shared and loaded ahead; once picked it lands on the
/// stage after, as before.
#[test]
fn rules_land_on_a_stage_selected_but_not_picked() {
    let rules = {
        let mut r = MatchRules::hosted();
        r.stages = 3;
        r
    };
    let mut r = Room::new(rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join("ana", Intent::Play);
    let hard = PlayRules {
        difficulty: MatchDifficulty::Hard,
        ..MatchRules::hosted().play_rules()
    };
    // Picked: the match runs, stage 1's window is open; its share comes,
    // ana loads it and readies.
    r.pick(1).unwrap();
    let stages: Vec<StageId> =
        r.s.current
            .as_ref()
            .unwrap()
            .stages
            .iter()
            .map(|s| s.id)
            .collect();
    r.sys(Command::SetContentStatus {
        stage: stages[0],
        status: ContentStatus::Available,
        share: Some("share-1".into()),
    })
    .unwrap();
    r.ready(ana);
    assert_eq!(r.s.rules_land_on(), Some(stages[1]), "picked: the stage after");
    // The picker backs out of the prompt: the window closes; ana's load
    // and Ready stay recorded against stage 1.
    r.by(host, Command::CancelStart { stage: stages[0] })
        .unwrap();
    assert!(matches!(r.s.phase, Phase::Preparing { window: None, .. }));
    let prep = r.s.stage(stages[0]).unwrap().prep(ana);
    assert_eq!(prep.loading, Loading::Loaded);
    assert_eq!(r.s.rules_land_on(), Some(stages[0]), "backed out: stage 1");
    // A song resting under the spotlight, shared ahead: still stage 1.
    r.by(
        host,
        Command::Spotlight {
            content: Some(song(2)),
            card: Some(Box::new(card(2))),
        },
    )
    .unwrap();
    r.sys(Command::SetSpotlightShare {
        chart: song(2).chart,
        share: "share-2".into(),
    })
    .unwrap();
    assert_eq!(r.s.rules_land_on(), Some(stages[0]), "selected: stage 1");
    r.by(
        host,
        Command::ChangeStageRules {
            from: stages[0],
            rules: hard.clone(),
        },
    )
    .unwrap();
    let m = r.s.current.as_ref().unwrap();
    assert_eq!(m.rules_for(stages[0]).play_rules(), hard);
    // Picked again: stage 1 plays under them, and a change now lands on
    // stage 2.
    r.pick(2).unwrap();
    assert_eq!(r.upcoming(), stages[0]);
    assert_eq!(r.s.rules_land_on(), Some(stages[1]));
}
