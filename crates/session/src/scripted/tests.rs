use ddi_chart::formats::sm::parse_simfile;
use ddi_noteskin::Dir;

use super::*;

/// No images: the test scripts draw quads.
struct NoImages;

impl Images for NoImages {
    fn size(&self, _: Dir, _: &str) -> Option<(u32, u32)> {
        None
    }
    fn max_size(&self) -> u32 {
        4096
    }
}

/// 120 BPM: beat `b` at `b / 2` s; a background script at beat 4 (2 s)
/// and a foreground at beat 2 (1 s).
fn song() -> ddi_chart::Song {
    parse_simfile(
        "#TITLE:t;#OFFSET:0;#BPMS:0=120;\
         #BGCHANGES:4=BG=1=0=0=0,99999=-nosongbg-;#FGCHANGES:2=FG=1=0=0=0;\n\
         #NOTES:dance-single::Easy:1::\n0000\n,\n0000\n,\n0000\n,\n0000\n,\n1000\n;",
        "sm",
    )
    .unwrap()
}

fn files(bg: &str) -> SongFiles {
    SongFiles {
        pack: "Pack".into(),
        song: "Song".into(),
        files: vec!["BG/default.lua".into(), "FG/default.lua".into()],
        texts: vec![
            ("BG/default.lua".into(), bg.into()),
            (
                "FG/default.lua".into(),
                "return Def.Quad { OnCommand = cmd(zoomto, 100, 100; linear, 0.5; x, 100) }".into(),
            ),
        ],
    }
}

fn load(bg: &str) -> (Scripts, [Vec<BgSegment>; LAYERS]) {
    let song = song();
    let scripts = ["BG", "FG"];
    let layers = backgrounds::layers(
        &song,
        &[] as &[&str],
        &[] as &[&str],
        &scripts,
        &[] as &[&str],
        false,
    );
    let fgs = backgrounds::foregrounds(&song, &scripts);
    let s = Scripts::load(
        files(bg),
        env(&song, 16.0 / 9.0),
        &layers,
        &fgs,
        &NoImages,
        Box::new(|| 0.0),
    )
    .unwrap()
    .unwrap();
    (s, layers)
}

const FADING_QUAD: &str = "return Def.ActorFrame {\n\
    InitCommand = cmd(x, 5),\n\
    Def.Quad { OnCommand = cmd(FullScreen; diffusealpha, 0; linear, 1; diffusealpha, 1) },\n\
}";

#[test]
fn backgrounds_come_on_at_their_change_and_foregrounds_last_their_tweens() {
    let (mut s, layers) = load(FADING_QUAD);
    assert_eq!(s.frame(0.0, 0.0, &layers), None);
    assert!(s.draws(0.0, &layers).is_empty());
    // The foreground at 1 s, for its half-second tween.
    s.frame(1.0, 2.0, &layers);
    let d = s.draws(1.0, &layers);
    assert_eq!(d.len(), 1);
    assert!(d[0].2);
    s.frame(1.25, 2.5, &layers);
    assert_eq!(s.draws(1.25, &layers)[0].0.current().pos[0], 50.0);
    s.frame(1.6, 3.2, &layers);
    assert!(s.draws(1.6, &layers).is_empty());
    // The background at 2 s: `InitState` dropped its Init's x (StepMania
    // sets a background's state anew each time it comes on), `On` fades
    // its quad in over a second, on StepMania's 16:9 screen.
    s.frame(2.0, 4.0, &layers);
    s.frame(2.5, 5.0, &layers);
    let d = s.draws(2.5, &layers);
    assert_eq!(d.len(), 1);
    let (frame, alpha, foreground) = (&d[0].0, d[0].1, d[0].2);
    assert_eq!((alpha, foreground), (1.0, false));
    // The `UpperLeft` frame, then the script's.
    let script = &frame.children()[0];
    assert_eq!(script.current().pos[0], 0.0);
    let quad = &script.children()[0];
    assert!((quad.current().diffuse()[3] - 0.5).abs() < 1e-4);
    assert!((quad.current().scale[0] - 480.0 * 16.0 / 9.0).abs() < 1e-3);
    // The foreground never shows again.
    s.frame(3.0, 6.0, &layers);
    assert_eq!(s.draws(3.0, &layers).len(), 1);
    assert_eq!(s.stopped(), None);
}

#[test]
fn a_script_that_runs_away_stops_them_all() {
    let (mut s, layers) = load(
        "return Def.ActorFrame {\n\
           OnCommand = function(self) self:SetUpdateFunction(function() while true do end end) end,\n\
         }",
    );
    assert_eq!(s.frame(1.0, 2.0, &layers), None);
    let why = s.frame(2.0, 4.0, &layers);
    assert!(why.is_some());
    assert!(s.stopped().is_some());
    assert!(s.draws(2.0, &layers).is_empty());
    // Reported once.
    assert_eq!(s.frame(2.1, 4.2, &layers), None);
}

#[test]
fn a_song_without_scripts_has_none() {
    let song = song();
    let layers = backgrounds::layers(
        &song,
        &[] as &[&str],
        &[] as &[&str],
        &[] as &[&str],
        &[] as &[&str],
        false,
    );
    let r = Scripts::load(
        SongFiles::default(),
        env(&song, 1.5),
        &layers,
        &[],
        &NoImages,
        Box::new(|| 0.0),
    );
    assert!(matches!(r, Ok(None)));
}

#[test]
fn a_long_load_gives_the_host_its_turns() {
    let song = song();
    let scripts = ["BG", "FG"];
    let none: &[&str] = &[];
    let layers = backgrounds::layers(&song, none, none, &scripts, none, false);
    let fgs = backgrounds::foregrounds(&song, &scripts);
    let turns = std::rc::Rc::new(std::cell::Cell::new(0));
    let counted = turns.clone();
    let mut pause = move || -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        counted.set(counted.get() + 1);
        Box::pin(async {})
    };
    let bg = "local n = 0\nfor i = 1, 100000 do n = n + i end\nreturn Def.Actor {}";
    let load = Scripts::load_sliced(
        files(bg),
        env(&song, 16.0 / 9.0),
        &layers,
        &fgs,
        &NoImages,
        Box::new(|| 0.0),
        Some(&mut pause),
    );
    let loaded = ddi_noteskin::lua::now(async { load.await.map_err(ddi_lua::Failure::Error) });
    assert!(matches!(loaded, Ok(Some(_))));
    assert!(turns.get() > 0);
}
