//! The audience's frames: the room's own frames ([`Frame`], built once a
//! tick for its members) coalesced a few a second into one frame of the
//! audience's own numbering, built once and encoded once by the room,
//! forwarded as the same bytes by every relay to every seat.
//!
//! An audience frame is a [`Frame`] (every competitor's news since the
//! last one, merged: moments in order, the latest acknowledgement and
//! judged time, the end; the attacks committed and checkpoints run; the
//! settling) with the audience's count beside it. Its binary form is
//! [`AUDIENCE_FRAME_V0`], the count, then the frame as
//! [`crate::live::wire`] writes it, so a member's reader never takes one
//! for its own and an audience device reads it with the same code.

use crate::ids::{Millis, ParticipantId, StageId};
use crate::live::server::{CompetitorStatus, LiveSnapshot};
use crate::live::stream::{Frame, FrameEntry};
use crate::live::wire::{self, WireError};
use std::collections::{BTreeMap, VecDeque};

/// An audience frame, version 0 (`audience-v0`).
pub const AUDIENCE_FRAME_V0: u8 = 0xA0;

/// One audience frame and the audience's count when it was built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudienceFrame {
    /// How many watch (every relay's seats), where the room knows it.
    pub count: Option<u32>,
    pub frame: Frame,
}

pub fn encode(f: &AudienceFrame) -> Vec<u8> {
    let body = wire::encode_frame(&f.frame);
    let mut out = Vec::with_capacity(body.len() + 6);
    out.push(AUDIENCE_FRAME_V0);
    // The count plus one, zero for none: one varint.
    let mut x = f.count.map_or(0, |c| u64::from(c) + 1);
    loop {
        let b = (x & 0x7f) as u8;
        x >>= 7;
        if x == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
    out.extend(body);
    out
}

pub fn decode(bytes: &[u8]) -> Result<AudienceFrame, WireError> {
    match bytes.first() {
        Some(&AUDIENCE_FRAME_V0) => {}
        Some(&b) => return Err(WireError::Unknown(b)),
        None => return Err(WireError::Truncated),
    }
    let mut at = 1;
    let mut x = 0u64;
    let mut shift = 0;
    loop {
        let b = *bytes.get(at).ok_or(WireError::Truncated)?;
        at += 1;
        if shift >= 35 {
            return Err(WireError::Invalid);
        }
        x |= u64::from(b & 0x7f) << shift;
        shift += 7;
        if b & 0x80 == 0 {
            break;
        }
    }
    let count = match x {
        0 => None,
        n => Some(u32::try_from(n - 1).map_err(|_| WireError::Invalid)?),
    };
    let frame = wire::decode_frame(&bytes[at..])?;
    Ok(AudienceFrame { count, frame })
}

/// `frames` (of one stage, in order) as one frame numbered `n`, built at
/// `at`: each competitor's entries merged (moments appended in order, the
/// last acknowledgement and judged time, an end where any had one), the
/// commitments and checkpoints in order, the last settling said.
pub fn coalesce(stage: StageId, n: u32, at: Millis, frames: &[Frame]) -> Frame {
    let mut entries: BTreeMap<ParticipantId, FrameEntry> = BTreeMap::new();
    let mut commitments = Vec::new();
    let mut checkpoints = Vec::new();
    let mut settling = None;
    for f in frames {
        for e in &f.entries {
            let m = entries.entry(e.participant).or_insert_with(|| FrameEntry {
                participant: e.participant,
                seq: 0,
                through_us: 0,
                moments: Vec::new(),
                end: None,
            });
            m.seq = e.seq;
            m.through_us = e.through_us;
            m.moments.extend(e.moments.iter().copied());
            if e.end.is_some() {
                m.end = e.end;
            }
        }
        commitments.extend(f.commitments.iter().copied());
        checkpoints.extend(f.checkpoints.iter().cloned());
        if f.settling.is_some() {
            settling = f.settling;
        }
    }
    Frame {
        stage,
        n,
        at,
        entries: entries.into_values().collect(),
        commitments,
        checkpoints,
        settling,
    }
}

/// The stage as a snapshot says it, moved on by a frame: what a relay
/// keeps so that a seat arriving mid-stage gets the stage as it stands
/// without asking the room (the same facts the room's own
/// [`LiveSnapshot`] carries).
pub fn fold(snap: &mut LiveSnapshot, f: &Frame) {
    snap.at = snap.at.max(f.at);
    for e in &f.entries {
        let i = match snap
            .competitors
            .binary_search_by_key(&e.participant, |c| c.participant)
        {
            Ok(i) => i,
            Err(i) => {
                snap.competitors.insert(
                    i,
                    CompetitorStatus {
                        participant: e.participant,
                        live: true,
                        seq: 0,
                        through_us: 0,
                        end: None,
                        life_ppm: 1_000_000,
                        combo: 0,
                        score: 0,
                        power: 0,
                    },
                );
                i
            }
        };
        let c = &mut snap.competitors[i];
        c.seq = c.seq.max(e.seq);
        c.through_us = c.through_us.max(e.through_us);
        if e.end.is_some() {
            c.end = e.end;
        }
        if let Some(m) = e.moments.last() {
            c.life_ppm = m.life_ppm;
            c.combo = m.combo;
            c.score = m.score;
        }
    }
    for k in &f.commitments {
        if !snap.commitments.iter().any(|c| c.attack == k.attack) {
            snap.commitments.push(*k);
        }
    }
    for cp in &f.checkpoints {
        for (p, power) in &cp.record.power {
            if let Ok(i) = snap.competitors.binary_search_by_key(p, |c| c.participant) {
                snap.competitors[i].power = *power;
            }
        }
    }
    if f.settling.is_some() {
        snap.settling = f.settling;
    }
}

/// The room's side: the member frames of a stage gathered and coalesced
/// every [`crate::audience::AudienceRules::frame_ms`] into one audience
/// frame, encoded once and kept for the relays' resumes.
#[derive(Clone, Debug)]
pub struct Coalescer {
    frame_ms: u64,
    keep: usize,
    stage: Option<StageId>,
    next_n: u32,
    pending: Vec<Frame>,
    last_at: Option<Millis>,
    kept: VecDeque<(u32, Vec<u8>)>,
}

/// What [`Coalescer::tick`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Coalesced {
    /// A stage begins: its first audience frame follows (a relay folds its
    /// frames onto the stage's snapshot from here).
    Stage(StageId),
    /// An audience frame, encoded.
    Frame(u32, Vec<u8>),
}

impl Coalescer {
    pub fn new(frame_ms: u64, keep: usize) -> Self {
        Self {
            frame_ms,
            keep,
            stage: None,
            next_n: 1,
            pending: Vec::new(),
            last_at: None,
            kept: VecDeque::new(),
        }
    }

    /// The stage whose frames it gathers.
    pub fn stage(&self) -> Option<StageId> {
        self.stage
    }

    /// The last audience frame built.
    pub fn last(&self) -> u32 {
        self.next_n - 1
    }

    /// The frames kept (number, bytes), oldest first.
    pub fn kept(&self) -> impl Iterator<Item = &(u32, Vec<u8>)> {
        self.kept.iter()
    }

    /// A frame the room broadcast to its members (its bytes, as sent).
    /// Anything else is passed over.
    pub fn member_frame(&mut self, bytes: &[u8]) {
        if let Ok(wire::Binary::Frame(f)) = wire::decode(bytes) {
            self.take(f);
        }
    }

    /// The same, decoded already.
    pub fn take(&mut self, f: Frame) {
        if self.stage.is_some_and(|s| f.stage < s) {
            return;
        }
        self.pending.push(f);
    }

    /// Whether an audience frame is due at `now` (news waits and the
    /// period has passed).
    pub fn due(&self, now: Millis) -> bool {
        !self.pending.is_empty()
            && (self.last_at.is_none_or(|t| now.0 >= t.0 + self.frame_ms)
                || self.pending.iter().any(|f| Some(f.stage) != self.stage))
    }

    /// When the next frame may be built, where news waits.
    pub fn next_wake(&self) -> Option<Millis> {
        (!self.pending.is_empty())
            .then(|| self.last_at.map_or(Millis(0), |t| t.after(self.frame_ms)))
    }

    /// The frames due at `now` (each audience frame carrying `count`):
    /// a new stage's mark before its first frame.
    pub fn tick(&mut self, now: Millis, count: Option<u32>) -> Vec<Coalesced> {
        let mut out = Vec::new();
        if !self.due(now) {
            return out;
        }
        let pending = std::mem::take(&mut self.pending);
        let mut i = 0;
        while i < pending.len() {
            let stage = pending[i].stage;
            let j = pending[i..]
                .iter()
                .position(|f| f.stage != stage)
                .map_or(pending.len(), |k| i + k);
            if self.stage != Some(stage) {
                self.stage = Some(stage);
                self.next_n = 1;
                self.kept.clear();
                out.push(Coalesced::Stage(stage));
            }
            let n = self.next_n;
            let frame = coalesce(stage, n, now, &pending[i..j]);
            let bytes = encode(&AudienceFrame { count, frame });
            self.kept.push_back((n, bytes.clone()));
            while self.kept.len() > self.keep {
                self.kept.pop_front();
            }
            self.next_n += 1;
            out.push(Coalesced::Frame(n, bytes));
            i = j;
        }
        self.last_at = Some(now);
        out
    }
}

/// A device's estimate of the room's clock from the frames' own times:
/// each frame says when the room built it (`Frame::at`), so the room's
/// clock is at least that when it arrives. The estimate is the largest
/// `at - arrival` of the last minute: the clocks' offset less the
/// quickest trip down (tens of milliseconds), which an audience device,
/// drawing everything (music too) a second or more behind, never sees.
/// The audience sends no pings.
#[derive(Clone, Debug, Default)]
pub struct ArrivalClock {
    samples: VecDeque<(u64, i64)>,
}

impl ArrivalClock {
    pub const WINDOW_MS: u64 = 60_000;

    /// A frame built at room ms `at` arrived at the device's `local_ms`.
    pub fn sample(&mut self, at: Millis, local_ms: u64) {
        self.samples
            .push_back((local_ms, at.0 as i64 - local_ms as i64));
        while self
            .samples
            .front()
            .is_some_and(|(t, _)| t + Self::WINDOW_MS < local_ms)
        {
            self.samples.pop_front();
        }
    }

    /// The room's ms less the device's, as estimated.
    pub fn offset_ms(&self) -> Option<i64> {
        self.samples.iter().map(|(_, o)| *o).max()
    }

    /// The room's time at the device's `local_ms`.
    pub fn room_ms(&self, local_ms: u64) -> Option<Millis> {
        let o = self.offset_ms()?;
        Some(Millis(local_ms.saturating_add_signed(o)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::stream::{Commitment, FrameReader, Moment, MomentKind, StreamEnd};

    fn moment(at_ms: i32) -> Moment {
        Moment {
            at_ms,
            tick: i64::from(at_ms),
            lane: (at_ms % 4) as u8,
            kind: MomentKind::Tap {
                tier: 0,
                offset_ms: -4,
                w0: true,
            },
            life_ppm: 900_000 + at_ms as u32,
            combo: at_ms as u32,
            score: i64::from(at_ms) * 10,
        }
    }

    fn member_frames(n: u32) -> Vec<Frame> {
        (1..=n)
            .map(|k| Frame {
                stage: StageId(2),
                n: k,
                at: Millis(u64::from(k) * 100),
                entries: (1..=3u64)
                    .filter(|p| (k as u64 + p).is_multiple_of(2) || k == n)
                    .map(|p| FrameEntry {
                        participant: ParticipantId(p),
                        seq: k,
                        through_us: i64::from(k) * 100_000,
                        moments: vec![moment(k as i32 * 100 + p as i32)],
                        end: (k == n && p == 2).then_some(StreamEnd::Out { at_us: 77 }),
                    })
                    .collect(),
                commitments: if k == 3 {
                    vec![Commitment {
                        attack: 1,
                        checkpoint: 2,
                        slot: 4,
                        sender: ParticipantId(1),
                        recipient: ParticipantId(3),
                    }]
                } else {
                    Vec::new()
                },
                checkpoints: Vec::new(),
                settling: None,
            })
            .collect()
    }

    #[test]
    fn coalesced_frames_read_as_the_members_frames_do() {
        let frames = member_frames(10);
        let mut members = FrameReader::new(StageId(2));
        for f in &frames {
            assert!(members.take(f, 0));
        }
        let mut c = Coalescer::new(250, 120);
        let mut audience = FrameReader::new(StageId(2));
        let mut built = 0;
        for f in &frames {
            c.take(f.clone());
            for out in c.tick(f.at, Some(7)) {
                if let Coalesced::Frame(n, bytes) = out {
                    let a = decode(&bytes).unwrap();
                    assert_eq!(a.count, Some(7));
                    assert_eq!(a.frame.n, n);
                    assert!(audience.take(&a.frame, 0));
                    built += 1;
                }
            }
        }
        for out in c.tick(Millis(10_000), None) {
            if let Coalesced::Frame(_, bytes) = out {
                assert!(audience.take(&decode(&bytes).unwrap().frame, 0));
                built += 1;
            }
        }
        // Ten member frames 100 ms apart: four or five audience frames.
        assert!((4..=5).contains(&built), "{built}");
        for p in 1..=3 {
            assert_eq!(
                audience.moments(ParticipantId(p)),
                members.moments(ParticipantId(p))
            );
            assert_eq!(
                audience.end(ParticipantId(p)),
                members.end(ParticipantId(p))
            );
            assert_eq!(
                audience.through_us(ParticipantId(p)),
                members.through_us(ParticipantId(p))
            );
        }
        assert_eq!(audience.commitments(), members.commitments());
    }

    #[test]
    fn an_audience_frame_reads_back_and_never_as_a_members() {
        let frame = coalesce(StageId(2), 9, Millis(1234), &member_frames(6));
        for count in [None, Some(0), Some(20_000), Some(u32::MAX - 1)] {
            let f = AudienceFrame {
                count,
                frame: frame.clone(),
            };
            let bytes = encode(&f);
            assert_eq!(decode(&bytes), Ok(f));
            assert!(
                wire::decode(&bytes).is_err(),
                "a member's reader refuses it"
            );
            for n in 0..bytes.len() {
                assert!(decode(&bytes[..n]).is_err());
            }
            for i in 0..bytes.len() {
                let mut b = bytes.clone();
                b[i] ^= 0x5a;
                let _ = decode(&b);
            }
        }
    }

    #[test]
    fn a_fold_keeps_each_competitors_numbers() {
        let mut snap = LiveSnapshot {
            stage: StageId(2),
            song_zero_at: Millis(0),
            at: Millis(0),
            first_frame: 1,
            competitors: Vec::new(),
            commitments: Vec::new(),
            plan: None,
            settling: None,
        };
        for f in member_frames(10) {
            fold(&mut snap, &f);
        }
        assert_eq!(snap.competitors.len(), 3);
        let two = &snap.competitors[1];
        assert_eq!(two.end, Some(StreamEnd::Out { at_us: 77 }));
        assert_eq!(two.seq, 10);
        assert_eq!(two.combo, 1_002);
        assert_eq!(snap.commitments.len(), 1);
    }

    #[test]
    fn the_arrival_clock_is_the_offset_less_the_quickest_trip() {
        let mut c = ArrivalClock::default();
        // The room is 5 s ahead; trips of 40 to 120 ms.
        for (i, trip) in [120u64, 40, 90, 60].iter().enumerate() {
            let room = 10_000 + i as u64 * 250;
            c.sample(Millis(room), room - 5_000 + trip);
        }
        assert_eq!(c.offset_ms(), Some(5_000 - 40));
    }
}
