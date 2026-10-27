# Controller button material study

Original hardware-inspired geometry, 2026-10-27. This is an exploratory visual
study for [controller hints](../../docs/plans/controller-hints.md). The selected
shared-satin bodies now supply the runtime atlas. It uses the keyboard study's monitor lighting and camera
angle to bring controller buttons into the same visual environment.

## Direction

- Xbox: shallow crowned charcoal buttons, glossy rim and colored lettering.
- PlayStation: flatter faces and colored symbols, with a pale, dark-ink
  DualSense-inspired alternative. The preview can switch between them.
- Nintendo: taller satin barrels, light letters and a conventional cross d-pad.
- Generic standard mapping: a small button diamond with the relevant physical
  position highlighted. Neutral START / SELECT caps replace the rejected
  left/right center-position markers. These are legacy naming conventions,
  not a claim that an unknown device prints those words on its controls.
  Unknown raw controls use a numbered cap, illustrated with button 7.

The second study adds family-specific shoulders and triggers: swept Xbox
bumpers and LT/RT, rounded PlayStation L1/R1 and deeper L2/R2 faces, and
Switch Pro-inspired shallow L/R with broad ZL/ZR rear paddles. Left/right
silhouettes mirror each other while printing remains readable. Generic
LB/RB and LT/RT are neutral conventions. They do not add trigger bindings.

The third study reduces generic START/SELECT printing and the caps' width,
adds an actual-size center-button sample, and replaces the raw numbered keycap
with a round button. Xbox, Nintendo and generic bumper legends use smaller
optically placed printing with margins on both sides; PlayStation's spacing
is retained. Classic PlayStation colors are more saturated, with reduced ink
specularity so the monitor reflection does not wash out their color.

The material comparison switches between the hardware-inspired treatments
and **shared charcoal satin**: one body color (#383a3d), roughness (0.36) and
coat weight (0.08) for all controller bodies, bumpers and triggers. Shapes and
colored legends retain family identity; neutral printing becomes light on the
common dark body. The common material is the selected runtime and preview default. Keyboard caps remain unchanged; sharing a satin
finish need not require giving keyboard and controller bodies the same color.

The fourth study establishes optical size categories in `proportions.json`
instead of giving every art box the same height. Relative to the 26 px base:
face/raw buttons use 0.85 (22.1 px), d-pads and generic direction clusters use
1.35 (35.1 px), and generic START/SELECT use 0.65 (16.9 px). Shoulders, triggers
and model-specific center controls retain the base size. The word caps are
about 35.5 px wide versus 41.6 px for shoulders, including transparent margins.
This is a visual hierarchy, not controller-scale geometry. The same factors
apply to enlarged specimens, actual-size footers and the static proof, so
directions remain visibly larger and generic word caps smaller in every view.

All specimens share a near-overhead camera and light direction. Their material,
silhouette and printing distinguish them. The preview compares enlarged
specimens with 26 px footer hints and the existing deeper-gray keyboard cap;
hint height, enlargement, PlayStation treatment, background and a grayscale
check are adjustable. Footer pairs retain the game's 18 px inter-hint spacing.
The enlarged comparison includes faces, center controls, shoulders, triggers
and the combined d-pad. Generic center controls and the raw button also have
an explicit actual-size row beneath the footer.

D-pad direction sets share the same art box: Up, Up/Down, Left/Right and all
four directions. Bright chevrons show the available directions. These mean
alternatives, not simultaneous presses. The song-list example uses all four
for Navigate. Production can represent a direction mask on one body; only
directions resolved to valid bindings on that same controller may be combined.
Keep repeat counts/holds outside the glyph, and do not collapse a d-pad and
stick binding into one invented control.

The footer illustrates the settled **physical** defaults: South plays, East
goes back, West opens More, and shoulders change charts. Thus Nintendo shows
B / A / Y for those actions. These are design examples, not a runtime binding
resolver. Production hints must come from effective bindings, and a family
override cannot establish physical positions for an unmapped device.

## Song-list and pre-start context study

`context.html` and `pack_context.py` place the accepted shared satin and optical
proportions into the current song-list footer and pre-start layout. The local
captures `target/scratch/controller-hints/context-select.png` and
`context-prompt.png` come from the web development build at 1280 × 720; the
capture routes are `#/ui-dev?screen=select` and
`#/play?song=some-things-must&chart=5`. The song list remains a captured
backdrop, while the footer and prompt card are
inspectable mock UI. Receptor outlines are extracted from that actual skin
preview. The glow is an illustrative press cue, not the skin renderer running
inside the mockup. These captures and generated button renders are local
prerequisites, not source assets or runtime integration.

The revised Dance Single diagram proposes LB / LT / RT / RB for Left / Down /
Up / Right (L1 / L2 / R2 / R1 on PlayStation; L / ZL / ZR / R on Nintendo).
D-pad directions and West / South / North / East face bindings remain selectable
alternatives. This is now the engine default for unsaved/reset Dance Single tables. Existing
saved/cleared bindings remain unchanged.
Family selection changes the printed legends, not those physical positions.
Individual left/down/right masks were added to the generator alongside up and
the existing direction sets, preserving the light direction instead of rotating
an up-arrow image. The live footer uses four directions for Song, paired
shoulders for Chart, North for Style, South for Play, West for More, East for
Start menu. These reflect defaults; the runtime resolves saved bindings through the same
dispatcher used for input.

Proposed press treatment: **1 px downward screen-space travel at the 26 px
base**, adjustable from 0 to 2 px in the context study. The keyboard currently
travels about 1.85 px at that size. This is an optical animation choice, not a
physical switch-travel measurement. Face buttons move as one face with an 18%
brightness reduction. D-pad prototypes keep the body stationary and shift/dim
only the active arm. The runtime atlas uses layered arm cells and the same
optical depression cue; a mechanically modeled hinge remains a possible refinement.
Bumpers use the same straight depression. Triggers add an illustrative 8°
hinge rotation at 1 px travel, around the top of their image; this CSS projection
is a state-design approximation, not a measured mechanical travel or a new 3D
render. Travel scales with rendered size, not family-specific real-world
millimeters.

The one-shot demo follows the existing 200 ms lead-in, 140 ms lane cadence and
110 ms depressed time, then demonstrates Left+Right and Down+Up for 220 ms each
before a one-second pause. It starts only on request and can
be stopped; clicking a binding holds/releases its press for inspection. Tweak
selects family, primary/alternative bindings, travel and a held lane, opposite
pair or all four lanes. The mockup
does not change the runtime's reduced-motion behavior.

Package after generating both material sets and capturing the two screens:

```sh
python3 tools/controller-design/pack_context.py /absolute/path/controller-context-study.html
```

## Modeling and light

`generate.py` builds every mesh, material and printed stroke from scratch.
Dimensions and finishes are visual approximations, not measured manufacturer
tooling. No vendor models, textures, photographs or fonts are included.
Original authoring sources follow the repository's MIT license; the preview
uses the existing bundled Iosevka fonts under their recorded OFL license.

The light matches the keycap B+D study: a 60 × 34 cm rectangular emitter at
(0, 60, 27) cm, tilted back 15°, 4.5 Blender watts, plus a neutral world at
0.08 strength. The camera looks from 38 mm forward and 85 mm above the target.
These are illustrative scene settings, not a calibrated monitor luminance.
Cycles renders 64 denoised samples with transparent backgrounds, AgX and a
deliberate +1.3-stop exposure lift for small UI contrast.

Optical adaptations are intentional:

- Lettering is enlarged and thickened for footer sizes; it is flat surface
  printing, not embossed geometry. Stroke meshes do not cast shadows.
- Shoulders/triggers now have different family silhouettes informed by the
  official references below. The projected contact faces, bevels and enlarged
  printing remain optical simplifications, not exact molded components.
- D-pad direction chevrons are UI cues, not a claim that the physical button
  bears that printing.
- PlayStation center pills carry abstract menu/share marks on their faces.
  These are exploratory UI substitutions for markings often placed on the
  controller housing, **not accurate model-specific Options/Share/Create
  legends**. Resolve that treatment before shipping; uncertain models should
  use neutral center-position artwork.
- Generic positions are designed symbols, not a fictitious hardware family.

The current set has 71 specimens: four face positions, two shoulders, two
triggers, two center controls and four d-pad direction sets on each of five
material treatments, plus a raw cap. Stick movement/clicks, remaining direction
masks, pressed states and arbitrary raw labels still need artwork before
complete runtime coverage.
Dance-pad profiles remain later work.

## Reproduce

Use a **separate background Blender process**. Do not load factory settings or
these scenes into an interactive Blender session belonging to the maintainer.
Blender 5.2.2 was used for this study; packaging needs Python 3 and Pillow.

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --factory-startup -t 4 --python tools/controller-design/generate.py
/Applications/Blender.app/Contents/MacOS/Blender --background --factory-startup -t 4 --python tools/controller-design/generate.py -- --material shared
python3 tools/controller-design/pack.py /absolute/path/controller-button-study.html
```

Other systems can substitute their Blender executable. `generate.py --
--quick` renders only the South specimen per family; run the full generation
before packaging. `-- --only generic-start,generic-back` rebuilds the complete
scene/manifest while rendering only those specimens, useful after a local edit.
`-- --exposure 1.3` controls the exposure lift.

Generated files stay in ignored `target/scratch/controller-hints/`:

- `controller-buttons.blend`: editable specimens arranged in named collections.
- `<family>-<control>.png`: centered, consistently lit transparent renders,
  384 px high (shoulders have wider art boxes).
- `study.json`: specimen inventory and lighting settings.
- `material-study.png`: static proof showing both PlayStation treatments.

The shared-material run writes its own 86 renders, manifest and editable scene
under `shared/`, without replacing the hardware-inspired reference renders.
Packaging requires both material runs.

`pack.py` embeds 144 px WebP versions of both materials and bundled font subsets into the
`study.html` template, producing a self-contained inline comparison below 1 MB.
The resulting preview is a design review aid, not a shipped atlas. Follow the
keycap workflow when the look is selected: derive compact reusable bodies and
optically corrected legends, then validate their actual in-game sizes.

## Visual references

Official product photography inspected for silhouettes, materials and legends;
all accessed 2026-10-27. References inform the approximations above and are not
bundled in the source or preview.

- [Xbox Wireless Controller](https://www.xbox.com/en-US/accessories/controllers/xbox-wireless-controller)
- [DualSense wireless controller](https://www.playstation.com/en-us/accessories/dualsense-wireless-controller/)
- [DUALSHOCK 4 wireless controller](https://www.playstation.com/en-us/accessories/dualshock-4-wireless-controller/)
- [Nintendo Switch Pro Controller](https://www.nintendo.com/us/store/products/pro-controller/)
- [Sony's controller diagrams](https://www.playstation.com/en-us/support/hardware/ps5-button-functions/): front and top/side views distinguish the bumper and trigger faces.
- [Nintendo's Pro Controller diagrams](https://www.nintendo.com/au/support/articles/nintendo-switch-pro-controller-diagram/): the back view shows ZL/ZR separately from L/R.

## Official presentation guidance checked

Checked 2026-10-27. Separate public accessibility recommendations, implementation
examples and hardware references from a normative console glyph specification.
This study does not claim vendor approval or console-certification compliance.

- Microsoft's [XAG 101](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/101)
  applies minimum text sizes to the **printing inside glyphs**, not just their
  bounds: at 1080p, 18 px for PC/VR and 26 px for console; support scaling to
  200%. Our 26 px total icon height has smaller printing and is a compact
  styling comparison, not evidence of meeting that recommendation. Hint
  scaling must include both glyph and adjacent action text.
- [XAG 102](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/102)
  recommends 4.5:1 contrast for standard-size meaningful text/visual elements,
  3:1 for large elements, and 7:1 in high-contrast mode. On shaded backgrounds,
  measure against the lowest-contrast area. Grayscale is a useful visual check,
  not a substitute for measuring contrast in the final drawn assets.
- Microsoft's [DirectXTK ControllerFont example](https://github.com/microsoft/DirectXTK/wiki/ControllerFont)
  shows controller artwork composed into text, with distinct shoulder/trigger
  and d-pad glyphs. It is an official implementation example, not a universal
  style mandate for all Xbox button art. No assets were imported from it.
- Sony's [partner onboarding](https://sonyinteractive.com/en/news/blog/showing-your-game-to-playstation/)
  describes documentation access after registration approval and agreement.
  Nintendo's [developer process](https://developer.nintendo.com/the-process)
  places developer information behind registration/NDA and separately approved
  Switch access. No public Sony/Nintendo in-game button-art specification was
  located in this review. Their public hardware diagrams establish labels and
  shapes, but do not establish rules for reproducing or stylizing glyphs.

Current priority, clarified by the maintainer: resolve the larger UX hurdles
first. Retain recognizable legends, distinguish bumpers from triggers and
keep shading quiet behind printing. Dedicated accessibility sizing, scaling
and high-contrast work are deferred; the recommendations above are background
research, not acceptance criteria for this design pass. Any future console release should
check the applicable partner specifications rather than treating this public
research as certification guidance. No virtual-keyboard work is added by this
research; the existing free-text exception stays in scope as agreed.


## Runtime atlas

`assets/ui/controllers.png` is the selected shared-satin runtime asset, built
from our own procedural meshes and printing under this repository's MIT license.
No manufacturer artwork is copied. The PNG has 29 columns × 10 rows of 64 px
cells (1856 × 640): Xbox, classic PlayStation, DualSense, Nintendo and Generic,
each with released and pressed rows. Columns 0–9 are face/shoulder/trigger/
center controls, 10–11 blank stick-click bodies, 12–27 all D-pad direction masks,
and 28 the blank round raw-control body. The theme adds live stick/raw labels
and preserves each category's optical aspect and size. Long axis/hat labels
sit beside the round body instead of being shrunk into it.

After the full shared-material generation above, rebuild with:

```sh
/Applications/Blender.app/Contents/MacOS/Blender --background --factory-startup -t 4 --python tools/controller-design/runtime_bodies.py
python3 tools/controller-design/export_runtime.py
```

The first command renders neutral D-pads/raw bodies in an isolated background
scene. The second downsamples and packs the selected renders and active arms.
Pressed cells move/dim the face by about 1 px at display size; triggers compress
slightly around their upper edge, and D-pads change only selected arms. This is
an optical state cue, not a physical travel measurement. Blender and Pillow
are not needed to build or run the game. The historical HTML mockups remain
available for comparing materials, proportions, binding alternatives and jumps.

`crates/desktop/tests/controller_hints_gpu.rs` renders the actual theme and
skin preview. Set `DDI_CONTROLLER_PROOFS` to an ignored output directory to
save its family comparisons; use an isolated `DDI_HOME` for this test.
