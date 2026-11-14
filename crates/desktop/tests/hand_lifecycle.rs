//! Consecutive supported-layout desktop plays on one host and cumulative fake audio clock.
mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_platform::HostTime;
use common::ui::key;

const SIZE: [u32; 2] = [960, 540];

#[test]
fn consecutive_completed_and_quit_plays_keep_the_hand_available() {
    common::indexes();
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let (mut ui, env) = common::ui::theme("select", common::ui::lowercase(), SIZE, false);
    ui.update(0.5);
    let mut host = Host::new(Rc::default(), None);
    let clock = common::rig::fake_clock(&mut host);
    let formats = common::gpu::formats();
    let dev = || Device {
        device: &device,
        queue: &queue,
        formats,
    };
    let target = common::gpu::Target::new(&device, SIZE);

    let dir = std::env::temp_dir().join(format!("ddi-hand-lifecycle-{}", std::process::id()));
    for (name, layout, rows) in [
        (
            "Double",
            "dance-double",
            "10000000\n00010000\n00001000\n00000001,\n00000010\n00000100\n00010000\n10000000",
        ),
        (
            "Half",
            "pump-halfdouble",
            "100000\n010000\n000100\n000001,\n000010\n001000\n100001\n100000",
        ),
        (
            "PumpDouble",
            "pump-double",
            "0010000000\n0001000000\n0000010000\n0000000100,\n0000001000\n0000100000\n0010000100\n0010000000",
        ),
        (
            "NineA",
            "danoni-9A",
            "100010000\n010000000\n000010001\n000001000,\n000020000\n000000100\n000000001\n000000010,\n000030000\n000010000\n0000M0000\n000010000",
        ),
        ("First", "dance-single", "1000\n0100\n0010\n0001"),
        ("Second", "dance-solo", "010010\n100001\n001000\n000100"),
        ("Third", "pump-single", "10000\n01000\n00100\n00010\n00001"),
        (
            "Fourth",
            "danoni-7",
            "1000000\n0100000\n0010000\n0001000\n0000100\n0000010\n0000001",
        ),
        (
            "Fifth",
            "techno-single8",
            "10000000\n01000000\n00100000\n00010000\n00001000\n00000100\n00000010\n00000001",
        ),
        (
            "Sixth",
            "danoni-9B",
            "100000000\n010000000\n001000000\n000100000\n000010000\n000001000\n000000100\n000000010\n000000001",
        ),
    ] {
        let song = dir.join("Pack").join(name);
        std::fs::create_dir_all(&song).unwrap();
        std::fs::write(song.join("song.sm"),format!("#TITLE:{name};#MUSIC:music.wav;#OFFSET:-1;#BPMS:0=120;#NOTES:{layout}::Easy:2:0,0,0,0,0:\n{rows};")).unwrap();
        std::fs::write(song.join("music.wav"), silence()).unwrap();
    }
    let (songs, report) = ddi_desktop::library::scan_folders(std::slice::from_ref(&dir));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(songs.len(), 10);
    ddi_desktop::library::register_for_tests(&songs);
    for round in 0..20 {
        let name = [
            "First",
            "Double",
            "Half",
            "PumpDouble",
            "Second",
            "Third",
            "Fourth",
            "Fifth",
            "Sixth",
            "NineA",
        ][round % 10];
        let song = songs.iter().find(|s| s.entry.title == name).unwrap();
        let request = Request::song(&song.entry.id, 0, true);
        *host.requested.borrow_mut() = Some(request);
        let started = Instant::now();
        loop {
            host.poll(&mut ui, &env, Some(dev()), 48_000);
            ui.update(1. / 60.);
            if host.prompt_ready() {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "round {round}: {:?}",
                host.prompt_waiting()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        key(&mut ui, "Enter", "Enter");
        ui.update(0.05);
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        assert!(host.playing());
        let h = host.hand_debug().unwrap();
        assert!(
            h["runtime_error"].is_null() && h["runtime"]["available"] == true,
            "round {round} startup: {h}"
        );
        assert!(h["character"]["drawn"].as_u64().unwrap() > 0, "{h}");
        assert_eq!(
            h["runtime"]["source_notes"],
            [4, 8, 9, 9, 6, 5, 7, 8, 9, 13][round % 10],
            "{h}"
        );
        assert_eq!(
            h["character"]["mat"]["pads"],
            if matches!(name, "Double" | "Half" | "PumpDouble" | "NineA") {
                2
            } else {
                1
            }
        );
        assert_eq!(
            h["character"]["tile_ids"].as_array().unwrap().len(),
            if matches!(name, "Double" | "Half" | "PumpDouble" | "NineA") {
                18
            } else {
                9
            }
        );
        let log_start = host.log.len();
        let mut frames = 0;
        while host.playing() {
            let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            if let Some(pressed) = match (round, frames) {
                (1, 60) | (1, 70) => Some(true),
                (1, 61) | (1, 71) => Some(false),
                _ => None,
            } {
                host.feed.borrow_mut().edges.push(ddi_platform::RawInput {
                    device: ddi_platform::DeviceId::Keyboard,
                    control: "Escape".into(),
                    pressed,
                    host_time: HostTime(now as f64 / 1e9),
                    slot: 0,
                });
            }
            common::rig::draw(&mut host, &dev(), &target, &mut ui, &env, now);
            let Some(h) = host.hand_debug() else {
                assert!(!host.playing());
                break;
            };
            assert!(
                h["runtime_error"].is_null(),
                "round {round}, frame {frames}: {h}"
            );
            assert_eq!(
                h["runtime"]["starved_frames"], 0,
                "round {round}, frame {frames}: {h}"
            );
            assert_eq!(h["runtime"]["available"], true, "{h}");
            frames += 1;
            assert!(frames < 1200, "play never ended");
        }
        let ending = if round == 1 { "quit " } else { "played " };
        assert!(
            host.log[log_start..].iter().any(|s| s.starts_with(ending)),
            "{:?}",
            host.log
        );
        ui.update(3.);
        key(&mut ui, "Escape", "Escape");
        ui.update(0.5);
        host.poll(&mut ui, &env, Some(dev()), 48_000);
        ui.update(0.5);
        assert!(host.idle(), "round {round} did not return to song select");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 4 s of stereo silence at 44.1 kHz.
fn silence() -> Vec<u8> {
    let quiet = vec![0.0f32; 44_100 * 4];
    ddi_library::wav::write(&[&quiet, &quiet], 44_100)
}
