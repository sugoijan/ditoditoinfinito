# Infinito shortcut keycaps

Original OEM-inspired UI artwork (2026-10-26). Squared shoulders, tapered
walls, a cylindrical scoop and a near-overhead camera replace the earlier
OSA study's rounded bowl. Deeper neutral gray satin contrasts with Infinito's dark plum plates.
Named legends are lowercase and single-letter keys uppercase. Live legends
use the theme text face at 10 px, Regular Italic, with an
upper-left inset and a shallow surface bend through the glyphs. Enter and
Tab include original stroked symbols. Space has a separate convex crown,
a 52 px minimum width and a centered label. Caps are 26 px tall; Tab, Enter and Space have wider
minimums, and arbitrary labels can still widen their cap.

The game atlas uses the home-row shape. The study compares
four rows under the same camera and light, with side views: function/number,
QWERTY, home and lower/arrow rows. We number those R1–R4 from top to bottom;
some manufacturers reverse the numbering. Their heights and tilts are
original visual approximations, not measured Keychron tooling or printable
parts. The cylinder's axis runs front to back; its dish varies across x.

References:

- [The maintainer's Keychron OEM set](https://keychron.it/en/collections/all-accessories/products/iso-dye-sub-oem-pbt-keycap-set): the squared silhouette and restrained, upper-left printing.
- [Keychron's OEM overview](https://github.com/Keychron/Keychron-Keyboards-Hardware-Design/blob/main/Keycap%20Profiles/OEM%20Profile/README.md): cylindrical, sculpted rows with varying height and tilt.

No manufacturer CAD, textures or photos are bundled. All generated assets
and cap-body sources are this project's own MIT work, covered by `REUSE.toml`.
Iosevka lettering uses the SIL Open Font License (the font provenance records
the pinned source and subsets).

## Current SVG bodies

`python3 tools/keycap-design/charcoal_vector.py` writes
`assets/ui/keycaps-a1.svg` and `keycaps-a2.svg`, preserving both shortlisted
charcoal looks under B+D lighting. These contain only original paths and
linear gradients, no embedded bitmap or text. The theme's live text/marks
supply the printing, preserving its legibility. The highlights are a vector
approximation fitted to the rendered references, not a physical shader.

The generator accepts arbitrary body widths and a pressed fraction, and
saves standalone released/pressed SVGs under `target/scratch/keycaps/svg/`.
The app's existing sprite bridge currently uses four width presets plus
Space, scaling the nearest width for intermediate labels. Exact runtime
width-dependent geometry is not yet part of this bridge. Both shells now
load checked SVG UI textures at the requested display size: native resvg,
web SVG worker with browser fallback. Text remains live; deeper gray satin
is the selected default and `Infinito::set_keycap_material(KeycapMaterial::SemiGloss)` selects
A2 before rebuilding the screen. A2 remains available as an alternative.

`assets/ui/keycaps-light.svg` provides neutral gray satin (`#a2a2a2` face),
with `keycaps-gray.svg` a slightly darker alternative (`#858585`). Both use
neutral RGB values and `#111111` ink for all printed text and keyboard marks.
`KeycapMaterial::LightSatin` / `GraySatin` select them. The final app default
is `DeepGraySatin`, using `keycaps-deep-gray.svg` with a `#7d7d7d` face and
the same near-black ink. All presets remain available to authoring tools.
`light_study.py`, run in a separate background Blender process, saves an
unprinted material reference under `target/scratch/keycaps/light/`, opening
only the existing B+D study. The SVG lifts its diffuse gray values to retain
contrast for small black printing; this is an intentionally brighter UI
interpretation of that dimly lit physical reference. It keeps the same
geometry, satin highlights and convex Space crown.

`preview.rs` is the reusable offscreen capture fixture: copy it temporarily
into `crates/desktop/tests/keycap_preview.rs`, build that test with
`DDI_IMPORTER_TEST_ONLY=1 cargo test -p ddi-desktop --no-default-features
--test keycap_preview --no-run`, then run its executable through the repository
watchdog with an isolated `DDI_HOME` and `DDI_OFFSCREEN_PPM` naming its output.
Omit `DDI_KEYCAP_MATERIAL` for the app default, or set it to `A1`, `A2`, `LIGHT`
or `GRAY` for an earlier alternative. Remove the temporary test afterward.
The fixture draws the actual sample song-list UI at 4× without a window.

Actual offscreen song-list captures of both variants are saved under
`target/scratch/keycaps/svg/`, including full 4× captures and enlarged footer
comparisons. The captured UI still has its original six song-list hints;
Space and pressed poses are retained as standalone SVG references for future
uses. The previous bitmap/Blender studies below remain reproducible history.

## Shortcut sizing playground

Song and Chart now use separate ↑ and ↓ caps; Chart's `2×` is a plain
prefix outside the keys. Navigation behavior is unchanged. `playground.html`
is the literal interactive study source, with the same body presets, embedded
Iosevka faces and shallow printing curve. Small print uses projected font
outlines to avoid browser antialiasing seams. To fill its SVG/font placeholders
(Python with FontTools):

```sh
python3 tools/keycap-design/playground.py /absolute/output/keyboard-hint-playground.html
```

The conversation host supplies size/spacing controls and a finish picker.
The selected Satin settings, also used in the app, are 26 px cap height,
10 px legends, no extra gap between keys and 18 px between shortcuts. Widths
retain enough room for the independent legend size as caps shrink.

Regular Italic (400) from the pinned Iosevka Term 34.9.0 release is selected,
registered on both shells, and used by the runtime's key legends. Other
weight/style subsets are retained for reproducing earlier studies. The
playground retains the previous gray satin (`#858585`) alongside the selected
deeper `#7d7d7d` app default, with identical print
metrics and key geometry. Enlarged 3× samples expose the letterforms and
shallow surface curve. The
earlier 32 px comparison remains in the previously delivered conversation
preview. The extra specimens include letters, horizontal arrows and Space.

## Reproduce the original Blender study

Use a **separate background Blender process**, never the maintainer's open
project. Blender 5 and Python 3 with Pillow and FontTools, from the repository root:

```sh
blender --background --factory-startup --python tools/keycap-design/generate.py
python3 tools/keycap-design/pack.py
python3 tools/keycap-design/oem_vector.py
```

The generator writes 16 transparent renders (four widths per row), four
side views, a convex spacebar and its side view and an editable `oem-keycaps.blend` to
`target/scratch/keycaps/oem/`. The packer writes `row-study.png` there and
the game's `assets/ui/keycaps.png`: five 256-pixel-high rows in a 768×1280
atlas, with image widths 256, 352, 448, 544 and 512 (Space). The empty margins protect
antialiasing and mipmaps. Blender and Pillow are authoring dependencies only.

`Infinito::key_hint` chooses the nearest width and draws live text over
it. `Look::image("KeyHint")` supplies the atlas, bound to `Cap0`, `Cap1`,
etc. by the shared screen helper. Both shells draw ordinary cached sprites. Legends are shaped once and
remain live text; `TextCurve` bends narrow adjacent pieces of the cached
run in the shared painter, keeping the font cache independent of cap width.
A small solid plate under each cap keeps the legend legible during loading.

## Surface check

`prepare_reference.py` creates identical theme-font decals for the comparison.
`surface_reference.py`, run in background Blender, opens **only our generated
study** and conforms subdivided decal meshes to the actual cylindrical top.
It saves a separate `printed-surface-study.blend` with packed decal textures,
four printed renders, blank bodies and `surface-measurements.json`.
`compare_surface.py` makes the side-by-side sheet: previous bend, Blender
surface, and the matched 2D bend. Their font and starting position match;
Blender's material/view transform makes the ink slightly darker.

```sh
python3 tools/keycap-design/prepare_reference.py
blender --background --factory-startup --python tools/keycap-design/surface_reference.py
python3 tools/keycap-design/compare_surface.py
```

The camera sits 38 mm forward and 85 mm above its target. A 1 mm dish
therefore projects to `32/23 * 38/hypot(38,85) = 0.568` UI pixels.
The top's half-width comes from each selected mesh and its horizontal atlas
fit. The visible drop from the inset to the center is about 0.45–0.50 px,
instead of the previous hand-tuned 1.0–1.25 px. The runtime and SVG study
use this measured projection. The comparison keeps the decal's nominal
projected font size constant to isolate the bend.

## Vector direction

`oem_vector.py` produces `vector-study.svg` and four standalone row SVGs.
These contain paths, gradients and outlines from the bundled Iosevka face, with no embedded raster art.
The width is a geometry parameter: corners, sidewall insets and legend size
stay constant when a label needs more room. The SVG shading is deliberately
simpler than the Blender rendering, a separate design proof for comparison.
The legend outlines bend with the same curve; Space switches to a convex
highlight gradient. The study uses FontTools to extract the bundled face,
so key legends do not depend on system font discovery.

The game still uses the rendered atlas while the look is being adjusted.
An eventual runtime port should construct the same width-dependent shapes
through the shared painter (or cache generated SVGs by size and row), not
stretch a single square image. The UI painter currently has rounded quads
and gradients through corner colors, but not arbitrary SVG paths. A full
port therefore remains a distinct renderer change after the look is chosen.

The previous spherical study is preserved as `osa.py` and `osa_pack.py`;
its scratch outputs remain under `target/scratch/keycaps/`.


## Key identity and custom printing

The shared UI accepts `Keycap::Key(KeyboardKey::…)` or
`Keycap::custom(label, KeyShape::Normal | KeyShape::Space)`. For example:

```rust
use ddi_ui::keycaps::{Keycap, KeyShape, KeyboardKey};
let caps = [
    Keycap::Key(KeyboardKey::Enter),
    Keycap::custom("jump", KeyShape::Space),
];
// screens::key_hint(env, "Hint", &caps, "Action", cx)
```

Specific keys choose their shape, case, marks and alignment. Custom printing
preserves the supplied spelling and uses only the requested shape: a normal
cap labeled `SPACE` stays normal, and a custom `enter` gains no Return mark.
`Spec::with_keycaps` supplies typed data plus text labels in `items` for older
looks. Existing footer strings pass through `Keycap::from_hint`; composite
captions such as `2×↑↓←→` remain one custom normal cap. Even these composites
use the same drawn arrows instead of the unequal font glyphs.

### Arrow placement references (2026-10-26)

The manufacturer's [Cherry G80-3000 photo](https://www.cherry.de/en-us/product/g80-3000)
([original grey model image](https://www.cherry.de/fileadmin/assets/bf8fd96aaf68a7b85b28954c7a60c98b.jpg))
shows horizontal arrows closer to the center and vertical arrows slightly
left of center. This is a visual observation, not a published dimension or
alignment standard. [Cherry's history](https://www.cherry.de/de-de/unternehmen/ueber-cherry)
dates the G80-3000 family to 1988; that supports using it as a traditional
Cherry reference, not a claim that its printing stayed identical over time.

The [Keychron Hacker OEM photos](https://www.keychron.com/products/oem-dye-sub-pbt-keycap-set-hacker)
show a more consistent upper-left bias. These examples do not establish a
market-wide frequency or one universal convention. The requested PCMag
review was blocked by browser safety policy; its page was not inspected.

Infinito follows the Cherry-style mixed alignment: horizontal arrows at
x=16 and vertical arrows at x=12 on a 32 px cap, all at the same y. A single
8-unit arrow with a 6-unit head span is rotated for every direction. Surface
bending stays shallow; font metrics cannot stretch one orientation.

## Dark material comparison

`material_study.py` is an isolated authoring study; it never writes the game
atlas or theme. It renders three darker base colors (charcoal, deep plum,
ink blue) crossed with satin, semi-gloss and gloss, all under the same
neutral softbox/strip lighting and camera. The previous plum material is
rendered under that same rig as a reference. Each tile includes the existing
home-row E, Enter and convex Space, with actual surface-conformed printing.
These are visual plastic finishes, not measured commercial material presets.

```sh
python3 tools/keycap-design/material_sheet.py prepare
blender --background --factory-startup --python tools/keycap-design/material_study.py
python3 tools/keycap-design/material_sheet.py
```

The nine PNGs, comparison sheets, packed `dark-material-study.blend` and
`materials.json` with the exact linear RGB/roughness/coat parameters are
saved under `target/scratch/keycaps/materials/`. Letter A/B/C identifies the
color; digit 1/2/3 identifies the finish. All nine materials remain in the
saved Blender file; it opens with B2 assigned as a starting point for review.
Selection and any eventual runtime/vector port are separate steps.

## Monitor lighting comparison

`monitor_study.py` opens only the saved dark-material study in a separate
background Blender process, converts its numeric millimeters to meters, and
replaces the studio lights with a uniform white rectangular area emitter.
The panel is 60 × 34 cm, upright and facing the player, centered 40 cm behind
the keys and 27 cm above the desk (bottom edge 10 cm above it). Its 4.5 W
Blender energy is an illustrative brightness, not calibrated display nits.
The camera/viewing angle, cap geometry, printing and B1–B3 materials stay fixed.

The comparison renders the monitor alone in a black world, then the same
monitor with faint neutral world illumination (strength 0.08). The previous
studio renders form the reference column. The front walls naturally receive
very little light from the screen alone; the convex Space crown still reflects
the screen toward the camera. An initial lower-brightness study shifted the
panel 15 cm left: the large emitter's resulting change was small. That local
first pass is retained under `monitor/initial-monitor-only/`.

```sh
blender --background --factory-startup --python tools/keycap-design/monitor_study.py
python3 tools/keycap-design/monitor_sheet.py
```

The outputs and packed `monitor-light-study.blend` go into
`target/scratch/keycaps/monitor/`, with exact rig settings in `lighting.json`.
The saved scene opens on B2 with the screen alone. This is an authoring
comparison; it changes neither the open interactive Blender project nor the
app's material/atlas.

## Monitor position sweep

`monitor_position_study.py` keeps the accepted faint room light (0.08),
monitor brightness (4.5 W), 60 × 34 cm panel, B2 deep-plum semi-gloss material,
camera and exposure fixed. It compares the previous 40 cm-back / 27 cm-high
screen center with ±15° tilts, 60 cm distance, 20 cm center height, and the
farther/lower combination. Positive tilt means the screen top leans away
from the player; it rotates around its center. Heights are above the desktop.

```sh
blender --background --factory-startup --python tools/keycap-design/monitor_position_study.py
python3 tools/keycap-design/monitor_position_sheet.py
```

Outputs go under `target/scratch/keycaps/monitor-positions/`. The saved
`monitor-position-study.blend` retains candidates A–F as frames 1–6 with
named timeline markers; `positions.json` records the exact settings. The
images retain their rendered brightness, so distance also changes the
illumination received by the caps. No material or app asset is changed.

The follow-up `monitor_blend_study.py` compares the midpoint of B and D
(50 cm behind, 27 cm high, 7.5° back tilt) with their combination B+D
(60 cm behind, 27 cm high, 15° back tilt). Run it in background Blender,
then run `python3 tools/keycap-design/monitor_blend_sheet.py` to compare
both against B and D. Renders, exact settings and a separate saved Blender
file for each candidate live in `target/scratch/keycaps/monitor-blends/`.
All other settings remain those of the faint-room-light position sweep.

## Material comparison under selected B+D lighting

The maintainer selected B+D as the lighting for material review: screen
center 60 cm behind the keys, 27 cm above the desk, tilted back 15°, with
the same faint room illumination. `bd_material_study.py` opens that saved
study and renders the original nine materials (unchanged RGB, roughness
and coat values), plus the previous plum reference, under this fixed rig.

```sh
blender --background --factory-startup --python tools/keycap-design/bd_material_study.py
python3 tools/keycap-design/bd_material_sheet.py
```

The grid retains A/B/C color rows and 1/2/3 finish columns. Outputs, settings
and the packed `BD-material-comparison.blend` live under
`target/scratch/keycaps/bd-materials/`. The file opens on B2 for inspection;
that is not a final material selection. The app atlas stays unchanged until
a material is chosen and the look is ported.

## Pressed references for A1 and A2

`pressed_study.py` records the two shortlisted charcoal finishes under B+D
plus faint room lighting, at 0/1/2/3/4 mm travel. Four millimeters is this
study's chosen full travel, not a claim about every keyboard switch. Each
cap and its surface printing translate together along world −Z; the cap
geometry, material, camera and lighting stay fixed. No keyboard deck is
introduced, so the study records translation and lighting changes without
inventing a frame that occludes the lowered cap. Individual E, Enter and
Space press renders keep their neighboring caps released.

```sh
blender --background --factory-startup --python tools/keycap-design/pressed_study.py
python3 tools/keycap-design/pressed_sheet.py
```

`target/scratch/keycaps/pressed/` contains transparent PNG poses, released/
pressed and individual-press contact sheets, GIF and lossless APNG loops,
`travel.json` with measured camera projection, and separate packed Blender
files with editable press/release animation for A1 and A2. The illustrative
loop uses actual rendered 1 mm steps; its timing is not switch dynamics.
The faint rules in the loop are stationary presentation guides outside the
cap artwork. Full travel projects to about 19.13 pixels in the 1500-pixel
render, equivalent to 2.27 pixels on the existing 32 px / 23 mm UI artboard.
A future tutorial can use the reference for movement independently of any
additional active-state treatment. The runtime remains unchanged.
