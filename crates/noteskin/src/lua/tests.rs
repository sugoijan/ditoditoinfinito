//! Loading skins' Lua: the stand-ins' own scripts, a skin's `NoteSkin.lua`
//! and elements, metrics commands as Lua, and failures.

use ddi_actor::{Clocks, Kind};

use super::*;
use crate::build::{TextureKey, Textures};
use crate::skin::tests::folder;
use crate::skin::{Dir, Folder, Skin};

struct Sizes(Vec<(&'static str, (u32, u32))>);

impl Images for Sizes {
    fn size(&self, _dir: Dir, path: &str) -> Option<(u32, u32)> {
        self.0.iter().find(|(p, _)| *p == path).map(|(_, s)| *s)
    }

    fn max_size(&self) -> u32 {
        8192
    }
}

fn lua_skin(folders: &[Folder], name: &str) -> Result<LuaSkin, Failure> {
    let skin = Skin::load(folders, "dance", name)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_owned();
    let mut t = 0.0;
    load(
        skin,
        Env::default(),
        LoadBudget::default(),
        Box::new(move || {
            t += 0.0001;
            t
        }),
    )
}

fn built(e: Result<Element, Failure>) -> ddi_actor::Actor {
    match e {
        Ok(Element {
            built: Built::Actor(a),
            ..
        }) => a,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_stand_ins_run_stepmanias_own_lua() {
    let mut skin = lua_skin(&[], "default").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let images = Sizes(vec![]);
    // dance/default's `Down Tap Note.lua`: an arrow and four circles, all
    // the stand-in's art, turned by `Rotate` (Left: 90).
    let tap = built(skin.element(Button::Left, "Tap Note", &mut textures, &images));
    assert_eq!(tap.base_rotation[2], 90.0);
    assert_eq!(tap.children().len(), 5);
    assert!(
        tap.textures()
            .iter()
            .all(|&t| matches!(textures.keys()[t as usize], TextureKey::BuiltIn(_)))
    );
    // Their Init ran: the second circle sits at y = 5.
    assert_eq!(tap.children()[2].current().pos[1], 5.0);
    // common's `Fallback Explosion.lua`: five parts, the row not turned
    // (dance/default's Load clears it), the flashes turned.
    let row = built(skin.element(Button::Up, "Explosion", &mut textures, &images));
    assert_eq!(row.base_rotation[2], 0.0);
    assert_eq!(row.children().len(), 5);
    assert_eq!(row.children()[3].base_rotation[2], 180.0);
    // Their Init: diffusealpha 0 on the flashes.
    assert_eq!(row.children()[3].current().diffuse()[3], 0.0);
    // dance/default blanks hold explosions: a plain actor (common's
    // `t .. { cmd(visible,false) }` puts the function in the array, where
    // it is no command, as in StepMania).
    assert_eq!(row.children()[0].kind, Kind::Actor);
    // The receptor: `_Tap Lead-in Receptor.lua` via `Fallback Receptor`
    // and dance/default's own `Down Receptor.lua`.
    let receptor = built(skin.element(Button::Down, "Receptor", &mut textures, &images));
    assert_eq!(receptor.children().len(), 2);
    // A hold body must be a sprite, and is.
    let body = built(skin.element(Button::Down, "Hold Body Active", &mut textures, &images));
    assert!(matches!(body.kind, Kind::Sprite(_)));
    assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
}

/// A skin in StepMania's style: its own `NoteSkin.lua` on dance/default,
/// a receptor in Lua with relative textures and metrics commands, a tap
/// note with `Frames`, and `cmd`.
fn delta_like() -> Vec<Folder> {
    vec![folder(
        "dance",
        "d",
        &[
            (
                "metrics.ini",
                "[Global]\nFallbackNoteSkin=default\n[ReceptorArrow]\nInitCommand=zoom,0.5;effectclock,\"beat\"\nNoneCommand=finishtweening\n[ReceptorOverlay]\nInitCommand=diffusealpha,0\nPressCommand=diffusealpha,1\n",
            ),
            (
                "NoteSkin.lua",
                "local ret = ... or {}\n\
                 local OldRedir = ret.Redir\n\
                 ret.Redir = function(b, e)\n\
                   b, e = OldRedir(b, e)\n\
                   if e == 'Tap Lift' then e = 'Tap Note' end\n\
                   return b, e\n\
                 end\n\
                 ret.Blank['Tap Mine'] = true\n\
                 return ret\n",
            ),
            (
                "Down Receptor.lua",
                "local t = Def.ActorFrame {\n\
                   Def.Sprite {\n\
                     Texture = 'Receptor 4x1.png';\n\
                     Frame0000 = 0; Delay0000 = 0.95;\n\
                     Frame0001 = 3; Delay0001 = 0.05;\n\
                     InitCommand = NOTESKIN:GetMetricA('ReceptorArrow', 'InitCommand');\n\
                     NoneCommand = NOTESKIN:GetMetricA('ReceptorArrow', 'NoneCommand');\n\
                   };\n\
                   Def.Sprite {\n\
                     Name = 'Overlay';\n\
                     Texture = 'Receptor 4x1.png';\n\
                     InitCommand = NOTESKIN:GetMetricA('ReceptorOverlay', 'InitCommand');\n\
                     PressCommand = NOTESKIN:GetMetricA('ReceptorOverlay', 'PressCommand');\n\
                   };\n\
                 }\n\
                 return t\n",
            ),
            ("Receptor 4x1.png", ""),
            (
                "Down Tap Note.lua",
                "return Def.Sprite {\n\
                   Texture = NOTESKIN:GetPath('_down', 'tap note');\n\
                   Frames = { { Frame = 1; Delay = 0.5 }, { Frame = 2 }, { { x = 0, y = 0 }, { x = 0.5, y = 1 } } };\n\
                   InitCommand = cmd(diffuse, color('1,0,0,1'); addy, 2; playcommand, 'Glow');\n\
                   GlowCommand = function(self) self:glow(color('#00ff00')) end;\n\
                 }\n",
            ),
            ("_down tap note 4x2.png", ""),
        ],
    )]
}

#[test]
fn a_skins_own_lua_builds_its_elements() {
    let folders = delta_like();
    let mut skin = lua_skin(&folders, "d").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let images = Sizes(vec![
        ("Receptor 4x1.png", (256, 64)),
        ("_down tap note 4x2.png", (256, 128)),
    ]);
    let receptor = built(skin.element(Button::Up, "Receptor", &mut textures, &images));
    assert_eq!(receptor.base_rotation[2], 180.0);
    let [under, overlay] = receptor.children() else {
        panic!("{receptor:?}")
    };
    let Kind::Sprite(s) = &under.kind else {
        panic!()
    };
    // Two states, frames 0 and 3 of the 4x1 sheet.
    assert_eq!(s.states.len(), 2);
    assert_eq!(s.states[1].rect, [0.75, 0.0, 1.0, 1.0]);
    assert_eq!(s.states[0].delay, 0.95);
    // Init from the metrics, compiled to Lua.
    assert_eq!(under.current().scale[0], 0.5);
    assert_eq!(overlay.name, "Overlay");
    assert_eq!(overlay.current().diffuse()[3], 0.0);
    assert!(matches!(
        &textures.keys()[s.texture as usize],
        TextureKey::Skin { path, .. } if path == "Receptor 4x1.png"
    ));

    // The tap note: `Frames` with a sub-rectangle, `cmd`, a Lua command
    // played from Init.
    let tap = built(skin.element(Button::Right, "Tap Note", &mut textures, &images));
    let Kind::Sprite(s) = &tap.kind else { panic!() };
    assert_eq!(s.states.len(), 3);
    assert_eq!(s.states[0].rect, [0.25, 0.0, 0.5, 0.5]);
    assert_eq!(s.states[0].delay, 0.5);
    // The third keeps frame 2 (the previous one's) and takes its left half.
    assert_eq!(s.states[2].rect, [0.5, 0.0, 0.625, 0.5]);
    assert_eq!(tap.current().diffuse(), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(tap.current().pos[1], 2.0);
    assert_eq!(tap.current().glow, [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(tap.base_rotation[2], -90.0);

    // The skin's NoteSkin.lua wrapped dance/default's: lifts are taps,
    // mines blank.
    let lift = built(skin.element(Button::Down, "Tap Lift", &mut textures, &images));
    assert!(matches!(lift.kind, Kind::Sprite(_)));
    let mine = built(skin.element(Button::Down, "Tap Mine", &mut textures, &images));
    assert_eq!(mine.kind, Kind::Actor);
    assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
    assert!(skin.fuel_used() > 0);
}

#[test]
fn update_after_load_animates_natively() {
    let folders = delta_like();
    let mut skin = lua_skin(&folders, "d").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let images = Sizes(vec![("Receptor 4x1.png", (256, 64))]);
    let mut receptor = built(skin.element(Button::Down, "Receptor", &mut textures, &images));
    let mut p = ddi_actor::Problems::default();
    // `effectclock,"beat"` from the metrics: the frames follow beats.
    let at = |beat| Clocks {
        beat,
        ..Clocks::default()
    };
    receptor.update(0.0, &at(0.0), &mut p);
    receptor.update(0.0, &at(0.97), &mut p);
    let Kind::Sprite(s) = &receptor.children()[0].kind else {
        panic!()
    };
    assert_eq!(s.state(), 1);
}

#[test]
fn errors_fall_back_per_element_and_limits_fail_the_skin() {
    let broken = |file: &'static str, text: &'static str| {
        vec![folder(
            "dance",
            "b",
            &[
                ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
                (file, text),
            ],
        )]
    };
    let mut textures = Textures::default();
    let images = Sizes(vec![]);
    // An error in an element: that element is built in, the rest loads.
    let folders = broken("Down Tap Note.lua", "error('no')");
    let mut skin = lua_skin(&folders, "b").unwrap_or_else(|e| panic!("{e}"));
    match skin.element(Button::Down, "Tap Note", &mut textures, &images) {
        Ok(Element {
            built: Built::BuiltIn,
            notes,
        }) => assert!(notes[0].contains("Down Tap Note.lua:1: no"), "{notes:?}"),
        other => panic!("{other:?}"),
    }
    built(skin.element(Button::Down, "Explosion", &mut textures, &images));
    // A loop: the budget stops it.
    let folders = broken("Down Tap Note.lua", "while true do end");
    let mut skin = lua_skin(&folders, "b").unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        skin.element(Button::Down, "Tap Note", &mut textures, &images),
        Err(Failure::Deadline) | Err(Failure::Fuel)
    ));
    // Memory.
    let folders = broken(
        "Down Tap Note.lua",
        "local t = {} while true do t[#t + 1] = ('x'):rep(2^19) .. #t end",
    );
    let mut skin = lua_skin(&folders, "b").unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        skin.element(Button::Down, "Tap Note", &mut textures, &images),
        Err(Failure::Limit(_))
    ));
    // A NoteSkin.lua that fails is reported and skipped (the previous
    // table stays): the skin still loads.
    let folders = broken("NoteSkin.lua", "error('broken chain')");
    let mut skin = lua_skin(&folders, "b").unwrap_or_else(|e| panic!("{e}"));
    assert!(
        skin.warnings().iter().any(|w| w.contains("broken chain")),
        "{:?}",
        skin.warnings()
    );
    built(skin.element(Button::Down, "Tap Note", &mut textures, &images));
    // Too many actors.
    let folders = broken(
        "Down Tap Note.lua",
        "local t = {} for i = 1, 200 do local f = {} for j = 1, 100 do f[j] = Def.Actor {} end t[i] = Def.ActorFrame(f) end return Def.ActorFrame(t)",
    );
    let mut skin = lua_skin(&folders, "b").unwrap_or_else(|e| panic!("{e}"));
    match skin.element(Button::Down, "Tap Note", &mut textures, &images) {
        Err(Failure::Error(m)) => assert!(m.contains("more than 16384 actors"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn files_outside_the_skin_cannot_be_reached() {
    let folders = vec![folder(
        "dance",
        "x",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "assert(loadfile('../../../../etc/passwd') == nil)\n\
                 assert(loadfile('/NoteSkins/dance/other/NoteSkin.lua') == nil)\n\
                 assert(io == nil and os == nil and debug == nil and require == nil)\n\
                 local f = assert(loadfile('helper.lua'))\n\
                 return f(7)\n",
            ),
            (
                "helper.lua",
                "local n = ... return Def.Actor { Name = 'n' .. n }",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "x").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let a = built(skin.element(Button::Down, "Tap Note", &mut textures, &Sizes(vec![])));
    assert_eq!(a.name, "n7");
}

#[test]
fn actors_keep_fields_and_skins_add_methods() {
    let folders = vec![folder(
        "dance",
        "f",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            ("_n 4x1.png", ""),
            (
                "Down Tap Note.lua",
                "function Actor:twice(n) self:zoom(n * 2) return self end\n\
                 return Def.ActorFrame {\n\
                   InitCommand = function(self)\n\
                     self.count = 3\n\
                     local s = self:GetChild('s')\n\
                     s:twice(self.count)\n\
                     assert(s:GetParent() == self and self:GetChild('none') == nil)\n\
                     assert(s.count == nil and self.count == 3)\n\
                   end;\n\
                   Def.Sprite {\n\
                     Name = 's';\n\
                     Texture = '_n';\n\
                     InitCommand = function(self)\n\
                       self:SetStateProperties({ { Frame = 3, Delay = 1 }, { Frame = 1, Delay = 2 } })\n\
                     end;\n\
                   };\n\
                 }\n",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "f").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let images = Sizes(vec![("_n 4x1.png", (256, 64))]);
    let tap = built(skin.element(Button::Down, "Tap Note", &mut textures, &images));
    let s = &tap.children()[0];
    assert_eq!(s.current().scale[0], 6.0);
    let Kind::Sprite(sprite) = &s.kind else {
        panic!()
    };
    assert_eq!(sprite.states.len(), 2);
    assert_eq!(sprite.states[0].rect, [0.75, 0.0, 1.0, 1.0]);
    assert_eq!(sprite.states[1].delay, 2.0);
    assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
}

#[test]
fn hostile_skins_end_quickly() {
    let cases: &[(&str, &str)] = &[
        // Tables the converter walks without fuel.
        (
            "wide",
            "local t = Def.Sprite {} for i = 1, 150000 do t['k' .. i] = i end return t",
        ),
        (
            "frames",
            // Not `return Def.Sprite { … }`: a tail call, after which the
            // nearest skin file up the stack is common's NoteSkin.lua (as
            // with StepMania's `debug.getinfo` loop), and `_n` is not there.
            "local f = {} for i = 1, 100000 do f[i] = { Frame = 0 } end \
             local t = Def.Sprite { Texture = '_n', Frames = f } return t",
        ),
        (
            "deep",
            "local t = Def.Actor {} for i = 1, 100 do t = Def.ActorFrame { t } end return t",
        ),
        // Host functions with large arguments, in a loop.
        (
            "paths",
            "local s = ('x'):rep(2^20) while true do NOTESKIN:GetPath(s, s) end",
        ),
        (
            "metrics",
            "while true do NOTESKIN:GetMetricA('ReceptorArrow', 'InitCommand') end",
        ),
        // An element loading itself.
        (
            "itself",
            "return NOTESKIN:LoadActor(Var 'Button', Var 'Element')",
        ),
        ("colors", "while true do color(('1,'):rep(10000)) end"),
        // From the review of this code: tables and strings the converter
        // and the host would copy or walk per actor.
        (
            "names",
            "local t = Def.Actor { Name = ('n'):rep(2^20 - 64) } \
             local k = {} for i = 1, 4000 do k[i] = t end return Def.ActorFrame { children = k }",
        ),
        (
            "texture",
            "local t = Def.Sprite { Texture = '/' .. ('n/'):rep(2^19 - 64) } \
             local k = {} for i = 1, 4000 do k[i] = t end return Def.ActorFrame { children = k }",
        ),
        (
            "source",
            "local t = { Class = 'Actor', _Source = ('s'):rep(2^20) } \
             for i = 1, 40 do t['x' .. i .. 'Command'] = 1 end \
             local k = {} for i = 1, 4000 do k[i] = t end return Def.ActorFrame { children = k }",
        ),
        (
            "sparse",
            "local t = Def.Actor {} for i = 1, 1000000 do t[i] = true end \
             for i = 1, 1000000 do t[i] = nil end \
             local k = {} for i = 1, 4000 do k[i] = t end return Def.ActorFrame { children = k }",
        ),
        (
            "queue",
            "local s = ('q'):rep(2^20 - 64) local k = {} \
             for i = 1, 4000 do k[i] = Def.Actor { InitCommand = function(self) \
               for j = 1, 51 do self:queuecommand(s) end end } end \
             return Def.ActorFrame { children = k }",
        ),
        (
            "getpath-cache",
            "local s = ('b'):rep(1000) local i = 0 \
             while true do i = i + 1 NOTESKIN:GetPath(s .. i, 'e') end",
        ),
        (
            "index-cycle",
            "setmetatable(Actor, { __index = Sprite }) \
             local t = Def.Actor { InitCommand = function(self) local x = self.nothere end } \
             return t",
        ),
        ("cmd-deep", ""),
    ];
    let deep = format!("x = {}1{}", "cmd(a,".repeat(60_000), ")".repeat(60_000));
    for (name, source) in cases {
        let source = if *name == "cmd-deep" {
            deep.as_str()
        } else {
            source
        };
        let folders = vec![folder(
            "dance",
            "h",
            &[
                ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
                ("_n 4x1.png", ""),
                ("Down Tap Note.lua", source),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "h")
            .unwrap_or_else(|e| panic!("{e}"))
            .into_owned();
        let start = std::time::Instant::now();
        let mut lua = load(
            skin,
            Env::default(),
            LoadBudget {
                fuel: 20_000_000,
                seconds: 2.0,
            },
            Box::new(move || start.elapsed().as_secs_f64()),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut textures = Textures::default();
        let images = Sizes(vec![("_n 4x1.png", (256, 64))]);
        let r = lua.element(Button::Down, "Tap Note", &mut textures, &images);
        let took = start.elapsed();
        println!(
            "{name}: {:?} in {took:?}, {} bytes",
            r.as_ref()
                .map(|e| (&e.built, &e.notes))
                .map_err(|e| e.to_string()),
            lua.memory()
        );
        assert!(took.as_secs_f64() < 3.0, "{name}: {took:?}");
        assert!(lua.memory() <= 32 << 20, "{name}");
        // Some of these load (cut down, or as StepMania would); none may
        // take long or a lot of memory, which the watchdog checks.
        let _ = r;
    }
}

#[test]
fn tables_on_their_own_path_are_skipped() {
    // `XNodeFromTable` skips a table already on the way down.
    let folders = vec![folder(
        "dance",
        "c",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "local t = Def.ActorFrame { Def.Actor { Name = 'a' } } t[2] = t return t",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "c").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let tap = built(skin.element(Button::Down, "Tap Note", &mut textures, &Sizes(vec![])));
    assert_eq!(tap.children().len(), 1);
}

#[test]
fn children_of_one_name_and_failing_commands() {
    let folders = vec![folder(
        "dance",
        "d",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "local t = Def.ActorFrame {\n\
                   InitCommand = function(self)\n\
                     local k = self:GetChildren()\n\
                     assert(#k.x == 2 and k.y:GetName() == 'y')\n\
                     k.x:zoom(3)\n\
                     self:GetChild('x'):diffusealpha(0.5)\n\
                   end;\n\
                   Def.Actor { Name = 'x' };\n\
                   Def.Actor { Name = 'x' };\n\
                   Def.Actor { Name = 'y', InitCommand = function(self) error('bad') end };\n\
                   Def.Actor { Name = 'z', InitCommand = function(self) self:zoom(2) end };\n\
                 }\n\
                 return t\n",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "d").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let tap = built(skin.element(Button::Down, "Tap Note", &mut textures, &Sizes(vec![])));
    let c = tap.children();
    // Methods on several of one name go to the last one.
    assert_eq!(c[0].current().scale[0], 1.0);
    assert_eq!(c[1].current().scale[0], 3.0);
    assert_eq!(c[1].current().diffuse()[3], 0.5);
    // `y`'s Init failed and was reported; `z`'s still ran.
    assert_eq!(c[3].current().scale[0], 2.0);
    assert!(
        skin.warnings().iter().any(|w| w.contains("bad")),
        "{:?}",
        skin.warnings()
    );
}

/// A skin whose Lua lives during a play: an explosion with a message
/// command taking parameters and a round robin re-armed by
/// `queuecommand`, a receptor with an update function on the beat.
fn live() -> Vec<Folder> {
    vec![folder(
        "dance",
        "live",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Explosion.lua",
                "local n = 0\n\
                 return Def.ActorFrame {\n\
                   ColumnJudgmentMessageCommand = function(self, p)\n\
                     self:x(p.Column):y(p.Bright and 1 or 0)\n\
                     self.score = p.TapNoteScore\n\
                   end;\n\
                   W1Command = function(self)\n\
                     assert(self.score == 'TapNoteScore_W1')\n\
                     self:diffusealpha(0.5)\n\
                   end;\n\
                   Def.Actor {\n\
                     Name = 'emit';\n\
                     OnCommand = function(self) self:sleep(0.1):queuecommand('Emit') end;\n\
                     EmitCommand = function(self)\n\
                       n = n + 1\n\
                       self:y(n):sleep(0.1):queuecommand('Emit')\n\
                     end;\n\
                     HitMineCommand = function(self) error('boom') end;\n\
                   };\n\
                   LiftCommand = function(self, p) assert(p.Column == nil) end;\n\
                 }\n",
            ),
            (
                "Down Receptor.lua",
                "return Def.ActorFrame {\n\
                   InitCommand = function(self)\n\
                     self:SetUpdateFunction(function(s, dt)\n\
                       s.t = (s.t or 0) + dt\n\
                       s:x(s.t)\n\
                       local beat = GAMESTATE:GetSongPosition():GetSongBeat()\n\
                       s:GetChild('r'):y(beat)\n\
                       s:z(s:GetChild('inner').n)\n\
                     end)\n\
                   end;\n\
                   Def.Actor { Name = 'r' };\n\
                   Def.ActorFrame {\n\
                     Name = 'inner';\n\
                     InitCommand = function(self)\n\
                       self:SetUpdateFunction(function(s) s.n = (s.n or 0) + 1 end)\n\
                     end;\n\
                   };\n\
                 }\n",
            ),
            (
                "Down Tap Note.lua",
                "return Def.ActorFrame {\n\
                   InitCommand = function(self)\n\
                     self:SetUpdateFunction(function(s) while true do end end)\n\
                   end;\n\
                 }\n",
            ),
        ],
    )]
}

/// An element as the play holds it.
fn boxed(skin: &mut LuaSkin, element: &str) -> Box<ddi_actor::Actor> {
    let mut textures = Textures::default();
    let mut tree = Box::new(built(skin.element(
        Button::Down,
        element,
        &mut textures,
        &Sizes(vec![]),
    )));
    assert!(skin.reclaim(&mut tree));
    tree
}

#[test]
fn events_play_commands_with_their_parameters() {
    let mut skin = lua_skin(&live(), "live").unwrap_or_else(|e| panic!("{e}"));
    let mut ex = boxed(&mut skin, "Explosion");
    let id = ex.id;
    skin.begin_frame(Position::default(), FrameBudget::default());
    assert!(skin.lend(&mut ex));
    assert!(!skin.lend(&mut ex));
    let params = [
        ("TapNoteScore", Param::Str("TapNoteScore_W1".into())),
        ("Column", Param::Int(1)),
        ("Bright", Param::Bool(true)),
    ];
    skin.play(id, "ColumnJudgmentMessage", &params).unwrap();
    skin.play(id, "W1", &[]).unwrap();
    // A failing command is reported and the play goes on.
    skin.play(id, "HitMine", &[]).unwrap();
    // Commands the engine plays get an empty table, as StepMania's
    // messages always have one.
    skin.play(id, "Lift", &[]).unwrap();
    assert!(skin.reclaim(&mut ex));
    assert!(!skin.reclaim(&mut ex));
    assert_eq!(ex.current().pos[..2], [1.0, 1.0]);
    assert_eq!(ex.current().diffuse()[3], 0.5);
    assert!(
        skin.warnings().iter().any(|w| w.contains("boom")),
        "{:?}",
        skin.warnings()
    );
    assert!(
        !skin.warnings().iter().any(|w| w.contains("Lift")),
        "{:?}",
        skin.warnings()
    );
    assert!(skin.frame_fuel() > 0);
}

#[test]
fn queued_commands_and_update_functions_run_through_lua() {
    let mut skin = lua_skin(&live(), "live").unwrap_or_else(|e| panic!("{e}"));
    let mut ex = boxed(&mut skin, "Explosion");
    let mut receptor = boxed(&mut skin, "Receptor");
    let clocks = Clocks::default();
    for frame in 0..30 {
        let seconds = f64::from(frame) / 60.0;
        skin.begin_frame(
            Position {
                seconds,
                beat: seconds * 2.0,
            },
            FrameBudget::default(),
        );
        assert!(skin.lend(&mut ex) && skin.lend(&mut receptor));
        if frame == 0 {
            skin.play(ex.id, "On", &[]).unwrap();
        }
        skin.update(ex.id, 1.0 / 60.0, &clocks).unwrap();
        skin.update(receptor.id, 1.0 / 60.0, &clocks).unwrap();
        assert!(skin.reclaim(&mut ex) && skin.reclaim(&mut receptor));
    }
    // Half a second: `Emit` ran every 0.1 s, re-armed by itself, its
    // count kept in Lua.
    let emit = &ex.children()[0];
    assert!(
        (4.0..=5.0).contains(&emit.current().pos[1]),
        "{:?}",
        emit.current()
    );
    // The update function ran each frame with the seconds, after the
    // children's (the inner frame's count is already this frame's), and
    // read the frame's song position.
    assert!((receptor.current().pos[0] - 0.5).abs() < 1e-4);
    assert!((receptor.children()[0].current().pos[1] - 29.0 / 30.0).abs() < 1e-4);
    assert_eq!(receptor.current().pos[2], 30.0);
    assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
}

#[test]
fn frame_budget_is_renewed_after_long_idle_gaps() {
    let folders = vec![folder(
        "dance",
        "idle",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Receptor.lua",
                r#"local frames = 0
return Def.ActorFrame {
    InitCommand = function(self)
        self:SetUpdateFunction(function(s)
            local sum = 0
            for i = 1, 300 do sum = sum + i end
            frames = frames + 1
            s:x(sum):y(frames)
        end)
    end,
}
"#,
            ),
        ],
    )];
    let skin = Skin::load(&folders, "dance", "idle").unwrap().into_owned();
    let time = Rc::new(std::cell::Cell::new(0.0));
    let clock = time.clone();
    let mut skin = load(
        skin,
        Env::default(),
        LoadBudget::default(),
        Box::new(move || {
            let now = clock.get();
            clock.set(now + 0.000001);
            now
        }),
    )
    .unwrap();
    let mut receptor = boxed(&mut skin, "Receptor");
    let budget = FrameBudget {
        fuel: 10_000,
        seconds: 0.01,
    };
    let before = skin.fuel_used();
    for frame in 1..=128 {
        // A hidden window may resume hours later; idle time is not Lua work.
        time.set(time.get() + 3600.0);
        skin.begin_frame(Position::default(), budget);
        assert!(skin.lend(&mut receptor));
        skin.update(receptor.id, 1.0 / 60.0, &Clocks::default())
            .unwrap();
        assert!(skin.reclaim(&mut receptor));
        assert_eq!(receptor.current().pos[0], 45_150.0);
        assert_eq!(receptor.current().pos[1], frame as f32);
        assert!(skin.frame_fuel() <= budget.fuel);
    }
    assert!(skin.fuel_used() - before > budget.fuel);
    assert!(skin.failure().is_none());
}

#[test]
fn a_frame_out_of_fuel_ends_the_lua() {
    let mut skin = lua_skin(&live(), "live").unwrap_or_else(|e| panic!("{e}"));
    let mut tap = boxed(&mut skin, "Tap Note");
    let mut ex = boxed(&mut skin, "Explosion");
    skin.begin_frame(Position::default(), FrameBudget::default());
    assert!(skin.lend(&mut tap) && skin.lend(&mut ex));
    let clocks = Clocks::default();
    let start = std::time::Instant::now();
    assert!(matches!(
        skin.update(tap.id, 1.0 / 60.0, &clocks),
        Err(Failure::Fuel)
    ));
    assert!(start.elapsed().as_secs_f64() < 0.5);
    assert!(skin.frame_fuel() >= FRAME_FUEL);
    // Every later call, in this frame or the next, fails the same way;
    // the trees still go back.
    skin.begin_frame(Position::default(), FrameBudget::default());
    assert!(matches!(skin.play(ex.id, "On", &[]), Err(Failure::Fuel)));
    assert!(matches!(skin.failure(), Some(Failure::Fuel)));
    assert!(skin.reclaim(&mut tap) && skin.reclaim(&mut ex));
    assert_eq!(tap.children().len(), 0);
    assert!(!skin.lend(&mut Box::new(ddi_actor::Actor::new(Kind::Actor))));
}

#[test]
fn a_sliced_load_gives_the_host_turns() {
    use std::cell::Cell;
    let turns = Rc::new(Cell::new(0));
    let folders = vec![folder(
        "dance",
        "slow",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "local x = 0\nfor i = 1, 200000 do x = x + i end\nreturn Def.Actor { Name = tostring(x) }\n",
            ),
        ],
    )];
    let skin = Skin::load(&folders, "dance", "slow").unwrap().into_owned();
    let counted = turns.clone();
    let mut pause = move || -> Pin<Box<dyn Future<Output = ()>>> {
        counted.set(counted.get() + 1);
        Box::pin(std::future::ready(()))
    };
    let mut t = 0.0;
    let clock = Box::new(move || {
        t += 0.0001;
        t
    });
    let mut skin = now(LuaSkin::new_sliced(
        skin,
        Env::default(),
        Settings::default(),
        LoadBudget::default(),
        clock,
        Some(&mut pause),
    ))
    .unwrap();
    let mut textures = Textures::default();
    let tap = now(skin.element_sliced(
        Button::Down,
        "Tap Note",
        &mut textures,
        &Sizes(vec![]),
        Some(&mut pause),
    ))
    .unwrap();
    assert!(matches!(tap.built, Built::Actor(_)), "{tap:?}");
    // The loop costs well over a slice.
    assert!(turns.get() >= 2, "{}", turns.get());
}

#[test]
fn elements_reach_each_other_while_loading_but_not_once_held() {
    let folders = vec![folder(
        "dance",
        "x",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Receptor.lua",
                "return Def.ActorFrame { InitCommand = function(self) RECEPTOR = self end; Def.Sprite { Texture = 'r.png' } }\n",
            ),
            (
                "Down Explosion.lua",
                "return Def.ActorFrame {\n\
                   InitCommand = function(self) RECEPTOR:x(42) end;\n\
                   GoCommand = function(self) RECEPTOR:x(7) end;\n\
                   Def.Sprite { Texture = 'r.png' };\n\
                 }\n",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "x").unwrap_or_else(|e| panic!("{e}"));
    let images = Sizes(vec![("r.png", (64, 64))]);
    let mut textures = Textures::default();
    let mut receptor = Box::new(built(skin.element(
        Button::Down,
        "Receptor",
        &mut textures,
        &images,
    )));
    let mut ex = Box::new(built(skin.element(
        Button::Down,
        "Explosion",
        &mut textures,
        &images,
    )));
    assert!(skin.reclaim(&mut receptor) && skin.reclaim(&mut ex));
    // The explosion's Init moved the receptor the play now holds.
    assert_eq!(receptor.current().pos[0], 42.0);
    // While the play holds the receptor, Lua cannot reach it: the command
    // fails visibly instead of moving a copy.
    skin.begin_frame(Position::default(), FrameBudget::default());
    assert!(skin.lend(&mut ex));
    skin.play(ex.id, "Go", &[]).unwrap();
    assert!(skin.reclaim(&mut ex));
    assert_eq!(receptor.current().pos[0], 42.0);
    assert!(
        skin.warnings()
            .iter()
            .any(|w| w.contains("the actor is gone")),
        "{:?}",
        skin.warnings()
    );
}

#[test]
fn methods_that_walk_trees_pay_for_them() {
    let folders = vec![folder(
        "dance",
        "w",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Receptor.lua",
                "local t = Def.ActorFrame {\n\
                   InitCommand = function(self)\n\
                     self:SetUpdateFunction(function(s) for i = 1, 50 do s:hurrytweening(1) end end)\n\
                   end;\n\
                 }\n\
                 for i = 1, 4000 do\n\
                   t[#t + 1] = Def.Actor { InitCommand = function(self) for j = 1, 50 do self:linear(1) end end }\n\
                 end\n\
                 return t\n",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "w").unwrap_or_else(|e| panic!("{e}"));
    let mut receptor = boxed(&mut skin, "Receptor");
    skin.begin_frame(Position::default(), FrameBudget::default());
    assert!(skin.lend(&mut receptor));
    let start = std::time::Instant::now();
    // About 200,000 actors and tweens a call: one call spends the frame.
    assert!(matches!(
        skin.update(receptor.id, 0.0, &Clocks::default()),
        Err(Failure::Fuel)
    ));
    assert!(start.elapsed().as_secs_f64() < 0.1, "{:?}", start.elapsed());
    assert!(skin.reclaim(&mut receptor));
}

#[test]
fn a_host_limit_stays_a_failure() {
    let text =
        "local t = Def.ActorFrame {}\nfor i = 1, 20000 do t[#t + 1] = Def.Actor {} end\nreturn t\n";
    let folders = vec![folder(
        "dance",
        "big",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            ("Down Receptor.lua", text),
        ],
    )];
    let mut skin = lua_skin(&folders, "big").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    assert!(
        skin.element(Button::Down, "Receptor", &mut textures, &Sizes(vec![]))
            .is_err()
    );
    // Later loads and plays fail the same way, never as soft errors.
    assert!(
        skin.element(Button::Down, "Tap Note", &mut textures, &Sizes(vec![]))
            .is_err()
    );
    skin.begin_frame(Position::default(), FrameBudget::default());
    assert!(skin.play(1, "On", &[]).is_err());
}

#[test]
fn outfox_and_fallback_names_skins_read() {
    // Dake's calls: the field's direction, the game button and its key, the
    // theme's NoteField metrics and a judgement's color.
    let folders = vec![folder(
        "dance",
        "o",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "assert(Var 'GameButton' == Var 'Button')\n\
                 assert(INPUTFILTER:GameButtonToKeyMapped(Var 'GameButton', Var 'Player') == '')\n\
                 assert(THEME:GetMetric('NoteField', 'NoteFieldType') == '')\n\
                 assert(THEME:GetMetric('NoteField', 'ShowBoard') == false)\n\
                 assert(THEME:GetMetric('NoteField', 'Nothing') == nil)\n\
                 assert(THEME:HasMetric('NoteField', 'ShowBoard'))\n\
                 assert(not THEME:HasMetric('NoteField', 'Nothing'))\n\
                 local unknown = JudgmentLineToColor('JudgmentLine_Nothing')\n\
                 assert(unknown[1] == 1 and unknown[4] == 1)\n\
                 local c = JudgmentLineToColor('JudgmentLine_ProW2')\n\
                 local reversed = GAMESTATE:GetIsFieldReversed()\n\
                 return Def.Sprite {\n\
                   Texture = 'tap.png';\n\
                   InitCommand = function(self) self:diffuse(c):zoom(reversed and 2 or 1) end;\n\
                 }\n",
            ),
            ("tap.png", ""),
        ],
    )];
    for reversed in [false, true] {
        let skin = Skin::load(&folders, "dance", "o").unwrap().into_owned();
        let mut skin = load(
            skin,
            Env {
                reversed,
                ..Env::default()
            },
            LoadBudget::default(),
            Box::new(|| 0.0),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let images = Sizes(vec![("tap.png", (64, 64))]);
        let tap = built(skin.element(Button::Left, "Tap Note", &mut Textures::default(), &images));
        assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
        assert_eq!(tap.current().diffuse(), [1.0, 0.9, 0.35, 1.0]);
        assert_eq!(tap.current().scale[0], if reversed { 2.0 } else { 1.0 });
    }
}

#[test]
fn a_dance_skin_is_told_a_dance_style_on_every_layout() {
    use ddi_chart::Layout;
    let style = |id: &str| dance_style(&Layout::builtin(id).unwrap());
    assert_eq!(style("dance-single"), "single");
    assert_eq!(style("dance-solo"), "solo");
    assert_eq!(style("dance-double"), "double");
    assert_eq!(style("pump-single"), "single");
    assert_eq!(style("pump-halfdouble"), "double");
    assert_eq!(style("pump-double"), "double");
    assert_eq!(style("danoni-7"), "single");
    assert_eq!(Env::default().game, "dance");
}

#[test]
fn a_pump_skin_sees_its_game() {
    // For pump skins (not imported yet): the game and style reach the Lua.
    let folders = vec![folder(
        "dance",
        "p",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "assert(GAMESTATE:GetCurrentGame():GetName() == 'pump')\n\
                 local style = GAMESTATE:GetCurrentStyle()\n\
                 assert(style:GetStepsType() == 'StepsType_Pump_Halfdouble')\n\
                 assert(style:GetStyleType() == 'StyleType_OnePlayerTwoSides')\n\
                 assert(style:GetName() == 'halfdouble')\n\
                 assert(style:ColumnsPerPlayer() == 6)\n\
                 return Def.Sprite { Texture = 'tap.png' }\n",
            ),
            ("tap.png", ""),
        ],
    )];
    let skin = Skin::load(&folders, "dance", "p").unwrap().into_owned();
    let mut skin = load(
        skin,
        Env {
            game: "pump".into(),
            style: "halfdouble".into(),
            columns: 6,
            ..Env::default()
        },
        LoadBudget::default(),
        Box::new(|| 0.0),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let images = Sizes(vec![("tap.png", (64, 64))]);
    built(skin.element(Button::Down, "Tap Note", &mut Textures::default(), &images));
    assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
}

#[test]
fn notes_load_per_color_when_their_lua_reads_it() {
    // A lift loads the tap through NOTESKIN:LoadActor, which loads the
    // 4ths' (as ITGmania's does): the lift is the same in every color.
    let folders = vec![folder(
        "dance",
        "c",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "local x = ({ ['4th'] = 1, ['16th'] = 4 })[Var 'Color'] or 9\n\
                 return Def.Sprite { Texture = 'tap.png'; InitCommand = function(self) self:x(x) end }\n",
            ),
            (
                "Down Tap Lift.lua",
                "return Def.ActorFrame { NOTESKIN:LoadActor(Var 'Button', 'Tap Note') }\n",
            ),
            (
                "Down Tap Mine.lua",
                "return Def.Sprite { Texture = 'tap.png' }\n",
            ),
            ("tap.png", ""),
        ],
    )];
    let mut skin = lua_skin(&folders, "c").unwrap_or_else(|e| panic!("{e}"));
    let images = Sizes(vec![("tap.png", (64, 64))]);
    let mut textures = Textures::default();
    let mut load = |element: &str, color: &str| {
        let (e, read) = now(skin.element_colored_sliced(
            Button::Down,
            element,
            color,
            &mut textures,
            &images,
            None,
        ))
        .unwrap();
        (built(Ok(e)), read)
    };
    let (tap, read) = load("Tap Note", "16th");
    assert!(read);
    assert_eq!(tap.current().pos[0], 4.0);
    let (lift, read) = load("Tap Lift", "16th");
    assert!(!read);
    assert_eq!(lift.children()[0].current().pos[0], 1.0);
    let (_, read) = load("Tap Mine", "16th");
    assert!(!read);
    // The engine's own load is the 4ths'.
    let (tap, _) = load("Tap Note", NOTE_COLORS[0]);
    assert_eq!(tap.current().pos[0], 1.0);
}

#[test]
fn print_and_table_concat_take_any_values() {
    // `print` warns its arguments joined by tabs (through `table.concat`,
    // which takes strings and numbers only, as in Lua 5.1).
    let folders = vec![folder(
        "dance",
        "p",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "print(1, nil, true, 'a')\n\
                 assert(table.concat({ 1, 'b', 2.5 }, '-') == '1-b-2.5')\n\
                 assert(not pcall(table.concat, { {} }))\n\
                 return Def.Sprite { Texture = 'tap.png' }\n",
            ),
            ("tap.png", ""),
        ],
    )];
    let mut skin = lua_skin(&folders, "p").unwrap_or_else(|e| panic!("{e}"));
    let images = Sizes(vec![("tap.png", (64, 64))]);
    built(skin.element(Button::Down, "Tap Note", &mut Textures::default(), &images));
    assert_eq!(skin.warnings(), ["1\tnil\ttrue\ta"]);
}

#[test]
fn frames_sort_by_draw_order_and_lua_follows() {
    let folders = vec![folder(
        "dance",
        "d",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "local order = {}\n\
                 return Def.ActorFrame {\n\
                   InitCommand = function(self)\n\
                     -- Sorted before the frame's own Init.\n\
                     local k = self:GetChild('a'):GetParent()\n\
                     assert(k == self)\n\
                     self:GetChild('back'):x(7)\n\
                     self:playcommand('Mark')\n\
                     self:GetChild('late'):draworder(-10)\n\
                   end;\n\
                   Def.Quad { Name = 'a', InitCommand = function(self) self:draworder(5) end,\n\
                     MarkCommand = function(self) order[#order + 1] = 'a' end };\n\
                   Def.Actor { Name = 'late', MarkCommand = function(self) order[#order + 1] = 'late' end };\n\
                   Def.ActorFrame { Name = 'back', InitCommand = function(self) self:draworder(-1) end,\n\
                     MarkCommand = function(self) order[#order + 1] = 'back' end,\n\
                     Def.Actor { Name = 'deep', InitCommand = function(self) self:draworder(9) end } };\n\
                   SortedCommand = function(self)\n\
                     self:SortByDrawOrder()\n\
                     self:GetChild('back'):GetChild('deep'):y(3)\n\
                     assert(table.concat(order, ',') == 'back,late,a', table.concat(order, ','))\n\
                   end;\n\
                 }\n",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "d").unwrap_or_else(|e| panic!("{e}"));
    let mut tap = boxed(&mut skin, "Tap Note");
    let names = |a: &ddi_actor::Actor| -> Vec<String> {
        a.children().iter().map(|c| c.name.clone()).collect()
    };
    // `back` (-1) first, then `late` and `a` (0 and 5); `late`'s later
    // draworder waits for the next sort.
    assert_eq!(names(&tap), ["back", "late", "a"]);
    assert_eq!(tap.children()[0].current().pos[0], 7.0);
    // The quad is untextured, a pixel big.
    let Kind::Sprite(q) = &tap.children()[2].kind else {
        panic!()
    };
    assert_eq!((q.texture, q.size), (ddi_actor::UNTEXTURED, [1.0, 1.0]));
    let id = tap.id;
    skin.begin_frame(Position::default(), FrameBudget::default());
    assert!(skin.lend(&mut tap));
    skin.play(id, "Sorted", &[]).unwrap();
    assert!(skin.reclaim(&mut tap));
    assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
    assert_eq!(names(&tap), ["late", "back", "a"]);
    // Methods reach the actors where they went.
    assert_eq!(tap.children()[1].children()[0].current().pos[1], 3.0);
}

#[test]
fn corners_masks_and_shadows_from_lua() {
    let folders = vec![folder(
        "dance",
        "d",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            (
                "Down Tap Note.lua",
                "return Def.ActorFrame {\n\
                   InitCommand = function(self) self:fov(60):SetDrawByZPosition(true) end;\n\
                   Def.Quad { InitCommand = function(self) self:MaskSource(true) end };\n\
                   Def.Quad { InitCommand = function(self)\n\
                     self:setsize(64, 64):diffuse(color('1,0,0,1')):MaskDest()\n\
                       :diffusetopedge(color('1,1,1,0.05')):shadowlength(2)\n\
                   end };\n\
                 }\n",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "d").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let tap = built(skin.element(Button::Down, "Tap Note", &mut textures, &Sizes(vec![])));
    assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
    assert_eq!((tap.fov, tap.draw_by_z), (60.0, true));
    let [mask, dest] = tap.children() else {
        panic!()
    };
    assert_eq!(mask.blend, ddi_actor::Blend::NoEffect);
    assert!(mask.depth.write && mask.depth.clear);
    assert_eq!(dest.depth.test, ddi_actor::ZTest::WriteOnPass);
    assert_eq!(dest.shadow, [2.0, 2.0]);
    assert_eq!(
        dest.current().diffuse,
        [
            [1.0, 1.0, 1.0, 0.05],
            [1.0, 1.0, 1.0, 0.05],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0]
        ]
    );
}

#[test]
fn a_shifted_env_times_beats_as_the_shifted_song() {
    let env = Env {
        timing: ddi_chart::TimingMap::constant(120.0, 0.25),
        first_beat: 4.0,
        ..Env::default()
    };
    let played = env.clone().shifted(0.1);
    // A note 0.1 s later, as `Song::shift_notes(0.1)` moves it.
    let later = played.timing.seconds_at_beat(8.0) - env.timing.seconds_at_beat(8.0);
    assert!((later - 0.1).abs() < 1e-12, "{later}");
    // Beats are the chart's: the shift moves only when they sound.
    assert_eq!(played.first_beat, 4.0);
}

/// A song's folder with a background script (`Skin::song`).
fn song_folder(script: &str) -> Folder {
    Folder {
        game: "Pack".into(),
        name: "Song".into(),
        files: vec![
            "song.sm".into(),
            "bg.png".into(),
            "BG/default.lua".into(),
            "BG/star.png".into(),
        ],
        texts: vec![("BG/default.lua".into(), script.into())],
    }
}

fn song_lua(script: &str) -> LuaSkin {
    let env = Env {
        screen: [854.0, 480.0],
        ..Env::default()
    };
    load(
        Skin::song(song_folder(script)),
        env,
        LoadBudget::default(),
        Box::new(|| 0.0),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_songs_background_script_loads_from_its_folder() {
    let mut lua = song_lua(
        "local dir = GAMESTATE:GetCurrentSong():GetSongDir()\n\
         return Def.ActorFrame {\n\
           Def.Quad { InitCommand = cmd(FullScreen; diffuse, color(\"0,0,1,1\")) },\n\
           LoadActor(\"star.png\") .. {\n\
             InitCommand = cmd(Center),\n\
             OnCommand = cmd(linear, 2; zoom, 2),\n\
           },\n\
           Def.Sprite { Texture = dir .. \"bg.png\", InitCommand = cmd(scale_or_crop_background) },\n\
           Def.BitmapText { Text = \"hi\" },\n\
         }\n",
    );
    let mut textures = Textures::default();
    let images = Sizes(vec![("BG/star.png", (32, 32)), ("bg.png", (64, 48))]);
    let element = lua.actor_file(
        "/Songs/Pack/Song/BG",
        Some("#FFFFFF"),
        &mut textures,
        &images,
    );
    let mut tree = Box::new(built(element));
    // The `UpperLeft` effect's frame around the script's.
    let script = &tree.children()[0];
    let [quad, star, bg, text] = script.children() else {
        panic!("{:?}", script.children().len())
    };
    // `FullScreen` on StepMania's screen: 854 × 480, at its centre.
    assert_eq!(quad.current().pos[..2], [427.0, 240.0]);
    assert_eq!(quad.current().scale[..2], [854.0, 480.0]);
    assert_eq!(quad.current().diffuse()[..3], [0.0, 0.0, 1.0]);
    assert_eq!(star.current().pos[..2], [427.0, 240.0]);
    // Covering the screen: 64 × 48 zoomed to 854 wide.
    assert!((bg.current().scale[0] - 854.0 / 64.0).abs() < 1e-4);
    // Text, in the face songs' text is drawn in.
    match &text.kind {
        Kind::Text(t) => assert_eq!((t.text(), t.style()), ("hi", &song_text_style())),
        other => panic!("{other:?}"),
    }
    // Both images are the song's.
    assert!(
        textures
            .keys()
            .iter()
            .all(|k| matches!(k, TextureKey::Skin { .. }))
    );
    assert_eq!(textures.keys().len(), 2);
    // `On` starts the star's tween; a second in, it is halfway.
    assert!(lua.reclaim(&mut tree));
    lua.begin_frame(Position::default(), FrameBudget::default());
    assert!(lua.lend(&mut tree));
    let id = tree.id;
    lua.play(id, "On", &[]).unwrap();
    lua.update(id, 1.0, &Clocks::default()).unwrap();
    assert!(lua.reclaim(&mut tree));
    let star = &tree.children()[0].children()[1];
    assert!(
        (star.current().scale[0] - 1.5).abs() < 1e-4,
        "{:?}",
        star.current().scale
    );
    assert!(lua.warnings().is_empty(), "{:?}", lua.warnings());
}

#[test]
fn a_song_script_that_fails_loads_blank_and_one_that_loops_stops() {
    let mut lua = song_lua("error('no')");
    let mut textures = Textures::default();
    let e = lua
        .actor_file("/Songs/Pack/Song/BG", None, &mut textures, &Sizes(vec![]))
        .unwrap();
    assert_eq!(e.built, Built::Blank);
    assert!(e.notes.iter().any(|n| n.contains("no")), "{:?}", e.notes);
    let mut lua = song_lua("while true do end");
    let r = lua.actor_file("/Songs/Pack/Song/BG", None, &mut textures, &Sizes(vec![]));
    assert!(r.is_err(), "{r:?}");
    assert!(lua.failure().is_some());
}

#[test]
fn a_skins_lua_reads_its_parameters_and_cannot_change_them() {
    use crate::params::Value;
    // Our extension: `NOTESKIN:GetParameter` gives each declared
    // parameter's value (the default when unset, a stored value only as
    // the declaration takes it), nil for anything else, and a copy each
    // time.
    let folders = vec![folder(
        "dance",
        "p",
        &[
            (
                "metrics.ini",
                "[Global]\nFallbackNoteSkin=default\n\
                 [Parameters]\nList=Look,Boom,Size,Tint\n\
                 [Parameter Look]\nType=Choice\nChoices=Plain,Gold\n\
                 [Parameter Boom]\nType=Toggle\nDefault=1\n\
                 [Parameter Size]\nType=Number\nMin=1\nMax=2\nStep=0.5\nDefault=1\n\
                 [Parameter Tint]\nType=Color\nDefault=#FF0000\n",
            ),
            (
                "Down Tap Note.lua",
                "local look = NOTESKIN:GetParameter('Look')\n\
                 assert(NOTESKIN:GetParameter('Nothing') == nil)\n\
                 assert(NOTESKIN:GetParameter(nil) == nil)\n\
                 local tint = NOTESKIN:GetParameter('Tint')\n\
                 local red = tint[1]\n\
                 tint[1] = 9\n\
                 assert(NOTESKIN:GetParameter('Tint')[1] == red)\n\
                 local size = NOTESKIN:GetParameter('Size')\n\
                 local boom = NOTESKIN:GetParameter('Boom')\n\
                 return Def.Sprite {\n\
                   Texture = 'tap.png';\n\
                   InitCommand = function(self)\n\
                     self:zoom(size):diffuse(NOTESKIN:GetParameter('Tint'))\n\
                     self:visible(boom):rotationz(look == 'Gold' and 90 or 0)\n\
                   end;\n\
                 }\n",
            ),
            ("tap.png", ""),
        ],
    )];
    let tap = |params: &[(&str, Value)]| {
        let skin = Skin::load(&folders, "dance", "p").unwrap().into_owned();
        let mut skin = load(
            skin,
            Env {
                params: params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
                ..Env::default()
            },
            LoadBudget::default(),
            Box::new(|| 0.0),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let images = Sizes(vec![("tap.png", (64, 64))]);
        let tap = built(skin.element(Button::Down, "Tap Note", &mut Textures::default(), &images));
        assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
        tap
    };
    // The defaults.
    let plain = tap(&[]);
    assert_eq!(plain.current().scale[0], 1.0);
    assert_eq!(plain.current().diffuse(), [1.0, 0.0, 0.0, 1.0]);
    assert!(plain.visible);
    assert_eq!(plain.current().rotation[2], 0.0);
    // Set values, as their declarations take them; others ignored.
    let set = tap(&[
        ("Look", Value::Text("gold".into())),
        ("Boom", Value::Bool(false)),
        ("Size", Value::Number(1.6)),
        ("Tint", Value::Text("#00ff00".into())),
        ("Nothing", Value::Bool(true)),
    ]);
    assert_eq!(set.current().scale[0], 1.5);
    assert_eq!(set.current().diffuse(), [0.0, 1.0, 0.0, 1.0]);
    assert!(!set.visible);
    assert_eq!(set.current().rotation[2], 90.0);
    // A declaration left out is said with the Lua's warnings.
    let mut broken = folders.clone();
    broken[0].texts[0]
        .1
        .push_str("[Parameters]\nList=Look,Boom,Size,Tint,Odd\n");
    let skin = Skin::load(&broken, "dance", "p").unwrap().into_owned();
    let skin = load(
        skin,
        Env::default(),
        LoadBudget::default(),
        Box::new(|| 0.0),
    )
    .unwrap();
    assert_eq!(
        skin.warnings(),
        ["metrics.ini: parameter Odd: no [Parameter Odd] Type"]
    );
    // A value of the wrong kind is the default.
    let wrong = tap(&[
        ("Boom", Value::Text("no".into())),
        ("Size", Value::Bool(true)),
    ]);
    assert!(wrong.visible);
    assert_eq!(wrong.current().scale[0], 1.0);
}

#[test]
fn reading_parameters_costs_fuel_like_any_call() {
    // A script that reads a parameter forever ends on its fuel, as any
    // loop does.
    let folders = vec![folder(
        "dance",
        "q",
        &[
            (
                "metrics.ini",
                "[Global]\nFallbackNoteSkin=default\n[Parameters]\nList=Look\n\
                 [Parameter Look]\nType=Choice\nChoices=Plain\n",
            ),
            (
                "Down Tap Note.lua",
                "while true do NOTESKIN:GetParameter('Look') end\n",
            ),
        ],
    )];
    let skin = Skin::load(&folders, "dance", "q").unwrap().into_owned();
    let mut skin = load(
        skin,
        Env::default(),
        LoadBudget {
            fuel: 200_000,
            seconds: 10.0,
        },
        Box::new(|| 0.0),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let images = Sizes(vec![]);
    assert!(
        skin.element(Button::Down, "Tap Note", &mut Textures::default(), &images)
            .is_err()
    );
}

#[test]
fn a_songs_render_target_is_made_within_its_pixels_and_shown_by_a_sprite() {
    // A 64 × 32 target drawing a quad, a sprite showing it, and a second
    // target too large for the song's pixels.
    let mut lua = song_lua(
        "local aft\n\
         return Def.ActorFrame {\n\
           Def.ActorFrameTexture {\n\
             InitCommand = function(self)\n\
               aft = self\n\
               self:SetWidth(64):SetHeight(32):EnableAlphaBuffer(true):Create()\n\
             end,\n\
             Def.Quad { InitCommand = cmd(zoomto, 64, 32; xy, 32, 16) },\n\
           },\n\
           Def.Sprite {\n\
             OnCommand = function(self)\n\
               local t = aft:GetTexture()\n\
               self:SetTexture(t):xy(t:GetImageWidth(), t:GetImageHeight())\n\
             end,\n\
           },\n\
           Def.ActorFrameTexture {\n\
             InitCommand = cmd(SetWidth, 4096; SetHeight, 4096; Create),\n\
           },\n\
         }\n",
    );
    let mut textures = Textures::default();
    let element = lua.actor_file("/Songs/Pack/Song/BG", None, &mut textures, &Sizes(vec![]));
    let mut tree = Box::new(built(element));
    assert_eq!(lua.render_targets(), vec![[64, 32]]);
    let target = tree.children()[0].target.as_deref().unwrap();
    assert_eq!(
        (target.index, target.size, target.alpha),
        (Some(0), [64.0, 32.0], true)
    );
    // The second is past the song's pixels: reported, not made.
    assert_eq!(tree.children()[2].target.as_deref().unwrap().index, None);
    assert!(
        lua.warnings().iter().any(|w| w.contains("pixels")),
        "{:?}",
        lua.warnings()
    );
    assert!(lua.reclaim(&mut tree));
    lua.begin_frame(Position::default(), FrameBudget::default());
    assert!(lua.lend(&mut tree));
    lua.play(tree.id, "On", &[]).unwrap();
    assert!(lua.reclaim(&mut tree));
    let sprite = &tree.children()[1];
    let Kind::Sprite(s) = &sprite.kind else {
        panic!("{:?}", sprite.kind)
    };
    assert_eq!(s.texture, ddi_actor::target_texture(0));
    assert_eq!(s.size, [64.0, 32.0]);
    assert_eq!(sprite.current().pos[..2], [64.0, 32.0]);
    // Drawn: the target's quad in its pixels, the sprite on the screen.
    let mut out = Vec::new();
    tree.draw([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [1.0; 4], [0.0; 4], &mut out);
    let Some(ddi_actor::Drawn::Target(t)) = out.first() else {
        panic!("{out:?}")
    };
    assert_eq!((t.index, t.size), (0, [64, 32]));
    let ddi_actor::Drawn::Quad(q) = &t.drawn[0] else {
        panic!()
    };
    assert_eq!(q.transform, [64.0, 0.0, 0.0, 32.0, 32.0, 16.0]);
    assert!(
        matches!(&out[1], ddi_actor::Drawn::Quad(q) if q.texture == ddi_actor::target_texture(0))
    );
}

/// Images whose component files are read (`Def.Component`).
struct WithComponents(&'static str);

impl Images for WithComponents {
    fn size(&self, _dir: Dir, path: &str) -> Option<(u32, u32)> {
        path.contains('#').then_some((64, 64))
    }

    fn max_size(&self) -> u32 {
        8192
    }

    fn components(
        &self,
        _dir: Dir,
        _path: &str,
    ) -> Option<Result<Vec<crate::component::SvgComponent>, String>> {
        Some(crate::component::components(self.0))
    }
}

#[test]
fn an_element_can_be_a_component() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><defs>
        <symbol id="arrow" viewBox="-32 -32 64 64">
          <path id="body" d="M-24 -8 L24 -8 L0 24 Z" fill="#f00"/>
          <g id="charge" data-frames=""><rect width="4" height="4"/><rect width="4" height="4"/></g>
        </symbol></defs></svg>"##;
    let folders = vec![folder(
        "dance",
        "c",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            ("arrow.svg", svg),
            (
                "NoteSkin.lua",
                "local ret = ... or {}\n\
                 local oldLoad = ret.Load\n\
                 ret.Load = function()\n\
                   if Var 'Element' == 'Tap Note' then\n\
                     return Def.Component { File = 'arrow.svg', Id = 'arrow', Rotation = 90,\n\
                       Zoom = Bind.Curve { clock = 'press', keys = { 0, 0.75, 0.1, 1 }, ease = 'out', power = 0.5 },\n\
                       Pivot = { 0, 8 },\n\
                       Parts = { charge = { Frame = Bind.Steps { clock = 'note', length = 4, count = 2 },\n\
                                            Opacity = 0.5, Nope = 1, Tint = { '#ff0000', {0, 0, 1} } },\n\
                                 body = { Rotation = Bind.Random { min = 0, max = 90, salt = 2 } },\n\
                                 missing = { Opacity = 0 } } }\n\
                   end\n\
                   return oldLoad()\n\
                 end\n\
                 return ret\n",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "c").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let e = skin
        .element(
            Button::Down,
            "Tap Note",
            &mut textures,
            &WithComponents(svg),
        )
        .unwrap();
    let Built::Component(c) = e.built else {
        panic!("{e:?}")
    };
    assert_eq!(
        (c.id.as_str(), c.file.as_str(), c.rotation),
        ("arrow", "arrow.svg", 90.0)
    );
    assert_eq!(c.parts.len(), 2);
    assert_eq!(c.parts[1].textures.len(), 2, "a texture a frame");
    assert_eq!(c.parts[1].bindings.len(), 1);
    assert_eq!(c.parts[1].base[0], 0.5);
    // Two tints TintMix mixes; a random rotation by the instance's seed.
    assert_eq!(
        c.parts[1].tint,
        [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]
    );
    assert!(matches!(
        c.parts[0].bindings[0].function,
        crate::component::Function::Random { salt: 2, .. }
    ));
    let keys: Vec<_> = textures.keys().iter().map(|k| format!("{k:?}")).collect();
    assert!(
        keys.iter().any(|k| k.contains("arrow.svg#arrow/1/1")),
        "{keys:?}"
    );
    // The whole component's own animation, about its pivot; a power ease.
    assert_eq!(c.pivot, [0.0, 8.0]);
    assert_eq!(c.bindings.len(), 1);
    let clocks = crate::component::Clocks {
        press: Some(0.05),
        ..Default::default()
    };
    let zoom = 0.75 + 0.25 * (1.0 - 0.5_f32.powf(0.5));
    assert!((c.placement(&clocks).zoom - zoom).abs() < 1e-5);
    // What is wrong is said, the rest kept.
    assert!(
        e.notes.iter().any(|n| n.contains("no property Nope")),
        "{:?}",
        e.notes
    );
    assert!(
        e.notes.iter().any(|n| n.contains("no part \"missing\"")),
        "{:?}",
        e.notes
    );
    // A frame that could not be drawn keeps its place: the next frame is
    // still the second, and the first shows nothing.
    struct FirstFrameMissing(&'static str);
    impl Images for FirstFrameMissing {
        fn size(&self, dir: Dir, path: &str) -> Option<(u32, u32)> {
            (!path.ends_with("#arrow/1/0"))
                .then(|| WithComponents(self.0).size(dir, path))
                .flatten()
        }
        fn max_size(&self) -> u32 {
            8192
        }
        fn components(
            &self,
            dir: Dir,
            path: &str,
        ) -> Option<Result<Vec<crate::component::SvgComponent>, String>> {
            WithComponents(self.0).components(dir, path)
        }
    }
    let mut textures = Textures::default();
    let e = skin
        .element(
            Button::Down,
            "Tap Note",
            &mut textures,
            &FirstFrameMissing(svg),
        )
        .unwrap();
    let Built::Component(c) = e.built else {
        panic!("{e:?}")
    };
    let keys = textures.keys();
    let path = |t: u32| format!("{:?}", keys[t as usize]);
    assert_eq!(c.parts[1].textures.len(), 2, "{:?}", c.parts[1].textures);
    assert!(path(c.parts[1].textures[1]).contains("#arrow/1/1"));
    assert!(
        e.notes
            .iter()
            .any(|n| n.contains("#arrow/1/0 could not be drawn")),
        "{:?}",
        e.notes
    );
}

#[test]
fn a_long_notes_layers_say_what_they_ignore() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><defs>
        <symbol id="body" viewBox="-16 0 32 16">
          <rect id="band" data-role="band" width="24" height="16"/>
          <rect id="pulse" data-role="pulse" width="24" height="16"/>
        </symbol></defs></svg>"##;
    let folders = vec![folder(
        "dance",
        "l",
        &[
            ("metrics.ini", "[Global]\nFallbackNoteSkin=default\n"),
            ("hold.svg", svg),
            (
                "NoteSkin.lua",
                "local ret = ... or {}\n\
                 local oldLoad = ret.Load\n\
                 ret.Load = function()\n\
                   if Var 'Element' == 'Hold Body Inactive' then\n\
                     return Def.LongNote { File = 'hold.svg', Body = 'body',\n\
                       Parts = { band = { X = 2, Frame = 0 }, pulse = { Opacity = 0.5 } } }\n\
                   end\n\
                   return oldLoad()\n\
                 end\n\
                 return ret\n",
            ),
        ],
    )];
    let mut skin = lua_skin(&folders, "l").unwrap_or_else(|e| panic!("{e}"));
    let mut textures = Textures::default();
    let e = skin
        .element(
            Button::Down,
            "Hold Body Inactive",
            &mut textures,
            &WithComponents(svg),
        )
        .unwrap();
    assert!(matches!(e.built, Built::LongNote(_)), "{e:?}");
    // The band's X is said; the pulse's Opacity is taken.
    assert!(
        e.notes
            .iter()
            .any(|n| n.contains("band: X has no effect on a long note's body")),
        "{:?}",
        e.notes
    );
    assert!(
        !e.notes.iter().any(|n| n.contains("pulse:")),
        "{:?}",
        e.notes
    );
}
