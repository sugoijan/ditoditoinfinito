# Implementation plan: phase 10 (2026-10-21)

Phase 10 of the roadmap (`docs/PLAN.md`, section 5): what was left
"later" once the game played. Replays and the ghost came first, with
the play history (`play-history.md`, decision 49). This plan covers the
rest but the rulesets:

1. **Importing plays**, the export read back on either shell.
2. **FFR `beatBox` import**, for the chart data a player has.
3. **The editor**: a plan of its own (`editor.md`); a first version
   built.

More rulesets (DDR's older scoring, FLARE, LIFE4) wait for the rulesets
work under way on the engine's presets and the rules options, and come
after it.

## Decided here (the maintainer may change any)

- **An import adds; it never replaces.** A play already kept (same id) is
  left as it is, whatever the file says of it; plays twice in one file
  count once. The report says how many were imported, already kept, from
  a newer version, and not plays, with the first problem's line.
- **Nothing worked out is trusted.** A line's record, settings snapshot,
  log and timing are kept as they are (the record's text as the export
  wrote it: the same JSON, its keys sorted); its summary
  is made again from the record. The score the line carried, when it is
  this play's (same chart, rules and time), is kept as unconfirmed: its
  judge version set to 0, so the play is stale and judged again from its
  log by the existing re-working (`Reworker`) once its chart is in the
  library. Until then a best it gives is marked pending, as after a judge
  change. A forged summary can show at most until its chart loads.
- **Plays of charts this library does not have are kept**, stale, as the
  history keeps plays of removed songs; they are judged when the song
  appears (an import of its pack, a song folder added).
- **Refused lines:** an id that is not a play id (`<15 digits>-<8 hex>`:
  nothing else names a file on desktop); a record that is not JSON, has no
  version or does not read as a `PlayRecord`; times or offsets out of
  range (a time before 1970, an offset or a logged press beyond a day);
  no song id, or one longer than 1 KB; a log or timing that does not
  decode. Plays from a newer game (a record format, a judge or a log
  format newer than this build's) are counted apart: "update to import
  them".
- **Bounds:** a line of at most 4 MB (a long chart's play takes about
  20 KB); at most 200,000 plays a file (5,004 short plays exported to
  40 MB); the file read 1 MB at a time and its plays kept 200 (web) or
  100 (desktop) at a time, so neither the file nor its plays are ever in
  memory whole. A byte-order mark, Windows line breaks and blank lines
  are read through.
- **Where it is offered:** Options → Data → Play history, a button
  "Import…" between Export and Reset (alone while nothing is kept), and
  on the web an export dropped anywhere on Options. While an import runs
  the row says "importing plays…" and offers only Export.
- **Web gesture rule:** Import clicks a hidden `<input type="file">`
  inside the press (a key, a click or a tap); a controller's press is no
  user activation, so the notice asks for a key, a click or a tap, or a
  drop, instead.
- **Desktop writes on a thread.** Each play's two files are synced
  (`F_FULLFSYNC` on macOS, milliseconds each), so the import reads and
  writes on a thread of its own; the event loop takes each written batch
  into memory (`Plays::adopt`) and shows the count so far.

## Step 1: importing plays

Done (2026-10-21).

- **Shared, sans IO** (`crates/ui/src/state/history/import.rs`):
  `PlayImport` takes the file in parts (`push`, `finish`), reads each
  line (`read_line`: checked, its summary made again), leaves the ids
  given as kept and those seen, and hands the plays over a batch at a
  time (`take`); `ImportReport` and its `notice`. `Plays::import` keeps a
  batch over any store (memory at once, the store behind with
  `PlayStore::append_all`, all of it taken back if the store fails);
  `Plays::adopt` takes plays a shell wrote itself. `History::contains`.
- **Platform:** `PlayStore::append_all`, by default every append asked
  and one answer; IndexedDB writes a batch in one transaction.
- **Web** (`crates/web/src/history.rs` `import`, the options page): the
  file read with `Blob.slice` a megabyte at a time, a batch kept before
  the next part is read, the background pass started for the new plays;
  `__DDI_DEBUG.history.import` (the last report) and
  `__DDI_DEBUG.options.importingPlays`; the hidden input
  (`input[data-ddi=import-plays]`) and the page's drop listeners.
- **Desktop** (`crates/desktop/src/history.rs` `Rework::import`,
  `Songs::import_plays`, `options.rs`): the picker (rfd, "Play history":
  `.jsonl`, `.ndjson`, `.json`, `.txt`), the import's thread, its batches
  adopted in `Rework::poll` and the report to Options while it is open.
- **Options** (`options_model.rs`, `options_shell.rs`): the button,
  `Act::ImportPlays`, `World::importing_plays`,
  `OptionsPlatform::import_plays`.

Checked:

- Natively (`import_tests.rs`, 12 tests): an export imported into an
  empty history fed a byte, 7 bytes, 4 KB and all at a time, the records
  (settings with fields this build does not know), logs and timing as
  they were, every play stale, then judged again to the summaries and
  bests it had, and its export the first export again; duplicates in the
  history and in the file; 16 kinds of broken line (not JSON, an array,
  no log, bad base64, logs and timing that do not decode, ids that would
  leave the folder or are not lowercase hex, a record that does not read,
  is text or null, version 0, a negative time, an offset of 10¹² s, no
  song, a press at 10⁹ s) counted with their lines, the good line among
  them still read; newer records, judges and logs counted apart; a line
  past 4 MB fed in parts never held, between two plays and as a whole
  file; a byte-order mark, CRLF, blank lines and a line that is not
  UTF-8; a forged summary kept unconfirmed and judged again to the truth;
  a play of a song not here kept stale until its chart is given; a store
  that fails leaving nothing; adoption once; 200,003 lines cut at
  200,000; ids. The options screen: the buttons with and without plays
  and during an import, Import pressed inside the gesture.
- Desktop (`crates/desktop/tests/import_plays.rs`): three plays of
  Beatucada and of a song not here kept in a plays folder, exported,
  imported into another on the import's thread (with a line that is not
  a play), judged again on the history's thread to what they were, the
  files' raw data equal, read again after a restart, the same file again
  importing nothing, a missing file reported.
- Exchange, both ways: the desktop's export (four plays, through the
  ignored `a_desktop_plays_folder_exports_for_the_web`) imported in
  headless Chrome (WebGPU) and Firefox (WebGL2) through Import…
  (Playwright's file chooser opened by the Enter press) — four imported,
  the three of Beatucada judged again in the background to the desktop's
  summaries, one left stale; the same file dropped on the page: four
  already kept; a file with a newer play, a broken line and a new play,
  CRLF-separated: one each. Then a web autoplay of Beatucada and the
  web's Export (six plays) imported on desktop
  (`DDI_WEB_EXPORT=… cargo test -p ddi-desktop --test import_plays --
  --ignored`): every play judged again to the web's summary, the means
  and spreads equal to the nanosecond (wasm's and native sums differ in
  their last bits, as a live play's and its replay's do).

Left: a progress bar (the notice counts); importing a pack's plays only;
merging a play kept on both sides under different ids (never the same
play: ids are random).

## Step 2: FFR `beatBox`

Done (2026-10-21): read, then offered on both import paths the same
day (see "Offered on import").

### What a player has

FFR serves its levels only to its own client, through a session
(`r3-songLoad.php`, research A4); R^3 (the AIR client) caches them
encrypted. What a player can legitimately have is a **level SWF**
(`level_<id>.swf`, from the old Flash client's cache or from a
stepartist's own files), and a **`beatBox` as text** (what a SWF
decompiler shows of a level they have, or a stepartist's notes).
Nothing here downloads from FFR, decrypts R^3's cache or reads its
playlist; levels and their music never enter the repository (the tests
build their own SWFs).

### Decided here (the maintainer may change any)

- **Accepted input:** an uncompressed (`FWS`) or zlib (`CWS`) SWF; LZMA
  (`ZWS`) refused (no level is known to use it, and it would need a
  decoder). And the text form: `_root.beatBox = [...]`, `beatBox = [...]`
  or a bare array (JSON is one), `//` comments, single or double quotes,
  `0x` numbers, `true`/`false`/`null`/`undefined`.
- **Reading the level as R^3 does** (rCubed, read for behavior only, its
  code being AGPL; the SWF layout from Adobe's published specification):
  the `DoAction`/`DoInitAction` bytecode run on a machine that knows only
  what builds an array (constant pool, push of every type, pop,
  duplicate, registers, get/set variable and member, init array; the
  rest passed over; a tag whose actions fail is left), then
  `_root.beatBox`; the music: the MP3 sound stream's blocks less their
  4-byte header, else the first MP3 `DefineSound` less its 9 bytes.
- **Notes as R^3 times them:** `int(frame + sync) / 30` s, or
  `ms / 1000` when an entry has a fourth value, on the game's clock;
  judged half a frame later, and the music started at the level's
  `mp3Frame` (since 2026-10-22: "Timing as R^3 has it" below; before,
  a note was put at its frame's start from the music's start);
  directions exactly `L`, `D`, `U`, `R` (others left out, as R^3 does),
  colors the third value, `blue` when falsy. `sync` (the playlist's
  offset) is not in a level: 0 unless given. A note twice at one time in
  one lane is one (R^3 would keep both; one press cannot take two).
- **On the grid:** one `dance-single` chart on a synthetic 1,500 BPM
  (`ffr::SYNTHETIC_BPM`), 1,200 ticks a second: a frame is exactly 40
  ticks (half a frame 20), a millisecond-timed note within 0.42 ms.
  Beat 0 is the music's start, or the first note when one comes before
  it.
  PLAN.md section 4 had 75 BPM (2 ticks a frame); that cannot hold
  millisecond timings.
- **Speed:** like Dancing☆Onigiri's works, an x-mod plays as on a
  150 BPM song (`FRAME_CHART_REFERENCE_BPM`; FFR has no tempo of its
  own), a c-mod as a c-mod (`formats::synthetic_bpm`, used by
  `ddi_engine::scroll::speed_for_song`); no BPM is shown (the song
  list's BPM is empty).
- **Colors:** FFR's nine names (red, blue, purple, yellow, pink, orange,
  cyan, green, white) as `Note::color`, in the hues of our quantization
  palette, shown with "songs' own colors"; other names: the quantization
  color.
- **Music timing:** the MP3 as StepMania 5.1 has it (no delay trimmed,
  decision 46); R^3 plays Flash's own decode of it (the stream's
  frames joined, its `SeekSamples` not applied), whose delay is not
  documented. To be checked against a level a player has.
- **Bounds:** 200,000 notes, a day of time, 16 levels of nesting and
  800,000 values in the text, 256 MB inflated, 20 million actions, a
  4-million-deep stack, 8 million array values a level.
- **FFR's rules** (its asymmetric 30 fps windows, Boo on every stray
  press, ±5 life on 0–100 starting at 50, the raw score and the end
  formula with combo × 1000; research A2) need a ruleset of their own:
  built since as `ffr` (2026-10-22, `rulesets.md`, decision 58). A
  level plays by it where the player leaves each chart to its game's
  rules (the importer marks the chart, `Chart::ffr`), else by the rules
  chosen.

### Built

- `crates/chart/src/formats/ffr.rs`: `Value`, `parse_beatbox_text`,
  `song_from_beatbox` (`LevelInfo`: title, artist, stepartist, sync,
  music, meter), `color`, `SYNTHETIC_BPM`; `formats::synthetic_bpm` for
  every frame-based format.
- `crates/library/src/swf.rs`: `read` (a `Level`: the `beatBox`, the MP3,
  its rate, the SWF's frame rate and version), `is_swf`; a level read
  into its song by `crates/library/src/ffr.rs`.
- `ddi_engine::scroll::speed_for_song` and the manifest's BPM take any
  frame-based format.

Checked natively: frames, sync (negative too, moving beat 0 back),
millisecond notes to the tick, colors and their default, entries that
are not notes, duplicates, the text form as decompilers and JSON write
it and seven malformed texts, nesting bounds, 5,000 damaged texts
without a panic; SWFs written by the tests (uncompressed and zlib): the
`beatBox` as Flash compiles `_root.beatBox = [[…]]` (constant pool,
`GetVariable`, `InitArray`, `SetMember`), as a variable, through a
register, doubles and floats, a path name, a sprite's init actions; the
stream's MP3 byte for byte across blocks and frames, the first MP3
`DefineSound` after an ADPCM one; broken levels (not a SWF, LZMA, cut
short, not zlib, inflating past its size, six kinds of broken actions,
a `beatBox` that is not an array), 3,000 damaged levels without a panic,
24 million actions stopped at the bound; a level autoplayed under ITG:
64 notes, all the best judgement, each judged at its frame's time; the
speed of an x-mod and a c-mod.

### Offered on import

Decided here (the maintainer may change any):

- **A level is a file, and a song.** A level SWF holds its chart and its
  music; a text `beatBox` (`.as`, as a decompiler exports a level's
  actions; `.beatbox`; a `.txt` whose name says `beatbox`) takes the
  audio file of its name beside it, else its folder's only one. Many
  levels share a folder, unlike StepMania's one song a folder.
- **The pack is the level's folder** (`Songs/FFR Levels/level_12.swf`:
  pack "FFR Levels", its cover the folder's image as StepMania finds a
  pack banner); a level at the import's root goes in the import's pack
  ("Imported", or a zip's name). The title is the file's name without its
  extension (a level does not hold its title). The id is
  `pack::song_id` of the pack and the file's name with its extension, so
  `level_1.swf` and a song folder `level_1` differ, and both shells give
  the same.
- **What is not a level is nothing:** a SWF without a `beatBox`, an `.as`
  of other actions, a file over 64 MB; no song, nothing reported. A
  level that cannot play (no notes, no music, a `.beatbox` that does not
  read) is reported. A level inside a simfile's song folder is left to
  the simfile.
- **The web stores what it read**: the `beatBox` as JSON
  (`chart.beatbox`) and the music, the SWF's MP3 taken out once
  (`music.mp3`) or the text level's audio, copied (never linked, so the
  SWF is never read again). **Desktop reads in place**: the level is the
  chart and, for a SWF, the music, taken out each time it plays and once
  for song select's sample (the previews cache). Its library cache
  version is 8, so caches from before read levels (10 since the timing
  below).
- The file pickers take `.swf`, `.as`, `.beatbox` and `.txt`; the import
  dialogs name "FFR levels (.swf)".

Built: `crates/library/src/ffr.rs` (`find_levels`, `read_level`,
`entry`, `names_in_place`, `chart_song`, `music`, `is_level_chart`),
`swf::write_level` (our own levels for tests and fixtures),
`Value::to_json`, a level's sample window (from its first note, 15 s);
the web's `import/ffr.rs` and the chart dispatch in `songs.rs`;
desktop's `scan_levels` (`library/mod.rs`, reused from the cache by the
level's and its music's files), the chart and music dispatch in
`play/load.rs` and the sample in `prepare_preview`.

Checked: natively, levels found in a listing (packs, titles, music
beside, ignored and song-folder files left), a SWF and a text level
read, their entries, read again from the SWF, the stored JSON and the
text, the music out of the SWF, files that are no level and broken
levels; desktop's scan of a folder (two SWFs, a text level with its
music, a silent level reported, a Flash file of no level passed over,
the pack's cover), the web's ids for the same files, reuse from the
cache and one changed level read again; desktop's play of three levels
headless on a fake clock (`tests/ffr_levels.rs`: the music, a 12 s MP3
of the platform's fixture tone, decoded out of the SWF; every note the
top tier). In headless Chrome (WebGPU) and Firefox (WebGL2), the same
levels (written by `DDI_FFR_FIXTURES=… cargo test -p ddi-desktop --test
ffr_levels -- --ignored`) imported as a folder through the import
panel (three songs, packs "FFR Levels" and "Decompiled", no "music
without a simfile") and a loose SWF dropped on the page (pack
"Imported"), each autoplayed from `#/play?song=<id>`: 40, 24, 16 and 24
notes, all the top tier.

Left:

- Its playlist's `sync`, title and difficulty (a player could give
  them on import: a legacy engine's levels may have a sync, FFR's own
  have none, below), and a check of the music's decode against a real
  level (Flash's MP3 delay). R^3's half frame and music start done
  (2026-10-22, below); FFR's rules done (decision 58).

### Timing as R^3 has it (2026-10-22)

Found while building `ffr` (`rulesets.md`, "FFR") and read again in
rCubed `e57179c` (AGPL-3.0: read for its behavior, nothing
transcribed):

- **A note's time is on the game's clock.** `ChartFFRLegacy.parseChart`
  makes each entry a `Note` of time `int(frame + sync) / 30` s (the
  frame and sync's sum an `int`, so made whole toward zero), or
  `ms / 1000` with a fourth value (its frame still `int(frame + sync)`,
  which only misses and spawning read).
- **Judged and drawn half a frame in.** `NoteBox.spawnArrow` gives every
  note it spawns `POSITION = (time + 0.5 / 30) × 1000` ms;
  `GameplayDisplay.judgeScorePosition` judges a press's game time
  against `POSITION`, and `NoteBox.updateNotePosition` draws the note
  at the receptor at `POSITION`. Any chart's notes, an FFR level's or a
  file's.
- **The music starts `mp3Frame` frames into the game.**
  `MP3Extraction.extractSound` counts the SWF's `ShowFrame` tags before
  the first `SoundStreamBlock` with data (the stream once named MP3;
  `metadata.frame`, −1 when the music is a `DefineSound`'s, which also
  ends the music: nothing after it is read); `Song.musicCompleteHandler`
  sets `mp3Frame = metadata.frame − 2`. `GameplayDisplay.initCore`
  makes it `songDelay` (ms, at rate 1 and without a global offset);
  `e_onFrame` starts the music once the game's time reaches it and from
  then on keeps the game's time at the music's position plus
  `songDelay` (`songOffset`, an average of the two's difference). So a
  note is heard `time + (0.5 − mp3Frame) / 30` s into the music: a
  stream from the SWF's first frame puts frame `F` at `(F + 2.5) / 30`,
  one from frame 7 at `(F − 4.5) / 30`, a `DefineSound`'s music at
  `(F + 3.5) / 30`. The reverse mod's sample arithmetic
  (`Song.onReverseSound`, `(63 − mp3Frame) × 1470`) agrees: game frame
  `F` is music sample `(F − mp3Frame) × 1470` at 44.1 kHz.
- **`sync`** comes from a legacy engine's playlist (`arc_sync`, or the
  engine's `legacySync` by level; `ChartFFRLegacy.parsePlaylist`). FFR's
  own playlist gives none: `Playlist` has a table of fixed syncs for
  FFR's earliest levels (`oldOffsets`), used when
  `isNaN(songInfo.sync)`, but `SongInfo.sync` is an `int`, 0 unless
  set, so it is never used and FFR's own levels play with sync 0, as
  the import has them. Another engine's level with a sync of its own
  plays off by it until a player can give one (left above).

Decided here (the maintainer may change any):

- **The import times notes where R^3 hears them**
  (`ddi_chart::formats::ffr::LevelInfo::music_frame`): the half frame
  and `mp3Frame` folded into the notes' ticks (half a frame is 20
  ticks: every frame note stays on the grid), so the timing holds under
  every ruleset and on both shells, which share the import. A SWF's
  `mp3Frame` is `swf::Level::music_frame` (its `stream_frame` less 2;
  −3 for a `DefineSound`'s music); the reader now also stops taking
  music after a `DefineSound`, keeps a stream named MP3 one when a
  later head names another codec, and makes a frame whole as R^3 does.
- **A text level's music starts at the game's frame 0** (`mp3Frame` 0),
  with the half frame. R^3 has no text level; the only way it plays a
  chart whose music is a file of its own is its file loader
  (`menu/FileLoader.buildSong`: the `Song`'s music from the file,
  `mp3Frame` left at 0, each note's frame `floor(time × 30)`), which
  starts the music at the game's start; and a stepartist writing a
  `beatBox` against an audio file counts its frames from that file's
  start. The stream's first frame is not in a decompiled `beatBox`: a
  decompiled level whose stream began at SWF frame `s` plays `s − 2`
  frames off, and importing the SWF itself times it exactly.
- **The web stores where the music starts**: `chart.beatbox` is now
  `{"musicFrame": n, "beatBox": […]}` (`ffr::stored_chart_song`). One
  stored before (the `beatBox` alone, its SWF's stream frame lost) is
  read as levels were then (`LevelInfo::frame_start`: each note at its
  frame's start from the music's), so its entry's digests stay its
  chart's; importing the level again times it as R^3 does.
- **Digests change; nothing is judged again.** Every level's notes
  move (half a frame at least), so its chart digest is new: as for an
  edited pack (`play-history.md`, "A chart whose notes change is a new
  chart"; `editor.md`), the new chart starts with no bests, and plays
  of the old one stay in the history as plays of a chart this library
  no longer has (their replays refused as "a chart that changed").
  `JUDGE_VERSION` stays: judging did not change, and judging the old
  plays against the moved notes would score inputs made against the
  old timing. Desktop's library cache is version 10, so desktop reads
  its levels again (their entries' digests the new ones); on the web,
  levels imported before keep their old timing and digests until
  imported again.

Checked: natively, the half frame on frame and millisecond notes and on
a sync, `int(frame + sync)`, `mp3Frame` −2, 5 and as levels were read
before, beat 0 moved by a note before the music; SWFs with their
stream from frame 0, from frame 7 (`swf::write_level_at`) and from
frame 3 past an empty block and a later head of another codec, a
`DefineSound`'s music (−3, a stream after it not music); the web's
stored form read back to the same digest, the old form to the old
ticks; the engine's level autoplayed at its R^3 times. Desktop
(`tests/ffr_levels.rs`): a SWF with its stream from frame 0, one from
frame 7 and a text level, each note judged at `(F + 0.5 − mp3Frame) /
30` s into its music (exactly, and the autoplay's presses on the
music's clock within 0.1 ms), AAA under `ffr`, the top tier under ITG,
and each play's kept log replayed to the same tally, score, FFR result
and timeline.

## Step 3: the editor

Planned (2026-10-21): `docs/plans/editor.md`. A first version for
`.ssc`/`.sm` timing and notes on the UI core, on both shells, with
playtests from the editor, saving on desktop and saving or downloading
on the web; eight steps, the first an `.ssc` writer. Built
(2026-10-21): the writer, the model, the screen on both shells,
listening, playtests, saving and the autosave; left: the field in the
player's skin, the metronome and tap-to-tempo, new songs (`editor.md`,
"Left").
