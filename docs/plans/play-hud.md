# Play HUD (2026-10-29)

Implemented. Port the maintainer's accepted browser study to the shared
play renderer. `tools/play-hud-design/approved.json` records the selected
parameters; sample life, combo, judgment and score format are preview data,
not gameplay settings. Keep Iosevka: ExtraBold judgments, SemiBold numbers.

The quiet rounded gauge is four lanes wide and 8 px high, centered over
the field, with jade/mint turbulence, coral damage and blue recovery
using the same field. It fills from the middle out, to both ends at once,
brightening toward the ends of the fill, and less than life's share below
full (`life × (0.2 + 0.8 × life)`: half life fills 30%, a loss from full
shows 1.8 times its size, a tenth still about 3%); the warning and every
number keep the true life. Damage snaps life immediately,
then contracts after 80 ms over 380 ms; recovery catches up in 130 ms. Full life blends to Vivid over 240 ms, evolves 2.5
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

The study's gauge was 22% of the screen's width, filled left to right.
Since 2026-11-06 the game's is four lanes wide, Single's field (22% of a
1280 px screen within a few pixels), so it scales with the notes rather
than the window, and fills from the middle out, by the curve above
(`hud::life_rect`, `hud::shown`, `hud.wgsl`'s `life`). The study still shows the earlier gauge.

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

## The music's volume (round AB, 2026-11-19)

In a multiplayer test a player muted the music with song select's quick
volume and then could not unmute it at the Ready card or during the play.
The start prompt and the Ready card have the quick volume since
(`docs/plans/start-prompt.md`, "The music's volume"); a play has a
pointer-only one beside its quit button (`ddi_shell::play_volume`), for
the mouse and touch screens alike and no keys or controller (the
maintainer, 2026-11-19: "it's enough to have a pointer only control (near
the close button) for both touch and mouse"; the play's keys are its
lanes').

- **Where and when.** The quit button's size, beside it toward the
  screen's middle (below a quit button in the top half, above one in the
  bottom half: away from the receptors' row the quit button already
  avoids), shown exactly when the quit button shows: throughout on a
  touch screen, for two seconds after the mouse moved, in replays and
  watched stages too; never in calibration (its click track is left as
  it is). Drawn by the play's 2D pass over everything: a dark disc with a
  light rim as the quit button is, the speaker of `ui/volume.svg` in
  shapes (crossed out when muted, one to three waves).
- **What a press does.** A press on the button opens its slider (a plate
  of the quick volume's popup colors, `ddi_theme::volume_colors`: track,
  fill, knob and the level in percent), along the same way from the
  button where it fits, else beside it; a press on the button with the
  slider open mutes, or brings back the level the last mute kept
  (`ddi_ui::widgets::volume::kept_level`, the game's, so a mute on song
  select is undone here); a press or a drag along the slider sets the
  level, the music following as it moves, the setting kept when let go.
  The slider closes with the buttons, after three seconds untouched
  (`play_volume::IDLE`), and at a press elsewhere, which goes on to do
  what it does (a lane's press presses the lane).
- **What it never touches.** The shells hit-test it from where the frame
  drew it (`Drawn::volume_at`) as they hit-test the quit button, before
  the quit button and the lanes, and hand its edges to
  `PlayView::volume_edges` at the next frame, never to the play's inputs:
  nothing of it is in the play's log. `play_volume::apply` calls
  `PlaySession::set_volume`, which ramps the music's own gain over 20 ms
  (`VOLUME_RAMP`, `AudioBackend::set_volume`: the web's gain node,
  desktop's mixer voice), and plays the claps, mine sounds, scripts'
  sounds and the announcer's next lines (its share of the music's) at
  it; the audio clock, the offsets and the judging do not hear of it
  (`a_volume_changed_during_a_play_judges_the_same_from_the_same_presses`:
  the same presses judge the same into the same log).
- **No keep-out of its own.** The miniatures' slots keep clear of the quit
  button's place, not of this button: a keep-out for it moved the stage's
  layouts (eight fields to seven on a 360×640 phone, 28 to 26 px lanes for
  eight at 1280×720), so it is drawn over a miniature while it shows.
- **Tests.** `ddi_shell::play_volume`'s own (its place at several quit
  buttons, presses, drags, closing), the session's above, desktop's
  `play_volume` offscreen test (the prompt's control and the play's
  through `Shell::play_pointer`), and the web's
  `__DDI_DEBUG.playVolume` (`percent`, `open`, `button`, `plate`,
  `volume`, `musicGain`: the music's gain node now).
