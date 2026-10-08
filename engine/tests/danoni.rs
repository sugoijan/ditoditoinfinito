//! Dancing☆Onigiri works through the engine: imported charts autoplay
//! perfectly (holds, dummies, boosts, two-row and header-defined key modes),
//! speed changes move notes without moving their judged times, and the
//! player's speed means the same as on a StepMania song at the work's tempo.

use ddi_chart::formats::danoni::{self, Dos, DosFlags};
use ddi_chart::{DisplayBpm, NoteKind, Song, Tick};
use ddi_engine::autoplay;
use ddi_engine::player::{PlayOptions, Player};
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::presets::RulesetMode;
use ddi_engine::rules::{FullCombo, Ruleset, presets};
use ddi_engine::scroll::{
    ScrollOptions, SpeedMod, SpeedSource, note_y, speed_for_chart, speed_for_song,
};
use ddi_engine::{InputEvent, TransformOptions, Turn};
use ddi_platform::{ClockSample, HostTime};

/// A work at the default blank frame (200): frame F sounds at (F − 200)/60 s.
fn work(text: &str) -> Song {
    let i = danoni::import(&Dos::parse(text, DosFlags::default())).unwrap();
    // Only the deliberate duplicate in SEVEN is reported.
    assert!(
        i.warnings.iter().all(|w| w.contains("1 tap(s)")),
        "{:?}",
        i.warnings
    );
    i.songs.into_iter().next().unwrap()
}

/// Autoplays `song`'s first chart; returns the player at the end.
fn autoplay(ruleset: &dyn Fn() -> Ruleset, song: &Song, options: PlayOptions) -> Player {
    let probe = Player::new(song, 0, ruleset(), options);
    let mut script: Vec<(f64, u8, bool)> = autoplay::script(probe.judge().notes(), 0.0)
        .into_iter()
        .flat_map(|p| [(p.press, p.lane, true), (p.release, p.lane, false)])
        .collect();
    script.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
    let mut player = Player::new(song, 0, ruleset(), options);
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    player.start(0.0, 0.0);
    let mut next = 0;
    for step in 0..=2000u32 {
        let now = f64::from(step) / 100.0;
        player.update(HostTime(now));
        let _ = player.frame(HostTime(now));
        while next < script.len() && script[next].0 < now + 0.01 - 1e-9 {
            let (t, lane, pressed) = script[next];
            player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            });
            next += 1;
        }
    }
    player
}

const SEVEN: &str = "|musicTitle=T,A,,,180|difData=7,Hard|dummyId=2|\
    |left_data=260,320,380,500|leftdia_data=290,410|down_data=350,350.4|\
    |space_data=440|up_data=470|rightdia_data=530|right_data=560,620|\
    |frzLeft_data=640,700|frzRight_data=660,720|frzLdia_data=740,800|\
    |left2_data=300,600|frzDown2_data=450,480|\
    |speed_data=400,2,500,0.5,600,1|boost_data=520,1.5|";

const SEVEN_I: &str = "|difData=7i,N|\
    |left_data=260|leftdia_data=280|down_data=300|space_data=320|\
    |up_data=340|rightdia_data=360|right_data=380|foni_data=400,460|";

const NINE_B: &str = "|difData=9B,N|\
    |left_data=260|down_data=280|up_data=300|right_data=320|space_data=340|\
    |sleft_data=360|sdown_data=380|sup_data=400|sright_data=420|sfrzLeft_data=460,520|";

#[test]
fn works_autoplay_perfectly_under_every_turn() {
    let judged_starts = format!("{SEVEN}|frzStartjdgUse=true|excessiveJdgUse=true|");
    for text in [SEVEN, SEVEN_I, NINE_B, &judged_starts] {
        let song = work(text);
        let layout = song.layout_of(&song.charts[0]).unwrap();
        let chart = song.charts[0].clone();
        let own = move || {
            presets::for_chart(
                RulesetMode::Original,
                "itg",
                &DanoniOptions::default(),
                &chart,
            )
        };
        let rulesets: [&dyn Fn() -> Ruleset; 5] = [
            &presets::itg,
            &presets::sm5,
            &presets::ddr_a,
            &presets::danoni,
            &own,
        ];
        for ruleset in rulesets {
            for (turn, seed) in [(Turn::Off, 0), (Turn::Mirror, 0), (Turn::Shuffle, 5)] {
                let options = PlayOptions {
                    transform: TransformOptions {
                        turn,
                        ..Default::default()
                    },
                    seed,
                    ..PlayOptions::default()
                };
                let p = autoplay(ruleset, &song, options);
                let r = p.results();
                let ctx = p.score_ctx();
                let label = format!("{} {} {turn:?}", layout.id, p.ruleset().id);
                assert!(ctx.steps > 0, "{label}");
                assert_eq!(r.tally.taps[0], ctx.steps, "{label}: {:?}", r.tally);
                assert_eq!(r.tally.taps.iter().sum::<u32>(), ctx.steps, "{label}");
                assert_eq!(r.tally.held, ctx.holds, "{label}");
                assert_eq!((r.tally.let_go, r.tally.mine_hit), (0, 0), "{label}");
                assert_eq!(r.full_combo, FullCombo::MarvelousFC, "{label}");
                assert_eq!((r.fast, r.slow), (0, 0), "{label}");
                assert!(p.finished() && !p.failed(), "{label}");
                if p.ruleset().id == "danoni" {
                    assert_eq!(r.score.money, Some(1_000_000), "{label}");
                    assert_eq!(r.grade, "AP", "{label}");
                }
            }
        }
    }
}

#[test]
fn works_in_other_key_modes_are_left_out() {
    for text in [
        "|difData=11,N|left_data=300|",
        "|difData=6x,N|keyCtrl6x=4A,S,D|left_data=300|",
    ] {
        let r = danoni::import(&Dos::parse(text, DosFlags::default()));
        assert!(r.is_err(), "{text}");
    }
}

#[test]
fn imported_chart_shape() {
    let song = work(SEVEN);
    let c = &song.charts[0];
    // 350.4 rounds onto 350, which is a tap on the same frame: dropped.
    // 13 taps and 3 holds remain, plus dummies from chart 2.
    let judged = c
        .notes
        .iter()
        .filter(|n| matches!(n.kind, NoteKind::Tap | NoteKind::HoldHead { .. }))
        .count();
    assert_eq!(judged, 15);
    let dummies = c.notes.iter().filter(|n| n.kind == NoteKind::Dummy).count();
    assert_eq!(dummies, 3);
    assert_eq!(song.display_bpm, DisplayBpm::Single(180.0));
}

#[test]
fn speed_changes_move_notes_but_not_judged_times() {
    let song = work(SEVEN);
    let timing = song.charts[0].timing(&song).clone();
    // Judged times follow the frames alone.
    let at = |f: i64| timing.seconds_at(Tick(f));
    assert!((at(500) - 5.0).abs() < 1e-9);
    // From frame 400 notes move twice as fast: a note 60 frames ahead is
    // twice as far as before the change.
    let opts = ScrollOptions {
        speed: SpeedMod::XMod(1.0),
        ..ScrollOptions::default()
    };
    let before = note_y(opts, &timing, Tick(380), at(320));
    let after = note_y(opts, &timing, Tick(460), at(400));
    assert!((after / before - 2.0).abs() < 1e-4, "{before} {after}");
}

#[test]
fn a_boosted_hold_scales_its_tail_with_its_head() {
    // Two identical holds; the second is boosted ×2.
    let song = work("|difData=5,N|frzLeft_data=500,560|frzDown_data=900,960|boost_data=700,2|");
    let mut p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    p.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    p.start(0.0, 0.0);
    let span = |p: &mut Player, lane: u8, t: f64| {
        let f = p.frame(HostTime(t));
        let n = f
            .notes
            .iter()
            .find(|n| n.lane == lane)
            .expect("note on screen");
        match n.kind {
            ddi_engine::SpriteKind::HoldHead { tail_y, .. } => (n.y, tail_y),
            _ => panic!("not a hold"),
        }
    };
    // 2 s before each head (frames 380 and 780, (F − 200) / 60 s).
    let (h1, t1) = span(&mut p, 0, 3.0 - 2.0);
    let (h2, t2) = span(&mut p, 1, 3.0 + 400.0 / 60.0 - 2.0);
    assert!((h2 / h1 - 2.0).abs() < 1e-4, "{h1} {h2}");
    assert!((t2 / t1 - 2.0).abs() < 1e-4, "{t1} {t2}");
}

#[test]
fn speed_means_the_same_as_on_a_stepmania_song() {
    let song = work(SEVEN);
    // x2 at the work's 180 BPM: 2 × 180 / 75 against the synthetic tempo.
    assert_eq!(
        speed_for_song(&song, SpeedMod::XMod(2.0)),
        SpeedMod::XMod(2.0 * 180.0 / 75.0)
    );
    assert_eq!(
        speed_for_song(&song, SpeedMod::CMod(300.0)),
        SpeedMod::XMod(4.0)
    );
    // Without a declared BPM, 150.
    let plain = work("|difData=5,N|left_data=300|");
    assert_eq!(
        speed_for_song(&plain, SpeedMod::XMod(1.0)),
        SpeedMod::XMod(2.0)
    );
    // StepMania songs are untouched.
    let mut sm = plain.clone();
    sm.source.format = ddi_chart::SourceFormat::Sm;
    assert_eq!(
        speed_for_song(&sm, SpeedMod::XMod(1.5)),
        SpeedMod::XMod(1.5)
    );
    // At x1 on 150 BPM a note one beat (0.4 s) away is one arrow up, as on
    // a StepMania song at 150 BPM.
    let timing = plain.charts[0].timing(&plain).clone();
    let opts = ScrollOptions {
        speed: speed_for_song(&plain, SpeedMod::XMod(1.0)),
        ..ScrollOptions::default()
    };
    let y = note_y(opts, &timing, Tick(300), timing.seconds_at(Tick(300)) - 0.4);
    assert!((y - 1.0).abs() < 1e-4, "{y}");
}

#[test]
fn a_chart_can_bring_its_own_speed() {
    // difData speed 4: 4 × 2.4 = 9.6 arrow heights a second, at the
    // synthetic 75 BPM (1.25 beats a second) an x-mod of 7.68.
    let song = work("|musicTitle=T,A,,,180|difData=5,N,4|left_data=300|");
    let own = speed_for_chart(&song, 0, SpeedMod::XMod(2.0), SpeedSource::Chart);
    assert!(
        matches!(own, SpeedMod::XMod(x) if (x - 7.68).abs() < 1e-9),
        "{own:?}"
    );
    // The player's speed otherwise, and on charts without one.
    assert_eq!(
        speed_for_chart(&song, 0, SpeedMod::XMod(2.0), SpeedSource::Player),
        speed_for_song(&song, SpeedMod::XMod(2.0))
    );
    let mut sm = song.clone();
    sm.source.format = ddi_chart::SourceFormat::Sm;
    sm.charts[0].danoni = None;
    assert_eq!(
        speed_for_chart(&sm, 0, SpeedMod::XMod(2.0), SpeedSource::Chart),
        SpeedMod::XMod(2.0)
    );
}
