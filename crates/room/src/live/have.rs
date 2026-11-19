//! Who has which of the room's listed songs (`docs/plans/multiplayer-room-songs.md`,
//! stage 2, `room-have-v0`, PROVISIONAL): each device says which entries
//! of the room's song list it has (`ClientLive::Have`, ranges of entry
//! ids), the room keeps it beside its sockets and never in its stored
//! state (it decides nothing: a pick, the readiness window and every
//! stage go as they would without it), and says each member's to every
//! live socket (`ServerLive::Have`): the lists show "11 of 12 have it",
//! the picker is told before a pick who does not have the song yet.
//!
//! A member's set is the union of what their sockets said; a socket's
//! goes with it. A socket that says hello is told everyone's at once.

use crate::ids::{ParticipantId, RoomSongId};
use crate::live::ServerLive;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How often a device says what it has, at most (ms): a download ending
/// every second says it every five.
pub const HAVE_EVERY_MS: u64 = 5_000;

/// The most ranges a set says: a list of a hundred songs, every other
/// one had, is fifty. A device with more says its first ones (fewer than
/// it has: a count is low, never wrong the other way).
pub const MAX_RANGES: usize = 64;

/// A set of the list's entries, as sorted, disjoint, inclusive ranges of
/// their ids (a list's ids grow by one as songs are added: everything a
/// device has is one or a few ranges).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HaveSet(Vec<(u64, u64)>);

impl HaveSet {
    /// The set of `ids` (any order, repeats once), its first
    /// [`MAX_RANGES`] ranges.
    pub fn of(ids: impl IntoIterator<Item = RoomSongId>) -> HaveSet {
        let mut ids: Vec<u64> = ids.into_iter().map(|i| i.0).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        for id in ids {
            match ranges.last_mut() {
                Some((_, hi)) if *hi + 1 == id => *hi = id,
                _ => {
                    if ranges.len() == MAX_RANGES {
                        break;
                    }
                    ranges.push((id, id));
                }
            }
        }
        HaveSet(ranges)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn has(&self, id: RoomSongId) -> bool {
        self.0.iter().any(|(lo, hi)| (*lo..=*hi).contains(&id.0))
    }

    /// The ranges as said.
    pub fn ranges(&self) -> &[(u64, u64)] {
        &self.0
    }

    /// Whether a set received is well made: at most [`MAX_RANGES`]
    /// ranges, each in order, sorted and apart.
    pub fn fits(&self) -> bool {
        self.0.len() <= MAX_RANGES
            && self.0.iter().all(|(lo, hi)| lo <= hi)
            && self.0.windows(2).all(|w| w[0].1.saturating_add(1) < w[1].0)
    }

    /// Both sets' entries.
    pub fn union(&self, other: &HaveSet) -> HaveSet {
        let mut all: Vec<(u64, u64)> = self.0.iter().chain(&other.0).copied().collect();
        all.sort_unstable();
        let mut out: Vec<(u64, u64)> = Vec::new();
        for (lo, hi) in all {
            match out.last_mut() {
                Some((_, h)) if lo <= h.saturating_add(1) => *h = (*h).max(hi),
                _ => out.push((lo, hi)),
            }
        }
        out.truncate(MAX_RANGES);
        HaveSet(out)
    }
}

/// What the room keeps of who has what: each socket's last set and whose
/// socket it is (sans-IO: the server's room and the fake one run it).
#[derive(Clone, Debug, Default)]
pub struct Haves {
    by_conn: BTreeMap<String, (ParticipantId, HaveSet)>,
}

impl Haves {
    /// Socket `conn` of `who` says `set`: the message every live socket
    /// is sent where `who`'s set changed by it (none for a set not well
    /// made, which is passed over).
    pub fn set(&mut self, conn: &str, who: ParticipantId, set: HaveSet) -> Option<ServerLive> {
        if !set.fits() {
            return None;
        }
        let before = self.of(who);
        if set.is_empty() {
            self.by_conn.remove(conn);
        } else {
            self.by_conn.insert(conn.to_string(), (who, set));
        }
        self.said(who, before)
    }

    /// Socket `conn` closed: what it said goes with it.
    pub fn closed(&mut self, conn: &str) -> Option<ServerLive> {
        let (who, _) = self.by_conn.get(conn)?.clone();
        let before = self.of(who);
        self.by_conn.remove(conn);
        self.said(who, before)
    }

    /// Everyone's sets, for a socket that said hello (none while nobody
    /// said one).
    pub fn all(&self) -> Option<ServerLive> {
        let mut sets: BTreeMap<ParticipantId, HaveSet> = BTreeMap::new();
        for (who, set) in self.by_conn.values() {
            let s = sets.entry(*who).or_default();
            *s = s.union(set);
        }
        (!sets.is_empty()).then(|| ServerLive::Have {
            sets: sets.into_iter().collect(),
        })
    }

    /// `who`'s set: the union of their sockets'.
    pub fn of(&self, who: ParticipantId) -> HaveSet {
        self.by_conn
            .values()
            .filter(|(p, _)| *p == who)
            .fold(HaveSet::default(), |all, (_, s)| all.union(s))
    }

    fn said(&self, who: ParticipantId, before: HaveSet) -> Option<ServerLive> {
        let now = self.of(who);
        (now != before).then(|| ServerLive::Have {
            sets: vec![(who, now)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[u64]) -> Vec<RoomSongId> {
        v.iter().map(|i| RoomSongId(*i)).collect()
    }

    #[test]
    fn a_set_is_its_ranges_and_reads_back_as_said() {
        let s = HaveSet::of(ids(&[5, 1, 2, 3, 9, 3, 10]));
        assert_eq!(s.ranges(), &[(1, 3), (5, 5), (9, 10)]);
        assert!(s.fits());
        assert!(s.has(RoomSongId(2)) && s.has(RoomSongId(10)) && !s.has(RoomSongId(4)));
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "[[1,3],[5,5],[9,10]]");
        assert_eq!(serde_json::from_str::<HaveSet>(&json).unwrap(), s);
        let every_other = HaveSet::of((1..=300).step_by(2).map(RoomSongId));
        assert_eq!(every_other.ranges().len(), MAX_RANGES);
        assert!(every_other.fits());
        assert!(!HaveSet(vec![(3, 1)]).fits());
        assert!(!HaveSet(vec![(1, 3), (4, 5)]).fits());
        assert!(!HaveSet(vec![(5, 6), (1, 2)]).fits());
    }

    #[test]
    fn a_member_s_set_is_their_sockets_and_goes_with_them() {
        let (a, b) = (ParticipantId(1), ParticipantId(2));
        let mut h = Haves::default();
        assert_eq!(h.all(), None);
        let said = h.set("c1", a, HaveSet::of(ids(&[1, 2])));
        assert_eq!(
            said,
            Some(ServerLive::Have {
                sets: vec![(a, HaveSet::of(ids(&[1, 2])))]
            })
        );
        // The same again: nothing to say.
        assert_eq!(h.set("c1", a, HaveSet::of(ids(&[1, 2]))), None);
        // A second socket of theirs: the union.
        let said = h.set("c2", a, HaveSet::of(ids(&[3])));
        assert_eq!(
            said,
            Some(ServerLive::Have {
                sets: vec![(a, HaveSet::of(ids(&[1, 2, 3])))]
            })
        );
        h.set("c3", b, HaveSet::of(ids(&[7])));
        assert_eq!(
            h.all(),
            Some(ServerLive::Have {
                sets: vec![
                    (a, HaveSet::of(ids(&[1, 2, 3]))),
                    (b, HaveSet::of(ids(&[7])))
                ]
            })
        );
        let said = h.closed("c2");
        assert_eq!(
            said,
            Some(ServerLive::Have {
                sets: vec![(a, HaveSet::of(ids(&[1, 2])))]
            })
        );
        assert_eq!(h.closed("c9"), None);
        // A set not well made is passed over.
        assert_eq!(h.set("c3", b, HaveSet(vec![(2, 1)])), None);
        assert_eq!(h.of(b), HaveSet::of(ids(&[7])));
        // An empty set: nothing kept of the socket.
        assert!(h.set("c3", b, HaveSet::default()).is_some());
        assert!(h.of(b).is_empty());
    }
}
