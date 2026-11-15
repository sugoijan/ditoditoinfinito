//! The lines of who attacks whom (`ddi_shell::attack_links`) as the
//! desktop window draws them, on this machine's GPU offscreen and a fake
//! clock with no sound device: chosen arrangements of links shown in a
//! real CPU-practice stage in the battle's place
//! (`ddi_shell::play_view::PlayView::stage_links`, a hook for tests and
//! pictures), so the same moments can be looked at again after a change:
//! one link, four at once from the player, an incoming and an outgoing
//! crossing, a chained attack, 72 opponents with several links, and a
//! watcher's grid with links between its fields; at 1280×720 and on a
//! phone held upright. Skipped where there is no GPU adapter (CI).
//!
//! `DDI_OFFSCREEN_PPM=<dir> cargo test -p ddi-desktop --test
//! attack_lines_offscreen -- --ignored --nocapture` keeps the pictures
//! (PNG) there.

mod common;
use std::rc::Rc;

use ddi_desktop::play::host::Host;
use ddi_room::CpuMode;
use ddi_shell::play_view::StagedLink;
use ddi_ui::screens::data::Difficulty;
use ddi_ui::state::settings::Settings;

/// A link from a seat to a seat (none: the player), `age` seconds old.
fn link(from: Option<usize>, to: Option<usize>, age: f32) -> StagedLink {
    StagedLink {
        from,
        to,
        age,
        chained: false,
    }
}

fn chained(l: StagedLink) -> StagedLink {
    StagedLink { chained: true, ..l }
}

/// A stage of `cpus` CPUs at `size`, played (autoplayed) or watched,
/// three seconds into its song.
fn stage(name: &str, size: [u32; 2], cpus: u16, watched: bool) -> Option<common::rig::Rig> {
    common::indexes();
    let adapter = common::gpu::adapter_or_skip()?;
    let settings = Settings {
        dancing_hand: false,
        ..Settings::default()
    };
    settings.save();
    let (ui, env) = common::ui::theme("start", common::ui::lowercase(), size, false);
    let host = Host::new(Rc::default(), None);
    let mut rig = common::rig::Rig::new(&adapter, size, ui, env, host);
    ddi_desktop::room::develop(0);
    let room = common::practice::open(&mut rig.ui, &rig.env, &rig.host);
    let entry = common::practice::song_entry("beatucada", "Beatucada", 4, Difficulty::Medium);
    {
        let mut r = room.borrow_mut();
        let mut rules = ddi_room::MatchRules {
            stages: 1,
            ..r.rules().clone()
        };
        rules.seats = rules.seats.max(cpus + 1);
        rules.cpu.mode = CpuMode::Fixed { count: cpus };
        rules.cpu.opponent.level = 9;
        rules.cpu.opponent.seed = 7;
        rules.charts = ddi_room::Charts::AsPicked;
        rules.set_attacks(true);
        r.save_rules(rules).unwrap();
    }
    if watched {
        ddi_ui::screens::room::toggle_intent(&room);
        rig.ui.update(0.1);
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    rig.ui.update(0.1);
    let mut request = rig.host.requested.borrow_mut().take().expect("the stage");
    request.auto = !watched;
    *rig.host.requested.borrow_mut() = Some(request);
    rig.wait_prompt();
    if watched {
        rig.ui.update(0.5);
        rig.countdown(&room);
    } else {
        rig.ready_and_start(&room);
    }
    assert!(rig.host.playing(), "{name}: the stage plays");
    for i in 0..(30 * 5) {
        rig.frame(1_000_000_000 / 30);
        if i % 16 == 0 {
            rig.wait_gpu();
        }
    }
    Some(rig)
}

/// `links` shown in the stage's frames, and the picture kept as `name`.
fn shoot(rig: &mut common::rig::Rig, name: &str, links: Vec<StagedLink>) {
    let asked = links.len();
    rig.host.stage_links(Some(links));
    // The fields' fonts are read on a thread: real time for them.
    for _ in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        rig.frame(1_000_000_000 / 120);
        rig.wait_gpu();
    }
    let drawn = rig.host.attack_links();
    let lines = drawn.iter().filter(|l| l.path.len() >= 2).count();
    eprintln!("picture {name}: {asked} links asked, {} shown, {lines} with a line", drawn.len());
    let size = rig.target.size;
    common::shot(&format!("{name}.png"), &rig.read(), size);
}

const WIDE: [u32; 2] = [1280, 720];
const PHONE: [u32; 2] = [390, 844];

#[test]
#[ignore = "pictures"]
fn eight_opponents_links() {
    for (tag, size) in [("wide", WIDE), ("phone", PHONE)] {
        let Some(mut rig) = stage(tag, size, 8, false) else {
            return;
        };
        let me = None;
        shoot(&mut rig, &format!("{tag}-1-one"), vec![link(me, Some(5), 0.3)]);
        shoot(
            &mut rig,
            &format!("{tag}-2-four-from-the-player"),
            vec![
                link(me, Some(0), 0.2),
                link(me, Some(2), 0.3),
                link(me, Some(5), 0.4),
                link(me, Some(7), 0.5),
            ],
        );
        shoot(
            &mut rig,
            &format!("{tag}-3-incoming-and-outgoing"),
            vec![
                link(me, Some(7), 0.3),
                link(Some(4), me, 0.4),
                link(me, Some(1), 0.2),
                link(Some(2), me, 0.5),
            ],
        );
        shoot(
            &mut rig,
            &format!("{tag}-4-chained"),
            vec![
                link(Some(4), me, 0.3),
                chained(link(Some(6), me, 0.3)),
                chained(link(Some(1), me, 0.3)),
            ],
        );
        shoot(
            &mut rig,
            &format!("{tag}-5-everyone-at-the-player"),
            (0..8).map(|s| link(Some(s), me, 0.1 + 0.05 * s as f32)).collect(),
        );
    }
}

#[test]
#[ignore = "pictures"]
fn seventy_two_opponents_links() {
    for (tag, size) in [("wide", WIDE), ("phone", PHONE)] {
        let Some(mut rig) = stage(tag, size, 72, false) else {
            return;
        };
        let me = None;
        shoot(
            &mut rig,
            &format!("{tag}-6-seventy-two-several"),
            vec![
                link(me, Some(10), 0.2),
                link(me, Some(40), 0.3),
                link(me, Some(70), 0.4),
                link(Some(5), me, 0.3),
                link(Some(33), me, 0.35),
                link(Some(50), me, 0.4),
                link(Some(60), me, 0.5),
            ],
        );
        shoot(
            &mut rig,
            &format!("{tag}-7-seventy-two-many"),
            (0..24)
                .map(|i| {
                    let seat = (i * 3) % 72;
                    let age = 0.1 + 0.02 * i as f32;
                    if i % 3 == 0 {
                        link(me, Some(seat), age)
                    } else {
                        link(Some(seat), me, age)
                    }
                })
                .collect(),
        );
    }
}

#[test]
#[ignore = "pictures"]
fn a_watched_grid_s_links() {
    for (tag, size) in [("wide", WIDE), ("phone", PHONE)] {
        for cpus in [8usize, 24, 72] {
            let Some(mut rig) = stage(tag, size, cpus as u16, true) else {
                return;
            };
            let n = cpus;
            shoot(
                &mut rig,
                &format!("{tag}-8-grid-{cpus}-few"),
                vec![
                    link(Some(0), Some(n - 3), 0.3),
                    link(Some(3), Some(1), 0.3),
                    link(Some(n - 2), Some(2), 0.4),
                    link(Some(2), Some(n - 2), 0.2),
                ],
            );
            shoot(
                &mut rig,
                &format!("{tag}-9-grid-{cpus}-many"),
                (0..n.min(20))
                    .map(|i| link(Some((i * 5) % n), Some((i * 7 + 3) % n), 0.1 + 0.02 * i as f32))
                    .filter(|l| l.from != l.to)
                    .collect(),
            );
            // Several against one and from one.
            shoot(
                &mut rig,
                &format!("{tag}-10-grid-{cpus}-fan"),
                (1..n.min(7))
                    .map(|i| link(Some(i), Some(0), 0.3))
                    .chain((0..n.min(5)).map(|i| link(Some(n - 1), Some(n - 2 - i), 0.3)))
                    .collect(),
            );
        }
    }
}
