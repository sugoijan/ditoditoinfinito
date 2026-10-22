//! Scripted backgrounds and foregrounds on desktop
//! (`docs/plans/scripted-backgrounds.md`): a song folder made here with a
//! `#BGCHANGES` and an `#FGCHANGES` naming folders with a `default.lua`,
//! played autoplayed on a fake clock with no sound device, drawn
//! offscreen through the play's view, pixels left of the field read back
//! at chosen song times. Skipped without a GPU adapter.

use std::path::Path;

use ddi_desktop::gpu;
use ddi_ui::state::settings::{BackgroundFrame, Settings};

mod common;
use common::offscreen_play::{OffscreenPlay, load, pixel};

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
    // 16 s of 16-bit stereo silence.
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
    std::fs::write(song.join("BG/boom.wav"), &wav).ok()?;
    std::fs::write(song.join("music.wav"), wav).ok()?;
    Some(())
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
) -> (Vec<(f64, [[u8; 3]; 2], usize)>, Option<String>, usize) {
    let loaded = load(root);
    let scripts = &loaded.entry.bg_scripts;
    assert_eq!(
        scripts.len(),
        if scripts.iter().any(|f| f.as_str().starts_with("Stop")) {
            5
        } else {
            4
        },
        "{scripts:?}"
    );
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
    let (samples, stopped, sounds) = play(&adapter, &plain, &[1.0, 3.0, 6.5, 7.5]);
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
    let (samples, stopped, _) = play(&adapter, &stop, &[9.5, 10.5]);
    assert!(near(samples[0].1[0], GREEN), "{samples:?}");
    assert!(near(samples[1].1[0], BLUE), "{samples:?}");
    assert!(stopped.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
