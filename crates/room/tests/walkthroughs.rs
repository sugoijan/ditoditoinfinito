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
    assert_eq!(cara_after_one, Points::whole(3)); // experimental 3-player field
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
    r.by(co, choose(2)).unwrap();
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
    assert_eq!(ana_points, Points::whole(3));

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
    assert_eq!(record.played.standings()[0].total, Points::whole(2));
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
fn the_competitor_cap_is_enforced() {
    let mut r = Room::hosted(1);
    for i in 0..8 {
        r.join(&format!("p{i}"), Intent::Play);
    }
    assert_eq!(
        r.join_as("tenth", Intent::Play, false),
        Err(Rejection::Cap(CapExceeded {
            requested: 10,
            cap: 9
        }))
    );
    let watcher = r.join("tenth", Intent::Watch);
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
    assert_eq!(result.rule, AwardRule::ExperimentalLinearV0);
    assert_eq!(
        result.placement(cpus[1]).unwrap().points,
        Points::ratio(3, 2)
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
    assert_eq!(r.total(me), Points::whole(7));
    assert_eq!(r.total(cpus[0]), Points::whole(7));
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
        },
    )
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
        },
        OpponentSpec {
            level: 4,
            seed: 12,
            personality: None,
            name: Some("Tobi".into()),
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
    many.seats = 10;
    many.cpu.roster = vec![many.cpu.roster[0].clone(); 9];
    assert!(matches!(
        many.validate(&RoomPolicy::default(), 1),
        Err(RulesProblem::OverCap(_))
    ));
}
