//! Watching a stage, as the desktop window runs it (`watch-v0`,
//! `docs/plans/multiplayer-prototype-progress.md`, "Watching a stage"),
//! on this machine's GPU offscreen and a fake clock with no sound device.
//!
//! - A practice stage watched by someone who chose Watch: its prompt
//!   (Watching, nothing to ready), the grid of everyone's fields, one
//!   competitor followed (Tab, a press on a field), back to the grid (a
//!   press on the large field, Escape once), the stage's results with
//!   its winner and no play of the viewer's own, back to the room.
//! - A player whose life runs out watches the stage on from their own
//!   dimmed field and skips to the results (Enter); a player who leaves
//!   the play (Escape twice) forfeits, watches on and skips (a press on
//!   the plate); each ranked as before.
//!
//! Skipped where there is no GPU adapter (CI). `DDI_ROOM_SHOTS=<dir>`
//! keeps pictures (PPM).

mod common;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::history::MemoryPlayStore;
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_room::{Finish, Phase};
use ddi_ui::event::{KeyEvent, UiEvent};
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::history::Plays;
use ddi_ui::state::room::SharedRoom;

const SIZE: [u32; 2] = [1280, 720];
/// Beatucada's Medium chart: long enough for attacks.
const CHART: usize = 4;

fn top(ui: &ddi_ui::Ui) -> String {
    ui.layers()
        .last()
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

fn key(ui: &mut ddi_ui::Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, k)));
}

fn ppm(name: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("DDI_ROOM_SHOTS") else {
        return;
    };
    let mut out = format!("P6\n{} {}\n255\n", SIZE[0], SIZE[1]).into_bytes();
    out.extend(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]]),
    );
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out).unwrap();
}

/// The window's parts a test drives: the UI over the room's song select,
/// the play host on a fake clock, the picture it draws into.
struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    _ui_gpu: Gpu,
    ui: ddi_ui::Ui,
    env: ddi_ui::screens::Env,
    host: Host,
    clock: Arc<AtomicU64>,
    texture: wgpu::Texture,
    formats: Formats,
    room: SharedRoom,
    entry: SongEntry,
    store: Rc<MemoryPlayStore>,
}

impl Rig {
    fn new() -> Option<Rig> {
        common::indexes();
        let instance = gpu::instance();
        let Some(adapter) = gpu::adapter(&instance, None) else {
            eprintln!("no GPU adapter: skipped");
            return None;
        };
        let (device, queue) = gpu::device(&adapter).unwrap();
        let (ui_device, ui_queue) = gpu::device(&adapter).unwrap();
        let ui_gpu = Gpu::new(ui_device, ui_queue, wgpu::TextureFormat::Rgba8Unorm);
        let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
        let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
        env.reduced_motion.set(true);
        ui.handle(UiEvent::Resize {
            size: [SIZE[0] as f32, SIZE[1] as f32],
            ratio: 1.0,
        });
        let store = Rc::new(MemoryPlayStore::default());
        let plays = Plays::new(store.clone());
        let mut host = Host::new(Rc::default(), Some(plays));
        let clock = Arc::new(AtomicU64::new(1_000_000_000));
        let c = clock.clone();
        host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
        let formats = Formats::of(wgpu::TextureFormat::Rgba8Unorm, true);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: SIZE[0],
                height: SIZE[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let room = ddi_desktop::room::current();
        room.borrow_mut().set_rules_seen(true);
        ui.push(ddi_desktop::room::room_screen_with(
            &env,
            None,
            None,
            Some(host.requested.clone()),
        ));
        ui.update(0.5);
        let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
        let entry = SongEntry {
            id: "beatucada".into(),
            title: "Beatucada".into(),
            charts: vec![ChartEntry {
                style: "dance-single".into(),
                difficulty: Difficulty::Medium,
                index: CHART,
                digest: Some(song.chart_digest(CHART)),
                ..ChartEntry::default()
            }],
            ..SongEntry::default()
        };
        {
            let mut r = room.borrow_mut();
            let rules = ddi_room::MatchRules {
                stages: 3,
                ..r.rules().clone()
            };
            r.save_rules(rules).unwrap();
        }
        Some(Rig {
            device,
            queue,
            _ui_gpu: ui_gpu,
            ui,
            env,
            host,
            clock,
            texture,
            formats,
            room,
            entry,
            store,
        })
    }

    fn poll(&mut self) {
        let dev = Device {
            device: &self.device,
            queue: &self.queue,
            formats: self.formats,
        };
        self.host.poll(&mut self.ui, &self.env, Some(dev), 48_000);
    }

    /// The pick, its prompt readied (`auto`: the player's play is
    /// autoplayed).
    fn pick(&mut self, auto: bool) -> ddi_room::StageId {
        self.room
            .borrow_mut()
            .pick(&self.entry, &self.entry.charts[0])
            .unwrap();
        self.ui.update(0.1);
        let mut request = self
            .host
            .requested
            .borrow_mut()
            .take()
            .expect("the stage asked for");
        let stage = request.stage.as_ref().expect("a stage").stage;
        request.auto = auto;
        *self.host.requested.borrow_mut() = Some(request);
        let mut waited = 0;
        loop {
            self.poll();
            self.ui.update(0.05);
            if self.host.prompt_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 1;
            assert!(waited < 1000, "the song was never read");
        }
        self.ui.update(0.5);
        assert_eq!(top(&self.ui), "play-prompt");
        stage
    }

    /// The room's countdown passed: the stage starts, the prompt pressing
    /// Start for the host.
    fn start(&mut self) {
        {
            let mut r = self.room.borrow_mut();
            let later = r.now_ms() + 3_000;
            r.tick(later);
        }
        self.ui.update(0.05);
        self.poll();
        assert!(self.host.playing(), "{:?}", self.ui.layers());
    }

    /// One frame of the play, a quarter of a second on; the host second
    /// it was drawn at.
    fn frame(&mut self) -> f64 {
        let now = self.clock.fetch_add(1_000_000_000 / 4, Ordering::SeqCst) + 1_000_000_000 / 4;
        let srgb = self.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });
        let plain = self.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let dev = Device {
            device: &self.device,
            queue: &self.queue,
            formats: self.formats,
        };
        self.host.draw_play(
            &dev,
            &mut encoder,
            &srgb,
            Some(&plain),
            SIZE,
            HostTime(now as f64 / 1e9),
            &mut self.ui,
            &self.env,
        );
        self.queue.submit(Some(encoder.finish()));
        now as f64 / 1e9
    }

    fn shot(&mut self, name: &str) {
        // The fields' fonts are read on a thread: real time for them.
        for _ in 0..12 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            self.frame();
        }
        ppm(name, &gpu::read_rgba(&self.device, &self.queue, &self.texture));
    }

    /// The frame drawn last, as it is (a moment that would pass in
    /// [`Rig::shot`]'s frames: an attack's line lasts 1.5 s).
    fn shot_now(&mut self, name: &str) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        ppm(name, &gpu::read_rgba(&self.device, &self.queue, &self.texture));
    }

    fn edge(&mut self, device: DeviceId, control: &str, pressed: bool) {
        let at = self.clock.load(Ordering::SeqCst) as f64 / 1e9 - 0.1;
        self.host.feed.borrow_mut().edges.push(RawInput {
            device,
            control: control.into(),
            pressed,
            host_time: HostTime(at),
            slot: 0,
        });
    }

    fn press(&mut self, code: &str) {
        self.edge(DeviceId::Keyboard, code, true);
        self.edge(DeviceId::Keyboard, code, false);
    }

    /// The play's frames to its end; the results' screen is up.
    fn until_results(&mut self) {
        let mut frames = 0;
        while self.host.playing() {
            self.frame();
            if frames % 16 == 0 {
                let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            }
            frames += 1;
            assert!(frames < 4 * 400, "the play never ended");
        }
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.ui.update(0.05);
        assert_eq!(top(&self.ui), "results");
        self.ui.update(3.0);
    }

    /// The results left (Enter: Back to room has the focus): the room's
    /// song select again.
    fn back_to_room(&mut self) {
        key(&mut self.ui, "Enter", "Enter");
        self.ui.update(0.5);
        self.poll();
        self.ui.update(1.0);
        assert!(!self.host.prompting() && !self.host.playing());
        assert_eq!(top(&self.ui), "select", "{:?}", self.ui.layers());
    }
}

#[test]
fn a_practice_stage_is_watched_from_the_grid_and_a_seat_to_its_results() {
    let Some(mut rig) = Rig::new() else {
        return;
    };
    // Watch only, chosen on the room's song select (More's item).
    ddi_ui::screens::room::toggle_intent(&rig.room);
    rig.ui.update(0.1);
    assert!(rig.room.borrow().watching());
    let view = rig.env.state.borrow().room.clone().unwrap();
    assert!(view.watching, "{view:?}");
    let stage = rig.pick(false);
    // Nobody is waited for: the start is committed with the pick; the
    // prompt says Watching and offers no Ready.
    assert!(matches!(
        rig.room.borrow().state().phase,
        Phase::Countdown { .. }
    ));
    assert!(!rig.room.borrow().in_stage(stage));
    assert!(!rig.room.borrow().ready());
    rig.start();
    assert!(!rig.room.borrow().ready(), "nothing was readied");

    // Every field over the whole screen.
    for _ in 0..8 {
        rig.frame();
    }
    let w = rig.host.watch().expect("a watched stage");
    assert_eq!((w.view, w.focus), ("grid", None));
    assert_eq!(w.fields.len(), 3);
    assert_eq!(w.lane, 63.0, "three in a row at 1280×720");
    assert_eq!(
        w.label.as_deref(),
        Some("Tab or tap a field to follow one player")
    );
    let fields = rig.host.opponent_fields().expect("the grid");
    assert_eq!(fields.slots.len(), 3);
    let p = rig.host.running().expect("playing");
    assert_eq!(p.session.opponents().len(), 3);
    assert_eq!(p.session.player.judge().last_time(), None, "nothing judged");
    assert!(p.session.battle().unwrap().my_power(&p.session.player).is_none());
    rig.shot("watch-grid");

    // Tab: the first competitor followed, large in the play's field's
    // place, the others beside it.
    rig.press("Tab");
    rig.frame();
    rig.frame();
    let w = rig.host.watch().unwrap();
    assert_eq!((w.view, w.focus), ("focus", Some(0)));
    assert_eq!(w.fields.len(), 3, "two beside, the large one last");
    assert_eq!(w.fields[2].0, 0);
    let seats: Vec<usize> = w.fields.iter().map(|f| f.0).collect();
    assert_eq!(seats, [1, 2, 0]);
    let names: Vec<String> = rig
        .host
        .running()
        .unwrap()
        .session
        .opponents()
        .iter()
        .map(|o| o.name.clone())
        .collect();
    assert_eq!(w.label, Some(format!("Watching {}", names[0])));
    // The large field is the play's own field's band.
    let large = w.fields[2].1;
    assert!(large[0] > 400.0 && large[2] < 880.0 && large[1] == 0.0 && large[3] == 720.0);
    rig.shot("watch-focus");

    // A press on a field beside it follows that competitor.
    let at = rig.host.feed.borrow().watch_at.clone();
    assert_eq!(at.len(), 3);
    assert_eq!(at.iter().filter(|(s, _)| s.is_none()).count(), 1);
    let (seat, _) = at.iter().find(|(s, _)| *s == Some(2)).copied().unwrap();
    assert_eq!(seat, Some(2));
    rig.edge(DeviceId::Touch, "watch:2", true);
    rig.frame();
    rig.frame();
    assert_eq!(rig.host.watch().unwrap().focus, Some(2));
    // A press on the large field: every field again.
    rig.edge(DeviceId::Touch, ddi_session::play::TOUCH_WATCH_ALL, true);
    rig.frame();
    rig.frame();
    assert_eq!(rig.host.watch().unwrap().view, "grid");
    // Shift+Tab from the grid: the last; Escape once: the grid, the
    // stage going on.
    rig.edge(DeviceId::Keyboard, "ShiftLeft", true);
    rig.press("Tab");
    rig.edge(DeviceId::Keyboard, "ShiftLeft", false);
    rig.frame();
    rig.frame();
    assert_eq!(rig.host.watch().unwrap().focus, Some(2));
    rig.press("Escape");
    for _ in 0..12 {
        rig.frame();
    }
    assert!(rig.host.playing(), "Escape once does not leave");
    assert_eq!(rig.host.watch().unwrap().view, "grid");

    // To its end: who attacks whom is drawn between the fields, the
    // warning beside the recipient's.
    // Then (the maintainer, 2026-11-13) a followed competitor's own
    // attacks, to and from the fields beside theirs: the recipient of the
    // grid's first line followed at once (Tab to their seat), its line
    // and outline drawn from the field beside, then back to the grid.
    let (mut lines, mut warned, mut attacks) = (0, 0, 0);
    let mut frames = 0;
    let mut shot = false;
    let (mut following, mut focus_shot, mut focus_lines) = (false, false, 0);
    let mut followed_for = 0;
    while rig.host.playing() {
        rig.frame();
        let links = rig.host.attack_links();
        if let Some(p) = rig.host.running()
            && let Some(a) = p.session.battle().and_then(|b| b.attacks())
        {
            attacks = a.ledger().attacks().len();
        }
        let w = rig.host.watch();
        if w.as_ref().is_some_and(|w| w.view == "grid") {
            for l in &links {
                assert!(l.from.is_some(), "between two competitors");
                assert_ne!(l.from, Some(l.seat));
            }
            lines += links.iter().filter(|l| l.line > 0.0).count();
        } else if following {
            // The followed competitor's: theirs to or from a field beside.
            assert!(links.iter().all(|l| l.from.is_none()));
            focus_lines += links.len();
            followed_for += 1;
            if !focus_shot && links.iter().any(|l| l.line > 0.0) {
                focus_shot = true;
                rig.shot_now("watch-focus-attack");
            }
            if focus_shot || followed_for > 240 {
                rig.press("Escape");
                following = false;
            }
        }
        let now_warned = w.as_ref().map_or(0, |w| w.warned.len());
        warned += now_warned;
        if !shot && now_warned > 0 && links.iter().any(|l| l.line > 0.0) {
            shot = true;
            rig.shot_now("watch-grid-attack");
            let to = links.iter().find(|l| l.line > 0.0).unwrap().seat;
            for _ in 0..=to {
                rig.press("Tab");
                rig.frame();
            }
            following = true;
        }
        if frames % 16 == 0 {
            let _ = rig.device.poll(wgpu::PollType::wait_indefinitely());
        }
        frames += 1;
        assert!(frames < 4 * 400, "the stage never ended");
    }
    assert!(attacks > 0, "the CPUs attacked each other");
    assert!(lines > 0 && warned > 0, "{lines} lines, {warned} warnings");
    assert!(focus_lines > 0, "a followed competitor's attacks drawn");
    let _ = rig.device.poll(wgpu::PollType::wait_indefinitely());
    rig.ui.update(0.05);
    assert_eq!(top(&rig.ui), "results");
    rig.ui.update(3.0);

    // Its results: the standings of the competitors alone, the winner
    // where a play's own judgments would be, no Graphs tab.
    let view = rig.env.state.borrow().stage.clone().expect("the standings");
    assert!(view.watched && view.confirmed, "{view:?}");
    assert_eq!(view.rows.len(), 3);
    assert!(view.rows.iter().all(|r| !r.me && r.place.is_some()));
    assert!(!view.stage_winners.is_empty());
    assert!(names.contains(&view.stage_winners[0]));
    assert!(rig.ui.actor("Watched.Note").is_some() && rig.ui.actor("Watched.Winner0").is_some());
    assert!(rig.ui.actor("ScoreBox").is_none(), "no grade of their own");
    assert!(rig.ui.bounds("Tab.Standings").is_some() && rig.ui.bounds("Tab.Graphs").is_none());
    assert!(rig.ui.bounds("Retry").is_none());
    let pixels = rig
        ._ui_gpu
        .draw_offscreen(&mut rig.ui, SIZE, wgpu::TextureFormat::Rgba8Unorm);
    ppm("watch-results", &pixels);
    rig.back_to_room();
    assert!(matches!(
        rig.room.borrow().state().phase,
        Phase::Intermission { .. }
    ));
    assert!(rig.room.borrow().mark_of(stage).is_none(), "no place of theirs");
    assert!(rig.store.is_empty(), "nothing kept in the history");

    // The next stage, left by its quit (Escape twice): nobody forfeits,
    // the CPUs are played out, its results follow.
    let stage = rig.pick(false);
    rig.start();
    for _ in 0..6 {
        rig.frame();
    }
    rig.edge(DeviceId::Keyboard, "Escape", true);
    rig.edge(DeviceId::Keyboard, "Escape", false);
    rig.frame();
    rig.edge(DeviceId::Keyboard, "Escape", true);
    rig.until_results();
    let view = rig.env.state.borrow().stage.clone().expect("the standings");
    assert!(view.watched && view.confirmed);
    assert_eq!(view.rows.len(), 3);
    let m = rig.room.borrow();
    let play = m.state().stage(stage).unwrap().play.clone().unwrap();
    let result = play.result.unwrap();
    assert!(
        result
            .placements
            .iter()
            .all(|p| p.finish != Finish::Forfeit)
    );
    drop(m);
    rig.back_to_room();
}

#[test]
fn a_player_out_of_a_stage_watches_on_and_skips_to_its_results() {
    let Some(mut rig) = Rig::new() else {
        return;
    };
    // Stage 1: nobody presses; the player's life runs out.
    let stage = rig.pick(false);
    key(&mut rig.ui, "Enter", "Enter");
    rig.ui.update(0.05);
    assert!(rig.room.borrow().ready());
    rig.start();
    let mut frames = 0;
    while rig.host.watch().is_none() {
        rig.frame();
        frames += 1;
        assert!(rig.host.playing(), "the play ended at once");
        assert!(frames < 4 * 200, "the player never went out");
    }
    let w = rig.host.watch().unwrap();
    assert_eq!((w.view, w.focus), ("out", None));
    assert_eq!(w.mark.as_deref(), Some("Out"));
    assert_eq!(w.skip.as_deref(), Some("Enter: skip to results"));
    // The stage goes on: the others beside the player's own field, the
    // song playing, for as long as they watch.
    for _ in 0..20 {
        rig.frame();
        assert!(rig.host.playing(), "watching on");
    }
    assert_eq!(rig.host.opponent_fields().unwrap().slots.len(), 3);
    let p = rig.host.running().unwrap();
    assert!(p.session.player.failed() && p.session.song_going());
    assert!(rig.host.feed.borrow().skip_at.is_some());
    rig.shot("watch-out");
    // Tab follows a competitor large, as a watcher's focus: their own
    // field then a miniature among the others (its seat the last, after
    // the three CPUs'), marked Out; the skip's plate stays.
    rig.press("Tab");
    rig.frame();
    rig.frame();
    let w = rig.host.watch().unwrap();
    assert_eq!((w.view, w.focus), ("out", Some(0)));
    let seats: Vec<usize> = w.fields.iter().map(|(s, _)| *s).collect();
    assert_eq!(seats.len(), 4, "{seats:?}");
    assert!(seats.contains(&3) && seats.last() == Some(&0), "{seats:?}");
    assert!(rig.host.feed.borrow().skip_at.is_some());
    assert_eq!(w.label.as_deref().map(|l| l.starts_with("Watching")), Some(true));
    rig.shot("watch-out-follow");
    // Escape once: back to their own dimmed field.
    rig.press("Escape");
    rig.frame();
    rig.frame();
    assert_eq!(rig.host.watch().unwrap().focus, None);
    assert!(rig.host.playing(), "once is not the quit");
    // Enter: the results at once.
    rig.press("Enter");
    rig.until_results();
    let view = rig.env.state.borrow().stage.clone().expect("the standings");
    assert!(!view.watched && view.confirmed, "{view:?}");
    assert_eq!(view.rows.len(), 4);
    let me = view.rows.iter().find(|r| r.me).expect("their line");
    assert_eq!(me.place, Some(4));
    assert!(rig.ui.bounds("Tab.Graphs").is_some(), "their own play's graphs");
    let mark = rig.room.borrow().mark_of(stage).unwrap();
    assert_eq!(mark.placement.finish, Finish::Eliminated);
    rig.back_to_room();

    // Stage 2: autoplayed, left by Escape twice: a forfeit, watched on
    // from the player's own field, skipped by a press on its plate.
    let stage = rig.pick(true);
    key(&mut rig.ui, "Enter", "Enter");
    rig.ui.update(0.05);
    rig.start();
    for _ in 0..12 {
        rig.frame();
    }
    assert!(rig.host.watch().is_none(), "playing");
    rig.edge(DeviceId::Keyboard, "Escape", true);
    rig.edge(DeviceId::Keyboard, "Escape", false);
    rig.frame();
    rig.edge(DeviceId::Keyboard, "Escape", true);
    rig.edge(DeviceId::Keyboard, "Escape", false);
    rig.frame();
    rig.frame();
    assert!(rig.host.playing(), "the quit does not end the stage");
    let w = rig.host.watch().expect("watching on");
    assert_eq!(w.view, "left");
    assert_eq!(w.mark.as_deref(), Some("Left the stage"));
    let p = rig.host.running().unwrap();
    assert!(p.session.battle().unwrap().forfeited());
    for _ in 0..12 {
        rig.frame();
        assert!(rig.host.playing());
    }
    rig.shot("watch-left");
    // A press on the plate (the shell sends `skip` for one inside it).
    let [l, t, r, b] = rig.host.feed.borrow().skip_at.expect("the skip's plate");
    assert!(l < r && t < b && l > 400.0 && r < 880.0);
    rig.edge(DeviceId::Touch, ddi_session::play::TOUCH_SKIP_CONTROL, true);
    rig.until_results();
    let view = rig.env.state.borrow().stage.clone().expect("the standings");
    assert!(!view.watched && view.confirmed, "{view:?}");
    assert_eq!(view.rows.len(), 4);
    let mark = rig.room.borrow().mark_of(stage).unwrap();
    assert_eq!(mark.placement.finish, Finish::Forfeit);
    assert_eq!(mark.placement.points, ddi_room::Points::ZERO);
    rig.back_to_room();
    assert!(rig.store.is_empty(), "a stage is not kept in the history");
}
