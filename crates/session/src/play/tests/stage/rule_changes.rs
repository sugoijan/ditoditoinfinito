//! A practice match whose per-stage rules change from a later stage on
//! (2026-11-13, `ddi_room::Command::ChangeStageRules`): Hard timing from
//! stage 2, attacks off from stage 3; each stage played, judged and
//! ranked under its own rules, the earlier ones as they were.

use super::*;

#[test]
fn each_stage_is_played_under_its_own_rules_after_a_change() {
    let song = bundled("beatucada");
    let chart = song
        .charts
        .iter()
        .position(|c| c.layout == "dance-single" && c.derivation.is_none())
        .unwrap();
    let mut room = attacks_room(&song, 3, 5);
    let rules = MatchRules {
        stages: 3,
        ..room.rules().clone()
    };
    room.save_rules(rules).unwrap();
    let mut now = 0;
    let mut played = Vec::new();
    for stage in 1..=3u16 {
        let order = begin_at(&song, &mut room, &mut now, chart);
        assert_eq!(order.number, stage);
        // While stage 1 plays, Hard timing from stage 2; while stage 2
        // plays, attacks off from stage 3.
        match stage {
            1 => {
                let (from, n) = room.rules_land_on().unwrap();
                assert_eq!(n, 2);
                let mut r = room.rules_ahead().unwrap();
                r.difficulty = ddi_room::MatchDifficulty::Hard;
                room.change_stage_rules(from, &r).unwrap();
            }
            2 => {
                let (from, n) = room.rules_land_on().unwrap();
                assert_eq!(n, 3);
                let mut r = room.rules_ahead().unwrap();
                r.set_attacks(false);
                room.change_stage_rules(from, &r).unwrap();
            }
            _ => assert_eq!(room.rules_land_on(), None, "no stage left"),
        }
        let s = play_at(&song, &order, 60.0);
        let judge = s.player.ruleset().judge.clone();
        let attacks = s.battle().unwrap().attacks().is_some();
        let mut s = s;
        let outcome = s.take_stage_outcome().expect("its outcome");
        room.report_stage(outcome.stage, outcome.ranking, outcome.confirmed_early)
            .unwrap();
        played.push((order, judge, attacks));
    }
    let (o1, j1, a1) = &played[0];
    let (o2, j2, a2) = &played[1];
    let (o3, j3, a3) = &played[2];
    // Stage 1: the match's own (Normal, attacks); 2: Hard, attacks; 3:
    // Hard, none.
    assert_eq!(o1.rules.difficulty, ddi_room::MatchDifficulty::Normal);
    assert!(!o1.rules.changed && o2.rules.changed && o3.rules.changed);
    assert_eq!(o2.rules.difficulty, ddi_room::MatchDifficulty::Hard);
    assert_eq!(o3.rules.difficulty, ddi_room::MatchDifficulty::Hard);
    assert_ne!(j1, j2, "Hard timing judges with other windows");
    assert_eq!(j2, j3);
    assert_eq!(*j2, crate::battle::stage_ruleset(&o2.rules).judge);
    assert_eq!(o1.battle, ddi_room::BattlePreset::GaugeAttacksV0);
    assert_eq!(o2.battle, ddi_room::BattlePreset::GaugeAttacksV0);
    assert_eq!(o3.battle, ddi_room::BattlePreset::GaugeSurvivalV0);
    assert!(*a1 && *a2 && !*a3, "attacks played where the stage had them");
    // Ranked and recorded: every stage confirmed, each table its rules.
    assert_eq!(room.state().phase, Phase::MatchResults);
    for (o, changed) in [(o1, false), (o2, true), (o3, true)] {
        let t = room.stage_table(o.stage).unwrap();
        assert!(t.confirmed);
        assert_eq!(t.rules.changed, changed);
    }
    assert!(!room.stage_table(o3.stage).unwrap().attacks);
}
