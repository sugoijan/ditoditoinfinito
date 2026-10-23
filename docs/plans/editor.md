# Plan: a chart editor, first version (2026-10-21)

The last item of phase 10 (`docs/PLAN.md`, section 5): edit a song's
timing and notes in the game, try them at once, and keep them
(`docs/plans/phase-10.md`, step 3).

Status (2026-10-21): the writer, the editing model, the screen on both
shells, listening, playtests, saving and the autosave are built (steps
1, 2, 4, 6, 7, the listening of 5 and 8; see
"Built" below); the field in the player's skin (3), the metronome,
tap-to-tempo and the overview (5) are left ("Left").

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
- Waveform display beyond a simple overview (the first version shows the
  music's amplitude per beat row; spectrograms later).
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
- The field is drawn with the menus' actors, not the play renderer with
  the player's skin: the same picture on both shells and in the
  harness; the skin can come later without changing the model.
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
- Listening takes off the settings' default audio offset, not a device
  profile's (the editor does not ask which device plays).
- One autosave slot, the last song edited; recovered without asking
  (leaving without saving drops it).
- The web's listening may start after the press when the music was not
  decoded yet (its context runs by then); the maintainer's Safari check
  should cover a first press right after opening.
- "New song" sits in Add songs (reached in both layouts, and where
  songs come from); the start menu stays as `start-menu.md` has it.
- A new song is made when its music is picked (its folder, or its
  stored copy, with the music and an empty chart), so listening and
  playtests work before a save; left before its first save it is
  removed (desktop: its folder; the web: its stored song).
- Desktop's new songs go in `Dito Edits` in the first song folder
  Options lists, or in `Songs` in the data folder (then listed in
  Options) when none is; the web's are imported songs of the pack
  `Dito Edits`.
- A new song's chart is the model's: one empty Medium dance-single
  chart, meter 1, at 120 BPM with offset 0; its title and artist are
  the music's tags, else the title is its file name (`_` as spaces).

## Left

- **Step 3**: the field in the player's skin and speed (the play
  scene paused at the cursor, `crates/render`), with offscreen tests.
- **Step 5, the rest**: the metronome (calibration's click on the
  beats while listening), tap-to-tempo, the music's overview strip.
- Notes inside warps and overlapping holds from imported files are
  kept as read; the editor does not report them before a playtest.
- Song-level `#LASTSECONDHINT` and other cache-only tags are not kept
  (the reader drops them).
