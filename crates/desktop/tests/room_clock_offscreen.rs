//! A room ticked on the window's clocks (review3 desktop-1, desktop-6):
//! the rooms are made and a live play reads them on the wall clock, the
//! screens tick them each frame with what the window gives the UI. After
//! the computer slept (its host clock standing still, the wall clock
//! moving on), a practice room opened then still counts its stage down
//! and starts it, on the UI's frames alone (never ticked by the test).
//! Without a GPU adapter (CI) only the
//! pictures (and a play's prompt, which readies its skin on one) are
//! skipped: the rest is checked all the same.

mod common;

use ddi_desktop::gpu;
use ddi_desktop::play::host::{Device, Host};
use ddi_ui::screens::data::Difficulty;
use common::rig::ShellClock;
use common::ui::{key, top};

const SIZE: [u32; 2] = [1280, 720];

#[test]
fn a_practice_countdown_ends_on_the_ui_frames_after_a_sleep() {
    common::indexes();
    // The GPU a play's prompt readies its skin on, where there is one:
    // the room's clock, the countdown and the stage's start need none.
    let gpu = common::gpu::adapter_or_skip().and_then(|a| gpu::device(&a).ok());
    let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), SIZE, true);
    let mut host = Host::new(Default::default(), None);
    let _fake = common::rig::fake_clock(&mut host);
    let formats = common::gpu::formats();
    let dev = || {
        gpu.as_ref().map(|(device, queue)| Device {
            device,
            queue,
            formats,
        })
    };
    // The window's first frame, a minute before now; then the computer
    // sleeps for that minute.
    let mut clock = ShellClock::now();
    clock.wall_ms -= 60_000;
    clock.frame(&mut ui, 0.05);
    clock.sleep(60);
    // Awake: the practice room opened for the first time (made on the
    // wall clock), a song picked, its prompt shown.
    let room = common::practice::open(&mut ui, &env, &host);
    clock.frame(&mut ui, 0.05);
    let entry = common::practice::song_entry("beatucada", "Beatucada", 1, Difficulty::Hard);
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    clock.frame(&mut ui, 0.1);
    let mut request = host.requested.borrow_mut().take().expect("the stage asked for");
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    let started = |room: &ddi_ui::state::room::SharedRoom| {
        room.borrow().state().phase.kind() == ddi_room::PhaseKind::Stage
    };
    if gpu.is_some() {
        let mut waited = 0;
        loop {
            host.poll(&mut ui, &env, dev(), 48_000);
            clock.frame(&mut ui, 0.05);
            if host.prompt_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 1;
            assert!(waited < 1000, "the song was never read");
        }
        for _ in 0..10 {
            clock.frame(&mut ui, 0.05);
        }
        assert_eq!(top(&ui), "play-prompt");
        // Ready: the room counts down three seconds; four seconds of
        // frames start the stage.
        key(&mut ui, "Enter", "Enter");
    } else {
        // No GPU: a prompt never readies its skin, so Ready is told the
        // room as the prompt's Ready tells it; the countdown on the UI's
        // frames is the room's and the shell's clock's all the same.
        for _ in 0..10 {
            host.poll(&mut ui, &env, None, 48_000);
            clock.frame(&mut ui, 0.05);
        }
        let jobs = room.borrow_mut().take_load_jobs();
        for job in jobs {
            room.borrow_mut().finish_load(&job, Ok(()));
        }
        room.borrow_mut().set_ready(true).expect("ready");
    }
    clock.frame(&mut ui, 0.05);
    assert!(room.borrow().ready());
    for _ in 0..80 {
        clock.frame(&mut ui, 0.05);
        host.poll(&mut ui, &env, dev(), 48_000);
        if host.playing() || (gpu.is_none() && started(&room)) {
            break;
        }
    }
    assert!(
        started(&room),
        "the room's stage started: {:?}, room at {} ms, wall {} ms",
        room.borrow().state().phase,
        room.borrow().now_ms(),
        clock.wall_ms
    );
    if gpu.is_none() {
        return;
    }
    assert!(
        host.playing(),
        "the stage started: {:?}, room at {} ms, wall {} ms",
        room.borrow().state().phase,
        room.borrow().now_ms(),
        clock.wall_ms
    );
}

/// The gap list's seventh: the wall clock steps back (a correction by
/// the network's time, a clock set by hand) while a practice room counts
/// a stage down. It was ignored as a finding (the countdown stood still
/// for as long as the clock went back: the rooms ran on the wall clock,
/// `RoomClient::tick` moving only forward); since round P1b (the
/// maintainer's of 2026-11-16) the rooms, the screens' ticks and a live
/// play run on one steady clock (`ddi_desktop::clock::now_ms`,
/// `ddi_platform::steady`), and it ends as ever. No GPU needed: the room
/// is readied as a prompt's Ready readies it.
#[test]
fn a_practice_countdown_still_ends_after_the_wall_clock_steps_back() {
    countdown_after_a_step(-60_000);
}

/// Round P1b: the wall clock steps forward a minute mid-countdown: the
/// countdown neither ends at once nor late, but on its three seconds.
#[test]
fn a_practice_countdown_ends_on_time_after_the_wall_clock_steps_forward() {
    countdown_after_a_step(60_000);
}

/// A practice room's countdown with the wall clock stepped by `step_ms`
/// half a second in: the stage starts after the rest of the countdown,
/// not a frame before and not much after.
fn countdown_after_a_step(step_ms: i64) {
    common::indexes();
    let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), SIZE, true);
    let mut host = Host::new(Default::default(), None);
    let _fake = common::rig::fake_clock(&mut host);
    let mut clock = ShellClock::now();
    clock.frame(&mut ui, 0.05);
    let room = common::practice::open(&mut ui, &env, &host);
    clock.frame(&mut ui, 0.05);
    let entry = common::practice::song_entry("beatucada", "Beatucada", 1, Difficulty::Hard);
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    clock.frame(&mut ui, 0.1);
    let jobs = room.borrow_mut().take_load_jobs();
    for job in jobs {
        room.borrow_mut().finish_load(&job, Ok(()));
    }
    room.borrow_mut().set_ready(true).expect("ready");
    clock.frame(&mut ui, 0.05);
    assert!(room.borrow().ready());
    let start_at = match room.borrow().state().phase {
        ddi_room::Phase::Countdown { start_at, .. } => start_at.0,
        ref p => panic!("counting down: {p:?}"),
    };
    let steady = |clock: &mut ShellClock| clock.steady_ms();
    for _ in 0..10 {
        clock.frame(&mut ui, 0.05);
    }
    // The step, mid-countdown.
    clock.wall_ms = (clock.wall_ms as i64 + step_ms) as u64;
    let mut started_at = None;
    for _ in 0..100 {
        clock.frame(&mut ui, 0.05);
        if room.borrow().state().phase.kind() == ddi_room::PhaseKind::Stage {
            started_at = Some(steady(&mut clock));
            break;
        }
    }
    let started_at = started_at.unwrap_or_else(|| {
        panic!(
            "five seconds of frames after the clock stepped {step_ms} ms: room at {} ms, wall {} ms",
            room.borrow().now_ms(),
            clock.wall_ms
        )
    });
    assert!(
        started_at >= start_at && started_at < start_at + 100,
        "started at {started_at}, due {start_at} (stepped {step_ms} ms)"
    );
}
