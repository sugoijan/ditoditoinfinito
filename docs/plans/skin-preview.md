# Noteskin previews (2026-10-13)

A page that draws a noteskin's elements alone, as a play draws them: first
to check skins against reference renders, later to preview skins where
they are chosen.

## Step 1. The preview page

`#/skin-preview?skin=<key>` (`crates/web/src/components/skin_preview.rs`): the
skin's Down tap note (`notes=4`: the four arrows; `receptors=1`: their
receptors too) on a canvas, `size` pixels an arrow, over `bg`, held at
`beat` or running at `bpm`, on WebGPU or (`gfx=gl`) WebGL2. It goes through
the play's own path: the skin is readied as for a play
(`noteskins::prepare`), its textures uploaded by the game loop's function
(`game_loop::upload_noteskin`, shared), and drawn by `Renderer::render`
with `RenderOptions::preview` (`crates/render/src/scene.rs`: the skin's elements
alone, placed by the given `FieldGeometry`), so blending, models, mipmaps
and WebGL2's offscreen frames are the play's. A test sets
`window.__DDI_PREVIEW_BEAT` and waits for `window.__DDI_PREVIEW_DRAWN`.

**Done 2026-10-13.** With it, `FieldGeometry::lane_receptor_y` follows the
geometry's `receptor_y` (it assumed the play's placement; the same in a
play).

## Step 2. Peter Lawrence's renders as references

His repository's README shows a GIF of each SM5 skin's Down tap note:
128 px (the skin's 64 px at 2×), 60 frames a beat, a 4-beat loop (2 for
CF_ALLOY and CF_VIBRANTALLOY), a key color as transparency (red, or white
for the ALLOY ones). Drawn by the preview at the same beats and compared
frame by frame over the GIF's opaque pixels (scripts kept local; the GIFs
are not in the repository).

**Done 2026-10-13, as a check.** Shapes, sizes, textures, colors and their
animation match: CF_CHROME, CF_VIBRANT, CF_ALLOY, PETERS_METAL, PETERS_CEL,
Peters-DDR-Rainbow line up at the same beats; Peters-DDR-Note and
Peters-DDR-Vivid line up 16 frames (0.267 beats) apart, which one time
offset explains for both (their scrolls and Vivid's color cycle shift
together), while Peters-DDR-Rainbow, whose model scrolls the same texture
the same way, lines up exactly: their renders started elsewhere. Our
arrows sit 1 to 4 pixels lower than in the renders (by skin; the renders'
framing, or StepMania's perspective on a model near the top of its
screen); shifted to match, the mean difference over the arrow is 13 to 22
of 255, much of it the GIFs' 256-color dithering.

## Step 3. Holds, mines and explosions: the demo

`demo=1`: a short chart scrolling into the receptors (`crates/engine/src/demo.rs`):
taps of several note types in every lane, holds, rolls and a mine,
played by the engine's own `Player` (ITG's rules, failing off) with the
autoplayer's presses moved to land in each of the judgement windows in
turn (on time between them) and now and then left out, every other hold
let go and every other roll left a third of the way, and a step on each
mine, so the frames and the skin's events (presses, steps, each
judgement's explosion, a missed tap going by, holds kept and dropped, a
mine going off) are a play's; it loops,
numbering events on so a skin's actors carry over. It runs on any
built-in layout (`layout`): the four-lane phrase on dance-single, a
generated one on the others (dance-double, solo, Dancing☆Onigiri's key
modes). `rows` and `speed` size it. The preview now draws the field
through the play's own scene path (`crates/render/src/scene.rs`, stopping
before the HUD), so what a skin leaves to the built-in arrows shows, and
`skin=builtin` previews the built-in arrows themselves, in the
`ColorScheme` given.

**Done 2026-10-13**: every judgement's explosion, a miss, kept and dropped
holds and rolls, and mines hit, on every built-in layout (native tests); checked in headless Chrome
(WebGPU, WebGL2) with CF_CHROME, Delta, Dito and Peter Lawrence's old-
format skins.

## Step 4. Previews in Options

`PreviewCanvas` (`crates/web/src/components/skin_preview.rs`) is the page's canvas
as a component: it keeps its device and renderer and loads another skin
when its options change (an older load gives way to a newer one). (Superseded 2026-10-16: Options is now
the theme's screen on the canvas and draws its preview with
`crates/render/src/field_preview.rs`, `docs/plans/ui-core.md` 6.3;
`PreviewCanvas` serves `#/skin-preview` alone.) Under
the Arrows choice, Options showed the arrows a play would draw (the chosen
skin, or Dito in the note colors, or the built-in arrows) playing the
demo, with a layout to preview it on: since 2026-10-13 a skin draws
Dancing☆Onigiri's key modes too (`docs/plans/danoni-skins.md`); the
built-in arrows draw them when chosen.

A hidden page counts as paused, and a jump in time (more than a quarter
second) catches the demo up without its events, which a skin's Lua would
otherwise take in one frame and stop on. The canvas is sized for the
screen's pixel density, labelled for screen readers, and still (the
arrows on their receptors) when the system asks for reduced motion.

**Done 2026-10-13**: checked in headless Chrome (WebGPU) and Firefox
(WebGL2), with leaving and re-entering Options while it loads.

## Later

- A preview per skin in the list rather than of the one chosen (one
  canvas each costs a GPU device; a shared offscreen renderer could draw
  them all).
- Lifts and fakes in the demo.
- A native renderer for the same frames (wgpu off screen), so reference
  checks can run in `cargo test` on local renders.
