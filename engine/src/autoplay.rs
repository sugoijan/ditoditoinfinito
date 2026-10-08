//! The autoplayer's press script.
//!
//! Presses at each judgeable note's judged second: taps briefly, lifts
//! released on the note, holds kept to the tail, rolls re-tapped. Built
//! from the judge's (transformed) notes so it presses the lanes actually
//! played. The shell decides when to send each press (it follows the heard
//! audio or the drawn arrows, not the judged timeline); this only says
//! what to press and when, in song seconds.

use ddi_chart::NoteKind;

use crate::judge::JudgedNote;

/// Seconds between the autoplayer's taps on a roll.
pub const ROLL_TAP_INTERVAL: f64 = 0.12;
/// No roll re-tap later than this before the next note in the lane: beyond
/// the widest early window (0.18 s), so the tap cannot hit that note early.
pub const ROLL_TAP_STOP: f64 = 0.2;

/// One scripted press.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Press {
    /// Song seconds.
    pub press: f64,
    /// Song seconds.
    pub release: f64,
    pub lane: u8,
}

/// Presses for `notes`, each shifted by `bias` seconds (positive = late),
/// sorted by press time.
pub fn script(notes: &[JudgedNote], bias: f64) -> Vec<Press> {
    // Judged times of the press-judged notes per lane, for roll re-taps.
    let mut lane_times: Vec<Vec<f64>> = Vec::new();
    for n in notes.iter().filter(|n| n.judgeable) {
        if matches!(
            n.note.kind,
            NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. } | NoteKind::Lift
        ) {
            let lane = usize::from(n.note.lane);
            if lane_times.len() <= lane {
                lane_times.resize(lane + 1, Vec::new());
            }
            lane_times[lane].push(n.seconds);
        }
    }
    for times in &mut lane_times {
        times.sort_by(f64::total_cmp);
    }
    let mut out = Vec::new();
    for n in notes.iter().filter(|n| n.judgeable) {
        let t = n.seconds + bias;
        let lane = n.note.lane;
        let mut push = |press, release| {
            out.push(Press {
                press,
                release,
                lane,
            })
        };
        match n.note.kind {
            NoteKind::Tap => push(t, t + 0.06),
            // Lifts are judged on the release.
            NoteKind::Lift => push(t - 0.05, t),
            NoteKind::HoldHead { .. } => push(t, n.end_seconds + bias + 0.02),
            // Rolls decay unless re-tapped; tap well inside the shortest
            // roll window (DDR A's 0.25 s) up to the tail, but stop early
            // enough before the next note in the lane that a re-tap cannot
            // be judged as an early hit on it.
            NoteKind::RollHead { .. } => {
                let next = lane_times[usize::from(lane)]
                    .iter()
                    .find(|&&s| s > n.seconds)
                    .map_or(f64::INFINITY, |&s| s + bias - ROLL_TAP_STOP);
                let last = (n.end_seconds + bias).min(next);
                push(t, t + 0.04);
                let mut k = t + ROLL_TAP_INTERVAL;
                while k < last {
                    push(k, k + 0.04);
                    k += ROLL_TAP_INTERVAL;
                }
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| a.press.total_cmp(&b.press));
    out
}
