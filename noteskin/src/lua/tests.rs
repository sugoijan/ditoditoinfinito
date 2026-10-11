//! Loading skins' Lua: the stand-ins' own scripts, a skin's `NoteSkin.lua`
//! and elements, metrics commands as Lua, and failures.

use super::*;
use crate::actor::{Clocks, Kind};
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

fn built(e: Result<Element, Failure>) -> crate::actor::Actor {
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
    assert_eq!(row.children()[3].current().diffuse[3], 0.0);
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
    assert_eq!(overlay.current().diffuse[3], 0.0);
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
    assert_eq!(tap.current().diffuse, [1.0, 0.0, 0.0, 1.0]);
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
    let mut p = crate::actor::Problems::default();
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
        "local t = {} for i = 1, 5000 do t[i] = Def.Actor {} end return Def.ActorFrame(t)",
    );
    let mut skin = lua_skin(&folders, "b").unwrap_or_else(|e| panic!("{e}"));
    match skin.element(Button::Down, "Tap Note", &mut textures, &images) {
        Err(Failure::Error(m)) => assert!(m.contains("more than 4096 actors"), "{m}"),
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
    assert_eq!(c[1].current().diffuse[3], 0.5);
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
fn boxed(skin: &mut LuaSkin, element: &str) -> Box<crate::actor::Actor> {
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
    assert_eq!(ex.current().diffuse[3], 0.5);
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
    assert!(!skin.lend(&mut Box::new(crate::actor::Actor::new(Kind::Actor))));
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
        "local t = Def.ActorFrame {}\nfor i = 1, 5000 do t[#t + 1] = Def.Actor {} end\nreturn t\n";
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
