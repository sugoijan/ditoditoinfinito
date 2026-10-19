//! Calibration by ear ([`Player::judge_all_in`]): a press in any lane is
//! judged in one, while the receptors show the lane really pressed.
//!
//! Chart (60 BPM, beat `b` at `b` seconds), dance-single: taps in lane 1
//! (Down) at beats 1, 2 and 3.

use ddi_chart::{
    Chart, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, SourceInfo, Tick, TimingMap,
};
use ddi_engine::frame::SkinEventKind;
use ddi_engine::player::{PlayOptions, Player};
use ddi_engine::rules::{Judgement, presets};
use ddi_engine::{InputEvent, JudgeEventKind};
use ddi_platform::{ClockSample, HostTime};

fn song() -> Song {
    let notes = (1..=3)
        .map(|b| Note::new(Tick::from_beats(b), 1, NoteKind::Tap))
        .collect();
    Song {
        title: "Any key".into(),
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
            name: "any key".into(),
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

#[test]
fn any_lane_is_judged_in_one_and_shown_where_pressed() {
    let song = song();
    let mut player = Player::new(&song, 0, presets::calibration(), PlayOptions::default());
    player.judge_all_in(Some(1));
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    player.start(0.0, 0.0);
    let press = |p: &mut Player, lane, pressed, t| {
        p.input(InputEvent {
            lane,
            pressed,
            host_time: HostTime(t),
        })
    };
    let mut events = Vec::new();
    // Left at beat 1, Right at beat 2.
    player.update(HostTime(0.9));
    events.extend(press(&mut player, 0, true, 1.0));
    let frame = player.frame(HostTime(1.02));
    assert!(frame.receptors[0].pressed && !frame.receptors[1].pressed);
    assert!(frame.receptors[0].flash.is_some() && frame.receptors[1].flash.is_none());
    events.extend(press(&mut player, 0, false, 1.05));
    events.extend(press(&mut player, 3, true, 2.0));
    events.extend(press(&mut player, 3, false, 2.05));
    // Up held over beat 3, Left pressed for it and let go first: the
    // judged lane stays down while Up is.
    events.extend(press(&mut player, 2, true, 2.5));
    events.extend(press(&mut player, 0, true, 3.0));
    events.extend(press(&mut player, 0, false, 3.05));
    let frame = player.frame(HostTime(3.06));
    assert!(frame.receptors[2].pressed && !frame.receptors[0].pressed);
    events.extend(press(&mut player, 2, false, 3.5));
    player.update(HostTime(5.0));
    let taps: Vec<_> = events
        .iter()
        .filter_map(|e| match e.kind {
            JudgeEventKind::Tap(j, _) => Some((e.lane, j)),
            _ => None,
        })
        .collect();
    assert_eq!(taps, [(1, Judgement::W1); 3]);
    // The receptors' presses and the hits' flashes where the keys were.
    let shown: Vec<_> = player
        .skin_events_since(0)
        .filter_map(|e| match e.kind {
            SkinEventKind::Press => Some(('p', e.lane)),
            SkinEventKind::Tap { .. } => Some(('t', e.lane)),
            _ => None,
        })
        .collect();
    assert_eq!(
        shown,
        [
            ('p', 0),
            ('t', 0),
            ('p', 3),
            ('t', 3),
            ('p', 2),
            ('p', 0),
            ('t', 0)
        ]
    );
}
