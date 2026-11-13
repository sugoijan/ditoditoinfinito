//! The reducer: commands decided against the state into events, events
//! applied to the state. Deterministic (ordered maps, ids from counters,
//! time passed in) and atomic (a refused command changes nothing), so the
//! local practice authority and the server run the same transitions and a
//! client mirror applying the events holds the same state.

use crate::awards::{Finish, Finisher, StageResult};
use crate::battle::{BattlePreset, Entrant, ReportedRun, Run, RunDetail};
use crate::command::{Actor, CancelReason, Command, Event, RankingProblem, Rejection};
use crate::ids::{Millis, ParticipantId, Revision, StageId};
use crate::member::{Capability, Connection, Intent, Member, MemberKind, Role, can};
use crate::phase::{Phase, ReadinessWindow};
use crate::rules::{Access, Admission, ContentSource};
use crate::state::{
    Closure, ContentCard, ContentStatus, HostLeaves, Hosting, Match, MatchEnding, MatchRecord,
    RoomState, Seat, SitOutReason, Spotlight, Stage, StageContent, StagePlay,
};
use std::collections::{BTreeMap, BTreeSet};

type Decided = Result<(), Rejection>;

impl RoomState {
    /// Decide `command` from `actor`, made against the client's view at
    /// `base`, at authority time `now`. On success the state is at the next
    /// revision and the events say what changed (none: nothing changed, and
    /// the revision stays). On refusal nothing changes.
    pub fn apply(
        &mut self,
        actor: Actor,
        base: Option<Revision>,
        command: &Command,
        now: Millis,
    ) -> Result<Vec<Event>, Rejection> {
        if self.closed.is_some() {
            return Err(Rejection::RoomClosed);
        }
        let mut work = self.clone();
        let mut out = Vec::new();
        work.decide(actor, base, command, now, &mut out)?;
        work.settle_host(&mut out);
        work.maybe_commit(now, &mut out);
        // A room from before the host's succession says its setting from
        // the first change this reducer makes there.
        if !out.is_empty() && work.hosting.is_none() && work.closed.is_none() {
            let policy = work.if_host_leaves();
            work.emit(&mut out, Event::HostLeavesChanged { policy });
        }
        if !out.is_empty() {
            work.revision = work.revision.next();
            *self = work;
        }
        Ok(out)
    }

    /// Run what time decides: a readiness window's deadline, a committed
    /// countdown's start.
    pub fn advance(&mut self, now: Millis) -> Vec<Event> {
        if self.closed.is_some() {
            return Vec::new();
        }
        let mut work = self.clone();
        let mut out = Vec::new();
        work.tick(now, &mut out);
        if !out.is_empty() {
            work.revision = work.revision.next();
            *self = work;
        }
        out
    }

    /// Apply one revision's events (a mirror's way of following the
    /// authority).
    pub fn commit(&mut self, events: &[Event]) {
        for e in events {
            self.evolve(e);
        }
        self.revision = self.revision.next();
    }

    fn emit(&mut self, out: &mut Vec<Event>, event: Event) {
        self.evolve(&event);
        out.push(event);
    }

    // ---- permission helpers ----

    fn require(&self, actor: Actor, capability: Capability) -> Decided {
        match actor {
            Actor::Authority => Ok(()),
            Actor::Member(id) => {
                let m = self.present_human(id)?;
                if can(m.role, m.admin, capability, self.phase.kind()) {
                    Ok(())
                } else {
                    Err(Rejection::Unauthorized { capability })
                }
            }
        }
    }

    /// Whether `actor` chooses the room's songs now: the picker
    /// (`Picking`), whatever their role; another operator is told who has
    /// it. In a room from before the picker, every operator, as it was.
    fn require_picker(&self, actor: Actor) -> Decided {
        let Actor::Member(id) = actor else {
            return Ok(());
        };
        let m = self.present_human(id)?;
        let Some(picking) = &self.picking else {
            return self.require(actor, Capability::ChooseContent);
        };
        if picking.picker == Some(id) {
            Ok(())
        } else if can(
            m.role,
            m.admin,
            Capability::ChooseContent,
            self.phase.kind(),
        ) {
            Err(Rejection::NotPicker {
                picker: picking.picker,
            })
        } else {
            Err(Rejection::Unauthorized {
                capability: Capability::ChooseContent,
            })
        }
    }

    /// `from` can hold the pick no longer (they left the room, lost the
    /// role that lets one take it, or were disconnected past the
    /// authority's patience): it is the owner's again while the owner is
    /// in the room and not disconnected (the new owner's, where the one
    /// gone was the owner: call this after [`Self::settle_host`]), else
    /// nobody's until the owner is back or an operator takes it.
    fn release_pick(&mut self, from: ParticipantId, out: &mut Vec<Event>) {
        if self.closed.is_some() || self.picking.as_ref().and_then(|p| p.picker) != Some(from) {
            return;
        }
        let picker = self
            .owner()
            .filter(|o| o.id != from && !o.left && o.connection != Connection::Disconnected)
            .map(|o| o.id);
        self.emit(out, Event::PickerChanged { picker });
    }

    /// The host's succession ([`Hosting`]), from the room's facts alone,
    /// so it holds whatever led to them: a host who left the room, or who
    /// is away past the authority's patience (`Disconnected`), no longer
    /// has it. By the room's setting it ends ([`HostLeaves::Close`]) or
    /// goes to [`RoomState::heir`]: at once for one who left (anyone
    /// present, else the room ends), and for one who is away to someone
    /// connected, as soon as there is one (the room keeps its absent host
    /// meanwhile: nobody is there to run it for). Then a pick nobody
    /// holds is the host's while the host is there.
    fn settle_host(&mut self, out: &mut Vec<Event>) {
        if self.closed.is_some() {
            return;
        }
        if let Some(owner) = self.owner().filter(|o| !o.is_cpu()) {
            let (from, left) = (owner.id, owner.left);
            if left || owner.connection == Connection::Disconnected {
                let heir = match self.if_host_leaves() {
                    HostLeaves::Close => None,
                    HostLeaves::PassOn => self.heir(from, left),
                };
                match heir {
                    Some(to) => self.hand_over(from, to, out),
                    None if left || self.if_host_leaves() == HostLeaves::Close => {
                        self.emit(
                            out,
                            Event::RoomClosed {
                                reason: Closure::HostLeft,
                            },
                        );
                        return;
                    }
                    None => {}
                }
            }
        }
        if self.picking.as_ref().is_some_and(|p| p.picker.is_none())
            && let Some(owner) = self.owner().filter(|o| {
                !o.left && !o.is_cpu() && o.connection != Connection::Disconnected
            })
        {
            let picker = Some(owner.id);
            self.emit(out, Event::PickerChanged { picker });
        }
    }

    /// The room changes hands: `from` a co-host, `to` its owner.
    fn hand_over(&mut self, from: ParticipantId, to: ParticipantId, out: &mut Vec<Event>) {
        self.emit(
            out,
            Event::RoleChanged {
                participant: from,
                role: Role::CoHost,
            },
        );
        self.emit(
            out,
            Event::RoleChanged {
                participant: to,
                role: Role::Owner,
            },
        );
    }

    fn present_human(&self, id: ParticipantId) -> Result<&Member, Rejection> {
        let m = self.members.get(&id).ok_or(Rejection::NotAMember)?;
        if m.left {
            Err(Rejection::NotPresent)
        } else if m.is_cpu() {
            Err(Rejection::Unauthorized {
                capability: Capability::Participate,
            })
        } else {
            Ok(m)
        }
    }

    fn own(&self, actor: Actor) -> Result<ParticipantId, Rejection> {
        match actor {
            Actor::Authority => Err(Rejection::NotAMember),
            Actor::Member(id) => self.present_human(id).map(|m| m.id),
        }
    }

    fn authority_only(actor: Actor) -> Decided {
        match actor {
            Actor::Authority => Ok(()),
            Actor::Member(_) => Err(Rejection::AuthorityOnly),
        }
    }

    fn check_base(base: Option<Revision>, changed_at: Revision) -> Decided {
        let base = base.ok_or(Rejection::MissingBase)?;
        if changed_at > base {
            Err(Rejection::Stale { base, changed_at })
        } else {
            Ok(())
        }
    }

    fn wrong_phase(&self) -> Rejection {
        Rejection::WrongPhase {
            phase: self.phase.kind(),
        }
    }

    fn fresh_stage(&mut self) -> StageId {
        let id = StageId(self.counters.stage);
        self.counters.stage += 1;
        id
    }

    // ---- decisions ----

    fn decide(
        &mut self,
        actor: Actor,
        base: Option<Revision>,
        command: &Command,
        now: Millis,
        out: &mut Vec<Event>,
    ) -> Decided {
        match command {
            Command::SetIntent { intent } => {
                let id = self.own(actor)?;
                if self.members[&id].intent == *intent {
                    return Ok(());
                }
                let m = &self.members[&id];
                if *intent == Intent::Play && m.watch_only && m.role != Role::Owner {
                    return Err(Rejection::WatchOnly);
                }
                if *intent == Intent::Play {
                    self.policy
                        .check_competitors(self.reserved_humans() + 1)
                        .map_err(Rejection::Cap)?;
                }
                self.emit(
                    out,
                    Event::IntentChanged {
                        participant: id,
                        intent: *intent,
                    },
                );
                Ok(())
            }
            Command::SetTargeting { targeting } => {
                let id = self.own(actor)?;
                // A stage in play carries its own changes (its reports).
                if matches!(self.phase, Phase::Stage { .. } | Phase::SongCompletion { .. }) {
                    return Err(self.wrong_phase());
                }
                if self.members[&id].targeting != *targeting {
                    self.emit(
                        out,
                        Event::TargetingChanged {
                            participant: id,
                            targeting: *targeting,
                        },
                    );
                }
                Ok(())
            }
            Command::SetReady {
                stage,
                readiness_rev,
                ready,
            } => {
                let id = self.own(actor)?;
                if self.members[&id].intent != Intent::Play {
                    return Err(Rejection::NotPlaying);
                }
                self.check_preparing(id, *stage, *readiness_rev)?;
                let s = self.stage(*stage).ok_or(Rejection::UnknownStage)?;
                if s.content.is_none() {
                    return Err(Rejection::NoContent);
                }
                if s.prep(id).ready != *ready {
                    self.emit(
                        out,
                        Event::ReadyChanged {
                            stage: *stage,
                            participant: id,
                            ready: *ready,
                        },
                    );
                }
                Ok(())
            }
            Command::ReportLoading {
                stage,
                readiness_rev,
                loading,
            } => {
                let id = self.own(actor)?;
                let s = self.stage(*stage).ok_or(Rejection::UnknownStage)?;
                if s.play.is_some() {
                    return Err(Rejection::StageStarted);
                }
                if s.readiness_rev != *readiness_rev {
                    return Err(Rejection::StaleReadiness {
                        current: s.readiness_rev,
                    });
                }
                if s.content.is_none() {
                    return Err(Rejection::NoContent);
                }
                if s.prep(id).loading != *loading {
                    self.emit(
                        out,
                        Event::LoadingChanged {
                            stage: *stage,
                            participant: id,
                            loading: loading.clone(),
                        },
                    );
                }
                Ok(())
            }
            Command::Leave => {
                let id = self.own(actor)?;
                // Cancel first, so the departure also clears the readiness
                // the canceled start held.
                self.cancel_if_committed(id, out);
                self.emit(out, Event::MemberLeft { participant: id });
                // A host's leaving passes the room on (or ends it) before
                // the pick they held goes to its owner: the new one.
                self.settle_host(out);
                self.release_pick(id, out);
                if self.closed.is_none() && self.spotlight.as_ref().is_some_and(|s| s.by == id) {
                    self.emit(out, Event::SpotlightChanged { spotlight: None });
                }
                Ok(())
            }
            Command::ReportOutcome { stage, run, detail } => {
                let id = self.own(actor)?;
                // After a result confirmed before the song's end (a live
                // room's, decided as the stage went), a report is taken
                // for the standings' detail alone, and the song ends once
                // everyone reported.
                let confirmed = match self.phase {
                    Phase::Stage { stage: s } if s == *stage => false,
                    Phase::SongCompletion { stage: s } if s == *stage => true,
                    _ => return Err(self.wrong_phase()),
                };
                let play = self
                    .stage(*stage)
                    .and_then(|s| s.play.as_ref())
                    .ok_or(Rejection::UnknownStage)?;
                if !play.lineup.contains(&id) {
                    return Err(Rejection::NotInLineup);
                }
                if confirmed && (!play.self_reported(&self.members) || play.song_zero_at.is_none())
                {
                    return Err(Rejection::ResultAlreadyConfirmed);
                }
                // A stage with CPUs is ranked by the authority that judges
                // them (practice), never from reports.
                if !play.self_reported(&self.members) {
                    return Err(Rejection::AuthorityOnly);
                }
                match play.reports.get(&id) {
                    Some(r) if r == run => return Ok(()),
                    Some(_) => return Err(Rejection::AlreadyReported),
                    None => {}
                }
                self.emit(
                    out,
                    Event::OutcomeReported {
                        stage: *stage,
                        participant: id,
                        run: *run,
                        at: now,
                        detail: detail.clone().map(RunDetail::bounded),
                    },
                );
                let all = self
                    .stage(*stage)
                    .and_then(|s| s.play.as_ref())
                    .is_some_and(|p| p.lineup.iter().all(|id| p.reports.contains_key(id)));
                if all && confirmed {
                    self.end_song(*stage, out);
                } else if all {
                    self.settle(*stage, out);
                }
                Ok(())
            }
            Command::ChooseContent { stage, content } => {
                self.require_picker(actor)?;
                let s = self.stage(*stage).ok_or(Rejection::UnknownStage)?;
                Self::check_base(base, s.content_changed_at)?;
                if s.play.is_some() {
                    return Err(match self.phase {
                        Phase::Countdown { stage: c, .. } if c == *stage => {
                            Rejection::StartCommitted
                        }
                        _ => Rejection::StageStarted,
                    });
                }
                if s.content.as_ref().map(|c| &c.id) == content.as_ref() {
                    return Ok(());
                }
                let status = match self.source {
                    ContentSource::LocalLibrary => ContentStatus::Available,
                    ContentSource::Shared => ContentStatus::Pending,
                };
                if let Phase::Preparing {
                    stage: p,
                    window: Some(_),
                } = self.phase
                    && p == *stage
                {
                    self.emit(
                        out,
                        Event::StartCanceled {
                            stage: p,
                            reason: CancelReason::ContentReplaced,
                        },
                    );
                    self.emit(
                        out,
                        Event::PhaseChanged {
                            phase: Phase::Preparing {
                                stage: p,
                                window: None,
                            },
                        },
                    );
                }
                self.emit(
                    out,
                    Event::ContentChosen {
                        stage: *stage,
                        content: content.clone().map(|id| StageContent {
                            id,
                            status,
                            share: None,
                            card: None,
                            battle: None,
                        }),
                    },
                );
                Ok(())
            }
            Command::SaveNextRules { rules } => {
                self.require(actor, Capability::EditNextRules)?;
                self.save_next_rules(base, rules, out)
            }
            Command::RestartMatch { rules } => {
                // Both checked before anything is decided: refused whole.
                self.require(actor, Capability::EditNextRules)?;
                self.require(actor, Capability::StopMatch)?;
                self.save_next_rules(base, rules, out)?;
                self.interrupt(crate::phase::Interruption::Stopped, out);
                Ok(())
            }
            Command::StartMatch => {
                self.require(actor, Capability::Advance)?;
                if self.phase != Phase::Lobby {
                    return Err(self.wrong_phase());
                }
                Self::check_base(base, self.next.saved_at)?;
                self.start_match(out)
            }
            Command::PrepareNextStage => {
                self.require(actor, Capability::Advance)?;
                let Phase::Intermission { next } = self.phase else {
                    return Err(self.wrong_phase());
                };
                self.prepare(next, out);
                Ok(())
            }
            Command::StartStage {
                stage,
                readiness_rev,
            } => {
                self.require(actor, Capability::Advance)?;
                match self.phase {
                    // The match and its first stage, as one change.
                    Phase::Lobby if self.playlist.first().is_some_and(|s| s.id == *stage) => {
                        Self::check_base(base, self.next.saved_at)?;
                        self.start_match(out)?;
                    }
                    Phase::Intermission { next } if next == *stage => self.prepare(next, out),
                    Phase::Preparing { stage: p, window } if p == *stage => {
                        if window.is_some() {
                            return Err(Rejection::WindowOpen);
                        }
                    }
                    Phase::Countdown { stage: c, .. } if c == *stage => {
                        return Err(Rejection::StartCommitted);
                    }
                    Phase::Preparing { .. } | Phase::Intermission { .. } => {
                        return Err(Rejection::NotUpcoming);
                    }
                    _ => return Err(self.wrong_phase()),
                }
                self.start_stage(*stage, *readiness_rev, now, out)
            }
            Command::CancelStart { stage } => {
                self.require(actor, Capability::Advance)?;
                match self.phase {
                    Phase::Preparing {
                        stage: p,
                        window: Some(_),
                    }
                    | Phase::Countdown { stage: p, .. }
                        if p == *stage =>
                    {
                        self.cancel_start(p, CancelReason::ByOperator, out);
                        Ok(())
                    }
                    _ => Err(Rejection::NothingToCancel),
                }
            }
            Command::StopMatch => {
                self.require(actor, Capability::StopMatch)?;
                self.interrupt(crate::phase::Interruption::Stopped, out);
                Ok(())
            }
            Command::OpenNextMatch => {
                self.require(actor, Capability::Advance)?;
                self.open_next_match(out)
            }
            Command::Pick { content, card } => {
                // The picker's; starting (or going on with) the match is
                // still an operator's, which a picker is today. A rule
                // that passes the pick around would let the picker's pick
                // advance here.
                self.require_picker(actor)?;
                self.require(actor, Capability::Advance)?;
                if card.as_ref().is_some_and(|c| !c.fits()) {
                    return Err(Rejection::InvalidCard);
                }
                self.pick(base, content, card.as_deref(), now, out)
            }
            Command::TakePick => {
                self.require(actor, Capability::TakePick)?;
                let id = self.own(actor)?;
                // In a room from before the picker this turns one picker
                // at a time on.
                if self.picking.as_ref().is_none_or(|p| p.picker != Some(id)) {
                    self.emit(out, Event::PickerChanged { picker: Some(id) });
                }
                Ok(())
            }
            Command::Spotlight { content, card } => {
                self.require_picker(actor)?;
                let by = self.own(actor)?;
                if card.as_ref().is_some_and(|c| !c.fits()) {
                    return Err(Rejection::InvalidCard);
                }
                let spotlight = content.as_ref().map(|content| Spotlight {
                    content: content.clone(),
                    card: card.as_deref().cloned().unwrap_or_default(),
                    by,
                    share: self.known_share(content),
                });
                if spotlight != self.spotlight {
                    self.emit(out, Event::SpotlightChanged { spotlight });
                }
                Ok(())
            }
            Command::SetSpotlightShare { chart, share } => {
                Self::authority_only(actor)?;
                if let Some(s) = &self.spotlight
                    && s.content.chart == *chart
                    && s.share.as_deref() != Some(share.as_str())
                {
                    let spotlight = Some(Spotlight {
                        share: Some(share.clone()),
                        ..s.clone()
                    });
                    self.emit(out, Event::SpotlightChanged { spotlight });
                }
                // A pick of the same content waiting for its share takes it.
                let waiting: Vec<StageId> = self
                    .current
                    .iter()
                    .flat_map(|m| m.stages.iter())
                    .chain(self.playlist.iter())
                    .filter(|st| st.play.is_none())
                    .filter(|st| {
                        st.content.as_ref().is_some_and(|c| {
                            c.id.chart == *chart && c.status == ContentStatus::Pending
                        })
                    })
                    .map(|st| st.id)
                    .collect();
                for stage in waiting {
                    self.content_status(
                        stage,
                        ContentStatus::Available,
                        Some(share.clone()),
                        now,
                        out,
                    );
                }
                Ok(())
            }
            Command::SetCoHost { participant, on } => {
                self.require(actor, Capability::ManageCoHosts)?;
                let m = self
                    .members
                    .get(participant)
                    .ok_or(Rejection::UnknownParticipant)?;
                if m.is_cpu() {
                    return Err(Rejection::NotAHuman);
                }
                if m.left {
                    return Err(Rejection::NotPresent);
                }
                if m.role == Role::Owner {
                    return Err(Rejection::OwnerRole);
                }
                // A bot follows a room; it never operates one.
                if *on && m.is_bot() {
                    return Err(Rejection::NotAHuman);
                }
                let role = if *on { Role::CoHost } else { Role::Member };
                let operates = can(role, m.admin, Capability::TakePick, self.phase.kind());
                if m.role != role {
                    self.emit(
                        out,
                        Event::RoleChanged {
                            participant: *participant,
                            role,
                        },
                    );
                    // The pick is not kept without the role to take it.
                    if !operates {
                        self.release_pick(*participant, out);
                    }
                }
                Ok(())
            }
            Command::SetWatchOnly { participant, on } => {
                // The authority holds a newcomer to it; the owner only
                // lets them play.
                if *on {
                    Self::authority_only(actor)?;
                } else {
                    self.require(actor, Capability::ChangeAccess)?;
                }
                let m = self
                    .members
                    .get(participant)
                    .ok_or(Rejection::UnknownParticipant)?;
                if m.is_cpu() {
                    return Err(Rejection::NotAHuman);
                }
                if m.watch_only != *on {
                    self.emit(
                        out,
                        Event::WatchOnlyChanged {
                            participant: *participant,
                            on: *on,
                        },
                    );
                }
                Ok(())
            }
            Command::SetAccess {
                admission,
                rotate_invite,
            } => {
                self.require(actor, Capability::ChangeAccess)?;
                let access = Access {
                    admission: *admission,
                    invite_generation: self.access.invite_generation + u32::from(*rotate_invite),
                };
                if access != self.access {
                    self.emit(out, Event::AccessChanged { access });
                }
                Ok(())
            }
            Command::SetHostLeaves { policy } => {
                self.require(actor, Capability::ChangeAccess)?;
                if self.hosting.as_ref().map(|h| h.if_host_leaves) != Some(*policy) {
                    self.emit(out, Event::HostLeavesChanged { policy: *policy });
                }
                Ok(())
            }
            Command::MakeHost { participant } => {
                self.require(actor, Capability::MakeHost)?;
                let from = self.own(actor)?;
                let m = self
                    .members
                    .get(participant)
                    .ok_or(Rejection::UnknownParticipant)?;
                if m.is_cpu() || m.is_bot() {
                    return Err(Rejection::NotAHuman);
                }
                // Someone who is there to run it: a room handed to one
                // who is away would only pass on again.
                if m.left || m.connection != Connection::Connected {
                    return Err(Rejection::NotPresent);
                }
                if *participant != from {
                    self.hand_over(from, *participant, out);
                }
                Ok(())
            }
            Command::Join {
                identity,
                name,
                admin,
                intent,
            } => {
                Self::authority_only(actor)?;
                // A bot is never an administrator, whatever it is told.
                let admin = &(*admin && !identity.is_bot());
                if let Some(m) = self.member_by_identity(identity) {
                    // The same person back: the same row, whatever the
                    // admission now says.
                    if m.left || m.connection != Connection::Connected {
                        let participant = m.id;
                        self.emit(out, Event::MemberRestored { participant });
                    }
                    return Ok(());
                }
                if self.access.admission == Admission::Closed && !admin {
                    return Err(Rejection::AdmissionClosed);
                }
                let present = self
                    .members
                    .values()
                    .filter(|m| !m.left && !m.is_cpu())
                    .count();
                let limit = usize::from(self.policy.max_members)
                    + if *admin {
                        usize::from(self.policy.admin_reserve)
                    } else {
                        0
                    };
                if present >= limit {
                    return Err(Rejection::RoomFull);
                }
                // Every player's place taken: in to watch, not refused.
                let intent = &self.arrival_intent(*intent);
                let id = ParticipantId(self.counters.participant);
                self.emit(
                    out,
                    Event::MemberJoined {
                        member: Member {
                            id,
                            kind: MemberKind::Human {
                                identity: identity.clone(),
                                name: name.clone(),
                            },
                            role: Role::Member,
                            admin: *admin,
                            intent: *intent,
                            connection: Connection::Connected,
                            left: false,
                            targeting: Default::default(),
                            watch_only: false,
                        },
                    },
                );
                Ok(())
            }
            Command::SetConnection {
                participant,
                connection,
            } => {
                Self::authority_only(actor)?;
                let m = self
                    .members
                    .get(participant)
                    .ok_or(Rejection::UnknownParticipant)?;
                if m.is_cpu() {
                    return Err(Rejection::NotAHuman);
                }
                if m.connection == *connection {
                    return Ok(());
                }
                if *connection == Connection::Disconnected {
                    self.cancel_if_committed(*participant, out);
                }
                self.emit(
                    out,
                    Event::ConnectionChanged {
                        participant: *participant,
                        connection: *connection,
                    },
                );
                // Gone past the authority's patience (Reconnecting keeps
                // everything): the room does not wait for its host where
                // someone is there to run it, nor the pick for its holder.
                if *connection == Connection::Disconnected {
                    self.settle_host(out);
                    self.release_pick(*participant, out);
                }
                Ok(())
            }
            Command::SetContentStatus {
                stage,
                status,
                share,
            } => {
                Self::authority_only(actor)?;
                let s = self.stage(*stage).ok_or(Rejection::UnknownStage)?;
                let current = s.content.as_ref().ok_or(Rejection::NoContent)?;
                if current.status == *status && current.share == *share {
                    return Ok(());
                }
                if s.play.is_some()
                    && !matches!(self.phase, Phase::Countdown { stage: c, .. } if c == *stage)
                {
                    return Err(Rejection::StageStarted);
                }
                self.content_status(*stage, status.clone(), share.clone(), now, out);
                Ok(())
            }
            Command::SetBattlePlan { stage, digest } => {
                Self::authority_only(actor)?;
                let s = self.stage(*stage).ok_or(Rejection::UnknownStage)?;
                if s.play.is_some() {
                    return Err(Rejection::StageStarted);
                }
                let content = s.content.as_ref().ok_or(Rejection::NoContent)?;
                if content.battle.as_ref() == Some(digest) {
                    return Ok(());
                }
                self.emit(
                    out,
                    Event::BattlePlanSet {
                        stage: *stage,
                        digest: digest.clone(),
                    },
                );
                Ok(())
            }
            Command::ConfirmStage { stage, ranking } => {
                Self::authority_only(actor)?;
                match self.phase {
                    Phase::Stage { stage: s } if s == *stage => {}
                    Phase::SongCompletion { stage: s } if s == *stage => {
                        return Err(Rejection::ResultAlreadyConfirmed);
                    }
                    _ => return Err(self.wrong_phase()),
                }
                self.confirm(*stage, ranking, out)?;
                self.emit(
                    out,
                    Event::PhaseChanged {
                        phase: Phase::SongCompletion { stage: *stage },
                    },
                );
                Ok(())
            }
            Command::EndSong { stage, ranking } => {
                Self::authority_only(actor)?;
                match (&self.phase, ranking) {
                    (Phase::Stage { stage: s }, Some(ranking)) if s == stage => {
                        self.confirm(*stage, ranking, out)?;
                    }
                    (Phase::Stage { stage: s }, None) if s == stage => {
                        return Err(Rejection::ResultMissing);
                    }
                    (Phase::SongCompletion { stage: s }, None) if s == stage => {}
                    (Phase::SongCompletion { stage: s }, Some(_)) if s == stage => {
                        return Err(Rejection::ResultAlreadyConfirmed);
                    }
                    _ => return Err(self.wrong_phase()),
                }
                self.end_song(*stage, out);
                Ok(())
            }
            Command::Interrupt { reason } => {
                Self::authority_only(actor)?;
                if !self.phase.kind().match_running() {
                    return Err(self.wrong_phase());
                }
                self.interrupt(*reason, out);
                Ok(())
            }
        }
    }

    /// Stage `stage`'s content becomes `status` (with `share`). Unusable,
    /// its window or countdown is canceled; available, its window's
    /// deadline counts from now (the song reached everyone just now).
    fn content_status(
        &mut self,
        stage: StageId,
        status: ContentStatus,
        share: Option<String>,
        now: Millis,
        out: &mut Vec<Event>,
    ) {
        self.emit(
            out,
            Event::ContentStatusChanged {
                stage,
                status: status.clone(),
                share,
            },
        );
        match (&status, self.phase.clone()) {
            (
                ContentStatus::Unavailable { .. },
                Phase::Countdown { stage: p, .. }
                | Phase::Preparing {
                    stage: p,
                    window: Some(_),
                },
            ) if p == stage => self.cancel_start(p, CancelReason::ContentUnavailable, out),
            (
                ContentStatus::Available,
                Phase::Preparing {
                    stage: p,
                    window: Some(w),
                },
            ) if p == stage => self.emit(
                out,
                Event::PhaseChanged {
                    phase: Phase::Preparing {
                        stage: p,
                        window: Some(ReadinessWindow {
                            deadline: now.after(self.policy.readiness.window_ms),
                            ..w
                        }),
                    },
                },
            ),
            _ => {}
        }
    }

    /// Close the finished or interrupted match and open the next one's
    /// admission in the lobby, its saved draft checked again.
    fn open_next_match(&mut self, out: &mut Vec<Event>) -> Decided {
        let ending = match self.phase {
            Phase::MatchResults => MatchEnding::Completed,
            Phase::Interrupted { reason } => MatchEnding::Interrupted { reason },
            _ => return Err(self.wrong_phase()),
        };
        self.emit(out, Event::MatchClosed { ending });
        // The boundary: the saved draft is checked against who wants to
        // play now; a problem keeps it pending and blocks Start.
        let review = self
            .next
            .rules
            .validate(&self.policy, self.reserved_humans())
            .err();
        if review != self.next.review {
            self.emit(out, Event::NextRulesReviewed { review });
        }
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Lobby,
            },
        );
        Ok(())
    }

    /// [`Command::Pick`]: the stage the room plays next gets `content`, the
    /// match starts or goes on, and the stage's readiness window opens.
    fn pick(
        &mut self,
        base: Option<Revision>,
        content: &crate::ids::ContentId,
        card: Option<&ContentCard>,
        now: Millis,
        out: &mut Vec<Event>,
    ) -> Decided {
        let stage = match self.phase.clone() {
            Phase::Lobby => {
                Self::check_base(base, self.next.saved_at)?;
                let first = self.playlist.first().map(|s| s.id);
                self.start_match(out)?;
                first.ok_or(Rejection::UnknownStage)?
            }
            Phase::MatchResults | Phase::Interrupted { .. } => {
                Self::check_base(base, self.next.saved_at)?;
                self.open_next_match(out)?;
                let first = self.playlist.first().map(|s| s.id);
                self.start_match(out)?;
                first.ok_or(Rejection::UnknownStage)?
            }
            Phase::Intermission { next } => {
                self.prepare(next, out);
                next
            }
            Phase::Preparing {
                stage,
                window: None,
            } => stage,
            Phase::Preparing {
                window: Some(_), ..
            } => return Err(Rejection::WindowOpen),
            Phase::Countdown { .. } => return Err(Rejection::StartCommitted),
            _ => return Err(self.wrong_phase()),
        };
        // A field that could never start is refused before anything locks.
        let candidates = self.candidates();
        let humans = candidates
            .iter()
            .filter(|id| self.members.get(id).is_some_and(|m| !m.is_cpu()))
            .count();
        let p = &self.policy.readiness;
        if humans < self.humans_needed() || candidates.len() < usize::from(p.min_competitors) {
            return Err(Rejection::NotEnoughReady {
                ready_humans: u16::try_from(humans).unwrap_or(u16::MAX),
                competitors: u16::try_from(candidates.len()).unwrap_or(u16::MAX),
            });
        }
        let s = self.stage(stage).ok_or(Rejection::UnknownStage)?;
        if s.play.is_some() {
            return Err(Rejection::StageStarted);
        }
        if s.content.as_ref().map(|c| &c.id) != Some(content) {
            let (status, share) = match self.source {
                ContentSource::LocalLibrary => (ContentStatus::Available, None),
                ContentSource::Shared => match self.known_share(content) {
                    Some(share) => (ContentStatus::Available, Some(share)),
                    None => (ContentStatus::Pending, None),
                },
            };
            self.emit(
                out,
                Event::ContentChosen {
                    stage,
                    content: Some(StageContent {
                        id: content.clone(),
                        status,
                        share,
                        card: card.cloned(),
                        battle: None,
                    }),
                },
            );
        }
        let readiness_rev = self.stage(stage).map_or(0, |s| s.readiness_rev);
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Preparing {
                    stage,
                    window: Some(ReadinessWindow {
                        deadline: now.after(self.policy.readiness.window_ms),
                        readiness_rev,
                    }),
                },
            },
        );
        Ok(())
    }

    /// A readiness window ends early once everyone connected who could
    /// still join is ready and loaded (`hosted-deadline-v1`): the start
    /// commits at once. A member whose connection is down does not hold
    /// it; the deadline still waits for them.
    fn maybe_commit(&mut self, now: Millis, out: &mut Vec<Event>) {
        if self.closed.is_some() {
            return;
        }
        let Phase::Preparing {
            stage,
            window: Some(_),
        } = self.phase
        else {
            return;
        };
        let Some(s) = self.stage(stage) else { return };
        if self.content_ready(s).is_err() {
            return;
        }
        let (lineup, sat_out) = self.lineup(s);
        let waiting = sat_out
            .iter()
            .any(|(_, r)| r.may_still_join() && *r != SitOutReason::NotConnected);
        if waiting || !self.enough(&lineup).0 {
            return;
        }
        self.commit_start(stage, lineup, sat_out, now, out);
    }

    /// May `id` prepare for `stage` now?
    fn check_preparing(&self, id: ParticipantId, stage: StageId, readiness_rev: u32) -> Decided {
        if self.upcoming_stage() != Some(stage) {
            return Err(match self.phase {
                Phase::Countdown { stage: c, .. } if c == stage => Rejection::StartCommitted,
                _ if self.stage(stage).is_none() => Rejection::UnknownStage,
                _ => Rejection::NotUpcoming,
            });
        }
        if let Some(m) = self.running_match() {
            let seat = m.roster.get(&id).ok_or(Rejection::NotInMatch)?;
            if seat.out_for_match {
                return Err(Rejection::OutForMatch);
            }
        }
        let s = self.stage(stage).ok_or(Rejection::UnknownStage)?;
        if s.readiness_rev != readiness_rev {
            return Err(Rejection::StaleReadiness {
                current: s.readiness_rev,
            });
        }
        Ok(())
    }

    fn start_match(&mut self, out: &mut Vec<Event>) -> Decided {
        let rules = self.next.rules.clone();
        rules
            .validate(&self.policy, self.reserved_humans())
            .map_err(Rejection::RulesNeedReview)?;
        let humans: Vec<ParticipantId> = self
            .members
            .values()
            .filter(|m| !m.left && !m.is_cpu() && m.intent == Intent::Play)
            .map(|m| m.id)
            .collect();
        // Nobody wants to play: a match of the CPUs alone, watched
        // (`watch-v0`: CPU practice), or nothing to start.
        if humans.is_empty() && rules.cpu_seats(0) == 0 {
            return Err(Rejection::NoPlayingHumans);
        }
        let cpus = (1..=rules.cpu_seats(humans.len()))
            .map(|n| {
                let ordinal = u16::try_from(n).unwrap_or(u16::MAX);
                let id = ParticipantId(self.counters.participant + n as u64 - 1);
                Member {
                    id,
                    kind: MemberKind::Cpu {
                        ordinal,
                        opponent: rules.cpu_for_seat(ordinal),
                    },
                    role: Role::Member,
                    admin: false,
                    intent: Intent::Play,
                    connection: Connection::Connected,
                    left: false,
                    targeting: Default::default(),
                    watch_only: false,
                }
            })
            .collect();
        let playlist = (0..rules.stages).map(|_| self.fresh_stage()).collect();
        let id = crate::ids::MatchId(self.counters.matches);
        let first = self.playlist[0].id;
        self.emit(
            out,
            Event::MatchStarted {
                id,
                rules,
                humans,
                cpus,
                playlist,
            },
        );
        if self.next.review.is_some() {
            self.emit(out, Event::NextRulesReviewed { review: None });
        }
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Preparing {
                    stage: first,
                    window: None,
                },
            },
        );
        Ok(())
    }

    /// Enter `stage`'s preparation, admitting late entrants where the rules
    /// allow (into vacant seats only, never a CPU's).
    fn prepare(&mut self, stage: StageId, out: &mut Vec<Event>) {
        if let Some(m) = &self.current
            && m.rules.mid_match_entry
        {
            let vacant = usize::from(m.rules.seats).saturating_sub(m.roster.len());
            let waiting: Vec<ParticipantId> = self
                .members
                .values()
                .filter(|p| {
                    !p.left
                        && !p.is_cpu()
                        && p.intent == Intent::Play
                        && !m.roster.contains_key(&p.id)
                })
                .map(|p| p.id)
                .take(vacant)
                .collect();
            if !waiting.is_empty() {
                self.emit(
                    out,
                    Event::LateEntrantsAdmitted {
                        stage,
                        participants: waiting,
                    },
                );
            }
        }
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Preparing {
                    stage,
                    window: None,
                },
            },
        );
    }

    fn content_ready(&self, stage: &Stage) -> Decided {
        match &stage.content {
            None => Err(Rejection::NoContent),
            Some(c) => match c.status {
                ContentStatus::Pending => Err(Rejection::ContentPending),
                ContentStatus::Unavailable { .. } => Err(Rejection::ContentUnavailable),
                ContentStatus::Available => Ok(()),
            },
        }
    }

    /// Who would start `stage` now, and who would sit out and why.
    fn lineup(&self, stage: &Stage) -> (Vec<ParticipantId>, Vec<(ParticipantId, SitOutReason)>) {
        let mut lineup = Vec::new();
        let mut sat_out = Vec::new();
        for id in self.candidates() {
            match self.sit_out_reason(stage, id) {
                None => lineup.push(id),
                Some(reason) => sat_out.push((id, reason)),
            }
        }
        (lineup, sat_out)
    }

    fn enough(&self, lineup: &[ParticipantId]) -> (bool, u16, u16) {
        let humans = lineup
            .iter()
            .filter(|id| !self.members[id].is_cpu())
            .count();
        let p = &self.policy.readiness;
        let needed = self.humans_needed();
        let all = u16::try_from(lineup.len()).unwrap_or(u16::MAX);
        (
            humans >= needed && all >= p.min_competitors,
            u16::try_from(humans).unwrap_or(u16::MAX),
            all,
        )
    }

    /// The ready people a start needs (`ReadinessPolicy::min_ready_humans`),
    /// never more than the people of the match who want to play
    /// (`watch-v0`): where every person watches, the CPUs start alone.
    fn humans_needed(&self) -> usize {
        let wanting = self
            .candidates()
            .iter()
            .filter(|id| {
                self.members
                    .get(id)
                    .is_some_and(|m| !m.is_cpu() && !m.left && m.intent == Intent::Play)
            })
            .count();
        usize::from(self.policy.readiness.min_ready_humans).min(wanting)
    }

    fn start_stage(
        &mut self,
        stage: StageId,
        readiness_rev: u32,
        now: Millis,
        out: &mut Vec<Event>,
    ) -> Decided {
        let s = self.stage(stage).ok_or(Rejection::UnknownStage)?;
        if s.readiness_rev != readiness_rev {
            return Err(Rejection::StaleReadiness {
                current: s.readiness_rev,
            });
        }
        self.content_ready(s)?;
        let (lineup, sat_out) = self.lineup(s);
        let (enough, ready_humans, competitors) = self.enough(&lineup);
        if !enough {
            return Err(Rejection::NotEnoughReady {
                ready_humans,
                competitors,
            });
        }
        // Wait only for those who could still become eligible; a departed
        // or watching competitor is recorded as sitting out, not waited for.
        if !sat_out.iter().any(|(_, reason)| reason.may_still_join()) {
            self.commit_start(stage, lineup, sat_out, now, out);
        } else {
            let deadline = now.after(self.policy.readiness.window_ms);
            self.emit(
                out,
                Event::PhaseChanged {
                    phase: Phase::Preparing {
                        stage,
                        window: Some(ReadinessWindow {
                            deadline,
                            readiness_rev,
                        }),
                    },
                },
            );
        }
        Ok(())
    }

    fn commit_start(
        &mut self,
        stage: StageId,
        lineup: Vec<ParticipantId>,
        sat_out: Vec<(ParticipantId, SitOutReason)>,
        now: Millis,
        out: &mut Vec<Event>,
    ) {
        let start_at = now.after(self.policy.readiness.countdown_ms);
        let song_zero_at = self
            .policy
            .live
            .as_ref()
            .map(|l| start_at.after(l.prelude_ms));
        self.emit(
            out,
            Event::StartCommitted {
                stage,
                lineup,
                sat_out,
                start_at,
                song_zero_at,
            },
        );
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Countdown { stage, start_at },
            },
        );
    }

    fn cancel_start(&mut self, stage: StageId, reason: CancelReason, out: &mut Vec<Event>) {
        self.emit(out, Event::StartCanceled { stage, reason });
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Preparing {
                    stage,
                    window: None,
                },
            },
        );
    }

    /// A committed starter leaving or disconnecting before the start
    /// invalidates the lineup.
    fn cancel_if_committed(&mut self, id: ParticipantId, out: &mut Vec<Event>) {
        if let Phase::Countdown { stage, .. } = self.phase
            && self
                .stage(stage)
                .and_then(|s| s.play.as_ref())
                .is_some_and(|p| p.lineup.contains(&id))
        {
            self.cancel_start(stage, CancelReason::RosterChanged, out);
        }
    }

    fn confirm(
        &mut self,
        stage: StageId,
        ranking: &[Vec<Finisher>],
        out: &mut Vec<Event>,
    ) -> Decided {
        let (lineup, scheme) = {
            let m = self.current.as_ref().ok_or(Rejection::UnknownStage)?;
            let s = m.stage(stage).ok_or(Rejection::UnknownStage)?;
            let play = s.play.as_ref().ok_or(Rejection::UnknownStage)?;
            (play.lineup.clone(), (m.rules.awards, m.rules.points))
        };
        let mut seen = BTreeSet::new();
        for group in ranking {
            if group.is_empty() {
                return Err(Rejection::InvalidRanking(RankingProblem::EmptyGroup));
            }
            for f in group {
                if !lineup.contains(&f.participant) {
                    return Err(Rejection::InvalidRanking(RankingProblem::NotInLineup(
                        f.participant,
                    )));
                }
                if !seen.insert(f.participant) {
                    return Err(Rejection::InvalidRanking(RankingProblem::Duplicate(
                        f.participant,
                    )));
                }
            }
        }
        if let Some(missing) = lineup.iter().find(|id| !seen.contains(id)) {
            return Err(Rejection::InvalidRanking(RankingProblem::Missing(*missing)));
        }
        let rule = scheme
            .0
            .rule_for(lineup.len(), scheme.1)
            .map_err(Rejection::NoAwardRule)?;
        let result = StageResult::award(rule, ranking).ok_or(Rejection::NoAwardRule(
            crate::awards::NoAwardRule {
                field: u16::try_from(lineup.len()).unwrap_or(u16::MAX),
            },
        ))?;
        self.emit(out, Event::StageConfirmed { stage, result });
        Ok(())
    }

    /// A self-reported stage's result from its starters' reports (those
    /// who never reported are forfeits), ranked by the match's battle as
    /// practice ranks it (`battle::rank`), then the song's end.
    fn settle(&mut self, stage: StageId, out: &mut Vec<Event>) {
        let Some((entrants, preset)) = self.current.as_ref().and_then(|m| {
            let play = m.stage(stage)?.play.as_ref()?;
            let entrants: Vec<Entrant> = play
                .lineup
                .iter()
                .map(|id| Entrant {
                    participant: *id,
                    run: play
                        .reports
                        .get(id)
                        .map_or(Run::Forfeit, |r: &ReportedRun| r.run()),
                })
                .collect();
            Some((entrants, BattlePreset::parse(&m.rules.battle)))
        }) else {
            return;
        };
        // Both presets rank alike; an unknown id ranks as survival does
        // (`BattlePreset::GaugeSurvivalV0`).
        let ranking = match preset.unwrap_or(BattlePreset::GaugeSurvivalV0) {
            BattlePreset::GaugeSurvivalV0 | BattlePreset::GaugeAttacksV0 => {
                crate::battle::rank(&entrants)
            }
        };
        if self.confirm(stage, &ranking, out).is_ok() {
            self.end_song(stage, out);
        }
    }

    fn end_song(&mut self, stage: StageId, out: &mut Vec<Event>) {
        self.emit(out, Event::SongEnded { stage });
        let Some(m) = &self.current else { return };
        if !m.rules.return_each_stage {
            let out_now: Vec<ParticipantId> = m
                .stage(stage)
                .and_then(|s| s.play.as_ref())
                .and_then(|p| p.result.as_ref())
                .map(|r| {
                    r.placements
                        .iter()
                        .filter(|p| matches!(p.finish, Finish::Eliminated | Finish::Forfeit))
                        .map(|p| p.participant)
                        .collect()
                })
                .unwrap_or_default();
            if !out_now.is_empty() {
                self.emit(
                    out,
                    Event::OutForMatch {
                        participants: out_now,
                    },
                );
            }
        }
        let next = self
            .current
            .as_ref()
            .and_then(|m| m.stage_index(stage).and_then(|i| m.stages.get(i + 1)))
            .map(|s| s.id);
        let phase = match next {
            Some(next) => Phase::Intermission { next },
            None => Phase::MatchResults,
        };
        self.emit(out, Event::PhaseChanged { phase });
    }

    /// `SaveNextRules` (and the first half of `RestartMatch`), its
    /// capability checked: the base, the rules validated, then the draft
    /// saved with the playlist's new stages.
    fn save_next_rules(
        &mut self,
        base: Option<Revision>,
        rules: &crate::rules::MatchRules,
        out: &mut Vec<Event>,
    ) -> Decided {
        Self::check_base(base, self.next.saved_at)?;
        rules
            .validate(&self.policy, self.reserved_humans())
            .map_err(Rejection::InvalidRules)?;
        let have = self.playlist.len();
        let added = (have..usize::from(rules.stages))
            .map(|_| self.fresh_stage())
            .collect();
        if self.current.is_none() {
            self.emit(out, Event::PlaylistReadinessCleared);
        }
        self.emit(
            out,
            Event::NextRulesSaved {
                rules: rules.clone(),
                added,
            },
        );
        if self.next.review.is_some() {
            self.emit(out, Event::NextRulesReviewed { review: None });
        }
        Ok(())
    }

    fn interrupt(&mut self, reason: crate::phase::Interruption, out: &mut Vec<Event>) {
        self.emit(out, Event::MatchInterrupted { reason });
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Interrupted { reason },
            },
        );
    }

    fn tick(&mut self, now: Millis, out: &mut Vec<Event>) {
        match self.phase.clone() {
            Phase::Preparing {
                stage,
                window: Some(w),
            } if now >= w.deadline => {
                let Some(s) = self.stage(stage) else { return };
                // A song still being shared: the window waits for it (its
                // deadline starts again once it is available).
                if self.content_pending(stage) {
                    return;
                }
                if self.content_ready(s).is_err() {
                    self.cancel_start(stage, CancelReason::ContentUnavailable, out);
                    return;
                }
                let (lineup, sat_out) = self.lineup(s);
                if self.enough(&lineup).0 {
                    self.commit_start(stage, lineup, sat_out, now, out);
                } else {
                    self.cancel_start(stage, CancelReason::BelowMinimum, out);
                }
            }
            Phase::Stage { stage } if self.report_deadline(stage).is_some_and(|at| now >= at) => {
                self.settle(stage, out);
            }
            Phase::SongCompletion { stage }
                if self.report_deadline(stage).is_some_and(|at| now >= at) =>
            {
                self.end_song(stage, out);
            }
            Phase::Countdown { stage, start_at } if now >= start_at => {
                self.emit(
                    out,
                    Event::PhaseChanged {
                        phase: Phase::Stage { stage },
                    },
                );
                // Persistent elimination: missing a stage ends the match.
                let missed: Vec<ParticipantId> = self
                    .current
                    .as_ref()
                    .filter(|m| !m.rules.return_each_stage)
                    .and_then(|m| m.stage(stage))
                    .and_then(|s| s.play.as_ref())
                    .map(|p| p.sat_out.keys().copied().collect())
                    .unwrap_or_default();
                if !missed.is_empty() {
                    self.emit(
                        out,
                        Event::OutForMatch {
                            participants: missed,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    // ---- events applied ----

    pub(crate) fn evolve(&mut self, event: &Event) {
        let pending = self.revision.next();
        match event {
            Event::MemberJoined { member } => {
                self.counters.participant = self.counters.participant.max(member.id.0 + 1);
                self.members.insert(member.id, member.clone());
            }
            Event::MemberRestored { participant } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.left = false;
                    m.connection = Connection::Connected;
                }
            }
            Event::MemberLeft { participant } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.left = true;
                    m.connection = Connection::Disconnected;
                }
                self.drop_prep(*participant, true);
            }
            Event::ConnectionChanged {
                participant,
                connection,
            } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.connection = *connection;
                }
                if *connection == Connection::Disconnected {
                    self.drop_prep(*participant, true);
                }
            }
            Event::IntentChanged {
                participant,
                intent,
            } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.intent = *intent;
                }
                if *intent == Intent::Watch {
                    self.drop_prep(*participant, false);
                }
            }
            Event::TargetingChanged {
                participant,
                targeting,
            } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.targeting = *targeting;
                }
            }
            Event::RoleChanged { participant, role } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.role = *role;
                }
            }
            Event::WatchOnlyChanged { participant, on } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.watch_only = *on;
                }
            }
            Event::AccessChanged { access } => self.access = *access,
            Event::HostLeavesChanged { policy } => {
                self.hosting = Some(Hosting {
                    if_host_leaves: *policy,
                });
            }
            Event::RoomClosed { reason } => self.closed = Some(*reason),
            Event::SpotlightChanged { spotlight } => self.spotlight = spotlight.clone(),
            Event::PickerChanged { picker } => {
                let mut picking = self.picking.take().unwrap_or_default();
                // The song the last picker rested on goes with them; the
                // room's share of it stays known by its chart.
                if let Some(s) = self.spotlight.take()
                    && let Some(share) = s.share
                {
                    picking.keep_share(s.content.chart, share);
                }
                picking.picker = *picker;
                self.picking = Some(picking);
            }
            Event::ContentChosen { stage, content } => {
                if let Some(s) = self.stage_mut(*stage) {
                    s.content = content.clone();
                    s.content_changed_at = pending;
                    s.readiness_rev += 1;
                    s.prep.clear();
                }
            }
            Event::ContentStatusChanged {
                stage,
                status,
                share,
            } => {
                if let Some(c) = self.stage_mut(*stage).and_then(|s| s.content.as_mut()) {
                    c.status = status.clone();
                    c.share = share.clone();
                }
            }
            Event::OutcomeReported {
                stage,
                participant,
                run,
                at,
                detail,
            } => {
                if let Some(p) = self.stage_mut(*stage).and_then(|s| s.play.as_mut()) {
                    p.reports.insert(*participant, *run);
                    if let Some(d) = detail {
                        p.details.insert(*participant, d.clone());
                    }
                    if run.finished_song() && p.song_end_at.is_none() {
                        p.song_end_at = Some(*at);
                    }
                }
            }
            Event::BattlePlanSet { stage, digest } => {
                if let Some(c) = self.stage_mut(*stage).and_then(|s| s.content.as_mut()) {
                    c.battle = Some(digest.clone());
                }
            }
            Event::PlaylistReadinessCleared => {
                for s in &mut self.playlist {
                    s.readiness_rev += 1;
                    s.prep.clear();
                }
            }
            Event::ReadyChanged {
                stage,
                participant,
                ready,
            } => {
                if let Some(s) = self.stage_mut(*stage) {
                    s.prep.entry(*participant).or_default().ready = *ready;
                }
            }
            Event::LoadingChanged {
                stage,
                participant,
                loading,
            } => {
                if let Some(s) = self.stage_mut(*stage) {
                    s.prep.entry(*participant).or_default().loading = loading.clone();
                }
            }
            Event::NextRulesSaved { rules, added } => {
                self.next.rules = rules.clone();
                self.next.saved_at = pending;
                self.playlist.truncate(usize::from(rules.stages));
                for id in added {
                    self.counters.stage = self.counters.stage.max(id.0 + 1);
                    self.playlist.push(Stage::new(*id));
                }
            }
            Event::NextRulesReviewed { review } => self.next.review = *review,
            Event::MatchStarted {
                id,
                rules,
                humans,
                cpus,
                playlist,
            } => {
                let stages = std::mem::take(&mut self.playlist);
                let first = stages.first().map_or(StageId(0), |s| s.id);
                let mut roster = BTreeMap::new();
                for p in humans.iter().chain(cpus.iter().map(|c| &c.id)) {
                    roster.insert(
                        *p,
                        Seat {
                            first_stage: first,
                            out_for_match: false,
                        },
                    );
                }
                for cpu in cpus {
                    self.counters.participant = self.counters.participant.max(cpu.id.0 + 1);
                    self.members.insert(cpu.id, cpu.clone());
                }
                for s in playlist {
                    self.counters.stage = self.counters.stage.max(s.0 + 1);
                    self.playlist.push(Stage::new(*s));
                }
                self.counters.matches = self.counters.matches.max(id.0 + 1);
                // The draft now names the match after this one; an edit made
                // before the start is stale and never reaches these rules.
                self.next.saved_at = pending;
                self.current = Some(Match {
                    id: *id,
                    rules: rules.clone(),
                    roster,
                    stages,
                });
            }
            Event::LateEntrantsAdmitted {
                stage,
                participants,
            } => {
                if let Some(m) = &mut self.current {
                    for p in participants {
                        m.roster.insert(
                            *p,
                            Seat {
                                first_stage: *stage,
                                out_for_match: false,
                            },
                        );
                    }
                }
            }
            Event::PhaseChanged { phase } => self.phase = phase.clone(),
            Event::StartCommitted {
                stage,
                lineup,
                sat_out,
                start_at,
                song_zero_at,
            } => {
                if let Some(s) = self.stage_mut(*stage) {
                    s.play = Some(StagePlay {
                        lineup: lineup.iter().copied().collect(),
                        sat_out: sat_out.iter().copied().collect(),
                        start_at: *start_at,
                        result: None,
                        song_ended: false,
                        interrupted: false,
                        reports: BTreeMap::new(),
                        song_end_at: None,
                        details: BTreeMap::new(),
                        song_zero_at: *song_zero_at,
                    });
                }
            }
            Event::StartCanceled { stage, .. } => {
                if let Some(s) = self.stage_mut(*stage)
                    && s.play.as_ref().is_some_and(|p| p.result.is_none())
                {
                    s.play = None;
                }
            }
            Event::StageConfirmed { stage, result } => {
                if let Some(p) = self.stage_mut(*stage).and_then(|s| s.play.as_mut()) {
                    p.result = Some(result.clone());
                }
            }
            Event::SongEnded { stage } => {
                if let Some(p) = self.stage_mut(*stage).and_then(|s| s.play.as_mut()) {
                    p.song_ended = true;
                }
            }
            Event::OutForMatch { participants } => {
                if let Some(m) = &mut self.current {
                    for p in participants {
                        if let Some(seat) = m.roster.get_mut(p) {
                            seat.out_for_match = true;
                        }
                    }
                }
            }
            Event::MatchInterrupted { .. } => {
                if let Some(m) = &mut self.current {
                    for p in m.stages.iter_mut().filter_map(|s| s.play.as_mut()) {
                        if p.result.is_none() && !p.song_ended {
                            p.interrupted = true;
                        }
                    }
                }
            }
            Event::MatchClosed { ending } => {
                if let Some(played) = self.current.take() {
                    let cpus = played
                        .roster
                        .keys()
                        .filter_map(|id| {
                            self.members
                                .get(id)
                                .filter(|m| m.is_cpu())
                                .map(|m| (*id, m.clone()))
                        })
                        .collect::<BTreeMap<_, _>>();
                    for id in cpus.keys() {
                        self.members.remove(id);
                    }
                    self.history.push(MatchRecord {
                        played,
                        cpus,
                        ending: *ending,
                    });
                    let keep = usize::from(self.policy.history);
                    if self.history.len() > keep {
                        let drop = self.history.len() - keep;
                        self.history.drain(..drop);
                    }
                }
            }
        }
    }

    /// Forget `id`'s readiness for every stage not yet started (and their
    /// loading too when `loading`), as a departure or disconnect does.
    fn drop_prep(&mut self, id: ParticipantId, loading: bool) {
        let stages = self
            .current
            .iter_mut()
            .flat_map(|m| m.stages.iter_mut())
            .chain(self.playlist.iter_mut())
            .filter(|s| s.play.is_none());
        for s in stages {
            if loading {
                s.prep.remove(&id);
            } else if let Some(p) = s.prep.get_mut(&id) {
                p.ready = false;
            }
        }
    }
}
