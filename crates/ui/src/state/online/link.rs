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

use super::SharedOnline;
use super::api::{self, ApiResponse, InviteDoc, Summary, TicketDoc};

/// What the link last did that the room's status line says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkNotice {
    InviteCopied,
    InviteReplaced,
    /// Replacing the invite was refused or failed.
    InviteNotReplaced,
}

/// A stage's upload grant as the room answered it: for the content chosen
/// at `chosen` (its `content_changed_at`).
#[derive(Clone, Debug, PartialEq)]
pub struct GrantNews {
    pub stage: StageId,
    pub chosen: Revision,
    pub answer: Result<GrantAnswer, api::Problem>,
}

pub struct OnlineLink {
    pub(crate) authority: OnlineAuthority<Box<dyn Transport>, ManualClock>,
    online: SharedOnline,
    room_id: String,
    identity: Identity,
    name: Option<String>,
    invite: Option<String>,
    epoch: u64,
    ticket_req: Option<u64>,
    summary_req: Option<u64>,
    invite_req: Option<u64>,
    invite_asked: bool,
    rotate_req: Option<u64>,
    grant_reqs: Vec<(u64, StageId, Revision)>,
    grants: Vec<GrantNews>,
    notice: Option<LinkNotice>,
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
        let summary_req = Some(online.borrow_mut().request(api::summary(room_id)));
        OnlineLink {
            authority: OnlineAuthority::new(RoomId::new(room_id), transport, clock),
            online,
            room_id: room_id.into(),
            identity,
            name: None,
            invite: None,
            epoch,
            ticket_req: None,
            summary_req,
            invite_req: None,
            invite_asked: false,
            rotate_req: None,
            grant_reqs: Vec::new(),
            grants: Vec::new(),
            notice: None,
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

    /// The invite link to share, for an operator, once read.
    pub fn invite_link(&self) -> Option<String> {
        let site = self.online.borrow().config.site.clone();
        self.invite.as_deref().map(|i| api::invite_link(&site, i))
    }

    pub fn notice(&self) -> Option<LinkNotice> {
        self.notice
    }

    pub fn set_notice(&mut self, notice: Option<LinkNotice>) {
        self.notice = notice;
    }

    /// Ask for the invite (operators only; others are refused and see
    /// none).
    pub fn fetch_invite(&mut self) {
        if !self.invite_asked {
            self.invite_asked = true;
            self.invite_req = Some(self.online.borrow_mut().request(api::invite(&self.room_id)));
        }
    }

    /// The owner replaces the invite: old links stop working.
    pub fn rotate_invite(&mut self) {
        if self.rotate_req.is_none() {
            self.rotate_req = Some(
                self.online
                    .borrow_mut()
                    .request(api::rotate_invite(&self.room_id)),
            );
        }
    }

    /// Ask the room for stage `stage`'s upload grant (its content chosen
    /// at `chosen`); the answer comes in [`Self::take_grants`].
    pub fn request_grant(&mut self, stage: StageId, chosen: Revision) {
        let id = self
            .online
            .borrow_mut()
            .request(api::grant(&self.room_id, stage.0));
        self.grant_reqs.push((id, stage, chosen));
    }

    /// The grants answered since the last call.
    pub fn take_grants(&mut self) -> Vec<GrantNews> {
        std::mem::take(&mut self.grants)
    }

    /// Sign in again after the room's socket said the session ended: the
    /// page returns to this room.
    pub fn sign_in_again(&mut self) {
        self.online
            .borrow_mut()
            .sign_in(super::Dest::Room(self.room_id.clone()));
    }

    /// Answers in, tickets asked for: before the authority's own poll.
    /// Whether anything the screen shows changed.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        let mut changed = false;
        self.online.borrow_mut().poll(now_ms);
        // Signed in again since the session ended here: start over.
        let epoch = self.online.borrow().epoch();
        if epoch != self.epoch {
            self.epoch = epoch;
            if let Some(me) = self.online.borrow().me() {
                self.identity = Identity::new(me.player_id.clone());
            }
            if self.authority.link() == LinkState::SignInAgain {
                self.authority.restart();
                changed = true;
            }
        }
        if self.authority.wants_ticket() {
            self.authority.ticket_asked();
            self.ticket_req = Some(self.online.borrow_mut().request(api::ticket(&self.room_id)));
            changed = true;
        }
        if let Some(r) = self.take(self.ticket_req) {
            self.ticket_req = None;
            self.authority.ticket(ticket_of(&r));
            changed = true;
        }
        if let Some(r) = self.take(self.summary_req) {
            self.summary_req = None;
            if let Ok(s) = api::read::<Summary>(&r) {
                self.name = Some(s.name);
            }
            changed = true;
        }
        if let Some(r) = self.take(self.invite_req) {
            self.invite_req = None;
            if let Ok(i) = api::read::<InviteDoc>(&r) {
                self.invite = Some(i.invite);
            }
            changed = true;
        }
        if let Some(r) = self.take(self.rotate_req) {
            self.rotate_req = None;
            match api::read::<InviteDoc>(&r) {
                Ok(i) => {
                    self.invite = Some(i.invite);
                    self.notice = Some(LinkNotice::InviteReplaced);
                }
                Err(_) => self.notice = Some(LinkNotice::InviteNotReplaced),
            }
            changed = true;
        }
        let asked = std::mem::take(&mut self.grant_reqs);
        for (id, stage, chosen) in asked {
            let r = self.online.borrow_mut().take_answer(id);
            match r {
                Some(r) => {
                    self.grants.push(GrantNews {
                        stage,
                        chosen,
                        answer: api::read::<GrantAnswer>(&r),
                    });
                    changed = true;
                }
                None => self.grant_reqs.push((id, stage, chosen)),
            }
        }
        changed
    }

    fn take(&self, req: Option<u64>) -> Option<ApiResponse> {
        self.online.borrow_mut().take_answer(req?)
    }
}

/// A ticket's answer as the authority takes it.
fn ticket_of(r: &ApiResponse) -> Result<String, TicketProblem> {
    match api::read::<TicketDoc>(r) {
        Ok(t) => Ok(t.ticket),
        Err(p) => Err(match p.status {
            401 => TicketProblem::SignedOut,
            403 => TicketProblem::NotAMember,
            404 | 410 => TicketProblem::RoomClosed,
            _ => TicketProblem::Unreachable,
        }),
    }
}
