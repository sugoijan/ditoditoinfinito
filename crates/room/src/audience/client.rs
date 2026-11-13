//! An audience device's side, sans-IO ([`AudienceLink`]) and over a
//! shell's socket ([`Audience`]): its hello on every socket, the view,
//! the count, the frames read in order by the members' own
//! [`FrameReader`] (a gap asks the relay for a resume), the stage as it
//! stands after a snapshot, a keep-alive between stages, and the room's
//! clock estimated from the frames' times ([`ArrivalClock`]). It sends
//! nothing else.
//!
//! The game draws everything (the music too) a delay behind the room's
//! song time: at most [`crate::audience::AudienceRules::presentation_delay_ms`],
//! as little as `crate::live::ViewDelay` measures (the frames' lateness,
//! as a member's views do).

use crate::audience::frames::{self, ArrivalClock};
use crate::audience::public::PublicView;
use crate::audience::{
    AUDIENCE_V0, AUDIENCE_VERSION, AudienceDown, AudienceDownEnvelope, AudienceEnd, AudienceRules,
    AudienceUp, AudienceUpEnvelope, KEEPALIVE, KEEPALIVE_ANSWER, close,
};
use crate::authority::Clock;
use crate::ids::{Millis, RoomId};
use crate::live::client::LiveSend;
use crate::live::server::LiveSnapshot;
use crate::live::stream::{Frame, FrameReader};
use crate::online::{Transport, TransportEvent};

/// What came, for the game.
#[derive(Clone, Debug, PartialEq)]
pub enum AudienceNews {
    View(Box<PublicView>),
    /// A frame taken in order (the reader has it too), and the count it
    /// carried.
    Frame(Frame),
    Snapshot(Box<LiveSnapshot>),
    Count(u32),
    Ended(AudienceEnd),
}

/// An audience seat's link.
#[derive(Clone, Debug)]
pub struct AudienceLink {
    rules: AudienceRules,
    on: bool,
    hello_due: bool,
    keepalive_at: u64,
    reader: Option<FrameReader>,
    view: Option<PublicView>,
    count: Option<u32>,
    ended: Option<AudienceEnd>,
    clock: ArrivalClock,
    news: Vec<AudienceNews>,
    frames: u32,
}

impl Default for AudienceLink {
    fn default() -> Self {
        Self::new(AUDIENCE_V0)
    }
}

impl AudienceLink {
    pub fn new(rules: AudienceRules) -> Self {
        Self {
            rules,
            on: false,
            hello_due: false,
            keepalive_at: 0,
            reader: None,
            view: None,
            count: None,
            ended: None,
            clock: ArrivalClock::default(),
            news: Vec::new(),
            frames: 0,
        }
    }

    pub fn rules(&self) -> AudienceRules {
        self.rules
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    pub fn view(&self) -> Option<&PublicView> {
        self.view.as_ref()
    }

    /// How many watch, as last said.
    pub fn count(&self) -> Option<u32> {
        self.count
    }

    /// The relay closed it for good (the room ended, the audience closed).
    pub fn ended(&self) -> Option<AudienceEnd> {
        self.ended
    }

    /// The frames read so far of the stage followed.
    pub fn reader(&self) -> Option<&FrameReader> {
        self.reader.as_ref()
    }

    /// Frames taken in order so far.
    pub fn frames(&self) -> u32 {
        self.frames
    }

    /// The room's time at the device's `local_ms`, once a frame came.
    pub fn room_ms(&self, local_ms: u64) -> Option<Millis> {
        self.clock.room_ms(local_ms)
    }

    pub fn take_news(&mut self) -> Vec<AudienceNews> {
        std::mem::take(&mut self.news)
    }

    /// A socket opened (`now` the device's ms).
    pub fn opened(&mut self, now: u64) {
        self.on = true;
        self.hello_due = true;
        self.keepalive_at = now + self.rules.keepalive_ms;
    }

    /// The socket closed with `code`: a close that ends the audience for
    /// this seat is kept ([`AudienceLink::ended`]).
    pub fn closed(&mut self, code: u16) {
        self.on = false;
        let end = match code {
            close::ROOM_CLOSED => Some(AudienceEnd::RoomClosed),
            close::AUDIENCE_CLOSED => Some(AudienceEnd::Closed),
            _ => None,
        };
        if let Some(end) = end
            && self.ended.is_none()
        {
            self.ended = Some(end);
            self.news.push(AudienceNews::Ended(end));
        }
    }

    pub fn text(&mut self, text: &str, _now: u64) {
        if text == KEEPALIVE_ANSWER {
            return;
        }
        let Some(msg) = AudienceDownEnvelope::read(text) else {
            // A newer relay's message: passed over.
            return;
        };
        match msg {
            AudienceDown::View(v) => {
                if self
                    .view
                    .as_ref()
                    .is_none_or(|old| old.revision <= v.revision)
                {
                    self.view = Some((*v).clone());
                    self.news.push(AudienceNews::View(v));
                }
            }
            AudienceDown::Count { count } => {
                self.count = Some(count);
                self.news.push(AudienceNews::Count(count));
            }
            AudienceDown::Snapshot(snap) => {
                let r = self
                    .reader
                    .get_or_insert_with(|| FrameReader::new(snap.stage));
                if r.stage() != Some(snap.stage) {
                    *r = FrameReader::new(snap.stage);
                }
                r.snapshot(&snap);
                self.news.push(AudienceNews::Snapshot(snap));
            }
            AudienceDown::Ended { end } => {
                self.ended = Some(end);
                self.news.push(AudienceNews::Ended(end));
            }
        }
    }

    pub fn binary(&mut self, bytes: &[u8], now: u64) {
        let Ok(a) = frames::decode(bytes) else {
            return;
        };
        let f = a.frame;
        self.clock.sample(f.at, now);
        if self
            .reader
            .as_ref()
            .is_none_or(|r| r.stage().is_none_or(|s| s < f.stage))
        {
            let mut r = FrameReader::new(f.stage);
            if f.n != 1 {
                r.ask_resume();
            }
            self.reader = Some(r);
        }
        let Some(r) = self.reader.as_mut() else {
            return;
        };
        if !r.take(&f, now) {
            return;
        }
        if let Some(c) = a.count
            && self.count != Some(c)
        {
            self.count = Some(c);
            self.news.push(AudienceNews::Count(c));
        }
        self.frames += 1;
        self.news.push(AudienceNews::Frame(f));
    }

    /// What to send now: the hello, a resume after a gap, the keep-alive.
    pub fn due(&mut self, now: u64) -> Vec<LiveSend> {
        let mut out = Vec::new();
        if !self.on {
            return out;
        }
        if self.hello_due {
            self.hello_due = false;
            out.push(LiveSend::Text(AudienceUpEnvelope::text(
                AudienceUp::Hello {
                    version: AUDIENCE_VERSION,
                },
            )));
        }
        if let Some(r) = self.reader.as_mut()
            && let Some(after) = r.wants_resume(now)
            && let Some(stage) = r.stage()
        {
            out.push(LiveSend::Text(AudienceUpEnvelope::text(
                AudienceUp::Resume { stage, after },
            )));
        }
        if now >= self.keepalive_at {
            self.keepalive_at = now + self.rules.keepalive_ms;
            out.push(LiveSend::Text(KEEPALIVE.into()));
        }
        out
    }
}

/// An audience seat over a shell's socket (`Transport`, as a member's
/// `OnlineAuthority`), on the device's clock. Its tickets come from the
/// server's audience join (`POST /rooms/<id>/audience`), one a socket: a
/// socket lost asks for another ([`Audience::wants_ticket`]) unless the
/// audience ended for it.
pub struct Audience<T: Transport, C: Clock> {
    room: RoomId,
    transport: T,
    clock: C,
    link: AudienceLink,
    wants_ticket: bool,
    open: bool,
}

impl<T: Transport, C: Clock> Audience<T, C> {
    pub fn new(room: RoomId, transport: T, clock: C) -> Self {
        Self {
            room,
            transport,
            clock,
            link: AudienceLink::default(),
            wants_ticket: true,
            open: false,
        }
    }

    pub fn link(&self) -> &AudienceLink {
        &self.link
    }

    pub fn link_mut(&mut self) -> &mut AudienceLink {
        &mut self.link
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// It needs a ticket (none yet, or its socket was lost).
    pub fn wants_ticket(&self) -> bool {
        self.wants_ticket && self.link.ended().is_none()
    }

    /// A ticket from the audience join: the socket opens.
    pub fn ticket(&mut self, ticket: &str) {
        self.wants_ticket = false;
        self.transport.open(&self.room, ticket);
    }

    /// The device leaves.
    pub fn leave(&mut self) {
        self.wants_ticket = false;
        self.transport.close();
        self.open = false;
    }

    /// Read what came, send what is due; what came for the game.
    pub fn poll(&mut self) -> Vec<AudienceNews> {
        let now = self.clock.now().0;
        for e in self.transport.poll() {
            match e {
                TransportEvent::Opened => {
                    self.open = true;
                    self.link.opened(now);
                }
                TransportEvent::Text(t) => self.link.text(&t, now),
                TransportEvent::Binary(b) => self.link.binary(&b, now),
                TransportEvent::Closed { code } => {
                    self.open = false;
                    self.link.closed(code);
                    self.wants_ticket = true;
                }
            }
        }
        if self.open {
            for s in self.link.due(now) {
                match s {
                    LiveSend::Text(t) => self.transport.send(t),
                    LiveSend::Binary(b) => self.transport.send_binary(b),
                }
            }
        }
        self.link.take_news()
    }
}
