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
pub mod kept;
pub mod link;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

pub use kept::Kept;

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

/// Where a room's Twitch setting asked is kept across a trip through
/// Twitch (round TW1b).
pub const TWITCH_WANTED: &str = "ddi.mp.twitch";

/// Where this device's "Twitch chat in rooms" asked is kept across a trip
/// through Twitch (round TW3, [`Online::connect_twitch_chat`]).
pub const CHAT_WANTED: &str = "ddi.mp.twitch.chat";

/// A room's Twitch setting asked, waiting for the person's Twitch access.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TwitchWanted {
    pub room: String,
    pub mirror: ddi_room::TwitchMirror,
    pub chat: ddi_room::TwitchChat,
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
    /// Watch with the room's audience by this link
    /// (`docs/plans/multiplayer-audience.md`): no sign-in is asked for to
    /// watch since 2026-11-15; kept for a return the web saved before
    /// (`ddi.mp.after`), which goes straight to the seat.
    Audience(String),
}

impl Dest {
    /// The route the server returns to (its allow-list: `/mp`,
    /// `/mp/join/<invite>`, `/mp/room/<id>`).
    pub fn route(&self) -> String {
        match self {
            Dest::Home | Dest::Create => "/mp".into(),
            // The server returns to an invite's page alone: an audience
            // link's is one (its preview says what it is).
            Dest::Join(invite) | Dest::Audience(invite) => format!("/mp/join/{invite}"),
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
    /// Desktop: the system browser is on the attempt's page, which asks
    /// for `code` (shown grouped, `api::grouped_code`; none from a server
    /// that predates codes); the attempt is polled.
    Waiting {
        dest: Dest,
        attempt: String,
        secret: String,
        open_url: String,
        code: Option<String>,
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
    /// A join refused at the room's member bound offered its audience
    /// instead (`{"error":"room_full","audience":true}`).
    pub offered: bool,
}

impl InviteView {
    /// Invite `invite` looked at, nothing read of it yet.
    pub fn new(invite: &str) -> InviteView {
        InviteView {
            invite: invite.into(),
            preview: None,
            joining: false,
            refused: None,
            offered: false,
        }
    }

    /// The page offers the room's audience: an audience link, or the
    /// room's member bound reached (the preview's, or a join's refusal).
    pub fn audience(&self) -> bool {
        self.offered
            || self
                .preview
                .as_ref()
                .and_then(|p| p.as_ref().ok())
                .is_some_and(|p| p.audience || p.kind == api::InviteKind::Audience)
    }

    /// The link is the room's audience link.
    pub fn audience_link(&self) -> bool {
        self.preview
            .as_ref()
            .and_then(|p| p.as_ref().ok())
            .is_some_and(|p| p.kind == api::InviteKind::Audience)
    }
}

/// A seat in a room's audience given (`POST /rooms/<id>/audience`): the
/// link it was given by and the first socket's ticket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seated {
    pub room: String,
    pub link: String,
    pub ticket: String,
}

enum Want {
    /// Round TW1: the Twitch access disconnected; `/me` read again.
    TwitchDisconnected,
    Me,
    Logout,
    NewAttempt,
    Poll,
    Cancel,
    Create,
    Preview(String),
    Join(String),
    Audience(String),
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
    /// A seat in a room's audience given, for the shell to show.
    seated: Option<Seated>,
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
    /// What kind of client this is, as the shell says it, for a room's
    /// usage report (`ddi_room::client_stats`; none: nothing is sent).
    client_kind: Option<ClientKindFn>,
    /// The page hidden or shown, and since when (the rooms' clock); none
    /// until the shell says.
    visibility: Option<(bool, u64)>,
    /// How a trip through Twitch for this device's chat ended (round TW3,
    /// [`Online::connect_twitch_chat`]): connected, or not (nothing
    /// changed), until a screen takes it.
    chat_outcome: Option<bool>,
}

/// The shell's account of what kind of client it is, asked when a report
/// is due (its renderer may come after the online state).
pub type ClientKindFn = Rc<dyn Fn() -> ddi_room::client_stats::ClientKind>;

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
            seated: None,
            seated_to_watch: None,
            redirect: None,
            chat_outcome: None,
            next_id: 1,
            pending: BTreeMap::new(),
            answers: BTreeMap::new(),
            polling: false,
            poll_at: 0,
            now: 0,
            epoch: 0,
            version: 0,
            client_kind: None,
            visibility: None,
        }
    }

    /// What kind of client this is, for the rooms' usage reports.
    pub fn with_client_kind(mut self, kind: ClientKindFn) -> Online {
        self.client_kind = Some(kind);
        self
    }

    /// What kind of client this is, said after the state was made.
    pub fn set_client_kind(&mut self, kind: ClientKindFn) {
        self.client_kind = Some(kind);
    }

    /// What kind of client this is, where the shell said
    /// (`ddi_room::client_stats::ClientKind`).
    pub fn client_kind(&self) -> Option<ddi_room::client_stats::ClientKind> {
        self.client_kind.as_ref().map(|k| k())
    }

    /// The page hidden or shown at `at_ms` (the rooms' clock): how long a
    /// room's socket lived hidden, for its usage report.
    pub fn set_hidden(&mut self, hidden: bool, at_ms: u64) {
        if self.visibility.map(|(h, _)| h) != Some(hidden) {
            self.visibility = Some((hidden, at_ms));
        }
    }

    pub fn visibility(&self) -> Option<(bool, u64)> {
        self.visibility
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

    /// A seat in a room's audience to show now, once.
    pub fn take_seated(&mut self) -> Option<Seated> {
        self.seated.take()
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

    /// Connect the person's Twitch access for `scopes` (round TW1,
    /// `bans,blocks,chat`), then go on to `dest`: a sign-in's trip through
    /// Twitch, asking for those scopes (and what the access has).
    pub fn connect_twitch(&mut self, dest: Dest, scopes: &str) {
        self.connect_twitch_for(dest, scopes, None);
    }

    /// [`Online::connect_twitch`] for a room's Twitch setting asked
    /// (`wanted`: the room, the mirror and the chat): kept for the session
    /// across the web page's leaving (`ddi.mp.twitch`, as the sign-in's
    /// `ddi.mp.after`) and made by itself once the access is there
    /// (round TW1b, [`Online::take_twitch_wanted`]).
    pub fn connect_twitch_for(&mut self, dest: Dest, scopes: &str, wanted: Option<TwitchWanted>) {
        if let Some(w) = &wanted
            && let Ok(text) = serde_json::to_string(w)
        {
            let _ = ddi_platform::storage::set(ddi_platform::storage::Area::Session, TWITCH_WANTED, &text);
        }
        match self.config.platform {
            Platform::Web => {
                self.redirect = Some(Redirect::Page(api::connect_path(&dest.route(), scopes)));
                self.sign_in = SignIn::Completing { dest };
            }
            Platform::Desktop => {
                self.start(api::connect_attempt(&dest.route(), scopes), Want::NewAttempt);
                self.sign_in = SignIn::Starting { dest };
            }
        }
        self.changed();
    }

    /// Connect the person's Twitch access for this device's chat (round
    /// TW3, Options' "Twitch chat in rooms" turned on without the scope):
    /// the trip through Twitch for the closed scope set `chat`, back to
    /// `dest` (the room kept, else the multiplayer home), the setting at
    /// `level` kept for the session across the web page's leaving
    /// ([`CHAT_WANTED`]) and made by itself once the access has the scope
    /// ([`Online::take_chat_outcome`]).
    pub fn connect_twitch_chat(&mut self, dest: Dest, level: ddi_chat::Level) {
        if let Ok(text) = serde_json::to_string(&level) {
            let _ = ddi_platform::storage::set(ddi_platform::storage::Area::Session, CHAT_WANTED, &text);
        }
        self.chat_outcome = None;
        self.connect_twitch_for(dest, "chat", None);
    }

    /// This device's chat asked across a trip through Twitch, settled
    /// once the trip is over: the setting made where the access now has
    /// the chat's scope (saved; nothing where the setting is already on),
    /// forgotten where the trip was canceled or failed.
    fn settle_chat_wish(&mut self) {
        use ddi_platform::storage::{self, Area};
        let Some(level) = storage::load::<ddi_chat::Level>(Area::Session, CHAT_WANTED) else {
            return;
        };
        if self.sign_in.busy() || matches!(self.account, Account::Checking | Account::Unknown) {
            return;
        }
        let granted = self
            .twitch()
            .is_some_and(|t| t.features.iter().any(|f| f == "chat"));
        let ended = matches!(self.sign_in, SignIn::Canceled { .. } | SignIn::Failed { .. });
        if !granted && !ended {
            // The `/me` before the trip: wait for the next.
            return;
        }
        storage::remove(Area::Session, CHAT_WANTED);
        if granted {
            let mut s = crate::state::settings::Settings::load();
            if s.twitch_chat == ddi_chat::Level::Off {
                s.twitch_chat = level;
                s.save();
            }
        }
        self.chat_outcome = Some(granted);
        self.changed();
    }

    /// How the trip for this device's chat ended, once: connected (the
    /// setting made) or not (nothing changed).
    pub fn take_chat_outcome(&mut self) -> Option<bool> {
        self.chat_outcome.take()
    }

    /// Desktop or the web.
    pub fn platform(&self) -> Platform {
        self.config.platform
    }

    /// The room's Twitch setting a trip through Twitch was for, where it
    /// is room `room`'s and the trip is over: the setting itself where
    /// the access now grants what it needs (to be sent), or none and
    /// `canceled` where the trip ended without it. Forgotten either way.
    pub fn take_twitch_wanted(&mut self, room: &str) -> Option<(TwitchWanted, bool)> {
        use ddi_platform::storage::{self, Area};
        let w: TwitchWanted = storage::load(Area::Session, TWITCH_WANTED)?;
        if w.room != room || self.sign_in.busy() || matches!(self.account, Account::Checking) {
            return None;
        }
        let me = match &self.account {
            Account::SignedIn(me) => me,
            _ => return None,
        };
        let needs = api::twitch_needs(w.mirror, w.chat);
        let granted = needs.iter().all(|n| me.twitch.features.iter().any(|f| f == n));
        let canceled = matches!(self.sign_in, SignIn::Canceled { .. });
        if !granted && !canceled {
            // The `/me` before the trip: wait for the next.
            return None;
        }
        storage::remove(Area::Session, TWITCH_WANTED);
        Some((w, !granted))
    }

    /// The person's Twitch access as `/me` last said it.
    pub fn twitch(&self) -> Option<&api::TwitchAccess> {
        match &self.account {
            Account::SignedIn(me) => Some(&me.twitch),
            _ => None,
        }
    }

    /// Disconnect the person's Twitch access (A4): deleted on the server,
    /// revoked at Twitch; `/me` read again after.
    pub fn disconnect_twitch(&mut self) {
        self.start(api::twitch_disconnect(), Want::TwitchDisconnected);
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
        self.settle_chat_wish();
        self.changed();
    }

    /// The web came back from the server's sign-in canceled
    /// (`?signin=canceled`).
    pub fn sign_in_canceled(&mut self, dest: Dest) {
        self.sign_in = SignIn::Canceled { dest };
        self.settle_chat_wish();
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
        self.invite = Some(InviteView::new(invite));
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

    /// Watch with the room's audience by the invite looked at (an
    /// audience link, or the room's own where it offers its audience at
    /// its member bound): at once, signed in or not
    /// (`docs/plans/multiplayer-audience.md`, "Watching without signing
    /// in": a person signed in watches as themselves, anyone else
    /// anonymously).
    pub fn watch_audience(&mut self) {
        let Some(view) = &mut self.invite else {
            return;
        };
        let invite = view.invite.clone();
        view.refused = None;
        self.join_audience(&invite);
        self.changed();
    }

    fn join_audience(&mut self, link: &str) {
        self.join_by(link, true);
    }

    fn join(&mut self, invite: &str) {
        self.join_by(invite, false);
    }

    /// Join the room of `link`: to play (`api::join`), or with its
    /// `audience`.
    fn join_by(&mut self, link: &str, audience: bool) {
        let Some(room) = api::room_of(link) else {
            return;
        };
        if let Some(v) = &mut self.invite {
            v.joining = true;
        }
        if audience {
            self.start(api::audience_join(room, link), Want::Audience(link.into()));
        } else {
            self.start(api::join(room, Some(link)), Want::Join(link.into()));
        }
    }

    /// A problem answered: a session the server no longer knows is
    /// forgotten, and the account signed out.
    fn note_problem(&mut self, p: &Problem) {
        if p.signed_out() {
            self.forget_session();
            self.account = Account::SignedOut;
        }
    }

    /// Invite `invite` looked at (a new view unless it is the one shown),
    /// its preview read again.
    fn ensure_invite(&mut self, invite: &str) {
        if self.invite.as_ref().is_none_or(|v| v.invite != invite) {
            self.invite = Some(InviteView::new(invite));
        }
        self.start(api::preview(invite), Want::Preview(invite.into()));
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
            Want::TwitchDisconnected => {
                let _ = response;
                self.start(api::me(), Want::Me);
            }
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
                        self.settle_chat_wish();
                    }
                    Err(p) if p.signed_out() => {
                        self.note_problem(&p);
                        if let Some(dest) = dest {
                            self.sign_in = SignIn::Failed { dest, code: p.code };
                        }
                        self.settle_chat_wish();
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
                            code: a.user_code,
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
                        (AttemptStatus::Canceled, _)
                            if p.reason.as_deref() == Some(api::TOO_MANY_CODES) =>
                        {
                            // Ended by wrong codes in the page: Try again
                            // makes a new attempt with a new code.
                            self.sign_in = SignIn::Failed {
                                dest,
                                code: api::TOO_MANY_CODES.into(),
                            };
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
                    Err(p) if p.transient() => {
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
                        self.note_problem(&p);
                        self.create_problem = Some(p);
                    }
                }
            }
            Want::Preview(invite) => {
                if let Some(v) = self.invite.as_mut().filter(|v| v.invite == invite) {
                    v.preview = Some(api::read::<Preview>(&response));
                }
            }
            Want::Audience(link) => {
                let result = api::read_current::<api::AudienceTicketDoc>(&response);
                match result {
                    Ok(t) => {
                        self.seated = Some(Seated {
                            room: api::room_of(&link).unwrap_or_default().into(),
                            link: link.clone(),
                            ticket: t.ticket,
                        });
                    }
                    // The audience join needs no session: only a server
                    // from before asks for one (nobody is signed out for
                    // it).
                    Err(p) if p.signed_out() => {
                        if let Some(v) = self.invite.as_mut().filter(|v| v.invite == link) {
                            v.refused = Some(Problem {
                                status: p.status,
                                code: ddi_room::SERVER_BEHIND.into(),
                            });
                        }
                    }
                    Err(p) => {
                        self.note_problem(&p);
                        let p = api::worded_for(p, self.me().is_some());
                        if let Some(v) = self.invite.as_mut().filter(|v| v.invite == link) {
                            v.refused = Some(p);
                        }
                    }
                }
                if let Some(v) = &mut self.invite {
                    v.joining = false;
                }
            }
            Want::Join(invite) => {
                let offered = api::offers_audience(&response);
                let result = api::read_current::<api::Joined>(&response);
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
                        self.note_problem(&p);
                        if let Some(v) = &mut self.invite {
                            v.refused = Some(p);
                            v.offered |= offered;
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
                self.ensure_invite(&invite);
                self.join(&invite);
            }
            Dest::Audience(link) => {
                self.ensure_invite(&link);
                self.join_audience(&link);
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

#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod away_tests;

#[cfg(test)]
mod usage_tests;

#[cfg(test)]
mod ahead_tests;
