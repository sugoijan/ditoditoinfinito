use super::*;
use crate::skin::Folder;
use crate::skin::tests::folder;

/// Every image 64 × 32, but `big` ones (a quarter of the screen).
struct Sizes;

impl Images for Sizes {
    fn size(&self, _: Dir, path: &str) -> Option<(u32, u32)> {
        if path.contains("big") {
            Some((320, 240))
        } else {
            crate::is_image(path).then_some((64, 32))
        }
    }

    fn max_size(&self) -> u32 {
        4096
    }
}

/// StepMania 3.9's 640 × 480.
const SCREEN: [f32; 2] = [640.0, 480.0];

fn song(files: &[(&str, &str)]) -> Folder {
    folder("Pack", "Song", files)
}

fn build(files: &[(&str, &str)], name: &str) -> (Animation, Vec<String>) {
    let skin = Skin::song(song(files));
    load(&skin, name, SCREEN, &mut Textures::default(), &Sizes)
}

/// The layers' frames under the animation's.
fn layers(a: &Animation) -> &[Actor] {
    a.tree.children()
}

fn pos(a: &Actor) -> [f32; 2] {
    let p = a.current().pos;
    [p[0], p[1]]
}

#[test]
fn ini_layers_center_or_stretch_their_sprites_in_number_order() {
    let ini = "[BGAnimation]\nLengthSeconds=3\n\
        [Layer10]\nFile=b.png\nType=1\n\
        [Layer2]\nfile=a.png\noncommand=diffusealpha,0;linear,1;diffusealpha,1\n\
        [Layer3]\nFile=a.png\nPlayer=2\n\
        [Layer4]\nFile=a.png\nCondition=IsPlayerEnabled(PLAYER_1)\n";
    let (mut a, notes) = build(
        &[
            ("BG/BGAnimation.ini", ini),
            ("BG/a.png", ""),
            ("BG/b.png", ""),
        ],
        "bg",
    );
    // Layer2, Layer10, the `LengthSeconds` actor; Player 2's left out,
    // the condition noted.
    assert_eq!(layers(&a).len(), 3, "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("Condition")), "{notes:?}");
    let sprite = &layers(&a)[0].children()[0];
    assert_eq!(pos(sprite), [320.0, 240.0]);
    let stretched = &layers(&a)[1].children()[0];
    assert_eq!(pos(stretched), [320.0, 240.0]);
    assert_eq!(stretched.current().scale[0], 10.0);
    assert_eq!(stretched.current().scale[1], 15.0);
    // `On` as a change shows it: the lowercased command plays.
    a.gain_focus();
    assert_eq!(a.tree.tween_time_left(), 3.0);
    a.update(0.5, &Clocks::default());
    let sprite = &layers(&a)[0].children()[0];
    assert!((sprite.current().diffuse()[3] - 0.5).abs() < 1e-4);
}

#[test]
fn particles_and_tiles_move_each_frame() {
    let ini = "[Layer1]\nType=particles\nFile=a.png\nNumParticles=5\n\
        VelocityXMin=100\nVelocityXMax=100\n\
        [Layer2]\nType=3\nFile=a.png\nTileVelocityX=-64\n";
    let (mut a, notes) = build(&[("BG/BGAnimation.ini", ini), ("BG/a.png", "")], "BG");
    assert!(notes.is_empty(), "{notes:?}");
    let particles = &layers(&a)[0];
    assert_eq!(particles.children().len(), 5);
    let before: Vec<_> = particles.children().iter().map(pos).collect();
    for p in &before {
        assert!((0.0..=640.0).contains(&p[0]) && (0.0..=480.0).contains(&p[1]));
    }
    // 2 + 640 / 64 by 2 + 480 / 32 tiles, placed before the first frame.
    let tiles = &layers(&a)[1];
    assert_eq!(tiles.children().len(), 12 * 17);
    assert_eq!(pos(&tiles.children()[0]), [0.0, 0.0]);
    assert_eq!(pos(&tiles.children()[1]), [64.0, 0.0]);
    a.update(0.5, &Clocks::default());
    for (p, b) in layers(&a)[0].children().iter().zip(&before) {
        let x = pos(p)[0];
        // 50 pixels right, wrapping past the right edge to the left one.
        assert!(
            x == b[0] + 50.0 || x == -32.0 * p.current().scale[0],
            "{x} {b:?}"
        );
    }
    // Half a second at -64: half a tile left; a second more, and the
    // first wraps around to the right.
    let tiles = &layers(&a)[1];
    assert_eq!(pos(&tiles.children()[1]), [32.0, 0.0]);
    assert_eq!(pos(&tiles.children()[0]), [-32.0, 0.0]);
    a.update(1.0, &Clocks::default());
    let tiles = &layers(&a)[1];
    assert_eq!(pos(&tiles.children()[0]), [12.0 * 64.0 - 96.0, 0.0]);
}

#[test]
fn a_repeating_layer_plays_on_again() {
    let ini = "[Layer1]\nFile=a.png\nCommandRepeatSeconds=1\n\
        OnCommand=addx,10\n";
    let (mut a, _) = build(&[("BG/BGAnimation.ini", ini), ("BG/a.png", "")], "BG");
    a.gain_focus();
    assert_eq!(pos(&layers(&a)[0].children()[0])[0], 330.0);
    a.update(0.6, &Clocks::default());
    a.update(0.6, &Clocks::default());
    assert_eq!(pos(&layers(&a)[0].children()[0])[0], 340.0);
}

#[test]
fn imports_bring_another_folders_layers() {
    let (a, notes) = build(
        &[
            ("BG/BGAnimation.ini", "[Layer1]\nImport=../Other\n"),
            ("Other/BGAnimation.ini", "[Layer1]\nFile=a.png\n"),
            ("Other/a.png", ""),
        ],
        "BG",
    );
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(layers(&a).len(), 1);
    assert!(matches!(layers(&a)[0].children()[0].kind, Kind::Sprite(_)));
}

#[test]
fn default_xml_comes_before_the_ini_and_draws_quads_and_text() {
    let xml = "<ActorFrame><children>\n\
        <Quad OnCommand=\"zoomto,640,480;xy,320,240;diffuse,1,0,0,1\" />\n\
        <Layer Text=\"Hello::world\" File=\"_shared\" />\n\
        <Layer File=\"sub\" />\n\
        </children></ActorFrame>";
    let (mut a, notes) = build(
        &[
            ("BG/default.xml", xml),
            ("BG/BGAnimation.ini", "[Layer1]\nFile=missing.png\n"),
            ("BG/sub/BGAnimation.ini", "[Layer1]\nFile=x.png\n"),
            ("BG/sub/x.png", ""),
        ],
        "BG",
    );
    assert!(notes.is_empty(), "{notes:?}");
    a.gain_focus();
    let kids = a.tree.children();
    assert_eq!(kids.len(), 3);
    assert_eq!(kids[0].current().diffuse(), [1.0, 0.0, 0.0, 1.0]);
    let Kind::Text(t) = &kids[1].kind else {
        panic!("{:?}", kids[1].kind)
    };
    assert_eq!(t.text(), "Hello world");
    // A folder loaded by a file: generic, its layer's sprite at its
    // parent's origin.
    let sub = &kids[2];
    assert_eq!(pos(&sub.children()[0].children()[0]), [0.0, 0.0]);
}

#[test]
fn old_folders_make_a_layer_of_each_image_by_its_name() {
    let (mut a, notes) = build(
        &[
            ("BG/1 center.png", ""),
            ("BG/2 stretchscrollleft add.png", ""),
            ("BG/3 big particlesfloatup.png", ""),
            ("BG/4 tilescrollright cyclecolor.png", ""),
            ("BG/_skipped.png", ""),
            ("BG/notes.txt", ""),
        ],
        "BG",
    );
    assert!(notes.is_empty(), "{notes:?}");
    let l = layers(&a);
    assert_eq!(l.len(), 4);
    assert_eq!(pos(&l[0].children()[0]), [320.0, 240.0]);
    let scroll = &l[1].children()[0];
    assert_eq!(scroll.blend, Blend::Add);
    let Kind::Sprite(s) = &scroll.kind else {
        panic!()
    };
    assert_eq!(s.texcoord_velocity, [0.5, 0.0]);
    assert_eq!(scroll.current().scale, [10.0, 15.0, 1.0]);
    // 640 × 480 over 320 × 240: four particles.
    assert_eq!(l[2].children().len(), 4);
    assert_eq!(l[3].children().len(), 12 * 17);
    assert_eq!(l[3].children()[0].effect(), ddi_actor::Effect::Rainbow);
    let y = pos(&l[2].children()[0])[1];
    a.update(0.1, &Clocks::default());
    let p = &layers(&a)[2].children()[0];
    // Up at 300 pixels a second times its zoom (0.7 for the first).
    let moved = y - pos(p)[1];
    assert!((moved - 21.0).abs() < 1e-3 || pos(p)[1] > 480.0, "{moved}");
}

#[test]
fn what_cannot_be_drawn_is_noted() {
    let (a, notes) = build(
        &[
            (
                "BG/BGAnimation.ini",
                "[Layer1]\nFile=songbackground\n[Layer2]\nFile=@GetSongBackground()\n\
                 [Layer3]\nType=tiles\nFile=a.png\nTilesSpacingX=0.001\n",
            ),
            ("BG/a.png", ""),
        ],
        "BG",
    );
    assert!(layers(&a).is_empty());
    assert_eq!(notes.len(), 3, "{notes:?}");
}

#[test]
fn every_play_places_particles_the_same() {
    let files = [
        (
            "BG/BGAnimation.ini",
            "[Layer1]\nType=2\nFile=a.png\nNumParticles=3\nZoomMin=0.5\nZoomMax=2\n",
        ),
        ("BG/a.png", ""),
    ];
    let (a, _) = build(&files, "BG");
    let (b, _) = build(&files, "BG");
    assert_eq!(a.tree, b.tree);
}
