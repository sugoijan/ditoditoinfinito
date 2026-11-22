//! The reducer: commands decided against the state into events, events
//! applied to the state. Deterministic (ordered maps, ids from counters,
//! time passed in) and atomic (a refused command changes nothing), so the
//! local practice authority and the server run the same transitions and a
//! client mirror applying the events holds the same state.

use crate::awards::{Finish, Finisher, StageResult};
use crate::battle::{ReportedRun, RunDetail};
use crate::command::{Actor, CancelReason, Command, Event, RankingProblem, Rejection};
use crate::ids::{Millis, ParticipantId, Revision, StageId};
use crate::member::{Capability, Connection, Intent, Member, Role, can};
use crate::phase::{Phase, ReadinessWindow};
use crate::rules::{Access, Admission, ContentSource};
use crate::state::{
    Closure, ContentCard, ContentStatus, HostLeaves, Hosting, Loading, Match, MatchEnding, MatchRecord,
    RoomState, Seat, SitOutReason, Spotlight, Stage, StageContent, StagePlay, Succession,
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
        work.seat_newcomers(&mut out);
        work.maybe_commit(now, &mut out);
        // A choice the room takes from the sender of the change is theirs
        // to know of: said to the others only (round RL4b).
        let by = match actor {
            Actor::Member(id) => Some(id),
            Actor::Authority => None,
        };
        work.settle_choices(now, by, &mut out);
        if !out.is_empty() {
            work.revision = work.revision.next();
            *self = work;
        }
        Ok(out)
    }

    /// Run what time decides: a readiness window's deadline, a committed
    /// countdown's start. In place: `tick` changes the state only through
    /// `emit` and cannot fail halfway, so no copy is needed for atomicity.
    pub fn advance(&mut self, now: Millis) -> Vec<Event> {
        let mut out = Vec::new();
        if self.closed.is_some() {
            return out;
        }
        self.tick(now, &mut out);
        self.settle_choices(now, None, &mut out);
        if !out.is_empty() {
            self.revision = self.revision.next();
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
    /// it.
    fn require_picker(&self, actor: Actor) -> Decided {
        let Actor::Member(id) = actor else {
            return Ok(());
        };
        let m = self.present_human(id)?;
        let picking = &self.picking;
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
        if self.closed.is_some() || self.picking.picker != Some(from) {
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
    /// goes to [`RoomState::succession`]: at once for one who left (anyone
    /// present, else the room ends), and for one who is away to someone
    /// connected, as soon as there is one (the room keeps its absent host
    /// meanwhile: nobody is there to run it for); while a co-host inside
    /// the grace is waited for, nothing changes hands. The host gone keeps
    /// no role: a member from then on (only an explicit
    /// [`crate::Command::MakeHost`] makes the one before a co-host). Then
    /// a pick nobody holds is the host's while the host is there.
    fn settle_host(&mut self, out: &mut Vec<Event>) {
        if self.closed.is_some() {
            return;
        }
        if let Some(owner) = self.owner().filter(|o| !o.is_cpu()) {
            let (from, left) = (owner.id, owner.left);
            if left || owner.connection == Connection::Disconnected {
                let heir = match self.if_host_leaves() {
                    HostLeaves::Close => Succession::Nobody,
                    HostLeaves::PassOn => self.succession(from, left),
                };
                match heir {
                    Succession::To(to) => self.hand_over(from, Role::Member, to, out),
                    // A co-host inside the grace: the room waits for them.
                    Succession::Wait(_) => {}
                    Succession::Nobody if left || self.if_host_leaves() == HostLeaves::Close => {
                        self.emit(
                            out,
                            Event::RoomClosed {
                                reason: Closure::HostLeft,
                            },
                        );
                        return;
                    }
                    Succession::Nobody => {}
                }
            }
        }
        if self.picking.picker.is_none()
            && let Some(owner) = self.owner().filter(|o| {
                o.present_person() && o.connection != Connection::Disconnected
            })
        {
            let picker = Some(owner.id);
            self.emit(out, Event::PickerChanged { picker });
        }
    }

    /// The room changes hands: `from` becomes `was` (a co-host where the
    /// host handed it over, a member where it passed on by itself), `to`
    /// its owner.
    fn hand_over(
        &mut self,
        from: ParticipantId,
        was: Role,
        to: ParticipantId,
        out: &mut Vec<Event>,
    ) {
        self.emit(
            out,
            Event::RoleChanged {
                participant: from,
                role: was,
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

    /// A run reported for `stage`: `id`'s own (`ReportOutcome`), or a
    /// CPU's by the authority for the device that computed it
    /// (`ReportFor`, `for_cpu`).
    #[allow(clippy::too_many_arguments)]
    fn take_report(
        &mut self,
        stage: StageId,
        id: ParticipantId,
        for_cpu: bool,
        run: &ReportedRun,
        detail: &Option<RunDetail>,
        now: Millis,
        out: &mut Vec<Event>,
    ) -> Decided {
        // After a result confirmed before the song's end (a live room's,
        // decided as the stage went), a report is taken for the standings'
        // detail alone, and the song ends once everyone reported.
        let confirmed = match self.phase {
            Phase::Stage { stage: s } if s == stage => false,
            Phase::SongCompletion { stage: s } if s == stage => true,
            _ => return Err(self.wrong_phase()),
        };
        let play = self
            .stage(stage)
            .and_then(|s| s.play.as_ref())
            .ok_or(Rejection::UnknownStage)?;
        if !play.lineup.contains(&id) {
            return Err(Rejection::NotInLineup);
        }
        let is_cpu = self.members.get(&id).is_some_and(Member::is_cpu);
        if for_cpu != is_cpu {
            return Err(Rejection::NotInLineup);
        }
        let reported = play.reported(&self.members);
        if confirmed && (!reported || play.song_zero_at.is_none()) {
            return Err(Rejection::ResultAlreadyConfirmed);
        }
        // A stage with CPUs is ranked by the authority that judges them
        // (practice), never from reports; online, where its authority
        // seats CPUs, by the reports (the CPUs' through it).
        if !reported {
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
                stage,
                participant: id,
                run: *run,
                at: now,
                // Graded as the run ended (revision 16, round AC): a
                // forfeit never keeps the letter of the notes judged
                // before it.
                detail: detail.clone().map(|d| d.bounded().graded_as(run)),
            },
        );
        let all = self
            .stage(stage)
            .and_then(|s| s.play.as_ref())
            .is_some_and(|p| p.lineup.iter().all(|id| p.reports.contains_key(id)));
        if all && confirmed {
            self.end_song(stage, out);
        } else if all {
            self.settle(stage, out);
        }
        Ok(())
    }

    /// CPUs in `rules` where this room's authority cannot seat them.
    fn check_cpus_here(&self, rules: &crate::rules::MatchRules) -> Result<(), crate::rules::RulesProblem> {
        if rules.cpu_seats(self.reserved_humans()) > 0 && !self.policy.seats_cpus(self.source) {
            return Err(crate::rules::RulesProblem::CpusNotHonored);
        }
        Ok(())
    }

    /// Song choice: Group where this room has no song list (revision 22).
    fn check_choice_here(&self, choice: crate::choice::SongChoice) -> Decided {
        if choice == crate::choice::SongChoice::Group && self.source != ContentSource::Shared {
            return Err(Rejection::InvalidRules(crate::rules::RulesProblem::GroupNeedsList));
        }
        Ok(())
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
                    self.match_place_for(id).map_err(Rejection::Cap)?;
                }
                // A committed starter who chooses to watch invalidates the
                // lineup, as one leaving or disconnecting does.
                if *intent == Intent::Watch {
                    self.cancel_if_committed(id, out);
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
                if !*ready
                    && self.policy.readiness.unready_cancels_countdown
                    && let Phase::Countdown { stage: c, .. } = self.phase
                    && c == *stage
                {
                    return self.take_back_countdown(id, *stage, *readiness_rev, now, out);
                }
                // A late entry (round P1b): Ready after the start committed.
                if *ready
                    && self.policy.live.is_some()
                    && let Phase::Countdown { stage: c, .. } | Phase::Stage { stage: c } = self.phase
                    && c == *stage
                {
                    return self.enter_late(id, *stage, *readiness_rev, now, out);
                }
                self.check_preparing(id, *stage, *readiness_rev)?;
                let s = self.stage(*stage).ok_or(Rejection::UnknownStage)?;
                if s.content.is_none() {
                    return Err(Rejection::NoContent);
                }
                if s.prep_ref(id).ready != *ready {
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
                // One counted out of a started stage may still load it to
                // enter late (round P1b).
                if s.play.is_some() && self.late_entry_open(id, *stage, now).is_err() {
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
                // Kept bounded, whatever was sent (round RFa).
                let loading = loading.clone().bounded();
                if s.prep_ref(id).loading != loading {
                    self.emit(
                        out,
                        Event::LoadingChanged {
                            stage: *stage,
                            participant: id,
                            loading,
                        },
                    );
                }
                Ok(())
            }
            Command::SetStart {
                stage,
                readiness_rev,
                chart,
            } => {
                let id = self.own(actor)?;
                let chart = self.start_refusal(id, *stage, *readiness_rev, chart.as_ref(), now)?;
                let s = self.stage(*stage).ok_or(Rejection::UnknownStage)?;
                if s.prep_ref(id).start != chart {
                    self.emit(
                        out,
                        Event::StartChosen {
                            stage: *stage,
                            participant: id,
                            chart,
                        },
                    );
                }
                Ok(())
            }
            Command::Leave => {
                let id = self.own(actor)?;
                self.depart(id, Event::MemberLeft { participant: id }, out);
                Ok(())
            }
            Command::Remove { participant } => {
                let id = *participant;
                let gone = match actor {
                    // A host's Kick, told to everyone.
                    Actor::Member(by) => {
                        self.may_kick(by, id)?;
                        // A co-host kicked loses the role with the kick
                        // (round RFa): back by the invite, a member.
                        if self.members.get(&id).is_some_and(|m| m.role == Role::CoHost) {
                            self.emit(
                                out,
                                Event::RoleChanged {
                                    participant: id,
                                    role: Role::Member,
                                },
                            );
                        }
                        Event::MemberRemoved { participant: id, by }
                    }
                    // The mirror's: to the others, a member leaving.
                    Actor::Authority => {
                        let m = self.members.get(&id).ok_or(Rejection::UnknownParticipant)?;
                        if m.is_cpu() {
                            return Err(Rejection::NotAHuman);
                        }
                        if m.left {
                            return Err(Rejection::NotPresent);
                        }
                        Event::MemberLeft { participant: id }
                    }
                };
                self.depart(id, gone, out);
                Ok(())
            }
            Command::ReportOutcome { stage, run, detail } => {
                let id = self.own(actor)?;
                self.take_report(*stage, id, false, run, detail, now, out)
            }
            Command::ReportFor {
                stage,
                participant,
                run,
                detail,
            } => {
                Self::authority_only(actor)?;
                self.take_report(*stage, *participant, true, run, detail, now, out)
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
            Command::ChangeStageRules { from, rules } => {
                self.require(actor, Capability::EditNextRules)?;
                if !self.phase.kind().match_running() {
                    return Err(self.wrong_phase());
                }
                let m = self.current.as_ref().ok_or_else(|| self.wrong_phase())?;
                Self::check_base(base, m.changes_saved_at())?;
                let lands = self.rules_land_on();
                if lands != Some(*from) {
                    return Err(Rejection::RulesLandLater {
                        asked: *from,
                        from: lands,
                    });
                }
                // The one form a change is kept in: the bias named only
                // where it is not the match's own. A change that names
                // none keeps the stage's as it stands (an earlier change's),
                // never the match's by default.
                // As revision 16 keeps them ([`crate::MatchRules::normalized`]).
                let rules = m.play_rules_of(&m.rules_for(*from).with_play(&rules.clone().normalized()));
                rules.check().map_err(Rejection::InvalidRules)?;
                self.check_choice_here(rules.choice)?;
                if m.play_rules_for(*from).normalized() != rules {
                    self.emit(out, Event::StageRulesChanged { from: *from, rules });
                }
                Ok(())
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
            Command::SetLastStage { stage, last } => {
                self.require(actor, Capability::Advance)?;
                self.set_last_stage(*stage, *last, out)
            }
            Command::EndMatch => {
                self.require(actor, Capability::Advance)?;
                self.end_match_now(out)
            }
            Command::Pick { content, card } => {
                // The picker's; starting (or going on with) the match is
                // still an operator's, which a picker is today. A rule
                // that passes the pick around would let the picker's pick
                // advance here.
                self.require_picker(actor)?;
                self.not_by_group()?;
                self.require(actor, Capability::Advance)?;
                if card.as_ref().is_some_and(|c| !c.fits()) {
                    return Err(Rejection::InvalidCard);
                }
                self.pick(base, content, card.as_deref(), now, out)
            }
            Command::PickListed { song, chart } => {
                self.require_picker(actor)?;
                self.not_by_group()?;
                self.require(actor, Capability::Advance)?;
                if self.source != ContentSource::Shared {
                    return Err(Rejection::NoSongList);
                }
                let entry = self.songs.get(*song).ok_or(Rejection::NotListed)?;
                let line = entry.chart(chart).ok_or(Rejection::ChartNotInSong)?;
                if !entry.available() {
                    return Err(Rejection::SongNotReady);
                }
                let content = crate::ids::ContentId {
                    song: entry.key.clone(),
                    chart: chart.clone(),
                };
                let listed = (entry.id, entry.share.clone(), entry.card_for(line));
                self.pick_content(base, &content, Some(&listed.2), Some((listed.0, listed.1)), now, out)
            }
            Command::AddSong { song } => {
                let id = self.own(actor)?;
                // Who may add, the same song, a person's half and a full
                // list: the one decision the clients ask before sending.
                if let Some(why) = self.add_refusal(id, Some(song)) {
                    return Err(why);
                }
                let rules = self.songs.rules;
                self.make_room(usize::from(rules.size), rules.when_full, out)?;
                let entry = crate::songs::RoomSong {
                    id: crate::ids::RoomSongId(self.songs.next.max(1)),
                    key: song.key.clone(),
                    card: song.card.clone(),
                    charts: song.charts.clone(),
                    by: id,
                    added_at: self.revision.next(),
                    // Uploaded by its adder, a bundled song too (round DB).
                    status: ContentStatus::Pending,
                    share: None,
                    bundled: false,
                };
                self.emit(
                    out,
                    Event::SongAdded {
                        song: Box::new(entry),
                    },
                );
                Ok(())
            }
            Command::RemoveSong { song } => {
                let id = self.own(actor)?;
                let entry = self.songs.get(*song).ok_or(Rejection::NotListed)?;
                if !self.may_remove(id, entry) {
                    return Err(Rejection::Unauthorized {
                        capability: Capability::AddSongs,
                    });
                }
                if self.song_in_use(entry) {
                    return Err(Rejection::SongInUse);
                }
                let share = entry.share.clone();
                self.emit(
                    out,
                    Event::SongRemoved {
                        song: *song,
                        why: crate::songs::Removal::Removed,
                        share,
                    },
                );
                Ok(())
            }
            Command::SetSongList { rules } => {
                self.require(actor, Capability::ChangeAccess)?;
                if self.source != ContentSource::Shared {
                    return Err(Rejection::NoSongList);
                }
                if !rules.valid() {
                    return Err(Rejection::InvalidSong);
                }
                if self.songs.rules != *rules {
                    self.emit(out, Event::SongListChanged { rules: *rules });
                }
                Ok(())
            }
            Command::SetWhoAdds { who } => {
                self.require(actor, Capability::ChangeAccess)?;
                if self.source != ContentSource::Shared {
                    return Err(Rejection::NoSongList);
                }
                if self.songs.who_adds != *who {
                    self.emit(out, Event::WhoAddsChanged { who: *who });
                }
                Ok(())
            }
            Command::SetSongContent {
                song,
                status,
                share,
                charts,
            } => {
                Self::authority_only(actor)?;
                let entry = self.songs.get(*song).ok_or(Rejection::NotListed)?;
                if charts.as_ref().is_some_and(|c| {
                    c.is_empty()
                        || c.len() > crate::songs::MAX_CHARTS
                        || !c.iter().all(crate::songs::ChartLine::fits)
                }) {
                    return Err(Rejection::InvalidSong);
                }
                let same_charts = charts.as_ref().is_none_or(|c| *c == entry.charts);
                if entry.status == *status && entry.share == *share && same_charts {
                    return Ok(());
                }
                self.emit(
                    out,
                    Event::SongContentSet {
                        song: *song,
                        status: status.clone(),
                        share: share.clone(),
                        charts: charts.clone(),
                    },
                );
                Ok(())
            }
            Command::Choose { song, chart } => {
                let id = self.own(actor)?;
                let Some(song) = song else {
                    // One's own choice cleared, whoever they are now.
                    if self.choices.contains_key(&id) {
                        self.emit(
                            out,
                            Event::ChoiceChanged {
                                participant: id,
                                choice: None,
                                why: None,
                            },
                        );
                    }
                    return Ok(());
                };
                let chart = chart.as_ref().ok_or(Rejection::ChartNotInSong)?;
                if let Some(why) = self.choose_refusal(id, *song, chart) {
                    return Err(why);
                }
                // Made while another stage is readied or played, a choice
                // waits: a host's opens no countdown until it is made
                // again once the room can pick (round FX).
                let waits = self.pick_stage().is_none();
                if self
                    .choices
                    .get(&id)
                    .is_some_and(|c| c.song == *song && c.chart == *chart && (waits || !c.waits))
                {
                    return Ok(());
                }
                let choice = crate::choice::Choice {
                    song: *song,
                    chart: chart.clone(),
                    at: self.revision.next(),
                    waits,
                };
                self.emit(
                    out,
                    Event::ChoiceChanged {
                        participant: id,
                        choice: Some(choice),
                        why: None,
                    },
                );
                Ok(())
            }
            Command::Draw { roll } => {
                Self::authority_only(actor)?;
                if !self.draw_due(now) {
                    return Err(Rejection::NoDrawDue);
                }
                self.draw_now(*roll, now, out);
                Ok(())
            }
            Command::TakePick => {
                self.require(actor, Capability::TakePick)?;
                let id = self.own(actor)?;
                if self.picking.picker != Some(id) {
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
                // The authority holds a newcomer to it; an operator (the
                // owner or a co-host) lets them play.
                if *on {
                    Self::authority_only(actor)?;
                } else {
                    self.require(actor, Capability::LetPlay)?;
                }
                let m = self
                    .members
                    .get(participant)
                    .ok_or(Rejection::UnknownParticipant)?;
                if m.is_cpu() {
                    return Err(Rejection::NotAHuman);
                }
                let (id, held) = (*participant, m.watch_only);
                // Held to watching, they watch (round D1): one who had
                // left wanting to play and comes back by the watch-only
                // link keeps their row, not their place among the
                // players (a newcomer by it joined to watch already).
                // Never the owner, whom it does not hold.
                let plays = *on && m.intent == Intent::Play && m.role != Role::Owner && !m.left;
                if held != *on {
                    self.emit(out, Event::WatchOnlyChanged { participant: id, on: *on });
                }
                if plays {
                    self.cancel_if_committed(id, out);
                    self.emit(
                        out,
                        Event::IntentChanged {
                            participant: id,
                            intent: Intent::Watch,
                        },
                    );
                }
                Ok(())
            }
            Command::SetAccess {
                admission,
                rotate_invite,
            } => {
                // The admission is the owner's; replacing the invite an
                // operator's, as copying it is.
                if *admission != self.access.admission || !*rotate_invite {
                    self.require(actor, Capability::ChangeAccess)?;
                } else {
                    self.require(actor, Capability::ReplaceInvite)?;
                }
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
                if self.hosting.if_host_leaves != *policy {
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
                // A deliberate hand-over: the one before stays a co-host.
                if *participant != from {
                    self.hand_over(from, Role::CoHost, *participant, out);
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
                let limit = usize::from(self.policy.max_members)
                    + if *admin {
                        usize::from(self.policy.admin_reserve)
                    } else {
                        0
                    };
                if let Some(m) = self.member_by_identity(identity) {
                    // The same person back: the same row, whatever the
                    // admission now says. One who had left arrives again
                    // as anyone does: refused at the member bound, and in
                    // to watch where every player's place is taken.
                    let participant = m.id;
                    let flag = (m.admin != *admin).then_some(*admin);
                    if m.left && self.present_people() >= limit {
                        return Err(Rejection::RoomFull);
                    }
                    // An administrator is one wherever they are, and only
                    // while their session says so: the row follows the
                    // join (revision 12).
                    if let Some(admin) = flag {
                        self.emit(out, Event::AdminChanged { participant, admin });
                    }
                    let m = &self.members[&participant];
                    if m.left || m.connection != Connection::Connected {
                        // Its seat given away meanwhile, with every place
                        // taken: to watch too (round RFa).
                        let to_watch = m.left
                            && m.intent == Intent::Play
                            && (self.players_full() || self.match_place_for(participant).is_err());
                        self.emit(out, Event::MemberRestored { participant });
                        if to_watch {
                            self.emit(
                                out,
                                Event::IntentChanged {
                                    participant,
                                    intent: Intent::Watch,
                                },
                            );
                        }
                    }
                    return Ok(());
                }
                if self.access.admission == Admission::Closed && !admin {
                    return Err(Rejection::AdmissionClosed);
                }
                if self.present_people() >= limit {
                    return Err(Rejection::RoomFull);
                }
                // The rows the room keeps, those who left too (round
                // RFa): a new person past them is refused as a full room.
                let rows = self.members.values().filter(|m| !m.is_cpu()).count();
                if rows >= self.policy.member_rows() {
                    return Err(Rejection::RoomFull);
                }
                // Every player's place taken: in to watch, not refused.
                let intent = &self.arrival_intent(*intent);
                let id = ParticipantId(self.counters.participant);
                self.emit(
                    out,
                    Event::MemberJoined {
                        member: Member::person(
                            id,
                            identity.clone(),
                            name.clone(),
                            Role::Member,
                            *admin,
                            *intent,
                        ),
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
                    self.drop_uploads_of(*participant, out);
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
            Command::CloseRoom { reason } => {
                Self::authority_only(actor)?;
                self.emit(out, Event::RoomClosed { reason: *reason });
                Ok(())
            }
            Command::SetTwitch { member, twitch } => {
                Self::authority_only(actor)?;
                let to = twitch.clone().filter(|t| t.on());
                self.may_set_twitch(*member, to.as_ref())?;
                if self.twitch != to {
                    self.emit(out, Event::TwitchChanged { twitch: to });
                }
                Ok(())
            }
        }
    }

    /// A pick is the draw's where the stage it would take is chosen by
    /// the group (revision 22): refused to the picker.
    fn not_by_group(&self) -> Decided {
        if self.pick_stage().is_some() && self.group_choice() {
            Err(Rejection::GroupChooses)
        } else {
            Ok(())
        }
    }

    /// The pick of a drawn entry: chart `won.chart` of listed song
    /// `won.song`, as the picker's pick of a listed song (its base the
    /// room as it stands: the draw is the room's own).
    fn pick_drawn(
        &mut self,
        won: &crate::choice::DrawEntry,
        now: Millis,
        out: &mut Vec<Event>,
    ) -> Decided {
        let entry = self.songs.get(won.song).ok_or(Rejection::NotListed)?;
        let line = entry.chart(&won.chart).ok_or(Rejection::ChartNotInSong)?;
        if !entry.available() {
            return Err(Rejection::SongNotReady);
        }
        let content = crate::ids::ContentId {
            song: entry.key.clone(),
            chart: won.chart.clone(),
        };
        let card = entry.card_for(line);
        let listed = (entry.id, entry.share.clone());
        let base = Some(self.revision);
        self.pick_content(base, &content, Some(&card), Some(listed), now, out)
    }

    /// Why a draw opened now by `by`'s choice could not pick, if anything
    /// (too few players, the next match's rules to review): the countdown
    /// waits until it could. What a screen says while a host's or
    /// co-host's choice stands and nothing counts down.
    pub fn draw_refusal(&self, by: ParticipantId, now: Millis) -> Option<Rejection> {
        let c = self.choices.get(&by)?;
        let won = crate::choice::DrawEntry {
            participant: by,
            song: c.song,
            chart: c.chart.clone(),
        };
        let mut trial = self.clone();
        trial.pick_drawn(&won, now, &mut Vec::new()).err()
    }

    /// The draw ([`Command::Draw`]): one entry a choosing member, by id;
    /// the winner `roll % entries`; the pick of it tried first, and the
    /// countdown canceled where it cannot be made (the choices stand).
    fn draw_now(&mut self, roll: u64, now: Millis, out: &mut Vec<Event>) {
        use crate::choice::{DrawEnd, DrawEntry, DrawRecord};
        let Some(d) = self.draw.clone() else { return };
        let entries: Vec<DrawEntry> = self
            .choices
            .iter()
            .map(|(p, c)| DrawEntry {
                participant: *p,
                song: c.song,
                chart: c.chart.clone(),
            })
            .collect();
        let Some(winner) = DrawRecord::winner_of(roll, entries.len()) else {
            self.emit(out, Event::DrawCanceled { why: DrawEnd::Empty });
            return;
        };
        let won = entries[usize::from(winner)].clone();
        let mut trial = self.clone();
        if trial.pick_drawn(&won, now, &mut Vec::new()).is_err() {
            self.emit(out, Event::DrawCanceled { why: DrawEnd::NoStart });
            return;
        }
        let record = DrawRecord {
            stage: d.stage,
            entries,
            winner,
            roll,
            at: now,
        };
        self.emit(out, Event::Drawn { record });
        let _ = self.pick_drawn(&won, now, out);
    }

    /// After every change and every tick (revision 22): choices that no
    /// longer stand go (the rule is Host now, their maker will not play,
    /// is away past the reconnect grace, or the song may not be chosen
    /// any more); a countdown whose choices all went, or whose stage the
    /// room can no longer pick by the group, is canceled; and where none
    /// runs, a host's or co-host's own choice opens one, if a draw now
    /// could pick ([`RoomState::draw_refusal`]). `by`: who sent the change
    /// (a member's command), whose own choice taken away says no why.
    fn settle_choices(&mut self, now: Millis, by: Option<ParticipantId>, out: &mut Vec<Event>) {
        use crate::choice::{ChoiceGone, DRAW_V0, Draw, DrawEnd};
        if self.closed.is_some() || (self.choices.is_empty() && self.draw.is_none()) {
            return;
        }
        let group = self.group_choice();
        let excluded = if group {
            self.excluded_songs().1
        } else {
            BTreeSet::new()
        };
        // Each choice that no longer stands, and why its maker is told (none
        // where it was their own doing or nothing is to be said).
        let gone: Vec<(ParticipantId, Option<ChoiceGone>)> = self
            .choices
            .iter()
            .filter_map(|(id, c)| {
                let why = if !group {
                    Some(ChoiceGone::Host)
                } else if self.chooser_refusal(*id).is_some()
                    || self
                        .members
                        .get(id)
                        .is_none_or(|m| m.connection == Connection::Disconnected)
                {
                    None
                } else if self
                    .songs
                    .get(c.song)
                    .is_none_or(|e| !e.available() || e.chart(&c.chart).is_none())
                {
                    Some(ChoiceGone::Song)
                } else if excluded.contains(&c.song) {
                    Some(ChoiceGone::Repeats)
                } else {
                    return None;
                };
                Some((*id, why.filter(|_| by != Some(*id))))
            })
            .collect();
        for (participant, why) in gone {
            self.emit(
                out,
                Event::ChoiceChanged {
                    participant,
                    choice: None,
                    why,
                },
            );
        }
        if let Some(d) = &self.draw {
            let moved = self.pick_stage() != Some(d.stage);
            let why = if !group || moved {
                Some(DrawEnd::Rules)
            } else if self.choices.is_empty() {
                Some(DrawEnd::Empty)
            } else if self
                .choices
                .keys()
                .next()
                .is_some_and(|by| self.draw_refusal(*by, now).is_some())
            {
                // A draw now could not start (too few players left): the
                // countdown waits until one could, the choices standing.
                Some(DrawEnd::NoStart)
            } else {
                None
            };
            // The stage it counted down for can no longer be drawn for (the
            // match ended or was stopped, a host started the stage with its
            // song; round RL4b): the choices made for it go, each told, so
            // that no countdown opens again by itself for a stage nobody
            // chose for (a new match starting ten seconds after a stop).
            if group && moved {
                let left: Vec<ParticipantId> = self.choices.keys().copied().collect();
                for participant in left {
                    self.emit(
                        out,
                        Event::ChoiceChanged {
                            participant,
                            choice: None,
                            why: (by != Some(participant)).then_some(ChoiceGone::Stage),
                        },
                    );
                }
            }
            if let Some(why) = why {
                self.emit(out, Event::DrawCanceled { why });
            }
            return;
        }
        let (Some(stage), Some(by)) = (self.pick_stage(), self.draw_starter()) else {
            return;
        };
        if !group || self.draw_refusal(by, now).is_some() {
            return;
        }
        self.emit(
            out,
            Event::DrawOpened {
                draw: Draw {
                    stage,
                    by,
                    opened_at: now,
                    ends_at: now.after(DRAW_V0.countdown_ms),
                },
            },
        );
    }

    /// `id` is out of the room (`gone`: their leaving, or their removal):
    /// a start they were committed to canceled first (so the departure
    /// also clears the readiness it held), their uploads dropped, the room
    /// passed on (or ended) where they were its host before the pick they
    /// held goes to its owner (the new one), their resting song cleared,
    /// and the room's Twitch setting ended where it was theirs (B3).
    fn depart(&mut self, id: ParticipantId, gone: Event, out: &mut Vec<Event>) {
        self.cancel_if_committed(id, out);
        self.emit(out, gone);
        self.drop_uploads_of(id, out);
        self.settle_host(out);
        self.release_pick(id, out);
        if self.closed.is_none() && self.spotlight.as_ref().is_some_and(|s| s.by == id) {
            self.emit(out, Event::SpotlightChanged { spotlight: None });
        }
        if self.closed.is_none() && self.twitch.as_ref().is_some_and(|t| t.by == id) {
            self.emit(out, Event::TwitchChanged { twitch: None });
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
                        window: Some(self.window_from(p, now, w.readiness_rev)),
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
        self.pick_content(base, content, card, None, now, out)
    }

    /// A pick of `content`; `listed`, the entry of the room's song list it
    /// plays and that entry's share (revision 17,
    /// [`Command::PickListed`]: available at once).
    fn pick_content(
        &mut self,
        base: Option<Revision>,
        content: &crate::ids::ContentId,
        card: Option<&ContentCard>,
        listed: Option<(crate::ids::RoomSongId, Option<String>)>,
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
        if humans < self.humans_needed(&candidates) || candidates.len() < usize::from(p.min_competitors) {
            return Err(Rejection::NotEnoughReady {
                ready_humans: u16::try_from(humans).unwrap_or(u16::MAX),
                competitors: u16::try_from(candidates.len()).unwrap_or(u16::MAX),
            });
        }
        let s = self.stage(stage).ok_or(Rejection::UnknownStage)?;
        if s.play.is_some() {
            return Err(Rejection::StageStarted);
        }
        // A new song, or the same one whose share failed: chosen (and
        // shared) anew, never a window on content nobody can load.
        let unusable = |c: &StageContent| matches!(c.status, ContentStatus::Unavailable { .. });
        let song = listed.as_ref().map(|(id, _)| *id);
        if s
            .content
            .as_ref()
            .is_none_or(|c| &c.id != content || unusable(c) || c.song != song)
        {
            let (status, share) = match (self.source, listed) {
                (ContentSource::LocalLibrary, _) => (ContentStatus::Available, None),
                (ContentSource::Shared, Some((_, share))) => (ContentStatus::Available, share),
                (ContentSource::Shared, None) => match self.known_share(content) {
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
                        song,
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
                    window: Some(self.window_from(stage, now, readiness_rev)),
                },
            },
        );
        Ok(())
    }

    /// A readiness window ends early once everyone connected who could
    /// still join is ready and loaded (`hosted-deadline-v1`): the start
    /// commits at once. A member reconnecting for a moment holds it as a
    /// connected one does (revision 12, `Connection::holds_place`); one
    /// past the authority's reconnect grace (`Disconnected`) does not,
    /// and the deadline still waits for them.
    fn maybe_commit(&mut self, now: Millis, out: &mut Vec<Event>) {
        if self.closed.is_some() {
            return;
        }
        let Phase::Preparing {
            stage,
            window: Some(w),
        } = self.phase
        else {
            return;
        };
        let Some(s) = self.stage(stage) else { return };
        if self.content_ready(s).is_err() {
            return;
        }
        // `hosted-deadline-v2`: everyone loaded, the Ready countdown
        // counts from now.
        // The candidates once for the checks below (this runs after every
        // accepted command).
        let candidates = self.candidates();
        let w = if w.loading() {
            if self.still_loading(s, &candidates) {
                return;
            }
            self.end_loading(stage, w, now, out)
        } else {
            w
        };
        // A download being tried again holds the early end until the
        // loading grace would have ended (revision 16).
        if w.hold_until.is_some_and(|h| now < h) && self.retrying(stage) {
            return;
        }
        let Some(s) = self.stage(stage) else { return };
        let grace = self.policy.readiness.grace().is_some();
        let (lineup, sat_out) = self.lineup(s, &candidates);
        // v2 waits for those who loaded to ready (the loading is past);
        // v1 for everyone connected who could still join.
        let waiting = sat_out.iter().any(|(id, r)| {
            r.may_still_join()
                && *r != SitOutReason::NotConnected
                && (!grace || s.prep_ref(*id).loading == Loading::Loaded)
        });
        if waiting || !self.enough(&lineup, &candidates).0 {
            return;
        }
        self.commit_start(stage, lineup, sat_out, now, out);
    }

    /// A readiness window opened at `now` (a pick, the song available to
    /// everyone, a Start): under `hosted-deadline-v2` its loading grace
    /// first, the Ready countdown's latest end after it. Where the group's
    /// draw just picked the song the Ready countdown counts from the end of
    /// the draw's reveal at the earliest ([`RoomState::ready_from`]).
    fn window_from(&self, stage: StageId, now: Millis, readiness_rev: u32) -> ReadinessWindow {
        let p = &self.policy.readiness;
        let ready = self.ready_from(stage, now);
        match p.grace() {
            Some(grace) => ReadinessWindow {
                deadline: now.after(grace).max(ready).after(p.window_ms),
                readiness_rev,
                loading_until: Some(now.after(grace)),
                hold_until: None,
            },
            None => ReadinessWindow {
                deadline: ready.after(p.window_ms),
                readiness_rev,
                loading_until: None,
                hold_until: None,
            },
        }
    }

    /// When stage `stage`'s Ready countdown may count from at `now`: now,
    /// or, where the group's draw picked its song (revision 22) and every
    /// device still shows the draw's reveal, the reveal's end
    /// (`DRAW_V0.reveal_ms` after the draw; round RL4b: the players see
    /// their Ready card only then, and the window is theirs whole).
    fn ready_from(&self, stage: StageId, now: Millis) -> Millis {
        self.drawn
            .as_ref()
            .filter(|d| d.stage == stage)
            .map(|d| d.at.after(crate::choice::DRAW_V0.reveal_ms))
            .map_or(now, |end| end.max(now))
    }

    /// Someone connected who could play `stage` has not loaded it yet
    /// (`hosted-deadline-v2`'s loading grace waits for them; a failed load
    /// waits for nothing).
    fn still_loading(&self, stage: &Stage, candidates: &[ParticipantId]) -> bool {
        candidates.iter().any(|&id| {
            self.members.get(&id).is_some_and(|m| {
                m.wants_to_play() && m.connection.holds_place()
            }) && stage.prep_ref(id).loading.in_progress()
        })
    }

    /// The loading grace over (everyone loaded, or its end reached): the
    /// Ready countdown counts from `now`.
    fn end_loading(
        &mut self,
        stage: StageId,
        w: ReadinessWindow,
        now: Millis,
        out: &mut Vec<Event>,
    ) -> ReadinessWindow {
        let window = ReadinessWindow {
            deadline: self.ready_from(stage, now).after(self.policy.readiness.window_ms),
            loading_until: None,
            // Ended early: a retrying download may hold the early end
            // until the grace's own end (revision 16).
            hold_until: w.loading_until.filter(|l| now < *l),
            ..w
        };
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Preparing {
                    stage,
                    window: Some(window),
                },
            },
        );
        window
    }

    /// Why `id` may not start `stage` on `chart` now ([`Command::SetStart`]),
    /// else the start as kept (none for the stage's own chart).
    fn start_refusal(
        &self,
        id: ParticipantId,
        stage: StageId,
        readiness_rev: u32,
        chart: Option<&crate::ids::ChartKey>,
        now: Millis,
    ) -> Result<Option<crate::ids::ChartKey>, Rejection> {
        if self.members[&id].intent != Intent::Play {
            return Err(Rejection::NotPlaying);
        }
        let s = self.stage(stage).ok_or(Rejection::UnknownStage)?;
        // Locked from the start; one counted out of it who may still
        // enter late chooses until they do.
        if let Some(play) = &s.play {
            if play.lineup.contains(&id) || s.prep_ref(id).ready {
                return Err(Rejection::StartCommitted);
            }
            self.late_entry_open(id, stage, now)?;
            if s.readiness_rev != readiness_rev {
                return Err(Rejection::StaleReadiness {
                    current: s.readiness_rev,
                });
            }
        } else {
            self.check_preparing(id, stage, readiness_rev)?;
        }
        if self.charts_for(stage) != crate::rules::Charts::Adaptive {
            return Err(Rejection::ChartsAsPicked);
        }
        let content = s.content.as_ref().ok_or(Rejection::NoContent)?;
        let Some(chart) = chart.filter(|c| **c != content.id.chart) else {
            return Ok(None);
        };
        if chart.as_str().is_empty() || chart.as_str().len() > crate::songs::MAX_KEY {
            return Err(Rejection::ChartNotInSong);
        }
        // A listed song's: one of its charts, in the stage chart's style
        // (the style its ladder is of). A song picked otherwise is not
        // known to the room: a chart its devices do not find on the
        // stage's ladder plays the stage's own.
        if let Some(entry) = content.song.and_then(|e| self.songs.get(e)) {
            let line = entry.chart(chart).ok_or(Rejection::ChartNotInSong)?;
            if entry
                .chart(&content.id.chart)
                .is_some_and(|stage| stage.style != line.style)
            {
                return Err(Rejection::ChartNotInSong);
            }
        }
        Ok(Some(chart.clone()))
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
        self.check_cpus_here(&rules)
            .map_err(Rejection::RulesNeedReview)?;
        let humans: Vec<ParticipantId> = self
            .members
            .values()
            .filter(|m| m.wants_to_play())
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
                Member::cpu(id, ordinal, rules.cpu_for_seat(ordinal))
            })
            .collect();
        let playlist = (0..rules.planned_stages())
            .map(|_| self.fresh_stage())
            .collect();
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
    /// allow (up to the competitor cap, never in a CPU's place:
    /// `RoomState::late_entrants`).
    fn prepare(&mut self, stage: StageId, out: &mut Vec<Event>) {
        self.admit(stage, out);
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

    /// Seat in `stage` whoever is waiting to join the running match
    /// ([`RoomState::late_entrants`]), if anyone.
    fn admit(&mut self, stage: StageId, out: &mut Vec<Event>) {
        let waiting = self.late_entrants();
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

    /// Joining during a match (room model revision 15,
    /// `MatchRules::mid_match_entry`), after every accepted command:
    /// someone who wants to play and has no seat (just joined, or a
    /// watcher who chose Play) is seated at once in the stage being
    /// prepared whose play has not started (its song to load like
    /// anyone's, the readiness window's grace and deadline as they
    /// stand); during a stage's countdown or play they wait for the next
    /// pick (`prepare`). Never while the room is closed.
    fn seat_newcomers(&mut self, out: &mut Vec<Event>) {
        if self.closed.is_some() {
            return;
        }
        let Phase::Preparing { stage, .. } = self.phase else {
            return;
        };
        let unstarted = self
            .current
            .as_ref()
            .and_then(|m| m.stage(stage))
            .is_some_and(|s| s.play.is_none());
        if unstarted && self.running_match().is_some() {
            self.admit(stage, out);
        }
    }

    /// [`Command::SetLastStage`].
    fn set_last_stage(&mut self, stage: StageId, last: bool, out: &mut Vec<Event>) -> Decided {
        let m = self.running_match().ok_or_else(|| self.wrong_phase())?;
        if !m.rules.open_ended {
            return Err(Rejection::NotOpenEnded);
        }
        let s = m.stage(stage).ok_or(Rejection::UnknownStage)?;
        if s.play.is_some() {
            return Err(Rejection::StageStarted);
        }
        if s.last != last {
            self.emit(out, Event::LastStageChanged { stage, last });
        }
        Ok(())
    }

    /// [`Command::EndMatch`]: between stages, with a stage settled.
    fn end_match_now(&mut self, out: &mut Vec<Event>) -> Decided {
        let m = self.running_match().ok_or_else(|| self.wrong_phase())?;
        if !m.rules.open_ended {
            return Err(Rejection::NotOpenEnded);
        }
        match self.phase {
            Phase::Intermission { .. } => {}
            Phase::Preparing { stage, .. } if m.stage(stage).is_some_and(|s| s.play.is_none()) => {}
            Phase::Countdown { .. } => return Err(Rejection::StartCommitted),
            _ => return Err(Rejection::StageStarted),
        }
        let settled = m
            .stages
            .iter()
            .any(|s| s.play.as_ref().is_some_and(|p| p.result.is_some()));
        if !settled {
            return Err(Rejection::NothingPlayed);
        }
        self.emit(out, Event::StagesDropped);
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::MatchResults,
            },
        );
        Ok(())
    }

    /// Room for one more song in a list of `size`: none needed, or the
    /// oldest songs no stage not yet over names removed (`RemoveOldest`);
    /// refused where it refuses, or where nothing can go.
    fn make_room(
        &mut self,
        size: usize,
        when_full: crate::songs::WhenFull,
        out: &mut Vec<Event>,
    ) -> Decided {
        if self.songs.len() < size {
            return Ok(());
        }
        if when_full == crate::songs::WhenFull::Refuse {
            return Err(Rejection::ListFull);
        }
        let over = self.songs.len() + 1 - size;
        let gone: Vec<(crate::ids::RoomSongId, Option<String>)> = self
            .songs
            .entries
            .iter()
            .filter(|e| !self.song_in_use(e))
            .take(over)
            .map(|e| (e.id, e.share.clone()))
            .collect();
        if gone.len() < over {
            return Err(Rejection::ListFull);
        }
        for (song, share) in gone {
            self.emit(
                out,
                Event::SongRemoved {
                    song,
                    why: crate::songs::Removal::ListFull,
                    share,
                },
            );
        }
        Ok(())
    }

    /// `id` is gone (left, or away past the authority's patience): the
    /// songs they added whose upload was not done leave the list (nobody
    /// else can finish it). Their songs that are there stay: the room's.
    fn drop_uploads_of(&mut self, id: ParticipantId, out: &mut Vec<Event>) {
        let gone: Vec<crate::ids::RoomSongId> = self
            .songs
            .entries
            .iter()
            .filter(|e| e.by == id && e.status == ContentStatus::Pending)
            .map(|e| e.id)
            .collect();
        for song in gone {
            self.emit(
                out,
                Event::SongRemoved {
                    song,
                    why: crate::songs::Removal::AdderGone,
                    share: None,
                },
            );
        }
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

    /// Who of `candidates` (`RoomState::candidates`) would start `stage`
    /// now, and who would sit out and why.
    fn lineup(
        &self,
        stage: &Stage,
        candidates: &[ParticipantId],
    ) -> (Vec<ParticipantId>, Vec<(ParticipantId, SitOutReason)>) {
        let mut lineup = Vec::new();
        let mut sat_out = Vec::new();
        for &id in candidates {
            match self.sit_out_reason(stage, id) {
                None => lineup.push(id),
                Some(reason) => sat_out.push((id, reason)),
            }
        }
        (lineup, sat_out)
    }

    fn enough(&self, lineup: &[ParticipantId], candidates: &[ParticipantId]) -> (bool, u16, u16) {
        let humans = lineup
            .iter()
            .filter(|id| !self.members[id].is_cpu())
            .count();
        let p = &self.policy.readiness;
        let needed = self.humans_needed(candidates);
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
    fn humans_needed(&self, candidates: &[ParticipantId]) -> usize {
        let wanting = candidates
            .iter()
            .filter(|id| {
                self.members
                    .get(id)
                    .is_some_and(|m| m.wants_to_play())
            })
            .count();
        usize::from(self.policy.readiness.min_ready_humans).min(wanting)
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
        self.open_next_stage(stage, out);
    }

    /// An open-ended match (`MatchRules::open_ended`) whose stage `stage`
    /// started and is not its last (`Match::is_last`: its last song is
    /// final once it starts) gets its next stage now, so that a change of
    /// the per-stage rules made during the play lands on it as in a
    /// counted match. Nothing where it has one.
    fn open_next_stage(&mut self, stage: StageId, out: &mut Vec<Event>) {
        let max = self.policy.max_stages;
        let due = self.current.as_ref().is_some_and(|m| {
            m.rules.open_ended
                && !m.is_last(stage, max)
                && m.stage_index(stage).is_some_and(|i| i + 1 == m.stages.len())
        });
        if due {
            let added = self.fresh_stage();
            self.emit(out, Event::StageAdded { stage: added });
        }
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

    /// Whether `id` may still enter `stage`'s started play late (round
    /// P1b, the maintainer's of 2026-11-16: "Yes, within a few seconds"):
    /// a room that plays live; the stage's start committed and its result
    /// not decided; `id` counted out of its lineup when it committed for
    /// a reason that may change (not ready, still loading, a load that
    /// failed, reconnecting), connected and wanting to play now, still in
    /// the match, a place under the competitor cap; and `now` no later
    /// than the live rules' late join after its song's time zero (the
    /// countdown before it counts as before the start).
    pub fn late_entry_open(
        &self,
        id: ParticipantId,
        stage: StageId,
        now: Millis,
    ) -> Decided {
        let rules = self
            .policy
            .live
            .as_ref()
            .and_then(|l| l.rules())
            .ok_or(Rejection::StartCommitted)?;
        let s = self.stage(stage).ok_or(Rejection::UnknownStage)?;
        let play = s.play.as_ref().ok_or(Rejection::NotUpcoming)?;
        let running = matches!(
            self.phase,
            Phase::Countdown { stage: c, .. } | Phase::Stage { stage: c } if c == stage
        );
        if !running || play.result.is_some() || play.interrupted || play.song_ended {
            return Err(Rejection::LateEntryClosed);
        }
        if !play
            .sat_out
            .get(&id)
            .is_some_and(|r| r.may_still_join())
        {
            return Err(Rejection::LateEntryClosed);
        }
        let m = self.members.get(&id).ok_or(Rejection::NotAMember)?;
        if !m.wants_to_play() {
            return Err(Rejection::NotPlaying);
        }
        if m.connection != Connection::Connected {
            return Err(Rejection::LateEntryClosed);
        }
        if let Some(m) = self.running_match() {
            let seat = m.roster.get(&id).ok_or(Rejection::NotInMatch)?;
            if seat.out_for_match {
                return Err(Rejection::OutForMatch);
            }
        }
        if play.lineup.len() >= usize::from(self.policy.max_competitors) {
            return Err(Rejection::Cap(crate::rules::CapExceeded {
                requested: u16::try_from(play.lineup.len() + 1).unwrap_or(u16::MAX),
                cap: self.policy.max_competitors,
            }));
        }
        let zero = play.song_zero_at.unwrap_or(play.start_at);
        if now > zero.after(rules.late_join_ms) {
            return Err(Rejection::LateEntryClosed);
        }
        Ok(())
    }

    /// [`Command::SetReady`] `true` after `stage`'s start committed: the
    /// member enters it late where [`Self::late_entry_open`] says they
    /// may and their song is loaded.
    fn enter_late(
        &mut self,
        id: ParticipantId,
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
        if s.play.as_ref().is_some_and(|p| p.lineup.contains(&id)) {
            return Ok(());
        }
        self.late_entry_open(id, stage, now)?;
        if s.prep_ref(id).loading != Loading::Loaded {
            return Err(Rejection::NotLoaded);
        }
        self.emit(
            out,
            Event::EnteredLate {
                stage,
                participant: id,
            },
        );
        Ok(())
    }

    /// [`Command::SetReady`] `false` from a starter during the countdown,
    /// where the policy lets it cancel the start (CPU practice, round
    /// P1b): the start canceled, a window opened again from `now`, the
    /// starter not ready in it.
    fn take_back_countdown(
        &mut self,
        id: ParticipantId,
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
        if !s.play.as_ref().is_some_and(|p| p.lineup.contains(&id)) {
            return Err(Rejection::StartCommitted);
        }
        self.emit(
            out,
            Event::StartCanceled {
                stage,
                reason: CancelReason::ReadyTakenBack,
            },
        );
        self.emit(
            out,
            Event::PhaseChanged {
                phase: Phase::Preparing {
                    stage,
                    window: Some(self.window_from(stage, now, readiness_rev)),
                },
            },
        );
        self.emit(
            out,
            Event::ReadyChanged {
                stage,
                participant: id,
                ready: false,
            },
        );
        Ok(())
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
            // Each stage awarded by the bias of the rules it is played
            // under: the match's, or a change from it on.
            let rules = m.rules_for(stage);
            (play.lineup.clone(), (rules.awards, rules.points))
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
        // Every preset ranks alike, an unknown one as survival does.
        let Some(ranking) = self.current.as_ref().and_then(|m| {
            let play = m.stage(stage)?.play.as_ref()?;
            Some(crate::battle::rank(&play.entrants(), m.rules_for(stage).charts.rank_by()))
        }) else {
            return;
        };
        // A result no rule can confirm (a field no award rule knows)
        // interrupts the match (round RFa): the stage stayed, its report
        // deadline past, every alarm doing nothing.
        match self.confirm(stage, &ranking, out) {
            Ok(_) => self.end_song(stage, out),
            Err(_) => self.interrupt(crate::phase::Interruption::AuthorityFailed, out),
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
        let mut next = self
            .current
            .as_ref()
            .and_then(|m| m.stage_index(stage).and_then(|i| m.stages.get(i + 1)))
            .map(|s| s.id);
        // An open-ended match's next stage was made when this one
        // started (`open_next_stage`); one that has none (this stage its
        // last) ends here.
        if next.is_none() {
            self.open_next_stage(stage, out);
            next = self
                .current
                .as_ref()
                .and_then(|m| m.stage_index(stage).and_then(|i| m.stages.get(i + 1)))
                .map(|s| s.id);
        }
        // An open-ended match's last song ends it whatever was made ahead.
        let max = self.policy.max_stages;
        if next.is_some()
            && self
                .current
                .as_ref()
                .is_some_and(|m| m.rules.open_ended && m.is_last(stage, max))
        {
            self.emit(out, Event::StagesDropped);
            next = None;
        }
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
        // As revision 16 keeps them, played as they were sent (When life
        // runs out under both chart rules from now on).
        let rules = &rules.clone().normalized();
        rules
            .validate(&self.policy, self.reserved_humans())
            .map_err(Rejection::InvalidRules)?;
        self.check_cpus_here(rules).map_err(Rejection::InvalidRules)?;
        self.check_choice_here(rules.choice)?;
        let have = self.playlist.len();
        let added = (have..usize::from(rules.planned_stages()))
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
            // `hosted-deadline-v2`: the loading grace's end; the Ready
            // countdown counts, those still loading may still make it.
            Phase::Preparing {
                stage,
                window: Some(w),
            } if w.loading_until.is_some_and(|l| now >= l) => {
                if self.content_pending(stage) {
                    return;
                }
                self.end_loading(stage, w, now, out);
                self.maybe_commit(now, out);
            }
            // A retrying download held the early end until the grace's
            // own end (revision 16): it starts without them now if
            // everyone else is ready.
            Phase::Preparing {
                stage,
                window: Some(w),
            } if now < w.deadline
                && w.hold_until.is_some_and(|h| now >= h)
                && self.retrying(stage) =>
            {
                let window = ReadinessWindow {
                    hold_until: None,
                    ..w
                };
                self.emit(
                    out,
                    Event::PhaseChanged {
                        phase: Phase::Preparing {
                            stage,
                            window: Some(window),
                        },
                    },
                );
                self.maybe_commit(now, out);
            }
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
                let candidates = self.candidates();
                let (lineup, sat_out) = self.lineup(s, &candidates);
                if self.enough(&lineup, &candidates).0 {
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
            Event::MemberLeft { participant } | Event::MemberRemoved { participant, .. } => {
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
            Event::AdminChanged { participant, admin } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.admin = *admin;
                }
            }
            Event::WatchOnlyChanged { participant, on } => {
                if let Some(m) = self.members.get_mut(participant) {
                    m.watch_only = *on;
                }
            }
            Event::AccessChanged { access } => self.access = *access,
            Event::TwitchChanged { twitch } => {
                self.twitch = twitch.clone();
            }
            Event::HostLeavesChanged { policy } => {
                self.hosting = Hosting {
                    if_host_leaves: *policy,
                };
            }
            Event::RoomClosed { reason } => self.closed = Some(*reason),
            Event::SongAdded { song } => {
                let list = std::sync::Arc::make_mut(&mut self.songs);
                list.next = list.next.max(song.id.0 + 1);
                list.entries.push((**song).clone());
                list.changed_at = pending;
            }
            Event::SongRemoved { song, .. } => {
                let list = std::sync::Arc::make_mut(&mut self.songs);
                list.entries.retain(|e| e.id != *song);
                list.changed_at = pending;
            }
            Event::SongContentSet {
                song,
                status,
                share,
                charts,
            } => {
                let list = std::sync::Arc::make_mut(&mut self.songs);
                if let Some(e) = list.entries.iter_mut().find(|e| e.id == *song) {
                    e.status = status.clone();
                    e.share = share.clone();
                    if let Some(c) = charts {
                        e.charts = c.clone();
                    }
                }
                list.changed_at = pending;
            }
            Event::SongListChanged { rules } => {
                let list = std::sync::Arc::make_mut(&mut self.songs);
                list.rules = *rules;
                list.changed_at = pending;
            }
            Event::WhoAddsChanged { who } => {
                let list = std::sync::Arc::make_mut(&mut self.songs);
                list.who_adds = *who;
                list.changed_at = pending;
            }
            Event::ChoiceChanged {
                participant,
                choice,
                ..
            } => match choice {
                Some(c) => {
                    self.choices.insert(*participant, c.clone());
                }
                None => {
                    self.choices.remove(participant);
                }
            },
            Event::DrawOpened { draw } => self.draw = Some(draw.clone()),
            Event::DrawCanceled { .. } => self.draw = None,
            Event::Drawn { record } => {
                self.draw = None;
                self.choices.clear();
                self.drawn = Some(record.clone());
            }
            Event::SpotlightChanged { spotlight } => self.spotlight = spotlight.clone(),
            Event::PickerChanged { picker } => {
                let mut picking = std::mem::take(&mut self.picking);
                // The song the last picker rested on goes with them; the
                // room's share of it stays known by its chart.
                if let Some(s) = self.spotlight.take()
                    && let Some(share) = s.share
                {
                    picking.keep_share(s.content.chart, share);
                }
                picking.picker = *picker;
                self.picking = picking;
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
            Event::StartChosen {
                stage,
                participant,
                chart,
            } => {
                if let Some(s) = self.stage_mut(*stage) {
                    s.prep.entry(*participant).or_default().start = chart.clone();
                }
            }
            Event::NextRulesSaved { rules, added } => {
                self.next.rules = rules.clone();
                self.next.saved_at = pending;
                self.playlist.truncate(usize::from(rules.planned_stages()));
                for id in added {
                    self.counters.stage = self.counters.stage.max(id.0 + 1);
                    self.playlist.push(Stage::new(*id));
                }
            }
            Event::NextRulesReviewed { review } => self.next.review = *review,
            Event::StageRulesChanged { from, rules } => {
                if let Some(m) = &mut self.current
                    && let Some(at) = m.stage_index(*from)
                {
                    let stages = &m.stages;
                    m.changes.retain(|c| {
                        stages
                            .iter()
                            .position(|s| s.id == c.from)
                            .is_some_and(|i| i < at)
                    });
                    m.changes.push(crate::state::RulesChange {
                        from: *from,
                        rules: rules.clone(),
                        saved_at: pending,
                    });
                }
            }
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
                    changes: Vec::new(),
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
            Event::StageAdded { stage } => {
                self.counters.stage = self.counters.stage.max(stage.0 + 1);
                if let Some(m) = &mut self.current {
                    m.stages.push(Stage::new(*stage));
                }
            }
            Event::LastStageChanged { stage, last } => {
                if let Some(s) = self
                    .current
                    .as_mut()
                    .and_then(|m| m.stages.iter_mut().find(|s| s.id == *stage))
                {
                    s.last = *last;
                }
            }
            Event::StagesDropped => {
                if let Some(m) = &mut self.current {
                    m.stages.retain(|s| s.play.is_some());
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
            Event::EnteredLate { stage, participant } => {
                if let Some(s) = self.stage_mut(*stage) {
                    s.prep.entry(*participant).or_default().ready = true;
                    if let Some(p) = s.play.as_mut() {
                        p.sat_out.remove(participant);
                        p.lineup.insert(*participant);
                    }
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
