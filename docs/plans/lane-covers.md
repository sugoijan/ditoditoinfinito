# Lane covers (Sudden+, Hidden+) and the preview video

Status: lane covers done 2026-10-21; `#PREVIEWVID` see "Preview video"
below. Closes the lane covers left by `rules-options.md` and the
`#PREVIEWVID` left by `format-gaps.md`.

## Research

**DDR.** RemyWiki's "DDR AC General Info" (Options): HIDDEN+, SUDDEN+
and HIDSUD+ ("HIDDEN+&SUDDEN+" before DDR WORLD) came with
DanceDanceRevolution X2, "similar to beatmania IIDX". Hidden+ is "a lane
cover placed at the top (or bottom if using REVERSE) of the player's
side", Sudden+ one "at the bottom (or top if using REVERSE)", HIDSUD+
both, "the shutter window" between them. Each "can be adjusted by using
the Up and Down navigational buttons", and "the lane cover can also be
toggled off and back on by pressing the Start button during gameplay".
Since DDR A the old Hidden and Sudden are gone and these are the only
visibility covers. No height steps or numbers are published.

**StepMania 5.1** (`5_1-new` at `825467b`, read, not copied) draws no such
panel. Its `Cover` modifier (`PlayerOptions::m_fCover`, "hide the
background per-player") darkens the song background
(`ScreenGameplay.cpp`, the background brightness), which our Field filter
already does. `HiddenOffset` and `SuddenOffset`
(`APPEARANCE_HIDDEN_OFFSET`, `ArrowEffects.cpp` 1049–1074) move the lines
where Hidden and Sudden fade notes by a fraction of the centre line;
they fade notes, they draw nothing over the lanes. Simply Love (SM5)
offers no lane cover either. So the model is DDR's (and IIDX's): a
panel with a height the player sets and moves in play.

## Decided here (the maintainer may change any)

1. **The model** (`ddi_engine::cover`): `LaneCover { mode: Off | Sudden
   | Hidden | HiddenSudden, sudden, hidden }`, heights in whole percent
   of the lane, 25% when first turned on. The lane is what the notes
   cross before they reach the receptors: from the receptors' far edge
   (half an arrow past their middle, so the receptors and the hit
   explosions stay in sight) to the end of the screen the notes come
   from. With both, Sudden+ gives way where the two would overlap.
2. **Presentation only.** A cover is drawn over the notes, holds and a
   skin's notes, and under the explosions, the judgements, the combo and
   the HUD (`ddi_render::scene::push_covers`, the shared scene, so the
   WebGPU, WebGL2 and desktop paths and Options' field preview all draw
   it; the noteskin is not touched). Judging never changes: an autoplay
   under covers is all Fantastic, and the lane presses made while moving
   a cover are judged as any press.
3. **The look:** an opaque panel of the theme's dark violet
   (drawn as rgb(19, 14, 33)) across the field filter's band, a lavender
   line on its inner edge; for 1.5 s after a change (the last 0.3 s
   fading) its height, "30%", on it near that edge.
4. **In play** (`ddi_session::play::CoverControl`): Page Up and Page
   Down move the cover up and down the screen (Sudden+ grows as its edge
   goes towards the receptors, Hidden+ shrinks; with both the window
   moves as one and stops where either reaches its end); Start (Enter on
   a keyboard, a controller's bound Start) held with the up or down
   arrow lane moves it the same way, and Start tapped alone hides the
   cover and shows it again, as DDR's Start does. A moving control held
   steps on after 0.35 s every 0.04 s. Keys bound to a lane stay the
   lane's. Touch screens set the heights in Options only.
5. **Kept.** Every change during a play is kept in its log
   (`PlayLog::covers`, the log's binary form version 2, written only
   when there are changes, so older logs and readers are unaffected),
   at its judged song second; a replay draws the covers as the play
   did and ignores the keys, and `history::replay` changes them too, so
   the results of a kept play say the same. The results and the mods
   line say the cover as the play ended ("hidden+ 17% sudden+ 33%"; the
   last shown, if it ended hidden). The heights a play leaves the cover
   at are kept in the settings for the next song (DDR and IIDX keep an
   adjusted cover), not a replay's or a calibration's; the mode stays
   the one chosen.
6. **Options**, in the Notes section after Appearance: "Lane cover"
   (Off, Sudden+, Hidden+, Both) and, for the covers chosen, "Sudden+
   height" and "Hidden+ height" steppers (0–100%). Song select's mods
   line, the start prompt's and the results' say it. Calibration plays
   without a cover.
7. A layout with receptors at both ends (a lower row scrolling the other
   way) has its lanes end at the middle of the screen, each lane its
   own panels.

## Checks

- Engine: `cover::tests` (moving, clamping, the drawn shares, lenient
  settings), `history::tests::cover_changes_round_trip_and_a_log_without_them_stays_version_1`,
  `tests/history.rs` `lane_cover_changes_are_kept_and_judge_nothing`
  (frames, the results' cover, judgements equal to a play without one,
  the replay alike).
- Render: `scene::tests::lane_covers_cover_their_share_of_the_lanes_over_the_notes`
  (places under normal scroll and Reverse, the draw order, the label).
- Session: `lane_covers_move_and_hide_in_play_and_replay_alike` (Page Up
  held and its repeat, Enter's toggle, a pad's Start with the up lane,
  the log, the settings kept, a replay ignoring its keys).
- UI: `lane_cover_rows_follow_the_covers_chosen`,
  `the_results_say_the_lane_covers_as_the_play_ended`.
- Desktop offscreen: `play_offscreen.rs`
  `lane_covers_hide_the_notes_under_them` (Dito and the built-in arrows
  at Beatucada's 12 s: the panels one flat color where notes showed
  without them, the receptors and the window unchanged).
- Browser (2026-10-21, `target/scratch/pw/covers.mjs`, Beatucada
  Beginner autoplayed with both covers at 25%): headless Chrome WebGPU
  and WebGL2 and Playwright's Firefox: 38 Fantastics; Page Up held
  0.6 s gives "hidden+ 17% sudden+ 33%" (`__DDI_DEBUG.cover`), Enter
  hides it and shows it again, the settings keep 33/17, the results'
  mods line says it; screenshots show the panels and the labels.

## Preview video

See the section added below when it is done.
