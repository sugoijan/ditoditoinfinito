//! What a play's record keeps of it (`docs/plans/play-history.md`): the
//! presses and releases as judged ([`PlayLog`]), judged again by
//! [`replay`] with no clock, device or audio, and the judgements and life
//! the play showed ([`PlayTiming`]). Both have a compact binary form with
//! a version, for storage.
//!
//! Judging does not depend on how often a play is updated
//! ([`crate::judge::Judge::update`]), so a replay that feeds the logged
//! edges at their times and updates once at the end judges exactly what
//! the play did.

use ddi_chart::Song;
use serde::{Deserialize, Serialize};

use crate::judge::JudgeEventKind;
use crate::player::{Judged, PlayOptions, Player, Results, to_microsecond};
use crate::rules::{Judgement, RulesetKey};

/// One press or release as the play judged it.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoggedInput {
    /// The judged song second: after the audio clock, the output latency
    /// and the offsets; to the microsecond.
    pub time: f64,
    pub lane: u8,
    pub pressed: bool,
}

/// A play's presses and releases ([`crate::Player::log`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayLog {
    /// Where the play started: the `song_offset` given to
    /// [`crate::Player::start`] (notes before it are not judged).
    pub start: f64,
    /// In the order judged; their times never go back.
    pub inputs: Vec<LoggedInput>,
}

/// Why a stored log or timing could not be read.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogError {
    /// Nothing to read.
    Empty,
    /// Written by a newer format than this build reads.
    Version(u8),
    /// Ends in the middle of an entry.
    Truncated,
    /// A number too large for its field.
    Overflow,
    /// Bytes after the last entry.
    Trailing,
    /// An entry of a kind this format does not have.
    Kind(u8),
}

impl core::fmt::Display for LogError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LogError::Empty => write!(f, "empty"),
            LogError::Version(v) => write!(f, "format version {v} is not known"),
            LogError::Truncated => write!(f, "cut short"),
            LogError::Overflow => write!(f, "a number out of range"),
            LogError::Trailing => write!(f, "bytes after the end"),
            LogError::Kind(k) => write!(f, "unknown entry kind {k}"),
        }
    }
}

impl std::error::Error for LogError {}

/// The log's binary format.
const LOG_VERSION: u8 = 1;
/// The timing's binary format.
const TIMING_VERSION: u8 = 1;
/// In a log entry, a lane this high is followed by the lane itself.
const WIDE_LANE: u8 = 15;
/// A log entry's bits below its time: the lane (four) and the press.
const ENTRY_BITS: u32 = 5;

fn put_varint(out: &mut Vec<u8>, v: u64) {
    put_wide(out, u128::from(v));
}

fn put_wide(out: &mut Vec<u8>, mut v: u128) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_signed(out: &mut Vec<u8>, v: i64) {
    put_varint(out, ((v << 1) ^ (v >> 63)) as u64);
}

/// Reads what [`put_varint`] and the rest wrote.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, LogError> {
        let b = *self.bytes.get(self.at).ok_or(LogError::Truncated)?;
        self.at += 1;
        Ok(b)
    }

    fn varint(&mut self) -> Result<u64, LogError> {
        u64::try_from(self.wide()?).map_err(|_| LogError::Overflow)
    }

    fn wide(&mut self) -> Result<u128, LogError> {
        let mut v = 0u128;
        for shift in (0..128).step_by(7) {
            let b = self.byte()?;
            let bits = u128::from(b & 0x7f);
            if shift == 126 && bits > 3 {
                return Err(LogError::Overflow);
            }
            v |= bits << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(LogError::Overflow)
    }

    fn signed(&mut self) -> Result<i64, LogError> {
        let v = self.varint()?;
        Ok((v >> 1) as i64 ^ -((v & 1) as i64))
    }

    fn f64(&mut self) -> Result<f64, LogError> {
        let mut b = [0u8; 8];
        for x in &mut b {
            *x = self.byte()?;
        }
        Ok(f64::from_le_bytes(b))
    }

    /// A count of entries of at least `min` bytes each: not more than
    /// the bytes left could hold.
    fn count(&mut self, min: usize) -> Result<usize, LogError> {
        let n = usize::try_from(self.varint()?).map_err(|_| LogError::Overflow)?;
        if n > (self.bytes.len() - self.at) / min.max(1) {
            return Err(LogError::Truncated);
        }
        Ok(n)
    }

    fn end(&self) -> Result<(), LogError> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(LogError::Trailing)
        }
    }
}

/// Microseconds of a song time.
fn micros(seconds: f64) -> i64 {
    (seconds * 1e6).round() as i64
}

fn from_micros(us: i64) -> f64 {
    us as f64 / 1e6
}

impl PlayLog {
    /// The log in its binary form: a version byte, the start (eight bytes,
    /// exact), the number of edges, then per edge one varint holding its
    /// time in microseconds since the edge before (zigzag-signed), above
    /// whether it is a press (a bit) and its lane (four bits; a lane of
    /// 15 or more is the next byte): three bytes an edge when edges come
    /// within about 30 ms of each other, four up to 4 s. Times are kept
    /// to the microsecond, as the player judges them
    /// ([`crate::Player::input`]).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.inputs.len() * 3);
        out.push(LOG_VERSION);
        out.extend_from_slice(&self.start.to_le_bytes());
        put_varint(&mut out, self.inputs.len() as u64);
        let mut last = 0i64;
        for i in &self.inputs {
            let us = micros(i.time);
            let delta = i128::from(us) - i128::from(last);
            last = us;
            let zigzag = ((delta << 1) ^ (delta >> 127)) as u128;
            let low = (u128::from(i.pressed) << 4) | u128::from(i.lane.min(WIDE_LANE));
            put_wide(&mut out, (zigzag << ENTRY_BITS) | low);
            if i.lane >= WIDE_LANE {
                out.push(i.lane);
            }
        }
        out
    }

    /// Reads [`PlayLog::encode`]'s form (every version so far).
    pub fn decode(bytes: &[u8]) -> Result<PlayLog, LogError> {
        let mut r = Reader { bytes, at: 0 };
        let version = r.byte().map_err(|_| LogError::Empty)?;
        if version != LOG_VERSION {
            return Err(LogError::Version(version));
        }
        let start = r.f64()?;
        let n = r.count(1)?;
        let mut inputs = Vec::with_capacity(n);
        let mut last = 0i64;
        for _ in 0..n {
            let v = r.wide()?;
            let lane = match (v & 0x0f) as u8 {
                WIDE_LANE => r.byte()?,
                lane => lane,
            };
            let zigzag = v >> ENTRY_BITS;
            let delta = (zigzag >> 1) as i128 ^ -((zigzag & 1) as i128);
            last = i64::try_from(i128::from(last) + delta).map_err(|_| LogError::Overflow)?;
            inputs.push(LoggedInput {
                time: from_micros(last),
                lane,
                pressed: v & 0x10 != 0,
            });
        }
        r.end()?;
        Ok(PlayLog { start, inputs })
    }

    /// Every judged time moved by `seconds`, to the microsecond: the play
    /// as judged under other offsets. An audio offset raised by `d` judges
    /// every edge `d` earlier, so a play recorded with audio offset `a`
    /// shifted by `a − b` is the same presses played with offset `b`. An
    /// edge the play judged later than it happened
    /// ([`Results::late_edges`]) keeps the time it was judged at, moved
    /// like the rest: the time it happened is not known.
    pub fn shifted(&self, seconds: f64) -> PlayLog {
        PlayLog {
            start: self.start,
            inputs: self
                .inputs
                .iter()
                .map(|i| LoggedInput {
                    time: to_microsecond(i.time + seconds),
                    ..*i
                })
                .collect(),
        }
    }
}

/// Judges `log` again: `song`'s chart `chart` under the ruleset `rules`
/// builds for it, with `options` (their clock options aside: the log's
/// times are already judged), each edge at its time and an update at the
/// end. A play recorded to its end gives the same results.
///
/// # Panics
/// If `chart` is out of range.
pub fn replay(
    song: &Song,
    chart: usize,
    rules: &RulesetKey,
    options: &PlayOptions,
    log: &PlayLog,
) -> Results {
    replay_with(song, chart, rules.build(&song.charts[chart]), options, log)
}

/// [`replay`] under a ruleset at hand (one tuned beyond what a
/// [`RulesetKey`] keeps, say).
///
/// # Panics
/// If `chart` is out of range.
pub fn replay_with(
    song: &Song,
    chart: usize,
    ruleset: crate::rules::Ruleset,
    options: &PlayOptions,
    log: &PlayLog,
) -> Results {
    let mut player = Player::new(song, chart, ruleset, *options);
    player.start(0.0, log.start);
    for i in &log.inputs {
        player.input_at(i.lane, i.pressed, i.time);
    }
    let last = log.inputs.last().map_or(log.start, |i| i.time);
    player.update_at(last.max(player.song_end_time()));
    player.results()
}

/// The judgements and the life a play showed ([`Results::timeline`] and
/// [`Results::life`]), as a record keeps them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayTiming {
    pub timeline: Vec<Judged>,
    pub life: Vec<[f32; 2]>,
}

/// A float's bits in an order that follows its value, so that a small
/// step is a small number.
fn ordered(f: f32) -> i64 {
    let b = f.to_bits();
    i64::from(if b & 0x8000_0000 != 0 {
        !b
    } else {
        b | 0x8000_0000
    })
}

fn from_ordered(v: i64) -> Result<f32, LogError> {
    let b = u32::try_from(v).map_err(|_| LogError::Overflow)?;
    Ok(f32::from_bits(if b & 0x8000_0000 != 0 {
        b & 0x7fff_ffff
    } else {
        !b
    }))
}

/// A tier's or an event's kind in a timing entry.
const HELD: u8 = 6;
const LET_GO: u8 = 7;
const HIT_MINE: u8 = 8;
const AVOID_SHOCK: u8 = 9;
const HIT_SHOCK: u8 = 10;
const BOO: u8 = 11;
const CHECKPOINT: u8 = 12;

impl PlayTiming {
    pub fn from_results(results: &Results) -> PlayTiming {
        PlayTiming {
            timeline: results.timeline.clone(),
            life: results.life.clone(),
        }
    }

    /// The binary form, exact: a version byte; the number of judgements,
    /// then per judgement its time (the step from the one before in the
    /// float's own precision, a signed varint), its lane, a byte of its
    /// kind (a tap's tier, or the event) and for a tap its offset in
    /// microseconds (a signed varint; a checkpoint row its counts and
    /// lanes); then the life's changes, time and life each as a step the
    /// same way. About seven bytes a judgement.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.timeline.len() * 7 + self.life.len() * 5);
        out.push(TIMING_VERSION);
        put_varint(&mut out, self.timeline.len() as u64);
        let mut last = ordered(0.0);
        for j in &self.timeline {
            let t = ordered(j.time);
            put_signed(&mut out, t - last);
            last = t;
            out.push(j.lane);
            match j.kind {
                JudgeEventKind::Tap(tier, delta) => {
                    out.push(tier.index() as u8);
                    put_signed(&mut out, micros(delta));
                }
                JudgeEventKind::Held => out.push(HELD),
                JudgeEventKind::LetGo => out.push(LET_GO),
                JudgeEventKind::HitMine => out.push(HIT_MINE),
                JudgeEventKind::AvoidShock => out.push(AVOID_SHOCK),
                JudgeEventKind::HitShock => out.push(HIT_SHOCK),
                JudgeEventKind::Boo => out.push(BOO),
                JudgeEventKind::Checkpoint {
                    held,
                    missed,
                    lanes,
                } => {
                    out.push(CHECKPOINT);
                    put_varint(&mut out, u64::from(held));
                    put_varint(&mut out, u64::from(missed));
                    put_varint(&mut out, lanes);
                }
            }
        }
        put_varint(&mut out, self.life.len() as u64);
        let (mut last_t, mut last_l) = (ordered(0.0), ordered(0.0));
        for &[t, l] in &self.life {
            let (t, l) = (ordered(t), ordered(l));
            put_signed(&mut out, t - last_t);
            put_signed(&mut out, l - last_l);
            (last_t, last_l) = (t, l);
        }
        out
    }

    /// Reads [`PlayTiming::encode`]'s form (every version so far).
    pub fn decode(bytes: &[u8]) -> Result<PlayTiming, LogError> {
        let mut r = Reader { bytes, at: 0 };
        let version = r.byte().map_err(|_| LogError::Empty)?;
        if version != TIMING_VERSION {
            return Err(LogError::Version(version));
        }
        let n = r.count(3)?;
        let mut timeline = Vec::with_capacity(n);
        let mut last = ordered(0.0);
        for _ in 0..n {
            last = last.checked_add(r.signed()?).ok_or(LogError::Overflow)?;
            let time = from_ordered(last)?;
            let lane = r.byte()?;
            let kind = match r.byte()? {
                tier @ 0..=5 => JudgeEventKind::Tap(
                    Judgement::from_index(usize::from(tier)).ok_or(LogError::Kind(tier))?,
                    from_micros(r.signed()?),
                ),
                HELD => JudgeEventKind::Held,
                LET_GO => JudgeEventKind::LetGo,
                HIT_MINE => JudgeEventKind::HitMine,
                AVOID_SHOCK => JudgeEventKind::AvoidShock,
                HIT_SHOCK => JudgeEventKind::HitShock,
                BOO => JudgeEventKind::Boo,
                CHECKPOINT => {
                    let small = |v: u64| u16::try_from(v).map_err(|_| LogError::Overflow);
                    JudgeEventKind::Checkpoint {
                        held: small(r.varint()?)?,
                        missed: small(r.varint()?)?,
                        lanes: r.varint()?,
                    }
                }
                other => return Err(LogError::Kind(other)),
            };
            timeline.push(Judged { time, lane, kind });
        }
        let n = r.count(2)?;
        let mut life = Vec::with_capacity(n);
        let (mut last_t, mut last_l) = (ordered(0.0), ordered(0.0));
        for _ in 0..n {
            last_t = last_t.checked_add(r.signed()?).ok_or(LogError::Overflow)?;
            last_l = last_l.checked_add(r.signed()?).ok_or(LogError::Overflow)?;
            life.push([from_ordered(last_t)?, from_ordered(last_l)?]);
        }
        r.end()?;
        Ok(PlayTiming { timeline, life })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_round_trip_at_their_edges() {
        for v in [
            0i64,
            1,
            -1,
            63,
            -64,
            64,
            1 << 40,
            -(1 << 40),
            i64::MAX,
            i64::MIN,
        ] {
            let mut out = Vec::new();
            put_signed(&mut out, v);
            let mut r = Reader { bytes: &out, at: 0 };
            assert_eq!(r.signed(), Ok(v));
            assert!(r.end().is_ok());
        }
        // Twenty bytes of continuation: too long.
        let mut r = Reader {
            bytes: &[0xff; 20],
            at: 0,
        };
        assert_eq!(r.varint(), Err(LogError::Overflow));
        // Past 64 bits for a count.
        let mut out = Vec::new();
        put_wide(&mut out, u128::from(u64::MAX) + 1);
        let mut r = Reader { bytes: &out, at: 0 };
        assert_eq!(r.varint(), Err(LogError::Overflow));
    }

    #[test]
    fn ordered_float_bits_follow_the_value() {
        let values = [-3.5f32, -0.25, -0.0, 0.0, 1e-30, 0.18, 1.0, 120.5];
        for w in values.windows(2) {
            assert!(ordered(w[0]) <= ordered(w[1]), "{w:?}");
        }
        for v in values {
            assert_eq!(from_ordered(ordered(v)).unwrap().to_bits(), v.to_bits());
        }
    }

    #[test]
    fn logs_round_trip_exactly() {
        let log = PlayLog {
            start: 12.345_678_901,
            inputs: vec![
                LoggedInput {
                    time: -0.5,
                    lane: 0,
                    pressed: true,
                },
                LoggedInput {
                    time: to_microsecond(0.123_456_7),
                    lane: 3,
                    pressed: false,
                },
                LoggedInput {
                    time: 3_600.000_001,
                    lane: 200,
                    pressed: true,
                },
            ],
        };
        let bytes = log.encode();
        assert_eq!(PlayLog::decode(&bytes), Ok(log.clone()));
        assert_eq!(PlayLog::decode(&[]), Err(LogError::Empty));
        assert_eq!(PlayLog::decode(&[9]), Err(LogError::Version(9)));
        assert_eq!(
            PlayLog::decode(&bytes[..bytes.len() - 1]),
            Err(LogError::Truncated)
        );
        let mut more = bytes.clone();
        more.push(0);
        assert_eq!(PlayLog::decode(&more), Err(LogError::Trailing));
        // A count past what the bytes could hold is refused at once.
        let mut huge = vec![LOG_VERSION];
        huge.extend_from_slice(&0.0f64.to_le_bytes());
        put_varint(&mut huge, u64::MAX >> 1);
        assert_eq!(PlayLog::decode(&huge), Err(LogError::Truncated));
        // Shifted: to the microsecond.
        let s = log.shifted(-0.000_000_4);
        assert_eq!(s.inputs[1].time, log.inputs[1].time);
        assert_eq!(s.inputs[0].time, -0.5);
    }

    #[test]
    fn timings_round_trip_exactly() {
        let t = PlayTiming {
            timeline: vec![
                Judged {
                    time: 0.0123,
                    lane: 1,
                    kind: JudgeEventKind::Tap(Judgement::W2, to_microsecond(-0.031_234_56)),
                },
                Judged {
                    time: 0.0123,
                    lane: 2,
                    kind: JudgeEventKind::Tap(Judgement::Miss, to_microsecond(0.181_5)),
                },
                Judged {
                    time: 7.25,
                    lane: 0,
                    kind: JudgeEventKind::Checkpoint {
                        held: 3,
                        missed: 1,
                        lanes: u64::MAX,
                    },
                },
                Judged {
                    time: 5.0,
                    lane: 9,
                    kind: JudgeEventKind::HitShock,
                },
            ],
            life: vec![[0.0, 0.5], [0.0123, 0.4917], [7.25, 1.0]],
        };
        let bytes = t.encode();
        assert_eq!(PlayTiming::decode(&bytes), Ok(t.clone()));
        // One judgement at 0 in lane 0 of a kind there is not.
        let bad = [TIMING_VERSION, 1, 0, 0, 99, 0];
        assert_eq!(PlayTiming::decode(&bad), Err(LogError::Kind(99)));
        assert_eq!(
            PlayTiming::decode(&bytes[..bytes.len() - 1]),
            Err(LogError::Truncated)
        );
    }
}
