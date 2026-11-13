use super::*;

const A: ParticipantId = ParticipantId(1);
const B: ParticipantId = ParticipantId(2);

const C: ParticipantId = ParticipantId(3);

/// Seconds as microseconds.
const S: i64 = 1_000_000;

fn roster(seats: &[(ParticipantId, Targeting)], lead_us: i64, seed: u64) -> StageRoster {
    StageRoster {
        seats: seats
            .iter()
            .map(|&(participant, targeting)| RosterSeat {
                participant,
                targeting,
                lead_us,
            })
            .collect(),
        seed,
    }
}

/// A chart of `n` four-beat steps of 2 s (120 BPM): a slot every 2 s
/// from 0, each with a section (eight shocks a quarter second apart)
/// where `room(j)`; checkpoints every 2 s from 2 s.
fn schedule(n: usize, room: impl Fn(usize) -> bool) -> Schedule {
    Schedule {
        slots: (0..n)
            .map(|j| {
                let start_us = j as i64 * 2 * S;
                Slot {
                    start_us,
                    section: room(j).then(|| {
                        let cells: Vec<Cell> = (0..8)
                            .map(|i| Cell {
                                tick: (j as i64 * 4 * 48) + i * 24,
                                lane: (i % 4) as u8,
                                at_us: start_us + i * S / 4,
                            })
                            .collect();
                        let last = cells.last().unwrap().at_us;
                        Section {
                            clear_us: last + 100_000,
                            recover_after_us: last + 100_000 + 2 * S,
                            cells,
                        }
                    }),
                }
            })
            .collect(),
    }
}

fn checkpoints(n: usize) -> Vec<i64> {
    (1..=n as i64).map(|k| k * 2 * S).collect()
}

fn battle(seats: &[(ParticipantId, Targeting)], seed: u64) -> StageBattle {
    StageBattle::new(
        ATTACKS_V0,
        POWER_CLASSES_V0,
        "itg-fa+",
        checkpoints(40),
        roster(seats, 3 * S, seed),
        schedule(40, |_| true),
    )
}

fn ledger() -> StageBattle {
    StageBattle::new(
        ATTACKS_V0,
        POWER_CLASSES_V0,
        "itg-fa+",
        vec![2_000_000, 4_000_000, 6_000_000, 8_000_000],
        roster(&[(A, Targeting::Spread), (B, Targeting::Spread)], 0, 1),
        Schedule::default(),
    )
}

fn report(p: ParticipantId, tiers: &[(i64, u8)]) -> Report {
    Report {
        participant: p,
        judged: tiers
            .iter()
            .map(|&(at_us, tier)| Judged { at_us, tier })
            .collect(),
        life_ppm: 1_000_000,
        out_at_us: None,
        targeting: None,
    }
}

/// A report for checkpoint `k` (at `(k + 1) · 2 s`): `full`, fifty of the
/// best in the interval before it (a full bar); else `steady` judgments of
/// the worst (nothing earned, but judged: toward recovery).
fn rep(p: ParticipantId, k: usize, full: bool, steady: usize) -> Report {
    let end = (k as i64 + 1) * 2 * S;
    let from = end - 2 * S;
    let (n, tier) = if full { (50, 0) } else { (steady, 5) };
    let step = (2 * S) / (n as i64 + 1);
    report(
        p,
        &(1..=n as i64)
            .map(|i| (from + i * step, tier))
            .collect::<Vec<_>>(),
    )
}

fn committed(events: &[AttackEvent]) -> Vec<(ParticipantId, ParticipantId)> {
    events
        .iter()
        .filter_map(|e| match e {
            AttackEvent::Committed {
                sender, recipient, ..
            } => Some((*sender, *recipient)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_classes_are_named_and_cover_every_match_ruleset() {
    assert_eq!(POWER_CLASSES_V0.id, "power-classes-v0");
    // DDI (W0 inside W1: tier 0) and every id a room could name before.
    assert!(POWER_CLASSES_V0.rulesets().any(|id| id == "ddi"));
    for id in crate::MatchRules::RULESETS
        .into_iter()
        .chain(crate::MatchRules::LEGACY_RULESETS)
    {
        assert_eq!(POWER_CLASSES_V0.earned(id, 0), 2, "{id}");
        assert_eq!(POWER_CLASSES_V0.earned(id, 1), 1, "{id}");
        for tier in 2..6 {
            assert_eq!(POWER_CLASSES_V0.earned(id, tier), 0, "{id}");
        }
    }
    assert_eq!(POWER_CLASSES_V0.earned("danoni", 0), 0);
    assert_eq!(POWER_CLASSES_V0.earned("itg", 9), 0);
}

#[test]
fn power_is_earned_by_timing_and_credited_at_checkpoints() {
    let mut l = ledger();
    assert_eq!(l.next_checkpoint(), Some((0, 2_000_000)));
    // Two best, one next best, two worse; one after the checkpoint
    // waits for the next.
    let r = report(A, &[(100, 0), (200, 0), (300, 1), (400, 2), (500, 5)]);
    assert!(l.checkpoint(0, &[r, report(B, &[(1_000, 1)])]).is_empty());
    assert_eq!((l.power(A), l.power(B)), (5, 1));
    assert!(!l.charged(A));
    // Not the next checkpoint: nothing.
    assert!(l.checkpoint(0, &[report(A, &[(2_100_000, 0)])]).is_empty());
    assert_eq!(l.power(A), 5);
    let late = report(A, &[(3_000_000, 0), (4_000_000, 0)]);
    l.checkpoint(1, &[late]);
    assert_eq!(
        l.power(A),
        7,
        "one at the checkpoint's time is not before it"
    );
    assert_eq!(l.record().len(), 2);
    assert_eq!(l.record()[1].power, vec![(A, 7), (B, 1)]);
}

#[test]
fn the_bar_caps_at_a_hundred_and_the_overflow_is_lost() {
    let mut l = ledger();
    let many: Vec<(i64, u8)> = (0..60).map(|i| (i * 1_000, 0)).collect();
    let events = l.checkpoint(0, &[report(A, &many)]);
    // Full, and no section to attack in (an empty schedule): it waits.
    assert_eq!(
        events,
        vec![
            AttackEvent::Charged { participant: A },
            AttackEvent::Blocked {
                sender: A,
                reason: BlockReason::NoSafeSection
            }
        ]
    );
    assert_eq!((l.power(A), l.overflow(A)), (100, 20));
    assert!(l.charged(A));
    assert_eq!(l.fill(A), 1.0);
    // Full already: no second Charged (nor Blocked: the same reason),
    // the rest lost too.
    let more = report(A, &[(2_500_000, 0)]);
    assert!(l.checkpoint(1, &[more]).is_empty());
    assert_eq!(l.waiting(A), Some(BlockReason::NoSafeSection));
    assert_eq!(l.stats(A).waited_no_safe_section, 2);
    assert_eq!((l.power(A), l.overflow(A)), (100, 22));
}

#[test]
fn those_out_earn_nothing_more() {
    let mut l = ledger();
    let mut r = report(A, &[(100, 0)]);
    r.out_at_us = Some(1_500_000);
    l.checkpoint(0, &[r]);
    assert!(l.out(A));
    assert_eq!(l.power(A), 0);
    // Out after the checkpoint: still in at it.
    let mut r = report(B, &[(100, 0)]);
    r.out_at_us = Some(2_000_000);
    l = ledger();
    l.checkpoint(0, &[r]);
    assert!(!l.out(B));
    assert_eq!(l.power(B), 2);
}

#[test]
fn checkpoints_only_rise() {
    let l = StageBattle::new(
        ATTACKS_V0,
        POWER_CLASSES_V0,
        "itg",
        vec![1, 1, 3, 2, 4],
        roster(&[(A, Targeting::Spread)], 0, 1),
        Schedule::default(),
    );
    assert_eq!(l.checkpoints_us(), &[1, 3, 4]);
}

#[test]
fn a_full_bar_is_spent_on_eight_shocks_from_the_target_s_lead() {
    let mut l = battle(&[(A, Targeting::Spread), (B, Targeting::Spread)], 1);
    let events = l.checkpoint(0, &[rep(A, 0, true, 0), rep(B, 0, false, 4)]);
    assert_eq!(committed(&events), vec![(A, B)]);
    assert_eq!(l.power(A), 0, "spent");
    let a = l.incoming(B).unwrap().clone();
    assert_eq!(a.cells.len(), 8, "never fewer (D4)");
    // At 2 s, a lead of 3 s: the first section from 5 s (and at least
    // the next checkpoint's, 4 s): the slot at 6 s.
    assert_eq!(a.slot, 3);
    assert!(a.cells[0].at_us >= 5 * S);
    assert_eq!(l.outgoing(A).count(), 1);
    assert_eq!(l.outstanding().collect::<Vec<_>>(), vec![(B, a.clear_us)]);
    assert_eq!(l.stats(B).received, 1);
    // A lead of nothing still waits for the next checkpoint (four beats).
    let mut quick = StageBattle::new(
        ATTACKS_V0,
        POWER_CLASSES_V0,
        "itg-fa+",
        checkpoints(10),
        roster(&[(A, Targeting::Spread), (B, Targeting::Spread)], 0, 1),
        schedule(10, |_| true),
    );
    quick.checkpoint(0, &[rep(A, 0, true, 0)]);
    assert_eq!(quick.incoming(B).unwrap().slot, 2, "from 4 s");
}

#[test]
fn a_duel_waits_for_recovery_before_the_next_attack() {
    let mut l = battle(&[(A, Targeting::Spread), (B, Targeting::Spread)], 2);
    // Both full at once: each attacks the other.
    let e = l.checkpoint(0, &[rep(A, 0, true, 0), rep(B, 0, true, 0)]);
    let mut pairs = committed(&e);
    pairs.sort();
    assert_eq!(pairs, vec![(A, B), (B, A)]);
    let clear = l.incoming(A).unwrap().clear_us;
    // Full again at once: nobody to attack (each under one).
    let e = l.checkpoint(1, &[rep(A, 1, true, 0), rep(B, 1, true, 0)]);
    assert!(e.contains(&AttackEvent::Blocked {
        sender: A,
        reason: BlockReason::NoEligible
    }));
    assert_eq!(l.waiting(A), Some(BlockReason::NoEligible));
    // Through the section (cleared), then judged on: recovered after
    // eight judgments and four beats, then attacked again.
    let mut k = 2;
    let mut cleared = false;
    let mut recovered_at = None;
    let mut again = None;
    while k < 20 && again.is_none() {
        let e = l.checkpoint(k, &[rep(A, k, false, 3), rep(B, k, false, 3)]);
        cleared |= e.iter().any(|e| matches!(e, AttackEvent::Cleared { .. }));
        if recovered_at.is_none() && e.iter().any(|e| matches!(e, AttackEvent::Recovered { .. })) {
            recovered_at = Some(k);
        }
        if !committed(&e).is_empty() {
            again = Some(k);
        }
        k += 1;
    }
    assert!(cleared);
    let r = recovered_at.expect("recovered");
    assert!((r as i64 + 1) * 2 * S >= clear + 2 * S, "four beats after");
    assert_eq!(again, recovered_at, "launched as soon as it can be");
    assert!(l.stats(A).waited_no_eligible >= 2);
}

#[test]
fn everyone_under_attack_once_and_leader_falls_back_to_the_next() {
    // A and C both go for the leader (B, most life): B gets one attack,
    // the second sender the next leader.
    let mut l = battle(
        &[
            (A, Targeting::Leader),
            (B, Targeting::Spread),
            (C, Targeting::Leader),
        ],
        3,
    );
    let mut b = rep(B, 0, false, 0);
    b.life_ppm = 900_000;
    let mut a = rep(A, 0, true, 0);
    a.life_ppm = 500_000;
    let mut c = rep(C, 0, true, 0);
    c.life_ppm = 400_000;
    let e = l.checkpoint(0, &[a, b, c]);
    let pairs = committed(&e);
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs.iter().filter(|(_, r)| *r == B).count(), 1);
    // Rotation at checkpoint 0 starts with A: A takes B, C the next
    // leader (A, life 0.5, over nobody else).
    assert_eq!(pairs, vec![(A, B), (C, A)]);
}

#[test]
fn spread_goes_to_the_fewest_attacked_and_the_order_turns() {
    let seats = [
        (A, Targeting::Spread),
        (B, Targeting::Spread),
        (C, Targeting::Spread),
    ];
    // Simultaneous full bars: the first to pick turns with the
    // checkpoint (k mod n in participant order).
    for k0 in 0..3usize {
        let mut l = battle(&seats, 4);
        for k in 0..k0 {
            l.checkpoint(k, &[]);
        }
        let e = l.checkpoint(
            k0,
            &[
                rep(A, k0, true, 0),
                rep(B, k0, true, 0),
                rep(C, k0, true, 0),
            ],
        );
        let pairs = committed(&e);
        assert_eq!(pairs[0].0, [A, B, C][k0 % 3], "{pairs:?}");
        // Each of the three is someone's recipient, no one twice.
        let mut r: Vec<_> = pairs.iter().map(|p| p.1).collect();
        r.sort();
        r.dedup();
        assert_eq!(r.len(), pairs.len());
    }
    // Repeated focus avoided: B attacked once already, A's Spread goes
    // to C, who was not.
    let mut l = battle(&seats, 5);
    l.checkpoint(0, &[rep(C, 0, true, 0)]);
    let first = l.incoming(A).or(l.incoming(B)).unwrap().recipient;
    let other = if first == A { B } else { A };
    // Recover the first recipient.
    for k in 1..12 {
        l.checkpoint(k, &[rep(first, k, false, 4)]);
    }
    assert!(l.incoming(first).is_none(), "recovered");
    let sender = other;
    let e = l.checkpoint(12, &[rep(sender, 12, true, 0)]);
    assert_eq!(committed(&e), vec![(sender, C)], "the one never attacked");
}

#[test]
fn retaliate_remembers_from_the_next_batch_on() {
    let pick = |targeting: Targeting, seed: u64| {
        let mut l = battle(
            &[
                (A, Targeting::Leader),
                (B, targeting),
                (C, Targeting::Spread),
            ],
            seed,
        );
        let mut a = rep(A, 0, true, 0);
        a.life_ppm = 300_000;
        let mut b = rep(B, 0, true, 0);
        b.life_ppm = 900_000;
        let mut c = rep(C, 0, false, 0);
        c.life_ppm = 300_000;
        // A attacks the leader B; B, full in the same batch, has no
        // memory yet: it picks as Spread would.
        let first = committed(&l.checkpoint(0, &[a, b, c]));
        let b_first = first.iter().find(|p| p.0 == B).map(|p| p.1);
        (l, b_first)
    };
    for seed in 0..8 {
        let (_, spread) = pick(Targeting::Spread, seed);
        let (mut l, retaliate) = pick(Targeting::Retaliate, seed);
        assert_eq!(spread, retaliate, "seed {seed}: no memory within a batch");
        // Once its own attack's recipient recovered, B full again
        // retaliates against A (if A can be attacked).
        for k in 1..14 {
            l.checkpoint(
                k,
                &[
                    rep(A, k, false, 4),
                    rep(B, k, false, 4),
                    rep(C, k, false, 4),
                ],
            );
        }
        if l.incoming(A).is_some() {
            continue;
        }
        let e = l.checkpoint(14, &[rep(B, 14, true, 0)]);
        assert_eq!(committed(&e), vec![(B, A)], "seed {seed}");
    }
}

/// M8 round 3 (D8): a policy changed during the play, as the reports
/// carry it, goes for the launches of the checkpoint that hears it and
/// those after, never for one already run; a report naming none keeps the
/// policy; the change is in the record.
#[test]
fn a_policy_changed_in_play_applies_from_the_next_checkpoint() {
    let seats = [
        (A, Targeting::Spread),
        (B, Targeting::Spread),
        (C, Targeting::Spread),
    ];
    // B at half its life, C at full: Leader picks C, whatever the seed.
    let lives = |k: usize| {
        let mut b = rep(B, k, false, 0);
        b.life_ppm = 500_000;
        vec![b, rep(C, k, false, 0)]
    };
    // Spread throughout: whoever the seed draws.
    let spread = |seed: u64| {
        let mut l = battle(&seats, seed);
        let mut r = lives(0);
        r.push(rep(A, 0, true, 0));
        committed(&l.checkpoint(0, &r))[0].1
    };
    let seed = (0..64).find(|&s| spread(s) == B).expect("a seed whose Spread picks B");
    // Leader chosen before the first checkpoint: this launch goes by it.
    let mut l = battle(&seats, seed);
    let mut r = lives(0);
    let mut a = rep(A, 0, true, 0);
    a.targeting = Some(Targeting::Leader);
    r.push(a);
    let e = l.checkpoint(0, &r);
    assert_eq!(committed(&e), vec![(A, C)], "the leader, not Spread's B");
    assert!(e.contains(&AttackEvent::Retargeted {
        participant: A,
        targeting: Targeting::Leader,
    }));
    assert_eq!(l.targeting(A), Targeting::Leader);
    assert!(l.record()[0].events.contains(&AttackEvent::Retargeted {
        participant: A,
        targeting: Targeting::Leader,
    }));
    // Changed after a checkpoint ran: that one launched by the old
    // policy, and the next reports say nothing new.
    let mut l = battle(&seats, seed);
    let mut r = lives(0);
    r.push(rep(A, 0, true, 0));
    assert_eq!(committed(&l.checkpoint(0, &r)), vec![(A, B)]);
    let mut r = lives(1);
    let mut a = rep(A, 1, false, 0);
    a.targeting = Some(Targeting::Leader);
    r.push(a);
    let e = l.checkpoint(1, &r);
    assert_eq!(
        e.iter()
            .filter(|e| matches!(e, AttackEvent::Retargeted { .. }))
            .count(),
        1
    );
    // The same policy again is no change; none named keeps it.
    let mut r = lives(2);
    let mut a = rep(A, 2, false, 0);
    a.targeting = Some(Targeting::Leader);
    r.push(a);
    let retargeted = |e: &[AttackEvent]| {
        e.iter()
            .any(|e| matches!(e, AttackEvent::Retargeted { .. }))
    };
    assert!(!retargeted(&l.checkpoint(2, &r)));
    let mut r = lives(3);
    r.push(rep(A, 3, false, 0));
    assert!(!retargeted(&l.checkpoint(3, &r)));
    assert_eq!(l.targeting(A), Targeting::Leader);
    // Someone out changes nothing.
    let mut out = rep(B, 4, false, 0);
    out.out_at_us = Some(1);
    out.targeting = Some(Targeting::Retaliate);
    l.checkpoint(4, &[out]);
    assert_eq!(l.targeting(B), Targeting::Spread);
    // The report's field is additive: left out of the JSON when none,
    // read where it is missing.
    let plain = serde_json::to_string(&rep(A, 0, false, 1)).unwrap();
    assert!(!plain.contains("targeting"), "{plain}");
    let read: Report = serde_json::from_str(&plain).unwrap();
    assert_eq!(read.targeting, None);
}

#[test]
fn an_attack_against_someone_out_expires_unrefunded() {
    let mut l = battle(&[(A, Targeting::Spread), (B, Targeting::Spread)], 6);
    l.checkpoint(0, &[rep(A, 0, true, 0)]);
    let id = l.incoming(B).unwrap().id;
    let mut b = rep(B, 1, false, 2);
    b.out_at_us = Some(3 * S);
    let e = l.checkpoint(1, &[b]);
    assert!(e.contains(&AttackEvent::Expired { attack: id }));
    assert!(l.out(B));
    assert_eq!(l.power(A), 0, "no refund");
    assert_eq!(l.outstanding().count(), 0);
    // Full again: no one left to attack.
    let e = l.checkpoint(2, &[rep(A, 2, true, 0)]);
    assert!(e.contains(&AttackEvent::Blocked {
        sender: A,
        reason: BlockReason::NoEligible
    }));
}

#[test]
fn no_safe_section_in_reach_waits() {
    // Only one section, late: out of reach (16 beats past the lead) at
    // first, taken once it is in reach.
    let mut l = StageBattle::new(
        ATTACKS_V0,
        POWER_CLASSES_V0,
        "itg-fa+",
        checkpoints(30),
        roster(&[(A, Targeting::Spread), (B, Targeting::Spread)], 0, 7),
        schedule(30, |j| j == 12),
    );
    let e = l.checkpoint(0, &[rep(A, 0, true, 0)]);
    assert_eq!(
        e.last(),
        Some(&AttackEvent::Blocked {
            sender: A,
            reason: BlockReason::NoSafeSection
        })
    );
    let mut k = 1;
    while l.incoming(B).is_none() {
        l.checkpoint(k, &[]);
        k += 1;
        assert!(k < 12);
    }
    assert_eq!(l.incoming(B).unwrap().slot, 12);
    assert_eq!(l.waiting(A), None);
    assert!(l.stats(A).waited_no_safe_section >= 1);
}

#[test]
fn a_hundred_and_twenty_eight_seats_stay_bounded() {
    let seats: Vec<(ParticipantId, Targeting)> = (1..=128)
        .map(|i| {
            (
                ParticipantId(i),
                [Targeting::Spread, Targeting::Retaliate, Targeting::Leader][i as usize % 3],
            )
        })
        .collect();
    let mut l = StageBattle::new(
        ATTACKS_V0,
        POWER_CLASSES_V0,
        "itg-fa+",
        checkpoints(60),
        roster(&seats, 3 * S, 8),
        schedule(60, |j| j % 3 != 1),
    );
    let mut total = 0;
    for k in 0..60 {
        let reports: Vec<Report> = seats
            .iter()
            .map(|&(p, _)| rep(p, k, k % 3 == 0, 4))
            .collect();
        let e = l.checkpoint(k, &reports);
        let pairs = committed(&e);
        // No one attacked twice at once.
        let mut r: Vec<_> = pairs.iter().map(|p| p.1).collect();
        r.sort();
        r.dedup();
        assert_eq!(r.len(), pairs.len());
        assert!(pairs.len() <= 128);
        total += pairs.len();
        let open: Vec<_> = l
            .attacks()
            .iter()
            .filter(|a| matches!(a.state, AttackState::Active | AttackState::Cleared))
            .map(|a| a.recipient)
            .collect();
        let mut uniq = open.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), open.len(), "one attack a recipient");
    }
    assert!(total > 128, "{total}");
    assert_eq!(l.record().len(), 60);
}

#[test]
fn the_same_reports_give_the_same_record() {
    let run = || {
        let mut l = battle(
            &[
                (A, Targeting::Spread),
                (B, Targeting::Retaliate),
                (C, Targeting::Leader),
            ],
            9,
        );
        for k in 0..30 {
            l.checkpoint(
                k,
                &[
                    rep(A, k, k % 4 == 0, 3),
                    rep(B, k, k % 5 == 1, 3),
                    rep(C, k, k % 3 == 2, 3),
                ],
            );
        }
        l.record().to_vec()
    };
    assert_eq!(run(), run());
    assert!(run().iter().any(|c| !committed(&c.events).is_empty()));
}

fn entrant(p: ParticipantId, run: Run) -> Entrant {
    Entrant {
        participant: p,
        run,
    }
}

#[test]
fn settlement_waits_for_the_last_attack_and_a_knockout_by_it_is_drawn() {
    let st = SETTLEMENT_V0;
    assert_eq!(st.id, "settlement-v0");
    let live = Run::Live {
        life: 0.5,
        accuracy: 0.9,
        absolute: None,
        score: None,
        steps: 0,
    };
    // B alone in, an attack against B outstanding: not decided.
    let field = [
        entrant(A, Run::Out { at: 10.0 }),
        entrant(B, live),
        entrant(C, Run::Out { at: 10.5 }),
    ];
    assert!(!st.decided(&field, &[B]));
    assert!(st.decided(&field, &[]));
    assert!(st.decided(&field, &[A]), "against someone out: nothing");
    let cps = checkpoints(20);
    let settling = Settling {
        survivor: B,
        last_out_us: 10_500_000,
    };
    // B still in when it settles: won alone.
    let (r, drawn) = st.rank(&field, &cps, Some(settling));
    assert!(!drawn);
    assert_eq!(r[0].len(), 1);
    assert_eq!(r[0][0].participant, B);
    // B knocked out while settling: first shared with those out in the
    // same checkpoint interval as C (10 s to 12 s: A at 10 s too, since a
    // checkpoint at 10 s counts what came before it).
    let fell = [
        entrant(A, Run::Out { at: 9.5 }),
        entrant(B, Run::Out { at: 14.0 }),
        entrant(C, Run::Out { at: 10.5 }),
        entrant(ParticipantId(4), Run::Forfeit),
    ];
    let (r, drawn) = st.rank(&fell, &cps, Some(settling));
    assert!(drawn);
    let ids: Vec<Vec<u64>> = r
        .iter()
        .map(|g| g.iter().map(|f| f.participant.0).collect())
        .collect();
    assert_eq!(ids, vec![vec![2, 3], vec![1], vec![4]]);
    let fell2 = [
        entrant(A, Run::Out { at: 10.2 }),
        entrant(B, Run::Out { at: 14.0 }),
        entrant(C, Run::Out { at: 10.5 }),
    ];
    let (r, _) = st.rank(&fell2, &cps, Some(settling));
    assert_eq!(r.len(), 1, "everyone out in that interval shares first");
    // Without settling: the plain ranking.
    let (plain, drawn) = st.rank(&fell, &cps, None);
    assert!(!drawn);
    assert_eq!(plain, crate::battle::rank(&fell));
}

// `attacks-v1` (round 3b): no recovery period, and one more attack may be
// reserved behind the one on its way or passing.

const D: ParticipantId = ParticipantId(4);

/// [`schedule`] as `attacks-v1`'s maker has it: an attack is over as it
/// clears.
fn schedule_v1(n: usize) -> Schedule {
    let mut s = schedule(n, |_| true);
    for slot in &mut s.slots {
        if let Some(sec) = slot.section.as_mut() {
            sec.recover_after_us = sec.clear_us;
        }
    }
    s
}

/// A, B and C, each aiming at the leader, and D, who never fills a bar
/// and has the most life: under `attacks-v1`.
fn chain_battle() -> StageBattle {
    StageBattle::new(
        ATTACKS_V1,
        POWER_CLASSES_V0,
        "ddi",
        checkpoints(40),
        roster(
            &[
                (A, Targeting::Leader),
                (B, Targeting::Leader),
                (C, Targeting::Leader),
                (D, Targeting::Leader),
            ],
            3 * S,
            7,
        ),
        schedule_v1(40),
    )
}

/// A report whose sender has half its life (so D, at full life, leads).
fn half(mut r: Report) -> Report {
    r.life_ppm = 500_000;
    r
}

fn states(b: &StageBattle) -> Vec<AttackState> {
    b.attacks().iter().map(|a| a.state).collect()
}

#[test]
fn the_rules_are_named_and_v1_has_no_recovery_and_a_chain_of_one() {
    assert_eq!(ATTACKS_V0.id, "attacks-v0");
    assert_eq!((ATTACKS_V0.recover_judgments, ATTACKS_V0.recover_beats, ATTACKS_V0.chain), (8, 4, 0));
    assert_eq!(ATTACKS_V1.id, "attacks-v1");
    assert_eq!((ATTACKS_V1.recover_judgments, ATTACKS_V1.recover_beats, ATTACKS_V1.chain), (0, 0, 1));
    assert_eq!((ATTACKS_V1.cap, ATTACKS_V1.cost), (ATTACKS_V0.cap, ATTACKS_V0.cost));
    assert_eq!((ATTACKS_V1.chain_gap, ATTACKS_V1.cooldown_checkpoints, ATTACKS_V1.version), (0, 0, 1));
}

/// Three attackers against one target: the first two land back to back
/// (the second reserved while the first is on its way, in the first
/// section after the first's last shock), the third is blocked while one
/// passes and one waits, and launches once the first is over (there is no
/// recovery to wait for), behind the second.
#[test]
fn a_chain_lands_back_to_back_and_a_third_waits_for_the_first_to_pass() {
    let mut b = chain_battle();
    // 2 s: A's bar is full. Its attack's section is the first from D's
    // lead (5 s): the slot at 6 s, clearing at 7.85 s.
    let e = b.checkpoint(
        0,
        &[half(rep(A, 0, true, 0)), half(rep(B, 0, false, 4)), half(rep(C, 0, false, 4)), rep(D, 0, false, 4)],
    );
    assert_eq!(committed(&e), vec![(A, D)]);
    let first = b.attack(1).unwrap().clone();
    assert_eq!((first.slot, first.clear_us), (3, 7_850_000));
    // 4 s: B's bar is full while A's attack is still on its way: one more
    // may be reserved against D, in the first section after A's clears.
    let e = b.checkpoint(
        1,
        &[half(rep(A, 1, false, 4)), half(rep(B, 1, true, 0)), half(rep(C, 1, false, 4)), rep(D, 1, false, 4)],
    );
    assert_eq!(committed(&e), vec![(B, D)]);
    let second = b.attack(2).unwrap().clone();
    assert_eq!(second.slot, 4, "the slot at 8 s");
    assert!(second.cells.first().unwrap().at_us > first.clear_us, "never a shared section");
    assert_eq!(second.clear_us, 9_850_000);
    // Both are against D: the nearer first, then the one waiting.
    assert_eq!(b.incoming(D).map(|a| a.id), Some(1));
    assert_eq!(b.incoming_all(D).map(|a| a.id).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(b.outstanding().map(|(p, _)| p).collect::<Vec<_>>(), vec![D, D]);
    // 6 s: C's bar is full; A and B went out meanwhile (their attacks
    // stay). D has one passing and one waiting: no one can be attacked.
    let out = |mut r: Report, at: i64| {
        r.out_at_us = Some(at);
        r
    };
    let e = b.checkpoint(
        2,
        &[
            out(half(rep(A, 2, false, 0)), 5 * S),
            out(half(rep(B, 2, false, 0)), 5 * S + S / 2),
            half(rep(C, 2, true, 0)),
            rep(D, 2, false, 4),
        ],
    );
    assert!(committed(&e).is_empty());
    assert!(e.contains(&AttackEvent::Blocked {
        sender: C,
        reason: BlockReason::NoEligible
    }));
    assert_eq!(b.waiting(C), Some(BlockReason::NoEligible));
    assert_eq!(states(&b), vec![AttackState::Active; 2]);
    // 8 s: the first attack's last shock has passed (7.85 s): it is over
    // at once, with no recovery, so C's bar launches, behind the second.
    let e = b.checkpoint(
        3,
        &[half(rep(C, 3, false, 4)), rep(D, 3, false, 0)],
    );
    assert!(e.contains(&AttackEvent::Cleared { attack: 1 }));
    assert!(e.contains(&AttackEvent::Recovered { attack: 1 }));
    assert_eq!(committed(&e), vec![(C, D)]);
    assert_eq!(b.waiting(C), None);
    let third = b.attack(3).unwrap().clone();
    assert!(third.cells.first().unwrap().at_us > second.clear_us);
    assert!(third.cells.first().unwrap().at_us >= 8 * S + 3 * S, "from D's lead");
    assert_eq!(third.slot, 6, "the slot at 12 s");
    assert_eq!(
        states(&b),
        vec![AttackState::Recovered, AttackState::Active, AttackState::Active]
    );
    assert_eq!(b.incoming_all(D).map(|a| a.id).collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(b.stats(D).received, 3);
    // The same reports give the same record.
    assert_eq!(b.record().len(), 4);
}

/// The target goes out with one attack passing and one waiting: both
/// expire, nothing refunded; and while they are outstanding the stage is
/// not settled, the waiting one counted too.
#[test]
fn a_chain_expires_whole_and_keeps_the_stage_from_settling() {
    let mut b = chain_battle();
    b.checkpoint(
        0,
        &[half(rep(A, 0, true, 0)), half(rep(B, 0, false, 4)), half(rep(C, 0, false, 4)), rep(D, 0, false, 4)],
    );
    b.checkpoint(
        1,
        &[half(rep(A, 1, false, 4)), half(rep(B, 1, true, 0)), half(rep(C, 1, false, 4)), rep(D, 1, false, 4)],
    );
    assert_eq!(states(&b), vec![AttackState::Active; 2]);
    // Settlement: D alone in (A, B and C out) with attacks against it
    // outstanding, the waiting one among them: not decided.
    let st = SETTLEMENT_V0;
    let live = Run::Live {
        life: 1.0,
        accuracy: 0.9,
        absolute: None,
        score: None,
        steps: 0,
    };
    let field = [
        entrant(A, Run::Out { at: 4.5 }),
        entrant(B, Run::Out { at: 4.6 }),
        entrant(C, Run::Out { at: 4.7 }),
        entrant(D, live),
    ];
    let outstanding = |b: &StageBattle| b.outstanding().map(|(p, _)| p).collect::<Vec<_>>();
    assert!(!st.decided(&field, &outstanding(&b)));
    // 8 s: the first has passed; the second (8 s to 9.85 s) still waits.
    let gone = |p: ParticipantId, k: usize, at: i64| {
        let mut r = half(rep(p, k, false, 0));
        r.out_at_us = Some(at);
        r
    };
    for k in 2..4 {
        b.checkpoint(
            k,
            &[gone(A, k, 4_500_000), gone(B, k, 4_600_000), gone(C, k, 4_700_000), rep(D, k, false, 4)],
        );
    }
    assert_eq!(states(&b), vec![AttackState::Recovered, AttackState::Active]);
    assert_eq!(outstanding(&b), vec![D]);
    assert!(!st.decided(&field, &outstanding(&b)), "the waiting one counts");
    // 10 s: it has passed too: decided.
    b.checkpoint(
        4,
        &[gone(A, 4, 4_500_000), gone(B, 4, 4_600_000), gone(C, 4, 4_700_000), rep(D, 4, false, 4)],
    );
    assert_eq!(states(&b), vec![AttackState::Recovered; 2]);
    assert!(st.decided(&field, &outstanding(&b)));

    // Expiry: D out at 5 s, both attacks still to come.
    let mut b = chain_battle();
    b.checkpoint(
        0,
        &[half(rep(A, 0, true, 0)), half(rep(B, 0, false, 4)), half(rep(C, 0, false, 4)), rep(D, 0, false, 4)],
    );
    b.checkpoint(
        1,
        &[half(rep(A, 1, false, 4)), half(rep(B, 1, true, 0)), half(rep(C, 1, false, 4)), rep(D, 1, false, 4)],
    );
    let mut d = rep(D, 2, false, 0);
    d.out_at_us = Some(5 * S);
    let e = b.checkpoint(
        2,
        &[half(rep(A, 2, false, 4)), half(rep(B, 2, false, 4)), half(rep(C, 2, false, 4)), d],
    );
    assert!(e.contains(&AttackEvent::Expired { attack: 1 }));
    assert!(e.contains(&AttackEvent::Expired { attack: 2 }));
    assert_eq!(states(&b), vec![AttackState::Expired; 2]);
    assert_eq!((b.power(A), b.power(B)), (0, 0), "nothing refunded");
    assert!(b.outstanding().next().is_none());
}

/// Under `attacks-v0` nothing changed: one attack at a time against
/// anyone, until they have recovered.
#[test]
fn attacks_v0_never_chains() {
    let mut b = StageBattle::new(
        ATTACKS_V0,
        POWER_CLASSES_V0,
        "ddi",
        checkpoints(40),
        roster(&[(A, Targeting::Leader), (B, Targeting::Leader), (D, Targeting::Leader)], 3 * S, 7),
        schedule(40, |_| true),
    );
    b.checkpoint(0, &[half(rep(A, 0, true, 0)), half(rep(B, 0, false, 4)), rep(D, 0, false, 4)]);
    // B's full bar finds D under an attack: it goes to A instead.
    let e = b.checkpoint(1, &[half(rep(A, 1, false, 4)), half(rep(B, 1, true, 0)), rep(D, 1, false, 4)]);
    assert_eq!(committed(&e), vec![(B, A)]);
}

// `attacks-v2` (2026-11-13): an attack is over for the ledger once it is
// placed; attacks against one recipient chain without limit, one section
// after another; Spread prefers whoever has the fewest still to come.

const E: ParticipantId = ParticipantId(5);

fn v2_battle(rules: AttackRules, seats: &[(ParticipantId, Targeting)], n: usize) -> StageBattle {
    StageBattle::new(
        rules,
        POWER_CLASSES_V0,
        "ddi",
        checkpoints(n),
        roster(seats, 3 * S, 7),
        schedule_v1(n),
    )
}

/// The checkpoint's reports: `full` fill their bars, the rest judge four
/// of the worst; everyone but `leader` at half life.
fn round(k: usize, all: &[ParticipantId], full: &[ParticipantId], leader: ParticipantId) -> Vec<Report> {
    all.iter()
        .map(|&p| {
            let r = rep(p, k, full.contains(&p), 4);
            if p == leader { r } else { half(r) }
        })
        .collect()
}

#[test]
fn the_rules_are_named_and_v2_chains_without_limit() {
    assert_eq!(ATTACKS, ATTACKS_V2, "what stages play");
    assert_eq!(ATTACKS_V2.id, "attacks-v2");
    assert_eq!(ATTACKS_V2.chain, CHAIN_UNLIMITED);
    assert_eq!(
        (ATTACKS_V2.chain_gap, ATTACKS_V2.cooldown_checkpoints, ATTACKS_V2.version),
        (0, 0, 2),
        "no gap, no cooldown"
    );
    assert_eq!(
        (ATTACKS_V2.cap, ATTACKS_V2.cost, ATTACKS_V2.checkpoint_beats),
        (ATTACKS_V1.cap, ATTACKS_V1.cost, ATTACKS_V1.checkpoint_beats)
    );
    assert_eq!((ATTACKS_V2.recover_judgments, ATTACKS_V2.recover_beats), (0, 0));
    for r in ALL_ATTACKS {
        assert_eq!(attack_rules(r.id), Some(r));
    }
    assert_eq!(attack_rules("attacks-v9"), None);
    assert_eq!(SETTLEMENT, SETTLEMENT_V1);
    assert_eq!(SETTLEMENT_V1.id, "settlement-v1");
}

/// Four attackers aiming at the leader fill their bars at once: all four
/// are placed against them at that checkpoint, in four consecutive safe
/// sections (never the same one), the first from their lead. Nobody is
/// skipped for being under attack.
#[test]
fn v2_places_a_chain_of_any_length_one_section_after_another() {
    let all = [A, B, C, D, E];
    let mut b = v2_battle(
        ATTACKS_V2,
        &[
            (A, Targeting::Leader),
            (B, Targeting::Leader),
            (C, Targeting::Leader),
            (D, Targeting::Leader),
            (E, Targeting::Leader),
        ],
        40,
    );
    let e = b.checkpoint(0, &round(0, &all, &[A, B, C, E], D));
    assert_eq!(committed(&e).len(), 4);
    assert!(committed(&e).iter().all(|(_, r)| *r == D), "{e:?}");
    let slots: Vec<usize> = b.attacks().iter().map(|a| a.slot).collect();
    assert_eq!(slots, vec![3, 4, 5, 6], "from D's lead (5 s: the slot at 6 s), one after another");
    for w in b.attacks().windows(2) {
        assert!(w[1].cells[0].at_us >= w[0].clear_us, "never the same region");
    }
    assert_eq!(b.incoming_all(D).count(), 4);
    assert_eq!(b.stats(D).received, 4);
    // The next checkpoint: A full again, D still the leader: a fifth,
    // behind the fourth.
    let e = b.checkpoint(1, &round(1, &all, &[A], D));
    assert_eq!(committed(&e), vec![(A, D)]);
    assert_eq!(b.attacks()[4].slot, 7);
    // As they pass, each is cleared; nothing waits on them.
    for k in 2..6 {
        b.checkpoint(k, &round(k, &all, &[], D));
    }
    assert_eq!(b.attacks()[0].state, AttackState::Recovered);
    assert_eq!(b.incoming(D).map(|a| a.id), Some(4), "at 12 s, the one from 12 s next");
    assert_eq!(b.incoming_all(D).count(), 2);
}

/// Spread under `attacks-v2` prefers whoever has the fewest attacks still
/// to come, so a batch of launches spreads over the field.
#[test]
fn v2_spread_prefers_the_fewest_still_to_come() {
    let all = [A, B, C, D];
    let seats: Vec<_> = all.iter().map(|&p| (p, Targeting::Spread)).collect();
    let mut b = v2_battle(ATTACKS_V2, &seats, 40);
    b.checkpoint(0, &round(0, &all, &all, D));
    let mut got: Vec<ParticipantId> = b.attacks().iter().map(|a| a.recipient).collect();
    got.sort();
    assert_eq!(got, all.to_vec(), "one each");
    // One more round: each sender picks among the others whoever has
    // the fewest still to come (a sender never picks itself, so the last
    // of a round may find only those with one more).
    let before: Vec<usize> = all.iter().map(|&p| b.incoming_all(p).count()).collect();
    b.checkpoint(1, &round(1, &all, &all, D));
    let counts: Vec<usize> = all.iter().map(|&p| b.incoming_all(p).count()).collect();
    assert_eq!(before, vec![1; 4]);
    assert_eq!(counts.iter().sum::<usize>(), 8);
}

/// No safe section left before the song's end: blocked, the power kept;
/// the search reaches to the song's end, not 16 beats.
#[test]
fn v2_blocks_when_no_section_is_left_and_searches_to_the_end() {
    let all = [A, B, C, D];
    let seats: Vec<_> = all.iter().map(|&p| (p, Targeting::Leader)).collect();
    // Sections only at 6 s, 8 s and at 40 s (far past 16 beats from 6 s).
    let mut b = StageBattle::new(
        ATTACKS_V2,
        POWER_CLASSES_V0,
        "ddi",
        checkpoints(30),
        roster(&seats, 3 * S, 7),
        {
            let mut s = schedule_v1(30);
            for (j, slot) in s.slots.iter_mut().enumerate() {
                if !matches!(j, 3 | 4 | 20) {
                    slot.section = None;
                }
            }
            s
        },
    );
    let e = b.checkpoint(0, &round(0, &all, &[A, B, C], D));
    assert_eq!(committed(&e).len(), 3);
    assert_eq!(b.attacks().iter().map(|a| a.slot).collect::<Vec<_>>(), vec![3, 4, 20]);
    let e = b.checkpoint(1, &round(1, &all, &[A], D));
    assert!(committed(&e).is_empty());
    assert_eq!(b.waiting(A), Some(BlockReason::NoSafeSection));
    assert_eq!(b.power(A), 100, "kept");
}

/// The named places for a limit: a gap of one safe section between
/// chained attacks, and a sender's cooldown. Neither is on by default.
#[test]
fn v2_gap_and_cooldown_are_named_rules_off_by_default() {
    let all = [A, B, C, D];
    let seats: Vec<_> = all.iter().map(|&p| (p, Targeting::Leader)).collect();
    let gap = AttackRules {
        id: "attacks-v2-gap1-test",
        chain_gap: 1,
        ..ATTACKS_V2
    };
    let mut b = v2_battle(gap, &seats, 40);
    b.checkpoint(0, &round(0, &all, &[A, B, C], D));
    assert_eq!(b.attacks().iter().map(|a| a.slot).collect::<Vec<_>>(), vec![3, 5, 7]);

    let cool = AttackRules {
        id: "attacks-v2-cooldown-test",
        cooldown_checkpoints: 2,
        ..ATTACKS_V2
    };
    let mut b = v2_battle(cool, &seats, 40);
    b.checkpoint(0, &round(0, &all, &[A], D));
    let e = b.checkpoint(1, &round(1, &all, &[A], D));
    assert!(committed(&e).is_empty(), "cooling down");
    assert_eq!(b.waiting(A), None, "no reason told");
    let e = b.checkpoint(2, &round(2, &all, &[], D));
    assert_eq!(committed(&e), vec![(A, D)]);
}

/// `settlement-v1`: the one left while attacks against them are still to
/// come is last standing; won once every one has passed; a knockout by
/// any of them shares first ("Drawn: final attack"). An attack committed
/// at the checkpoint the last other one goes out after counts.
#[test]
fn settlement_v1_tells_last_standing_before_the_win() {
    let all = [A, B, C, D];
    let seats: Vec<_> = all.iter().map(|&p| (p, Targeting::Leader)).collect();
    let mut b = v2_battle(ATTACKS_V2, &seats, 40);
    // A, B and C fill their bars at the checkpoint at 2 s and go out
    // after it (2.5 s, 3 s, 3.5 s): three attacks against D, placed.
    b.checkpoint(0, &round(0, &all, &[A, B, C], D));
    assert_eq!(b.incoming_all(D).count(), 3);
    let st = SETTLEMENT;
    let live = Run::Live {
        life: 1.0,
        accuracy: 0.9,
        absolute: None,
        score: None,
        steps: 0,
    };
    let field = [
        entrant(A, Run::Out { at: 2.5 }),
        entrant(B, Run::Out { at: 3.0 }),
        entrant(C, Run::Out { at: 3.5 }),
        entrant(D, live),
    ];
    let outstanding = |b: &StageBattle| b.outstanding().map(|(p, _)| p).collect::<Vec<_>>();
    assert_eq!(st.last_standing(&field, &outstanding(&b)), Some(D));
    assert!(!st.decided(&field, &outstanding(&b)));
    let gone = |p: ParticipantId, k: usize, at: i64| {
        let mut r = half(rep(p, k, false, 0));
        r.out_at_us = Some(at);
        r
    };
    let mut k = 1;
    // Through the three sections (6 s to 11.85 s).
    while b.outstanding().next().is_some() {
        let e = b.checkpoint(
            k,
            &[gone(A, k, 2_500_000), gone(B, k, 3 * S), gone(C, k, 3_500_000), rep(D, k, false, 4)],
        );
        assert!(committed(&e).is_empty(), "nobody out launches");
        if b.outstanding().next().is_some() {
            assert_eq!(st.last_standing(&field, &outstanding(&b)), Some(D));
        }
        k += 1;
    }
    assert_eq!(k, 6, "the last clears before 12 s");
    assert_eq!(st.last_standing(&field, &outstanding(&b)), None);
    assert!(st.decided(&field, &outstanding(&b)), "won");
    // Two still in: nobody is last standing.
    let two = [entrant(A, live), entrant(D, live)];
    assert_eq!(st.last_standing(&two, &[D]), None);
    // Nothing outstanding: last standing and won at once: only won.
    assert_eq!(st.last_standing(&field, &[]), None);
    assert!(st.decided(&field, &[]));
    // D knocked out by the second attack: first shared with everyone
    // out in the same checkpoint interval as the one before them (2.5 s,
    // 3 s and 3.5 s: all between 2 s and 4 s).
    let fell = [
        entrant(A, Run::Out { at: 2.5 }),
        entrant(B, Run::Out { at: 3.0 }),
        entrant(C, Run::Out { at: 3.5 }),
        entrant(D, Run::Out { at: 9.0 }),
    ];
    let settling = Settling {
        survivor: D,
        last_out_us: 3_500_000,
    };
    let (r, drawn) = st.rank(&fell, b.checkpoints_us(), Some(settling));
    assert!(drawn);
    let mut first: Vec<_> = r[0].iter().map(|f| f.participant).collect();
    first.sort();
    assert_eq!(first, vec![A, B, C, D]);
}
