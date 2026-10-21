# Implementation plan: the play history (2026-10-16)

Every play is kept, so the game can show a player's best scores (song
select's chart ladder, `docs/plans/ui-core.md` step 6.2) and, later,
replay a play (phase 10). Asked for by the maintainer (2026-10-15): keep
the raw play data and the metadata worked out from it, and work the
metadata out again when something changes.

## What a record holds

**Raw (never changes):**

- **The chart.** The song's id plus a digest of the chart's content
  (`ChartKey`).
  - Song ids alone are not enough: an imported song's id is a hash of its
    folder path (`crates/library/src/pack.rs:641`), so a pack updated in place
    keeps its ids.
  - The digest covers what judging reads: notes, timing (BPMs, stops,
    delays, warps, scrolls, speeds, fakes, combos), the offset and the
    layout. It leaves out titles, art and credits. A chart whose notes
    change is a new chart, with no bests yet.
  - The chart's index is kept too, to find it again.
- **The rules.** The ruleset as it played (`RulesetKey`): its id plus what
  varies under one id. That means the hold style (ticked or dance) and
  Dancing☆Onigiri's gauge, judge range and excessive setting. From the
  key and the chart the same `Ruleset` is built again.
- **The options.** `PlayOptions`: speed, reverse, scroll action, turn,
  cut, appearance, receptor snap, and the seed (it orders a Shuffle).
- **Every parameter of the play** (the maintainer, 2026-10-16): all
  settings that shaped it, not only what judging reads.
  - **Calibration and the clock:** the audio and visual offsets in effect
    and where they came from (the device profile and whether it was
    calibrated, or the defaults). Also the audio path and display
    fingerprints with their descriptions, the output latency, the sample
    rate and the refresh rate as the play started, and the latency's
    range over the play.
  - **The song offset** in effect. The engine plays a song shifted by it
    (`Song::shift_notes`), and re-judging shifts it the same way.
  - **How it was drawn:** the arrows (noteskin), note colors, the field
    filter and background brightness, the background frame and videos
    setting, the timing-error display, the graphics backend.
  - **How it was played:** the input devices used (keyboard, pads by
    model, touch).
  - A snapshot of the settings as stored, so a field added later is kept
    too.
- **The input log.** Every press and release as the engine judged it:
  lane, pressed or released, and the judged song time. This is the time
  after the audio clock, the output latency and the offsets. Re-judging
  needs no clock, no device and no audio. With the offsets kept, the
  times before them are known exactly (judged time plus the audio
  offset), so a play can also be judged again under a corrected offset.
  - Danoni's `startFrame` start (`song_offset` of `Player::start`) is kept
    with it.
- **The notes' timing as played** (the maintainer, 2026-10-16): every
  judgement with its note, lane, time and offset, and the life over the
  song (`Results::timeline`, `Results::life`), as the play judged them.
  Re-working never changes this record of what happened. A re-worked
  play shows the new judgement beside it when they differ. About 8 bytes
  a judgement.
- **How it ended:** finished, failed, or quit. Quits are not stored for
  now (below).
- **Flags:** autoplay (any kind: `auto=1`, `auto=pad`), and the bias of an
  autoplay.
- **When,** and the app's version.

**Worked out (rebuilt when stale):**

- Score, grade, lamp, assist, failed, the tally, max combo, fast and
  slow, mean and σ, and a judge version (`JUDGE_VERSION`, below).
- The results screen of an old play draws the stored timing; after a
  re-working, the re-judged one.

## Re-judging exactly

Judging today depends on when `Player::update` is called, not only on the
inputs. Mine crossings and hold decay and regrab are measured between one
update and the next (`crates/engine/src/judge.rs:975, 997, 1013-1016`). So a play
on a 144 Hz display and the same presses at 60 Hz may differ.

That is a fairness problem on its own, before any history. So the first
step makes judging independent of the update cadence:

- Every rule is measured in song time, between events: an input, a note's
  time, a window's edge, a mine's crossing.
- A property test feeds the same inputs at random update cadences (from 1
  to 50 ms, uneven) under every ruleset, hold style and transform. It
  expects identical judge events, tallies, scores and life.

With that, the log is the inputs alone. A replay feeds them at their
times, with an update at each input and at the end.

If a rule must stay frame-bound (StepMania itself is), the log keeps the
update times for that ruleset: delta-coded, about 3 bytes a frame. The
test says which. (None had to: step 1 below.)

**Size.** An event takes a lane and pressed byte plus a varint delta in
microseconds, about 3 bytes. A 1,000-note chart with holds is about 6 KB
of inputs and 8 KB of note timing, and the parameters and summary about
2 KB. Ten thousand plays take tens of
megabytes on IndexedDB or on disk. Measured (2026-10-16): a 296-note
chart's play keeps a 2.4 KB record (1.2 KB of it the settings), 2 KB of
log and 2 KB of timing; a long chart's play is about 15–22 KB in the
export, base64 included (5,004 such short plays export to 40 MB).

## Versions and re-working

- **The judge version.** `ddi_engine::JUDGE_VERSION` is a number raised
  whenever judging, scoring, grading or a gauge changes what a play would
  show. A summary with an older version is stale.
- **When stale summaries are worked out again:**
  - when the chart is at hand (a song loaded for play or preview);
  - in the background on song select, a few at a time, from the chart
    alone (no music).
- **While stale,** a best keeps showing its old value, marked as
  pending. Records whose chart is gone (a removed pack) stay, unmatched,
  until the player removes them.
- **The raw format** has a version of its own. The reader keeps reading
  every past version.

## Bests

- **The key:** a best is per chart content (the digest, not the song id)
  and ruleset (`RulesetKey` without the gauge choice). Scores under
  different rules are not comparable (percent, dance points, money). The
  song id is only shown: a pack imported again from another folder gets
  new ids, and its charts keep their bests. The song list's entries keep
  each chart's digest (`ChartInfo::digest`, from `xtask gen-songs` and
  the import; songs imported before get theirs when their chart is next
  read), so song select finds the bests without reading charts.
- **Which plays count:** autoplay plays are kept but never count; assist
  plays (cuts) count separately, marked as assist, as StepMania marks
  them.
- **For each chart and ruleset:** the best score, the best lamp (they can
  come from different plays), the play count and the last played date.
- **Song select** asks the history for the chart ladder's rows. A
  ruleset other than the one selected shows nothing, or "played under
  ITG", as the theme decides.

## Storage

- **The trait:** `ddi_platform::history::PlayStore`. Writing and reading
  is asynchronous on the web, and the trait keeps that out of `ui`:
  - append a record;
  - list the summaries;
  - read raw records by id;
  - replace a summary;
  - remove records;
  - export everything.
- **Web.** IndexedDB, `ddi-library` version 3, with two new stores,
  both keyed by `<time>-<random>`: `plays` (the raw record as JSON, the
  log and the timing as bytes) and `play_summaries` (the summary as
  JSON), so the start reads the summaries without the logs.
  - The upgrade only creates the stores, as before.
  - The summaries load once at start (a few hundred KB for thousands of
    plays) into memory.
- **Desktop.** Two files a play under the data folder:
  `plays/<time>-<random>.json` (the play as played, the log base64 in it,
  never rewritten) and `<time>-<random>.summary.json`; the start reads
  the summaries alone. It is written by the shell, through the same
  trait.
- **Export.** JSON lines, one record a line, everything included (what
  does not read as JSON kept as a string, a missing summary as `null`),
  read and written a batch at a time. Imported back on either shell since
  2026-10-21 (`phase-10.md`, step 1).
- **Removal:** all, a chart's, or a pack's, and the Reset section's
  "remove imported songs" leaves the history alone unless asked.

## What is not kept (for now)

- **Quits** (Escape, the touch quit button): a quit is not a play. To
  revisit if players want "attempts".
- **Calibration plays.**
- **Plays on the developer routes.**

## Steps

1. **Engine: judging independent of the update cadence.** The property
   test across rulesets, hold styles and transforms; fix what differs. A
   replay-fixture test per ruleset.

   Done (2026-10-16). A press or a release first judges everything up to
   its time with the lanes as they were (`Judge::advance`): misses, mine
   crossings, hold decay and drops, ticked heads and checkpoints, each at
   its own instant. A lane's state is kept from its edges alone, so a mine
   is stepped on when its row passes a lane that is down, and a hold
   decays from its release. That is StepMania's `Player::Update` /
   `UpdateHoldNotes` / `CrossedRows` at an infinitely fast frame rate.
   One convention for an instant: what happens at `t` is judged once
   judging is past `t`, so an edge at `t` comes first (StepMania handles a
   frame's input before its update); each span's events are ordered by
   time, lane and note, so however time is cut into updates the events
   come in one order. Judging never goes back in time. In a live play,
   time-judged events lag the clock by `JUDGE_LAG` (30 ms): an edge that
   reaches the player late (a key's time is before the frame's update
   that dispatches it; an output latency reported anew steps the heard
   time back) is judged at its own time; later ones count when judged and
   are counted (`Results::late_edges`). Times are kept to the
   microsecond. `crates/engine/tests/cadence.rs` plays generated charts and
   presses (mines, shocks, holds let go and taken again, rolls, lifts,
   checkpoints, a stop, a tempo change, stray presses) with updates every
   millisecond, at 60 and 144 Hz, at uneven 1–50 ms steps, and exactly at
   every edge, note time and window boundary; under ITG, StepMania 5 and
   DDR A with dance and ticked holds, a pump chart, Dancing☆Onigiri (two
   option sets) and the turns and cuts; with and without failing: the
   same events, results and life everywhere. Events after an immediate
   fail are no longer returned by `Player`.

   After an independent review (2026-10-16): the instant convention above
   (an update exactly at a hold's tail could put it before a miss at the
   same instant, and a replay judge otherwise); a hold hit early and let
   go before its head decays from its head (StepMania judges the body from
   the head row); times that are no number are ignored; judging walks
   each lane without copying it (16,700 notes: 85 ms → 8 ms). The
   "edges" cadence now also updates at every event's own time, a
   Dancing☆Onigiri chart on its frame grid is tested, and
   `crates/engine/tests/instants.rs` keeps the review's probes (the tie, the
   early release, a NaN, a late edge, presses snapped to window edges,
   noise for the codecs, a long play's time). `JUDGE_VERSION` stays 1: no
   record had been kept.
2. **Engine: the input log and replay.**
   - `Player` keeps the log in judged song time (`PlayLog`).
   - `replay(song, chart, RulesetKey, PlayOptions, start, &PlayLog) ->
     Results`.
   - `RulesetKey` round trips (`Ruleset::key()`, `RulesetKey::build(chart)`)
     and `JUDGE_VERSION`.
   - Tests: every autoplay and replay fixture, live-style with clock
     samples, drift and latency, replays to identical results.

   Done (2026-10-16). `ddi_engine::history`: `PlayLog` (`Player::log`,
   every edge that reached a started play, at its judged time),
   `PlayLog::encode`/`decode` (one varint an edge: the time delta in
   microseconds above the press bit and the lane; 7.7 KB for a
   1,041-note chart's 2,098 edges), `PlayLog::shifted` (another audio
   offset), `replay` and `replay_with`, and `PlayTiming` (the timeline and
   life, exact, 7 bytes a judgement; a tap's offset is kept to the
   microsecond in `Results::timeline`). `RulesetKey` (id, the holds as
   played, Dancing☆Onigiri's options with the excessive setting in
   effect; `best_key` drops the gauge),
   `Ruleset::key`, `RulesetKey::build`, `JUDGE_VERSION = 1`. Every replay
   fixture (`replay.rs`, `ticked.rs`, `shocks.rs`, `danoni.rs`) and every
   corpus autoplay (`local_autoplay.rs`) is judged again from its log to
   the same results; `crates/engine/tests/history.rs` adds autoplays and random
   plays under every ruleset and transform, live plays with a drifting,
   jittering clock, latency and offsets, a part-way start, a log shifted
   to another offset, the codecs and the sizes.
3. **Chart digest** (`ddi_chart`): stable across parses and platforms,
   blind to metadata, changed by any note or timing change; tests on the
   bundled songs and the local corpus (`DDI_DIFF_CORPUS`).

   Done (2026-10-16). `ddi_chart::ChartDigest` (128-bit FNV-1a, 32 hex
   digits) and `Song::chart_digest(index)` over the notes (tick, lane,
   kind, end, a boost), the chart's timing (offset, BPMs, stops, delays,
   warps, speeds, scrolls, fakes, tick counts, combos), the layout (id,
   lane count, the lanes' shuffle groups and its quarter turn, what turns
   read), and a Dancing☆Onigiri chart's speed, gauge, headers, place
   in the work, start and end; numbers as exact bits. `crates/chart/tests/digest.rs`:
   a pinned value, distinct digests across the bundled songs, the same
   after other metadata and Windows line endings, changed by a moved note,
   a BPM, the offset, a stop, the layout. Over 1,222 local files (9,285
   charts): every digest the same from two parses and from other
   metadata; 10 digests shared, all by charts with the same notes and
   timing (one song's two versions; two identical charts in one file).
4. **The model** (`ui::state::history`, platform-free):
   - records and the binary log codec with its version;
   - summaries and bests;
   - the stale check;
   - the `PlayStore` trait in `platform`;
   - tests with an in-memory store.

   Done (2026-10-16). `ddi_platform::history`: `PlayEntry` (id, summary
   and record as JSON texts, log and timing as bytes; `to_json`/`from_json`
   for the export and desktop's files, with base64), the `PlayStore` trait
   (append, summaries, records by id, replace summaries, remove, export;
   each answers through a callback, now or later), `play_id` (the time in
   ms padded to 15 digits, then 8 random hex digits) and
   `MemoryPlayStore`. `ddi_ui::state::history`: `PlayRecord` (format
   `RECORD_VERSION = 1`; `ChartKey`, `RulesetKey`, `PlayOptions`, the song
   offset, `PlaySetup` with the clock and offsets and their sources, the
   devices' fingerprints and labels, latency, sample rate, refresh rate
   and the latency's range, how it was drawn and the input devices, the
   settings as stored, the ending, the autoplay and its bias, the time,
   the app's version), `PlaySummary` (the keys, time, autoplay, ending,
   and `Worked`: score, grade, `Lamp`, full combo, assist, failed, tally,
   max combo, fast/slow, hits, mean, σ, judge version), `History` (the
   summaries in memory; unreadable ones listed apart, never dropped;
   stale ones by song; bests by chart content, `best_key` and assist,
   with the best score (`score_value`: money, else percent, else EX) and
   the best lamp from any plays, the count, the last played and
   `pending` while a play under the key is stale; autoplay never counts,
   failed plays count, as StepMania keeps them graded Failed), `rework`
   (from the record and the log on the chart as stored, found by its
   digest where it is now; the song offset applied as in play) and
   `Plays`, the history over a store for the shells (load, record,
   re-work a song's stale plays a few at a time, remove all / a chart / a
   pack's songs, export as JSON lines). A play whose chart is gone or
   changed stays stale and unmatched. Tests (`crates/ui/src/state/history/
   tests.rs`): a play kept whole and read back (the settings' unknown
   fields too), judged again to the same summary, charts found by
   content, bests, stale plays worked out again and written back,
   unmatched plays, unreadable summaries, removal, a play recorded before
   the summaries load, and a pinned version 1 entry that keeps reading.
5. **Web.**
   - The IndexedDB store.
   - Recording at the end of a song's play (finished or failed;
     autoplay flagged).
   - Re-working stale summaries (when a chart loads, and a background
     pass).
   - An export (a request that downloads the file).
   - `__DDI_DEBUG.history`.
   - Checked with Playwright: plays recorded, re-judged after a version
     raise in a test build, bests kept across reloads.

   Done (2026-10-16). `crates/web/src/history.rs`: `IdbPlayStore`; the page's
   `Plays`, its summaries read at start (`history::start` at boot); a
   song's play recorded when it ends, finished or failed
   (`GameCanvas::record_play`, from `PlayFacts` the session hands over:
   the log, the options as played, the `RulesetKey`, the input devices
   whose presses reached a lane, the latency at the start and its range,
   the sample rate); quits, interruptions and calibration are not kept,
   and the play routes are the only ones that record. Stale summaries are
   worked out again when a song's chart loads for a play, and by a
   background pass (every 1.5 s, four plays of one song from its chart
   alone through `songs::load_chart`, which reads no music; it waits
   while a song plays and stops when nothing is stale; a song whose chart
   cannot be read is left for the session). The export is a button on
   the Options page ("Play history": how many plays are kept, "export
   play history"), downloading `ditoditoinfinito-plays-<date>.jsonl`.
   Removal (`history::remove_all`, `remove_chart`, `remove_songs`) has no
   screen yet; the Reset section leaves the history alone.
   `__DDI_DEBUG.history`: `count`, `loaded`, `error`, `stale`,
   `unreadable`, `last` (the latest summary and its `id`), `song` and
   `bests` (the play screen's song). Checked in Chrome (WebGPU and
   WebGL2) and Firefox: two autoplays and a failed one recorded with
   their summaries and kept out of the bests, a play pressed on the test
   pad counted, its record (pad, clock, drawing, settings, chart, rules,
   log and timing bytes), bests the same after a reload, summaries made
   stale in storage worked out again to what they were (in the
   background, and when the chart loads), the export parsed line by line,
   and a version 2 library upgraded with its songs and files kept.

   Review fixes (2026-10-16):
   - **Summaries that do not read are rebuilt.** A summary's keys
     (chart, ruleset, time, autoplay, ending) read apart from `worked`;
     a `worked` that does not read is `None`, stale, and worked out again
     from the record. A summary that does not read at all, or a play
     with none (the store lists those too), gets one from its record
     (`PlaySummary::from_record`) and is worked out again; a play whose
     record does not read either stays listed (`unreadable`) and goes
     with `remove_all`. The engine's stored types read with defaults for
     fields added later (`#[serde(default)]` on `ScoreView`, `Tally`,
     `ScrollOptions`, `ClockOptions` and `RulesetKey`'s `holds` and
     `danoni`). `Worked` keeps `late_edges` (not shown).
   - **The export streams.** The store hands the plays over 100 at a
     time (each batch its own IndexedDB transaction), each batch becomes
     a blob part at once, and the file is a blob of the parts: no single
     string of the whole history, in wasm or in Rust.
   - **Re-working is cut into steps.** Eight plays a step (a few ms for
     long charts), a song's whose chart is at hand first (16 ms apart),
     the rest in the background (1.5 s apart); never while a song plays
     (looked at before every step, after the chart is read too); the
     song's id gone, its charts are looked for by digest in the library
     (`songs::find_by_digest`).
   - **A play that could not be kept** is rolled back from memory and
     the results say so ("This play could not be saved: …"); they wait
     up to a second for the write.
   - **A failed play is kept when it fails** (`SessionEvent::Failed`,
     with final results), so a quit or a tab switch during the fail's
     hold does not lose it.
   - **A blocked upgrade waits.** Opening the library waits for another
     tab to let go of its older connection instead of failing (one open
     shared by every caller; `__DDI_DEBUG.history.blocked` meanwhile),
     and the history tries again later if it still cannot read.
   - **Bests by digest** and **digests in the song list**: above (Bests).
6. **Desktop:** the file store, tested natively (desktop plays come with
   phase 9).

   Done (2026-10-16). `crates/desktop/src/plays.rs`: `FilePlayStore`, two
   files a play in `plays/` beside `storage.json`: `<id>.json`, the play
   as played (the export's line without the summary: the log and the
   timing in base64), written once; `<id>.summary.json`, rewritten when
   the play is worked out again. The start reads the summaries alone.
   Files are written whole (a temporary file, synced, renamed); temporary
   files a crash left are removed at start; a play whose file does not
   read is listed (its summary `None`) and left in place; ids that could
   leave the folder are refused. Tested natively: plays across a
   restart, a stray file, an unreadable play, a summary without its
   play, a crash's temporary file, the play's file unchanged by a new
   summary, the export in batches, removal, and the model over it.
   Nothing records on desktop until it plays.

   **Re-working on both shells** (2026-10-20). Desktop recorded plays
   from phase 9 but never worked stale ones out again: their bests stayed
   pending. What the web's `history.rs` decided moved into
   `ddi_ui::state::history::Reworker` (sans IO, `reworker.rs`): which
   step is next (`ReworkStep`: wait, done, read a song's chart, judge a
   batch of `REWORK_BATCH` = 8), the focused song first every
   `FOCUS_STEP_MS` (16 ms), the background pass every `PASS_STEP_MS`
   (1.5 s, also the first step's wait), a song at a time in id order with
   its chart read once, a song whose chart cannot be read left for the
   session, nothing during a play or before the summaries are in.
   `Plays::rework_song` is now `Plays::stale_batch`, the pure
   `rework_entries` and `Plays::set_worked` (memory and the unmatched
   plays), which a shell may also run apart. The web runs the reworker on
   its timers as before (the same order, batches, waits and fallbacks).
   Desktop (`crates/desktop/src/history.rs`, `Rework`): driven from the
   event loop (`Songs::rework` before `Songs::poll`, which sends song
   select the bests when the history's generation moves; the loop sleeps
   until `Rework::due`), with charts read (`play::load::read_chart`, by
   digest through `library::find_by_digest` where the song id is gone),
   records read, plays judged and summaries written (a synced file each,
   about 5 ms on an Apple SSD) on a thread of its own; the summaries in
   memory are set on the event loop's thread. It steps only once the
   library's scan is done, takes a song's plays first when the play
   host has read its chart (`Host::chart_loaded`), clears that focus
   when the prompt, play and results are gone, and tries songs it could
   not read again when the library changes (the web never retries).
   Tested: `crates/ui/src/state/history/tests.rs`
   (`the_reworker_takes_the_focused_song_first_then_the_rest_in_batches`)
   and `crates/desktop/tests/rework.rs` (stale plays of a bundled song, one
   under a gone id, worked out on the thread, written, bests no longer
   pending; nothing during a play).
7. **Song select's bests** (`ui-core.md` 6.2): done 2026-10-16, the
   bests on the ladder rows.

## Replays (phase 10, 2026-10-16)

A kept play can be watched again, as it looked, on both shells. The log
is in judged song time, so a replay needs nothing of the play's clock:
its edges are fed to a new play when this device's clock reaches them.

**What a replay plays with.**

- **The chart** found by its digest (`find_chart`) in the song the
  record names, else in any song with a chart of that content
  (`find_by_digest`). The record's song offset shifts it, as the play
  was shifted.
- **The rules** built from the `RulesetKey`, and **the options** as
  played (speed, reverse, turn and its seed, cut, appearance, scroll
  action, receptor snap), never the settings' now.
- **How it was drawn,** from the settings the record kept: the skin, the
  note colors, the chart's colors, the field filter, the background's
  brightness and frame, the videos setting, lyrics, the timing-error
  display. Not this device's volume, bindings or graphics backend.
- **The offsets:** the record's when this is the audio path and display
  it was played on (the play as the player saw and heard it); this
  device's otherwise, since the record's corrected other hardware.
  Either way the judged times are the log's, so the results do not
  depend on them.
- **The inputs:** the log's edges, each fed at its judged time
  (`Player::input_at`) once this play's judged time reaches it, before
  the frame's update. No edge is ever behind what was judged, so none is
  late, at any frame rate. Live keys, pads and touches reach no lane;
  the quit gesture still leaves.

**What a replay never does:** record a play, count toward a best, change
a song offset or a calibration, or report its fail to the history.

**Checked, not assumed.** At its end a replay's results are compared
with the same log judged offline (`replay`: equal) and with what the
record kept: the timing (judgements and life, exactly) and the summary
(all of it but `late_edges`, which only a live play can have), when the
record was judged by today's judge. A difference is said on the results
("This replay judged differently from the play: …") and in
`__DDI_DEBUG`, never hidden. A play judged by an older judge is judged
by today's; its results say so and are not compared with the old
timing.

**Refused with a reason, never played wrong:** a record or log of a
format this build does not read, a play judged by a newer judge than
this build's, a song that is gone (no song with a chart of that
content), a chart that changed (no chart with the record's digest), a
noteskin no longer installed.

**Where.**

- **Results:** "Watch replay" (W) once the play is kept. After a replay
  the results are marked REPLAY, Retry becomes "Watch again", and the
  song offset editor is not offered (a replay changes nothing).
- **Song select:** P opens the chosen chart's plays (also the footer's
  "Plays…" link; on a phone, a long press on a chart): every play kept of its content, newest first, eight a
  page; when, grade, score, lamp, ruleset and the options as played,
  autoplays marked, the best marked. Each can be watched, opened on the
  results screen (judged again from its log: the same results), or
  removed (pressed twice). W watches the chart's best play.
- **The play route:** `#/play?replay=<play id>` on the web
  (`&view=results` opens its results); desktop's host takes the same
  requests (`--replay ID [--results]`).
- **Summaries keep the options** (`PlaySummary::options`, optional;
  none in older summaries, whose rows then show no options), so the list
  needs no records.
- **The ghost** (off by default; Options → Display → "Ghost of your
  best"): during a play, under the score, how far ahead of the chart's
  best play (under the rules it is played by, assist apart) the play is
  at the same point, in the ruleset's terms (percentage points, or
  points), green ahead, red behind. The best play is played again beside
  the play from its log (`GhostRun`: its edges up to the time the play
  is judged to, then the same update), so the two are judged the same
  way at every frame and the last value is the difference of the final
  scores; the song offsets each was played with are lined up. Not for a
  replay or a test.

Done (2026-10-16).

- **The model** (`crates/ui/src/state/history/replay.rs`): `Replay` (a play read
  back: record, log, timing, summary; `from_entry` refuses a newer
  record format or judge and a log that does not read), `chart`,
  `played_song`, `results` (judged at once), `settings`, `clock`,
  `replaying` (what the session needs), `skin`, `check` (`ReplayCheck`),
  `play_rows` and `plays_list` (song select's list), `local_time`.
  Records keep `judge_version` (1 when absent), summaries `options`.
  `Plays::entry` and `Plays::remove_play`.
- **The session** (`crates/session/src/play.rs`): `SessionConfig::replay`; the
  record's rules and options (never the settings', never worked out
  again), its start; the log's edges fed before each update, up to the
  time the update judges to; lane edges from the shell ignored, the quit
  gesture kept; no autoplay, no fail report. `replaying`,
  `replay_progress`.
- **Screens:** the results' "Watch replay" (W; on top by the title
  beside the offset editor, in the bar on an upright phone), `ResultsKind`
  (`Play`, `Replay`: "Watch again", `Kept`: no Retry) and `mark_replay`;
  song select's plays list (`PlaysDialog`), P, W, "Plays…", a long press
  on a phone's chart.
- **Web:** `#/play?replay=<id>[&view=results]` (`crates/web/src/replay.rs`: the
  loader reads the play, finds its chart, looks for its skin, then puts
  the play screen up, or the refusal on its prompt); the play screen
  replays with the replay's settings, checks it at its end
  (`__DDI_DEBUG.replay`), never records; a play's results offer its
  replay once kept. The UI host navigates only after its own history
  pops (a dialog closing as it navigates: Firefox popped back after the
  navigation and undid it).
- **Desktop:** `Request::replay`/`view_results`/`refused`,
  `play/replays.rs` (the same resolution, synchronous;
  `library::find_by_digest`), the host's prompt, results ("Watch
  replay", "Watch again", a kept play's at once) and `--replay ID
  [--results]`; dates in the system's time zone (`localtime_r`; UTC on
  Windows for now).
- **The ghost:** `Ghost` and `GhostRun` (`replay.rs`), the session's
  `SessionConfig::ghost` and `ghost_delta`, `RenderOptions::ghost`
  (`ScoreDelta`, drawn under the score in ASCII signs: the HUD's font is
  a subset), the setting and its row; the web reads the best play when
  the chart loads, desktop when the play starts. `__DDI_DEBUG.ghost`.
- **Checked:** natively, every bundled chart (4 songs, 38 charts) autoplayed
  by the session and by desktop's play, kept, replayed at 144 Hz and at
  uneven 37 and 50 fps on another clock and latency: the same results,
  `check` matching the record; a play pressed by keys with misses, early
  and late presses, holds let go and stray presses, the same, with keys
  pressed throughout the replay; a failed play replays to its fail; a
  play read back from the export's form replays the same (a web record on
  desktop); Escape twice quits a replay; refusals (not kept, newer format,
  newer judge, garbled log, chart changed, song gone, found in another
  song, skin missing); the screens' buttons and keys at three sizes with
  nothing overlapping; desktop's whole flow offscreen. In headless Chrome
  (WebGPU) and Firefox (WebGL2): a play (autoplay 35 ms late), its replay
  from the results to the end (`check.matches`, grade, score, tiers,
  stats, hits and life equal, the history unchanged), the plays list on
  song select, the kept play's results (equal) and its replay, the
  device lost mid-replay (the prompt again, replayed from the start), a
  quit mid-replay, a refused id. The ghost: natively the same play
  beside itself is level at every frame (60 Hz, 1,000+ frames), a play
  40 ms late is behind all along and ends at the difference of the final
  scores, in the session and in desktop's play; in Chrome and Firefox an
  autoplay kept as the player's own, then a play 40 ms late beside it
  with the setting on: −4.74 points after 30 s, −20.00 at the end, the
  difference of the two results' percentages, drawn under the score.

Review fixes (2026-10-16):

- **A replay changes no setting:** the web no longer registers the
  devices' profiles for a replay (it made them on a fresh device, which
  hid the first-run notice and the next play's new-device step, and
  touched `last_seen`); checked in Chrome and Firefox: the stored
  settings byte for byte the same after a whole replay on a device with
  no profile.
- Desktop offers "Watch replay" only once the play is kept.
- The kept summary is compared as values read from its text (`0` and
  `0.0` alike, as JavaScript writes them).
- An older judge's play says so on its results; a settings snapshot that
  does not read falls back to how the record says it was drawn; a
  replay whose folder waited for access looks for its skin once the
  song is read.
- One `go(-n)` fires one `popstate`: one pop ignored, not `n`
  (`ddi_ui::back_stack`, tested with two layers closed in one call).
- The plays list's armed Remove disarms when focus moves away.
- A synthetic chart with rolls (kept and dropped), holds, mines, a stop
  and a tempo change replays exactly, autoplayed and pressed by hand.

## Decided here (the maintainer may change any)

- Inputs are logged in judged song time, not host time, with the offsets
  and clock figures beside them. A replay does not depend on the device
  it was played on, and can be judged under another offset.
- Judging becomes independent of the display's frame rate.
- A chart's identity is its content, so bests do not survive a change to
  its notes.
- Autoplay is kept but never counts; quits and calibration are not kept.
