//! The pointer on a play's battle controls, through the window's own
//! mapping (`Shell::play_pointer`: the review's gap 6, round M), on a
//! fake clock with no sound device: a practice stage with attacks, the
//! window at a scale factor of 2.
//!
//! - A press on the attack policy's label (its place as the play drew
//!   it, logical pixels) cycles the policy once, kept in the play's log;
//!   its release is taken too and does nothing more; a press beside the
//!   label is the UI's.
//! - A watched stage: a press on a field follows that competitor, one on
//!   the large field goes back to every field; out of a stage, a press
//!   on the skip's plate ends the watch, also where a field lies under
//!   the plate.
//!
//! On this machine's GPU the places are the ones the play drew at that
//! scale (so their units are checked against the cursor's); without one
//! (CI) the stage runs undrawn (`common::rig::Draws`) and the places are
//! given by the test, so the mapping from a press to the play's control
//! is checked all the same.

mod common;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::shell::Shell;
use ddi_room::attacks::Targeting;
use ddi_ui::event::{PointerPhase, UiEvent};
use ddi_ui::screens::data::Difficulty;
use ddi_ui::state::room::SharedRoom;

/// The window: 1280×720 points at a scale factor of 2.
const LOGICAL: [f32; 2] = [1280.0, 720.0];
const SCALE: f32 = 2.0;
const PHYSICAL: [u32; 2] = [2560, 1440];
const STEP: u64 = 1_000_000_000 / 30;

struct Window {
    shell: Shell,
    env: ddi_ui::screens::Env,
    draws: common::rig::Draws,
    clock: Arc<AtomicU64>,
    room: SharedRoom,
}

impl Window {
    /// The shell over a fresh practice room with attacks, three CPUs.
    fn new() -> Window {
        common::indexes();
        ddi_desktop::audio::set_headless(true);
        let home = std::env::temp_dir().join(format!("ddi-play-pointer-{}", std::process::id()));
        // SAFETY: set before any thread of this test reads the environment
        // (both tests set the same value).
        unsafe { std::env::set_var("DDI_HOME", &home) };
        let mut shell = Shell::named_at("select", Some(0.0));
        let env = shell.theme.clone().expect("the theme").0;
        shell.ui.handle(UiEvent::Resize {
            size: LOGICAL,
            ratio: SCALE,
        });
        shell.ui.update(0.5);
        let clock = Arc::new(AtomicU64::new(1_000_000_000));
        let c = clock.clone();
        shell.play.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
        shell.play.scale = SCALE;
        let draws = common::rig::Draws::here(&mut shell.play, PHYSICAL);
        ddi_desktop::room::discard();
        let room = common::practice::open(&mut shell.ui, &env, &shell.play);
        {
            let mut r = room.borrow_mut();
            let mut rules = ddi_room::MatchRules {
                stages: 2,
                charts: ddi_room::Charts::AsPicked,
                ..r.rules().clone()
            };
            rules.set_attacks(true);
            rules.cpu.opponent.seed = 7;
            r.save_rules(rules).unwrap();
        }
        Window {
            shell,
            env,
            draws,
            clock,
            room,
        }
    }

    /// The pick's stage started (`auto`: the player's play autoplayed;
    /// a watched stage starts by itself).
    fn start(&mut self, auto: bool) {
        let entry = common::practice::song_entry("beatucada", "Beatucada", 4, Difficulty::Medium);
        self.room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
        self.shell.ui.update(0.1);
        let mut request = self.shell.play.requested.borrow_mut().take().expect("asked for");
        request.auto = auto;
        *self.shell.play.requested.borrow_mut() = Some(request);
        self.draws.wait_prompt(&mut self.shell.play, &mut self.shell.ui, &self.env);
        if self.room.borrow().watching() {
            self.draws
                .countdown(&mut self.shell.play, &mut self.shell.ui, &self.env, &self.room);
        } else {
            self.draws
                .ready_and_start(&mut self.shell.play, &mut self.shell.ui, &self.env, &self.room);
        }
        assert!(self.shell.play.playing(), "{:?}", self.shell.ui.layers());
    }

    fn frame(&mut self) -> f64 {
        let now = self.clock.fetch_add(STEP, Ordering::SeqCst) + STEP;
        self.shell.play.scale = SCALE;
        self.draws
            .frame(&mut self.shell.play, &mut self.shell.ui, &self.env, now);
        now as f64 / 1e9
    }

    fn frames(&mut self, n: usize) {
        for i in 0..n {
            self.frame();
            if i % 16 == 0 {
                self.draws.wait_gpu();
            }
        }
    }

    /// Whether a rect the play drew lies inside the window in its
    /// cursor's units (logical pixels).
    fn in_window([l, t, r, b]: [f32; 4]) -> bool {
        l >= 0.0 && t >= 0.0 && l < r && t < b && r <= LOGICAL[0] && b <= LOGICAL[1]
    }

    /// The attack policy the play has now.
    fn policy(&self) -> Targeting {
        let p = self.shell.play.running().expect("playing");
        p.session.battle().expect("a stage").policy(&p.session.player).expect("a policy")
    }

    /// The watch as the session has it.
    fn watching(&self) -> Option<(ddi_session::play::Watching, Option<usize>)> {
        let now = self.clock.load(Ordering::SeqCst) as f64 / 1e9;
        let w = self.shell.play.running()?.session.watch(now)?;
        Some((w.why, w.focus))
    }
}

fn center([l, t, r, b]: [f32; 4]) -> [f32; 2] {
    [(l + r) / 2.0, (t + b) / 2.0]
}

#[test]
fn a_press_on_the_policy_label_cycles_the_policy_once() {
    let mut w = Window::new();
    w.start(true);
    w.frames(60);
    // Where the label takes a press: as the play drew it at this scale
    // (logical pixels, the cursor's), or the test's where nothing is
    // drawn.
    if !w.draws.drawn() {
        w.shell.play.feed.borrow_mut().policy_at = Some([440.0, 30.0, 520.0, 52.0]);
    }
    let label = w.shell.play.feed.borrow().policy_at.expect("the label's place");
    assert!(Window::in_window(label), "{label:?} in the window's points");
    if w.draws.drawn() {
        // Under the life gauge's left end, inside the field (the field is
        // the window's middle): never the physical pixels' place.
        assert!(label[0] > 400.0 && label[2] < 640.0 && label[3] < 80.0, "{label:?}");
    }
    let before = w.policy();
    assert_eq!(before, Targeting::Spread);
    // A press beside it is not the play's.
    let beside = [label[2] + 40.0, label[3] + 200.0];
    assert!(!w.shell.play_pointer(PointerPhase::Down, beside));
    assert!(!w.shell.play_pointer(PointerPhase::Up, beside));
    w.frames(4);
    assert_eq!(w.policy(), before);
    // Down and up on the label: the next policy, once.
    assert!(w.shell.play_pointer(PointerPhase::Down, center(label)));
    assert!(w.shell.play.feed.borrow().policy_down);
    w.frames(2);
    // (Released off the label: the press begun on it is still its own.)
    assert!(w.shell.play_pointer(PointerPhase::Up, beside), "the release is the label's");
    assert!(!w.shell.play.feed.borrow().policy_down);
    w.frames(4);
    assert_eq!(w.policy(), Targeting::Retaliate, "cycled once");
    let p = w.shell.play.running().expect("playing");
    assert_eq!(p.session.player.log().policies.len(), 1, "kept in the play's log once");
    // A release with nothing down is not the play's.
    assert!(!w.shell.play_pointer(PointerPhase::Up, center(label)));
    // No play: nothing is taken.
    drop(w);
}

#[test]
fn presses_on_a_watched_stage_follow_a_field_and_go_back() {
    use ddi_session::play::Watching;
    let mut w = Window::new();
    // Stage 1 watched: a press on a field follows it, on the large one
    // goes back.
    ddi_ui::screens::room::toggle_intent(&w.room);
    w.shell.ui.update(0.1);
    assert!(w.room.borrow().watching());
    w.start(false);
    w.frames(30);
    assert_eq!(w.watching(), Some((Watching::Spectator, None)));
    if !w.draws.drawn() {
        w.shell.play.feed.borrow_mut().watch_at = vec![
            (Some(0), [40.0, 100.0, 400.0, 600.0]),
            (Some(1), [460.0, 100.0, 820.0, 600.0]),
            (Some(2), [880.0, 100.0, 1240.0, 600.0]),
        ];
    }
    let fields = w.shell.play.feed.borrow().watch_at.clone();
    assert_eq!(fields.len(), 3, "{fields:?}");
    assert!(fields.iter().all(|(_, r)| Window::in_window(*r)), "{fields:?}");
    let (_, second) = fields.iter().find(|(s, _)| *s == Some(1)).copied().unwrap();
    // Between the fields: nobody's.
    assert!(!w.shell.play_pointer(PointerPhase::Down, [LOGICAL[0] - 1.0, LOGICAL[1] - 1.0]));
    assert!(w.shell.play_pointer(PointerPhase::Down, center(second)));
    // (Its release is nothing of the play's: a press is the whole of it.)
    assert!(!w.shell.play_pointer(PointerPhase::Up, center(second)));
    w.frames(3);
    assert_eq!(w.watching(), Some((Watching::Spectator, Some(1))));
    if !w.draws.drawn() {
        w.shell.play.feed.borrow_mut().watch_at = vec![
            (Some(0), [20.0, 100.0, 200.0, 400.0]),
            (Some(2), [1080.0, 100.0, 1260.0, 400.0]),
            (None, [440.0, 0.0, 840.0, 720.0]),
        ];
    }
    let fields = w.shell.play.feed.borrow().watch_at.clone();
    let (_, large) = fields.iter().find(|(s, _)| s.is_none()).copied().expect("the large field");
    assert!(Window::in_window(large), "{large:?}");
    assert!(w.shell.play_pointer(PointerPhase::Down, center(large)));
    w.frames(3);
    assert_eq!(w.watching(), Some((Watching::Spectator, None)), "every field again");
}

/// Out of a stage, the skip's plate over the player's own field: a press
/// on it ends the watch, though a field's place lies under it.
#[test]
fn a_press_on_the_skip_plate_ends_the_watch() {
    use ddi_session::play::Watching;
    let mut w = Window::new();
    // Nothing pressed: the player's life runs out.
    w.start(false);
    let mut frames = 0;
    while w.watching().is_none() {
        w.frame();
        frames += 1;
        assert!(w.shell.play.playing() && frames < 30 * 200, "the player never went out");
    }
    w.frames(6);
    assert_eq!(w.watching().map(|x| x.0), Some(Watching::Out));
    if !w.draws.drawn() {
        let mut feed = w.shell.play.feed.borrow_mut();
        feed.skip_at = Some([520.0, 400.0, 760.0, 440.0]);
        // (A field's place under the plate: the plate comes first.)
        feed.watch_at = vec![(Some(0), [440.0, 0.0, 840.0, 720.0])];
    }
    let plate = w.shell.play.feed.borrow().skip_at.expect("the skip's plate");
    assert!(Window::in_window(plate), "{plate:?} in the window's points");
    if w.draws.drawn() {
        assert!(plate[0] > 400.0 && plate[2] < 880.0, "over the player's own field: {plate:?}");
    }
    assert!(w.shell.play_pointer(PointerPhase::Down, center(plate)));
    let mut frames = 0;
    while w.shell.play.playing() {
        w.frame();
        frames += 1;
        assert!(frames < 30 * 20, "the press on the plate did not skip");
    }
    w.shell.ui.update(0.05);
    assert_eq!(common::ui::top(&w.shell.ui), "results");
}
