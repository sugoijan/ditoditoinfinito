//! The ten levels a CPU opponent plays at
//! (`docs/plans/cpu-player.md`, "Ten levels";
//! `docs/plans/multiplayer-prototype-progress.md`, feedback round A).
//!
//! Levels 6 to 10 are the five frozen strengths ([`super::STRENGTHS`],
//! the solo CPU's `relaxed` to `expert`), unchanged: the same chart, seed
//! and skills give the same presses as before there were ten. Levels 1
//! to 5 are weaker. A strength under the frozen five soon stops making a
//! CPU worse (the timing law's width is bounded at 80 ms, its share at
//! every note, and a wide press is still a judged press: nothing is ever
//! missed), so a weak level also has a *lapse*: the chance it leaves a
//! note out, growing with how demanding the chart is at that note (the
//! plan's frequency: close rows, repeated lanes, chords). On an easy
//! chart it misses a few notes; on one above its head it misses many and
//! its gauge empties.
//!
//! PROVISIONAL: [`CPU_LEVELS_V0`]'s values were chosen by measurement
//! over local arcade packs (the tables in `docs/plans/cpu-player.md`),
//! never against people. A level is strength relative to the chart's own
//! difficulty plus that lapse: never a skill rating.

use super::STRENGTHS;

/// How many levels there are.
pub const LEVELS: usize = 10;

/// How a CPU plays at a level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    /// The ability multiplier ([`super::STRENGTHS`]' meaning): its bad
    /// presses' spread is the plan's width over it, their share the
    /// plan's frequency over its square.
    pub strength: f64,
    /// The chance it leaves a note out where the chart is at its most
    /// demanding (times the plan's frequency at the note, `0.1..=1`);
    /// none for the frozen strengths.
    pub lapse: f64,
}

impl Level {
    /// One of the five frozen strengths, as its index into
    /// [`super::STRENGTHS`]: planned exactly as before.
    pub fn frozen(&self) -> Option<usize> {
        if self.lapse != 0.0 {
            return None;
        }
        STRENGTHS.iter().position(|s| *s == self.strength)
    }

    /// Within what a plan can be made for.
    pub fn validate(&self) -> Result<(), String> {
        if self.strength.is_finite()
            && (0.05..=4.0).contains(&self.strength)
            && self.lapse.is_finite()
            && (0.0..=0.95).contains(&self.lapse)
        {
            Ok(())
        } else {
            Err("CPU level out of range".into())
        }
    }
}

/// The levels, named and versioned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelTable {
    pub id: &'static str,
    /// From the weakest; the last five are [`super::STRENGTHS`].
    pub levels: [Level; LEVELS],
}

impl LevelTable {
    /// Level `index` (from 0, the weakest).
    pub fn level(&self, index: usize) -> Option<Level> {
        self.levels.get(index).copied()
    }
}

const fn frozen(index: usize) -> Level {
    Level {
        strength: STRENGTHS[index],
        lapse: 0.0,
    }
}

/// `cpu-levels-v0` (PROVISIONAL, measured over local arcade packs, never
/// against people): levels 1 to 5 by strength and lapse, 6 to 10 the
/// frozen strengths.
pub const CPU_LEVELS_V0: LevelTable = LevelTable {
    id: "cpu-levels-v0",
    levels: [
        Level {
            strength: 0.40,
            lapse: 0.20,
        },
        Level {
            strength: 0.47,
            lapse: 0.19,
        },
        Level {
            strength: 0.54,
            lapse: 0.16,
        },
        Level {
            strength: 0.61,
            lapse: 0.12,
        },
        Level {
            strength: 0.68,
            lapse: 0.07,
        },
        frozen(0),
        frozen(1),
        frozen(2),
        frozen(3),
        frozen(4),
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_levels_the_last_five_the_frozen_strengths() {
        let t = CPU_LEVELS_V0;
        assert_eq!(t.id, "cpu-levels-v0");
        assert_eq!(t.levels.len(), 10);
        for (i, s) in STRENGTHS.iter().enumerate() {
            let l = t.level(5 + i).unwrap();
            assert_eq!((l.strength, l.lapse), (*s, 0.0));
            assert_eq!(l.frozen(), Some(i));
        }
        // The new ones weaker step by step: less strength, more lapse,
        // none of them a frozen strength.
        for i in 0..5 {
            let (a, b) = (t.levels[i], t.levels[i + 1]);
            assert!(a.strength < b.strength, "{i}");
            assert!(a.lapse > b.lapse, "{i}");
            assert_eq!(a.frozen(), None);
            a.validate().unwrap();
        }
        assert!(t.level(10).is_none());
        assert!(
            Level {
                strength: f64::NAN,
                lapse: 0.0
            }
            .validate()
            .is_err()
        );
        assert!(
            Level {
                strength: 1.0,
                lapse: 1.0
            }
            .validate()
            .is_err()
        );
    }
}
