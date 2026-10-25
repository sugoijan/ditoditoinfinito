# Dosis title study

An editable design study for **Dito Dito Infinito**: Dosis ExtraBold (800),
with Infinito's Vivid paint confined to the outer outlines of the D, D and I.
The other letters and the initial fills use the theme's warm white. An
optional hollow treatment uses dark initial fills. The comparison menu line
is Inconsolata Regular, the maintainer's intended UI face.

Open `index.html` in a browser. Everything is local; fonts are already shaped
to paths, so the preview needs no installation, network, server or build.
Adjust the outline, gap, initial fill and palette phase. The outline grows
outward from the gap, up to 24 units (one quarter of the capital height);
the gap ranges from 0 to 12 units. The colored outlines are one background
layer. A transparent clearance mask protects **all** letters, including the
adjacent `i`s, the `n`, counters and dots, before the foreground letters are
drawn. Growing an initial's outline can therefore carry it behind a neighbor
without touching or covering that neighbor. Padding accommodates the maximum
outline plus gap without cropping or changing the title scale.

The title animates automatically: both the palette and the texture folds
evolve. **Pause motion** stops both; reduced motion starts it still.
**Save SVG** exports the current title, including its exact color snapshot.
`title.svg` is the default study's exported snapshot. It has vector letter
shapes and masks, with small embedded procedural color-field PNGs. It has a
transparent background and light lettering intended for the dark theme.

The game uses the accepted filled title with outline 12 and gap 12.
`crates/theme/src/wordmark.rs` supplies its atlas layers; the shared painter
runs Infinito's actual WGSL material, with a four-second palette cycle and
unwrapped time for the flowing texture. Reduced motion freezes both, and
the start menu stops requesting animation frames. Other UI fonts are unchanged.

## Relationship to Infinito

Based on `main` at `a285cc84`, specifically:

- `assets/noteskins/dance/Infinito/_spiral components.svg`: the eight-stop
  closed cyan → indigo → purple → pink → orange → yellow → green → cyan palette.
- `_spiral material.wgsl`: evolving domain-warped noise and the closed palette
  window. `preview.js` translates its Vivid branch, with scale 0.021, warp
  1.35, evolution speed 0.3, contour density 2, palette span 0.43 and seam
  blend 0.5. It uses the same hash and smooth noise as
  `crates/render/src/material_prelude.wgsl`.
- `_spiral component.lua`: phase begins at 0.626 and moves backward one
  palette cycle in four beats. The title preview uses 60 BPM (four seconds).

Title-specific choices: normalize each 96-unit capital to a 64-unit skin
field; offset D/D/I by 0, -0.25 and -0.5 palette turns; start the
spatial field at four seconds, then let it evolve. The accepted defaults are
12-unit outline and gap, filled initials, and starting phase 1. There are no note
timestamps in a title. These phase offsets are a design choice, not the
game's subdivision clock. Changing the palette phase does not reset the
spatial field. CPU double precision and analytic sRGB ramp interpolation
approximate the GPU's float math and sampled palette texture, so this is a
behavioral port, not a pixel-identical renderer capture.

## Editing and reproduction

`preview.html` contains the design surface and controls; `preview.js` the
paint and rendering. `generate.py` shapes both fonts with HarfBuzz and
extracts the palette directly from the bundled skin. It generates
`geometry.js` and `preview-markup.js`. No hand-reconstructed glyphs or
synthetic font weights are used.

With Python, `fonttools==4.67.0` and `uharfbuzz==0.57.0` installed:

```sh
python3 tools/title-design/generate.py
```

After editing `preview.html`, regenerate `preview-markup.js` with that same
command. After changing the design, export a fresh `title.svg` from the
preview's default paused state. Keep the editable source and exported
snapshot together.

The unmodified font files and their original OFL notices are in `fonts/`.
`PROVENANCE.toml` records their pinned sources and hashes. The shaped title
artwork is MIT under this repository's default annotation; the font software
remains OFL-1.1.

## Game asset

`generate.py` also writes `atlas.svg` (three outline masks with clearance
around every letter, then the lettering), and the theme's field coordinates.
Run `node tools/title-design/rasterize.cjs` with Playwright available through
`NODE_PATH` and Chrome installed to reproduce `assets/title/dosis.png`. The
atlas is rendered at twice the design resolution, with ample transparent
padding between frames. The app loads it through the ordinary image cache.
The bundled image is artwork made using Dosis, not a modified font binary.
