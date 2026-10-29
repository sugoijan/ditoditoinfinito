# Play HUD (2026-10-29)

Implemented. Port the maintainer's accepted browser study to the shared
play renderer. `tools/play-hud-design/approved.json` records the selected
parameters; sample life, combo, judgment and score format are preview data,
not gameplay settings. Keep Iosevka: ExtraBold judgments, SemiBold numbers.

The quiet rounded gauge is 22% wide and 8 px high, with jade/mint
turbulence, coral damage and blue recovery using the same field. Damage
snaps life immediately, then contracts after 80 ms over 380 ms; recovery
catches up in 130 ms. Full life blends to Vivid over 240 ms, evolves 2.5
times faster and cycles its palette in five seconds. The selected 25%
warning threshold also honors an earlier ruleset danger flag. No gauge
or score rules change.

Combo hundreds have colored outlines separated by a real transparent
2.5 px gap; at 1000, invert the white and colored parts and scale by 1.2.
Keep the small milestone pulse, freeze it and the palette/turbulence under
reduced motion. A 170 px still title at 10% opacity retains gray D/D/I
outlines. Sizes scale with the field's arrow size from a 720 px high play.

Implementation: a shared GPU HUD pass, an offline digit distance atlas
from the bundled Iosevka SemiBold and the existing title masks, and lighter
score text through glyphon. Presentation time is independent of music
rate; reset it between sessions. Previews omit the HUD. Preserve judgment
names/colors, hold outcomes, timing deltas, ghost scores and quit controls.

The score sits below the gauge row to keep long scores clear on narrow
screens. Lyrics reserve the gauge, score/ghost and watermark bounds; the
mark hides if it intersects receptors or the touch quit button. Combo
numbers fit the viewport, including ten digits. Life loss/recovery is
clipped against true life even when the transitions interrupt one another.

Validation (2026-10-29):

- `cargo test -p ddi-render -p ddi-shell --lib`: 144 renderer tests and
  12 shell tests passed, including interrupted transitions, full-life
  timing, reduced motion, seek resets and GLSL ES 3.0 shader generation.
- Desktop `--no-default-features` offscreen tests `play_hud_offscreen`,
  `hud_colors_offscreen` and `hud_text_offscreen` passed on the native GPU.
  They check real transparent gaps, shared life texture coordinates,
  blue/coral feedback and HUD-free field previews, on plain, sRGB and
  encoded half-float targets. Existing judgment colors/pop caching pass.
- Inspected native GPU captures of 99, 100, 200, 300, 900, 1000 and 1200,
  damage/recovery/full/warning states, a bright backdrop and a 320 px wide
  Reverse field with a ten-digit combo. Test captures use synthetic frame
  data and fallback receptors; no noteskin assets were changed.
- `cargo check` and `cargo clippy -- -D warnings` for `ddi-web` on
  `wasm32-unknown-unknown`, formatting and REUSE lint passed. The shader
  runs through the same shared renderer on both shells; a browser runtime
  smoke test was not part of this port.

Reproduce captures by setting `DDI_HUD_SNAPSHOTS` to a scratch directory
when running `play_hud_offscreen`. Native runs use an isolated `DDI_HOME`
and the repository's memory watchdog, without a window or audio device.
