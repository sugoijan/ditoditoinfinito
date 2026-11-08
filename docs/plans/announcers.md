# Announcers

Status: done 2026-11-03 (PLAN.md decision 81); planned the same day, the
maintainer's choices taken then (step 6's lines all wanted, `oni` on the
battery gauges, a first clear in place of a new record); our own packs,
`Mr. Clank` in English and `ガキンさん` in Japanese, bundled 2026-11-06,
the announcer still Off by default (the last section). Import the announcer packs made for StepMania
and its forks and play them where those engines do (steps 1–5), then a
few lines of our own in the same layout (step 6). The research, with
sources, is `docs/research/announcers.md`.

## What the research settled

- **One format.** StepMania 3.9, 5.0, 5.1, ITGmania, OutFox and Etterna
  read the same layout: `Announcers/<Name>/<line folder>/`, the sound
  files directly in the line folder (mp3, wav, ogg, oga), names matched
  without regard to case, one file picked per play of a line (5.1:
  shuffled, no repeat until each has played; 5.0 and Etterna: at random).
  A single file in place of a folder is documented but never read.
  Old names reach new ones through 5.1's alias table (13 screen rows,
  `gameplay combo N` for `gameplay N combo`).
- **Etterna is not an extension.** It is 5.1's system with fewer lines
  (no stage, course, battle or `evaluation final` lines) and two new
  names, `evaluation aaaaa` (Wife ≥ 99.9935%) and `evaluation f` (a
  failed play; StepMania says `e`). No archived pack uses either. The
  set to recognize is 5.1's plus those two.
- **When 5.1 plays them.** All from the engine, none from the themes:
  in play, a comment every 10 s (`hot` at full life, `danger` below 20%,
  `good` otherwise, `oni` in courses), the combo crossing each hundred
  to 1000, `combo overflow` every hundred past it, `combo stopped` when
  a combo of more than 50 breaks, behind one shared timer that drops a
  line (never queues it) within 2 s (combo) or 10 s (comments) of the
  last; `ready` and `here we go normal|extra|final` before the first
  note, `cleared` or `failed` at the end, ungated. On the results,
  exactly one line, by precedence: `new record`, a full combo (W4, then
  W1–W3), then the grade (`evaluation aaaa…d`, `e` failed), and
  `evaluation cheer` 2.5 s later for AA or better. Menus: `<Screen>
  intro`, `title menu game name`, `select music intro`, `select music
  comment new|hard|repeat|general` on choosing a song, an idle comment
  every 20 s on song select, `hurry up`. No volume of its own, no
  ducking, lines overlap.
- **Packs in the wild** (33 archived, three on the maintainer's
  machine): four root shapes inside zips (`Announcers/<Name>/` in
  `.smzip`, `<Name>/`, the line folders at the root, an odd top folder),
  so the root is found by its line names; mostly 2–7 MB, a median of
  two files a line, up to 150; mp3, ogg and wav mixed; old names
  (`select style …`, `select difficulty …`) common; 3.9's lines that 5.1
  never plays (`stage 1…`, `stage event`, `game over`, `caution`) in most;
  nested copies, readmes and peak files beside the sounds; one pack no
  announcer at all (numbered WAVs). Nearly all are rips of commercial
  games: **none is bundled, ever**, and no freely licensed one exists.

## Decided here (the maintainer may change any)

1. **Imported, never bundled** (amended 2026-11-06: ours come with the
   game, the last section; no other pack is ever bundled). Announcers
   come only from the player's imports and folders. Tests use packs generated in code (tones and the
   event sounds' synthesis, `crates/session/src/sounds.rs`), and the
   local packs only through an ignored test (step 2). An original
   reference pack of our own (recorded, or made with a voice service
   whose terms allow a free licence) can follow once the import is
   proven on existing packs; not before.
2. **Off by default.** A fresh profile has no announcer; Options →
   Sound → Announcer lists Off, ours (since 2026-11-06, the maintainer
   keeping Off the default) and the imported ones. One choice for
   every layout (StepMania keeps one per game type; we have no game
   types to the player).
3. **Its own volume**, Announcer volume beside Music volume, as a share
   of it (100% by default, so muting the music mutes the voice).
4. **Presentation only.** Lines never touch judging, the clock, a play's
   log or its replay. A line that is not decoded when it is due is left
   out; Start never waits for the announcer.
5. **Where it speaks:** plays (autoplay and playtests too), their
   results and, from step 5, song select. Replays too (Etterna does; a
   replay is the play as it was). Never in calibration (nothing may cue
   the player), the field preview, the controls diagram or the editor.
6. **StepMania's timings,** as 5.1 has them: one shared gate in play,
   lines dropped rather than queued, lines may overlap, no ducking. A
   restart or a quit stops what is speaking (`stop_all`, as now).
7. **A folder with no sound in it is no line** (5.1 counts any file, so
   a readme silences a line and skips its aliases; that helps nobody).
8. **Light.** A play decodes only the lines it can use, and at most four
   variants of each, the next ones from the line's shuffle (5.1's
   order, kept for the session), so a 150-variant line costs what a
   four-variant one does and a session still hears them all.

## Step 1. The catalog and the scan (`ddi-library`)

`crates/library/src/announcer.rs`, pure and natively tested:

- `Line`: every name 5.1 and Etterna play, plus 3.9's still found in
  packs, each with its folder name and its use (`Play`, `Results`,
  `Menu`, `Unused`: known, kept, not played yet); unknown folders kept by
  name (a later line, or ours, may use them).
- The alias table, complete (5.1's rows; Etterna's are a subset), and
  the fallbacks we add, each a list tried in order:
  `evaluation aaaaa` → `aaaa`; `evaluation e` ⇄ `evaluation f`;
  `gameplay N combo` (N > 1000) → `gameplay combo overflow` (the Mega
  pack has 1100…2000).
- `find_announcers(paths)`: the announcers in an import's file list. A
  folder is an announcer when at least three of its child folders are
  known lines (by name or alias) holding audio; its name is the
  folder's, or the archive's or picked folder's when the line folders
  sit at the root. A folder inside a line folder is never an announcer
  of its own (the Mega pack's nested copy is reported, not imported).
  Files other than audio are ignored and counted.
- The report per announcer: lines found and played, found and not
  played (3.9's), unknown folders, ignored files, empty folders. An
  import with none says so ("no announcer lines found").
- Caps, as the skins': 2,000 files and 128 MB an announcer, 50 an
  import.
- `pack.rs`'s `INSTALL_FOLDERS` already keeps `Announcers/` out of
  songs; an announcer's line folders at an import's root must not read
  as song folders without simfiles either.

Check: generated layouts for each root shape, old names, mixed case,
nested copies, empty folders, non-audio files, the caps.

`DDI_ANNOUNCERS=<dir>[:<dir>…] cargo test -p ddi-library --test
local_announcers -- --ignored --nocapture` finds the announcers in every
folder and zip under local folders and prints each report.

**Done 2026-11-03.** `ddi_library::announcer`: `Line` (every line we
speak, with its folder, where it is spoken, who named it and its
fallback chain), `classify` (a folder's line, an unused known name under
StepMania's alias, or unknown), `find_announcers` and `announcer_dirs`;
`pack::folders_without_simfile` leaves announcers' folders out. Two
choices made on the way: a line spoken takes the sounds of every folder
that names it (two spellings, or an alias and its name) rather than
5.1's first; and the caps keep the lines spoken first, in their folders'
order, so a pack of thousands of variants of one line could crowd out a
later line (none comes near: the largest has 1,270 sounds). On the
maintainer's three packs: *DDR 1st Mix* 19 lines and 4 unused names;
the Mega pack 33 lines, 68 unused and 11 unknown names (`SSStyle …`,
`gameplayNonStop ready`), its nested copy reported and its 128 other
files (the copy, peak files, a readme) passed over; the numbered WAVs no
announcer.

## Step 2. Import and storage on both shells

- **Web:** the import dialog finds announcers in any import (folder,
  zip, `.smzip`) as it finds skins, and stores them in IndexedDB: an
  `announcers` store (database version 4: name, lines, files, bytes) and
  the files under `announcer/<name>/…` in `files`. The import report
  gets an Announcers section from step 1's report. Re-reading a kept
  folder imports them again; removing one is Options' (step 3).
- **Desktop:** read in place, as its skins are
  (`library/noteskins.rs`): the `Announcers/` folder of each StepMania
  root among the song folders Options lists, and a listed folder that is
  itself an announcer or a folder of them; zips found there read in
  place too. The first folder holding a name wins, as for skins.

**Done 2026-11-03.** Both shells keep `ddi_library::announcer::Kept`
(the lines spoken, by folder, each with its sounds relative to the
announcer's folder; the key is the name in lower case, as skins' are).
The web stores it in `announcers` (database version 4) with the sounds
in `files` under `announcer/<key>/` (`crates/web/src/announcers.rs`);
only the lines spoken are stored, the names no line uses stay out (a
later line wanting them asks for a new import). The import report has
an Announcers section (`Announcer::said`: lines by where they are
spoken, sounds, size), the sounds left out by the caps among its
warnings and the rest (files passed over, empty folders, nested copies,
names no line uses) among its details; an announcer of 3.9's lines only
is not imported, and says so. Desktop finds them in place
(`crates/desktop/src/library/announcers.rs`): a StepMania root's
`Announcers/` and the zips in it, a listed folder that is one or is
named `Announcers`, one among a listed folder's packs, a listed zip;
`--seconds` prints them. Both shells now read `.smzip` as a zip
(`ddi_library::zip::archive_stem`), for songs as well. Removing an
announcer and listing them in Options come with step 3. On the
maintainer's packs, desktop finds *DDR 1st Mix* (19 lines) and the Mega
pack (33 lines) inside their zips; the web imports them in headless
Chrome (the Mega pack keeps 917 of its 1,270 sounds, 44.8 MB: its lines'),
says the numbered WAVs are music without a simfile as before, and
upgrades a version 3 library in place.

## Step 3. The setting, Options and the voice's own bus

- Settings: `announcer` (the imported key, empty for Off) and
  `announcer_volume`; a chosen announcer that is gone plays as Off.
- Options → Sound: Announcer (Off, each imported one), Announcer
  volume, and removing the chosen one. Choosing one, or moving its
  volume, speaks a line of it (`select music intro`, else any), as the
  music sample does (from the gesture on the web).
- **The voice outlives the fail's fade.** The fail's fade and a work's
  fades ramp the master gain (`AudioBackend::master_ramp`), which would
  take `gameplay failed` with the music. The backend gets a voice bus
  the ramps leave alone (both shells' mixers, the web's gain nodes),
  which the announcer plays through; the event sounds stay where they
  are.

**Done 2026-11-03.** `Settings::announcer` (the key, `None` for Off)
and `announcer_volume` (a gain, a share of the music's, 1 by default;
`voice_volume` for a play). Options → Sound lists Off and the
announcers by name (the web's imported ones from storage, desktop's
from the song folders on a thread), then Announcer volume on the music
volume's scale and, on the web, Remove (a confirmation, then the record
and its sounds deleted). Choosing one or moving its volume speaks a
line (`Kept::sample`: one of `select music intro`'s, else of its first
line) through `OptionsPlatform::announce`, from the press: the web makes
its context in the press and reads and decodes after
(`crate::announcers::speak_sample`), desktop decodes on a thread and
plays on the preview player (`Player::announce`). A chosen announcer
that is gone is let go once the web has listed them all; desktop keeps
it (a folder away for now), as it keeps skins, and shows it by its key
until it is listed. Removing the imported songs, or forgetting the
folders, lets it go. `AudioBackend::play_voice` plays on the voice's
bus: on the web a gain node beside the master, into the analyser; on
desktop voices flagged `bus` mixed after the master under a gain of
their own. Both buses follow the master's volume (a preview's, desktop's
`--volume`), never its ramps, and `stop_all` stops them. Checked: the
mixer's test (the master faded to 0, the voice heard at its gain), the
player decoding and speaking a generated WAV, the model's rows, and in
headless Chrome under the user-activation autoplay policy: the
announcer imported, chosen with a key (its 0.3 s line started on a
running context), its volume moved, removed (no record or sound left).

## Step 4. In play and on the results (`ddi-session`)

`crates/session/src/announcer.rs`: the lines' cues as a pure state
machine over what the play already reports each frame (the combo, the
life and the gauge's danger, judge events, the fail, the end), tested on
the fake clock; the shells only hand it decoded buffers and the audio.

- **Loading:** with the play's other loads, the lines the play can use
  and up to four variants each (decision 8), decoded on the web by the
  browser (the wasm decoder where it refuses, as for music) and on
  desktop on the loader thread.
- **Before the first note:** `gameplay ready` as the play begins, then
  `gameplay here we go normal` timed to end about half a second before
  the first note; when the lead-in has no room for both, `here we go`
  alone, and when it has none, neither (5.1: with no Go, only Ready).
  `extra` and `final` are never chosen (no stages; Etterna does the
  same).
- **The comments,** every 10 s (the timer counted from the play's
  start, as 5.1 counts it from the music's; nothing before the first
  note): `hot` at full life,
  `danger` while the gauge is in danger (never with failing off),
  `good` otherwise; on the battery gauges (LIFE4, RISKY, the lives of
  Dancing☆Onigiri's gauges where they have them) `oni`, as 5.1 does in
  courses (the maintainer's choice), falling back to the life's line
  where a pack lacks it.
- **Combos:** each hundred to 1000, then `gameplay N combo` where the
  pack has it, else `combo overflow`; `combo stopped` when a combo above
  50 breaks; on the play's own combo (per row or per note, by ruleset).
- **The end:** `gameplay cleared` when the last note is judged,
  `gameplay failed` at an immediate fail (or at the end, failing at the
  end).
- **The results:** one line by 5.1's precedence, with step 6's lines in
  it: `evaluation quit` on a quit's results (nothing else); then
  `evaluation first clear` on the chart's first cleared play under the
  ruleset (the maintainer's choice: a first play is a first clear, not a
  new record); then `evaluation new record`, the history's new best for
  the chart and ruleset after an earlier play; then the full combo, then
  the grade. A first play's best is no record: its `new record` speaks
  only where the pack has nothing else for it (no `first clear`, no full
  combo line, no grade line). A full
  combo maps `MarvelousFC` → W1, `PerfectFC` → W2, `GreatFC` → W3 and
  `FC` → W4 where the ruleset keeps the combo on the fourth judgment;
  the grade is the ruleset's tier rescaled onto 5.1's seven
  (`GradeToOldGrade`: the SM5 preset's AAAA…D directly, ITG's stars and
  letters by position, DDR's by letter, Dancing☆Onigiri's and FFR's by
  position), failed `e` (or `f`); `aaaaa` never (we have no Wife
  grade) unless a ruleset earns one later. `evaluation cheer` 2.5 s on
  for AA or better.
  The shells play them as their results open, on the sound the play
  ran on (whether each shell keeps it through the results is checked
  first in this step).

Checks: the cues on the fake clock (gates, drops, precedence, fallback
chains, the lead-in cases, restart and quit silencing), `play_flow` with
a generated pack (desktop, offscreen), headless Chrome on the web
(`__DDI_DEBUG.announcer`: lines spoken, dropped and not decoded),
Firefox by the maintainer.

**Done 2026-11-03.** `ddi_session::announcer`: the `Announcer` the
session ticks each frame with what the play knows (`Moment`: the
combo, the gauge's life, danger with failing on, lives, the fail, the
end) and asks for the results' line as a play ends (`Ending`); the
shells decode the sounds (`wanted`, `Bags`) into a `SharedVoice` they
hand the session (`PlaySession::set_voice`), the web from storage on an
offline context behind the prompt (`crate::announcers::load_voice`),
desktop on a thread (`play/voice.rs`); the history's best before the
play (`Prior`, from `best_of_play` as the play begins; none for
autoplay, replays and playtests) tells first clears and records. The
engine's gauges say their lives (`GaugeRules::lives`, the battery's).
Choices made on the way:

- **How many sounds a play decodes:** four of each comment and of the
  overflow, two of `combo stopped`, one of every line said once a play;
  combos the chart cannot reach and the lines not spoken yet (step 6's,
  song select's) are left out.
- **Ready and here we go** are placed at the first frame from the
  sounds decoded then; a here we go decoded later still goes where it
  fits after ready. A line not decoded when due is left out.
- **What is decoded first:** the opening (ready or restart, then here
  we go) and song select's intro come first in `wanted` and `menu`; the
  web reads and decodes four sounds at once (`load_voice`, one loader
  for a play and song select), desktop one after another on its thread.
  Desktop's song folders are looked through for an announcer once per
  library scan (`library::announcers::found`), so one added appears
  once the library scans again, as songs do.
- **The grade:** a table of StepMania's letters (SM5's, DDR's with
  their `+` and `-`) by letter; any other (ITG's seventeen,
  Dancing☆Onigiri's eleven) by place, rounded as 5.1's
  `GradeToOldGrade` (`lround(i × 6 / (n − 1))`); FFR's, which has none,
  no grade line.
- **A missing full combo line gives way to the grade** (5.1 says
  nothing then), as decision 7 lets no missing folder silence another
  line. A missing comment has no stand-in (`hot` absent: nothing), as in
  5.1, but `oni` falls back to the life's.
- **The voice outlives a fail's hold:** `AudioBackend::stop_music`
  stops all but the voice's bus, so `gameplay failed` goes on into the
  results; a quit, a restart and the session's end stop it.
- **Each shell keeps the play's sound through its results:** the web's
  session stays until Retry or leaving clears it; desktop dropped the
  play as the results came up and now keeps its sound
  (`PlaySession::into_sound`) until then.

Checked: the announcer's tests (bags, what a play decodes, the lead-in
cases, comments by life and battery, the gate and its drops, combos and
the overflow, the end, the results' precedence and cheer, the grade
mappings, missing and absent lines, off in calibration); a whole play
on the session's fake clock (Beatucada's Hard autoplayed: ready, here
we go, the comments, 100 combo, cleared, the full combo cheered, every
line on the voice's bus at its share; a play left unplayed: failed,
then `evaluation e`, the music stopped and not the voice; calibration
silent); desktop's `announcer_flow` (a generated pack in a song folder,
offscreen through the host: ready, here we go, the comments, cleared,
the full combo and cheer, nothing missing); headless Chrome on the web
(a generated pack imported, an autoplayed play: the same lines and the
cheer 2.5 s after the full combo; a quit: `evaluation quit` alone; a
play left unplayed: `gameplay failed` at the fail and `evaluation e`
after the hold). Firefox and Safari by the maintainer.

## Step 5. Song select

- `select music intro` when song select opens (the web's song select
  has its context from the press that opened it).
- On choosing a song: `select music comment repeat` (played this
  session), `new` (no plays of the chart in the history), `hard` (meter
  10 or more, 5.1's `HardCommentMeter`), `general` otherwise.
- Not taken: the idle comment (20 s of silence on song select would
  talk over the preview), `title menu game name` (Start is silent),
  `hurry up` (no menu timer), 3.9's `stage event` and `game over`
  (kept as Unused until we choose otherwise).
- The UI core asks for a line (`Request::Announce`), the shells play it;
  screens stay theme-agnostic.

**Done 2026-11-03.** Song select asks for `select music intro` at each
showing (5.1's `BeginScreen`: on arriving, and on coming back from a
play or Options) and, on a chart chosen, its comment
(`UiRequest::Announce`; `ddi_ui::screens::select::comment`). The shells
decode song select's lines the first time one is asked for
(`ddi_session::announcer::menu`: one sound of each, the next of its
bag) and speak them at the announcer's share of the music volume; a
line asked for while its sound decodes waits for it up to 1.5 s, then
its moment has passed (`ddi_session::announcer::MenuVoice`, both
shells'; a line asked for meanwhile takes its place). The web speaks on song select's context where it
runs (a page loaded with no input yet stays quiet, as its preview does),
and the comment on the one the play takes from it (the intro still
speaking stops at that hand-over); desktop on the preview player. A
preview stopping no longer stops a line (`stop_music`). Choices made on
the way:

- **`repeat`** is a chart whose best was last played in this run
  (`ddi_platform::storage::started_ms`, noted as the shells install
  their clock): a play the history counts (not autoplayed, not quit).
  5.1 counts a song played this session.
- **`new`** is a chart with no best under the rules in effect (5.1: a
  song the profile never played). `hard` is 5.1's meter of 10.

Checked: the screen's requests and the comment's order (UI tests); the
menu's sounds and the voice's turns (session tests); desktop's preview
player speaking the intro asked before it was decoded, a comment, and
nothing for a line the pack lacks (`announcer_flow`); headless Chrome
under the user-activation autoplay rule, by keys from the start menu:
the intro, `new` on Beatucada Hard, its play left to fail, the intro
again on coming back, `repeat` on the same chart
(`__DDI_DEBUG.menuAnnouncer`). Firefox and Safari by the maintainer.

## Step 6. Our own lines (all wanted by the maintainer)

Folder names in StepMania's style, so a pack with them still works in
StepMania (which ignores folders it does not know), each falling back
to a 5.1 line or to silence, documented for pack authors in
`docs/announcer-lines.md`:

- `gameplay N combo` past 1000 (already in the Mega pack) → overflow.
- `gameplay restart` on a quick restart (decision 79), in place of
  `ready` → `ready`.
- `gameplay full combo w1|w2|w3` and `gameplay full combo` as the last
  note keeps a full combo (the moment DDR says it): the kind's line →
  `gameplay full combo` → silence. The results still speak theirs.
- `gameplay last life` and `gameplay life lost` on the battery gauges →
  silence.
- `evaluation quit` on a quit's results → silence.
- `evaluation first clear` on the first cleared play of a chart under a
  ruleset, before `new record` → the rest of the results' precedence
  (step 4), `new record` last.
- Lamps past the full combos (single-digit perfects, LIFE4's) only if a
  pack author asks.

**Done 2026-11-03.** The play speaks every line of ours
(`ddi_session::announcer::speaks` now takes every play and results
line, so a play decodes them too): `gameplay restart` opens a quick
restart's play (`SessionConfig::restart`, set by both shells as they
begin it again; `ready` where the pack lacks it); the full combo as the
last note is judged (`Moment::full_combo`, worked out once as the end
comes); a battery gauge's lives (`Moment::lives`, the gauge's
`lives()`). `docs/announcer-lines.md` lists every line the game reads
and speaks, when, what it falls back to and whose it is, for pack
authors. Choices made on the way:

- **The full combo, then cleared:** `gameplay cleared` follows the full
  combo's line once it has been said, rather than over it.
- **Lives:** `gameplay last life` speaks whatever the gate (once a play,
  and the one that matters); `gameplay life lost` waits behind it, as
  the combo lines do; a life lost takes the frame from `combo stopped`;
  the life that fails the play is `gameplay failed`'s.
- **The gate counts from the play's start,** as 5.1 counts it from the
  music's (its dancing state), lines held back until the first note:
  counted from the first note, it dropped a life lost early in a chart.
  The first comment comes 10 s after the start, or as the first note
  comes after a longer lead-in.

Checked: the announcer's tests (a restart's line or ready; the full
combo's kinds and the plain one, then cleared after it; lives lost, the
last, the fail; a long lead-in's first comment); whole plays on the
session's fake clock (Beatucada's Hard autoplayed: `gameplay full
combo w1` then `cleared`; on StepMania's battery left unplayed: a life
lost, the last life, failed); desktop's `announcer_flow`, restarted two
seconds in (R twice): `gameplay restart`, then `gameplay full combo
w1, gameplay cleared`; headless Chrome the same (the web's restart) with
the quit and the fail as before.

## Step 7. Documentation

`docs/PLAN.md` (a decision; phase 8's row), `AGENTS.md` (the library,
session and both shells' lines), the verify skill (the import, the
debug state, a generated pack), `docs/plans/README.md`.

**Done 2026-11-03.** PLAN.md's decision 81, phase 8's row and note and
the status; AGENTS.md's lines for the library, the session and both
shells (written with each step; the web's bus added); the verify skill's
Options rows and state, the import's report and stores, a generated
pack on both shells, the debug state; the plans index's line.

## Packs of our own (bundled 2026-11-06)

Decision 1's original pack, explored from 2026-11-03 with ElevenLabs
(its terms as of 2026-10-09), to be followed by real recordings some day:

- **Auditioned on the free plan,** whose audio is non-commercial and
  must credit ElevenLabs: heard, never committed, bundled or shared
  (`tools/announcer-design/`, its output in
  `target/scratch/announcer-audition/`).
- **Made, once a voice is chosen, during a paid month.** Every paid
  plan carries the commercial licence (Starter is enough), and what is
  made while subscribed stays usable after; free-plan audio never
  becomes so. A non-commercial pack would leave the game the one part
  nobody may sell or fork freely.
- **Released as CC0:** the terms leave the output's rights with us,
  and generated audio may not be protected at all, which CC0 does not
  depend on.
- **Voices** from Voice Design or ElevenLabs' default voices, never the
  Voice Library's clones; no description names or imitates a real
  person, another game or its announcer.
- **Before bundling:** ElevenLabs' creative terms forbid publishing
  sound effects as isolated files or a sound library, and do not say
  whether speech falls under that; their written answer goes with the
  pack's `PROVENANCE.toml`. Bundling it amends decision 1 (and maybe
  decision 2, Off by default).
- **Or Kokoro, as the placeholder** (from 2026-11-03): Kokoro-82M
  (Apache-2.0), run here by `tools/announcer-design/kokoro.py`, says the
  same lines with no account or plan, and what it makes is ours to
  release as CC0; it makes the same bytes again from its pinned
  revision. Its stock voices share their names with other services' and
  its model card says some of its training audio was closed services'
  synthetic speech, so a pack to bundle would take a blend of them. Not
  chosen yet; it would amend decision 1 the same way.
- **Our pack, `Mr. Clank`** (2026-11-04; "Infinito" names the noteskin only): Kokoro's `bm_george` as it is
  (the maintainer's choice), the game called "DDI" (said as its
  letters). `tools/announcer-design/pack.toml` writes every line the
  game speaks (`docs/announcer-lines.md`) and `kokoro.py pack` makes
  them, 165 sounds over 50 lines (3.6 MB): our own lines too, the combos
  past 1000 at 1500 and 2000 with the overflow for the other hundreds,
  every line but `evaluation aaaaa` (nothing gives it) and `evaluation
  f` (it falls back to `e`). Variants where a line comes again, eight
  for the comments; no text names a grade's letter or a judgment, since
  grades map by place and judgments' names are the ruleset's. Each line
  said after a lead that is cut off (Kokoro is weakest on short
  utterances), one for each place since its tone carries into the line
  (song select's, play's and the results', `[lead]`). Checked: the scan reads all 50 lines with nothing unused
  or unknown, desktop's decoder decodes every sound, and the pack is
  made again byte for byte. Bundled since 2026-11-06 (below).
- **And in Japanese** (2026-11-04): `tools/announcer-design/pack-ja.toml`,
  the same lines in Japanese, `ガキンさん` in `jm_kumo` (the maintainer's
  choice over `jf_alpha`, Kokoro's best Japanese voice, C+), the game
  ディトイン. `jm_kumo` and three more (`jf_gongitsune`, `jf_tebukuro`, `jf_nezumi`, and
  `jm_kumo`, the one man) each learned from one TV Nishinippon
  announcer's CC BY 3.0 readings (the Koniwa corpus's `tnc`): a pack in
  them, or a blend with them, credits that and has that announcer's
  voice. Any voice speaks Japanese with `lang = "j"` (`bm_george` with
  an English accent). The phonemes are misaki's cutlet over UniDic's
  lite edition, Kokoro's own way; kana for Latin letters and for words it
  misreads, kanji for numbers.
- **Bundled** (2026-11-06, the maintainer's asking; the announcer still
  Off by default). `kokoro.py pack --bundle` puts each pack in the game
  as it was made, `assets/announcers/<name>/` (3.7 MB each), with a
  `PROVENANCE.toml` it writes: the pack file, the model and revision,
  the voice, the phonemizer's versions, CC0 held by the maintainer, and
  for a voice that learned from CC BY recordings what it learned from
  (`[[trained_on]]`: `ガキンさん`'s `jm_kumo`, a TV Nishinippon
  announcer reading 蜘蛛の糸 in Koniwa's `tnc`, CC BY 3.0 by Koniwa's
  README, the file named in Kokoro's VOICES.md), which the credits
  carry. `xtask gen-announcers` checks each (one announcer of our lines
  and their sounds alone beside its record) and writes the web's index,
  `announcers/index.json` (each pack's `Kept` and each sound's hash, its
  URL's `?v=`); desktop reads the assets' `announcers/` itself. Their
  keys are `bundled:` and the name (`bundled:mr. clank`,
  `ddi_library::announcer::bundled_key`), apart from an imported pack of
  the same name. Options lists them first, never with Remove, and
  removing the imported songs keeps one chosen. The web fetches a play's
  sounds from the site as it decodes them; a play on desktop takes ours
  without walking the song folders.
