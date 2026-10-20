//! The autoplayer's press script.
//!
//! Presses at each judgeable note's judged second: taps briefly, lifts
//! released on the note, holds kept to the tail, rolls re-tapped; every
//! press let go before the lane's next press or mine, and no re-tap near a
//! mine, as StepMania's autoplayer never steps on one. Built
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
/// No roll re-tap this close to a mine in the lane: beyond the widest mine
/// window (0.09 s), and short enough that the taps before a mine just past
/// a roll's tail keep it alive under the shortest roll window (DDR A's
/// 0.25 s: at most an interval and this without a tap).
pub const ROLL_MINE_CLEARANCE: f64 = 0.1;
/// Roll taps closer together than this are merged (each is held 0.04 s).
const ROLL_TAP_GAP: f64 = 0.06;
/// A press is let go at least this long before the lane's next press or
/// mine.
const RELEASE_GAP: f64 = 0.002;

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
    // Judged times of the press-judged notes and of the mines per lane.
    let lanes = notes
        .iter()
        .map(|n| usize::from(n.note.lane) + 1)
        .max()
        .unwrap_or(0);
    let mut lane_times: Vec<Vec<f64>> = vec![Vec::new(); lanes];
    let mut mine_times: Vec<Vec<f64>> = vec![Vec::new(); lanes];
    for n in notes.iter().filter(|n| n.judgeable) {
        let lane = usize::from(n.note.lane);
        match n.note.kind {
            NoteKind::Tap
            | NoteKind::HoldHead { .. }
            | NoteKind::RollHead { .. }
            | NoteKind::Lift => {
                lane_times[lane].push(n.seconds + bias);
            }
            NoteKind::Mine | NoteKind::Shock => mine_times[lane].push(n.seconds + bias),
            _ => {}
        }
    }
    for times in lane_times.iter_mut().chain(&mut mine_times) {
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
                    .find(|&&s| s > t)
                    .map_or(f64::INFINITY, |&s| s - ROLL_TAP_STOP);
                let last = (n.end_seconds + bias).min(next);
                // Taps on a grid, none near a mine in the lane, and one
                // each side of such a mine instead, so the roll is never
                // left longer than an interval and twice the clearance.
                let mines = &mine_times[usize::from(lane)];
                let clear = |k: f64| !mines.iter().any(|m| (m - k).abs() < ROLL_MINE_CLEARANCE);
                let beside: Vec<f64> = mines
                    .iter()
                    .filter(|&&m| m > t && m < last)
                    .flat_map(|&m| [m - ROLL_MINE_CLEARANCE, m + ROLL_MINE_CLEARANCE])
                    .filter(|&k| k > t && k < last && clear(k))
                    .collect();
                let mut taps: Vec<f64> = std::iter::successors(Some(t + ROLL_TAP_INTERVAL), |k| {
                    Some(k + ROLL_TAP_INTERVAL)
                })
                .take_while(|&k| k < last)
                .filter(|&k| clear(k) && beside.iter().all(|b| (b - k).abs() >= ROLL_TAP_GAP))
                .collect();
                taps.extend(beside);
                taps.sort_by(f64::total_cmp);
                push(t, t + 0.04);
                let mut previous = t;
                for k in taps {
                    if k - previous >= ROLL_TAP_GAP {
                        push(k, k + 0.04);
                        previous = k;
                    }
                }
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| a.press.total_cmp(&b.press));
    // Let go before the lane's next press (a tap's release would drop a
    // hold that starts during it) and before its next mine.
    for i in 0..out.len() {
        let Press { press, lane, .. } = out[i];
        let next_press = out[i + 1..]
            .iter()
            .find(|p| p.lane == lane)
            .map_or(f64::INFINITY, |p| p.press);
        let next_mine = mine_times[usize::from(lane)]
            .iter()
            .find(|&&m| m > press)
            .copied()
            .unwrap_or(f64::INFINITY);
        let limit = next_press.min(next_mine);
        let release = &mut out[i].release;
        if *release > limit - RELEASE_GAP {
            *release = (limit - RELEASE_GAP).max(press + (limit - press) / 2.0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::{Judge, JudgeEventKind};
    use crate::rules::presets;
    use ddi_chart::{Note, Tick, TimingMap};

    fn presses(notes: &[Note]) -> Vec<Press> {
        let judge = Judge::new(notes, &TimingMap::constant(60.0, 0.0), presets::itg().judge);
        script(judge.notes(), 0.0)
    }

    #[test]
    fn a_press_lets_go_before_the_lanes_next_press_or_mine() {
        // A tap a 48th before a hold (21 ms at 60 BPM), a mine a 48th
        // after the hold's tail.
        let p = presses(&[
            Note::new(Tick(48), 0, NoteKind::Tap),
            Note::new(Tick(49), 0, NoteKind::HoldHead { end: Tick(96) }),
            Note::new(Tick(97), 0, NoteKind::Mine),
        ]);
        assert_eq!(p.len(), 2);
        assert!(p[0].release < p[1].press);
        assert!(p[1].release > 2.0 && p[1].release < 97.0 / 48.0);
    }

    #[test]
    fn a_roll_with_a_mine_in_its_lane_is_kept() {
        // A roll over beats 1 to 4 at 60 BPM and a mine inside it, at
        // every twelfth of beat 2: kept under DDR A's 0.25 s roll window,
        // the mine never stepped on.
        for twelfth in 0..12 {
            let mine = Tick(96 + twelfth * 4);
            let notes = [
                Note::new(Tick(48), 0, NoteKind::RollHead { end: Tick(192) }),
                Note::new(mine, 0, NoteKind::Mine),
            ];
            let timing = TimingMap::constant(60.0, 0.0);
            let mut judge = Judge::new(&notes, &timing, presets::ddr_a().judge);
            let mut events = Vec::new();
            for p in script(judge.notes(), 0.0) {
                events.extend(judge.update(p.press));
                events.extend(judge.press(p.lane, p.press));
                events.extend(judge.update(p.release));
                events.extend(judge.release(p.lane, p.release));
            }
            events.extend(judge.update(5.0));
            let kinds: Vec<_> = events
                .iter()
                .map(|e| e.kind)
                .filter(|k| !matches!(k, JudgeEventKind::Tap(..)))
                .collect();
            assert_eq!(kinds, [JudgeEventKind::Held], "mine at {mine:?}");
        }
    }

    #[test]
    fn roll_taps_keep_clear_of_mines() {
        let p = presses(&[
            Note::new(Tick(48), 1, NoteKind::RollHead { end: Tick(144) }),
            Note::new(Tick(150), 1, NoteKind::Mine),
        ]);
        let mine = 150.0 / 48.0;
        assert!(p.len() > 5);
        assert!(
            p.iter()
                .all(|p| (p.press - mine).abs() >= ROLL_MINE_CLEARANCE)
        );
    }
}
