# Infinito's wave as its own material (2026-10-30)

Decided with the maintainer: Infinito's hold, roll and lift wave is drawn
by Infinito's own WGSL material, as its taps, receptors and effects are,
and the engine's long-note wave goes outright: the `HoldMaterial*`,
`RollMaterial*`, `RollPulse*` and `Lift*` metric family, the paths that
read it and the built-in `HOLD_WAVE` material. Infinito looks the same
pixel for pixel. `noteskin-authoring.md` documents only the material way.

## What exists

The move itself is already made. Infinito has drawn its long notes with
its own `_spiral wave.wgsl` since it was introduced (e2e621a5,
2026-10-25): every hold and roll body element, active and inactive, on
all ten buttons, returns `Def.LongNote` (`_spiral hold.lua`), and every
lift `Def.Lift` (`_spiral lift.lua`), which
`spiral_long_notes_and_lifts_use_roles_and_the_active_roll_window`
(`crates/noteskin/tests/bundled.rs:959`) checks, with the onigiri on and
off. The renderer draws those and `continue`s before the metric paths
(`crates/render/src/noteskin.rs:2340-2409` for long notes,
`:2595-2601` for lifts). The metrics Infinito still sets are read and
never used: what is left is dead code and 32 dead lines.

The metric mechanism:

- `crates/noteskin/src/metrics.rs:102-222`: `HoldMaterial` (opt-in
  `HoldStencilMaterial`, `<prefix>Material{Pitch,Period,Angle,Phase,Speed,
  Wave,WaveWidth,WaveLength,Green,Lime,Highlight}`), `RollMaterial`
  (`RollStencilMaterial`, the `Roll` prefix, `RollPulse{Duty,Rest,
  Brightness}`, `opacity`), `LiftMaterial` (`LiftStencilMaterial`,
  `LiftPreparationSeconds`, `LiftTrailFade`, `LiftTrailOpacity`,
  `LiftBandWhite`, the `Hold` prefix's wave), and `bounded`, used by them
  alone; `NoteDisplay::{lift,hold,roll}_material` (`:228-230`), read at
  `:293-295`, `None` for legacy skins at `:360-362`; tests `:442-500`.
- `crates/render/src/material.rs:8-9, 22-24, 31-32, 59-60, 353-356`:
  `HOLD_WAVE` (id 0), `HOLD_WAVE_SOURCE`, its compile test;
  `crates/render/src/materials/hold_wave.wgsl`, the directory's only
  file; registered first in `TexturedPipeline::new`
  (`crates/render/src/textured.rs:555-557`, doc `:392`).
- `crates/render/src/noteskin/hold_material.rs`: `Fill::Wave` (`:55-56`),
  `HoldLayers::of_sprites` (`:89-130`, sprite-state indices as roles),
  `wave_params`, `wave_phase` (`:203-224`), `wave_base`, `wave_quad`
  (`:268-304`), the `Fill::Wave` arm of `material_hold` (`:489-503`),
  `head_phase` (`:12-16`), the module doc (`:1-6`), the test `:575-594`.
- `crates/render/src/noteskin.rs:2411-2470`: the sprite-sheet hold path
  (`m.hold_material`/`m.roll_material`, the pulse from `RollMaterial::
  opacity`); `:2633-2663`: the lift's wave slot and `material_lift`; test
  `material_roll_shadows_and_charge_follow_the_moving_body` (`:4538-4689`,
  through `of_sprites`).
- `crates/render/src/noteskin/lift_material.rs:6-20, 192-380`: `layer`,
  `ImageLayer`, `lead_length`, `material_lift` (the `LiftTrail`,
  `LiftBand`, `LiftCharge`, `LiftColors` children); its test (`:382-418`)
  builds a `LiftMaterial` to test `lead`.

No other user: no other bundled skin sets these keys, none falls back on
Infinito, and neither `../skincraft` nor `../skincraft-mcp` references the
metrics, `NoteDisplay`'s fields or `HOLD_WAVE` (grepped without
`target/`; one SkinCraft project snapshot under its untracked `.local/`
holds a copy of Infinito's old `metrics.ini` text, which then loads with
the keys ignored). `../ddi-scene` and `../ddi-svg` are stale worktrees.

## How the material gets its parameters

`Def.Material {File, Space="lane"|"note", Origin, Params, Images}` on a
`Def.LongNote` (its band's fill), a `Def.Lift` or a part: `Params` is up
to eight `vec4`s (a hex color is its RGB), `MaterialParams`
(`material.rs:20`), deduplicated into one of 64 slots a frame
(`NoteskinPlay::material_slot`, `noteskin.rs:1441`) and read as
`m.params[i]`; the `Phase` binding is `m.phase`, `m.seconds` and `m.beat`
are the frame's. A let-go hold greys by the quad's tint
(`hold_material.rs:513`), not the built-in's `value.x`. The skin's WGSL is
checked at load by `material::compile` for WebGL2 (naga, no loops or
bindings, GLSL ES 3.0 written); one refused is said in the skin's notes.

Every metric value is already in Infinito's Lua, as material parameters
where hold and roll differ (one file serves both), so nothing moves into
WGSL constants:

| Metric (hold / roll) | Value | Now |
| --- | --- | --- |
| `*StencilMaterial=1` | | the element returns `Def.LongNote` / `Def.Lift` |
| `*MaterialPitch` | 12.5 / 25 | `Def.LongNote` `Pitch` |
| `*MaterialAngle` | 60° / 11° | `params[0].xy`, its sine and cosine |
| `*MaterialPeriod`, `*MaterialWave` | 121, 31 / 171, 31.5 | `params[0].zw` |
| `*MaterialWaveWidth`, `*WaveLength` | 209, 206 | `params[1].xy` |
| `*MaterialGreen`, `Lime`, `Highlight` | hex | `params[2..4]` |
| `*MaterialPhase`, `*Speed` | 0.7, 1 | `Phase=Bind.Linear {clock="seconds", length=1, from=0.7, to=1.7}` |
| `RollPulseDuty`, `Rest`, `Brightness` | 0.25, 0.08, 0.85 | `body-pulse`/`cap-pulse` `Opacity=Bind.Curve` keys |
| `LiftPreparationSeconds`, `LiftTrailFade`, `LiftTrailOpacity` | 0.28, 2, 1 | `Def.Lift` `Seconds`, `Fade`, `Opacity` |
| `LiftBandWhite` | 0.7 | `head-band`'s `wave(0.7)`, `params[5].x` |

Infinito's wave also has what the built-in never had: `params[6]`, a slow
evolution (0.08 turns a second) and a second sine's mix (0.22). That is
why the built-in could not simply be deleted from under Infinito's
metrics: it is not what Infinito shows.

## What changes

Infinito (`assets/noteskins/dance/Infinito/`):

- `metrics.ini`: lines 54-85 (`LiftStencilMaterial` through
  `HoldMaterialHighlight`) go; `[NoteDisplay]` keeps
  `AnimationIsBeatBased` and what follows. The Lua, SVG and WGSL do not
  change. `README.md` already describes the material way;
  `PROVENANCE.toml` (`ours = true`, MIT) and `REUSE.toml` (the `**` MIT
  catch-all; Infinito has no annotation of its own) need nothing.
- `gen-noteskins` writes `target/noteskins/index.json` (the metrics' text,
  every file's size and hash), never committed; Trunk, `just test` and CI
  regenerate it, so after the change it is run locally only before the
  desktop's tests if not through `just test`.

Code that goes: everything listed under "What exists" but the shared
stencil drawing (`HoldLayers`, `of_long_note`, `material_hold`,
`fill_quad`, `component_lift`, `lead`), which `Def.LongNote` and
`Def.Lift` keep using. `Fill` keeps `Skin` and `Nothing`. The sprite-sheet
long notes go with the wave: without it they have no fill, and their
roles by sprite-state index are what `Def.LongNote`'s named roles
replaced. `material.rs`'s doc says the engine has no paints of its own.
Skin materials take ids from 0 (`LOCAL` maps them by name; the only
literal `material: 0`, `textured.rs:1429`, is the instance placeholder of
a quad without a material). Every shell compiles one shader fewer at boot.

## Steps

Each commit builds, passes `just test` and leaves Infinito's pictures
identical.

1. **Infinito's dead metrics.** Delete `metrics.ini:54-85`. Asset only:
   the before/after pictures below must be byte-identical, which proves
   the paths were dead for Infinito before any code goes. Add this plan
   and its line in `docs/plans/README.md`.
2. **The renderer's metric paths.** Remove the hold branch
   (`noteskin.rs:2411-2470`, falling through to the legacy path),
   the lift's wave slot and `material_lift` (`:2633-2663` becomes the
   plain `note_actor` call), `material_lift`, `layer`, `ImageLayer`,
   `lead_length`, `Fill::Wave`, `of_sprites`, `wave_*`, `head_phase`,
   `HOLD_WAVE`, `HOLD_WAVE_SOURCE`, `materials/hold_wave.wgsl` and its
   registration. Tests: `of_sprites` becomes a `#[cfg(test)]` helper
   taking `pitch` and a `Fill` (a `Fill::Skin` with a stub quad), so
   `material_roll_shadows_and_charge_follow_the_moving_body` keeps
   checking the shared stencil geometry (lengths 32-173, both directions,
   the join's crops); the lift test calls `lead(0.28, motion)`; the wave
   test goes. Replace `the_hold_wave_compiles` with a test compiling
   Infinito's bundled `_spiral wave.wgsl`, `_spiral material.wgsl` and
   `_spiral effect.wgsl` through `material::compile` (`include_str!` from
   `CARGO_MANIFEST_DIR/../../assets/...`), so the WebGL2 check covers
   the wave that ships. `NoteDisplay` still has its (unread, public)
   fields, so nothing warns.
3. **The metric family.** Remove `HoldMaterial`, `RollMaterial`,
   `LiftMaterial`, `bounded`, the three fields, their reads and defaults
   and tests `metrics.rs:442-500`. In the same commit (AGENTS.md's rule):
   AGENTS.md's `crates/noteskin/` entry drops the `LiftStencilMaterial`
   and `HoldStencilMaterial`/`RollStencilMaterial` sentences (keeping
   `Frame.note_motion`, now for `Def.Lift`'s lead) and says Infinito's
   holds, rolls and lifts are drawn by `Def.LongNote`/`Def.Lift` with its
   wave material; `noteskin-authoring.md` replaces "Continuous masked hold
   material", "Continuous roll materials", the masked-path paragraphs and
   "Leading lift material" with one section on `Def.LongNote` and
   `Def.Lift`: roles (`band`, `envelope`, `shadow`, `overlap-shadow`,
   `pulse`, `protect`, `outside`, `trail`), `Pitch`, `Join`, `Anchor`,
   `Seconds`/`Fade`/`Opacity`, `Def.Material` and its `Params`, the pulse
   as an `Opacity` binding on the `roll` clock, Infinito's files as the
   example; `noteskin-scene.md` gets a dated line under step 4 (the
   metrics path and `HOLD_WAVE` removed), its history otherwise kept; the
   same one line in `noteskin-svg.md`'s opening. Then check SkinCraft
   against this checkout with its local patch (`noteskin-authoring.md`'s
   rule for shared APIs); it needs no change.

## Checks

- Pictures, before step 1 and after each step, at the same `DDI_HOME`
  (`target/scratch/infinito-wave/home`, under `target/scratch/wd.sh`),
  saved in the scratch folder:
  `cargo run -p ddi-desktop --no-default-features --example
  hold_reference -- OUT.png` for holds, with `DDI_REFERENCE_ROLL=1`,
  `DDI_REFERENCE_ACTIVE=1` and `DDI_HOLD_LENGTH` 32 and 173; and
  `noteskin_cost -- Infinito OUT.png` (`--release`; a dense field of
  holds, rolls and lifts in ten lanes), also with `DDI_COST_REVERSE=1`
  and `DDI_COST_ONLY=long`. Compared with an exact pixel diff (`compare
  -metric AE` or a few lines of Pillow): zero pixels differ, since
  Infinito's quads and shaders do not change. `noteskin_cost`'s quads,
  runs and clear pixels must match too.
- `just test`: `crates/noteskin/tests/bundled.rs` (Infinito's long notes
  and lifts are components with the wave's parameters), and on desktop
  `long_note_skin.rs`, `component_skin.rs`, `material_art.rs`
  (`DDI_OFFSCREEN_PPM=<dir>` keeps its pictures) and
  `field_skin_offscreen.rs` (Infinito read for preview and play). These
  use synthetic skins but the same `material_hold` and `component_lift`.
- `cargo clippy` on the workspace and for wasm; the web through the
  verify skill in Chrome (WebGPU and WebGL2) and Firefox (WebGL2): a play
  of holds, rolls and lifts with Infinito, no shader or pipeline error,
  since skin material ids now start at 0.

## Risks

- A third-party skin setting these keys (none known: our extension, used
  only by Infinito and the unbundled Spiral Base) loads with them
  ignored and its holds and lifts drawn as legacy actors. Accepted.
- Losing coverage of the shared stencil drawing with `of_sprites`: kept
  by the test helper in step 2.
- Ids shifting by one: checked by the web run above; nothing else names
  an id.

## Left

- Renaming `hold_material.rs` and `lift_material.rs` (now the component
  long notes and lifts) is churn for another time.
- SkinCraft's pin moves to the new revision whenever it next updates.
