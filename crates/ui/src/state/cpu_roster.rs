//! The named CPUs practice seats (`docs/plans/multiplayer-prototype-progress.md`,
//! M7b): our own catalog (`cpu_roster.json`), each CPU an invented name (data,
//! the same in every language), a personality (`ddi_engine::cpu::Skills`: a
//! preset's name or seven multipliers) and a level (one of the engine's
//! ten, `ddi_engine::cpu::CPU_LEVELS_V0`: strength relative to the chart,
//! never a skill rating; shown as five stars, `crate::stars`). A match's CPUs are a roster
//! of them (`ddi_room::CpuMode::Roster`), edited in the Rules pop-up: Add
//! random, or Add custom by level (several at once) or by name.

use std::sync::OnceLock;

use ddi_room::{CpuMode, MatchRules, OpponentSpec};
use serde::Deserialize;

/// One CPU of the catalog.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RosterCpu {
    pub name: String,
    /// A `ddi_engine::cpu::Skills` preset's name or seven multipliers.
    pub skills: String,
    /// From 1 (the file's); [`RosterCpu::spec`] gives the engine's index.
    pub level: u8,
}

impl RosterCpu {
    /// Its seat's profile, its level as the engine's index, with `seed`.
    pub fn spec(&self, seed: u64) -> OpponentSpec {
        OpponentSpec {
            level: self.level.saturating_sub(1),
            seed,
            personality: Some(self.skills.clone()),
            name: Some(self.name.clone()),
        }
    }
}

#[derive(Deserialize)]
struct File {
    cpus: Vec<RosterCpu>,
}

/// The catalog, in its file's order.
pub fn catalog() -> &'static [RosterCpu] {
    static CATALOG: OnceLock<Vec<RosterCpu>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str::<File>(include_str!("cpu_roster.json"))
            .map(|f| f.cpus)
            .unwrap_or_default()
    })
}

/// What Add random aims for. PROVISIONAL (M7b): any CPU not seated yet; the
/// hook where a balanced pick (a level near the people's) goes later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RosterAim {
    #[default]
    Any,
}

/// A seed for the `n`th draw from `seed` (splitmix64's step).
fn mix(seed: u64, n: u64) -> u64 {
    let mut z = seed.wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Whether `cpu` is seated in `roster` already (by its name).
fn seated(roster: &[OpponentSpec], cpu: &RosterCpu) -> bool {
    roster
        .iter()
        .any(|s| s.name.as_deref() == Some(cpu.name.as_str()))
}

/// The catalog's CPUs not in `roster`.
pub fn unseated(roster: &[OpponentSpec]) -> Vec<&'static RosterCpu> {
    catalog().iter().filter(|c| !seated(roster, c)).collect()
}

/// One CPU not in `roster`, drawn from `seed` as `aim` says, or none when
/// every one is seated.
pub fn random_cpu(roster: &[OpponentSpec], aim: RosterAim, seed: u64) -> Option<OpponentSpec> {
    let RosterAim::Any = aim;
    let free = unseated(roster);
    if free.is_empty() {
        return None;
    }
    let pick = mix(seed, roster.len() as u64);
    let cpu = free[(pick % free.len() as u64) as usize];
    Some(cpu.spec(mix(pick, 1)))
}

/// `count` CPUs not in `roster` at level `level` (the engine's index):
/// those of that level first, then others brought to it, drawn from
/// `seed`.
pub fn at_level(roster: &[OpponentSpec], level: u8, count: usize, seed: u64) -> Vec<OpponentSpec> {
    let mut out: Vec<OpponentSpec> = Vec::new();
    let mut taken = roster.to_vec();
    for n in 0..count as u64 {
        let free = unseated(&taken);
        if free.is_empty() {
            break;
        }
        let same: Vec<&RosterCpu> = free
            .iter()
            .copied()
            .filter(|c| c.level.saturating_sub(1) == level)
            .collect();
        let pool = if same.is_empty() { free } else { same };
        let pick = mix(seed, taken.len() as u64 + n);
        let mut spec = pool[(pick % pool.len() as u64) as usize].spec(mix(pick, 1));
        spec.level = level;
        taken.push(spec.clone());
        out.push(spec);
    }
    out
}

/// The catalog's CPU named `name`, if not in `roster`.
pub fn by_name(roster: &[OpponentSpec], name: &str, seed: u64) -> Option<OpponentSpec> {
    unseated(roster)
        .into_iter()
        .find(|c| c.name == name)
        .map(|c| c.spec(mix(seed, roster.len() as u64)))
}

/// `count` CPUs drawn for a new practice room.
pub fn starting(count: usize, seed: u64) -> Vec<OpponentSpec> {
    let mut out = Vec::new();
    for n in 0..count as u64 {
        match random_cpu(&out, RosterAim::Any, mix(seed, n)) {
            Some(spec) => out.push(spec),
            None => break,
        }
    }
    out
}

/// The CPUs `rules` seat by name and profile (a roster's; a fixed count's
/// as its common profile, unnamed).
pub fn roster_of(rules: &MatchRules, humans: usize) -> Vec<OpponentSpec> {
    match rules.cpu.mode {
        CpuMode::Roster => rules.cpu.roster.clone(),
        _ => (1..=rules.cpu_seats(humans))
            .map(|n| rules.cpu_for_seat(u16::try_from(n).unwrap_or(u16::MAX)))
            .collect(),
    }
}

/// `rules` seating `roster` beside `humans` people (the seats following;
/// without CPUs the seats stay as they were).
pub fn with_roster(mut rules: MatchRules, roster: Vec<OpponentSpec>, humans: usize) -> MatchRules {
    if roster.is_empty() {
        rules.cpu.mode = CpuMode::Off;
    } else {
        let humans = u16::try_from(humans.max(1)).unwrap_or(u16::MAX);
        rules.seats = humans.saturating_add(u16::try_from(roster.len()).unwrap_or(u16::MAX));
        rules.cpu.mode = CpuMode::Roster;
    }
    rules.cpu.roster = roster;
    rules
}

/// The personality's preset name (`stream`), or `None` for seven
/// multipliers of their own.
pub fn preset(skills: &str) -> Option<&'static str> {
    let parsed = ddi_engine::cpu::Skills::parse(skills).ok()?;
    [
        "balanced",
        "stream",
        "jacks",
        "chords",
        "stamina",
        "technical",
    ]
    .into_iter()
    .find(|p| ddi_engine::cpu::Skills::parse(p).ok() == Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_reads_and_every_cpu_is_playable() {
        let all = catalog();
        assert_eq!(all.len(), 24);
        let levels = ddi_engine::cpu::LEVELS as u8;
        assert_eq!(levels, 10);
        let mut names = std::collections::BTreeSet::new();
        for c in all {
            assert!(names.insert(&c.name), "{} twice", c.name);
            assert!((1..=levels).contains(&c.level), "{}", c.name);
            ddi_engine::cpu::Skills::parse(&c.skills).unwrap();
            assert!(c.spec(1).fits());
        }
        // Every level and every preset is somebody's.
        for l in 1..=levels {
            assert!(all.iter().any(|c| c.level == l), "level {l}");
        }
        assert!(all.iter().any(|c| preset(&c.skills).is_none()));
    }

    #[test]
    fn random_and_custom_picks_never_seat_one_twice() {
        let mut roster = starting(3, 7);
        assert_eq!(roster.len(), 3);
        assert_eq!(starting(3, 7), roster, "the same seed, the same CPUs");
        let one = random_cpu(&roster, RosterAim::Any, 9).unwrap();
        assert!(!roster.iter().any(|s| s.name == one.name));
        roster.push(one);
        let four = at_level(&roster, 9, 4, 3);
        assert_eq!(four.len(), 4);
        assert!(four.iter().all(|s| s.level == 9));
        roster.extend(four);
        let name = unseated(&roster)[0].name.clone();
        let named = by_name(&roster, &name, 1).unwrap();
        assert_eq!(named.name.as_deref(), Some(name.as_str()));
        roster.push(named);
        assert!(by_name(&roster, &name, 1).is_none(), "seated already");
        // Everyone seated: nothing more to add.
        let all = at_level(&roster, 0, 100, 5);
        roster.extend(all);
        assert_eq!(roster.len(), catalog().len());
        assert!(random_cpu(&roster, RosterAim::Any, 1).is_none());
        let names: std::collections::BTreeSet<_> = roster.iter().map(|s| &s.name).collect();
        assert_eq!(names.len(), roster.len());
    }

    #[test]
    fn a_roster_sets_the_seats_and_the_mode() {
        let rules = with_roster(MatchRules::hosted(), starting(2, 1), 1);
        assert_eq!((rules.seats, rules.cpu.mode), (3, CpuMode::Roster));
        assert_eq!(roster_of(&rules, 1).len(), 2);
        let none = with_roster(rules, Vec::new(), 2);
        assert_eq!((none.seats, none.cpu.mode), (3, CpuMode::Off));
    }
}
