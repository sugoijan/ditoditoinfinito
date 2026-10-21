# Plan: a chart editor, first version (2026-10-21)

The last item of phase 10 (`docs/PLAN.md`, section 5): edit a song's
timing and notes in the game, try them at once, and keep them. A plan
only; nothing is built yet (`docs/plans/phase-10.md`, step 3).

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
  a **writer** (new: `ddi_chart::formats::ssc_write`), written so that
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

## Open questions (decide when building)

- Whether a bundled song may be opened (read-only, "save as" only), and
  how a song read in place from another game's folder is saved (in place
  by default on desktop, with the backup).
- Whether the first version needs a waveform at all, or the metronome
  and the overview are enough to sync.
- How the editor's playtest reports a chart's problems (notes inside a
  warp, overlapping holds) before playing, or only on save.
