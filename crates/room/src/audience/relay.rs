//! A relay, sans-IO: what one relay object runs (the server's
//! `AudienceRelay` Durable Object, and the fake's). It holds a few hundred
//! seats' sockets and one socket up to the room.
//!
//! - **Seats**: admitted against what the room said (the audience link's
//!   key, open or not, how many seats a relay holds) and the rules (two
//!   seats a signed-in identity, eight an anonymous one, whose joins are
//!   also bounded a minute: [`AudienceRules::seats_for`]);
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
use crate::ids::StageId;
use crate::live::server::LiveSnapshot;
use crate::live::stream::KeptFrames;
use crate::online::Message;
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

/// What the relay did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelayOut {
    pub to_seats: Vec<(Target, Message)>,
    /// Sockets to close (after what was sent), with their code.
    pub close: Vec<(Target, u16)>,
    /// For the room (JSON text, `RelayUpEnvelope`).
    pub upstream: Vec<String>,
}

impl RelayOut {
    fn text(&mut self, to: Target, msg: AudienceDown) {
        self.to_seats
            .push((to, Message::Text(AudienceDownEnvelope::text(msg))));
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
    let mut h = crate::ids::Fnv64::new();
    h.raw(identity.as_bytes());
    h.0
}

/// One relay.
#[derive(Clone, Debug)]
pub struct Relay {
    rules: AudienceRules,
    id: u32,
    admission: Option<Admission>,
    seats: BTreeMap<SeatId, Seat>,
    per_identity: BTreeMap<u64, u32>,
    /// Each anonymous identity's joins within the last
    /// `anonymous_join_window_ms` (their times; memory only, so a
    /// restarted relay counts afresh).
    anonymous_joins: BTreeMap<u64, VecDeque<u64>>,
    next_seat: SeatId,
    view: Option<PublicView>,
    /// How many watch in all, as the room last said.
    count: Option<u32>,
    stage: Option<StageId>,
    fold: Option<LiveSnapshot>,
    kept: KeptFrames,
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
            anonymous_joins: BTreeMap::new(),
            next_seat: 1,
            view: None,
            count: None,
            stage: None,
            fold: None,
            kept: KeptFrames::new(rules.keep_frames as usize),
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
        if super::is_anonymous(identity) {
            // Every join that reached this far counts, a refused one too.
            let joins = self.anonymous_joins.entry(mark).or_default();
            let window = self.rules.anonymous_join_window_ms;
            while joins.front().is_some_and(|t| now >= t + window)
            {
                joins.pop_front();
            }
            if joins.len() as u32 >= self.rules.anonymous_joins_per_minute {
                return Err(AudienceRefusal::Busy);
            }
            joins.push_back(now);
        }
        let held = self.per_identity.get(&mark).copied().unwrap_or(0);
        if held >= self.rules.seats_for(identity) {
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
        let (snapshot, frames) = self.kept.resume(after, last, self.rules.snapshot_frames);
        if let Some(first) = snapshot {
            let mut snap = fold.clone();
            snap.first_frame = first;
            out.text(to, AudienceDown::Snapshot(Box::new(snap)));
        }
        for b in frames {
            out.to_seats.push((to, Message::Binary(b.clone())));
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
        out.upstream.push(self.hello());
        out
    }

    /// Its socket to the room closed (it opens another).
    pub fn upstream_closed(&mut self) {
        self.up = false;
    }

    pub fn upstream(&self) -> bool {
        self.up
    }

    /// Its hello to the room: what it has of the view and the frames.
    fn hello(&self) -> String {
        RelayUpEnvelope::text(RelayUp::Hello {
            relay: self.id,
            version: AUDIENCE_VERSION,
            view: self.view.as_ref().map(|v| v.revision),
            after: self.last_frame(),
        })
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
            self.fold = Some(LiveSnapshot::empty(f.stage, f.at, f.n));
        }
        let next = self.last_n.map_or(1, |l| l + 1);
        if f.n < next {
            return out;
        }
        if f.n > next {
            // Frames missed on the way up (never on one socket): ask again.
            out.upstream.push(self.hello());
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
        self.kept.push(f.n, bytes.to_vec());
        out.to_seats
            .push((Target::All, Message::Binary(bytes.to_vec())));
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
        // Anonymous joins out of the window are forgotten.
        let window = self.rules.anonymous_join_window_ms;
        self.anonymous_joins
            .retain(|_, joins| joins.back().is_some_and(|t| now < t + window));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A seat's mark is kept across a relay's restart: a fixed value.
    #[test]
    fn an_identity_s_mark_is_this_value() {
        assert_eq!(identity_mark("twitch:12345"), 5_433_475_829_025_022_859);
        assert_eq!(identity_mark(""), 0xcbf2_9ce4_8422_2325);
    }

    fn admitting() -> Relay {
        let mut r = Relay::new(crate::audience::AUDIENCE_V0, 0);
        let _ = r.from_room_text(
            &RelayDownEnvelope::text(RelayDown::Admission {
                key: "k".into(),
                open: true,
                seats: 500,
            }),
            0,
        );
        r
    }

    #[test]
    fn an_anonymous_identity_holds_eight_seats_and_a_person_two() {
        let mut r = admitting();
        for _ in 0..8 {
            r.admit("anon:net", "k", 1_000).unwrap();
        }
        assert_eq!(r.admit("anon:net", "k", 1_000), Err(AudienceRefusal::TooManySeats));
        assert_eq!(r.held_by("anon:net"), 8);
        // Another network's mark, and a signed-in person's two.
        assert!(r.admit("anon:other", "k", 1_000).is_ok());
        r.admit("twitch:1", "k", 1_000).unwrap();
        r.admit("twitch:1", "k", 1_000).unwrap();
        assert_eq!(r.admit("twitch:1", "k", 1_000), Err(AudienceRefusal::TooManySeats));
        // A seat that goes frees its place.
        // The bounds are apart: a network's eight and its people's two
        // each, admitted at once.
        for _ in 0..8 {
            r.admit("anon:shared", "k", 1_000).unwrap();
        }
        r.admit("twitch:2", "k", 1_000).unwrap();
        r.admit("twitch:2", "k", 1_000).unwrap();
        assert_eq!(r.admit("anon:shared", "k", 1_000), Err(AudienceRefusal::TooManySeats));
        assert_eq!(r.admit("twitch:2", "k", 1_000), Err(AudienceRefusal::TooManySeats));
        let seat = r.admit("anon:third", "k", 1_000).unwrap();
        r.seat_gone(seat);
        assert_eq!(r.held_by("anon:third"), 0);
    }

    #[test]
    fn an_anonymous_identity_s_joins_are_bounded_a_minute_and_come_back() {
        let mut r = admitting();
        // A viewer whose socket keeps dropping: each join a new seat, the
        // old one gone.
        let mut t = 0;
        for _ in 0..30 {
            let seat = r.admit("anon:net", "k", t).unwrap();
            r.seat_gone(seat);
            t += 1_000;
        }
        assert_eq!(r.admit("anon:net", "k", t), Err(AudienceRefusal::Busy));
        // A signed-in person is never counted so.
        for _ in 0..40 {
            let seat = r.admit("twitch:1", "k", t).unwrap();
            r.seat_gone(seat);
        }
        // A minute after the first join, the next is let in.
        let _ = r.tick(60_000);
        assert!(r.admit("anon:net", "k", 60_000).is_ok());
        // Old joins are forgotten by the tick.
        let _ = r.tick(200_000);
        assert!(r.anonymous_joins.is_empty());
    }
}
