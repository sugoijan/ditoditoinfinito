# Plan: a chart editor, first version (2026-10-21)

The last item of phase 10 (`docs/PLAN.md`, section 5): edit a song's
timing and notes in the game, try them at once, and keep them
(`docs/plans/phase-10.md`, step 3).

Status (2026-10-23): the writer, the editing model, the field in the
player's skin, the screen on both shells, listening with the
metronome, tap-to-tempo, the music's overview and the audio view,
note record,
playtests, saving
and the autosave are built (steps 1–8; see "Built" below); what is
left is under "Left".

## Scope of the first version

**In:**

- **Formats:** `.ssc` and `.sm` songs, opened from the library (an
  imported song on the web, a song folder's on desktop) or made new from
  a music file. Saved as `.ssc` (the format that holds everything the
  editor edits: split timing included); `.sm` written too when the chart
  has nothing `.sm` cannot say (a choice on export, never silent loss).
- **Timing:** the offset; BPM changes and stops on the beat grid (what
  most charts need), with the beat under the cursor shown as its time;
  tap-to-tempo and a metronome click (calibration's click track) to set
  the first BPM and the offset by ear; delays, warps, scrolls, speeds,
  time signatures, tick counts, combos, labels and fakes kept and shown
  as markers, edited as numbers in a dialog (no dedicated tools).
- **Notes:** the built-in StepMania layouts (dance single, double,
  solo; pump single, half-double, double): taps, holds, rolls, mines,
  lifts and fakes placed on the grid at a chosen snap (4th to 192nd);
  select a range, cut, copy, paste, delete, mirror and shift it by a
  snap; charts added, removed and their difficulty, meter, name,
  description and credit set; the song's title, artist, music, banner,
  background, preview start and length, and display BPM.
- **Playing it:** from the cursor or from the start, autoplayed or
  played by the player, then back to the editor at the same place (the
  play session as play runs it, on the chart in memory).
- **Both shells:** the same screen on the UI core; desktop saves in
  place (with a backup) or to a new folder; the web saves an imported
  song back into its storage, and downloads a song as a zip (its chart
  and the files it names) or the chart alone.
- **Undo and redo** of every change; an autosave of the session (the
  chart in memory) so a crash or a closed tab loses nothing.

**Out (later):**

- Dancing☆Onigiri works and FFR levels (their own formats; they keep
  being read, not written), `.dwi` writing.
- Keysounds, attacks, `#BGCHANGES` editing (kept as they are, written
  back unchanged), lyrics.
- Audio display beyond the overview strip and the audio view (a
  waveform and a spectrogram on the field's rows, "The audio view"
  below): onset or beat markers found in the music, zoomed spectra.
- Automatic BPM detection, automatic note generation, chart statistics
  (density graphs come from the song list's existing data where cheap).
- Multiplayer editing, a separate editor application (SkinCraft is one
  for skins; charts belong in the game, where they are played).

## What it reuses

- **Chart model and parsers** (`ddi_chart`): `Song`, `Chart`, `Note`,
  `TimingMap` with every segment kind, the `.sm`/`.ssc` parsers. Missing:
  a **writer** (new: `ddi_chart::formats::sm_write`), written so that
  `parse(write(song)) == song` for every bundled and test fixture chart
  (the differential corpus tests give the checks: `DDI_DIFF_CORPUS`), and
  that keeps the tags the parser did not understand
  (`SourceInfo::unknown_tags`) in their place.
- **The digest** (`ChartDigest`): a saved chart whose notes changed is a
  new chart for the history and bests, as for any edited pack; the editor
  says so before saving over a chart with plays.
- **The UI core and the theme** (`crates/ui`, `crates/theme`): a screen
  of its own (`screens/editor.rs`) with the existing widgets (lists,
  option rows, dialogs, the text field for names and numbers, hit-testing,
  gestures for touch and mouse, key repeat), and its look in
  `infinito.toml`.
- **Drawing the field** (`crates/render`): the play scene draws notes,
  holds, receptors and the noteskin at any time and speed; the editor
  draws it paused at the cursor with a beat grid, the selection and the
  timing markers over it (new layers of the same scene). Options' field
  preview shows how to draw a field into a texture the 2D pass shows.
- **Playtesting** (`crates/session`): `PlaySession::start` takes a `Song`
  in memory and a start time (`song_offset`); the editor hands it the
  chart being edited, autoplay or not; the results come back as for any
  play but are never kept in the history (an editor play is a test, like
  calibration).
- **Music:** the shells' preview players (seek, play, pause, volume) for
  scrubbing and listening; the decoders (`ddi_platform` `decode`,
  `decode-all`) for the overview; calibration's click for the metronome.
- **Files:** desktop's library (song folders, `RelPath`, rfd dialogs as
  Options' export uses) and its cache invalidation after a save; the
  web's import storage (IndexedDB `songs`/`files`) and its zip reader,
  plus a small zip writer (stored entries) for downloads.

## Design notes

- **One editing model, sans IO** (`ddi_ui::state::editor`): the song,
  the cursor (a tick), the snap, the selection, the undo stack (each
  change an inverse operation), and the operations themselves; tested
  natively like the rest of `ui::state`. The shells only load, save,
  play sound and start playtests.
- **Input:** keys as StepMania's editor (the lane keys place and remove
  notes at the cursor; held while moving makes a hold; Up/Down move by
  the snap, Page Up/Down by a measure; Left/Right change the snap; a key
  for each note type), mouse and touch on the field (tap a lane to
  place, drag to make a hold or select, wheel or drag to scroll), and a
  pad (menu actions) for moving and playing. StepMania's key map is read
  for behavior only.
- **Timing changes keep the notes on their beats** (as StepMania does):
  a BPM change moves later notes in time, never off the grid; an option
  to keep times instead (a re-sync of an existing chart) moves notes to
  the nearest tick.
- **Gesture rules on the web:** music starts inside the press that asks
  for it (AGENTS.md); a download is a link clicked in the press.
- **Safety:** saving over a file keeps the previous one beside it
  (`.old`), once a session; the autosave lives in the scratch space of
  each shell (`localStorage`/IndexedDB, the data folder).

## Steps

1. **The writer.** `ssc` (and `sm`) from `Song`, round-trip tests over the
   bundled songs, the chart fixtures and the local differential corpus;
   unknown tags kept. Nothing in the UI.
2. **The editing model.** Cursor, snap, selection, notes and timing
   operations, undo/redo, chart and song metadata; property tests (any
   sequence of operations undone gives the song back; any song written,
   read and edited the same gives the same result).
3. **The field in edit mode.** The scene drawn paused at a cursor with
   the grid, markers and selection, at the player's skin and speed;
   offscreen tests as for play (`crates/desktop/tests/*_offscreen.rs`).
4. **The screen.** Its layout in the theme (the field, a side panel of
   timing and chart rows, a toolbar), keys, mouse, touch and pad; the
   harness's tests at the three sizes the other screens are tested at.
5. **Sound.** Scrubbing, playing from the cursor, the metronome,
   tap-to-tempo, the overview strip.
6. **Playtest.** From the cursor and from the start, autoplay or not,
   back to the same place; never in the history.
7. **Saving.** Desktop: in place with a backup, or a new song folder, and
   the library read again; web: back into the imported song, a zip
   download, the chart alone; the autosave and its recovery. Verified in
   headless Chrome and Firefox and offscreen on desktop: a new song made
   from a music file, charted, played and saved, then opened again equal.
8. **Entry points.** Song select's song menu ("Edit"), a "New song" in
   the start menu's Singleplayer page or song select, both shells.

Each step is a commit; steps 1 and 2 can land without any screen.

## Built (2026-10-21)

1. **The writer** (`ddi_chart::formats::sm_write`): `write_ssc` (version
   0.83) and `write_sm`, with `sm_losses` naming what `.sm` cannot say
   (warps, speeds, scrolls, fakes, labels, combos, split timing, chart
   names, credits and display BPMs, chart tags, the jacket, notes `.sm`
   has no character for, a Hard chart described as `smaniac`). Numbers
   are written to read back bit for bit (shortest decimals; whole ones
   with three decimals as StepMania does; beats with three or six
   decimals, the reader rounding to the tick); text escaped as the MSD
   reader needs (`\`, `;`, `#`, the second `/` of `//`, `:` in one-value
   tags); background changes from their raw value, attacks in place;
   tags kept as written (`SourceInfo::unknown_tags`) after the song's
   own and in their chart's block, except those the target format
   would read as its own (a `.sm`'s `#COMBOS`); generated charts left
   out. Tests (`crates/chart/tests/write.rs`): the bundled songs, the
   fixtures (DWI converted to `.ssc`), 400 generated songs using every
   field, as `.ssc` and, when nothing is lost, `.sm`, read back equal and
   written again the same; danceparser reads the written `.sm` as ours
   (`differential.rs`). `DDI_WRITE_CORPUS=<dir>[:<dir>…]` does the same
   over local folders: 1,482 of 1,482 local simfiles round-tripped
   (1,276 as `.sm` too).
2. **The model** (`ddi_ui::state::editor`): `Editor` holds the song (its
   generated charts left out), the chart shown, the cursor, the snap
   (4th to 192nd), the selection, the tool (tap/hold, roll, mine, lift,
   fake), the clipboard and the undo and redo stacks. Each edit is one
   `Change` (notes out and in, a timing map swapped, the song's or a
   chart's fields, a chart inserted or removed with its tags in their
   places, a batch) applied and inverted; every edit keeps the song as a
   file keeps it (sorted, one note to a cell, nothing inside a hold in
   its lane, timing tidy, a 4/4 at 0 when a later time signature is
   set, text trimmed, refused values: a BPM not above 0, a time
   signature with a 0). Notes: a press toggles at the cursor (a hold's
   whole span counts), held while the cursor moves it grows into a hold
   or a roll (one undo), `place_long`, cut, copy, paste (replacing the
   rows it covers), delete, shift by the snap, mirror and the turns
   (`ddi_engine::transform::lane_map`). Timing: every segment kind set
   or taken away at a tick, the offset, the chart's own timing on or
   off; `Retime::KeepTimes` moves every chart on that timing to the
   tick nearest each note's old time. Tests: the operations, random
   sequences over the bundled songs undone to the song and redone, the
   song read back after every sequence, and a song read back from its
   written file edited the same.
3. **The field in the player's skin** (2026-10-23;
   `ddi_ui::screens::editor_field`, `ddi_render::editor_field`,
   `ddi_shell::editor_field`): the shell draws the chart as play draws
   it, paused at the cursor: the screen hands it a play's frame of the
   notes in view (`FieldPicture`: their `y` in arrow heights, the
   field's geometry in the texture's pixels, whether the music plays)
   whenever it changes (`EditorHost::show_field`), and the renderer
   draws it with the player's noteskin (the one a play of the chart
   would use, `Settings::play_skin`, readied with its parameters and the
   chart's Lua environment; read on a thread on desktop, in slices on
   the web) into a texture the 2D pass shows under `editor-field`, with
   no HUD, as Options' field preview is drawn. Over it the screen draws
   what must read exactly: the grid at the snap, measure and beat
   numbers, a tick in each note's lane at its row, a line at each
   hold's and roll's tail, badges for fakes and lifts, warps and fake
   regions shaded, the selection, the markers and the cursor line, all
   from lane and row, never from the art; pointers find lanes and rows
   the same way. One mapping from ticks to the screen places both.
   **Two views**: the edit view (default) spaces rows evenly by beat
   at the editor's zoom (− and =), whatever the timing does, and raises
   the zoom at dense snaps so the snap's rows stay at least a third of
   a note apart (the art keeps its size); the play view (V, the menu,
   or a tap on the panel's view line) is the moment as play draws it:
   the player's speed, BPM changes, stops, scrolls, speed segments,
   Boost and the like, Reverse (the line as far from the bottom as it
   is from the top in the edit view, later notes above it). Editing
   works in both. **The skin is tamed**: its clock is held at rest
   (two seconds in, beat 0: no receptor pulse, no explosions or glow)
   except while the music plays, when it runs with the second heard
   and the notes passing the line are hit as an autoplay hits them
   (`editor_field::passed`). The notes are drawn at the cursor's beat
   all the same: their animation and the colors it picks (Vivid's
   cycle, Rainbow's flow, a skin's beat-based frames) are the play's
   when that row reaches the receptors, as StepMania's editor draws its
   notes at its cursor's beat (`ScreenEdit::DrawPrimitives`,
   `src/ScreenEdit.cpp:2020-2058` at 5.1's `825467b`); a song's own
   colors are drawn as play draws them (Dito's `ChartColors`).
   **Cost**: only the notes whose span shows are in the frame, and a
   frame is drawn (the skin's Lua run) only when the picture changed
   or a skin came: nothing between changes, every frame while the music
   plays. **The compact field** (F, the menu) draws notes on plates
   with the menus' actors as before, at the zoom without the raise;
   the same plates are drawn where the shell draws no skin (the
   harness). Tests: the mapping and frames (`editor_field.rs`), the
   screen with a shell that draws the skin (`editor_skin_tests.rs`:
   every note's tick at its picture row in both views, Reverse, a tap
   on a row in the play view, the zoom at 192nds, the compact field,
   listening's events, leaving), and offscreen
   (`crates/desktop/tests/editor_skin_offscreen.rs`: a tap's art on its
   row with the built-in arrows and Dito in the edit view and in an
   x-mod and a c-mod play view across a BPM change and a stop, and
   moving exactly as its row moves with Delta, Peters-DDR and the
   CF_CHROME models; the edit view equal to the play view at one tempo
   when the zoom is the speed; the editor on a bundled song drawing the
   skin's texture only for a change, the play view and the compact
   field).
4. **The screen** (`ddi_ui::screens::editor`, dialogs in
   `editor_dialogs.rs`): the field (lanes, the beat grid at the snap,
   measure and beat numbers, notes as their lanes' arrows on plates
   colored by beat, holds and rolls, mines, lifts, fakes, the selection,
   the timing's markers, the cursor line) drawn with the menus' actors;
   a panel (the chart, beat, measure and time, snap and tool, BPM, what
   is at the cursor, notes, selection, saved or not, the last action;
   Menu, Save, Play); a footer of keys. The menu (Tab, a pad's Select),
   the timing at the cursor (each kind, the offset, keep beats or
   times, the chart's own timing), the song's fields and the chart's
   (difficulty, meter, name, description, step artist; previous, next,
   add, remove) are lists of buttons in columns that fit; values are
   typed in a dialog and refused with a reason. Keys as the file's
   header says; a pointer taps notes, drags holds and selects rows;
   reduced motion jumps instead of gliding. Tested in the harness at
   1280×720, 844×390 and 390×844. The UI core sends key releases to a
   screen that asks (`Screen::key_releases`), for the held lanes.
5. **Listening** (`ddi_session::listen::Listening`, both shells): Space,
   Enter or a pad's Start plays the chart's music from the cursor and the
   field follows the second heard (the output's latency and the
   settings' audio offset taken off, as a play hears it); stopping puts
   the cursor on the snap's row nearest. The music is decoded when the
   editor opens (desktop on a thread at the output's rate; the web on an
   offline context), so the press plays it at once; the web makes its
   audio context inside that press.
   The sound tools (2026-10-23; `ddi_session::listen::Cues`,
   `ddi_ui::screens::editor`'s `sound`, `state::editor::tap` and
   `overview`). **The metronome** (K, the menu, or a tap on the panel's
   line) clicks calibration's click on every beat while listening (its
   note's click on a measure's first beat by the time signatures, its
   lead-in's on the others) and **the assist clap** (Shift K) claps the
   play's clap on the rows of taps and hold and roll heads judged; both
   are scheduled 0.3 s ahead on the audio clock at the context time the
   music plays the beat's second (`TimingMap::seconds_at`, as a note on
   it is judged: through BPM changes, stops, delays, and one click for a
   warp's beats), so they are heard with the music, and are told again
   when the song, the chart or the sounds change while listening. They
   are the editor's state (`Editor::sounds`), not the settings.
   **Tap-to-tempo**: Y tapped along; the BPM is a least squares line
   through the last 24 taps against beat numbers from their gaps (each
   gap rounded to the median gap: a skipped beat counts two, an extra
   tap none), the farthest tap a fifth of a beat off the line left out
   and the line fitted again; a pause of 2 s starts again. While
   listening each tap also has the second heard, and the run says where
   a beat was heard, from which the offset that puts the nearest beat
   there (to the millisecond). Shift Y (or a tap on the panel's line)
   sets the BPM at the cursor, the menu also the BPM and the offset, one
   undo each (`Editor::set_tapped_tempo`). **The overview**: the
   music's RMS in up to 4,096 bins of at least 10 ms, on a 30 dB scale
   under the loudest, made once from the music decoded for listening
   (desktop on a thread, the web from the `AudioBuffer` a block at a
   time, its MP3 lead kept), drawn as a strip at the field's right edge
   with the rows in view and the cursor; a tap or a drag on it moves the
   cursor there (listening stops). The screen asks the shell for it
   every 0.25 s while it is being made (`EditorHost::overview_coming`).
6. **Playtest**: `EditorCall::Playtest` hands the shell the song as
   edited and a start 2 s before the cursor (`PLAYTEST_LEAD`);
   `SessionConfig::start` starts the session there (earlier notes
   skipped, the music fading in, as after a work's `startFrame`).
   Desktop: `play::Request::edited`/`start`, read by `play::load`, never
   kept (`Play::record`), the library not told the edited charts.
   Web: `#/play?…&edit=1` takes the session's song (`crate::editor`),
   is never kept, has no ghost, and goes back to `#/edit`.
7. **Saving** and **the autosave**. Desktop (`crates/desktop/src/editor.rs`):
   a library song read from `.ssc` is saved over it, the file as it was
   kept as `<name>.old` at the first save of an editor session, after a
   question; one read from `.sm` or `.dwi` gets a `.ssc` beside it,
   which the library reads first from then on; written through a
   temporary file renamed over; the pack read again. Bundled songs and
   zips' are saved as a copy only (the system's dialog, `.ssc` or `.sm`
   when nothing is lost). Web (`crates/web/src/editor.rs`): an imported
   song's stored chart becomes `chart.ssc` and its entry is made again
   (`ddi_library::edited::entry_after_edit`, natively tested), after a
   question; the files it was imported from are never touched; any song
   downloads as `.ssc` (or `.sm`), a link clicked inside the press.
   The autosave (`ddi_ui::state::editor::autosave`): every 5 s while
   there are unsaved changes the song as edited is kept (a wake-up at
   that time, `Screen::wake_at`, with no frames drawn meanwhile; desktop:
   `editor-autosave.json` in the data folder; the web: `ddi.editor.v1`
   in local storage), forgotten by a save or by leaving; opening that
   song again recovers it, unsaved, and says so.
8. **Entry points**: song select's E on both shells
   (`SelectScreen::with_editing`, `SelectCall::Edit`); the web's route
   `#/edit?song=<id>&chart=<n>`; desktop's `--edit SONG [--chart N]`
   (the editor over song select once the library has the song, for
   runs and screenshots). **New songs** (2026-10-23): Add songs' "New
   song from a music file" (`SelectCall::NewSong`, where the shell
   offers editing; keys, a pad and taps reach it as the pickers above
   it) picks a music file (the web: a hidden file input clicked inside
   the press; desktop: the system's dialog), names the song from the
   file's tags (`ddi_library::tags`: ID3v2, Vorbis comments in Ogg and
   FLAC, a WAV's `INFO`) or else its name, and makes it
   (`ddi_ui::state::editor::new_song`): one empty chart at 120 BPM from
   the music's start (`Editor::new_song`) in the pack `Dito Edits`, its
   folder named after the title (numbered when taken). Desktop
   (`crates/desktop/src/editor/new_song.rs`): the folder
   `Dito Edits/<title>/` in the first song folder Options lists (in its
   `Songs` for a StepMania root), the music copied in and
   `<title>.ssc` written, read into the library at once. The web
   (`crates/web/src/editor/new_song.rs`): an imported song in
   IndexedDB, its music the file picked and its chart `chart.ssc`
   (`ddi_library::edited::entry_for_new`). The editor opens on it
   unsaved, so listening and playtests read it as any song's; its
   first save asks nothing and keeps no `.old`; left before that, the
   song is taken away again, as the question to leave says. Cancelling
   the picker changes nothing.

Verified: the native tests above; desktop offscreen
(`crates/desktop/tests/editor_offscreen.rs`: the editor in the theme on
a bundled song, a note placed, the menu, listening on the headless
clock) and the library flow
(`editor.rs`'s tests: an `.sm` song saved as `.ssc` beside it, read back
as edited, playtested part-way headless and not kept); headless Chrome
and Firefox (`target/scratch/pw/editor-web.mjs`: deep link, a note, an
autoplayed playtest from the end and back to the editor as it was, the
download, song select's E, an imported song saved and read back after a
reload; listening started by a key under Chrome's user-activation
autoplay policy, stopped on the grid; the autosave kept, recovered after
a reload and dropped by leaving).

The sound tools (2026-10-23): `listen.rs`'s tests on a fake clock (the
clicks' scheduled times are the beats' heard times through a BPM
change, a stop, a delay, a warp and a 3/4; the claps on the rows
judged; cues changed while listening go on from what was scheduled);
`tap.rs` (jitter, a stray tap, a skipped beat, a double tap, a pause,
the offset from taps heard, one undo); `overview.rs`; the harness
(`editor_sound_tests.rs`: the toggles told to the shell while
listening, taps by keys at their time stamps, the offset from taps
while listening, the overview asked for until it comes and a tap and a
drag on the strip at the three sizes, the panel's lines above its
buttons); desktop offscreen (`crates/desktop/tests/editor_sound.rs`:
the overview made on its thread and drawn, the metronome on while
listening headless, a click on the strip); headless Chrome (under the
user-activation autoplay policy) and Firefox
(`target/scratch/pw/editor-sound.mjs`: the overview, clicks scheduled
on Beatucada's beats a little ahead of what is heard, accented on the
measures, claps once turned on while listening, taps estimating
122 BPM, Shift Y and its undo, a click on the strip;
`__DDI_DEBUG.editorCues`).

New songs (2026-10-23): the harness (`editor_tests.rs`: Add songs'
"New song" at the three sizes, by keys and taps, only where editing is
offered; leaving a new song unsaved says it goes); desktop headless
(`crates/desktop/tests/editor_new_song.rs`, the picked file injected:
the picker cancelled, then a generated WAV named by its `INFO` made
into a song from Add songs, listened to, charted by keys, playtested
autoplayed and not kept, saved, listed by song select and opened again
equal; a second one left unsaved and removed; `--edit` waiting for the
library); headless Chrome and Firefox (`target/scratch/pw/new-song-web.mjs`,
the file set on the page's input: the picker cancelled with nothing
made, then a generated MP3 (Chrome) or Ogg Opus (Firefox) named by its
tags made into a song, charted, playtested autoplayed and back, saved,
listed by song select and opened again equal after a reload; a second
one left unsaved and gone from the list).

**Note record** (2026-10-23; `ddi_ui::state::editor::record`,
`ddi_ui::screens::editor`'s `record`, `ddi_session::listen`). R plays
the music from a measure before the cursor (by the time signature
there; not before the song's start) with a count-in, the metronome's
click on the pre-roll's beats whether it is on or not
(`Cues::with_count_in`), then the metronome and the clap as toggled;
the field follows the second heard as when listening; on the web the
press starts the music (`EditorCall::Record`, handled as `Listen`).
With a selection it records over the selection alone and stops by
itself once no press could land in it. The lanes are the play's
bindings for the chart's layout, keys and controllers
(`ddi_session::listen::record_input` over `play::play_bindings` and
the controllers connected: desktop's `input::connected_pads`, the
web's `gamepad::page_pads`); while recording the screen takes keys and
controls as they are (`Screen::raw_input`), so bound keys are lanes
even when they are the editor's (the arrows); R, Escape, Backspace,
Space or Enter when not bound, a controller's Back or Select, or a tap
stop it; nothing else does anything. Each press's time stamp (desktop's
OS-stamped keys, the pads' thread's stamps, the web's `timeStamp`)
becomes the second heard as a play judges a press: the listen keeps an
`ddi_engine::clock::SongClock` started with the music and updated each
frame (the output's latency and the audio offset taken off once,
`Listener::heard_at`), then the beat by `TimingMap::beat_at` (a press
in a stop or a delay on its beat, past a warp after it), then the
snap's nearest row, the later of two as near; a row in a warp moves to
the first after it. Presses outside the rows recorded (in the pre-roll
less half a snap, past a selection) are ignored. A press held longer
than 0.3 s is a hold from its row to the release's nearest (at least a
snap long, growing while held; a roll with the roll tool). The notes
show tinted at their rows as they come, the skin's receptor flashes.
Stopped, the take is one change: R replaces (the notes whose heads are
in the rows recorded, from the start row to the row it stopped on or
the selection's end, are taken away first), Shift R adds (a note
recorded on a taken cell is left out, a hold over a later note ends a
snap before it); the cursor goes to the row it stopped on; the panel
says the notes recorded and the presses' mean and spread from their
rows ("12 notes recorded · 12 ms late on average, ±9 ms"), and the
debug state `record` (`active`, `mode`, `start`, `notes`, `presses`,
`meanMs`, `spreadMs`, `rows`). The menu has both for controllers and
phones. Listening and recording take the audio offset of the output's
profile when the settings have one (`Settings::audio_offset_for`, what
`play_options` takes), else the default.
From StepMania 5.1's editor (`src/ScreenEdit.cpp`, read for behavior
only): `InputRecord` puts a tap at `Quantize(GetBeat(), snap)`
(`RageUtil.h`: half rounds up) and ignores rows outside the range; a
tap on a row of a hold recorded before takes the hold's place
(`IsHoldNoteAtRow` counts its tail); `Update` makes a hold of a press
held more than `record_hold_seconds` (`record_hold_default`, 0.3 s),
its ends quantized the same way and clamped to the range; only taps,
holds and rolls are recorded whatever note type is chosen (rolls while
its roll key is held: here the roll tool), so the mine, lift and fake
tools record taps and holds; `TransitionEditState` records over a copy
of the chart and, when recording ends, clears the range and copies the
take in, so StepMania keeps the notes there (our Shift R); R
replacing them is our choice. StepMania's lead-in is a second
(`EditRecordModeLeadIn`) without a count-in; record from the cursor
ends at the chart's last note, ours when stopped.
Verified: `record_tests.rs` (every snap from 4th to 192nd through a
BPM change, a stop, a delay and a warp, 0.4 of a snap early and late;
in a stop, a delay, before a warp; ties; 12ths and 24ths; the hold
threshold 0.01 s under and over, a release stamped before a frame saw
it held; rolls; mines record taps; a tap on a recorded hold's tail;
replace and add; a selection's bounds and end; the pre-roll's half
snap; a 3/4 pre-roll; one undo; the mean and spread); `listen.rs`
(`heard_at` against the second heard and a press stamped a frame
before, the count-in's clicks with the metronome off); the harness
(`editor_record_tests.rs`, at 1280×720, 844×390 and 390×844: R asks
for the music from the pre-roll, stamped arrows become notes on the
8ths' rows and a hold, the arrows neither move the cursor nor change
the snap, Escape stops without leaving, one undo; Shift R adds, a
controller's d-pad records and its B (Back) stops, a tap and R stop,
other keys do nothing; a selection stops by itself at its end; the
menu); desktop headless (`crates/desktop/tests/editor_record.rs`: on a
fake clock, keys stamped and a fake controller's edges routed raw by
the shell, an audio offset of 50 ms in the settings: eight presses
through a BPM change and a stop land on their rows, the mean off the
grid is the presses' own to 0.01 ms, saved and read back).

**The audio view** (2026-10-23; `ddi_ui::state::editor::audio`,
`ddi_ui::screens::editor`'s `audio`, `ddi_render::editor_audio`): the
music's waveform and spectrogram beside the lanes, time down the field
as the notes are, so a transient and its beat's line read together:
when the BPM and the offset are right, a kick's onset is on its
beat's line. **One analysis** (`audio::Builder`, sans IO) is made from
the music decoded for listening, the very samples listening plays and
with the same start (the web's MP3 lead, `ddi_platform::mp3::align`),
so second `s` is the same sample heard and drawn; it is fed the
samples in order and stepped in slices, and makes the same analysis
in any slices (tested). Desktop runs it whole on the thread that made
the overview (about 75 ms for a 12 s song); the web reads the
`AudioBuffer` a block at a time and steps it 4 ms an animation frame.
The overview is now made in the same pass (from the mid channel), so
there is no second pass and no second decode. **The waveform**: the
mid channel kept to the sample as 16-bit (2 bytes a sample) with a
min/max/sum-of-squares pyramid of blocks of 16, 256, 4,096… (half a
byte a sample); a row's least and greatest sample and RMS come from
whole blocks and the samples at its ends, exact at any zoom down to a
sample. **The spectrogram**: a Hann window of the power of two nearest
21.3 ms (1,024 at 44.1 and 48 kHz) every 5 ms (240 samples at 48 kHz),
a frame's time its window's middle (zero outside the music), our own
radix-2 transform (a real one through a complex one of half the
size), 128 log-spaced bands from 30 Hz to 12 kHz (each the energy of
its bins; a band narrower than a bin, the bins' in proportion), a byte
each: 96 dB under full scale to full scale, drawn 70 dB under the
song's loudest band to it. A row spanning frames shows their loudest
per band; a row shorter than a hop, the frames either side of its
middle in proportion. Memory: a 5-minute song at 48 kHz keeps about
36 MB for the waveform and 7.7 MB for the spectrogram (its decoded
stereo music for listening is 115 MB); the raw samples are kept
rather than a coarser level so a 192nd's rows (about 0.4 ms) stay
exact. **The mapping is the field's**: each pixel row is the span of
song seconds between its edges as the field's own `Mapping` has them
(`audio::row_spans`): between the timing's changes (`audio::knots`: a
change's row before and after a delay or a stop on it, a warp's ends)
a second moves to a row in a straight line, found from two seconds
inside each piece; the play view's scroll action (Boost, Brake, Wave)
bends it after, undone exactly (`ScrollState::distance_at`), so rows
are exact at any zoom and place, in both views, Reverse too. In the
edit view (and an x-mod's play view) a stop or a delay has no height:
its seconds are all in its row (the hit that starts a stop shows on
its row, the rest of the stop folded in), marked by a line across the
column; a warp's beats have no seconds (shaded); a c-mod's play view
gives a stop its height. **Placement**: a column at the field's left
(the side away from the panel), spectrogram (low to high left to
right) then the waveform nearest the lanes, inside the field's rows,
with the grid's lines at the snap, the measures and the cursor line
drawn across it; where the field has no room beside the lanes (the
compact layouts, 844×390 and 390×844) it is drawn behind the lanes at
40 % and starts hidden. Colors from the theme (`Editor.Audio.Ramp1`–`5`:
the ground up through violet, magenta, the accent and gold to near
white; `Editor.Audio.Wave`, `Editor.Audio.Rms`). **Keys**: G shows or
hides it, Shift G shows the waveform, the spectrogram or both, in turn
(W and Shift W until 2026-10-30, when W A S D joined the arrows);
the menu has both, and the panel a line ("Audio: waveform and
spectrogram (G)", a tap toggles); kept with the screen as V and F are.
A tap on the column moves the cursor to its row (as left of the lanes),
the wheel scrolls. **Drawing**: the screen paints an RGBA picture of
the column at its whole physical pixels (`audio::AudioPicture`) only
when the mapping, the timing, the place, what it shows or the analysis
changed, and hands it to the shell (`EditorHost::show_audio`), which
writes it into the texture the 2D pass shows as `editor-audio`
(`ddi_render::editor_audio::AudioTexture`; desktop in `Gpu::draw_over`,
the web in the editor page's frame hook beside the field's); the
buffer of the picture before last is painted over again. Nothing is
painted between changes; while listening, every frame: in the edit
view the rows move as one, so rows painted are kept by their place on
the field's grid of rows (ticks) and only the rows coming into view
are painted, the picture placed on the whole pixel nearest (at most
half a pixel from the field's place; a picture at rest is painted
whole, exact); the play view is painted whole each frame. **Cost**
(measured): the rows' seconds 7–20 µs; painting a whole full-height
column at a ratio of 2 (320×1,168) 290–450 µs natively in the desktop
profile, 370–540 µs in the release (size-optimized) one; while
listening in the edit view, a 161×584 column 75 µs median on
desktop's offscreen test (test profile, 0.5 ms painted whole), 0.1 ms
median (0.4 ms at most) in headless Chrome and 1 ms (4 ms at most) in
Playwright's Firefox, whose wasm ran everything about nine times
slower than Chrome's here (its MP3 alignment too); a whole picture
2.6 ms in Chrome, 9 ms in that Firefox. The debug state
(`EditorView::audio`, the web's `editor.audioView`: `ready`, `coming`,
`shown`, `mode`, `layout` column/behind, `rect`, `onsets` (the rows
where the waveform rises past half the loudest after a quiet row, in
the music's order), `painted`, `micros`).
Verified: `audio.rs`'s tests (the transform; a full-scale sine near the
top of its band; the pyramid against the samples; a click at its
second in the waveform, and a short tick's loudest frame within half a
hop of it, after an MP3-like lead; sliced equals whole; empty music);
`editor_audio_tests.rs` (clicks at 128 BPM from an offset of 0.123 s,
170 BPM from beat 16, a 0.4 s stop at beat 24, a click inside the
stop, the music led by 25.1 ms: each click's onset row is its beat's
row within a pixel at rows of 1 and 0.5 pixels, in the edit view at 22,
88, 352 and 1,152 pixels a beat and in the play view at 1x, 3x, C450,
2x Reverse Boost and 1.5x Brake, with no other onset; the stop's
seconds in its row alone in the edit view, the click inside it on its
own row in a C300 play view; rows tiling the seconds; at 1280×720 the
column left of the lanes with its picture on whole pixels, the cursor
line on its click's onset, Shift W's three modes, W hiding and showing
it, the panel's line, the menu; a tap on the column, the wheel;
painted every frame while listening and not between changes; the play
view; at 844×390 and 390×844 hidden at first, W showing it behind the
lanes translucent, a tap there a lane's; no music, no view);
desktop offscreen (`crates/desktop/tests/editor_audio.rs`: a song
folder of clicks read by the library, analyzed on its thread, drawn on
the GPU: each click's onset in the picture's waveform within 1.5 px of
its beat's line in the edit and the play view, the grid's beat lines
drawn across the column, the debug onsets the picture's, painted each
frame while listening; the analysis of 12 s in about 75 ms);
headless Chrome (WebGPU) and Firefox (WebGL2,
`target/scratch/pw/editor-audio.mjs`): Beatucada's column (analyzed
about 2 s after opening in Chrome, 16 s in Playwright's Firefox, at
4 ms a frame), Shift W and W, a click on the column moving the cursor
without a note, painted every frame while listening, the play view;
and a new song from a generated MP3 of clicks on 120 BPM's beats (the
new song's own timing): every click's onset at the same distance from
its beat's line (spread 0 px) at 96 and 704 pixels a beat, the
onset's row holding the 51.18 ms `__DDI_MP3` reports (the Info frame
and LAME's delay, as StepMania hears the file). Not verified: by ear,
Safari, real Firefox's speed.

## Decided here (the maintainer may change any)

- The writer's module is `formats::sm_write` (it writes both formats).
- `.sm` export refuses nothing silently: what it would lose is listed
  (`sm_losses`) and the menu offers `.sm` only when nothing is.
  `.sm` cannot hold warps here: StepMania's negative-BPM gimmicks read
  back as warps, and writing warps back as negative stops does not read
  back as the same model in every case, so such songs are `.ssc`.
- Charts are written in a 4-beat measure with the fewest rows of
  StepMania's set that hold them; timing lists on one line (some
  readers do not trim line breaks inside numbers); no radar values in
  `.ssc` and five zeros in `.sm` (StepMania ignores both); no
  `//---` banners (danceparser's buffer trips on them).
- Bundled songs open but save as a copy only (export); a zip's songs
  too on desktop.
- An `.sm` (or `.dwi`) song saved on desktop gets a `.ssc` beside it,
  the old file untouched; saving over a `.ssc` asks first and keeps
  `.old` once each time the editor is opened.
- The field is drawn by the play renderer in the player's skin, the
  timing marked over it by the menus' actors (the overlay is what is
  exact; the art is where play puts it, by its own anchor); the plates
  drawn with the menus' actors stay as the compact field and where a
  shell draws no skin (the harness).
- The edit view spaces rows by beat whatever the timing (StepMania's
  editor) and is the default; the play view shows the moment as play
  does, with the player's speed and Reverse.
- At dense snaps the edit view's zoom is raised (rows a third of a
  note apart) rather than the art shrunk.
- The skin's clock rests (two seconds in, beat 0) while the music does
  not play, so its receptors and Lua stay still; the notes are drawn
  at the cursor's beat, as play has them at that moment (a vivid
  note's color moves with the cursor, as in StepMania's editor; at
  beat 0 every picture showed the colors of a measure's first beat).
  While it plays, notes passing the line are hit as by an autoplay
  (W1).
- The skin's Lua is told the chart as the editor opened it (its BPMs
  and the like); edits do not read the skin again.
- V toggles the view and F the compact field (StepMania's editor has
  neither key).
- Lanes are the number keys (StepMania's editor); holds grow while a
  lane key is held and the cursor moves; Tab or a pad's Select opens a
  menu of every action for controllers and phones.
- A playtest from the cursor starts 2 s before it; playtests are never
  kept, have no ghost, and the library is not told their charts.
- On the web the song being edited lives for the session
  (`crate::editor`), so a playtest's page and the way back keep the
  cursor and the undo; a reload starts from the stored song.
- The editor's chart index after removing a chart is clamped; a
  removed chart's tags come back in their places on undo.
- Changing a song's music file name in the editor plays the old file in
  playtests and listening until saved (both read the library's entry).
- Listening and recording take off the audio offset of the output's
  profile when the settings have one, else the default (the web's
  context may not know its latency at its first press: then the
  profile may not be found, and the default is taken).
- One autosave slot, the last song edited; recovered without asking
  (leaving without saving drops it).
- The web's listening may start after the press when the music was not
  decoded yet (its context runs by then); the maintainer's Safari check
  should cover a first press right after opening.
- The metronome clicks quarter-note beats (StepMania's beats) and
  accents a measure's first by the time signatures; the field's
  measure numbers stay four beats.
- The metronome and the clap are kept with the editor while it is open
  (the web's across a playtest), not in the settings; they start off.
- Tap-to-tempo's BPM is applied to the hundredth; its offset only from
  the menu (it moves the whole grid), Shift Y sets the BPM alone.
- The overview spans from the music's start (or beat 0 before it) to
  its end (or the last note after it); on a small screen the panel
  shows one line of the sound tools at most (the menu has the rest).
- "New song" sits in Add songs (reached in both layouts, and where
  songs come from); the start menu stays as `start-menu.md` has it.
- A new song is made when its music is picked (its folder, or its
  stored copy, with the music and an empty chart), so listening and
  playtests work before a save; left before its first save it is
  removed (desktop: its folder; the web: its stored song).
- Note record: R replaces the notes in the rows recorded, Shift R adds
  (StepMania's record keeps them); a measure's pre-roll with a
  count-in; recording from the cursor runs until stopped; mine, lift
  and fake tools record taps and holds as StepMania does.
- Desktop's new songs go in `Dito Edits` in the first song folder
  Options lists, or in `Songs` in the data folder (then listed in
  Options) when none is; the web's are imported songs of the pack
  `Dito Edits`.
- A new song's chart is the model's: one empty Medium dance-single
  chart, meter 1, at 120 BPM with offset 0; its title and artist are
  the music's tags, else the title is its file name (`_` as spaces).
- The audio view is the field's mapping exactly; a stop's (or a
  delay's) seconds are folded into its row in the edit view rather
  than left out, so the hit that starts a stop shows on its beat;
  marked across the column.
- The audio view starts shown where the field has room beside the
  lanes, hidden where it would be drawn behind them; G and Shift G
  are kept with the screen, not the settings.
- The analysis keeps the mid channel to the sample (16-bit) for the
  waveform; the spectrogram uses a Hann window of about 21 ms every
  5 ms in 128 log bands, our own small transform (no new crate).

## Left

- Notes inside warps and overlapping holds from imported files are
  kept as read; the editor does not report them before a playtest.
- Song-level `#LASTSECONDHINT` and other cache-only tags are not kept
  (the reader drops them).
