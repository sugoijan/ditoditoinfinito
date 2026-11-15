//! Shift changes the attack policy during play (the maintainer,
//! 2026-11-13), as the desktop window draws it, on this machine's GPU
//! offscreen and a fake clock with no sound device: a CPU-practice
//! stage with attacks, its prompt's policy row saying how to change it
//! during play (Shift's cap, "during play"), then the play, where Shift
//! cycles the policy and the label under the life gauge has Shift's
//! small cap after it. Without a GPU adapter (CI) the stage runs
//! undrawn (`common::rig::Draws`): the prompt's row and its key's hint,
//! the play's policy key and Shift cycling it are checked all the same;
//! only the pictures are skipped. `DDI_OFFSCREEN_PPM=<dir>` keeps
//! `prompt.ppm` and `play.ppm`.

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::play::host::Host;
use ddi_platform::HostTime;
use ddi_ui::screens::data::Difficulty;
use common::ui::key;

const SIZE: [u32; 2] = [1280, 720];

#[test]
fn shift_is_shown_on_the_prompt_and_beside_the_policy_and_cycles_it() {
    common::indexes();
    let mut pictures = common::gpu::Pictures::new();
    let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), SIZE, true);
    let mut host = Host::new(Rc::default(), None);
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    let room = common::practice::open(&mut ui, &env, &host);
    let chart = 4;
    let entry = common::practice::song_entry("beatucada", "Beatucada", chart, Difficulty::Medium);
    {
        let mut r = room.borrow_mut();
        let mut rules = ddi_room::MatchRules {
            stages: 1,
            ..r.rules().clone()
        };
        rules.set_attacks(true);
        r.save_rules(rules).unwrap();
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    ui.update(0.1);
    let mut request = host.requested.borrow_mut().take().expect("asked for");
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    let mut waited = 0;
    while !host.prompt_ready() {
        draws.poll(&mut host, &mut ui, &env);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1000, "the song was never read");
    }
    ui.update(0.5);
    // The prompt: the policy row, and under it Shift's cap and "during
    // play".
    assert!(ui.actor("Opt.room.targeting").is_some(), "{:?}", ui.layers());
    assert!(ui.actor("Opt.room.targeting.Key").is_some(), "the key's hint");
    if pictures.drawn() {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    ui.update(0.1);
    pictures.shot(&mut ui, SIZE, "prompt.ppm");
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    {
        let mut r = room.borrow_mut();
        let later = r.now_ms() + 3_000;
        r.tick(later);
    }
    ui.update(0.05);
    draws.poll(&mut host, &mut ui, &env);
    assert!(host.playing(), "{:?}", ui.layers());
    use ddi_room::attacks::Targeting;
    let mut frames = 0;
    let mut shot = false;
    while host.playing() && frames < 200 {
        let now = clock.fetch_add(1_000_000_000 / 4, Ordering::SeqCst) + 1_000_000_000 / 4;
        if frames == 40 {
            let p = host.running().expect("playing");
            assert_eq!(p.session.policy_key(), Some(ddi_session::play::PolicyKey::Key("ShiftLeft")));
            for pressed in [true, false] {
                host.feed.borrow_mut().edges.push(ddi_platform::RawInput {
                    device: ddi_platform::DeviceId::Keyboard,
                    control: "ShiftLeft".into(),
                    pressed,
                    host_time: HostTime(now as f64 / 1e9 - 0.2),
                    slot: 0,
                });
            }
        }
        if frames == 44 {
            let p = host.running().expect("playing");
            let b = p.session.battle().expect("a stage");
            assert_eq!(b.policy(&p.session.player), Some(Targeting::Retaliate), "Shift cycled it");
            assert_eq!(p.session.player.log().policies.len(), 1);
        }
        draws.frame(&mut host, &mut ui, &env, now);
        if frames == 46 && !shot {
            if draws.drawn() {
                std::thread::sleep(std::time::Duration::from_millis(200));
                draws.wait_gpu();
                draws.frame(&mut host, &mut ui, &env, now);
                draws.shot("play.ppm");
            }
            shot = true;
        }
        if frames % 16 == 0 {
            draws.wait_gpu();
        }
        frames += 1;
    }
    assert!(shot, "the play went on past its policy change");
}
