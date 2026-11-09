// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! StepMania's No Stretch regions, extended to ongoing holds/rolls and lifts.
//! Source: NoteDataUtil::g_ValidRows at 825467b (dance/pump double). The
//! half-double middle-six region is already one Pump mask. For 9A, project
//! both arrow banks onto dance double and allow Oni at either pad's center.
//! These are logical span constraints, not a certificate of anatomical reach.
use super::{AtOnce, Layout, Note, NoteKind};
use ddi_chart::Tick;

pub(super) fn regions(layout: &Layout) -> Option<&'static [u16]> {
    let mat = ddi_dance_mat::MatLayout::from_id(&layout.id)?;
    (mat.lane_count() == layout.lane_count())
        .then(|| mat.no_stretch_regions())
        .flatten()
}
fn contact(n: &Note) -> bool {
    matches!(n.kind, NoteKind::Tap | NoteKind::Lift) || n.kind.is_long()
}
fn bit(lane: u8) -> u16 {
    1u16.checked_shl(u32::from(lane)).unwrap_or(u16::MAX)
}
fn fits(required: u16, regions: &[u16]) -> bool {
    regions.iter().any(|mask| required & !mask == 0)
}

pub(super) fn remove(notes: &mut Vec<Note>, regions: &[u16], at_once: AtOnce) {
    let max = match at_once {
        AtOnce::Any => usize::MAX,
        AtOnce::Two => 2,
        AtOnce::One => 1,
    };
    let mut removed = vec![false; notes.len()];
    // Hazard/visual rows must not erase an earlier hold's occupancy.
    let mut holds: [Option<Tick>; 10] = [None; 10];
    let mut start = 0;
    while start < notes.len() {
        let tick = notes[start].tick;
        let end = start + notes[start..].partition_point(|n| n.tick == tick);
        let mut held = 0;
        let mut held_mask = 0;
        for (lane, until) in holds.iter_mut().enumerate() {
            if until.is_some_and(|t| t >= tick) {
                held += 1;
                held_mask |= bit(lane as u8);
            } else {
                *until = None;
            }
        }
        loop {
            let mut count = held;
            let mut mask = held_mask;
            for k in start..end {
                if !removed[k] && contact(&notes[k]) {
                    count += 1;
                    mask |= bit(notes[k].lane);
                }
            }
            let stretched = !fits(mask, regions);
            if !stretched && count <= max {
                break;
            }
            // Existing holds are immutable. Remove leftmost new contacts;
            // Prefer taps/heads for count reductions, then excess lifts for
            // Up to two. No Stretch can also remove an unreachable lift.
            // One retains legacy lift protection. Hazards/visuals survive.
            let Some(k) = (start..end)
                .find(|&k| {
                    !removed[k]
                        && contact(&notes[k])
                        && (stretched || notes[k].kind != NoteKind::Lift)
                })
                .or_else(|| {
                    if at_once == AtOnce::Two {
                        (start..end).find(|&k| !removed[k] && notes[k].kind == NoteKind::Lift)
                    } else {
                        None
                    }
                })
            else {
                break;
            };
            removed[k] = true;
        }
        for k in start..end {
            if !removed[k]
                && let Some(until) = notes[k].kind.hold_end()
                && let Some(slot) = holds.get_mut(usize::from(notes[k].lane))
            {
                *slot = Some(slot.map_or(until, |old| old.max(until)));
            }
        }
        start = end;
    }
    let mut k = 0;
    notes.retain(|_| {
        let keep = !removed[k];
        k += 1;
        keep
    });
}
