# Noteskin previews (2026-10-13)

A page that draws a noteskin's elements alone, as a play draws them: first
to check skins against reference renders, later to preview skins where
they are chosen.

## Step 1. The preview page

`#/skin-preview?skin=<key>` (`app/src/components/skin_preview.rs`): the
skin's Down tap note (`notes=4`: the four arrows; `receptors=1`: their
receptors too) on a canvas, `size` pixels an arrow, over `bg`, held at
`beat` or running at `bpm`, on WebGPU or (`gfx=gl`) WebGL2. It goes through
the play's own path: the skin is readied as for a play
(`noteskins::prepare`), its textures uploaded by the game loop's function
(`game_loop::upload_noteskin`, shared), and drawn by `Renderer::render`
with `RenderOptions::preview` (`render/src/scene.rs`: the skin's elements
alone, placed by the given `FieldGeometry`), so blending, models, mipmaps
and WebGL2's offscreen frames are the play's. A test sets
`window.__DDI_PREVIEW_BEAT` and waits for `window.__DDI_PREVIEW_DRAWN`.

**Done 2026-10-13.** With it, `FieldGeometry::lane_receptor_y` follows the
geometry's `receptor_y` (it assumed the play's placement; the same in a
play).

## Step 2. Peter Lawrence's renders as references

His repository's README shows a GIF of each SM5 skin's Down tap note:
128 px (the skin's 64 px at 2×), 60 frames a beat, a 4-beat loop (2 for
CF_ALLOY and CF_VIBRANTALLOY), a key colour as transparency (red, or white
for the ALLOY ones). Drawn by the preview at the same beats and compared
frame by frame over the GIF's opaque pixels (scripts kept local; the GIFs
are not in the repository).

**Done 2026-10-13, as a check.** Shapes, sizes, textures, colours and their
animation match: CF_CHROME, CF_VIBRANT, CF_ALLOY, PETERS_METAL, PETERS_CEL,
Peters-DDR-Rainbow line up at the same beats; Peters-DDR-Note and
Peters-DDR-Vivid line up 16 frames (0.267 beats) apart, which one time
offset explains for both (their scrolls and Vivid's colour cycle shift
together), while Peters-DDR-Rainbow, whose model scrolls the same texture
the same way, lines up exactly: their renders started elsewhere. Our
arrows sit 1 to 4 pixels lower than in the renders (by skin; the renders'
framing, or StepMania's perspective on a model near the top of its
screen); shifted to match, the mean difference over the arrow is 13 to 22
of 255, much of it the GIFs' 256-colour dithering.

## Later

- Previews in the skin chooser (Options): a row of arrows per skin,
  animated, drawn by this page's path into a small canvas.
- Holds, mines, lifts and explosions in the preview.
- A native renderer for the same frames (wgpu off screen), so reference
  checks can run in `cargo test` on local renders.
