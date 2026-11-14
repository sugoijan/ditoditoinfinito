//! A large field's opponents in a few slots (`docs/plans/multiplayer-prototype-progress.md`,
//! "Rooms of 1+72 players"): which opponents a player sees beside their
//! field when there are more than the side space has readable miniatures
//! for ([`Rivals`]), and the ladder that makes the whole field felt
//! without a field for everyone ([`Ladder`]).
//!
//! Those that matter to the player now get the slots: whoever attacks
//! them or whom they attack (the stage's kept seats,
//! [`crate::opponents::priority_seats`]), then the nearest rivals by
//! standing (still in first, by life, then score), as many above as below.
//! A slot changes hands rarely and calmly: an opponent shown stays for at
//! least [`RivalPolicy::hold`] seconds, and while it is within
//! [`RivalPolicy::margin`] places (and a share of the field) of the
//! nearest it keeps its slot, so two
//! rivals trading places every tick never flicker; one attacking comes in
//! at once. A change fades in over [`RivalPolicy::fade`] seconds (at once
//! under reduced motion). Small rooms (every opponent in a slot of its
//! own) never come here.
//!
//! Pure and deterministic: the same standings at the same times give the
//! same slots, ties broken by seat.

/// An opponent's (or the player's) standing in the stage now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Standing {
    /// Its gauge's life, `0..=1`.
    pub life: f32,
    /// Out of the stage.
    pub out: bool,
    /// What it ranks by after life: the room's score (DDI's sum, a CPU's
    /// its own) or the percent where there is none.
    pub score: f64,
}

/// How slots change hands. PROVISIONAL (`rivals-v0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RivalPolicy {
    /// Seconds an opponent shown keeps its slot at least.
    pub hold: f64,
    /// Places past the nearest rivals an opponent shown may drift and
    /// still keep its slot, and that much more as a share of the field
    /// (a large field's standings move more at once).
    pub margin: usize,
    pub margin_share: f32,
    /// Seconds a slot's new opponent fades in over (none under reduced
    /// motion).
    pub fade: f32,
}

/// `rivals-v0`: eight seconds at least; two places of slack and a
/// twelfth of the field (eight places among 72); a third of a second's
/// fade.
pub const RIVALS_V0: RivalPolicy = RivalPolicy {
    hold: 8.0,
    margin: 2,
    margin_share: 1.0 / 12.0,
    fade: 0.3,
};

/// Whether `a` stands above `b`: still in before out, then life, then
/// score.
fn above(a: &Standing, b: &Standing) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.out, b.out) {
        (false, true) => return Ordering::Less,
        (true, false) => return Ordering::Greater,
        _ => {}
    }
    if !a.out {
        match b.life.total_cmp(&a.life) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    b.score.total_cmp(&a.score)
}

/// The order of `field` (indices), best first; ties by index.
pub fn order(field: &[Standing]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..field.len()).collect();
    idx.sort_by(|&a, &b| above(&field[a], &field[b]).then(a.cmp(&b)));
    idx
}

/// The whole field as the ladder shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ladder {
    /// The player's place among everyone (from 1), or none (a spectator
    /// following nobody).
    pub place: Option<usize>,
    /// Everyone in the stage, the player included.
    pub of: usize,
    /// Those still in.
    pub still_in: usize,
    /// Everyone's life, best first: `(life, out, the player's own)`.
    pub marks: Vec<(f32, bool, bool)>,
}

impl Ladder {
    /// The ladder of `opponents` and the player `me` (none: a spectator).
    pub fn of(opponents: &[Standing], me: Option<Standing>) -> Ladder {
        let mut field: Vec<Standing> = opponents.to_vec();
        if let Some(m) = me {
            field.push(m);
        }
        let mine = me.map(|_| field.len() - 1);
        let ranked = order(&field);
        Ladder {
            place: mine.and_then(|m| ranked.iter().position(|&i| i == m).map(|p| p + 1)),
            of: field.len(),
            still_in: field.iter().filter(|s| !s.out).count(),
            marks: ranked
                .iter()
                .map(|&i| (field[i].life, field[i].out, Some(i) == mine))
                .collect(),
        }
    }
}

/// Which opponents the slots show, kept from frame to frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rivals {
    /// Each slot's seat and the song second it came in.
    slots: Vec<Option<(usize, f64)>>,
}

impl Rivals {
    /// The slots' seats, in slot order (none: an empty slot).
    pub fn seats(&self) -> Vec<Option<usize>> {
        self.slots.iter().map(|s| s.map(|(seat, _)| seat)).collect()
    }

    /// When each slot's opponent came in.
    pub fn since(&self) -> Vec<Option<f64>> {
        self.slots.iter().map(|s| s.map(|(_, t)| t)).collect()
    }

    /// Chooses the `slots` opponents to show among `opponents` at song
    /// second `now`, around the player `me` (none: the top of the field),
    /// those `pinned` first, by `policy`.
    pub fn update(
        &mut self,
        slots: usize,
        opponents: &[Standing],
        me: Option<Standing>,
        pinned: &[usize],
        now: f64,
        policy: RivalPolicy,
    ) {
        let n = opponents.len();
        self.slots.resize(slots, None);
        self.slots.truncate(slots);
        // A seat that is no longer there leaves its slot.
        for s in &mut self.slots {
            if s.is_some_and(|(seat, _)| seat >= n) {
                *s = None;
            }
        }
        if slots == 0 {
            return;
        }
        // Each opponent's distance from the player by standing: the
        // player's place among the opponents (where they would stand).
        let ranked = order(opponents);
        let rank_of = {
            let mut r = vec![0usize; n];
            for (p, &i) in ranked.iter().enumerate() {
                r[i] = p;
            }
            r
        };
        let mine = me.map_or(0, |m| {
            ranked
                .iter()
                .take_while(|&&i| above(&opponents[i], &m).is_le())
                .count()
        });
        // Distance, above before below at the same distance.
        let distance = |seat: usize| -> (usize, bool) {
            let r = rank_of[seat];
            if r < mine {
                (mine - r - 1, false)
            } else {
                (r - mine, true)
            }
        };
        let pins: Vec<usize> = {
            let mut v = Vec::new();
            for &p in pinned {
                if p < n && !v.contains(&p) && v.len() < slots {
                    v.push(p);
                }
            }
            v
        };
        let mut nearest: Vec<usize> = (0..n).filter(|s| !pins.contains(s)).collect();
        nearest.sort_by_key(|&s| (distance(s), s));
        let wanted: Vec<usize> = pins
            .iter()
            .copied()
            .chain(nearest.iter().copied())
            .take(slots)
            .collect();
        // How far an opponent shown may be and still keep its slot.
        let reach = wanted
            .iter()
            .filter(|s| !pins.contains(s))
            .map(|&s| distance(s).0)
            .max()
            .unwrap_or(0)
            + policy.margin
            + (n as f32 * policy.margin_share) as usize;
        // Who keeps their slot: the pinned, those still near enough, and
        // those who came in less than `hold` ago.
        for s in &mut self.slots {
            if let Some((seat, at)) = *s {
                let keep =
                    pins.contains(&seat) || distance(seat).0 <= reach || now - at < policy.hold;
                if !keep {
                    *s = None;
                }
            }
        }
        let shown = |slots: &[Option<(usize, f64)>], seat: usize| {
            slots.iter().any(|s| s.is_some_and(|(x, _)| x == seat))
        };
        // A pinned opponent comes in at once: into a free slot, else in
        // place of the farthest one shown that is not pinned.
        for &p in &pins {
            if shown(&self.slots, p) {
                continue;
            }
            let free = self.slots.iter().position(Option::is_none).or_else(|| {
                self.slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.is_some_and(|(x, _)| !pins.contains(&x)))
                    .max_by_key(|(_, s)| s.map(|(x, _)| (distance(x), x)))
                    .map(|(i, _)| i)
            });
            if let Some(i) = free {
                self.slots[i] = Some((p, now));
            }
        }
        // The free slots to the nearest not shown, in slot order.
        for &w in &wanted {
            if shown(&self.slots, w) {
                continue;
            }
            let Some(i) = self.slots.iter().position(Option::is_none) else {
                break;
            };
            self.slots[i] = Some((w, now));
        }
        // Still free (fewer wanted than slots): the next nearest.
        for &w in &nearest {
            if !self.slots.iter().any(Option::is_none) {
                break;
            }
            if !shown(&self.slots, w)
                && let Some(i) = self.slots.iter().position(Option::is_none)
            {
                self.slots[i] = Some((w, now));
            }
        }
    }

    /// The slots of `fields` showing the chosen seats, each fading in
    /// after a change (at once under `reduced` motion).
    pub fn apply(
        &self,
        fields: &mut crate::opponents::Fields,
        now: f64,
        policy: RivalPolicy,
        reduced: bool,
    ) {
        for (slot, chosen) in fields.slots.iter_mut().zip(&self.slots) {
            match chosen {
                Some((seat, at)) => {
                    slot.seat = *seat;
                    let age = (now - at) as f32;
                    slot.dim = if reduced || policy.fade <= 0.0 || *at <= 0.0 {
                        0.0
                    } else {
                        (1.0 - age / policy.fade).clamp(0.0, 1.0)
                    };
                }
                None => {
                    slot.seat = usize::MAX;
                    slot.dim = 1.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(life: f32, score: f64) -> Standing {
        Standing {
            life,
            out: false,
            score,
        }
    }

    /// 72 opponents, life falling with the seat.
    fn field() -> Vec<Standing> {
        (0..72)
            .map(|i| s(1.0 - i as f32 / 100.0, 1000.0 - i as f64))
            .collect()
    }

    #[test]
    fn the_order_is_still_in_then_life_then_score() {
        let mut f = vec![s(0.5, 10.0), s(0.9, 1.0), s(0.5, 20.0)];
        f.push(Standing {
            life: 0.0,
            out: true,
            score: 99.0,
        });
        assert_eq!(order(&f), vec![1, 2, 0, 3]);
        let l = Ladder::of(&f, Some(s(0.6, 0.0)));
        assert_eq!((l.place, l.of, l.still_in), (Some(2), 5, 4));
        assert_eq!(l.marks.iter().filter(|m| m.2).count(), 1);
        assert!(l.marks[1].2, "the player's own mark");
        assert!(l.marks[4].1, "the out last");
        // A spectator: no place.
        assert_eq!(Ladder::of(&f, None).place, None);
    }

    #[test]
    fn the_nearest_rivals_above_and_below_take_the_slots() {
        let f = field();
        // The player between seats 29 and 30.
        let me = s(1.0 - 29.5 / 100.0, 0.0);
        let mut r = Rivals::default();
        r.update(8, &f, Some(me), &[], 1.0, RIVALS_V0);
        let mut seats: Vec<usize> = r.seats().into_iter().flatten().collect();
        seats.sort_unstable();
        assert_eq!(seats, (26..34).collect::<Vec<_>>());
        // Deterministic.
        let mut again = Rivals::default();
        again.update(8, &f, Some(me), &[], 1.0, RIVALS_V0);
        assert_eq!(again, r);
    }

    /// Where not all of a large field fit (a phone: 24 of 72), its slots
    /// are chosen as a room of nine's eight are: the attackers and the
    /// attacked first, then the nearest by standing, as many above as
    /// below.
    #[test]
    fn more_slots_than_eight_are_chosen_the_same_way() {
        let f = field();
        let me = s(1.0 - 29.5 / 100.0, 0.0);
        let mut r = Rivals::default();
        r.update(24, &f, Some(me), &[70, 3], 1.0, RIVALS_V0);
        let seats: Vec<usize> = r.seats().into_iter().flatten().collect();
        assert_eq!(seats.len(), 24);
        assert_eq!(&seats[..2], &[70, 3], "the pinned first");
        let mut rest = seats[2..].to_vec();
        rest.sort_unstable();
        // 22 more: eleven above the player, eleven below.
        assert_eq!(rest, (19..41).collect::<Vec<_>>());
    }

    #[test]
    fn an_attacker_comes_in_at_once_and_stays_while_pinned() {
        let f = field();
        let me = s(1.0 - 29.5 / 100.0, 0.0);
        let mut r = Rivals::default();
        r.update(8, &f, Some(me), &[], 1.0, RIVALS_V0);
        r.update(8, &f, Some(me), &[70], 1.1, RIVALS_V0);
        assert!(r.seats().contains(&Some(70)), "pinned in at once");
        // The farthest of the rivals made room for it.
        assert!(!r.seats().contains(&Some(33)) || !r.seats().contains(&Some(26)));
        r.update(8, &f, Some(me), &[70], 30.0, RIVALS_V0);
        assert!(r.seats().contains(&Some(70)));
        // No longer pinned: it goes once held long enough.
        r.update(8, &f, Some(me), &[], 40.0, RIVALS_V0);
        assert!(!r.seats().contains(&Some(70)));
    }

    #[test]
    fn rivals_trading_places_never_flicker() {
        let mut f = field();
        let me = s(1.0 - 29.5 / 100.0, 0.0);
        let mut r = Rivals::default();
        r.update(8, &f, Some(me), &[], 1.0, RIVALS_V0);
        let first = r.seats();
        // Seats 33 and 34 trade places every tick for a minute.
        for k in 0..600 {
            let t = 1.0 + k as f64 * 0.1;
            let (a, b) = if k % 2 == 0 {
                (0.66, 0.67)
            } else {
                (0.67, 0.66)
            };
            f[33].life = a;
            f[34].life = b;
            r.update(8, &f, Some(me), &[], t, RIVALS_V0);
            assert_eq!(r.seats(), first, "at {t}");
        }
    }

    #[test]
    fn a_rival_who_falls_far_is_replaced_only_after_the_hold() {
        let mut f = field();
        let me = s(1.0 - 29.5 / 100.0, 0.0);
        let mut r = Rivals::default();
        r.update(8, &f, Some(me), &[], 1.0, RIVALS_V0);
        assert!(r.seats().contains(&Some(30)));
        // Seat 30 goes out at 2 s: far down the field now.
        f[30].out = true;
        r.update(8, &f, Some(me), &[], 2.0, RIVALS_V0);
        assert!(r.seats().contains(&Some(30)), "held");
        r.update(8, &f, Some(me), &[], 9.5, RIVALS_V0);
        assert!(!r.seats().contains(&Some(30)), "replaced after the hold");
        assert!(r.seats().contains(&Some(34)), "by the next nearest");
    }

    #[test]
    fn the_slots_fade_in_and_reduced_motion_swaps_at_once() {
        let f = field();
        let mut r = Rivals::default();
        r.update(2, &f, None, &[], 10.0, RIVALS_V0);
        // The top of the field for a spectator following nobody.
        assert_eq!(r.seats(), vec![Some(0), Some(1)]);
        let slot = crate::opponents::Slot {
            seat: 0,
            rect: [0.0; 4],
            field: ddi_render::FieldGeometry {
                arrow: 1.0,
                left: 0.0,
                receptor_y: 0.0,
                direction: 1.0,
                width: 1.0,
                height: 1.0,
            },
            panel: [0.0; 4],
            left: true,
            dim: 0.0,
        };
        let mut fields = crate::opponents::Fields {
            slots: vec![slot; 2],
            status: [0.0; 4],
            rotating: true,
            lane: 8.0,
            rows: 10.0,
            range: 8.5,
            name: 9.0,
            stars: crate::opponents::Stars::Beside(9.0),
            ladder: None,
            carry: crate::opponents::Carry::ALL,
            dense: false,
        };
        r.apply(&mut fields, 10.1, RIVALS_V0, false);
        assert!(fields.slots[0].dim > 0.5);
        r.apply(&mut fields, 10.1, RIVALS_V0, true);
        assert_eq!(fields.slots[0].dim, 0.0);
        r.apply(&mut fields, 11.0, RIVALS_V0, false);
        assert_eq!(fields.slots[1].dim, 0.0);
    }
}
