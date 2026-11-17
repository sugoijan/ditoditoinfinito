//! The practice room as the window opens it, and the bundled songs its
//! stages are picked from. A test's practice rooms and voices are seeded
//! ([`SEED`], or the test's own through [`seed`]), never from the time:
//! the same stage in every run.

use std::cell::Cell;

use ddi_desktop::play::host::Host;
use ddi_ui::Ui;
use ddi_ui::screens::Env;
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::room::SharedRoom;

/// The seed of the practice rooms the tests make (their CPUs: the
/// roster, each one's play, so the stage's battle) and of their voices'
/// takes, where a test names none: a room's own is the time it is made
/// and the voice's the run's start, so each run would play another stage
/// (other CPUs, other attacks, now and then a CPU outscoring the
/// autoplayed player) and say other takes.
pub const SEED: u64 = 7;

thread_local! {
    /// Whether this thread's rooms and voices are seeded ([`seed`]).
    static SEEDED: Cell<bool> = const { Cell::new(false) };
}

/// This thread's practice rooms and voices drawn from `seed` from now on
/// (`ddi_desktop::room::seed_for_tests`,
/// `ddi_desktop::play::voice::seed_for_tests`): the practice room there
/// is forgotten and the voices' shuffles begun anew.
pub fn seed(seed: u64) {
    ddi_desktop::room::seed_for_tests(seed);
    ddi_desktop::play::voice::seed_for_tests(seed);
    SEEDED.set(true);
}

/// This thread's rooms and voices seeded from [`SEED`] where the test
/// did not seed them yet ([`seed`]): called before anything that may make
/// a practice room (the theme's UI opening on one, `common::ui::theme`).
pub fn seeded() {
    if !SEEDED.get() {
        seed(default_seed());
    }
}

/// [`SEED`], or the one `DDI_PRACTICE_SEED` names: for trying a test's
/// assertions on other stages (each must hold on every stage it can be
/// given, or arrange the one it needs and seed it itself).
fn default_seed() -> u64 {
    std::env::var("DDI_PRACTICE_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(SEED)
}

/// The practice room there is (`ddi_desktop::room::current`), seeded
/// ([`seeded`]).
pub fn room() -> SharedRoom {
    seeded();
    ddi_desktop::room::current()
}

/// The bundled song `id` as song select lists it with one chart, its
/// `chart` in single as `difficulty`, named by its digest (as a room
/// names a stage's chart).
pub fn song_entry(id: &str, title: &str, chart: usize, difficulty: Difficulty) -> SongEntry {
    let song = ddi_desktop::play::load::read_chart(id).unwrap();
    SongEntry {
        id: id.into(),
        title: title.into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty,
            index: chart,
            digest: Some(song.chart_digest(chart)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    }
}

/// The practice room (the one there is, seeded: [`room`]), its rules
/// seen, its song select on `ui` handing its stages' plays to `host`,
/// half a second on.
pub fn open(ui: &mut Ui, env: &Env, host: &Host) -> SharedRoom {
    let room = room();
    room.borrow_mut().set_rules_seen(true);
    ui.push(ddi_desktop::room::room_screen_with(
        env,
        None,
        None,
        Some(host.requested.clone()),
    ));
    ui.update(0.5);
    room
}
