//! Replays on desktop with no device (`Audio::headless` on a fake clock;
//! `docs/plans/play-history.md`, Replays): every chart of every bundled
//! song autoplayed and kept, then replayed from the history at other
//! frame rates to the same results; a play pressed on the keyboard with
//! misses, early and late presses, holds let go and stray presses, the
//! same; keys pressed during a replay reach no lane; a replay is never
//! kept; and the plays that cannot be replayed are refused, saying why.

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::play::{Ended, Feed, Play, Request, load, replays};
use ddi_engine::player::Results;
use ddi_platform::history::{MemoryPlayStore, PlayEntry, PlayStore};
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::state::history::Plays;

/// A clock the play reads (ns), and a play's run on it.
struct Run {
    clock: Arc<AtomicU64>,
    feed: Rc<RefCell<Feed>>,
}

impl Run {
    fn new() -> Run {
        Run {
            clock: Arc::new(AtomicU64::new(1_000_000_000)),
            feed: Rc::new(RefCell::new(Feed::default())),
        }
    }

    fn audio(&self) -> Audio {
        let c = self.clock.clone();
        Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))))
    }

    fn now(&self) -> f64 {
        self.clock.load(Ordering::SeqCst) as f64 / 1e9
    }

    /// Plays `p` at `fps` (uneven by `jitter` seconds) to its end; `keys`
    /// (host seconds, key, pressed), sorted, handed over as their time
    /// comes.
    fn run(
        &self,
        p: &mut Play,
        fps: f64,
        jitter: f64,
        mut keys: Vec<(f64, String, bool)>,
        plays: Option<&Rc<Plays>>,
    ) -> Option<Results> {
        keys.reverse();
        let mut k = 0u64;
        loop {
            k += 1;
            let wobble = jitter * (((k * 7919) % 13) as f64 / 6.0 - 1.0);
            let step = ((1.0 / fps + wobble).max(0.0005) * 1e9) as u64;
            let now = (self.clock.fetch_add(step, Ordering::SeqCst) + step) as f64 / 1e9;
            while keys.last().is_some_and(|(t, _, _)| *t <= now) {
                let (t, key, pressed) = keys.pop().unwrap();
                self.feed.borrow_mut().edges.push(RawInput {
                    device: DeviceId::Keyboard,
                    control: key,
                    pressed,
                    host_time: HostTime(t),
                    slot: 0,
                });
            }
            match p.tick(HostTime(now), plays) {
                Some(Ended::Finished { results, .. }) => return Some(*results),
                Some(_) => return None,
                None => {}
            }
            assert!(now < 2000.0, "never ends");
        }
    }
}

/// Play `id` as `store` keeps it.
fn stored(store: &MemoryPlayStore, id: &str) -> PlayEntry {
    let got: Rc<RefCell<Option<PlayEntry>>> = Rc::default();
    let g = got.clone();
    store.records(
        vec![id.to_string()],
        Box::new(move |r| *g.borrow_mut() = r.ok().and_then(|mut v| v.pop())),
    );
    got.borrow_mut().take().expect("kept")
}

fn without_late(r: &Results) -> Results {
    Results {
        late_edges: 0,
        ..r.clone()
    }
}

/// Watches kept play `id` at `fps`: its results, and the replay's.
fn watch(
    plays: &Rc<Plays>,
    id: &str,
    fps: f64,
    jitter: f64,
    keys: Vec<(f64, String, bool)>,
) -> Results {
    let request = replays::request_with(plays, id, false, &|_| None);
    assert!(request.refused.is_none(), "{:?}", request.refused);
    let replay = request.replay.clone().unwrap();
    let loaded = Rc::new(load::load(&request.song, 48_000).unwrap());
    let run = Run::new();
    // Another clock: another device's.
    run.clock.fetch_add(777_000_000, Ordering::SeqCst);
    let start = run.now();
    let keys = keys
        .into_iter()
        .map(|(t, k, p)| (start + t, k, p))
        .collect();
    let settings = request.settings();
    let mut p = Play::start(
        request,
        loaded.clone(),
        &settings,
        run.audio(),
        run.feed.clone(),
        None,
        fps,
    )
    .unwrap();
    let before = plays.history().len();
    let watched = run
        .run(&mut p, fps, jitter, keys, Some(plays))
        .expect("the replay ended");
    assert_eq!(plays.history().len(), before, "a replay is never kept");
    let offline = replay.results(&loaded.song).unwrap();
    let check = replay.check(&watched, &offline);
    assert!(check.matches && check.with_record, "{id}: {check:?}");
    watched
}

/// Plays chart `chart` of `song` (autoplayed when `keys` is none) at
/// `fps`, kept in `plays`: its results and id.
fn play(
    plays: &Rc<Plays>,
    loaded: &Rc<load::Loaded>,
    song: &str,
    chart: usize,
    fps: f64,
    keys: Option<Vec<(f64, String, bool)>>,
) -> (Results, String) {
    let run = Run::new();
    let start = run.now();
    let auto = keys.is_none();
    let mut p = Play::start(
        Request::song(song, chart, auto),
        loaded.clone(),
        &ddi_ui::state::settings::Settings::default(),
        run.audio(),
        run.feed.clone(),
        None,
        fps,
    )
    .unwrap();
    let keys = keys
        .unwrap_or_default()
        .into_iter()
        .map(|(t, k, pr)| (start + t, k, pr))
        .collect();
    let r = run
        .run(&mut p, fps, 0.0, keys, Some(plays))
        .expect("the play ended");
    (r, p.play_id.clone().expect("kept"))
}

#[test]
fn every_bundled_autoplay_replays_to_its_results() {
    common::indexes();
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut charts = 0;
    for entry in ddi_desktop::library::bundled() {
        let loaded = Rc::new(load::load(&entry.id, 48_000).unwrap());
        for chart in 0..loaded.song.charts.len() {
            if loaded.song.layout_of(&loaded.song.charts[chart]).is_none() {
                continue;
            }
            let (results, id) = play(&plays, &loaded, &entry.id, chart, 60.0, None);
            let watched = watch(&plays, &id, 144.0, 0.0, Vec::new());
            assert_eq!(
                without_late(&watched),
                without_late(&results),
                "{} {chart}",
                entry.id
            );
            charts += 1;
        }
    }
    assert!(charts >= 8, "{charts}");
    assert_eq!(store.len(), charts);
}

#[test]
fn a_keyboard_play_with_misses_replays_to_its_results() {
    common::indexes();
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let loaded = Rc::new(load::load("beatucada", 48_000).unwrap());
    let chart = 1;
    let song = &loaded.song;
    let layout = song.layout_of(&song.charts[chart]).unwrap();
    let rules = ddi_ui::state::settings::Settings::default().ruleset_for(&song.charts[chart]);
    let player = ddi_engine::player::Player::new(song, chart, rules, Default::default());
    let script = ddi_engine::autoplay::script(player.judge().notes(), 0.0);
    // Song second 0 is heard 0.1 s (the start's lookahead) after the
    // start, give or take the headless output's latency.
    let lead = ddi_session::play::START_LOOKAHEAD;
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 10_000) as f64 / 10_000.0
    };
    let mut keys = Vec::new();
    for (n, p) in script.iter().enumerate() {
        if n % 15 == 7 {
            continue;
        }
        let key = layout.lanes[p.lane as usize].default_keys[0].clone();
        let off = (rand() - 0.5) * 0.09;
        let release = if n % 4 == 0 && p.release - p.press > 0.3 {
            p.press + (p.release - p.press) * 0.4
        } else {
            p.release + off
        };
        keys.push((lead + p.press + off, key.clone(), true));
        keys.push((lead + release.max(p.press + off + 0.03), key, false));
        if n % 9 == 0 {
            let other = layout.lanes[(p.lane as usize + 1) % 4].default_keys[0].clone();
            keys.push((lead + p.press + 0.23, other.clone(), true));
            keys.push((lead + p.press + 0.27, other, false));
        }
    }
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (results, id) = play(&plays, &loaded, "beatucada", chart, 60.0, Some(keys));
    let t = &results.tally;
    assert!(
        t.taps[5] > 0 && t.taps[1..5].iter().any(|&n| n > 0),
        "{t:?}"
    );
    assert!(!results.failed, "{t:?}");
    // Watched at an uneven 50 frames a second, with keys pressed all along
    // (they reach no lane).
    let noise: Vec<(f64, String, bool)> = (0..60)
        .flat_map(|i| {
            let key = layout.lanes[i % 4].default_keys[0].clone();
            let t = 1.0 + i as f64 * 0.41;
            [(t, key.clone(), true), (t + 0.08, key, false)]
        })
        .collect();
    let watched = watch(&plays, &id, 50.0, 0.007, noise);
    assert_eq!(without_late(&watched), without_late(&results));
    // A web play's record replays here too: the same entry read back
    // from the export's form.
    let line = stored(&store, &id).to_json();
    let again = PlayEntry::from_json(&line).unwrap();
    let other = Rc::new(MemoryPlayStore::default());
    other.put(again);
    let others = Plays::new(other);
    let watched = watch(&others, &id, 30.0, 0.0, Vec::new());
    assert_eq!(without_late(&watched), without_late(&results));
}

#[test]
fn plays_that_cannot_be_replayed_are_refused() {
    common::indexes();
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let loaded = Rc::new(load::load("beatucada", 48_000).unwrap());
    let (_, id) = play(&plays, &loaded, "beatucada", 2, 60.0, None);
    // Not kept.
    let r = replays::request_with(&plays, "000000000000001-deadbeef", false, &|_| None);
    assert!(
        r.refused.as_deref().unwrap().contains("no longer kept"),
        "{r:?}"
    );
    // Kept, as it is: watched.
    let r = replays::request_with(&plays, &id, false, &|_| None);
    assert!(r.refused.is_none() && r.replay.is_some());
    assert_eq!((r.song.as_str(), r.chart), ("beatucada", 2));
    let entry = stored(&store, &id);
    let rewrite = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut e = entry.clone();
        let mut v: serde_json::Value = serde_json::from_str(&e.record).unwrap();
        f(&mut v);
        e.record = v.to_string();
        e.id = format!("{}-x", &id[..15]);
        let s = Rc::new(MemoryPlayStore::default());
        s.put(e.clone());
        (Plays::new(s), e.id)
    };
    // Its chart changed (another digest): refused, not played wrong.
    let (p, i) = rewrite(&|v| v["chart"]["digest"] = "0123456789abcdef0123456789abcdef".into());
    let r = replays::request_with(&p, &i, false, &|_| None);
    assert!(
        r.refused
            .as_deref()
            .unwrap()
            .contains("changed since the play"),
        "{r:?}"
    );
    // Its song gone, its chart in another song: found there.
    let (p, i) = rewrite(&|v| v["chart"]["song"] = "a-song-removed".into());
    let r = replays::request_with(&p, &i, false, &|_| Some("beatucada".into()));
    assert!(r.refused.is_none(), "{r:?}");
    assert_eq!(r.song, "beatucada");
    // Gone everywhere.
    let r = replays::request_with(&p, &i, false, &|_| None);
    assert!(
        r.refused
            .as_deref()
            .unwrap()
            .contains("no longer in the library"),
        "{r:?}"
    );
    // Drawn with a skin no longer installed.
    let (p, i) = rewrite(&|v| v["settings"] = serde_json::json!({ "noteskin": "dance/gone-skin" }));
    let r = replays::request_with(&p, &i, false, &|_| None);
    assert!(
        r.refused.as_deref().unwrap().contains("dance/gone-skin"),
        "{r:?}"
    );
    // Judged by a newer judge.
    let (p, i) = rewrite(&|v| v["judge_version"] = 99.into());
    let r = replays::request_with(&p, &i, false, &|_| None);
    assert!(
        r.refused.as_deref().unwrap().contains("newer version"),
        "{r:?}"
    );
}

#[test]
fn the_ghost_of_the_best_play_runs_beside_a_play() {
    common::indexes();
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let loaded = Rc::new(load::load("beatucada", 48_000).unwrap());
    let (best, id) = play(&plays, &loaded, "beatucada", 1, 60.0, None);
    let replay = replays::request_with(&plays, &id, false, &|_| None)
        .replay
        .unwrap();
    let ghost = ddi_ui::state::history::Ghost::of(&replay, &loaded.song).unwrap();
    // A play 40 ms late beside it: behind, never ahead.
    let run = Run::new();
    let mut request = Request::song("beatucada", 1, true);
    request.bias = 0.04;
    request.ghost = Some(Rc::new(ghost));
    let mut p = Play::start(
        request,
        loaded.clone(),
        &ddi_ui::state::settings::Settings::default(),
        run.audio(),
        run.feed.clone(),
        None,
        75.0,
    )
    .unwrap();
    let mut deltas = Vec::new();
    let late = loop {
        let now = run.clock.fetch_add(13_333_333, Ordering::SeqCst) + 13_333_333;
        let ended = p.tick(HostTime(now as f64 / 1e9), None);
        if let Some(d) = p.session.ghost_delta() {
            deltas.push(d);
        }
        if let Some(Ended::Finished { results, .. }) = ended {
            break *results;
        }
        assert!(now < 2_000_000_000_000, "never ends");
    };
    let value = |d: &ddi_ui::state::history::GhostDelta| match d {
        ddi_ui::state::history::GhostDelta::Percent(p) => *p,
        ddi_ui::state::history::GhostDelta::Points(p) => *p,
    };
    assert!(deltas.len() > 1000);
    assert!(deltas.iter().all(|d| value(d) <= 1e-9));
    let score = ddi_ui::state::history::score_value;
    let expected = (score(&late.score) - score(&best.score)) * 100.0;
    assert!(
        (value(deltas.last().unwrap()) - expected).abs() < 1e-9,
        "{:?} {expected}",
        deltas.last()
    );
    assert!(expected < -1.0, "{expected}");
}
