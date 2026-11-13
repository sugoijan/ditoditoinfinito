//! The binary form of a [`Batch`] and a [`Frame`]: what travels as a
//! WebSocket binary message (everything else of live play is JSON text).
//! Little-endian base-128 varints, signed ones zigzagged; a moment's time,
//! tick, life, combo and score as differences from the moment before it in
//! the same list (from zero for the first), so a batch or a frame decodes
//! alone. The first byte says what it is and its version ([`BATCH_V1`],
//! [`FRAME_V1`]); a reader refuses anything else, never panics, and never
//! allocates more than the message's length allows.
//!
//! Measured (`tests`): a moment takes about 9 bytes here against about 85
//! as the JSON of the same type.

use crate::attacks::{AttackEvent, BlockReason, CheckpointRecord, Judged, Settling, Targeting};
use crate::ids::{Millis, ParticipantId, StageId};
use crate::live::cpus::{CpuEntry, CpuNews};
use crate::live::stream::{
    Batch, CheckpointNews, Commitment, Frame, FrameEntry, LiveReport, Moment, MomentKind, StreamEnd,
};

/// A batch, version 1.
pub const BATCH_V1: u8 = 0xB1;
/// A frame, version 1.
pub const FRAME_V1: u8 = 0xF1;
/// A device's news of a stage's CPUs (`crate::live::cpus`,
/// `online-cpus-v0`), version 1. Sent only to a room whose policy seats
/// CPUs online: a room that predates it would refuse it (`bad_batch`).
pub const CPUS_V1: u8 = 0xC1;

/// A step down's moment travels as a refusal's kind with its flag (unused
/// before), the rung counted down from this in the attack's place: a
/// reader that predates it reads the refusal of an attack no stage has
/// (no stage numbers its attacks near it) and takes nothing off.
pub const STEP_DOWN_ATTACK: u32 = u32::MAX;

/// Why bytes were not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// Not a message this build reads (its first byte).
    Unknown(u8),
    /// It ended early, or a number ran past its type.
    Truncated,
    Invalid,
    /// Bytes left over after it.
    Trailing,
}

/// What a binary message is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binary {
    Batch(Batch),
    Frame(Frame),
    Cpus(CpuNews),
}

/// Read a binary message.
pub fn decode(bytes: &[u8]) -> Result<Binary, WireError> {
    match bytes.first() {
        Some(&BATCH_V1) => decode_batch(bytes).map(Binary::Batch),
        Some(&FRAME_V1) => decode_frame(bytes).map(Binary::Frame),
        Some(&CPUS_V1) => decode_cpus(bytes).map(Binary::Cpus),
        Some(&b) => Err(WireError::Unknown(b)),
        None => Err(WireError::Truncated),
    }
}

pub fn encode_batch(b: &Batch) -> Vec<u8> {
    let mut w = Writer(Vec::with_capacity(16 + b.moments.len() * 10));
    w.byte(BATCH_V1);
    w.u(b.stage.0);
    w.u(u64::from(b.seq));
    w.i(b.through_us);
    w.moments(&b.moments);
    w.u(b.reports.len() as u64);
    for r in &b.reports {
        w.report(r);
    }
    w.end(b.end);
    w.0
}

pub fn decode_batch(bytes: &[u8]) -> Result<Batch, WireError> {
    let mut r = Reader { b: bytes, at: 0 };
    if r.byte()? != BATCH_V1 {
        return Err(WireError::Invalid);
    }
    let stage = StageId(r.u()?);
    let seq = r.u32()?;
    let through_us = r.i()?;
    let moments = r.moments()?;
    let n = r.count(2)?;
    let mut reports = Vec::with_capacity(n);
    for _ in 0..n {
        reports.push(r.report()?);
    }
    let end = r.end()?;
    r.done()?;
    Ok(Batch {
        stage,
        seq,
        through_us,
        moments,
        reports,
        end,
    })
}

pub fn encode_cpus(n: &CpuNews) -> Vec<u8> {
    let mut w = Writer(Vec::with_capacity(16 + n.entries.len() * 64));
    w.byte(CPUS_V1);
    w.u(n.stage.0);
    w.u(n.entries.len() as u64);
    for e in &n.entries {
        w.u(e.cpu.0);
        w.i(e.through_us);
        w.u(e.reports.len() as u64);
        for r in &e.reports {
            w.report(r);
        }
        w.end(e.end);
    }
    w.0
}

pub fn decode_cpus(bytes: &[u8]) -> Result<CpuNews, WireError> {
    let mut r = Reader { b: bytes, at: 0 };
    if r.byte()? != CPUS_V1 {
        return Err(WireError::Invalid);
    }
    let stage = StageId(r.u()?);
    let n = r.count(3)?;
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        let cpu = ParticipantId(r.u()?);
        let through_us = r.i()?;
        let k = r.count(5)?;
        let mut reports = Vec::with_capacity(k);
        for _ in 0..k {
            reports.push(r.report()?);
        }
        let end = r.end()?;
        entries.push(CpuEntry {
            cpu,
            through_us,
            reports,
            end,
        });
    }
    r.done()?;
    Ok(CpuNews { stage, entries })
}

pub fn encode_frame(f: &Frame) -> Vec<u8> {
    let mut w = Writer(Vec::with_capacity(32 + f.entries.len() * 24));
    w.byte(FRAME_V1);
    w.u(f.stage.0);
    w.u(u64::from(f.n));
    w.u(f.at.0);
    w.u(f.entries.len() as u64);
    for e in &f.entries {
        w.u(e.participant.0);
        w.u(u64::from(e.seq));
        w.i(e.through_us);
        w.moments(&e.moments);
        w.end(e.end);
    }
    w.u(f.commitments.len() as u64);
    for c in &f.commitments {
        w.u(u64::from(c.attack));
        w.u(u64::from(c.checkpoint));
        w.u(u64::from(c.slot));
        w.u(c.sender.0);
        w.u(c.recipient.0);
    }
    w.u(f.checkpoints.len() as u64);
    for c in &f.checkpoints {
        w.u(u64::from(c.k));
        w.i(c.record.at_us);
        w.u(c.record.power.len() as u64);
        for (p, power) in &c.record.power {
            w.u(p.0);
            w.u(u64::from(*power));
        }
        w.u(c.record.events.len() as u64);
        for e in &c.record.events {
            w.event(e);
        }
    }
    match f.settling {
        None => w.byte(0),
        Some(s) => {
            w.byte(1);
            w.u(s.survivor.0);
            w.i(s.last_out_us);
        }
    }
    w.0
}

pub fn decode_frame(bytes: &[u8]) -> Result<Frame, WireError> {
    let mut r = Reader { b: bytes, at: 0 };
    if r.byte()? != FRAME_V1 {
        return Err(WireError::Invalid);
    }
    let stage = StageId(r.u()?);
    let n = r.u32()?;
    let at = Millis(r.u()?);
    let count = r.count(4)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let participant = ParticipantId(r.u()?);
        let seq = r.u32()?;
        let through_us = r.i()?;
        let moments = r.moments()?;
        let end = r.end()?;
        entries.push(FrameEntry {
            participant,
            seq,
            through_us,
            moments,
            end,
        });
    }
    let count = r.count(5)?;
    let mut commitments = Vec::with_capacity(count);
    for _ in 0..count {
        commitments.push(Commitment {
            attack: r.u32()?,
            checkpoint: r.u32()?,
            slot: r.u32()?,
            sender: ParticipantId(r.u()?),
            recipient: ParticipantId(r.u()?),
        });
    }
    let count = r.count(4)?;
    let mut checkpoints = Vec::with_capacity(count);
    for _ in 0..count {
        let k = r.u32()?;
        let at_us = r.i()?;
        let np = r.count(2)?;
        let mut power = Vec::with_capacity(np);
        for _ in 0..np {
            power.push((ParticipantId(r.u()?), r.u32()?));
        }
        let ne = r.count(2)?;
        let mut events = Vec::with_capacity(ne);
        for _ in 0..ne {
            events.push(r.event()?);
        }
        checkpoints.push(CheckpointNews {
            k,
            record: CheckpointRecord {
                at_us,
                power,
                events,
            },
        });
    }
    let settling = match r.byte()? {
        0 => None,
        1 => Some(Settling {
            survivor: ParticipantId(r.u()?),
            last_out_us: r.i()?,
        }),
        _ => return Err(WireError::Invalid),
    };
    r.done()?;
    Ok(Frame {
        stage,
        n,
        at,
        entries,
        commitments,
        checkpoints,
        settling,
    })
}

struct Writer(Vec<u8>);

impl Writer {
    fn byte(&mut self, b: u8) {
        self.0.push(b);
    }

    fn u(&mut self, mut x: u64) {
        loop {
            let b = (x & 0x7f) as u8;
            x >>= 7;
            if x == 0 {
                self.0.push(b);
                return;
            }
            self.0.push(b | 0x80);
        }
    }

    fn i(&mut self, x: i64) {
        self.u(((x << 1) ^ (x >> 63)) as u64);
    }

    fn moments(&mut self, ms: &[Moment]) {
        self.u(ms.len() as u64);
        let (mut at, mut tick, mut life, mut combo, mut score) = (0i64, 0i64, 0i64, 0i64, 0i64);
        for m in ms {
            // The kind and its flags in one byte: kind in the low nibble.
            let (kind, flag) = match m.kind {
                MomentKind::Tap { w0, .. } => (0u8, w0),
                MomentKind::HoldEnd { ok } => (1, ok),
                MomentKind::Mine => (2, false),
                MomentKind::Shock => (3, false),
                MomentKind::Boo => (4, false),
                MomentKind::Refused { .. } => (5, false),
                MomentKind::Step { .. } => (5, true),
            };
            self.byte(kind | if flag { 0x10 } else { 0 });
            self.i(i64::from(m.at_ms) - at);
            at = i64::from(m.at_ms);
            self.i(m.tick - tick);
            tick = m.tick;
            self.byte(m.lane);
            match m.kind {
                MomentKind::Tap {
                    tier, offset_ms, ..
                } => {
                    self.byte(tier);
                    self.i(i64::from(offset_ms));
                }
                MomentKind::Refused { attack } => self.u(u64::from(attack)),
                MomentKind::Step { rung } => {
                    self.u(u64::from(STEP_DOWN_ATTACK - u32::from(rung)))
                }
                _ => {}
            }
            self.i(i64::from(m.life_ppm) - life);
            life = i64::from(m.life_ppm);
            self.i(i64::from(m.combo) - combo);
            combo = i64::from(m.combo);
            self.i(m.score.wrapping_sub(score));
            score = m.score;
        }
    }

    fn report(&mut self, r: &LiveReport) {
        self.u(u64::from(r.k));
        self.u(r.judged.len() as u64);
        let mut at = 0i64;
        for j in &r.judged {
            self.i(j.at_us - at);
            at = j.at_us;
            self.byte(j.tier);
        }
        self.u(u64::from(r.life_ppm));
        match r.out_at_us {
            None => self.byte(0),
            Some(at) => {
                self.byte(1);
                self.i(at);
            }
        }
        self.byte(match r.targeting {
            None => 0,
            Some(Targeting::Spread) => 1,
            Some(Targeting::Retaliate) => 2,
            Some(Targeting::Leader) => 3,
        });
    }

    fn end(&mut self, e: Option<StreamEnd>) {
        match e {
            None => self.byte(0),
            Some(StreamEnd::Finished) => self.byte(1),
            Some(StreamEnd::Out { at_us }) => {
                self.byte(2);
                self.i(at_us);
            }
            Some(StreamEnd::Forfeit) => self.byte(3),
            Some(StreamEnd::Dropped) => self.byte(4),
            Some(StreamEnd::NoShow) => self.byte(5),
        }
    }

    fn event(&mut self, e: &AttackEvent) {
        match *e {
            AttackEvent::Charged { participant } => {
                self.byte(0);
                self.u(participant.0);
            }
            AttackEvent::Committed {
                attack,
                sender,
                recipient,
            } => {
                self.byte(1);
                self.u(u64::from(attack));
                self.u(sender.0);
                self.u(recipient.0);
            }
            AttackEvent::Blocked { sender, reason } => {
                self.byte(2);
                self.u(sender.0);
                self.byte(match reason {
                    BlockReason::NoEligible => 0,
                    BlockReason::NoSafeSection => 1,
                });
            }
            AttackEvent::Cleared { attack } => {
                self.byte(3);
                self.u(u64::from(attack));
            }
            AttackEvent::Recovered { attack } => {
                self.byte(4);
                self.u(u64::from(attack));
            }
            AttackEvent::Expired { attack } => {
                self.byte(5);
                self.u(u64::from(attack));
            }
            AttackEvent::Retargeted {
                participant,
                targeting,
            } => {
                self.byte(6);
                self.u(participant.0);
                self.byte(match targeting {
                    Targeting::Spread => 0,
                    Targeting::Retaliate => 1,
                    Targeting::Leader => 2,
                });
            }
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, WireError> {
        let b = *self.b.get(self.at).ok_or(WireError::Truncated)?;
        self.at += 1;
        Ok(b)
    }

    fn u(&mut self) -> Result<u64, WireError> {
        let mut x = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte()?;
            let part = u64::from(b & 0x7f);
            if shift == 63 && part > 1 {
                return Err(WireError::Truncated);
            }
            x |= part << shift;
            if b & 0x80 == 0 {
                return Ok(x);
            }
        }
        Err(WireError::Truncated)
    }

    fn i(&mut self) -> Result<i64, WireError> {
        let z = self.u()?;
        Ok(((z >> 1) as i64) ^ -((z & 1) as i64))
    }

    fn u32(&mut self) -> Result<u32, WireError> {
        u32::try_from(self.u()?).map_err(|_| WireError::Invalid)
    }

    /// A count of things each at least `min` bytes long: never more than
    /// the rest of the message could hold.
    fn count(&mut self, min: usize) -> Result<usize, WireError> {
        let n = usize::try_from(self.u()?).map_err(|_| WireError::Invalid)?;
        if n.saturating_mul(min.max(1)) > self.b.len() - self.at {
            return Err(WireError::Truncated);
        }
        Ok(n)
    }

    fn done(&self) -> Result<(), WireError> {
        if self.at == self.b.len() {
            Ok(())
        } else {
            Err(WireError::Trailing)
        }
    }

    fn moments(&mut self) -> Result<Vec<Moment>, WireError> {
        let n = self.count(7)?;
        let mut out = Vec::with_capacity(n);
        let (mut at, mut tick, mut life, mut combo, mut score) = (0i64, 0i64, 0i64, 0i64, 0i64);
        for _ in 0..n {
            let head = self.byte()?;
            let flag = head & 0x10 != 0;
            at = at.checked_add(self.i()?).ok_or(WireError::Invalid)?;
            tick = tick.checked_add(self.i()?).ok_or(WireError::Invalid)?;
            let lane = self.byte()?;
            let kind = match head & 0x0f {
                0 => {
                    let tier = self.byte()?;
                    let offset_ms = i16::try_from(self.i()?).map_err(|_| WireError::Invalid)?;
                    MomentKind::Tap {
                        tier,
                        offset_ms,
                        w0: flag,
                    }
                }
                1 => MomentKind::HoldEnd { ok: flag },
                2 => MomentKind::Mine,
                3 => MomentKind::Shock,
                4 => MomentKind::Boo,
                5 if flag => {
                    let rung = STEP_DOWN_ATTACK
                        .checked_sub(self.u32()?)
                        .ok_or(WireError::Invalid)?;
                    MomentKind::Step {
                        rung: u8::try_from(rung).map_err(|_| WireError::Invalid)?,
                    }
                }
                5 => MomentKind::Refused {
                    attack: self.u32()?,
                },
                _ => return Err(WireError::Invalid),
            };
            life = life.checked_add(self.i()?).ok_or(WireError::Invalid)?;
            combo = combo.checked_add(self.i()?).ok_or(WireError::Invalid)?;
            score = score.wrapping_add(self.i()?);
            out.push(Moment {
                at_ms: i32::try_from(at).map_err(|_| WireError::Invalid)?,
                tick,
                lane,
                kind,
                life_ppm: u32::try_from(life).map_err(|_| WireError::Invalid)?,
                combo: u32::try_from(combo).map_err(|_| WireError::Invalid)?,
                score,
            });
        }
        Ok(out)
    }

    fn report(&mut self) -> Result<LiveReport, WireError> {
        let k = self.u32()?;
        let n = self.count(2)?;
        let mut judged = Vec::with_capacity(n);
        let mut at = 0i64;
        for _ in 0..n {
            at = at.checked_add(self.i()?).ok_or(WireError::Invalid)?;
            judged.push(Judged {
                at_us: at,
                tier: self.byte()?,
            });
        }
        let life_ppm = self.u32()?;
        let out_at_us = match self.byte()? {
            0 => None,
            1 => Some(self.i()?),
            _ => return Err(WireError::Invalid),
        };
        let targeting = match self.byte()? {
            0 => None,
            1 => Some(Targeting::Spread),
            2 => Some(Targeting::Retaliate),
            3 => Some(Targeting::Leader),
            _ => return Err(WireError::Invalid),
        };
        Ok(LiveReport {
            k,
            judged,
            life_ppm,
            out_at_us,
            targeting,
        })
    }

    fn end(&mut self) -> Result<Option<StreamEnd>, WireError> {
        Ok(match self.byte()? {
            0 => None,
            1 => Some(StreamEnd::Finished),
            2 => Some(StreamEnd::Out { at_us: self.i()? }),
            3 => Some(StreamEnd::Forfeit),
            4 => Some(StreamEnd::Dropped),
            5 => Some(StreamEnd::NoShow),
            _ => return Err(WireError::Invalid),
        })
    }

    fn event(&mut self) -> Result<AttackEvent, WireError> {
        Ok(match self.byte()? {
            0 => AttackEvent::Charged {
                participant: ParticipantId(self.u()?),
            },
            1 => AttackEvent::Committed {
                attack: self.u32()?,
                sender: ParticipantId(self.u()?),
                recipient: ParticipantId(self.u()?),
            },
            2 => AttackEvent::Blocked {
                sender: ParticipantId(self.u()?),
                reason: match self.byte()? {
                    0 => BlockReason::NoEligible,
                    1 => BlockReason::NoSafeSection,
                    _ => return Err(WireError::Invalid),
                },
            },
            3 => AttackEvent::Cleared {
                attack: self.u32()?,
            },
            4 => AttackEvent::Recovered {
                attack: self.u32()?,
            },
            5 => AttackEvent::Expired {
                attack: self.u32()?,
            },
            6 => AttackEvent::Retargeted {
                participant: ParticipantId(self.u()?),
                targeting: match self.byte()? {
                    0 => Targeting::Spread,
                    1 => Targeting::Retaliate,
                    2 => Targeting::Leader,
                    _ => return Err(WireError::Invalid),
                },
            },
            _ => return Err(WireError::Invalid),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A plausible moment stream: a tap every `gap_ms`, offsets and tiers
    /// varying, a hold end and a shock now and then, life and score
    /// moving.
    pub(crate) fn moments(n: usize, gap_ms: i32, seed: u64) -> Vec<Moment> {
        let mut x = seed;
        let mut next = || {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (x >> 33) as i64
        };
        let (mut life, mut combo, mut score) = (1_000_000i64, 0u32, 0i64);
        (0..n)
            .map(|i| {
                let at_ms = 2_000 + i as i32 * gap_ms;
                let kind = match next() % 20 {
                    0 => MomentKind::HoldEnd {
                        ok: next() % 2 == 0,
                    },
                    1 => MomentKind::Shock,
                    _ => MomentKind::Tap {
                        tier: (next() % 3) as u8,
                        offset_ms: (next() % 61 - 30) as i16,
                        w0: next() % 3 == 0,
                    },
                };
                life = (life + next() % 20_001 - 8_000).clamp(0, 1_000_000);
                combo += 1;
                score += 5_000 + next() % 1_001;
                Moment {
                    at_ms,
                    tick: i64::from(at_ms) * 48 * 150 / 60_000,
                    lane: (next() % 4) as u8,
                    kind,
                    life_ppm: life as u32,
                    combo,
                    score,
                }
            })
            .collect()
    }

    fn judged(k: u32, n: usize) -> LiveReport {
        LiveReport {
            k,
            judged: (0..n)
                .map(|i| Judged {
                    at_us: i64::from(k) * 1_600_000 + i as i64 * 133_333,
                    tier: (i % 3) as u8,
                })
                .collect(),
            life_ppm: 987_654,
            out_at_us: (k == 3).then_some(4_000_123),
            targeting: Some(Targeting::Leader),
        }
    }

    #[test]
    fn a_batch_reads_back_equal() {
        let b = Batch {
            stage: StageId(12),
            seq: 345,
            through_us: 61_234_567,
            moments: moments(40, 83, 1),
            reports: vec![judged(2, 12), judged(3, 0)],
            end: Some(StreamEnd::Out { at_us: 61_000_000 }),
        };
        let bytes = encode_batch(&b);
        assert_eq!(decode(&bytes), Ok(Binary::Batch(b.clone())));
        // Never a panic on anything cut short or changed.
        for n in 0..bytes.len() {
            assert!(decode_batch(&bytes[..n]).is_err());
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(decode_batch(&longer), Err(WireError::Trailing));
        for i in 0..bytes.len() {
            let mut flipped = bytes.clone();
            flipped[i] ^= 0xa5;
            let _ = decode(&flipped);
        }
        assert_eq!(decode(&[0x7b]), Err(WireError::Unknown(0x7b)));
    }

    /// A step down's moment (a match's "At zero life: Step down") reads
    /// back as itself; a reader that predates it reads it as the refusal
    /// of an attack no stage has (a refusal's kind with its flag, the
    /// rung counted down from `STEP_DOWN_ATTACK`), never an error.
    #[test]
    fn a_step_down_reads_back_and_an_older_reader_takes_it_for_nothing() {
        let mut ms = moments(6, 83, 4);
        ms[3].kind = MomentKind::Step { rung: 2 };
        let b = Batch {
            stage: StageId(1),
            seq: 1,
            through_us: 3_000_000,
            moments: ms.clone(),
            reports: Vec::new(),
            end: None,
        };
        let bytes = encode_batch(&b);
        assert_eq!(decode_batch(&bytes), Ok(b));
        // The older reader reads kind 5 as a refusal whatever its flag:
        // the same bytes without the flag, read here.
        let one = Batch {
            stage: StageId(1),
            seq: 1,
            through_us: 0,
            moments: vec![ms[3]],
            reports: Vec::new(),
            end: None,
        };
        let mut bytes = encode_batch(&one);
        // The batch's byte, the stage, the seq, the time, the count: then
        // the moment's head.
        assert_eq!(bytes[5], 0x15);
        bytes[5] = 0x05;
        let older = decode_batch(&bytes).unwrap();
        assert_eq!(
            older.moments[0].kind,
            MomentKind::Refused {
                attack: STEP_DOWN_ATTACK - 2
            }
        );
    }

    #[test]
    fn a_cpus_news_reads_back_equal_and_never_panics() {
        let n = CpuNews {
            stage: StageId(7),
            entries: vec![
                CpuEntry {
                    cpu: ParticipantId(40),
                    through_us: 4_100_000,
                    reports: vec![judged(2, 12), judged(3, 4)],
                    end: Some(StreamEnd::Out { at_us: 4_000_123 }),
                },
                CpuEntry {
                    cpu: ParticipantId(41),
                    through_us: 90_000_000,
                    reports: Vec::new(),
                    end: Some(StreamEnd::Finished),
                },
                CpuEntry {
                    cpu: ParticipantId(42),
                    through_us: -3,
                    reports: vec![judged(0, 30)],
                    end: None,
                },
            ],
        };
        let bytes = encode_cpus(&n);
        assert_eq!(decode(&bytes), Ok(Binary::Cpus(n.clone())));
        for k in 0..bytes.len() {
            assert!(decode_cpus(&bytes[..k]).is_err());
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(decode_cpus(&longer), Err(WireError::Trailing));
        let mut x = 0x9E37_79B9u64;
        for _ in 0..2_000 {
            let mut b = bytes.clone();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let at = 1 + (x as usize) % (b.len() - 1);
            b[at] ^= (x >> 32) as u8 | 1;
            let _ = decode(&b);
        }
        // Sixty CPUs' reports of a checkpoint: well under the bound.
        let many = CpuNews {
            stage: StageId(7),
            entries: (0..60)
                .map(|i| CpuEntry {
                    cpu: ParticipantId(100 + i),
                    through_us: 9_000_000,
                    reports: vec![judged(5, 24)],
                    end: None,
                })
                .collect(),
        };
        assert!(encode_cpus(&many).len() < 16 * 1024, "{}", encode_cpus(&many).len());
    }

    #[test]
    fn a_frame_reads_back_equal() {
        let f = Frame {
            stage: StageId(3),
            n: 77,
            at: Millis(1_731_400_000_123),
            entries: (1..=9)
                .map(|p| FrameEntry {
                    participant: ParticipantId(p),
                    seq: 40 + p as u32,
                    through_us: 30_000_000 + p as i64,
                    moments: moments(3, 83, p),
                    end: (p == 4).then_some(StreamEnd::Forfeit),
                })
                .collect(),
            commitments: vec![Commitment {
                attack: 5,
                checkpoint: 12,
                slot: 16,
                sender: ParticipantId(2),
                recipient: ParticipantId(7),
            }],
            checkpoints: vec![CheckpointNews {
                k: 12,
                record: CheckpointRecord {
                    at_us: 24_000_000,
                    power: (1..=9).map(|p| (ParticipantId(p), p as u32 * 11)).collect(),
                    events: vec![
                        AttackEvent::Charged {
                            participant: ParticipantId(2),
                        },
                        AttackEvent::Committed {
                            attack: 5,
                            sender: ParticipantId(2),
                            recipient: ParticipantId(7),
                        },
                        AttackEvent::Blocked {
                            sender: ParticipantId(3),
                            reason: BlockReason::NoSafeSection,
                        },
                        AttackEvent::Retargeted {
                            participant: ParticipantId(9),
                            targeting: Targeting::Retaliate,
                        },
                        AttackEvent::Expired { attack: 2 },
                    ],
                },
            }],
            settling: Some(Settling {
                survivor: ParticipantId(1),
                last_out_us: 23_500_000,
            }),
        };
        let bytes = encode_frame(&f);
        assert_eq!(decode(&bytes), Ok(Binary::Frame(f.clone())));
        for n in 0..bytes.len() {
            assert!(decode_frame(&bytes[..n]).is_err());
        }
        for i in 0..bytes.len() {
            let mut flipped = bytes.clone();
            flipped[i] ^= 0x5a;
            let _ = decode(&flipped);
        }
    }

    #[test]
    fn a_moment_costs_about_nine_bytes_against_about_eighty_as_json() {
        let ms = moments(1_000, 83, 9);
        let batch = Batch {
            stage: StageId(1),
            seq: 1,
            through_us: 0,
            moments: ms,
            reports: Vec::new(),
            end: None,
        };
        let binary = encode_batch(&batch).len() as f64 / 1_000.0;
        let json = serde_json::to_string(&batch).unwrap().len() as f64 / 1_000.0;
        assert!(binary < 12.0, "{binary} bytes a moment");
        assert!(json > 60.0, "{json} bytes a moment as JSON");
    }
}
