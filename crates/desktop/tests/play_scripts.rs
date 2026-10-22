//! Scripted backgrounds and foregrounds on desktop
//! (`docs/plans/scripted-backgrounds.md`): a song folder made here with a
//! `#BGCHANGES` and an `#FGCHANGES` naming folders with a `default.lua`
//! (or in StepMania 3.9's formats),
//! played autoplayed on a fake clock with no sound device, drawn
//! offscreen through the play's view, pixels left of the field read back
//! at chosen song times. Skipped without a GPU adapter.

use std::path::Path;

use ddi_desktop::gpu;
use ddi_ui::state::settings::{BackgroundFrame, Settings};

mod common;
use common::offscreen_play::{OffscreenPlay, load, pixel};
use ddi_desktop::play::scripts::{LUA_TURN, ScriptLoad};

/// 120 BPM: beat `b` at `b / 2` s. The song background (blue) until 2 s,
/// then the `BG` script: a green quad over the whole screen and the
/// folder's red star, four times its size, left of the field; from 6 s,
/// the `FG` script: a white square at the screen's left edge for a
/// second (`sleep`), then nothing. `Stop` runs away when it comes on at
/// 10 s: every script stops, the song background shows.
fn simfile(stop: bool) -> String {
    let stop = if stop { ",20=Stop=1=0=0=0" } else { "" };
    format!(
        "#TITLE:Scripts;#ARTIST:Test;#MUSIC:music.wav;#OFFSET:0;#BPMS:0=120;\
         #BACKGROUND:bg.png;\
         #BGCHANGES:4=BG=1=0=0=0{stop},99999=-nosongbg-;#FGCHANGES:12=FG=1=0=0=0;\
         #NOTES:dance-single:x:Easy:1:0,0,0,0,0:\
         1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
         1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,1000\n0100\n0010\n0001\n,\
         1000\n0100\n0010\n0001\n;"
    )
}

const BG: &str = "return Def.ActorFrame {\n\
    Def.Quad { OnCommand = cmd(FullScreen; diffuse, color(\"0,0.8,0,1\")) },\n\
    LoadActor(\"star.png\") .. { OnCommand = cmd(xy, 100, 240; zoom, 4) },\n\
    Def.Sound { File = \"boom.wav\", OnCommand = cmd(play) },\n\
    Def.BitmapText { Font = \"Common Normal\", Text = \"TEXT\",\n\
      OnCommand = cmd(xy, 120, 400; zoom, 2; diffuse, color(\"1,1,0,1\")) },\n\
}\n";

const FG: &str = "return Def.Quad {\n\
    OnCommand = cmd(zoomto, 80, 80; xy, 40, 240; sleep, 1),\n\
}\n";

const STOP: &str = "return Def.ActorFrame {\n\
    OnCommand = function(self) self:SetUpdateFunction(function() while true do end end) end,\n\
}\n";

/// The song folder under `dir`.
fn song(dir: &Path, stop: bool) -> Option<()> {
    let song = dir.join("Pack/Scripts");
    for (folder, script) in [("BG", BG), ("FG", FG), ("Stop", STOP)] {
        std::fs::create_dir_all(song.join(folder)).ok()?;
        std::fs::write(song.join(folder).join("default.lua"), script).ok()?;
    }
    std::fs::write(song.join("song.sm"), simfile(stop)).ok()?;
    for (name, rgba) in [
        ("bg.png", [30, 30, 200, 255]),
        ("BG/star.png", [200, 30, 30, 255]),
    ] {
        image::RgbaImage::from_pixel(16, 16, image::Rgba(rgba))
            .save(song.join(name))
            .ok()?;
    }
    let wav = silence();
    std::fs::write(song.join("BG/boom.wav"), &wav).ok()?;
    std::fs::write(song.join("music.wav"), wav).ok()?;
    Some(())
}

/// 16 s of 16-bit stereo silence, a WAV.
fn silence() -> Vec<u8> {
    let rate = 44_100u32;
    let data = vec![0u8; (rate * 4 * 16) as usize];
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    wav
}

/// The pixels read: left of the field's middle (20, 180) and on the star
/// (75, 180).
const POINTS: [(u32, u32); 2] = [(20, 180), (75, 180)];

/// Plays the song under `root`, a frame each 1/60 s, and reads
/// [`POINTS`] at the first frame at or after each of `times` (song
/// seconds); with the results' note on the scripts, if any.
#[allow(clippy::type_complexity)]
fn play(
    adapter: &wgpu::Adapter,
    root: &Path,
    times: &[f64],
    files: usize,
) -> (Vec<(f64, [[u8; 3]; 2], usize)>, Option<String>, usize) {
    let loaded = load(root);
    let scripts = &loaded.entry.bg_scripts;
    assert_eq!(scripts.len(), files, "{scripts:?}");
    let settings = Settings {
        noteskin: Some("builtin".into()),
        bg_brightness: 1.0,
        field_filter: 0.0,
        background_frame: BackgroundFrame::SixteenNine,
        ..Settings::default()
    };
    let mut p = OffscreenPlay::new(adapter, loaded, &settings);
    assert!(p.view.scripts_running());
    let mut out = Vec::new();
    p.run(
        times,
        |view, play| {
            // The scripts' sounds, played as the host plays them.
            for sound in view.take_script_sounds() {
                play.session.play_now(&sound).unwrap();
            }
        },
        |t, pixels| {
            let at = |p| pixel(pixels, p);
            // The script's yellow text, left of the field's bottom half.
            let yellow = (270..330)
                .flat_map(|y| (30..160).map(move |x| (x, y)))
                .filter(|&p| {
                    let [r, g, b] = at(p);
                    r > 150 && g > 150 && b < 90
                })
                .count();
            out.push((t, POINTS.map(at), yellow));
        },
    );
    (
        out,
        p.view.scripts_stopped().map(str::to_string),
        p.view.sounds_played,
    )
}

fn near(got: [u8; 3], want: [u8; 3]) -> bool {
    got.iter()
        .zip(want)
        .all(|(&g, w)| (i32::from(g) - i32::from(w)).abs() <= 3)
}

const BLUE: [u8; 3] = [30, 30, 200];
const GREEN: [u8; 3] = [0, 204, 0];
const RED: [u8; 3] = [200, 30, 30];
const WHITE: [u8; 3] = [255, 255, 255];

#[test]
fn scripts_draw_behind_the_field_and_over_it() {
    let dir = std::env::temp_dir().join(format!("ddi-play-scripts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let plain = dir.join("plain");
    song(&plain, false).expect("the song folder");
    let (samples, stopped, sounds) = play(&adapter, &plain, &[1.0, 3.0, 6.5, 7.5], 4);
    assert_eq!(stopped, None);
    // The background's `On` played its sound once.
    assert_eq!(sounds, 1);
    let want = [
        [BLUE, BLUE],
        // The script's screen: green, the star on it.
        [GREEN, RED],
        // The foreground's white square over the field's left.
        [WHITE, RED],
        // Its second is over.
        [GREEN, RED],
    ];
    assert_eq!(samples.len(), want.len(), "{samples:?}");
    for ((t, got, _), want) in samples.iter().zip(want) {
        for (g, w) in got.iter().zip(want) {
            assert!(near(*g, w), "at {t:.3} s: {got:?}, want {want:?}");
        }
    }
    // The script's text (`BitmapText`), yellow, with the script only.
    assert_eq!(samples[0].2, 0, "{samples:?}");
    assert!(samples[1].2 > 40, "{samples:?}");
    // A script that runs away stops them all: the song background, and a
    // note on the results.
    let stop = dir.join("stop");
    song(&stop, true).expect("the song folder");
    let (samples, stopped, _) = play(&adapter, &stop, &[9.5, 10.5], 5);
    assert!(near(samples[0].1[0], GREEN), "{samples:?}");
    assert!(near(samples[1].1[0], BLUE), "{samples:?}");
    assert!(stopped.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A song of StepMania 3.9's animations (no Lua): the song background
/// until 2 s; then `Old` (`BGAnimation.ini`): a green image stretched over
/// the screen and the red one moved left of the field by its `On`; from
/// 6 s the `Front` foreground (an ini with `LengthSeconds=1`): a white
/// square at the screen's left edge for a second; from 10 s `Layers`, a
/// 3.0 folder whose one image (yellow) is named to stretch.
fn old_song(dir: &Path) -> Option<()> {
    let song = dir.join("Pack/Old");
    let files: [(&str, &str); 2] = [
        (
            "Old/BGAnimation.ini",
            "[Layer1]\nFile=green.png\nType=1\n\
             [Layer2]\nfile=red.png\noncommand=x,100;zoom,4\n",
        ),
        (
            "Front/BGAnimation.ini",
            "[BGAnimation]\nLengthSeconds=1\n\
             [Layer1]\nFile=white.png\nOnCommand=zoomto,80,80;x,40\n",
        ),
    ];
    for (name, text) in files {
        let path = song.join(name);
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(path, text).ok()?;
    }
    std::fs::create_dir_all(song.join("Layers")).ok()?;
    std::fs::write(
        song.join("song.sm"),
        simfile(false)
            .replace("=BG=", "=Old=")
            .replace("=FG=", "=Front=")
            .replace(",99999=", ",20=Layers=1=0=0=0,99999="),
    )
    .ok()?;
    for (name, rgba) in [
        ("bg.png", [30, 30, 200, 255]),
        ("Old/green.png", [0, 204, 0, 255]),
        ("Old/red.png", [200, 30, 30, 255]),
        ("Front/white.png", [255, 255, 255, 255]),
        ("Layers/1 stretchstill.png", [220, 220, 0, 255]),
    ] {
        image::RgbaImage::from_pixel(16, 16, image::Rgba(rgba))
            .save(song.join(name))
            .ok()?;
    }
    std::fs::write(song.join("music.wav"), silence()).ok()
}

#[test]
fn stepmania_3_9_animations_draw_natively() {
    let dir = std::env::temp_dir().join(format!("ddi-play-old-bgas-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    old_song(&dir).expect("the song folder");
    let (samples, stopped, _) = play(&adapter, &dir, &[1.0, 3.0, 6.5, 7.5, 10.5], 6);
    assert_eq!(stopped, None);
    const YELLOW: [u8; 3] = [220, 220, 0];
    let want = [
        [BLUE, BLUE],
        [GREEN, RED],
        [WHITE, RED],
        [GREEN, RED],
        [YELLOW, YELLOW],
    ];
    assert_eq!(samples.len(), want.len(), "{samples:?}");
    for ((t, got, _), want) in samples.iter().zip(want) {
        for (g, w) in got.iter().zip(want) {
            assert!(near(*g, w), "at {t:.3} s: {got:?}, want {want:?}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A script whose Lua spends most of the load's fuel before it returns
/// its actor.
const BURN: &str = "local n = 0\n\
    for i = 1, 8000000 do n = n + i end\n\
    return Def.Quad { OnCommand = cmd(FullScreen) }\n";

/// A script that takes long to load is readied a turn at a time: no poll
/// runs its Lua much past [`LUA_TURN`] (the loop draws between them), and
/// it loads all the same. No GPU, no window.
#[test]
fn scripts_load_a_turn_at_a_time() {
    let dir = std::env::temp_dir().join(format!("ddi-play-scripts-turns-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let song = dir.join("Pack/Burn");
    std::fs::create_dir_all(song.join("Burn")).unwrap();
    std::fs::write(song.join("Burn/default.lua"), BURN).unwrap();
    std::fs::write(
        song.join("song.sm"),
        "#TITLE:Burn;#ARTIST:Test;#MUSIC:music.wav;#OFFSET:0;#BPMS:0=120;\
         #BGCHANGES:0=Burn=1=0=0=0;\
         #NOTES:dance-single:x:Easy:1:0,0,0,0,0:1000\n0100\n0010\n0001\n;",
    )
    .unwrap();
    std::fs::write(song.join("music.wav"), silence()).unwrap();
    let loaded = load(&dir);
    let settings = Settings {
        bg_brightness: 1.0,
        ..Settings::default()
    };
    let mut scripts = ScriptLoad::start(&loaded, &settings, None).expect("scripts to load");
    let started = std::time::Instant::now();
    let (mut turns, mut longest) = (0, std::time::Duration::ZERO);
    let ready = loop {
        let running = scripts.running();
        let at = std::time::Instant::now();
        let polled = scripts.poll(LUA_TURN);
        if running {
            turns += 1;
            longest = longest.max(at.elapsed());
        }
        if let Some(r) = polled {
            break r;
        }
        assert!(started.elapsed().as_secs() < 60, "the load never ended");
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    eprintln!(
        "{turns} turns, the longest {:.1} ms, {:.0} ms in all",
        longest.as_secs_f64() * 1000.0,
        started.elapsed().as_secs_f64() * 1000.0
    );
    let ready = ready.expect("the script to load");
    assert!(ready.view.running());
    assert!(turns > 10, "{turns} turns");
    // A turn ends at the first slice past it.
    assert!(longest < LUA_TURN * 10, "{longest:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
