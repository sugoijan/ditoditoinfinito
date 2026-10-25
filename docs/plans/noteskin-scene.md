# Noteskin components and materials

Date: 2026-10-25. Designed with the maintainer before any code; the
decisions at the end were taken one by one. It follows the
maintainer's exploration of "a retained SVG scene for noteskins" and the
Spiral Base work (the SVG skin in development, not bundled yet; its port
to components is what the measurements below use), and would take over
`noteskin-svg.md`'s steps 6 and 7 (the spiral material and material-part
metadata).

## What we learned

### Spiral Base's cost is mostly paint drawn ahead of time

Spiral Base is 213 files, 199 of them SVG, 51 MB on disk and 56 of the
skin budget's 64 Mpx (`build::MAX_PIXELS`), 1.2–1.7 s to rasterize in the
web's workers. By role:

- 36 color sheets (arrow 8, center 4, onigiri 8, lift 16): the spiral paint
  baked at 128 phases, interleaved over sheets to stay under the 1 MB SVG
  limit, each phase a mesh of 4 px cells with their own linear gradient,
  overlapped 0.4 px against seams. About 24 MB of the SVG text and,
  estimated from their sizes, roughly 23 Mpx. A 128-entry Lua timeline per
  tap selects the one visible sheet.
- 146 long-note files: 10 shapes × head, head shadow, body, cap and clip for
  holds and rolls, and band, charge and trail for lifts. Their masks are
  canvas rasters traced back into run-length rectangles, close to the 1 MB
  limit each. Roles are sprite-state indices (body frame 0 the band, 1 the
  envelope, 2 the exterior shadow, …), the head join is the top cap actor's
  Y, and a lift's head origin is `LiftTrail`'s Y.
- The rest: shadows as 40 nested opacity contours (before filters), charge
  sheets, receptors, mines, hit effects. Per-shape alignment (Up +1, Down
  −1, Onigiri −7) is repeated by hand in each Lua factory.

The arrow master SVG is a single-phase reference; the real source is a JSON
profile and a Python generator, so SkinCraft cannot edit the design, only
its output.

### How a note is drawn today

- One actor tree per element per lane, shared by every visible note of
  it: for each note the renderer poses the tree (animation phase from the
  beat with Vivid's displacement, colors, placement) and draws it. Notes
  have no state of their own; tweens and effect clocks belong to the
  lane's one tree (`crates/render/src/noteskin.rs`, `note_actor`).
- No Lua runs per drawn note. Per frame, Lua updates the trees and plays
  events (press, judgments, hold on/off) within a 200k fuel budget.
- Every quad goes through one textured shader
  (`crates/render/src/textured.wgsl`): instanced, 140-byte instances,
  batched by texture, blend and mask in draw order, one texture per image
  file. Pipelines are made on first use, during a frame, keyed by blend and
  stencil state. The shader has no time input: all animation is decided on
  the CPU.
- Today's "materials" (`HoldStencilMaterial`, `RollStencilMaterial`,
  `LiftStencilMaterial`) are colored on the CPU: `material_color` is
  evaluated at the corners of every 4×4 logical-pixel cell and each cell is
  an untextured quad inside a stencil mask, so a hold is about six
  instances per pixel of its length and a long lift about 2,400. Each
  material hold clears the stencil with full-screen quads three to five
  times.

So the existing structure is already "shared templates, posed per note",
and it is efficient where it is that. What is expensive or awkward is
paint (baked into frames or emitted as cells), roles carried by indices and
positions, and per-note state that cannot exist (two notes of a lane share
one tween).

## Proposal in one paragraph

Keep templates posed per note, and make the pose a pure function of what
the engine knows: a note's instance has no state of its own beyond the
engine's facts, so drawing it at any time (after a seek, in the editor,
during a pause, in reverse) needs no simulation. A skin's SVG defines
components (`<symbol>` or `<g>` with an `id`) made of named parts; each part
is static geometry rasterized once at load, as today, and is either drawn
as is or used as a mask for a material. Lua, at setup only, declares
components, materials and bindings (a property of a part as a function of
one named clock); the engine evaluates them natively for every note. A
material is the skin's own fragment shader, a bounded WGSL function the
engine wraps, validates and compiles at load; the engine has no paints of
its own, so a spiral, a wave or a holographic foil are each a few lines of
the skin's WGSL.
This is an authoring frontend over the existing drawing back end (textures,
quads, blends, stencil masks, passes, the raster and its report), not over
the actor tree, and the existing StepMania path stays as it is.

## Components and parts

- **Where they live.** One or more SVG files of the skin, in the checked
  subset. A component is an element with an `id` (a `<symbol>` or a
  top-level `<g>`); its coordinate system is the note's: the origin at the
  note's center, up the lane's direction, 64 units a lane. Alignment
  (Spiral's Up +1, Onigiri −7) is then geometry in the component, never a
  factory's offset.
- **Parts** are its children with an `id` (or `data-part`, if ids must stay
  unique across a file). A part is rasterized on its own into the skin's
  atlas (one texture for all parts, so a skin's notes batch into a few
  draws instead of one per file) at the screen's scale
  (`build::svg_scale`), with its bounds and its offset from the
  component's origin.
- **Roles** a part can take: `image` (drawn as is), `mask` (its alpha
  filled by a material), or a long-note role (below). Only parts named by a
  binding or a role are drawn on their own; the rest of the component is
  drawn as one image.
- **Variants** without files: a tap is one component rotated per lane by
  the engine (as Spiral already does with `BaseRotationZ`); a shape that
  differs (center, onigiri) is its own component. Charge stages are parts
  (`charge1`…`charge4`) or a part's frames, not a sheet with a grid in its
  file name.
- **What the engine provides** is unchanged: which notes are visible, their
  lane, shape, position, reverse and layout transforms, hold extents and
  states, receptor states, judgments, Vivid and the clocks below.

### Long notes

A hold, roll or lift is a component whose parts take the roles today's
sprite states and placements imply:

| Role | Today |
| --- | --- |
| `head`, `cap` | top cap / tail actors |
| `body` with a `pitch` (repeat length) | body sheet frame height, `HoldMaterialPitch` |
| `band` (material mask) | body and cap frame 0, head `MaskSource` |
| `envelope` | frame 1 |
| `protect` (head knockout) | clip frame 0 |
| `shadow`, `overlap-shadow` | frames 2 and 3 |
| `pulse` | frame 4, `RollPulse*` |
| `join` (a horizontal line or a `y`) | the top cap actor's Y |
| `trail`, lead length | `LiftTrail`, its Y as the head origin, `LiftPreparationSeconds` |

The engine keeps drawing them as now (repeat anchored at the tail, the
stencil passes of `hold_material.rs`), with three changes: the material is
one quad per visible span instead of cells, the stencil clears cover the
hold's bounds instead of the screen, and each part declares whether its
material coordinates are anchored at the head or the tail. Missing roles
are diagnosed by name at load.

## Clocks and bindings

A binding sets one property of one part from one clock:

- **Properties:** `frame` (a part's frame or which of several parts
  shows), `visible`, `opacity`, `x`/`y`/`rotation`/`zoom` (in component
  units), a material's `phase`, a `tint`.
- **Functions** of the clock, all native: `steps(n, length)` (charge
  stages), `linear(speed, offset)` (scrolling a phase), `curve` (keyframes
  with eases, looping or clamped: a press's zoom, a pulse, a hit's fade).
- **Clocks**, named, each what the engine already computes:

| Clock | Meaning | Today used by |
| --- | --- | --- |
| `note` | song beat, plus Vivid's subdivision displacement for this note | tap color and charge frames, `AnimationOnChildren` |
| `beat` | song beat | receptors' `effectclock("beat")` |
| `seconds` | song seconds | hold material speed |
| `roll` | song seconds over the scaled roll window | roll pulse, `RollAnimationTimingWindowRatio` |
| `press` / `release` | seconds since this lane's last press or release | receptor press tweens |
| `hit` | seconds since this effect's judgment | explosion tweens |
| `lift` | lift preparation, 0 to 1 | lift charge |
| `hold` | this hold's progress or life, 0 to 1 | `HoldLetGoGrayPercent` |

Because every binding is a function of a clock, two notes of a lane differ
when their facts differ (Spiral's charge on two notes, a palette phase per
note) and nothing is shared by accident. Event-driven effects (judgments,
mine hits) become instances too: the engine keeps a few per lane (four,
say), each with its own `hit` clock, so a dense stream's flashes overlap
instead of restarting one tween.

Lua keeps its role for setup: reading parameters, choosing components and
values. It does not run per note, and in this mode it need not run per
frame at all. A skin that wants Lua's commands and tweens keeps using the
existing path for that element.

## Materials

A material is a WGSL function in the skin, which the fragment shader runs
for a part's pixels; its result is multiplied by the part's alpha. The
engine has no built-in paints: Spiral Base's spiral is a placeholder for a
look that may become something else entirely (a holographic foil like
fffuel's hhholographic, itself a GLSL shader), so the engine provides the
means and the skin the paint. Today's hold material and Spiral's spiral
become WGSL materials of their skins, which also proves the interface on
what we know works.

### What an author writes

```wgsl
// materials/foil.wgsl
fn paint(m: Material) -> vec4<f32> {
    let n = fbm4(m.pos / 48.0 + vec2(m.beat * 0.1, m.seed));
    let t = fract(n + m.screen.y * 0.3 + m.phase);
    return vec4(ramp(0, t), 1.0);
}
```

- **Inputs** (`Material`): `pos`, the material coordinates in logical
  pixels (their space and origin declared with the material, below); `uv`,
  the part's own coordinates; `mask`, the part's alpha; `screen`, the
  pixel's place in the field, 0 to 1 (so a foil can shift across lanes as if
  tilted); `phase` and `value` (a `vec4`), set per instance by bindings;
  `seed`, stable per note (variety between notes); `beat` and `seconds`,
  the song's clocks (a note's own clock reaches it as its `phase`);
  `params`, eight `vec4` per material from setup Lua or the skin's
  parameters (Options → Arrows). As built: `material_prelude.wgsl`.
- **Resources:** up to two images per material: a skin bitmap, an SVG part,
  or an SVG `<linearGradient>` sampled at load into a ramp (`ramp(row,
  t)`), so palettes stay editable as vector gradients.
- **A small library** in the engine's prelude: `hash12`, value `noise`,
  `fbm4`/`fbm8` (unrolled), `fmod` (GLSL's `mod`), color helpers (`hsv`,
  `oklab_mix`, `to_linear`, `to_srgb`), `PI` and `TAU`; the ramp and image
  lookups come with images (step 3).
- **Output:** straight alpha in sRGB-encoded values, as CSS colors and
  GLSL tools write them, so a ported shader looks the same; the wrapper
  converts for each render path (linear scene, plain view, WebGL2's
  offscreen target) and applies the alpha test the stencil masks need.

### Bounds and failures

- Validated with naga at load against WebGL2's limits: a fragment-only
  function, at most 16 KB of source, at most 8 materials a skin. No loops
  (the library's noise is unrolled), WGSL has no recursion, and `paint`
  evaluates at most 65,536 expressions with its calls inlined (functions
  calling others twice over would otherwise double it at each level), so
  a material's cost per pixel is bounded and it cannot hang the GPU; if a
  skin needs a loop, loops with a literal bound of at most 16 are the next
  step.
- A material that does not validate is reported with its line, as
  SkinCraft reports Lua, and its parts draw their own art instead.
- Cost scales with the pixels it covers; the frame counters below and
  desktop's GPU timestamps give each material's share, which SkinCraft
  shows.

### Coordinates

Material coordinates name their space: the component's (rotating with the
note), the lane's (not rotated: Spiral's lifts, where "only the field
rotates"), or the long note's (along the hold, anchored at the head or the
tail, continuous through head, body and cap). Each material has its own
origin, so moving a part to align it never moves the paint under it.

### How it is drawn

- **One pipeline per material,** generated from the engine's textured
  shader with the author's function spliced in, for the blends and masks
  its parts declare: all of them known at load, so all are compiled at load
  (on WebGL2 that is a GLSL compile each, to be measured). Parts without a
  material keep the existing shader.
- **The instance** keeps today's 140-byte layout: the material's phase,
  value and seed go in the fields today's gradients use (`c1`…`c3`);
  material parameters in one uniform buffer per skin (WebGL2's 16 KB
  allows far more than 8); the clocks in the frame's uniforms.
- **Batching:** material quads batch by material, texture, blend and mask,
  so a lane of foil notes drawn from one atlas is one draw.
- **Baked when static:** a material that reads no clock, phase, value,
  seed or screen position (known from its IR) is drawn once at load into
  a texture at the raster scale and then costs what an image costs.

## Efficiency

Constraints for the design, checked by the prototype:

- **Per note:** a few bindings evaluated natively and one instance per
  drawn part; no Lua, no allocation. Material paint costs one quad per part
  or span, not cells.
- **Per frame:** draws bounded by textures × blends × masks in draw order;
  one skin atlas keeps that near the number of layers. No pipeline is made
  during a frame: a skin's parts declare their blends, masks and materials
  statically, so every pipeline it needs is made at load (today's path makes
  them on first use, a hitch on WebGL2 where it compiles GLSL).
- **Per load:** pixels for geometry only, at the screen's scale; for
  Spiral Base that is the masks, charge stages and shadows, without the
  color sheets' roughly 23 Mpx and their 24 MB of SVG.
- **Measured, not assumed:** the frame's instances, draws, passes, stencil
  clears and pipelines made, counted on both shells (`__DDI_DEBUG` on the
  web; desktop's pacing report), and GPU time where timestamp queries
  exist (desktop), on a dense chart and a long-note-heavy one, before and
  after.

Redrawing a field's SVG on the CPU each frame is not pursued: Spiral's 199
images take 1.23 s on four workers, about 25 ms of a worker each (sheets of
many frames, so a single frame costs less), while a dense field shows
dozens of masked and filtered parts and a frame at 144 Hz has 7 ms, all of
it then uploaded again. A measurement would only confirm the order of
magnitude.

## Authoring, inspection and failures

- **SkinCraft edits the design itself.** The component SVG is the source,
  editable in VectorCraft; materials are WGSL in its code editor, checked as
  it checks Lua; their parameters are Lua values (or skin parameters)
  SkinCraft can expose as controls. A change reloads the skin;
  since instances are functions of the engine's facts, the same frame draws
  again identically, with no live instances to patch.
- **Inspection** (SkinCraft and MCP): a note's component, parts, the value
  of each binding and its clock, the material and its parameters, at any
  time, because they are computed, not accumulated.
- **Captures** record the skin's images hash (`ImageReport::hash`), the
  parameters, time and beat, the layout and the renderer path.
- **Diagnostics** at load, with the file and element `id`: an unknown
  part, a role missing from a long note, a clock a property cannot take,
  a ramp or material that does not exist, a WGSL function that does not
  validate. The skin then loads with that element falling back, as an
  image that fails to decode does today.

## Prototype

In order, each measured and checked on desktop, WebGPU and WebGL2:

1. **Material shaders, on today's skins.** The WGSL wrapper, validation,
   prelude, per-skin uniforms and pipelines made at load, first used by
   the existing `HoldStencilMaterial`/`Roll…`/`Lift…` metrics, whose field
   becomes a WGSL material the engine ships for them, instead of cells; and
   bounded stencil clears. Spiral Base looks the same and its long notes
   cost a few instances instead of thousands: a gain on its own, and the
   measurement harness for the rest, including the GLSL compile on WebGL2.
2. **Components and bindings.** One arrow and one receptor from an SVG
   component, its charge as a four-stage `steps` binding on `note`, the
   press as a `curve` on `press`, drawn by the real renderer and in
   SkinCraft's preview.
3. **A skin's own material on taps.** The arrow's color sheets replaced by
   a mask and Spiral's spiral written as the skin's WGSL, two notes with
   independent phase, compared with the accepted sheets; then a second,
   different material (a holographic foil) on the same parts, to check the
   interface is not shaped around the spiral.
4. **A hold component.** Named band, envelope, protect, shadow and join,
   continuous material, reverse and shrinking.
5. **One judgment effect** as instances with a `hit` clock.
6. Then: port a small part of Spiral Base, compare it with the accepted
   design, and refine the API from what it needed.

## Step 1: done (2026-10-25)

`ddi_render::material`: `compile` checks a material's `paint` (16 KB at
most; no entry points, bindings, global variables or loops; at most
65,536 expressions with its calls inlined, `MAX_INLINED`; the engine's
names reserved; `paint(m: Material) -> vec4<f32>`), validates the whole
shader with naga for WebGL2's capabilities and translates it to GLSL ES
3.0 as WebGL2 will (which caught the parameters first declared as an
array of arrays, which GLSL ES 3.0 lacks), with the material's own line in
its errors. `TexturedPipeline::add_material` makes its pipelines at load
for the blends given, with and without the mask; material quads keep the
textured shader's vertex stage and instance (now 188 bytes: the material
coordinates, phase, seed and value) and batch like other quads. The
long-note metrics' wave is the engine's one material
(`materials/hold_wave.wgsl`, `material::HOLD_WAVE`): a hold or roll paints
one quad a visible span, a lift's trail one 4-pixel row at a time (its fade
per row, as before), instead of a quad every 4×4 pixels; and the mask
clears of a material note cover that note's box, not the target. Frame
counters (`QuadStats`; `__DDI_DEBUG.noteskin.gpu` on the web) and the
desktop example `noteskin_cost` measure it.

Spiral Base on a dense field (`noteskin_cost`: 10 lanes at 1920×1080, in
every lane holds, rolls, lifts, taps and mines; M1 Max, median of 230
frames; forward and reverse alike):

| | before | after |
|---|---|---|
| quads a frame | 65,624 | 2,072 |
| runs (draws) | 1,475 | 1,491 |
| mask clears' pixels | 431 Mpx | 8.4 Mpx |
| build (CPU) | 11.0 ms | 0.48 ms |
| prepare (CPU) | 2.35 ms | 0.19 ms |
| draw (GPU wait) | 11.4 ms | 2.5 ms |

The picture is the same: 0.23% of pixels differ by more than 8 levels,
none by more than 31, along the wave's gradients, now evaluated at each
pixel rather than between the cells' corners. Chrome on WebGPU and WebGL2
and Firefox on WebGL2 draw Spiral Base's material holds and rolls with no
shader or pipeline error. Runs are next: a hold's parts are separate
images, so it takes several draws (one skin atlas, step 2).

## Step 2: components and bindings (2026-10-25)

Done: components and their parts from a skin's SVG
(`ddi_noteskin::component`), `Def.Component` and `Bind.*` in a skin's Lua,
compiled at load to native parts and bindings, each part (and frame) an
image of its own read and drawn as the skin's SVG files are (the pixel
budget, the image report, the web's SVG workers), and drawn per note and
per receptor by the play's renderer with the clocks below. Checked by
`crates/desktop/tests/component_skin.rs` (a skin of two components: the
tap turned for each lane, its charge's frame from the note's clock with
Vivid, the receptor's flash from the press). The atlas is done (see "Draws and
the atlas"); left: showing a part's bindings in SkinCraft.

## Step 3: a skin's own materials (2026-10-25)

A component's part may be painted by a material of the skin: its WGSL
(kept with the skin's texts, as its Lua is), compiled and checked when the
skin loads (`NoteskinPlay`, `material::compile`; one that fails is said in
the skin's notes and its parts draw their own images), registered once by
its source and its pipelines made then (`TexturedPipeline::skin_material`).
The part's image is its mask; the material's coordinates are the
component's (or the lane's), from the origin it declares; its parameters,
images (a file, or a gradient of the skin's SVG drawn as a ramp) and the
part's `Phase` binding reach the shader. Material pipelines take WebGL2's
fourth and last bind group, for the images.

Spiral Base's spiral, drawn as a material (the generator's math, the
palette a ramp, the 128 frames one `Phase` binding) over the arrow's band,
differed from the generator's first frame by 0.35 levels on average inside
the band, 2 at most, the accepted art's 4-pixel cell seams gone; a
holographic foil (noise, the field's place, a sheen) drew on the same part
unchanged. With this, Spiral Base's 36 color sheets can become a band mask
and one material each. That comparison stays with the skin, developed
apart; `crates/desktop/tests/material_art.rs` checks the same alignment
on its own: a material painting a gradient in the component's
coordinates, inside a circle part, is the same picture to the level as
resvg drawing that gradient as SVG.

## Step 4: long notes and lifts (2026-10-25)

A hold's or roll's body element may return `Def.LongNote`, drawing the
whole long note from three components (head, a body tile, cap) whose
parts' roles replace the sprite-state indices and placements of the
`HoldStencilMaterial` path; a lift's element may return `Def.Lift`, a head
component and a trail leading it over the time-sized lead. Both are drawn
by the same layered code as the metrics path (`HoldLayers`, refactored
from `material_hold`: Spiral Base's dense field draws pixel for pixel as
in step 1), their band or trail painted by the skin's material, and a
lift's band and charge become ordinary parts with materials (no stencil).
Checked by `crates/desktop/tests/long_note_skin.rs` (a hold's band painted
along the note to its cap, the shadow outside the envelope, the head's
protected hole, ring and mark; a roll's pulse; a lift's faded trail and
shadow; in both scroll directions).

## Step 5: judgment effects as instances (2026-10-25)

An explosion element may return a component: each lane keeps its last
judgments (eight at most, with when they happened and whether bright), and
the component is drawn once for each still within its `Duration`, its
`hit` clock the time since, its parts chosen by `When` and `Bright`; while
a long note is held, once more as `Holding` or `Rolling`. Overlapping
hits each play their own animation instead of restarting one tween; a seek
back forgets them. Checked in `crates/desktop/tests/component_skin.rs` (a
W1 and a W2 overlapping in one lane, each at its own point of its fade).

## Draws and the atlas (2026-10-25)

A skin's component parts are packed into atlas pages on the GPU at its
first frame (`TexturedPipeline::pack`, the parts `NoteLayer::atlas_textures`
names: never a long note's repeated body, a material's image, a model's
texture, or one a sprite hold's or roll's body repeats, whose quads wrap),
their quads drawn from there; the picture is the same pixel for pixel. A
page is as large as what it takes needs (a power of two from 256 to 4096
pixels, or the device's limit), so a few small parts do not cost a 64 MB
page. Measured with the port of Spiral Base (`noteskin_cost` on the dense
field of step 1: 10 lanes, about 40 long notes on screen): 958 draw runs a
frame before, 843 with the atlas (taps alone 299 to 254, long notes alone
689 to 589), 1.9 to 1.7 ms of GPU time. Long notes draw their stencil passes
in turn, about 15 runs each whatever their textures; drawing the passes of
long notes that do not overlap together would cut that, but parts as wide
as their `viewBox` (96 units on 64-unit lanes) make neighboring lanes
overlap, so it would need parts trimmed to their contents first.

Not done, measured instead: on a long-note-heavy chart (four lanes, each
holding or rolling half the time, about 120 runs a frame with the port), a
frame's main-thread time (its animation-frame callback, the frame encoded
and submitted) in headless Chrome on an M1 Max is 0.6 ms on WebGPU and 1.3
ms on WebGL2 with the port, 0.7 and 1.6 ms with Spiral Base's actors, 0.5
and 0.8 ms with Dito: about 5 µs a run on WebGL2. Grouping long notes (and
trimming parts) is the next step if a slower device shows the draws.

Images of the same pixels are kept once: a texture whose image an
earlier one's has is given that one's id (`NoteskinPlay::share_textures`),
its own image let go before it is uploaded, and the report says
`Shared` (the image it shares) and counts it no more. Desktop finds them
by a hash of the decoded rows and compares them; the web by the hash the
SVG worker sends with each image it draws (images the browser draws are
not shared). The port draws 502 images, 166 of them the same pixels as
another (the hold, roll and lift heads' frames, explosions' layers
repeated across shapes, a tap's and its receptor's art): 31.1 Mpx kept
before, 20.3 after.

A part's document holds only what of its file the part reaches
(`ddi_noteskin::component::part_svg`), not the whole file: each
definition it uses, through `use`, `href` and `url(#…)` and theirs in
turn, inside its ancestors (their ids left out) so that inherited
properties are as in the file. The port's part files are up to 900 KB,
and a part's document was one of them each: 475 documents, 218 MB of
text, read again by every check and draw. Now they are 14 MB, read once a
file (a file's documents are refused past 8 MB together,
`MAX_PART_DOCUMENTS`, as many parts reaching one large definition would
make them); the port's load on desktop (`noteskin_cost`, M1 Max) took 1.37 s
and 576 MB at most, now 0.48 s and 286 MB, its frames the same bytes.
Ids no definition names are made the same (`ddi-part`), so the same
part of two components is the same document, and a shell draws it once
(`SameDocuments`, `NoteskinPlay::take_images`, which moves each image to
its texture instead of copying it): on the web, 233 images drawn of the
port's 270 in 435 ms instead of 839, the same pictures on WebGPU and
WebGL2.

## Authoring reference (2026-10-25: stable enough to port; changes noted here)

What a skin writes to draw an element with a component: taps, lifts,
fakes, mines, receptors, explosions and long notes may all be.

Changes since the port began (from its feedback, 2026-10-25):

- A whole component may be animated: `X`, `Y`, `Rotation` and `Zoom` on
  `Def.Component` take bindings, turning and zooming about `Pivot` (a
  number for `Rotation` or `Zoom` is still its turn and zoom). A
  receptor's press zoom needs no repeating on each part.
- A receptor's parts may react to what its lane's last press did:
  `When = {"W1", …}` (`W1` … `W5`, `None` for a press that hit nothing,
  `HitMine`), with the `hit` clock the time since.
- Curves ease with any power: `ease = "out", power = 0.65` (`in`:
  `t^power`, `out`: `1 - (1 - t)^power`).
- A hold's or roll's head, tail and caps are not asked for once its body
  returns a `Def.LongNote`: no placeholders needed.
- Properties a long note's or a trail's layers ignore are said in the
  skin's notes (see "Long notes").
- The web prefetches a ramp's file, not its `file.svg#id`; an images
  report's hash no longer depends on the order the shell listed them in.
- A skin declares the buttons it draws in `metrics.ini`: `[Global]
  Buttons=Onigiri,Center` (see "Buttons"); forwarding files named for a
  button only to be noticed are not needed.
- From the step 5 port: `ease = "spring"` (StepMania's); a part's `Tint`
  (one color, or two that a bindable `TintMix` mixes), multiplying its
  image and a material's output as an actor's diffuse does; `Bind.Random
  {min, max, salt}`, a value fixed by the instance's seed (a note's, or a
  judgment's lane and time), for a varied rotation or zoom of vector art.
- A held effect has a tail: once its hold ends (kept or let go), the
  `Holding` or `Rolling` instance is drawn on for the component's
  `Duration`, a cycle on its `hit` clock finishing the one under way and
  not starting again, so echoes in flight fade out and no new ones
  begin; the `ended` clock counts the seconds since the end.
- A lift's head may have `protect` and `outside` parts (see "Lifts"),
  upright with the head: holes knocked out of the trail's own art turn
  over with it in reverse and miss the head's.
- Images of the same pixels are kept once (see "Draws and the atlas"):
  the report's `Shared` outcome names the image kept.
- A part's image is drawn from only what of its file it reaches, and
  the same part of two components once (see "Draws and the atlas"); the
  report's hash of a part is its new document's, so it changed for every
  component skin once.
- A seek in the field preview (Options', SkinCraft's scrubbing and
  `capture_preview`) or on the skin preview page (`beat`) plays the second before
  the time sought again, a 30th of a second at a time, its judgments
  included: effects under way show as they would have in play, actors'
  tweens and components' instances alike. A play's own seeks, and the
  demo catching up after a hidden page, stay silent.

### The SVG

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <defs>
    <linearGradient id="g">…</linearGradient>
    <symbol id="arrow" viewBox="-32 -32 64 64">
      <path d="…" fill="#000" opacity="0.4"/>          <!-- an unnamed part -->
      <path id="body" d="…" fill="url(#g)"/>           <!-- part "body" -->
      <g id="charge" data-frames="">                   <!-- part "charge", 2 frames -->
        <path d="…"/>
        <path d="…"/>
      </g>
    </symbol>
  </defs>
</svg>
```

- A component is a `<symbol>` with an `id` and a `viewBox`, a child of the
  root or of a root `<defs>`. Its coordinates are the note's: the origin at
  the note's center, 64 units a lane; the `viewBox` is the area its parts
  may draw in (draw beyond it and it is cut). An arrow is drawn pointing
  down, as StepMania's Down art is; the Lua turns it for each lane.
- Its children are its parts, in drawing order: a child with an `id` is the
  part of that name; a run of children without one is one unnamed part
  (drawn, never bound). `defs`, gradients, masks, clip paths, filters and
  titles among them are none.
- A part with `data-frames` is a group whose children are its frames, one
  shown at a time (the `Frame` property); the group's own attributes apply
  to each. A frame may not set `display`.
- `data-role` on a part names its role (for long notes and materials,
  later steps). `data-raster-scale` on the symbol caps its parts' drawing as
  it does a file's.
- A part draws only what it holds: presentation attributes on the
  `<symbol>` itself do not reach its parts (they are drawn through `use`).
- The file is checked as every SVG of a skin is (`ddi_noteskin::svg`).
  Each part is drawn as an image over the whole `viewBox`, so a part costs
  the `viewBox`'s pixels at the screen's scale (the skin's image report
  lists them as `file#component/part[/frame]`).

### Buttons

A lane of a layout's onigiri or centre panel is drawn with the skin's
`Onigiri` (else `Center`) elements, a pump down-left or down-right one with
its own, when the skin has them: files named for that button in its folders
(`Center Tap Note.lua`), or, for a skin whose `Load` draws them through its
redirects and components, its declaration in `metrics.ini` (our extension,
merged along its fallbacks):

```ini
[Global]
FallbackNoteSkin=Peters-DDR
Buttons=Onigiri,Center
```

Otherwise such a lane falls back on the next button, and last on Down
unturned.

### The Lua

An element's `Load` returns a component instead of actors:

```lua
local turn = {Left = 90, Down = 0, Up = 180, Right = 270,
              UpLeft = 135, UpRight = 225, DownLeft = 45, DownRight = 315}
ret.Load = function()
    local e = string.lower(Var "Element")
    if e == "tap note" then
        return Def.Component {
            File = "arrow.svg", Id = "arrow",
            Rotation = turn[Var "Button"] or 0,
            Parts = {
                charge = {Frame = Bind.Steps {clock = "note", length = 4, count = 4}},
                body = {Opacity = 0.9},
            },
        }
    end
    if e == "receptor" then
        return Def.Component {
            File = "arrow.svg", Id = "receptor",
            Parts = {
                flash = {Opacity = Bind.Curve {clock = "press", keys = {0, 1, 0.2, 0}}},
                frame = {Zoom = Bind.Curve {clock = "press", keys = {0, 0.9, 0.1, 1},
                                            ease = "decelerate"}},
            },
        }
    end
    return oldLoad()
end
```

- `Def.Component { File, Id, Rotation, Zoom, X, Y, Pivot, Parts }`:
  `File` relative to the file that writes it (in `NoteSkin.lua`, the
  skin's folder), or a `NOTESKIN:GetPath` result; `Id` the symbol's;
  `Rotation` in degrees (clockwise) and `Zoom` for the whole, numbers (its
  turn for the lane) or bindings (added to the turn, times the zoom), with
  `X` and `Y` bindings too, turning and zooming about `Pivot = {x, y}` (the
  component's units); `Parts` by part name.
- A part's properties, each a number (or boolean) or a binding: `Frame`
  (which frame shows: floored, wrapped), `Visible` (shown above 0.5),
  `Opacity`, `X`, `Y` (the component's units), `Rotation` (degrees),
  `Zoom` (about the component's origin), `Phase` (a material's),
  `TintMix` (0 to 1 between its two tints); and `Blend` (`"normal"` or
  `"add"`), `Tint` (`"#rrggbb[aa]"` or `{r, g, b[, a]}`, or two of them:
  the colors its image, or its material's paint, is multiplied by).
- Lua runs only while the skin loads: bindings are evaluated by the engine
  for every drawn note, with no Lua then, so a component costs nothing per
  frame in the skin's Lua budget.
- What is wrong (a part the symbol lacks, an unknown property, clock or
  ease, a malformed curve) is said in the skin's notes (SkinCraft's
  problems, the console), and the rest is kept.

### Materials

```lua
local spiral = Def.Material {
    File = "spiral.wgsl",                  -- fn paint(m: Material) -> vec4<f32>
    Space = "note",                        -- or "lane"
    Origin = {0, 0},                       -- where m.pos is 0, component units
    Params = {{120.6, 1, 97, 0.43}, {0.5, 1.089, 0, 8.71}, "#31d5ff"},
    Images = {"arrow.svg#palette"},        -- a gradient's ramp, or an image file
}
-- …
Parts = {fill = {Material = spiral,
                 Phase = Bind.Linear {clock = "note", length = 4, from = 0.626, to = -0.374}}},
```

- `File`: the skin's WGSL, a function `paint(m: Material) -> vec4<f32>`
  returning an sRGB-encoded color with straight alpha. What `Material`
  holds and the helpers it may call are `crates/render/src/material_prelude.wgsl`
  (`pos`, `uv`, `mask`, `screen`, `phase`, `value`, `seed`, `beat`,
  `seconds`, `params`; `fbm4`, `fbm8`, `noise`, `hash12`, `hsv`,
  `oklab_mix`, `to_linear`, `to_srgb`, `fmod`, `PI`, `TAU`), with
  `image_at(i, uv)` and `ramp(i, t)` for its images. No loops, bindings,
  entry points or global variables; at most 16 KB, and at most 65,536
  expressions with its calls inlined (sixteen `fbm8` calls are about
  20,000). Errors come with the material's own line.
- `Space`: `note`, the component's coordinates (turned and zoomed with
  it), or `lane`, upright whatever the component's `Rotation`. A part
  moved by its bindings moves through the paint; the paint stays.
- `Params`: up to eight, each a number, up to four numbers, or a color
  (`"#rrggbb"`, `"#rrggbbaa"`): `m.params[0]` … `m.params[7]`.
- `Images`: up to two. `file.svg#id` draws that gradient across a 256-pixel
  ramp (`ramp(i, t)` repeats: end a cyclic palette with its first color);
  any other path is an image of the skin, sampled repeating by
  `image_at(i, uv)`.
- On the part: `Material`, and `Phase` (any number or binding) for
  `m.phase`. The part's image is the material's mask (`m.mask`, and the
  result's alpha is multiplied by it); its `Opacity` still applies.
- `m.seed` is stable for a note and differs between notes (a receptor's,
  by lane).

### Long notes

A hold's or roll's body element (`Hold Body Inactive`, `Roll Body Active`,
…; one a state) returns the whole long note:

```lua
if string.find(string.lower(Var "Element"), "^hold body") then
    return Def.LongNote {
        File = "hold.svg", Head = "hold-head", Body = "hold-body", Cap = "hold-cap",
        Join = 8,                       -- where the body starts below the head
        Pitch = 13,                     -- the body's repeat (default: its viewBox's height)
        Material = wave, Phase = Bind.Linear {clock = "seconds", length = 1},
        Anchor = "head",                -- or "tail": where the fill's coordinates start
        Parts = {charge = {Frame = Bind.Steps {clock = "note", length = 4, count = 4}}},
    }
end
```

- `Head`: a component at the note (origin the head's center). Its parts by
  `data-role`: `band` (filled with the band), `protect` (takes the body's
  band and shading out under the head), `outside` (the envelope around the
  head), `shadow` (falls outside the envelope); parts without a role are
  drawn over everything, with their bindings (the charge).
- The body's, cap's and the head's `protect` and `outside` layers take a
  part's `Frame` and `Visible` (a pulse its `Opacity` too); anything else
  set on them (placement, materials, blending) is said in the skin's notes
  and has no effect: placement belongs in the SVG. The head's other parts
  take everything a component's do, as does `X`, `Y`, `Rotation`, `Zoom`
  on the `Def.LongNote` for the head.
- The hold's or roll's head, tail and caps elements of that state are not
  asked for.
- `Body`: one tile, its `viewBox` from y 0 to the pitch, repeated from the
  join to the cap, anchored at the tail (the tile's phase stays with the
  tail as the note shrinks). `Cap`: its `viewBox` around the tail (its
  origin). Their parts by role: `band`, `envelope`, `shadow` (outside the
  envelope), `overlap-shadow` (over the band, under the head's protection),
  `pulse` (drawn outside the head's protection at its `Opacity`, a
  binding on the `roll` clock for a roll's beat-keyed pulse).
- `Material` and `Phase` paint the band: its coordinates along the note
  (x across from the lane's middle, y from the head, or the tail with
  `Anchor = "tail"`), continuous through head, body and cap. A hold let go
  greys its fill.
- Reverse scroll mirrors the long note about its head, as today's do.

### Lifts

```lua
if string.lower(Var "Element") == "tap lift" then
    return Def.Lift {
        File = "lift.svg", Id = "lift-head", Trail = "lift-trail",
        Seconds = 0.28, Fade = 2, Opacity = 1,
        Material = wave, Phase = Bind.Linear {clock = "seconds", length = 1},
        Rotation = turn[Var "Button"] or 0,
        Parts = {charge = {Material = spiral, Frame = Bind.Steps {clock = "note", length = 4, count = 16}}},
    }
end
```

- `Id`: the head, a component drawn as a tap's (turned by `Rotation`; its
  band, charge or colors are parts, with materials of their own).
- `Trail`: a component upright in the lane, its `viewBox` from the note's
  center with up the lane (negative y) ahead of the note; drawn from the
  lead's far end (the distance the note travels in `Seconds`, a quarter
  beat at least) to its own end, faded toward the far end by `Fade` (an
  exponent) and `Opacity`. Its parts by role: `trail` (the mask `Material`
  paints), `shadow` (drawn under it, faded alike). In reverse the trail
  is turned over, the head is not.
- The head's parts by role, as a long note's head's: `protect` (its holes:
  kept out of the trail's fill) and `outside` (kept out of the trail's
  shadow), in the head's own coordinates, so they stay where the head's
  art is in either scroll direction; neither is drawn. Keep holes out of
  the trail's own art, which reverse turns over.

### Explosions

```lua
if string.lower(Var "Element") == "explosion" then
    local flash = Bind.Curve {clock = "hit", keys = {0, 1, 0.25, 0}, ease = "decelerate"}
    return Def.Component {
        File = "hits.svg", Id = "explosion", Duration = 0.4,
        Parts = {
            glow = {When = {"W1", "W2"}, Opacity = flash, Blend = "add"},
            ring = {When = {"W1"}, Bright = true, Opacity = flash,
                    Zoom = Bind.Curve {clock = "hit", keys = {0, 0.8, 0.25, 1.3}}},
            sparks = {When = {"HitMine"}, Opacity = flash},
            hold = {When = {"Holding", "Rolling"},
                    Opacity = Bind.Linear {clock = "hit", length = 0.1, from = 1, to = 0.6}},
        },
    }
end
```

- Drawn at the receptor once for each of the lane's judgments within its
  `Duration` (seconds, default 1), and continuously while a long note is
  held there, then for `Duration` more after the hold ends.
- A part's `When`: the judgments it shows for, `W1` … `W5` (a miss sets
  off no explosion, as in StepMania), `Held` (a long note kept to its
  end), `HitMine`, and `Holding` or
  `Rolling` (while held and after, the `hit` clock from when the hold
  began; once it has ended, a cycle on `hit` finishes the one under way
  and then stays at its end, a pulse in flight playing out and no new one
  starting; fade the rest with `ended`); without
  `When`, every judgment but `Holding` and `Rolling`. `Bright = true` (or
  `false`) shows it only for a bright (or dim) judgment.
- The `hit` clock: seconds since the instance's judgment. With
  `TapJudgmentsPerNote`, each note's judgment is an instance of its own.

### Bindings

| Binding | Value |
| --- | --- |
| `Bind.Linear {clock, length, from = 0, to = 1, offset = 0}` | from `from` to `to` over each cycle of `length` |
| `Bind.Steps {clock, length, count, offset = 0}` | the cycle in `count` steps: 0 … `count - 1` |
| `Bind.Curve {clock, keys = {t0, v0, t1, v1, …}, ease = "linear", length}` | keys eased between, held before the first and after the last; with `length`, repeated |
| `Bind.Random {min = 0, max = 1, salt = 0}` | fixed by the instance's seed and `salt`: the same every frame, different between notes or judgments (no clock) |

Eases: `linear`, `accelerate` (`in`), `decelerate` (`out`), `smooth`
(`inout`), `spring` (StepMania's: past the end and back); `in` and `out`
take a `power` (2 unless given).

### Clocks

| Clock | Value |
| --- | --- |
| `note` | for a note: the song's beat, in a cycle (Linear, Steps, a Curve with `length`) moved by Vivid's displacement for the note's subdivision, as StepMania's animation phase (`AnimationIsVivid`); outside one, beats from the note's own (negative before it). Elsewhere, the beat. |
| `beat` | the song's beat |
| `seconds` | the song's seconds |
| `roll` | the song's seconds over the roll timing window (scale it with a binding's `length`) |
| `press`, `release` | seconds since the lane's last press, release (long ago before any) |
| `hit` | an explosion's instance: seconds since its judgment; a receptor: since its lane's last press was judged (long ago elsewhere) |
| `ended` | a `Holding` or `Rolling` instance: seconds since its hold ended (0 while it lasts, and elsewhere) |

Every value is computed from the song's time and the lane's state, so a
note draws the same after a seek, in the chart editor and in reverse, and
two notes of a lane never share an animation.

## Decisions (2026-10-25)

Taken with the maintainer, one by one:

1. **Bindings:** the SVG holds geometry, ids, origins and long-note roles;
   setup Lua declares materials and bindings by id, compiled to native
   data at load.
2. **Geometry:** paths are drawn once at load and never change during
   play. Motion is transforms, frames, visibility, opacity and material
   inputs; shapes change inside a material (alpha carving, or a
   signed-distance shape drawn in a part that is only its bounds).
3. **Opting in:** per element. Setup Lua declares the elements it draws as
   components; every other element resolves as today, through the skin's
   files and its fallbacks.
4. **Color space:** a material writes sRGB-encoded straight alpha; the
   wrapper converts for each render path, and the prelude has linear and
   Oklab mixing for authors who want it.
5. **Language:** WGSL only. naga's GLSL reader takes only GLSL 440–460,
   not the WebGL dialect shader tools write, so GLSL would not spare a
   port; the prelude has `fmod` for GLSL's `mod` (WGSL has `atan2`, and no
   overloading, so helpers have one name each).
6. **Loops:** refused at first, with unrolled noise in the prelude; loops
   capped by the engine or literal-bound `for` loops can come later
   without breaking a skin.
7. **Across lanes:** nothing in the prototype; mine links and rims stay the
   engine's neighbor passes, through the per-element fallback. Field
   components (one material quad given per-lane facts) and pair components
   (between two notes) are the forms left room for.
8. **Underneath:** components compile to lean parts with native bindings,
   drawn into the same quad stream as actors, with the same blends, masks,
   passes and textures; actors stay for StepMania elements.


## Infinito authoring port (2026-10-25)

The authored skin formerly called Spiral Base and Dito is now bundled as `Infinito`, the
default, replacing its original art and color variants. Its editable SVG, WGSL
and Lua sources live in `assets/noteskins/dance/Infinito/`; experimental
authoring tools and retired assets are kept separately. Taps, receptors, holds, rolls, lifts and
judgment effects use SVG components and skin-owned materials. Mines keep
actors for field-aligned neighboring-lane electricity; opt-in judgment rim
and link roles join matching receptor shocks. The original Spiral Base
measurements above remain historical; its old bundled name is retired.
