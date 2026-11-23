//! A room's online link, as [`RoomClient`](crate::state::room::RoomClient)
//! holds it: `ddi_room`'s [`OnlineAuthority`] over the shell's socket, and
//! the HTTP around it through [`Online`](super::Online): the socket's
//! tickets (a fresh one each time it opens), the room's name, its invite
//! for its operators, replacing the invite, and a stage's upload grant
//! (M5d).

use ddi_library::share::GrantAnswer;
use ddi_room::{
    Identity, LinkState, ManualClock, OnlineAuthority, Revision, RoomId, StageId, TicketProblem,
    Transport,
};

use super::{Asked, SharedOnline};
use super::api::{self, ApiResponse, InviteDoc, Summary, TicketDoc};

/// What the link last did that the room's status line says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkNotice {
    InviteCopied,
    InviteReplaced,
    /// Replacing the invite was refused or failed.
    InviteNotReplaced,
    /// More's Copy watch-only link.
    WatchInviteCopied,
    /// Joined to watch because every player's place was taken: said for
    /// [`JOINED_TO_WATCH_MS`] once the room is connected.
    JoinedToWatch,
    /// More's Copy audience link.
    AudienceInviteCopied,
    /// Host options' Close the audience / Open the audience.
    AudienceClosed,
    AudienceOpened,
    /// Back by "Use this device" (round AC): said for
    /// [`JOINED_TO_WATCH_MS`] once the room is connected again, with
    /// whether they play the stage gathering or under way or the next.
    BackHere,
    /// Stage 2 of the room's song list: this device's storage ran out
    /// while downloading the room's songs ahead (the web's quota): said
    /// once for [`JOINED_TO_WATCH_MS`], Likely next from then on.
    AheadQuota,
    /// Room model revision 18: a host or a co-host (`by`) removed `who`
    /// from the room (a kick is told to everyone; the mirror's removal is
    /// a leave and never said).
    Kicked {
        who: ddi_room::ParticipantId,
        by: ddi_room::ParticipantId,
    },
    /// Round TW1b: the trip through Twitch for a room's Twitch setting
    /// (or, round TW3, this device's chat) ended without the access:
    /// nothing changed.
    TwitchNotConnected,
    /// Round TW3: the trip through Twitch for this device's "Twitch chat
    /// in rooms" connected it: the setting is on.
    TwitchChatOn,
    /// Round TW3: this player's Twitch access ended (refused at Twitch,
    /// disconnected elsewhere), so the room's Twitch setting, theirs,
    /// turned off (nobody removed).
    TwitchEnded,
    /// Round TW3: the one whose Twitch setting the room used (`who`) left
    /// it, so it turned off (B3).
    TwitchSetterLeft {
        who: ddi_room::ParticipantId,
    },
    /// Round TW7: the room shows its host's chat and this device's
    /// "Twitch chat in rooms" is on, but the person's Twitch access does
    /// not allow reading chat: what to do, said once for
    /// [`JOINED_TO_WATCH_MS`].
    TwitchChatNoAccess,
    /// Room model revision 22 (round RL4b): the room took this player's
    /// song choice away, not by their own doing (Song choice turned to
    /// Host, the Repeats rule excluding the song, the song gone, the draw
    /// called off): said once for [`JOINED_TO_WATCH_MS`].
    ChoiceGone {
        why: ddi_room::ChoiceGone,
    },
}

/// How long the status line says a join was to watch (from the room's
/// first connection).
pub const JOINED_TO_WATCH_MS: u64 = 12_000;

/// How long the status line says any other notice (a link copied, the
/// audience closed): a few seconds once, never carried into a screen
/// that opens later (feedback round H, item 2).
pub const NOTICE_MS: u64 = 5_000;

/// How long after a failed read of the invite it is asked for again.
pub const INVITE_RETRY_MS: u64 = 10_000;

/// A stage's upload grant as the room answered it: for the content chosen
/// at `chosen` (its `content_changed_at`).
#[derive(Clone, Debug, PartialEq)]
pub struct GrantNews {
    pub stage: StageId,
    pub chosen: Revision,
    pub answer: Result<GrantAnswer, api::Problem>,
}

/// An upload grant for an entry of the room's song list as the room
/// answered it (revision 17).
#[derive(Clone, Debug, PartialEq)]
pub struct SongGrantNews {
    pub song: ddi_room::RoomSongId,
    pub answer: Result<GrantAnswer, api::Problem>,
}

pub struct OnlineLink {
    pub(crate) authority: OnlineAuthority<Box<dyn Transport>, ManualClock>,
    online: SharedOnline,
    room_id: String,
    identity: Identity,
    name: Option<String>,
    invite: Option<String>,
    /// The watch-only link's invite, where the server makes one.
    watch_invite: Option<String>,
    /// The audience link's invite, where the room has an audience.
    audience_invite: Option<String>,
    epoch: u64,
    ticket_req: Option<Asked>,
    summary_req: Option<Asked>,
    invite_req: Option<Asked>,
    invite_asked: bool,
    /// Not asked for again before then (a read failed).
    invite_retry_at: u64,
    /// The wall clock of the last tick.
    now_ms: u64,
    rotate_req: Option<Asked>,
    /// The room's Twitch setting asked for (round TW1), answered later.
    twitch_req: Option<Asked>,
    /// The features the person's Twitch access must connect first for
    /// the setting asked (`409 scope_missing`), for the screen to start
    /// the trip through Twitch.
    twitch_trip: Option<String>,
    /// The setting last asked, for the trip to make it after.
    twitch_asked: Option<(ddi_room::TwitchMirror, ddi_room::TwitchChat)>,
    /// How the server answered the setting last asked (its status), once
    /// (round TB, [`Self::take_twitch_answer`]).
    twitch_answer: Option<u16>,
    grant_reqs: Vec<(Asked, StageId, Revision)>,
    grants: Vec<GrantNews>,
    song_grant_reqs: Vec<(Asked, ddi_room::RoomSongId)>,
    song_grants: Vec<SongGrantNews>,
    notice: Option<LinkNotice>,
    /// When the notice ends (set at the first tick after it was raised;
    /// [`LinkNotice::JoinedToWatch`]'s at the first tick connected).
    notice_until: Option<u64>,
}

impl OnlineLink {
    /// Room `room_id`'s link for the person signed in on `online`, its
    /// socket `transport`.
    pub fn new(
        online: SharedOnline,
        room_id: &str,
        transport: Box<dyn Transport>,
        clock: ManualClock,
    ) -> OnlineLink {
        let (identity, epoch) = {
            let o = online.borrow();
            let id = o.me().map(|m| m.player_id.clone()).unwrap_or_default();
            (Identity::new(id), o.epoch())
        };
        // A page loaded on the room's own address (a reload) knows nobody
        // yet: who is signed in is asked for, and taken in `tick`.
        online.borrow_mut().check();
        let summary_req = Some(Asked::ask(&online, api::summary(room_id)));
        let seated = online.borrow_mut().take_seated_to_watch(room_id);
        OnlineLink {
            authority: OnlineAuthority::new(RoomId::new(room_id), transport, clock),
            online,
            room_id: room_id.into(),
            identity,
            name: None,
            invite: None,
            watch_invite: None,
            audience_invite: None,
            epoch,
            ticket_req: None,
            summary_req,
            invite_req: None,
            invite_asked: false,
            invite_retry_at: 0,
            now_ms: 0,
            rotate_req: None,
            twitch_req: None,
            twitch_trip: None,
            twitch_asked: None,
            twitch_answer: None,
            grant_reqs: Vec::new(),
            grants: Vec::new(),
            song_grant_reqs: Vec::new(),
            song_grants: Vec::new(),
            notice: seated.then_some(LinkNotice::JoinedToWatch),
            notice_until: None,
        }
    }

    pub fn online(&self) -> &SharedOnline {
        &self.online
    }

    pub fn room_id(&self) -> &str {
        &self.room_id
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// The room's name, once read.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn state(&self) -> LinkState {
        self.authority.link()
    }

    /// Keep-alives sent, and sockets taken as dead for want of an answer
    /// (`ddi_room::online::KEEPALIVE_V0`).
    pub fn keepalives(&self) -> (u64, u64) {
        self.authority.keepalives()
    }

    /// The invite link to share, for an operator, once read.
    pub fn invite_link(&self) -> Option<String> {
        let i = self.invite.as_deref()?;
        Some(api::invite_link(&self.online.borrow().config.site, i))
    }

    /// The room's links as the server answered them.
    fn take_invites(&mut self, i: InviteDoc) {
        self.invite = Some(i.invite);
        self.watch_invite = i.watch_invite;
        self.audience_invite = i.audience_invite;
    }

    /// The watch-only link to share (joins to watch, held to it), for an
    /// operator, once read from a server that makes one.
    pub fn watch_invite_link(&self) -> Option<String> {
        let i = self.watch_invite.as_deref()?;
        Some(api::invite_link(&self.online.borrow().config.site, i))
    }

    /// The audience link to share (`#/mp/audience/<link>`), for an
    /// operator, once read from a server whose room has an audience.
    pub fn audience_invite_link(&self) -> Option<String> {
        let i = self.audience_invite.as_deref()?;
        Some(api::audience_link(&self.online.borrow().config.site, i))
    }

    pub fn notice(&self) -> Option<LinkNotice> {
        self.notice
    }

    pub fn set_notice(&mut self, notice: Option<LinkNotice>) {
        self.notice = notice;
        self.notice_until = None;
    }

    /// Ask for the invite (operators only; others are refused and see
    /// none).
    pub fn fetch_invite(&mut self) {
        if !self.invite_asked && self.now_ms >= self.invite_retry_at {
            self.invite_asked = true;
            self.invite_req = Some(Asked::ask(&self.online, api::invite(&self.room_id)));
        }
    }

    /// Read the invite again where it was read: another operator may have
    /// replaced it since (nothing in the room says so).
    pub fn refresh_invite(&mut self) {
        if self.invite.is_some() && self.invite_req.is_none() && self.rotate_req.is_none() {
            self.invite_req = Some(Asked::ask(&self.online, api::invite(&self.room_id)));
        }
    }

    /// The room's Twitch setting asked of the server (round TW1, `POST
    /// /rooms/<id>/twitch`): the room says the change when it is made.
    pub fn set_twitch(&mut self, mirror: ddi_room::TwitchMirror, chat: ddi_room::TwitchChat) {
        self.twitch_asked = Some((mirror, chat));
        // The setting asked before, its answer not read yet: this one
        // replaces it, so its answer is dropped, never held unread
        // (round LO).
        self.twitch_req = Some(Asked::ask(&self.online, api::room_twitch(&self.room_id, mirror, chat)));
    }

    /// The features to connect before the setting asked (once).
    pub fn take_twitch_trip(&mut self) -> Option<(String, Option<(ddi_room::TwitchMirror, ddi_room::TwitchChat)>)> {
        let trip = self.twitch_trip.take()?;
        Some((trip, self.twitch_asked))
    }

    /// Whether a trip through Twitch the setting asked needs waits to be
    /// taken ([`Self::take_twitch_trip`]).
    pub fn twitch_trip_waiting(&self) -> bool {
        self.twitch_trip.is_some()
    }

    /// How the server answered the setting last asked (its HTTP status:
    /// `409` a trip first), once.
    pub fn take_twitch_answer(&mut self) -> Option<u16> {
        self.twitch_answer.take()
    }

    /// The owner replaces the invite: old links stop working.
    pub fn rotate_invite(&mut self) {
        if self.rotate_req.is_none() {
            self.rotate_req = Some(Asked::ask(&self.online, api::rotate_invite(&self.room_id)));
        }
    }

    /// Ask the room for stage `stage`'s upload grant (its content chosen
    /// at `chosen`); the answer comes in [`Self::take_grants`].
    pub fn request_grant(&mut self, stage: StageId, chosen: Revision) {
        let asked = Asked::ask(&self.online, api::grant(&self.room_id, stage.0));
        self.grant_reqs.push((asked, stage, chosen));
    }

    /// The grants answered since the last call.
    pub fn take_grants(&mut self) -> Vec<GrantNews> {
        std::mem::take(&mut self.grants)
    }

    /// Ask the room for the upload grant of entry `song` of its song list
    /// (revision 17); the answer comes in [`Self::take_song_grants`].
    pub fn request_song_grant(&mut self, song: ddi_room::RoomSongId) {
        let asked = Asked::ask(&self.online, api::song_grant(&self.room_id, song.0));
        self.song_grant_reqs.push((asked, song));
    }

    /// The song grants answered since the last call.
    pub fn take_song_grants(&mut self) -> Vec<SongGrantNews> {
        std::mem::take(&mut self.song_grants)
    }

    /// Sign in after the room's socket said the session ended, or at a
    /// room's address opened signed out ([`LinkState::SignInToEnter`]):
    /// the page returns to this room.
    pub fn sign_in_again(&mut self) {
        self.online
            .borrow_mut()
            .sign_in(super::Dest::Room(self.room_id.clone()));
    }

    /// "Use this device" (round P1b): the room was opened on another of
    /// this person's devices and this one stopped
    /// ([`LinkState::OpenedElsewhere`]); joining again closes another
    /// device's socket in turn, which stops the same way. Whether it began.
    pub fn use_this_device(&mut self) -> bool {
        let stopped = self.authority.link() == LinkState::OpenedElsewhere;
        if stopped {
            self.authority.restart();
            self.set_notice(Some(LinkNotice::BackHere));
        }
        stopped
    }

    /// Answers in, tickets asked for: before the authority's own poll.
    /// Whether anything the screen shows changed.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        let mut changed = false;
        self.now_ms = now_ms;
        self.online.borrow_mut().poll(now_ms);
        // Whoever is signed in, once known (a reload learns it after the
        // link was made; a sign-in may change it).
        let me = self.online.borrow().me().map(|m| m.player_id.clone());
        if let Some(me) = me
            && self.identity.as_str() != me
        {
            self.identity = Identity::new(me);
            changed = true;
        }
        // Signed in again since the session ended here: start over.
        let epoch = self.online.borrow().epoch();
        if epoch != self.epoch {
            self.epoch = epoch;
            if matches!(
                self.authority.link(),
                LinkState::SignInAgain | LinkState::SignInToEnter
            ) {
                self.authority.restart();
                changed = true;
            }
        }
        if let Some(notice) = self.notice {
            match self.notice_until {
                None if notice == LinkNotice::AheadQuota => {
                    self.notice_until = Some(now_ms + JOINED_TO_WATCH_MS);
                }
                None if matches!(
                    notice,
                    LinkNotice::Kicked { .. } | LinkNotice::ChoiceGone { .. } | LinkNotice::TwitchChatNoAccess
                ) => {
                    self.notice_until = Some(now_ms + JOINED_TO_WATCH_MS);
                }
                None if !matches!(notice, LinkNotice::JoinedToWatch | LinkNotice::BackHere) => {
                    self.notice_until = Some(now_ms + NOTICE_MS);
                }
                None if self.authority.link() == LinkState::Connected => {
                    self.notice_until = Some(now_ms + JOINED_TO_WATCH_MS);
                }
                Some(until) if now_ms >= until => {
                    self.set_notice(None);
                    changed = true;
                }
                _ => {}
            }
        }
        self.usage_report();
        if self.authority.wants_ticket() {
            self.authority.ticket_asked();
            self.ticket_req = Some(Asked::ask(&self.online, api::ticket(&self.room_id)));
            changed = true;
        }
        if let Some(r) = self.ticket_req.as_ref().and_then(Asked::take) {
            self.ticket_req = None;
            self.authority.ticket(ticket_of(&r));
            changed = true;
        }
        if let Some(r) = self.summary_req.as_ref().and_then(Asked::take) {
            self.summary_req = None;
            if let Ok(s) = api::read::<Summary>(&r) {
                self.name = Some(s.name);
            }
            changed = true;
        }
        if let Some(r) = self.invite_req.as_ref().and_then(Asked::take) {
            self.invite_req = None;
            match api::read::<InviteDoc>(&r) {
                Ok(i) => self.take_invites(i),
                // Asked for again after a while (the screen asks while
                // there is none).
                Err(_) => {
                    self.invite_asked = false;
                    self.invite_retry_at = now_ms + INVITE_RETRY_MS;
                }
            }
            changed = true;
        }
        if let Some(r) = self.twitch_req.as_ref().and_then(Asked::take) {
            self.twitch_req = None;
            self.twitch_answer = Some(r.status);
            if r.status == 409
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(&r.body)
                && v["error"] == "scope_missing"
            {
                let need: Vec<&str> = v["need"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|n| n.as_str()).collect())
                    .unwrap_or_default();
                self.twitch_trip = Some(need.join(","));
            }
            changed = true;
        }
        if let Some(r) = self.rotate_req.as_ref().and_then(Asked::take) {
            self.rotate_req = None;
            match api::read::<InviteDoc>(&r) {
                Ok(i) => {
                    self.take_invites(i);
                    self.set_notice(Some(LinkNotice::InviteReplaced));
                }
                Err(_) => self.set_notice(Some(LinkNotice::InviteNotReplaced)),
            }
            changed = true;
        }
        let asked = std::mem::take(&mut self.grant_reqs);
        for (asked, stage, chosen) in asked {
            let r = asked.take();
            match r {
                Some(r) => {
                    self.grants.push(GrantNews {
                        stage,
                        chosen,
                        answer: api::read::<GrantAnswer>(&r),
                    });
                    changed = true;
                }
                None => self.grant_reqs.push((asked, stage, chosen)),
            }
        }
        let asked = std::mem::take(&mut self.song_grant_reqs);
        for (asked, song) in asked {
            match asked.take() {
                Some(r) => {
                    self.song_grants.push(SongGrantNews {
                        song,
                        answer: api::read::<GrantAnswer>(&r),
                    });
                    changed = true;
                }
                None => self.song_grant_reqs.push((asked, song)),
            }
        }
        changed
    }

    /// A reconnect that waits out its backoff tries now: the shell's page
    /// is shown again (the wait was reckoned while it stood still), or its
    /// socket closed while its timers cannot be trusted (a hidden tab's).
    /// Whether one was waiting. The link reads Reconnecting until the
    /// socket has the room again (`OnlineAuthority::retry_now`, room model
    /// revision 12: the backoff's attempts kept); a failure from here
    /// backs off as ever.
    pub fn hurry(&mut self) -> bool {
        let waiting = matches!(self.authority.link(), LinkState::Reconnecting { .. })
            && self.ticket_req.is_none()
            && self.authority.ticket_due().is_some()
            && !self.authority.wants_ticket();
        // Tried at once, still Reconnecting (never Joining for a moment)
        // and its attempts counted on.
        waiting && self.authority.retry_now()
    }

    /// The page's visibility handed on, and the usage report sent once a
    /// socket has the room (`ddi_room::client_stats`): only where Options'
    /// Usage statistics is on and the shell said what kind of client it
    /// is; otherwise dropped, nothing sent.
    fn usage_report(&mut self) {
        let visibility = self.online.borrow().visibility();
        if let Some((hidden, at)) = visibility {
            self.authority.set_hidden(hidden, at);
        }
        if !self.authority.stats_due() {
            return;
        }
        let kind = if crate::state::settings::Settings::cached().usage_stats {
            self.online.borrow().client_kind()
        } else {
            None
        };
        self.authority.send_stats(kind.as_ref());
    }
}

/// A ticket's answer as the authority takes it.
fn ticket_of(r: &ApiResponse) -> Result<String, TicketProblem> {
    match api::read::<TicketDoc>(r) {
        // An authority older than this client (round R2): no socket.
        Ok(t) if !ddi_room::check_authority(t.model) => Err(TicketProblem::ServerBehind),
        Ok(t) => Ok(t.ticket),
        Err(p) => Err(match p.status {
            _ if p.code == ddi_room::UPDATE_REQUIRED => TicketProblem::UpdateRequired,
            401 => TicketProblem::SignedOut,
            403 => TicketProblem::NotAMember,
            404 | 410 => TicketProblem::RoomClosed,
            _ => TicketProblem::Unreachable,
        }),
    }
}
