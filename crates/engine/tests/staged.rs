//! A staged start (`Player::start_staged`, the announcer studio's scenes,
//! `docs/plans/announcer-studio.md`, step 4): a play begun part-way with
//! the presses before its start judged at once is where a play of the
//! same presses from the song's start is at that second (its counts, its
//! combo, its life and score), and judges what follows as that play does;
//! a combo given past the one the presses made goes on as any combo does.

mod common;

use common::{chart, script};
use ddi_engine::autoplay;
use ddi_engine::player::{PlayOptions, Player};
use ddi_engine::rules::{Judgement, presets};
use ddi_platform::{ClockSample, HostTime};

/// A play of `edges` from the song's start, judged up to `until` (every
/// edge before it fed, then updated to it).
fn played_to(player: &mut Player, edges: &[(f64, u8, bool)], until: f64) {
    for &(t, lane, pressed) in edges.iter().filter(|e| e.0 < until) {
        player.update_at(t);
        player.input_at(lane, pressed, t);
    }
    player.update_at(until);
}

/// The rest of `edges`, from `from`, then to the end.
fn played_on(player: &mut Player, edges: &[(f64, u8, bool)], from: f64) {
    for &(t, lane, pressed) in edges.iter().filter(|e| e.0 >= from) {
        player.update_at(t);
        player.input_at(lane, pressed, t);
    }
    player.update_at(player.end_time() + 5.0);
}

#[test]
fn a_staged_start_is_where_the_play_from_the_start_is() {
    for (seed, ruleset) in [
        (3, presets::itg()),
        (5, presets::sm5()),
        (8, presets::ddr_a()),
        (13, presets::itg()),
    ] {
        let song = chart(seed, "dance-single");
        let probe = Player::new(&song, 0, ruleset.fresh(), PlayOptions::default());
        let edges = script(&probe, seed);
        let end = probe.song_end_time();
        for at in [end * 0.25, end * 0.6] {
            let mut whole = Player::new(&song, 0, ruleset.fresh(), PlayOptions::default());
            whole.start(0.0, 0.0);
            played_to(&mut whole, &edges, at);
            let mut staged = Player::new(&song, 0, ruleset.fresh(), PlayOptions::default());
            staged.start_staged(10.0, at, &edges, 0);
            assert!(staged.started());
            let id = &ruleset.id;
            assert_eq!(staged.tally(), whole.tally(), "{id} seed {seed} at {at}");
            assert_eq!(staged.combo(), whole.combo(), "{id} seed {seed}");
            assert_eq!(staged.max_combo(), whole.max_combo());
            assert_eq!(staged.failed(), whole.failed());
            assert_eq!(staged.ruleset().gauge.life(), whole.ruleset().gauge.life());
            // What follows is judged alike.
            played_on(&mut whole, &edges, at);
            played_on(&mut staged, &edges, at);
            let (a, b) = (whole.results(), staged.results());
            assert_eq!(a.tally, b.tally, "{id} seed {seed} at {at}");
            assert_eq!(a.score, b.score);
            assert_eq!(a.grade, b.grade);
            assert_eq!(a.max_combo, b.max_combo);
            assert_eq!(a.failed, b.failed);
        }
    }
}

/// Its clock runs from the start given: song second `at` at the context
/// time given.
#[test]
fn a_staged_play_hears_from_its_start() {
    let song = chart(3, "dance-single");
    let mut p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    p.clock_sample(ClockSample {
        context_time: 10.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    p.start_staged(10.0, 12.0, &[], 0);
    let at = |host: f64| p.clock().song_time_at_host(HostTime(host));
    assert!((at(0.0) - 12.0).abs() < 1e-9, "{}", at(0.0));
    assert!((at(1.5) - 13.5).abs() < 1e-9, "{}", at(1.5));
}

/// A combo given past the one the presses made is the play's; a step goes
/// on from it, a miss ends it; one below it leaves it as it is.
#[test]
fn a_given_combo_goes_on_from_the_staged_one() {
    let song = chart(5, "dance-single");
    let probe = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    let presses = autoplay::script(probe.judge().notes(), 0.0);
    let mut edges = autoplay::edges(presses.iter().copied());
    autoplay::sort_edges(&mut edges);
    let at = probe.song_end_time() * 0.5;
    let mut plain = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    plain.start_staged(0.0, at, &edges, 0);
    let made = plain.combo();
    assert!(made > 20, "{made}");
    let mut low = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    low.start_staged(0.0, at, &edges, made / 2);
    assert_eq!(low.combo(), made);
    let mut p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    p.start_staged(0.0, at, &edges, made + 900);
    assert_eq!(p.combo(), made + 900);
    assert_eq!(p.max_combo(), made + 900);
    // The next tap pressed goes on from it.
    let next = presses.iter().find(|q| q.press >= at + 0.1).unwrap();
    p.update_at(next.press);
    let events = p.input_at(next.lane, true, next.press);
    assert!(
        events.iter().any(
            |e| matches!(e.kind, ddi_engine::judge::JudgeEventKind::Tap(j, _) if j == Judgement::W1)
        ),
        "{events:?}"
    );
    p.input_at(next.lane, false, next.release);
    assert!(p.combo() > made + 900);
    // Left alone, the notes are missed: the combo ends.
    p.update_at(p.song_end_time());
    assert_eq!(p.combo(), 0);
    assert!(p.max_combo() > made + 900);
}
