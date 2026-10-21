# Implementation plan: phase 10 (2026-10-21)

Phase 10 of the roadmap (`docs/PLAN.md`, section 5): what was left
"later" once the game played. Replays and the ghost came first, with
the play history (`play-history.md`, decision 49). This plan covers the
rest but the rulesets:

1. **Importing plays**, the export read back on either shell.
2. **FFR `beatBox` import**, for the chart data a player has.
3. **The editor**: a plan of its own (`editor.md`), no code yet.

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

See below (filled in as it is built).

## Step 3: the editor

`docs/plans/editor.md`.
