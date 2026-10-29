# Play HUD study

A local browser playground for the play HUD, starting with the life bar.
Open `index.html` directly, or serve the repository root:

```sh
python3 -m http.server 8767 --bind 127.0.0.1
```

Then open <http://127.0.0.1:8767/tools/play-hud-design/>. No Rust build,
package installation, network assets or game session is needed. All assets
are already in the repository. The field is an illustrative static sketch,
not a running noteskin or an exact reproduction of the renderer's layout.

## What to try

- Damage, recovery, full and low life, or scrub life directly. Run demo
  loops through repeated damage, recovery, full, low and empty life.
- Adjust the bar's shape, jade/mint colors, track, damage color, low-life
  threshold, coral and blue gradient endpoints, shared texture contrast/scale, separate normal/full turbulence speeds,
  palette period and response timings.
- Type & combo selects Dosis or Iosevka for judgments, score and combo
  together. Numbers have a separate weight control (600 by default;
  judgments stay 800). Every hundred uses white text, a transparent gap
  and a colored outline. At 1000 the default is colored text with a white
  outline, with separate size/outline multipliers and a switch back to
  white text with a colored outline. Next hundred triggers a brief pop.
- Watermark controls the still title's size, opacity and visibility, and
  the flat gray outline on its D/D/I initials. The outline retains the
  approved title's 12-unit stroke thickness and 12-unit protective gap.
- Bright abstract checks contrast; Still texture freezes the texture clocks
  while leaving life changes active. Reduced motion also makes recovery
  immediate, replaces damage contraction with a 200 ms fade, freezes the
  full-life palette and removes the combo milestone pop. It starts enabled
  when the OS requests reduced motion, and can be toggled for comparison.

Settings save in localStorage under `ddi.play-hud-study.v1`, when available.
Export/import uses a versioned JSON file; only recognized, bounded values
are applied. Life and demo progress are transient; reload starts at 78%.
Reset restores the study defaults, including the OS motion preference.

## Presentation behavior

After damage, green immediately ends at actual life. The lost segment turns
coral, waits 80 ms and contracts over 380 ms, with an ease-out curve.
Repeated damage starts from the currently visible trail. Recovery immediately
paints the gained interval blue, then green catches up over 130 ms. The
green, coral and blue fills use the same turbulence field, coordinates and
clock with different gradient endpoints; clipping preserves the pattern
as each segment changes width. Recovery during a damage trail covers the
regained part in blue while any remaining red continues contracting on its
original trajectory. Reduced motion keeps immediate recovery and a still,
fading damage segment. At full life, once recovery reaches the end, the fill blends
to Vivid over 240 ms, with faster texture evolution and an independent
eight-second palette cycle. Damage leaves the full-life colors immediately.
The accepted controls are recorded in `approved.json`; its five-second
full-life palette cycle and final colors supersede the study defaults.

The noise, domain warp and folded palette ramp follow
`../title-design/preview.js` and the original Infinito material. The green
mapping uses the same folds with a continuous, low-contrast jade/mint ramp.
The browser samples a small canvas at up to 30 fps; this is a design
approximation, not a GPU pixel reference. Combo tier outlines currently
use a gradient from the same palette, without the life bar's turbulence.

Dosis and its OFL notice remain in `../title-design/fonts/`; Iosevka remains
in `../../assets/fonts/`. Dosis is variable; the Iosevka selector uses the
existing Term Regular/Medium/SemiBold/Bold files for weights 400–700 and
the existing Custom ExtraBold HUD file for 800. No separate number face
or zero substitution is applied: numbers follow HUD face. Font-load
completion repaints the canvas even with still texture or reduced motion.
The still title reuses the shaped glyph paths
from `../title-design/geometry.js`, so its lettering stays identical to the
approved wordmark; a mask clears the initials' outlines around every
letter, including neighboring letters. No font binaries are copied or modified.

Existing version 1 settings retain all chosen values when opening the
revised study. Missing new options receive the defaults above; import/export
also retains the number weight, combo gap, 1000 treatment and outline color.
Iosevka is the current default while a replacement face is considered;
saved HUD-face choices and the Dosis comparison option remain available.

The playground remains independent of the app. The accepted selection is
ported through `crates/render/src/hud.rs`, `hud.wgsl` and the text pipeline,
shared by web and desktop; see `docs/plans/play-hud.md`. The sample life,
combo, word and score format in `approved.json` are illustrative only.

`python3 tools/play-hud-design/generate.py` creates `assets/ui/play-hud.r8`
with Pillow, NumPy and SciPy (authoring dependencies only). It contains signed-distance digits from
Iosevka Term SemiBold and lettering/outline alpha masks from the existing
`assets/title/dosis.png`. Font sources and OFL notices remain in
`assets/fonts/PROVENANCE.toml` and `tools/title-design/fonts/`; no font
binaries are modified. The atlas is rendered artwork, as is the title.
The GPU unions neighboring digit distances before painting outlines so
clearance stays transparent between adjacent digits as well as outside them.
