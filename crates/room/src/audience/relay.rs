//! A relay, sans-IO: what one relay object runs (the server's
//! `AudienceRelay` Durable Object, and the fake's). It holds a few hundred
//! seats' sockets and one socket up to the room.
//!
//! - **Seats**: admitted against what the room said (the audience link's
//!   key, open or not, how many seats a relay holds, two an identity);
//!   each seat's hello answered with the view, the count and the stage as
//!   the relay keeps it (a snapshot folded from the frames, and the last
//!   frames), a resume from the frames kept; frames sent on as they came,
//!   the same bytes to every seat.
//! - **Up**: a hello naming the relay, the view it has and the last frame
//!   it has, so a relay that restarted (its memory lost, its seats'
//!   sockets kept through hibernation, [`Relay::restore`]) gets what it
//!   lacks; its seats' count when it changed, once a second at most.
//!
//! What a relay keeps is the room's to give again: nothing of it is
//! stored, every seat's socket carries its own seat (its attachment).

use crate::audience::frames::{self, fold};
use crate::audience::public::PublicView;
use crate::audience::{
    AUDIENCE_VERSION, AudienceDown, AudienceDownEnvelope, AudienceEnd, AudienceRefusal,
    AudienceRules, AudienceUp, AudienceUpEnvelope, RelayDown, RelayDownEnvelope, RelayUp,
    RelayUpEnvelope, close,
};
use crate::ids::{Millis, StageId};
use crate::live::server::LiveSnapshot;
use std::collections::{BTreeMap, VecDeque};

/// A seat on a relay (the relay's own number, kept in its socket's
/// attachment).
pub type SeatId = u64;

/// Who a message goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Seat(SeatId),
    /// Every seat whose hello the relay took (the host tags those
    /// sockets): the same bytes for all.
    All,
}

/// One message for a seat's socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelaySend {
    Text(String),
    Binary(Vec<u8>),
}

/// What the relay did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelayOut {
    pub to_seats: Vec<(Target, RelaySend)>,
    /// Sockets to close (after what was sent), with their code.
    pub close: Vec<(Target, u16)>,
    /// For the room (JSON text, `RelayUpEnvelope`).
    pub upstream: Vec<String>,
}

impl RelayOut {
    fn text(&mut self, to: Target, msg: AudienceDown) {
        self.to_seats
            .push((to, RelaySend::Text(AudienceDownEnvelope::text(msg))));
    }

    pub fn is_empty(&self) -> bool {
        *self == RelayOut::default()
    }
}

#[derive(Clone, Debug)]
struct Seat {
    identity: u64,
    hello: bool,
    /// When it was given (a seat not taken by then is given up).
    given: u64,
}

#[derive(Clone, Debug)]
struct Admission {
    key: String,
    open: bool,
    seats: u32,
}

/// An identity's mark (FNV-1a 64): the relay never keeps who.
pub fn identity_mark(identity: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in identity.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// One relay.
#[derive(Clone, Debug)]
pub struct Relay {
    rules: AudienceRules,
    id: u32,
    admission: Option<Admission>,
    seats: BTreeMap<SeatId, Seat>,
    per_identity: BTreeMap<u64, u32>,
    next_seat: SeatId,
    view: Option<PublicView>,
    /// How many watch in all, as the room last said.
    count: Option<u32>,
    stage: Option<StageId>,
    fold: Option<LiveSnapshot>,
    kept: VecDeque<(u32, Vec<u8>)>,
    last_n: Option<u32>,
    up: bool,
    ended: Option<AudienceEnd>,
    /// Its seats as last said up, and when.
    said: Option<u32>,
    said_at: Option<u64>,
    /// The count as last told the seats between stages, and when.
    told: Option<u32>,
    told_at: Option<u64>,
}

impl Relay {
    pub fn new(rules: AudienceRules, id: u32) -> Self {
        Self {
            rules,
            id,
            admission: None,
            seats: BTreeMap::new(),
            per_identity: BTreeMap::new(),
            next_seat: 1,
            view: None,
            count: None,
            stage: None,
            fold: None,
            kept: VecDeque::new(),
            last_n: None,
            up: false,
            ended: None,
            said: None,
            said_at: None,
            told: None,
            told_at: None,
        }
    }

    /// A relay whose memory was lost (evicted, restarted) with its seats'
    /// sockets still open: their seats and identities' marks, as their
    /// attachments keep them. It knows nothing else until the room's
    /// answer to its hello.
    pub fn restore(
        rules: AudienceRules,
        id: u32,
        seats: impl IntoIterator<Item = (SeatId, u64)>,
    ) -> Self {
        let mut r = Self::new(rules, id);
        for (seat, identity) in seats {
            r.seats.insert(
                seat,
                Seat {
                    identity,
                    hello: true,
                    given: 0,
                },
            );
            *r.per_identity.entry(identity).or_default() += 1;
            r.next_seat = r.next_seat.max(seat + 1);
        }
        r
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    /// Its seats now.
    pub fn seats(&self) -> u32 {
        self.seats.len() as u32
    }

    /// The seats whose hello it took (the host's tagged sockets).
    pub fn hello_seats(&self) -> impl Iterator<Item = SeatId> + '_ {
        self.seats.iter().filter(|(_, s)| s.hello).map(|(k, _)| *k)
    }

    /// The room answered its hello: seats may be admitted.
    pub fn ready(&self) -> bool {
        self.admission.is_some() && self.view.is_some()
    }

    pub fn view(&self) -> Option<&PublicView> {
        self.view.as_ref()
    }

    /// The stage as the relay keeps it.
    pub fn stage_snapshot(&self) -> Option<&LiveSnapshot> {
        self.fold.as_ref()
    }

    pub fn last_frame(&self) -> Option<(StageId, u32)> {
        Some((self.stage?, self.last_n?))
    }

    /// A seat for `identity` coming by a link whose key is `key` (the
    /// server's hash of the link's secret, as the room said it), before its
    /// socket opens: its seat, or why not.
    pub fn admit(
        &mut self,
        identity: &str,
        key: &str,
        now: u64,
    ) -> Result<SeatId, AudienceRefusal> {
        if self.ended == Some(AudienceEnd::RoomClosed) {
            return Err(AudienceRefusal::RoomClosed);
        }
        let Some(a) = &self.admission else {
            return Err(AudienceRefusal::NoAudience);
        };
        if a.key != key {
            return Err(AudienceRefusal::BadLink);
        }
        if !a.open {
            return Err(AudienceRefusal::Closed);
        }
        if self.seats() >= a.seats {
            return Err(AudienceRefusal::Full);
        }
        let mark = identity_mark(identity);
        let held = self.per_identity.get(&mark).copied().unwrap_or(0);
        if held >= self.rules.seats_per_identity {
            return Err(AudienceRefusal::TooManySeats);
        }
        let seat = self.next_seat;
        self.next_seat += 1;
        self.seats.insert(
            seat,
            Seat {
                identity: mark,
                hello: false,
                given: now,
            },
        );
        *self.per_identity.entry(mark).or_default() += 1;
        Ok(seat)
    }

    /// Seats `identity` holds here.
    pub fn held_by(&self, identity: &str) -> u32 {
        self.per_identity
            .get(&identity_mark(identity))
            .copied()
            .unwrap_or(0)
    }

    /// A seat's socket said `text`.
    pub fn seat_text(&mut self, seat: SeatId, text: &str) -> RelayOut {
        let mut out = RelayOut::default();
        if !self.seats.contains_key(&seat) {
            return out;
        }
        match AudienceUpEnvelope::read(text) {
            Some(AudienceUp::Hello { .. }) => {
                if let Some(s) = self.seats.get_mut(&seat) {
                    s.hello = true;
                }
                let to = Target::Seat(seat);
                if let Some(v) = &self.view {
                    out.text(to, AudienceDown::View(Box::new(v.clone())));
                }
                if let Some(count) = self.count {
                    out.text(to, AudienceDown::Count { count });
                }
                if let Some(end) = self.ended {
                    out.text(to, AudienceDown::Ended { end });
                    out.close.push((to, close::of(end)));
                    return out;
                }
                self.catch_up(seat, None, &mut out);
            }
            Some(AudienceUp::Resume { stage, after }) if Some(stage) == self.stage => {
                self.catch_up(seat, after, &mut out);
            }
            Some(AudienceUp::Resume { .. }) | None => {}
        }
        out
    }

    /// What a seat lacks of the stage: the frames after `after` where they
    /// are all kept, else the stage as it stands and the last frames.
    fn catch_up(&self, seat: SeatId, after: Option<u32>, out: &mut RelayOut) {
        let (Some(fold), Some(last)) = (&self.fold, self.last_n) else {
            return;
        };
        let to = Target::Seat(seat);
        if let Some(a) = after
            && a <= last
            && self.kept.front().map_or(a == last, |(n, _)| *n <= a + 1)
        {
            for (n, b) in &self.kept {
                if *n > a {
                    out.to_seats.push((to, RelaySend::Binary(b.clone())));
                }
            }
            return;
        }
        let first = self
            .kept
            .front()
            .map_or(last + 1, |(n, _)| *n)
            .max((last + 1).saturating_sub(self.rules.snapshot_frames));
        let mut snap = fold.clone();
        snap.first_frame = first;
        out.text(to, AudienceDown::Snapshot(Box::new(snap)));
        for (n, b) in &self.kept {
            if *n >= first {
                out.to_seats.push((to, RelaySend::Binary(b.clone())));
            }
        }
    }

    /// A seat's socket closed.
    pub fn seat_gone(&mut self, seat: SeatId) {
        if let Some(s) = self.seats.remove(&seat)
            && let Some(n) = self.per_identity.get_mut(&s.identity)
        {
            *n -= 1;
            if *n == 0 {
                self.per_identity.remove(&s.identity);
            }
        }
    }

    /// Its socket to the room opened: its hello.
    pub fn upstream_opened(&mut self) -> RelayOut {
        self.up = true;
        // The room counts this relay afresh from its hello.
        self.said = None;
        let mut out = RelayOut::default();
        out.upstream.push(RelayUpEnvelope::text(RelayUp::Hello {
            relay: self.id,
            version: AUDIENCE_VERSION,
            view: self.view.as_ref().map(|v| v.revision),
            after: self.last_frame(),
        }));
        out
    }

    /// Its socket to the room closed (it opens another).
    pub fn upstream_closed(&mut self) {
        self.up = false;
    }

    pub fn upstream(&self) -> bool {
        self.up
    }

    /// The room's text.
    pub fn from_room_text(&mut self, text: &str, now: u64) -> RelayOut {
        let mut out = RelayOut::default();
        let Some(msg) = RelayDownEnvelope::read(text) else {
            return out;
        };
        match msg {
            RelayDown::Admission { key, open, seats } => {
                if open && self.ended == Some(AudienceEnd::Closed) {
                    self.ended = None;
                }
                self.admission = Some(Admission { key, open, seats });
            }
            RelayDown::View(v) => {
                if self
                    .view
                    .as_ref()
                    .is_none_or(|old| old.revision < v.revision)
                {
                    out.text(Target::All, AudienceDown::View(v.clone()));
                    self.view = Some(*v);
                }
            }
            RelayDown::Snapshot(snap) => {
                self.stage = Some(snap.stage);
                self.last_n = snap.first_frame.checked_sub(1);
                self.kept.clear();
                self.fold = Some(*snap);
            }
            RelayDown::Count { count } => {
                self.count = Some(count);
                self.tell_count(now, &mut out);
            }
            RelayDown::Ended { end } => {
                self.ended = Some(end);
                if end == AudienceEnd::Closed
                    && let Some(a) = self.admission.as_mut()
                {
                    a.open = false;
                }
                // Every seat goes (those given and not taken yet too), each
                // told why: named one by one, as the relay forgets them.
                for seat in self.seats.keys().copied().collect::<Vec<_>>() {
                    if self.seats[&seat].hello {
                        out.text(Target::Seat(seat), AudienceDown::Ended { end });
                    }
                    out.close.push((Target::Seat(seat), close::of(end)));
                    self.seat_gone(seat);
                }
            }
        }
        out
    }

    /// An audience frame from the room: kept, folded, sent on as it came.
    pub fn from_room_binary(&mut self, bytes: &[u8], now: u64) -> RelayOut {
        let mut out = RelayOut::default();
        let Ok(a) = frames::decode(bytes) else {
            return out;
        };
        let f = a.frame;
        if Some(f.stage) != self.stage {
            // A stage the room gave no snapshot of (it always does first):
            // folded from nothing.
            self.stage = Some(f.stage);
            self.last_n = f.n.checked_sub(1);
            self.kept.clear();
            self.fold = Some(LiveSnapshot {
                stage: f.stage,
                song_zero_at: Millis(0),
                at: f.at,
                first_frame: f.n,
                competitors: Vec::new(),
                commitments: Vec::new(),
                plan: None,
                settling: None,
            });
        }
        let next = self.last_n.map_or(1, |l| l + 1);
        if f.n < next {
            return out;
        }
        if f.n > next {
            // Frames missed on the way up (never on one socket): ask again.
            out.upstream.push(RelayUpEnvelope::text(RelayUp::Hello {
                relay: self.id,
                version: AUDIENCE_VERSION,
                view: self.view.as_ref().map(|v| v.revision),
                after: self.last_frame(),
            }));
            return out;
        }
        if let Some(c) = a.count {
            self.count = Some(c);
            // Told by the frame itself.
            self.told = Some(c);
            self.told_at = Some(now);
        }
        if let Some(snap) = self.fold.as_mut() {
            fold(snap, &f);
        }
        self.last_n = Some(f.n);
        self.kept.push_back((f.n, bytes.to_vec()));
        while self.kept.len() > self.rules.keep_frames as usize {
            self.kept.pop_front();
        }
        out.to_seats
            .push((Target::All, RelaySend::Binary(bytes.to_vec())));
        out
    }

    /// The count to the seats where it changed and no frame carried it
    /// lately (between stages), at most every `idle_count_ms`.
    fn tell_count(&mut self, now: u64, out: &mut RelayOut) {
        let Some(count) = self.count else {
            return;
        };
        if self.told != Some(count)
            && self
                .told_at
                .is_none_or(|t| now >= t + self.rules.idle_count_ms)
        {
            self.told = Some(count);
            self.told_at = Some(now);
            out.text(Target::All, AudienceDown::Count { count });
        }
    }

    /// Time passes (any message woke it, or its alarm): its seats' count up
    /// when it changed, at most every `count_ms`; the count to its seats
    /// between stages.
    pub fn tick(&mut self, now: u64) -> RelayOut {
        let mut out = RelayOut::default();
        // Seats given and never taken are given up.
        let stale: Vec<SeatId> = self
            .seats
            .iter()
            .filter(|(_, s)| !s.hello && now >= s.given + self.rules.ticket_ms)
            .map(|(k, _)| *k)
            .collect();
        for seat in stale {
            out.close
                .push((Target::Seat(seat), crate::online::close::BAD_MESSAGE));
            self.seat_gone(seat);
        }
        let seats = self.seats();
        if self.up
            && self.said != Some(seats)
            && self.said_at.is_none_or(|t| now >= t + self.rules.count_ms)
        {
            self.said = Some(seats);
            self.said_at = Some(now);
            out.upstream
                .push(RelayUpEnvelope::text(RelayUp::Count { seats }));
        }
        self.tell_count(now, &mut out);
        out
    }

    /// When the host should wake it next (its alarm), if something waits.
    pub fn next_wake(&self) -> Option<u64> {
        let seats = (self.up && self.said != Some(self.seats()))
            .then(|| self.said_at.map_or(0, |t| t + self.rules.count_ms));
        let count = (self.count.is_some() && self.told != self.count)
            .then(|| self.told_at.map_or(0, |t| t + self.rules.idle_count_ms));
        [seats, count].into_iter().flatten().min()
    }
}
