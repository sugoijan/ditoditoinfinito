# Lane covers (Sudden+, Hidden+) and the preview video

Status: lane covers done 2026-10-21; `#PREVIEWVID` imported and kept
2026-10-21, not shown yet ("Preview video" below). Closes the lane covers left by `rules-options.md` and the
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

**Sources.** `.ssc` `#PREVIEWVID` (`SSCLoader`, `Song::SetPreviewVid`)
is reachable from Lua only (`Song:GetPreviewVidPath`); StepMania 5.1's
own themes (`default`, `_fallback`) never draw it. Themes made for Pump
It Up packs show it in song select's banner place while the song is
selected, muted (the song's sample plays). Locally 9 of 207 `.ssc`
files have one, all in one Pump pack (`Preview.mp4` beside the song's
`BGA.mp4`).

**Done: import and storage.** `ddi_library::pack::resolve_assets` finds
the file (`ResolvedAssets::preview_video`, case-insensitively, movies
only; a missing one is a note), `Reading::movies` lists it after the
background changes' movies, so both shells sniff it and keep it as they
keep those (desktop reads it in place; the web links it, or copies it
with "also store background videos", under `bg/<name>`), and
`ManifestEntry::preview_video` (`VideoRef`) names it. It is not a
background movie: it never plays in the field, never counts among the
song's videos, nor makes the opening still, unless a background change
names it too, or it is the only movie in the folder of a song without
background changes: StepMania 5.1 then plays it as the background
(`Song::TidyUpData`, "DWI style"), and so does the game. Songs imported before need importing again; desktop's
library cache is read again (version 9). Checked: `scan::tests::the_preview_video_is_kept_apart_from_the_background_movies`;
in headless Chrome a zip of a song with `#PREVIEWVID:Preview.mp4`
(generated) imports with `preview_video` `{name: "Preview.mp4", format:
mp4/h264, stored: true}` and `bg/Preview.mp4` in IndexedDB
(`target/scratch/pw/previewvid-import.mjs`).

**Decided: the display waits** (the maintainer may change any):

1. Where: in the selected song's banner place (the left column,
   `Select.Banner`, cover-fitted, as the Pump themes do), once its first
   frame is decoded; the banner shows until then and when it cannot
   play. Not as song select's background: that is the song background
   blurred to 20% (`Backdrop`), where a movie would read as noise.
2. Why it waits: song select is drawn by the 2D pass alone on both
   shells, which has no movie path. The movie decks (web
   `crates/web/src/movies.rs` `MovieDeck`, desktop
   `crates/desktop/src/play/movies.rs`) feed a play's `Renderer`, the
   web's FFmpeg module worker is one per play session, and neither UI
   host draws continuously at rest. Showing it means, per shell, a
   `Renderer` (or a bare `BackgroundPipeline`) drawing the movie into a
   texture the painter shows as an external image (as Options' field
   preview does, `Painter::set_external_image`), a deck fed by a wall
   clock from the selection, frames kept coming only while it plays,
   and a lifecycle tied to the preview's (`select_preview.rs`,
   desktop's `preview.rs`): started when the selection settles, stopped
   when it changes or song select is left, never during a play. Plus a
   `UiRequest` telling the shell where the banner is and an `Env` flag
   telling the screen a frame is ready. That is most of a plan of its
   own across both shells' song select hosts, for one local pack.
3. The rules it will follow: muted; looped; the Background videos
   setting off never fetches the decoder or the file (the banner
   shows); automatic stops it when frames fall behind, as in play;
   reduced motion shows its first frame still.
