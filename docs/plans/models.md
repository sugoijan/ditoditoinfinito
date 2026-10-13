# Implementation plan: noteskins' 3D models (2026-10-12)

Draw the MilkShape 3D ASCII models skins use for taps, heads, lifts, fakes
and mines (`docs/plans/noteskins.md`, step 8; 65 % of the skins players
use, every classic ITG one, and Peter Lawrence's CF skins, which can then
be bundled: `docs/plans/bundled-skins.md`). Until now such an element is
drawn by the built-in skin and reported as "a 3D model".

Sources: StepMania 5.1 (`5_1-new` @ `825467b`, MIT-style; read to learn
its behaviour, cited below, our code our own): `src/RageModelGeometry.cpp`,
`src/Model.cpp`, `src/ModelTypes.cpp`, `src/NoteDisplay.cpp`,
`src/Player.cpp`. A survey of 53 model skins (StepMania 5.0 and 5.1,
ITGmania, OutFox, Peter Lawrence's): every one reaches its models through
`Def.Model { Meshes, Materials, Bones }` in Lua; the median model has 89
vertices, 60 triangles, 2 meshes and 2 materials (at most 1,284 vertices,
5 meshes).

## What skins need (the survey)

| Feature | Skins (of 53) |
|---|---|
| Several meshes, each with its material | 48 |
| Textures scrolled by `.ini` `TexVelocity`, driven by the note's beat | 45 |
| Vertex flags masking the colour translation (u, v) | 44 |
| Colour by texture translation (`NoteColorTextureCoordSpacing`) | 42 |
| One rigid bone with key frames (ITG's spinning mine) | 32 |
| UVs past 0..1 (repeat wrapping) | 11 |
| Multi-frame `.ini` textures | 1 |
| Lighting, alpha maps, sphere maps, cel shading, skinning | 0 |

## How StepMania draws a model

- **Units and axes**: a model is in screen pixels (about ±30 for a 64 px
  arrow), MilkShape's y up (`Scale(1,-1,1)` before drawing); the actor's
  transform (`zoom` sets x, y and z; rotation `Rx(-x)·Ry(-y)·Rz(z)`)
  then applies in 3D.
- **Projection**: the note field is drawn in perspective, 45°, the camera
  on +z at `(W/2)/tan 22.5°` so the z = 0 plane maps 1:1, the vanishing
  point at the field's centre (`Player.cpp:1686-1690`). So a model and x/y
  rotations show depth; sprites at z = 0 look as in 2D.
- **Depth**: the z-buffer is on for models and back faces are culled, but
  it is cleared after every note (`NoteDisplay.cpp:625-628`): a model
  hides only its own faces; notes overlap by draw order.
- **Colour**: unlit; a mesh's colour is its material's ambient + diffuse
  + emissive (clamped), times the actor's diffuse; the texture modulates
  it; a glow pass (texture alpha in the glow colour) follows.
- **Textures**: relative to the materials file; an `.ini` is an animated
  texture (`[AnimatedTexture]`: frames and delays, `TexVelocityX/Y`,
  `TexOffsetX/Y`); its translation is velocity × the fraction of its
  animation + offset, the fraction from the note (`SetSecondsIntoAnimation`
  as for sprites: beats, vivid). Repeat wrapping. The note's colour
  translation applies to the whole model, except per vertex where flag 1
  (u) or 2 (v) masks it (`TextureMatrixScale`).
- **Bones**: key frames at 30 fps, position lerped and rotation slerped,
  looping, every model playing its "default" animation from load.

## Step 1. Parsing (commit 1)

`noteskin/src/model.rs`: the MilkShape ASCII meshes, materials and bones
as StepMania reads them (line rules, limits, the quirks that change what
is drawn), and `[AnimatedTexture]` `.ini` files. Pure, natively tested
on synthetic files (committed tests write their own; third-party models
stay local) and on the local corpus (`local_skins`). Imports and the
bundled index keep a skin's `.txt` and `.ini` texts with its record (1 MB
each at most, as other texts).

**Done 2026-10-12.** `noteskin/src/model.rs`: meshes, materials, bones
and animated textures, parsed as StepMania does (comment and header rules,
quoted names up to 31 characters, a byte for the material index with -1
as none, vertex flags 1 and 2 as the colour mask, a mesh following one
bone as a whole, texture paths with `\` and `..` cleaned), every count
checked against the lines left and at most 65,535 vertices; tested on a
synthetic model. All 338 model files of the local corpora (StepMania 5.0
and 5.1, ITGmania, OutFox, Peter Lawrence's) parse, with the same mesh,
vertex, triangle, material and bone counts as an independent survey's.
`kept_text` keeps `.ini` and `.txt` files (imports, the bundled index,
`local_skins`), and a skin stored before gets them from its stored files
(`add_missing_texts`, was `add_lua_texts`). Not done: vertex flag 4 (UVs
from position), which no skin sets.

## Step 2. The `Model` actor (commit 2)

`Kind::Model` in `noteskin/src/actor.rs`: its meshes (shared), materials
(texture ids, scroll), vertex flags and animation state; built from a
model file (`build.rs`) and from `Def.Model` (`lua/convert.rs`), with the
methods skins call (`playanimation`, `rate`, `loop`, `position`,
`SetState`, `GetNumStates`). Drawing carries a 3D transform down the
tree and outputs, beside quads, a model's triangles in 3D (screen pixels
before the projection). Textures are found and decoded like a sprite's.

**Done 2026-10-12, with step 3 (one commit: neither is any use alone).**
`Kind::Model` holds the shared geometry and animation, each material's
colour (unlit: ambient + diffuse + emissive, clamped only after the
actor's diffuse multiplies it, as a vertex colour is), frames and
animated texture, and the play state; built from a `.txt` element and
from `Def.Model` (paths relative to the file that wrote it, the three
files charged to the load's budget by their lines), with `playanimation`,
`SetDefaultAnimation`, `rate`, `loop`, `position`, `setstate`,
`SetSecondsIntoAnimation`, `GetNumStates` and
`GetAnimationLengthSeconds`. Drawing carries a 3D transform down the tree
(`T(pos)·Rx(-x)·Ry(-y)·Rz(z)·S·T(align)`, `zoom` setting x, y and z)
and outputs `Drawn::Model` beside quads. All 58 model skins of the local
corpora load through their Lua with every element an actor.

## Step 3. Drawing models (commit 3)

On the CPU, per note: the actor's 3D transform, the field's perspective
(45°, vanishing point at the field's centre), back faces culled and the
model's triangles sorted far to near (its own depth only, as StepMania's
cleared z-buffer gives), then drawn as textured triangles in the skin's
passes (`render/src/textured.rs`: a triangle list beside the instanced
quads, the same blends, the plain view on WebGPU), with the colour
translation masked per vertex, repeat wrapping and the glow pass. No
depth buffer: a few hundred triangles a note are cheap to sort, and the
renderer's passes stay as they are. Checked by screenshot against
StepMania's look (midi-note-3d, ITG's cel and metal, Peter's skins, local
only) and with a synthetic model skin committed for tests.

**Done 2026-10-12.** `render/src/model.rs`: the field's camera (vanishing
point at the field's centre, 1030/64 arrows away), each vertex projected
with its `w`, triangles with a corner behind the near plane or past the
far one dropped, back faces culled by their winding as seen (MilkShape's
counter-clockwise faces the viewer: true of the open models of the
corpus), the rest sorted far to near. `render/src/textured.rs`: an
instance is a quad or a triangle (its three corners, texture
coordinates and `w`, so the texture is drawn in perspective), and a
shape may be untextured (a mesh without a texture: its colour alone),
through the same batches, blends and passes. The note's colour and the
material's scroll move every vertex's texture coordinates but along the
axes it fixes (StepMania's texture-matrix shader); repeat wrapping; the
glow pass after every mesh's diffuse one; a mesh without a material is
StepMania's grey. A note's model gets its seconds into the animation from
the beat as a sprite does (vivid, beat or time based). Tested natively
(projection, culling, sorting, masks; a Lua skin's model tap drawn,
turned away, tilted) and by screenshot: StepMania's midi-note-3d,
ITGmania's cel and metal, Peter Lawrence's CF_CHROME and CF_VIBRANT in
headless Chrome on WebGPU and WebGL2 and in Firefox, all top tier.
Reviewed independently (steps 1–3); fixed from it: numbers read in one
pass (a megabyte-long field took quadratic time), at most 16,384
triangles a model and 1,024 bones (bones found by name through a map),
reservations no larger than 256 items ahead of a count, at most 262,144
model triangles a frame and no more instances than the device's largest
buffer holds, `zoom` setting z too (`SetZoom`; this plan had it wrong),
models animating while paused, `SetSecondsIntoAnimation` releasing a
`setstate`, each material wrapping time by its own length, the blend set
back to normal after the first mesh (`DrawPrimitives`), triangles with a
corner that is not a number dropped, and an import keeping at most 32 MB
of a skin's texts. Not done: mipmaps for model textures (StepMania makes
them).

## Step 4. Bones and animated textures (commit 4)

Rigid bones (a mesh or its vertices bound to one bone) with key frames,
as ITG's mine spins; multi-frame `.ini` textures. Full skinning only if a
skin turns up that needs it.

**Done 2026-10-12.** `noteskin/src/model.rs`: each bone at rest
(`RageMatrixAngles` and its position, under its parent) and posed at a
frame as `SetBones` does it (the keys either side, positions lerped,
rotations as StepMania's quaternions slerped; past the last key the last
holds, a bone without keys stays at rest, not under its parent; a parent
listed after its child counts as it was), StepMania's row-major matrices
being this crate's column-major ones. A mesh that follows one bone is drawn
under it; a vertex that follows its own is made relative to its bone at
load and moved by it each frame (no skin does: StepMania would subtract the
rest pose once per model sharing the geometry; here once). The frame runs
as `AdvanceFrame` runs it: 30 a second times the rate, `SetDefaultAnimation`
playing "default" again from its start when it runs out (another name
holds the frame), else looping or clamped. Two meshes of one name are
merged as StepMania merges them (4 of the corpus's models). Multi-frame
`.ini` textures were done in step 2 (frames by time, each frame's own
translation). Not done: StepMania's hot-pink colour key on model textures
(none of the corpus's 139 readable model textures has a hot-pink pixel).
Checked natively (poses, rest, merge, default animations) and by screenshot
(ITGmania's cel mines spin together, their centres pulsing).

## Step 5. Peter Lawrence's CF skins (commit 5)

CF_CHROME, CF_VIBRANT and CF_ALLOY (Unlicense; CF_CHROME's mine explosion
CC0, JangaFX), their dance files byte-identical with a `PROVENANCE.toml`
(`docs/plans/bundled-skins.md`).

## Later

- A depth buffer, should a skin need models to intersect other notes.
- Lighting (no noteskin turns it on), cel shading (not reachable from
  Lua), sphere maps and alpha maps (no skin uses them).
