//! The room's side of the audience, sans-IO: what the server's room (and
//! the fake) runs beside its live side. It reads the room's state and the
//! frames the live side already broadcast to its members; it never writes
//! the state and never changes what the members are sent.
//!
//! Per relay socket: a relay's hello is answered with who may take a seat
//! (the audience link's key, open or closed, the relay's seats), the view,
//! the count and what it lacks of the stage (the frames after the last it
//! has, or the stage's snapshot and its last frames); its counts are
//! summed. Per tick: the members' frames coalesced into one audience
//! frame (every [`AudienceRules::frame_ms`]), encoded once, the same bytes
//! to every relay; a new stage's snapshot before its first frame; the
//! view again when what it shows changed (at most every
//! [`AudienceRules::view_ms`]); the count to the relays and the members'
//! live sockets when it changed (at most every
//! [`AudienceRules::count_ms`]).

use crate::audience::frames::{Coalesced, Coalescer};
use crate::audience::public::PublicView;
use crate::audience::{
    AudienceEnd, AudienceRules, RelayDown, RelayDownEnvelope, RelayUp, RelayUpEnvelope,
};
use crate::ids::{Millis, ParticipantId, Revision, StageId};
use crate::live::ServerLive;
use crate::live::server::{LiveServer, LiveStage};
use crate::member::{Capability, can};
use crate::state::RoomState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One message for a relay's socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HubSend {
    /// JSON text (`RelayDownEnvelope`).
    Text(String),
    /// An audience frame's bytes.
    Binary(Vec<u8>),
}

/// What the room's audience side did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HubOut {
    /// For one relay alone, in order.
    pub to_relay: Vec<(u32, HubSend)>,
    /// For every relay connected, in order: the same bytes for all.
    pub to_relays: Vec<HubSend>,
    /// For every member's socket that said live hello (the count).
    pub to_members: Option<ServerLive>,
    /// What the host stores changed.
    pub durable: Option<AudienceDurable>,
}

impl HubOut {
    fn all(&mut self, msg: RelayDown) {
        self.to_relays
            .push(HubSend::Text(RelayDownEnvelope::text(msg)));
    }

    fn one(&mut self, relay: u32, msg: RelayDown) {
        self.to_relay
            .push((relay, HubSend::Text(RelayDownEnvelope::text(msg))));
    }

    pub fn is_empty(&self) -> bool {
        *self == HubOut::default()
    }
}

/// What the host keeps in its storage: whether the host closed the
/// audience.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceDurable {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
}

/// The room's audience side.
#[derive(Clone, Debug)]
pub struct AudienceHub {
    rules: AudienceRules,
    key: String,
    open: bool,
    /// Each relay connected and its seats.
    relays: BTreeMap<u32, u32>,
    coalescer: Coalescer,
    /// The view as last sent, and when.
    view: Option<PublicView>,
    view_at: Option<Millis>,
    /// The room's revision the view was last derived at.
    derived_at: Option<Revision>,
    /// A view that changed and waits for its period.
    view_due: Option<PublicView>,
    /// The count (and openness) as last told, and when.
    told: Option<(u32, bool)>,
    told_at: Option<Millis>,
    ended: bool,
}

impl AudienceHub {
    /// The audience side of a room whose audience link has `key` (opaque:
    /// what a relay compares a presented link's with).
    pub fn new(rules: AudienceRules, key: impl Into<String>) -> Self {
        Self {
            rules,
            key: key.into(),
            open: true,
            relays: BTreeMap::new(),
            coalescer: Coalescer::new(rules.frame_ms, rules.keep_frames as usize),
            view: None,
            view_at: None,
            derived_at: None,
            view_due: None,
            told: None,
            told_at: None,
            ended: false,
        }
    }

    /// From what the host kept.
    pub fn restore(rules: AudienceRules, key: impl Into<String>, d: AudienceDurable) -> Self {
        Self {
            open: !d.closed,
            ..Self::new(rules, key)
        }
    }

    pub fn durable(&self) -> AudienceDurable {
        AudienceDurable { closed: !self.open }
    }

    pub fn rules(&self) -> AudienceRules {
        self.rules
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// How many watch: every relay's seats.
    pub fn count(&self) -> u32 {
        self.relays.values().sum()
    }

    /// The relays connected.
    pub fn relays(&self) -> impl Iterator<Item = u32> + '_ {
        self.relays.keys().copied()
    }

    /// The audience frames built so far of the stage followed.
    pub fn frames(&self) -> u32 {
        self.coalescer.last()
    }

    /// The audience link's key changed (the room's invite was replaced):
    /// every relay is told; seats already given stay.
    pub fn set_key(&mut self, key: impl Into<String>) -> HubOut {
        self.key = key.into();
        let mut out = HubOut::default();
        for relay in self.relays.keys().copied().collect::<Vec<_>>() {
            out.one(relay, self.admission());
        }
        out
    }

    fn admission(&self) -> RelayDown {
        RelayDown::Admission {
            key: self.key.clone(),
            open: self.open && !self.ended,
            seats: self.rules.relay_seats,
        }
    }

    /// A frame the room's live side broadcast to its members (its bytes,
    /// as sent).
    pub fn member_frame(&mut self, bytes: &[u8]) {
        self.coalescer.member_frame(bytes);
    }

    /// A relay's socket said `text`: `from` the relay that socket said
    /// hello as (none before its hello; a hello names its relay).
    pub fn relay_text(
        &mut self,
        from: Option<u32>,
        text: &str,
        room: &RoomState,
        name: &str,
        live: Option<&LiveServer>,
        now: Millis,
    ) -> HubOut {
        let mut out = HubOut::default();
        let Some(msg) = RelayUpEnvelope::read(text) else {
            return out;
        };
        match msg {
            RelayUp::Hello {
                relay, view, after, ..
            } => {
                self.relays.entry(relay).or_insert(0);
                out.one(relay, self.admission());
                let current = PublicView::of(room, name);
                if view != Some(current.revision) {
                    out.one(relay, RelayDown::View(Box::new(current)));
                }
                out.one(
                    relay,
                    RelayDown::Count {
                        count: self.count(),
                    },
                );
                if self.ended {
                    out.one(
                        relay,
                        RelayDown::Ended {
                            end: AudienceEnd::RoomClosed,
                        },
                    );
                    return out;
                }
                self.catch_up(relay, after, live, now, &mut out);
            }
            RelayUp::Count { seats } => {
                // Only a relay that said hello is counted.
                if let Some(relay) = from {
                    self.relay_count(relay, seats);
                }
            }
        }
        out
    }

    /// A relay's count (the host knows which relay's socket it came on).
    pub fn relay_count(&mut self, relay: u32, seats: u32) {
        if let Some(s) = self.relays.get_mut(&relay) {
            *s = seats;
        }
    }

    /// What a relay lacks of the stage being followed.
    fn catch_up(
        &self,
        relay: u32,
        after: Option<(StageId, u32)>,
        live: Option<&LiveServer>,
        now: Millis,
        out: &mut HubOut,
    ) {
        let Some(stage) = self.coalescer.stage() else {
            return;
        };
        let kept: Vec<&(u32, Vec<u8>)> = self.coalescer.kept().collect();
        let last = self.coalescer.last();
        if let Some((s, a)) = after
            && s == stage
            && a <= last
            && kept.first().map_or(a == last, |(n, _)| *n <= a + 1)
        {
            for (n, b) in &kept {
                if *n > a {
                    out.to_relay.push((relay, HubSend::Binary(b.clone())));
                }
            }
            return;
        }
        let first = kept
            .first()
            .map_or(last + 1, |(n, _)| *n)
            .max((last + 1).saturating_sub(self.rules.snapshot_frames));
        if let Some(snap) = stage_of(live, stage).map(|s| {
            let mut snap = s.snapshot(first, now);
            snap.first_frame = first;
            snap
        }) {
            out.one(relay, RelayDown::Snapshot(Box::new(snap)));
        }
        for (n, b) in &kept {
            if *n >= first {
                out.to_relay.push((relay, HubSend::Binary(b.clone())));
            }
        }
    }

    /// A relay's socket closed: its seats are no longer counted (they come
    /// back with its hello).
    pub fn relay_gone(&mut self, relay: u32) {
        self.relays.remove(&relay);
    }

    /// A member asks to open or close the audience: an operator's
    /// (`Capability::CloseAudience`: the owner, a co-host; the owner's
    /// alone before round B1). Anyone else is passed over.
    pub fn set_open(&mut self, room: &RoomState, from: ParticipantId, open: bool) -> HubOut {
        let mut out = HubOut::default();
        let may = room.member(from).is_some_and(|m| {
            !m.left
                && !m.is_cpu()
                && can(m.role, m.admin, Capability::CloseAudience, room.phase.kind())
        });
        if !may || open == self.open || self.ended {
            return out;
        }
        self.open = open;
        out.durable = Some(self.durable());
        out.all(self.admission());
        if !open {
            out.all(RelayDown::Ended {
                end: AudienceEnd::Closed,
            });
        }
        // Said to the members at once (not at the count's period).
        self.told = Some((self.count(), self.open));
        self.told_at = None;
        out.to_members = Some(ServerLive::Audience {
            count: self.count(),
            open: self.open,
        });
        out
    }

    /// When the host should call [`AudienceHub::tick`] next.
    pub fn next_wake(&self) -> Option<Millis> {
        let view = self.view_due.as_ref().map(|_| {
            self.view_at
                .map_or(Millis(0), |t| t.after(self.rules.view_ms))
        });
        let count = (self.told != Some((self.count(), self.open))).then(|| {
            self.told_at
                .map_or(Millis(0), |t| t.after(self.rules.count_ms))
        });
        [self.coalescer.next_wake(), view, count]
            .into_iter()
            .flatten()
            .min()
    }

    /// Time passes (the room's alarm, and after every change of the room).
    pub fn tick(
        &mut self,
        room: &RoomState,
        name: &str,
        live: Option<&LiveServer>,
        now: Millis,
    ) -> HubOut {
        let mut out = HubOut::default();
        if self.ended {
            return out;
        }
        // The view, when what it shows changed.
        if self.derived_at != Some(room.revision) {
            self.derived_at = Some(room.revision);
            let v = PublicView::of(room, name);
            let changed = self.view.as_ref().is_none_or(|old| !old.same_as(&v));
            if changed {
                self.view_due = Some(v);
            }
        }
        if self.view_due.is_some()
            && self
                .view_at
                .is_none_or(|t| now.0 >= t.0 + self.rules.view_ms)
        {
            let v = self.view_due.take().expect("checked");
            out.all(RelayDown::View(Box::new(v.clone())));
            self.view = Some(v);
            self.view_at = Some(now);
        }
        // The count, when it changed.
        let count = self.count();
        if self.told != Some((count, self.open))
            && self
                .told_at
                .is_none_or(|t| now.0 >= t.0 + self.rules.count_ms)
        {
            self.told = Some((count, self.open));
            self.told_at = Some(now);
            out.to_members = Some(ServerLive::Audience {
                count,
                open: self.open,
            });
            out.all(RelayDown::Count { count });
        }
        // The frames.
        for c in self.coalescer.tick(now, Some(count)) {
            match c {
                Coalesced::Stage(stage) => {
                    if let Some(s) = stage_of(live, stage) {
                        let mut snap = s.snapshot(1, now);
                        snap.first_frame = 1;
                        out.all(RelayDown::Snapshot(Box::new(snap)));
                    }
                }
                Coalesced::Frame(_, bytes) => out.to_relays.push(HubSend::Binary(bytes)),
            }
        }
        if room.closed.is_some() {
            self.ended = true;
            out.all(RelayDown::Ended {
                end: AudienceEnd::RoomClosed,
            });
        }
        out
    }
}

fn stage_of(live: Option<&LiveServer>, stage: StageId) -> Option<&LiveStage> {
    let live = live?;
    [live.stage(), live.last_stage()]
        .into_iter()
        .flatten()
        .find(|s| s.id() == stage)
}
