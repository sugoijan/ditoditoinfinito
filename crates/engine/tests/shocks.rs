//! DDR's shock arrows (a mine in every lane of a row) under DDR A, and the
//! same rows as StepMania's mines under the other rules.
//!
//! Chart (60 BPM, so beat `b` is at `b` seconds), dance-single:
//!
//! | beat | lanes      | kind                     |
//! |------|------------|--------------------------|
//! | 0    | 0          | tap                      |
//! | 2    | 0, 1, 2, 3 | mines: a shock under DDR A |
//! | 4    | 3          | tap                      |
//! | 5    | 0, 1       | mines (not a full row)   |
//! | 6    | 2          | tap                      |

use ddi_chart::{
    Chart, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, SourceInfo, Tick, TimingMap,
};
use ddi_engine::frame::SkinEventKind;
use ddi_engine::player::{PlayOptions, Player, SHOCK_BLIND_SECONDS};
use ddi_engine::rules::FullCombo;
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::{InputEvent, JudgeEventKind};
use ddi_platform::{ClockSample, HostTime};

fn song() -> Song {
    let t = Tick::from_beats;
    let mut notes = vec![
        Note::new(t(0), 0, NoteKind::Tap),
        Note::new(t(4), 3, NoteKind::Tap),
        Note::new(t(5), 0, NoteKind::Mine),
        Note::new(t(5), 1, NoteKind::Mine),
        Note::new(t(6), 2, NoteKind::Tap),
    ];
    notes.extend((0..4).map(|lane| Note::new(t(2), lane, NoteKind::Mine)));
    notes.sort_by_key(|n| (n.tick, n.lane));
    Song {
        title: "Shocks".into(),
        subtitle: String::new(),
        artist: "test".into(),
        title_translit: String::new(),
        subtitle_translit: String::new(),
        artist_translit: String::new(),
        genre: String::new(),
        credit: String::new(),
        music: None,
        preview_start: 0.0,
        preview_length: 0.0,
        banner: None,
        background: None,
        jacket: None,
        cd_title: None,
        timing: TimingMap::constant(60.0, 0.0),
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            derivation: None,
            layout: "dance-single".into(),
            difficulty: Difficulty::Challenge,
            meter: 9,
            name: "shocks".into(),
            description: String::new(),
            credit: String::new(),
            notes,
            timing: None,
            display_bpm: None,
            danoni: None,
        }],
        effects: Vec::new(),
        keysounds: Vec::new(),
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Other("test".into()),
            unknown_tags: Vec::new(),
        },
    }
}

/// The three taps on time.
fn taps() -> Vec<(f64, u8, bool)> {
    vec![
        (0.0, 0, true),
        (0.05, 0, false),
        (4.0, 3, true),
        (4.05, 3, false),
        (6.0, 2, true),
        (6.05, 2, false),
    ]
}

/// Plays `script` (`(song seconds, lane, pressed)`), updating every 10 ms;
/// `at` sees the player after each update.
fn play(
    preset: &str,
    script: &[(f64, u8, bool)],
    at: impl FnMut(f64, &Player),
) -> (Player, Vec<JudgeEventKind>) {
    play_song(&song(), preset, HoldStyle::ByGame, script, at)
}

/// [`play`] on `song`, its holds kept as `holds` says.
fn play_song(
    song: &Song,
    preset: &str,
    holds: HoldStyle,
    script: &[(f64, u8, bool)],
    mut at: impl FnMut(f64, &Player),
) -> (Player, Vec<JudgeEventKind>) {
    let ruleset = presets::for_chart(
        RulesetMode::AllStepMania,
        preset,
        &DanoniOptions::default(),
        holds,
        &song.charts[0],
    );
    let mut script = script.to_vec();
    script.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut player = Player::new(song, 0, ruleset, PlayOptions::default());
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    player.start(0.0, 0.0);
    let mut events = Vec::new();
    let mut next = 0;
    for step in 0..=900u32 {
        let now = f64::from(step) / 100.0;
        while next < script.len() && script[next].0 <= now {
            let (t, lane, pressed) = script[next];
            events.extend(player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            }));
            next += 1;
        }
        events.extend(player.update(HostTime(now)));
        at(now, &player);
    }
    assert!(player.finished());
    replays_alike(song, &player);
    (player, events.into_iter().map(|e| e.kind).collect())
}

#[test]
fn a_shock_avoided_is_one_ok_that_adds_to_the_combo() {
    // O.K. shows over every lane of the row as it passes.
    let mut oks: Vec<Vec<u8>> = Vec::new();
    let (player, events) = play("ddr-a", &taps(), |now, p| {
        if (2.0..2.6).contains(&now) {
            let f = p.frame(HostTime(now));
            assert!(f.holds.flashes.iter().all(|h| h.held), "{:?}", f.holds);
            oks.push(f.holds.flashes.iter().map(|h| h.lane).collect());
        }
    });
    assert!(oks.iter().any(|l| l == &[0, 1, 2, 3]), "{oks:?}");
    let shocks: Vec<_> = events
        .iter()
        .filter(|k| matches!(k, JudgeEventKind::AvoidShock | JudgeEventKind::HitShock))
        .collect();
    assert_eq!(shocks, [&JudgeEventKind::AvoidShock]);
    let r = player.results();
    // Three steps and the shock: one combo and one O.K. each.
    assert_eq!(r.max_combo, 4);
    assert_eq!((r.tally.held, r.tally.let_go, r.tally.mine_hit), (1, 0, 0));
    assert_eq!(r.score.money, Some(1_000_000));
    assert_eq!((r.score.ex, r.score.max_ex), (Some(12), Some(12)));
    assert_eq!(r.full_combo, FullCombo::MarvelousFC);
    // The two mines at beat 5 stay mines.
    assert_eq!(player.score_ctx().mines, 2);
    assert_eq!(player.score_ctx().shocks, 1);
}

#[test]
fn a_shock_stepped_on_in_any_lane_is_one_ng_and_hides_what_comes() {
    let mut script = taps();
    script.extend([(2.0, 2, true), (2.05, 2, false)]);
    let mut shown = Vec::new();
    let (player, events) = play("ddr-a", &script, |now, p| {
        if (1.9..2.5).contains(&now) {
            shown.push((now, p.frame(HostTime(now)).notes.len()));
        }
    });
    let shocks: Vec<_> = events
        .iter()
        .filter(|k| matches!(k, JudgeEventKind::AvoidShock | JudgeEventKind::HitShock))
        .collect();
    assert_eq!(shocks, [&JudgeEventKind::HitShock]);
    let r = player.results();
    // An N.G.: the combo broken, a quarter of the score gone.
    assert_eq!((r.tally.held, r.tally.let_go), (0, 1));
    assert_eq!(r.max_combo, 2);
    assert_eq!(r.score.money, Some(750_000));
    assert_eq!(r.full_combo, FullCombo::None);
    // Every lane's shock goes off.
    let lanes: Vec<u8> = player
        .skin_events_since(0)
        .filter(|e| e.kind == SkinEventKind::HitMine)
        .map(|e| e.lane)
        .collect();
    assert_eq!(lanes, [0, 1, 2, 3]);
    // N.G. over each of them.
    let f = player.frame(HostTime(2.3));
    let ng: Vec<(u8, bool)> = f.holds.flashes.iter().map(|h| (h.lane, h.held)).collect();
    assert_eq!(ng, [(0, false), (1, false), (2, false), (3, false)]);
    // The notes to come vanish for a moment, then come back.
    for (now, n) in shown {
        let blind = (2.0..2.0 + SHOCK_BLIND_SECONDS - 1e-9).contains(&now);
        assert_eq!(n == 0, blind, "{n} notes shown at {now}");
    }
}

#[test]
fn with_ticked_holds_a_shock_still_shows_its_ok() {
    // Ticked holds show no O.K. of their own; shocks are not holds.
    let mut oks = 0;
    play_song(&song(), "ddr-a", HoldStyle::Ticked, &taps(), |now, p| {
        if (2.0..2.6).contains(&now) {
            oks += p.frame(HostTime(now)).holds.flashes.len();
        }
    });
    assert!(oks > 0);
}

#[test]
fn the_play_ends_after_the_last_ng_has_shown() {
    // The shock is the last row: stepped on, its N.G. shows 1.4 s, past
    // the second the play would otherwise end after it.
    let mut song = song();
    song.charts[0]
        .notes
        .retain(|n| n.tick <= Tick::from_beats(2));
    let script = [
        (0.0, 0, true),
        (0.05, 0, false),
        (2.0, 2, true),
        (2.05, 2, false),
    ];
    let mut end = None;
    let (player, _) = play_song(&song, "ddr-a", HoldStyle::ByGame, &script, |now, p| {
        if p.finished() && end.is_none() {
            end = Some(now);
            let f = p.frame(HostTime(now));
            assert!(f.holds.flashes.is_empty(), "{:?}", f.holds);
        }
    });
    let end = end.unwrap();
    assert!(end > player.song_end_time() + 0.3, "{end}");
    assert!(end < 2.0 + 1.6, "{end}");
}

#[test]
fn a_panel_held_as_the_shock_passes_steps_on_it() {
    let mut script = taps();
    script.extend([(1.5, 1, true), (2.5, 1, false)]);
    let (player, _) = play("ddr-a", &script, |_, _| {});
    assert_eq!(player.results().tally.let_go, 1);
}

#[test]
fn other_rules_keep_the_row_as_mines() {
    let mut script = taps();
    script.extend([(2.0, 2, true), (2.05, 2, false)]);
    for preset in ["itg", "sm5"] {
        let (player, events) = play(preset, &script, |_, _| {});
        assert!(
            events
                .iter()
                .all(|k| !matches!(k, JudgeEventKind::AvoidShock | JudgeEventKind::HitShock))
        );
        let r = player.results();
        assert_eq!((r.tally.mine_hit, r.tally.let_go), (1, 0));
        assert_eq!(player.score_ctx().shocks, 0);
        assert_eq!(player.score_ctx().mines, 6);
    }
}

#[test]
fn on_double_a_shock_is_all_eight_lanes() {
    // A row of eight mines and one of the left pad's four.
    let mut song = song();
    let t = Tick::from_beats;
    let chart = &mut song.charts[0];
    chart.layout = "dance-double".into();
    chart.notes = (0..8)
        .map(|lane| Note::new(t(2), lane, NoteKind::Mine))
        .chain((0..4).map(|lane| Note::new(t(4), lane, NoteKind::Mine)))
        .chain([Note::new(t(6), 7, NoteKind::Tap)])
        .collect();
    let ruleset = |id| {
        presets::for_chart(
            RulesetMode::AllStepMania,
            id,
            &DanoniOptions::default(),
            HoldStyle::ByGame,
            &song.charts[0],
        )
    };
    let player = Player::new(&song, 0, ruleset("ddr-a"), PlayOptions::default());
    assert_eq!(player.lanes(), 8);
    assert_eq!(
        (player.score_ctx().shocks, player.score_ctx().mines),
        (1, 4)
    );
    let player = Player::new(&song, 0, ruleset("itg"), PlayOptions::default());
    assert_eq!(
        (player.score_ctx().shocks, player.score_ctx().mines),
        (0, 12)
    );
}

/// The play judged again from its log ([`ddi_engine::history`]) gives
/// the same results, under its ruleset and, when the ruleset is one a key
/// builds (not tuned by the test), under its key.
fn replays_alike(song: &Song, player: &Player) {
    use ddi_engine::history::{replay, replay_with};
    assert!(player.finished());
    let ruleset = player.ruleset();
    let again = replay_with(song, 0, ruleset.fresh(), player.options(), player.log());
    assert_eq!(again, player.results());
    let built = ruleset.key().build(&song.charts[0]);
    if format!("{built:?}") == format!("{ruleset:?}") && built.fail == ruleset.fail {
        let again = replay(song, 0, &ruleset.key(), player.options(), player.log());
        assert_eq!(again, player.results());
    }
}
