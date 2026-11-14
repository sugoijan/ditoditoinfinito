//! The practice room as the window opens it, and the bundled songs its
//! stages are picked from.

use ddi_desktop::play::host::Host;
use ddi_ui::Ui;
use ddi_ui::screens::Env;
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::room::SharedRoom;

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

/// The practice room (the one there is), its rules seen, its song select
/// on `ui` handing its stages' plays to `host`, half a second on.
pub fn open(ui: &mut Ui, env: &Env, host: &Host) -> SharedRoom {
    let room = ddi_desktop::room::current();
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
