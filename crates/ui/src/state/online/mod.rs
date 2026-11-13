//! Invite-only online rooms as the screens see them
//! (`docs/plans/multiplayer-prototype-progress.md`, M5c;
//! `docs/plans/multiplayer-ui.md`, "Sign-in and participant identity"):
//! who is signed in, signing in (the web through a page of the server,
//! desktop through an attempt it polls while the system browser signs
//! in), making a room, an invite's preview and joining it. The room itself
//! is [`RoomClient`](crate::state::room::RoomClient) over an online link
//! ([`link::OnlineLink`]), the same client the practice room is.
//!
//! Sans-IO: requests go out through the shell's [`Http`] and their
//! answers come back to [`Online::poll`]; the screens read the state.
//! Shared by the screens and kept by the shell for the session or run
//! ([`SharedOnline`]). CPU practice never needs any of it.

pub mod api;
pub mod link;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use api::{ApiRequest, ApiResponse, AttemptDoc, AttemptPoll, AttemptStatus, Me, Preview, Problem};

/// The shell's HTTP: each request started, its answer returned later.
pub trait Http {
    fn start(&mut self, id: u64, request: ApiRequest);
    /// Answers since the last poll.
    fn poll(&mut self) -> Vec<(u64, ApiResponse)>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// Signs in by going to the server's page (the session a cookie).
    Web,
    /// Signs in through an attempt in the system browser (the session a
    /// bearer token kept beside the settings).
    Desktop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnlineConfig {
    pub platform: Platform,
    /// The game's web page, where invite links open (ends in `/`).
    pub site: String,
}

/// Who is signed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Account {
    /// Not asked yet.
    Unknown,
    Checking,
    SignedOut,
    SignedIn(Me),
    /// The server did not answer.
    Unreachable,
}

/// Where signing in leads: the game's route it returns to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dest {
    Home,
    /// Make a private room once signed in.
    Create,
    /// Join this invite once signed in.
    Join(String),
    /// Back into this room (after the session ended in it).
    Room(String),
}

impl Dest {
    /// The route the server returns to (its allow-list: `/mp`,
    /// `/mp/join/<invite>`, `/mp/room/<id>`).
    pub fn route(&self) -> String {
        match self {
            Dest::Home | Dest::Create => "/mp".into(),
            Dest::Join(invite) => format!("/mp/join/{invite}"),
            Dest::Room(id) => format!("/mp/room/{id}"),
        }
    }
}

/// Signing in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignIn {
    Idle,
    /// Desktop: asking the server for an attempt.
    Starting {
        dest: Dest,
    },
    /// Desktop: the system browser is on Twitch; the attempt is polled.
    Waiting {
        dest: Dest,
        attempt: String,
        secret: String,
        open_url: String,
    },
    /// Reading who signed in.
    Completing {
        dest: Dest,
    },
    Canceled {
        dest: Dest,
    },
    Failed {
        dest: Dest,
        code: String,
    },
}

impl SignIn {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            SignIn::Starting { .. } | SignIn::Waiting { .. } | SignIn::Completing { .. }
        )
    }
}

/// Where the shell is to go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Redirect {
    /// The web: this page of the server (a path after the API's base),
    /// leaving the game for Twitch's sign-in.
    Page(String),
    /// Desktop: open this URL in the system browser.
    Browser(String),
}

/// An invite being looked at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteView {
    pub invite: String,
    /// The preview, once read.
    pub preview: Option<Result<Preview, Problem>>,
    pub joining: bool,
    /// Why the join was refused.
    pub refused: Option<Problem>,
}

enum Want {
    Me,
    Logout,
    NewAttempt,
    Poll,
    Cancel,
    Create,
    Preview(String),
    Join(String),
    /// Someone else's (the room's link): kept for them.
    External,
}

/// A failed poll of the attempt is tried again after this long.
const POLL_AGAIN_MS: u64 = 2_000;

pub struct Online {
    http: Box<dyn Http>,
    pub config: OnlineConfig,
    /// Desktop's session, sent as a bearer.
    token: Option<String>,
    save_token: Option<SaveToken>,
    account: Account,
    sign_in: SignIn,
    invite: Option<InviteView>,
    creating: bool,
    create_problem: Option<Problem>,
    entered: Option<String>,
    /// A room joined to watch because every player's place was taken
    /// (the join asked to play): its id, until its link says so.
    seated_to_watch: Option<String>,
    redirect: Option<Redirect>,
    next_id: u64,
    pending: BTreeMap<u64, Want>,
    answers: BTreeMap<u64, ApiResponse>,
    /// The attempt's poll under way, and when a failed one goes again.
    polling: bool,
    poll_at: u64,
    now: u64,
    /// Counts sign-ins completed (a room's link starts again after one).
    epoch: u64,
    version: u64,
}

pub type SharedOnline = Rc<RefCell<Online>>;

/// Where desktop keeps its session (`None`: forgotten).
pub type SaveToken = Rc<dyn Fn(Option<&str>)>;

impl Online {
    pub fn new(http: Box<dyn Http>, config: OnlineConfig) -> Online {
        Online {
            http,
            config,
            token: None,
            save_token: None,
            account: Account::Unknown,
            sign_in: SignIn::Idle,
            invite: None,
            creating: false,
            create_problem: None,
            entered: None,
            seated_to_watch: None,
            redirect: None,
            next_id: 1,
            pending: BTreeMap::new(),
            answers: BTreeMap::new(),
            polling: false,
            poll_at: 0,
            now: 0,
            epoch: 0,
            version: 0,
        }
    }

    pub fn shared(self) -> SharedOnline {
        Rc::new(RefCell::new(self))
    }

    /// Desktop: the session kept from before, and where a new one is kept
    /// (`None`: forgotten).
    pub fn with_token(mut self, token: Option<String>, save: SaveToken) -> Online {
        self.token = token.filter(|t| !t.is_empty());
        self.save_token = Some(save);
        self
    }

    pub fn account(&self) -> &Account {
        &self.account
    }

    pub fn me(&self) -> Option<&Me> {
        match &self.account {
            Account::SignedIn(me) => Some(me),
            _ => None,
        }
    }

    pub fn sign_in_state(&self) -> &SignIn {
        &self.sign_in
    }

    pub fn invite(&self) -> Option<&InviteView> {
        self.invite.as_ref()
    }

    pub fn creating(&self) -> bool {
        self.creating
    }

    pub fn create_problem(&self) -> Option<&Problem> {
        self.create_problem.as_ref()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    fn changed(&mut self) {
        self.version += 1;
    }

    /// Start `request` (the session added); its answer is
    /// [`Online::take_answer`]'s.
    pub fn request(&mut self, request: ApiRequest) -> u64 {
        self.start(request, Want::External)
    }

    pub fn take_answer(&mut self, id: u64) -> Option<ApiResponse> {
        self.answers.remove(&id)
    }

    fn start(&mut self, mut request: ApiRequest, want: Want) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        if request.poll.is_none() {
            request.bearer = self.token.clone();
        }
        self.pending.insert(id, want);
        self.http.start(id, request);
        id
    }

    /// Where the shell is to go now, once.
    pub fn take_redirect(&mut self) -> Option<Redirect> {
        self.redirect.take()
    }

    /// A room to show now (made or joined), once.
    pub fn take_entered(&mut self) -> Option<String> {
        self.entered.take()
    }

    /// Whether room `room_id` was just joined to watch because every
    /// player's place was taken, once (its link's notice says so).
    pub fn take_seated_to_watch(&mut self, room_id: &str) -> bool {
        if self.seated_to_watch.as_deref() == Some(room_id) {
            self.seated_to_watch = None;
            true
        } else {
            false
        }
    }

    /// Ask who is signed in, unless known or being asked.
    pub fn check(&mut self) {
        if matches!(self.account, Account::Unknown | Account::Unreachable) {
            self.account = Account::Checking;
            self.start(api::me(), Want::Me);
            self.changed();
        }
    }

    /// Sign in, then go on to `dest`. The web leaves for the server's page
    /// ([`Redirect::Page`]); desktop asks for an attempt and opens the
    /// system browser on it.
    pub fn sign_in(&mut self, dest: Dest) {
        match self.config.platform {
            Platform::Web => {
                self.redirect = Some(Redirect::Page(api::login_path(&dest.route())));
                self.sign_in = SignIn::Completing { dest };
            }
            Platform::Desktop => {
                if let SignIn::Waiting {
                    attempt, secret, ..
                } = &self.sign_in
                {
                    let cancel = api::cancel_attempt(attempt, secret);
                    self.start(cancel, Want::Cancel);
                }
                self.start(api::new_attempt(&dest.route()), Want::NewAttempt);
                self.sign_in = SignIn::Starting { dest };
            }
        }
        self.changed();
    }

    /// Desktop: the browser again, on the same attempt (a new opening;
    /// the server takes only the latest one's return).
    pub fn open_browser_again(&mut self) {
        if let SignIn::Waiting { open_url, .. } = &self.sign_in {
            self.redirect = Some(Redirect::Browser(open_url.clone()));
        }
    }

    /// Stop signing in; nothing the browser does later signs in.
    pub fn cancel_sign_in(&mut self) {
        let dest = match std::mem::replace(&mut self.sign_in, SignIn::Idle) {
            SignIn::Waiting {
                dest,
                attempt,
                secret,
                ..
            } => {
                self.start(api::cancel_attempt(&attempt, &secret), Want::Cancel);
                dest
            }
            SignIn::Starting { dest } | SignIn::Completing { dest } => dest,
            other => {
                self.sign_in = other;
                return;
            }
        };
        self.polling = false;
        self.sign_in = SignIn::Canceled { dest };
        self.changed();
    }

    /// The web came back from the server's sign-in canceled
    /// (`?signin=canceled`).
    pub fn sign_in_canceled(&mut self, dest: Dest) {
        self.sign_in = SignIn::Canceled { dest };
        self.changed();
    }

    /// Forget the sign-in note (canceled, failed).
    pub fn clear_sign_in(&mut self) {
        if !self.sign_in.busy() && self.sign_in != SignIn::Idle {
            self.sign_in = SignIn::Idle;
            self.changed();
        }
    }

    pub fn sign_out(&mut self) {
        self.start(api::logout(), Want::Logout);
        self.forget_session();
        self.account = Account::SignedOut;
        self.changed();
    }

    fn forget_session(&mut self) {
        if self.token.take().is_some()
            && let Some(save) = &self.save_token
        {
            save(None);
        }
    }

    /// The home's Private room: made at once when signed in, else after
    /// signing in.
    pub fn private_room(&mut self) {
        self.create_problem = None;
        match &self.account {
            Account::SignedIn(_) => self.create(),
            _ => self.sign_in(Dest::Create),
        }
        self.changed();
    }

    fn create(&mut self) {
        if !self.creating {
            self.creating = true;
            self.start(api::create_room(None), Want::Create);
        }
    }

    /// Look at an invite: its preview read (no sign-in needed).
    pub fn open_invite(&mut self, invite: &str) {
        self.invite = Some(InviteView {
            invite: invite.into(),
            preview: None,
            joining: false,
            refused: None,
        });
        self.start(api::preview(invite), Want::Preview(invite.into()));
        self.check();
        self.changed();
    }

    /// Join the invite looked at: at once when signed in, else after
    /// signing in (the server checks the invite, the room's admission and
    /// seats again then).
    pub fn join_invite(&mut self) {
        let Some(view) = &mut self.invite else {
            return;
        };
        let invite = view.invite.clone();
        view.refused = None;
        match &self.account {
            Account::SignedIn(_) => self.join(&invite),
            _ => self.sign_in(Dest::Join(invite)),
        }
        self.changed();
    }

    fn join(&mut self, invite: &str) {
        let Some(room) = api::room_of(invite) else {
            return;
        };
        if let Some(v) = &mut self.invite {
            v.joining = true;
        }
        let request = api::join(room, Some(invite));
        self.start(request, Want::Join(invite.into()));
    }

    /// Answers in, time passes (`now_ms`).
    pub fn poll(&mut self, now_ms: u64) {
        self.now = now_ms;
        for (id, response) in self.http.poll() {
            if let Some(token) = response.session.clone().filter(|t| !t.is_empty())
                && self.token.is_some()
            {
                // The server re-signed the session: keep the new one.
                if let Some(save) = &self.save_token {
                    save(Some(&token));
                }
                self.token = Some(token);
            }
            let Some(want) = self.pending.remove(&id) else {
                continue;
            };
            self.answered(id, want, response);
        }
        if let SignIn::Waiting {
            attempt, secret, ..
        } = &self.sign_in
            && !self.polling
            && self.now >= self.poll_at
        {
            let poll = api::poll_attempt(attempt, secret);
            self.polling = true;
            self.start(poll, Want::Poll);
        }
    }

    fn answered(&mut self, id: u64, want: Want, response: ApiResponse) {
        match want {
            Want::External => {
                self.answers.insert(id, response);
                return;
            }
            Want::Me => {
                let dest = match &self.sign_in {
                    SignIn::Completing { dest } => Some(dest.clone()),
                    _ => None,
                };
                match api::read::<Me>(&response) {
                    Ok(me) => {
                        self.account = Account::SignedIn(me);
                        if let Some(dest) = dest {
                            self.sign_in = SignIn::Idle;
                            self.epoch += 1;
                            self.resume(dest);
                        }
                    }
                    Err(p) if p.signed_out() => {
                        self.forget_session();
                        self.account = Account::SignedOut;
                        if let Some(dest) = dest {
                            self.sign_in = SignIn::Failed { dest, code: p.code };
                        }
                    }
                    Err(p) => {
                        self.account = Account::Unreachable;
                        if let Some(dest) = dest {
                            self.sign_in = SignIn::Failed { dest, code: p.code };
                        }
                    }
                }
            }
            Want::Logout | Want::Cancel => return,
            Want::NewAttempt => {
                let SignIn::Starting { dest } = &self.sign_in else {
                    return;
                };
                let dest = dest.clone();
                match api::read::<AttemptDoc>(&response) {
                    Ok(a) => {
                        self.redirect = Some(Redirect::Browser(a.open_url.clone()));
                        self.polling = false;
                        self.poll_at = self.now;
                        self.sign_in = SignIn::Waiting {
                            dest,
                            attempt: a.attempt_id,
                            secret: a.poll_secret,
                            open_url: a.open_url,
                        };
                    }
                    Err(p) => self.sign_in = SignIn::Failed { dest, code: p.code },
                }
            }
            Want::Poll => {
                self.polling = false;
                let SignIn::Waiting { dest, .. } = &self.sign_in else {
                    return;
                };
                let dest = dest.clone();
                match api::read::<AttemptPoll>(&response) {
                    Ok(p) => match (p.status, p.session_token) {
                        (AttemptStatus::Pending, _) => {}
                        (AttemptStatus::Done, Some(token)) => {
                            if let Some(save) = &self.save_token {
                                save(Some(&token));
                            }
                            self.token = Some(token);
                            self.sign_in = SignIn::Completing { dest };
                            self.start(api::me(), Want::Me);
                        }
                        (AttemptStatus::Canceled, _) => {
                            self.sign_in = SignIn::Canceled { dest };
                        }
                        (AttemptStatus::Expired, _) => {
                            self.sign_in = SignIn::Failed {
                                dest,
                                code: "attempt_expired".into(),
                            }
                        }
                        (AttemptStatus::Done | AttemptStatus::Claimed, _) => {
                            self.sign_in = SignIn::Failed {
                                dest,
                                code: "attempt_claimed".into(),
                            }
                        }
                    },
                    Err(p) if p.status == 0 || p.status >= 500 => {
                        // The link or the server: polled again shortly.
                        self.poll_at = self.now + POLL_AGAIN_MS;
                        return;
                    }
                    Err(p) => self.sign_in = SignIn::Failed { dest, code: p.code },
                }
            }
            Want::Create => {
                self.creating = false;
                match api::read::<api::Created>(&response) {
                    Ok(c) => self.entered = Some(c.room_id),
                    Err(p) => {
                        if p.signed_out() {
                            self.forget_session();
                            self.account = Account::SignedOut;
                        }
                        self.create_problem = Some(p);
                    }
                }
            }
            Want::Preview(invite) => {
                if let Some(v) = self.invite.as_mut().filter(|v| v.invite == invite) {
                    v.preview = Some(api::read::<Preview>(&response));
                }
            }
            Want::Join(invite) => {
                let result = api::read::<api::Joined>(&response);
                if self.invite.as_ref().is_none_or(|v| v.invite != invite) {
                    return;
                }
                match result {
                    Ok(j) => {
                        // Asked to play (`api::join`), in to watch: the
                        // room was full of players (not the watch-only
                        // link, which says so on its page).
                        let watch_link = self
                            .invite
                            .as_ref()
                            .and_then(|v| v.preview.as_ref())
                            .and_then(|p| p.as_ref().ok())
                            .is_some_and(|p| p.kind == api::InviteKind::Watch);
                        self.seated_to_watch = (j.intent == ddi_room::Intent::Watch
                            && !j.already
                            && !watch_link)
                            .then(|| j.room_id.clone());
                        self.entered = Some(j.room_id);
                    }
                    Err(p) => {
                        if p.signed_out() {
                            self.forget_session();
                            self.account = Account::SignedOut;
                        }
                        if let Some(v) = &mut self.invite {
                            v.refused = Some(p);
                        }
                        // What the room says now.
                        self.start(api::preview(&invite), Want::Preview(invite.clone()));
                    }
                }
                if let Some(v) = &mut self.invite {
                    v.joining = false;
                }
            }
        }
        self.changed();
    }

    /// Signed in: on to where it was for, its terms checked again.
    fn resume(&mut self, dest: Dest) {
        match dest {
            Dest::Home => {}
            Dest::Create => self.create(),
            Dest::Join(invite) => {
                if self.invite.as_ref().is_none_or(|v| v.invite != invite) {
                    self.invite = Some(InviteView {
                        invite: invite.clone(),
                        preview: None,
                        joining: false,
                        refused: None,
                    });
                }
                self.start(api::preview(&invite), Want::Preview(invite.clone()));
                self.join(&invite);
            }
            Dest::Room(id) => self.entered = Some(id),
        }
    }

    /// The web came back signed in (to `dest`'s route): who it is read,
    /// then on to `dest`.
    pub fn returned(&mut self, dest: Dest) {
        self.sign_in = SignIn::Completing { dest };
        self.account = Account::Checking;
        self.start(api::me(), Want::Me);
        self.changed();
    }
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod live_tests;
