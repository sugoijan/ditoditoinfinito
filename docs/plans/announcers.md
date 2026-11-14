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
(`ddi_session::announcer::menu`: up to eight sounds of each,
`MENU_VARIANTS`, the next of its bag, kept for the session and said in
turn; one of each before 2026-11-14, so that every entering said the
same intro, heard as the line said twice: the progress note's "Feedback
round D") and speak them at the announcer's share of the music volume; a
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
- **The battle's lines** (2026-11-12): both packs gained the sixteen
  `battle …` lines a multiplayer stage speaks
  (`docs/announcer-lines.md`, "In battle"; the fourteen of M4 and the
  attacks' two), two to four texts each, short, since they are said over
  a play (0.6 to 1.6 s as made): 46 sounds for `Mr. Clank`, 45 for
  `ガキンさん`, 211 and 210 in all over 66 lines. Each said after a lead
  as the rest; `battle` is a place of its own for leads (`[lead] battle`
  in `pack.toml`), and the Japanese pack writes counts of players in
  kana (よにん, ふたり), which the dictionary would read as numbers.
  The studio shows them as the battle lines' examples, before
  `examples.toml`'s texts.

### Louder packs (2026-11-12)

The maintainer: our announcers were too quiet, at their full volume
still under the music at its default, reduced one.

(The level and the limiter of this section, `announcer-level-v2`, were
replaced the same day by `announcer-level-v3`: "Smoother leveling",
below. What it says of the gain path, the one leveler and the same
bytes stands; its numbers are v2's record.)

- **Measured** (`ffmpeg -af ebur128`, integrated loudness and true
  peak): our 330 sounds sat at −19.6 LUFS (`Mr. Clank`, −20.7 to −18.8,
  true peaks to −2.1 dBFS) and −20.2 LUFS (`ガキンさん`, −23.9 to −18.5,
  to −1.2); the bundled songs' music at −9.9 to −10.4 LUFS (true peaks
  about 0). About 10 LU apart.
- **The gain path is right.** The voice plays at `Settings::voice_volume`,
  the music's volume times the announcer's share (1 by default, never
  more), applied once: on the web as the sound's own gain into the
  voice's bus, whose gain is the master's volume (1 in a play, the
  music's volume on song select, where the share alone is the sound's
  gain); on desktop the same through the mixer's bus. Neither bus is
  ramped by the fail's or a work's fades, nothing ducks it, and a mono
  sound goes to both channels at unity. The cause was the level the
  sounds were made at: a voiced RMS of −18 dBFS, lower still where a
  peak held the gain down.
- **One leveler, the game's** (the maintainer's asking):
  `crates/library/src/announcer/tidy.rs`, the announcer studio's tidy,
  levels our packs too. `kokoro.py` makes each line's speech as before
  and cuts its lead off (the model's durations, the quietest 10 ms: its
  own, nothing of the tidy's), then hands the sounds a batch at a time
  to `cargo run -q -p xtask -- announcer tidy` (float WAVs in and out, a
  line of JSON each: what was cut, the gain, what the limiter took), and
  encodes the MP3s as before. What moved out of the script: the trim
  (now the studio's: the floor, 30 ms kept before and 100 ms after where
  the script kept 20 and 80, so every sound is 30 ms longer: 10 at its
  start, 20 at its end; `here we go` still ends half a second before
  the first note by its sound's length, its word 20 ms sooner), the
  fades (the same 5 and 15 ms) and the level. `sample`, `speak`,
  `context` and `pack` all go through it. `crates/desktop/tests/studio.rs`
  holds the two together: a take as the studio's export put it in a pack
  and the same recording through the command are the same samples.
- **The level, `announcer-level-v2`** (`tidy::LEVELING`): the voiced RMS
  of what comes out at −9 dBFS (`VOICED_RMS_DB`; the frames within 30 dB
  of the loudest), about −10.5 LUFS, under a limiter with a ceiling of
  −1.5 dBFS (`LIMIT_DB`) for the samples and the wave between them (four
  points a sample, a windowed sinc), so that true peaks stay under
  −1 dBFS once encoded (`PEAK_DB`: a sound with nothing over that is
  left to the gain alone). The limiter: a soft knee from 1.5 dB under
  the ceiling (a parabola in amplitude, flat at the ceiling), the gain
  seen 0.5 ms ahead, let back up over 1.5 ms and averaged over the
  look-ahead (it moves smoothly and never over what its own sample
  needs: no click), one gain for every channel. The gain before it is
  found by halving, so that the limited sound's voiced RMS is the
  target. A tidied sound read back from its encoded file asks for
  nothing more: its peaks are under what the limiter leaves alone, and
  within 0.75 dB of the target it is tidy already
  (`Measure::already_tidy`; our MP3s read back ask for 0.4 to 0.5 dB),
  so a pack opened in the studio and exported again is not encoded
  twice. v1 was a voiced RMS of −18 dBFS with peaks under −1 and no
  limiter.
- **What it costs, measured** (PROVISIONAL: nobody has listened yet).
  Speech this short has more crest than the 13 dB reckoned with: the
  loudest sample is 13.7 dB over the voiced RMS for `Mr. Clank` (10.6
  to 16.2) and 17.3 dB for `ガキンさん` (13.8 to 20.4: `jm_kumo`'s
  voice is narrow pulses), and both come out at 7.5. The limiter's
  largest reduction is 8.1 dB at the median for `Mr. Clank` (4.0 to
  11.3) and 14.9 dB for `ガキンさん` (9.0 to 18.4), not the 4 to 6
  expected: that is what the music's loudness takes of these voices.
  What of each result is not its sound times one gain (the shaping,
  `shaped_db` in the tool's records) is −13.5 dB of it at the median
  for `Mr. Clank` and −7.3 dB for `ガキンさん`. At −12 dBFS the limiter
  takes about 3 dB of `Mr. Clank` and 8 of `ガキンさん`, at about
  −13.7 LUFS. `kokoro.py loudness --dest DIR` makes six lines of each
  pack at several levels with a table of each one's LUFS, true peak and
  limiting, to choose by ear; the level is one constant
  (`VOICED_RMS_DB`, and the script's `PROCESS` raised), and the packs
  are made again from the recipes.
- **After**: 330 sounds at −10.4 LUFS (`Mr. Clank`, −11.7 to −9.3, true
  peaks to −1.5 dBFS) and −10.7 LUFS (`ガキンさん`, −11.8 to −9.5, to
  −1.4). Read back from their MP3s, all 330 are tidy already (nothing
  to cut, 0.41 to 0.49 dB asked for, the limiter not at work again).
- **The same bytes again** on the same machine (checked: both packs
  made twice from nothing, 330 files, none differing): the model, the
  tidy and ffmpeg's encoder each give the same output from the same
  input.
  Between machines it was never promised (onnxruntime's sums differ by
  processor, ffmpeg's builds by version), and the tidy adds nothing to
  that but one thing: its leveling is exact arithmetic in a fixed order
  (sums, products, quotients and square roots of IEEE floats, no fused
  operations, its constants written out), the same on every system,
  while its cut and its voiced frames compare levels in decibels, a
  logarithm a system's library may round differently in its last bit,
  which matters only to a frame exactly on a threshold.
- **Other packs** are played as they are. None is on the maintainer's
  machine to measure (no announcer under the simfiles' folder, the
  StepMania forks' `Announcers` folders empty). Should packs vary
  widely, a correction at load would take: each pack's loudness measured
  once as its sounds are decoded (the median of its lines' voiced RMS,
  or LUFS), kept with the pack's record, and one gain a pack toward our
  level, bounded (say ±9 dB, never past what its peaks allow without a
  limiter), applied as the voice's gain and shown nowhere. Not built.
- **No boost.** The announcer's volume stays a share of the music's, 0
  to 1; at this level the packs want none.

### Smoother leveling (2026-11-12)

The maintainer, on v2's limiting (a median of 8.1 dB of `Mr. Clank`'s
peaks and 14.9 dB of `ガキンさん`'s, which a limiter that fast turns
into a reshaped wave): could a compression pass help limit it more
smoothly, before or after the loudness correction or with it, so that
the limiting is minimal?

- **What the level asks, measured** on the 421 sounds as the model
  makes them. A voice's loudest sample stands 13.7 dB (`Mr. Clank`) and
  17.3 dB (`ガキンさん`) over its voiced RMS, and that is two things. The
  wave's own crest inside its loudest 20 ms: 7.1 and 9.6 dB (`jm_kumo`'s
  wave is narrow pulses, every harmonic at its peak at once). And the
  loudest 20 ms over the voiced RMS, 6.4 and 7.5 dB: partly a line's
  loud syllables, which a compressor evens, and partly what speech is,
  sound and near silence by turns (the louder half of the voiced frames
  is 2.4 dB over the voiced RMS and stays about there however a line is
  compressed). A voiced RMS of −9 dBFS under a ceiling of −1.5 leaves
  7.5 dB for all of it, less than speech leveled perfectly flat has
  (about 7 + 2.4). **So at −9 several decibels come off the peaks
  whatever is done first**; a chain chooses how, and how many.
- **The chain, `announcer-level-v3`** (`tidy::LEVELING`, `tidy::CHAIN`),
  each stage exact arithmetic in a fixed order as v2 was (sums,
  products, quotients and square roots; the constants written out):
  1. **A phase rotator**: eight first-order all-pass filters at 200 Hz.
     It changes no frequency's level and spreads a pulse's harmonics in
     time, so the peaks of a pulsed voice fall: `ガキンさん`'s crest
     from 17.3 to 14.5 dB (2.8 dB; inside its loudest 20 ms from 9.6 to
     6.9), `Mr. Clank`'s from 13.7 to 13.3. Four stages do 1.9 dB of
     that and six 2.2; twelve and sixteen leave the limiter no less
     than eight; 150 Hz does as 200, 100 and 300 to 600 Hz less.
     **Kept**, since it clears the bar set for it (2 dB or more on the
     peakier voice) and costs nothing measured: the spectrum in octave
     bands moves 0.00 dB, the filters
     look back only (nothing before a sound, so the cut finds the same
     start), what they push past the cut is −158 dB of a sound, and
     lengths are unchanged. What it does to a wave: low harmonics come
     later than high ones, by 10 ms at 100 Hz, 6 ms at 200 Hz, 1.8 ms
     at 500 Hz and 0.5 ms at 1 kHz.
  2. **A compressor on the envelope**, feed-forward: the mean power of
     the 10 ms to come (its look-ahead: the gain is down by the time a
     syllable starts), followed over 3 ms up and 100 ms down, against a
     threshold 6 dB under the sound's own voiced RMS (a quiet take and
     a loud one are compressed alike: the same line 20 dB quieter gives
     the same samples), four to one far over it with a knee about 12 dB
     wide (`(1 + p)^(−3/8)` of the power over the threshold's: square
     roots alone), then made up to the voiced RMS the sound had. It
     takes a median of 6.7 and 6.3 dB over a sound's voiced frames (9.6
     and 10.2 at its largest in the middle sound, 11.1 and 11.9 in the
     worst). A line's loud frames over its middle one (95th percentile
     over the median) go from 5.9 to 4.0 dB (`Mr. Clank`) and from 8.7
     to 4.7 (`ガキンさん`); v2 at −9 left 3.9 and 3.5 by limiting alone.
     (EBU R 128's loudness range wants longer material than a line: of
     a pack's sounds one after another it is 1.7 and 3.5 LU as the
     model makes them, 0.8 and 0.9 through v2 and 0.8 and 0.8 through
     v3, the lines leveled alike either way.)
     The settings by measurement: what is left for the limiter moves
     by 0.6 dB at the most over thresholds from 0 to −12 dB, by 0.9
     over ratios from 2 to 8 (gentler leaves more, and past these
     harder leaves no less: a threshold at the voiced RMS takes 3 dB
     and leaves the limiter 0.3 to 0.6 dB more), by 0.7 over windows
     from 3 to 20 ms and by 0.9 over releases from 50 to 200 ms
     (shorter leaves less).
  3. **One gain**, found by halving as v2's, so that what comes out of
     the limiter has the voiced RMS wanted.
  4. **The limiter, gentler**, in two parts under the same ceiling
     (−1.5 dBFS for the samples and the wave between them, the same
     knee). Up to 3 dB it **rides the level**: the gain comes down over
     the 2 ms before a peak, is held 8 ms after it (a voice's pitch
     period: the next pulse finds it there) and comes back over 50 ms,
     so the wave keeps its shape and a syllable is a little quieter.
     Only what a peak needs **beyond** those 3 dB is taken of the peak
     alone, shaping it, as v2 took everything (0.5 ms ahead, back over
     1.5 ms). A limiter that only rides cannot reach a level whose
     peaks are everywhere over the ceiling (it turns the loud half down
     and the gain chases it: at −9 it ran to its 24 dB bound), so the
     second part stays for what is asked beyond the first.
- **What each stage buys**: the limiter's largest reduction in a sound,
  the median over each pack, with in brackets how much of it is the
  shaping of peaks.

  | | `Mr. Clank` at −12 | `ガキンさん` at −12 | `Mr. Clank` at −9 | `ガキンさん` at −9 |
  |---|---|---|---|---|
  | v2 (its fast limiter alone) | 3.6 (3.6) | 8.9 (8.9) | 8.1 (8.1) | 14.9 (14.9) |
  | v3's limiter alone | 4.7 (1.7) | 11.0 (8.0) | 10.4 (7.4) | 17.5 (14.5) |
  | with the rotator | 4.1 (1.1) | 6.3 (3.3) | 9.8 (6.8) | 12.5 (9.5) |
  | with the compressor | 2.8 (0) | 8.6 (5.6) | 8.6 (5.6) | 14.6 (11.6) |
  | with both: v3 | 2.3 (0) | 3.8 (0.8) | 8.0 (5.0) | 9.9 (6.9) |

  Riding costs level, which the gain makes up, so the riding limiter's
  whole reduction is larger than a fast one's would be; its shaping is
  what compares with v2's. At −11 v3 takes 3.9 (0.9) and 5.8 (2.8), at
  −10 5.9 (2.9) and 7.7 (4.7).
- **The level: −12 dBFS** (`VOICED_RMS_DB`), about −13.4 LUFS. The aim
  was a limiter with a median of 3 dB or less at −9; no chain reaches
  that (above), so the packs are bundled at the highest level, to the
  decibel, where the limiter's median is under about 4 dB for both
  voices with one chain: 2.3 dB for `Mr. Clank` and 3.8 dB for
  `ガキンさん`, of which the shaping of peaks is nothing in the middle
  sound of the first and 0.8 dB of the second. That is 3 dB under v2
  and the music. PROVISIONAL, to be chosen by ear: the audition set
  (`kokoro.py loudness`) has the same six lines of each as v2 at −9 and
  v3 at −12, −11, −10, −9 and −7 beside 20 s of music. The level is
  one constant, and the packs are made again from the recipes.
- **Distortion, measured three ways.**
  - *The last round's energy measure*, what of each result is not its
    sound times one gain: it counts any gain that moves, the
    compressor's too (here against the rotated sound, since rotating
    changes no level). v2 at −9: −13.6 and −7.3 dB at the median; v2
    at −12: −23.1 and −11.6; v3 at −12: −14.3 and −10.7. It cannot tell
    a syllable turned down from a wave reshaped.
  - *What is left once a slow gain is undone*: the ratio of the two's
    power smoothed over 30 ms taken out, the rest over the result. v2
    at −9: −17.5 dB (`Mr. Clank`) and −10.7 dB (`ガキンさん`) at the
    median; v2 at −12: −27.2 and −15.2; v3 at −9: −22.0 and −17.4;
    **v3 at −12: −32.2 and −29.9**. And the long-term spectrum, the slow
    gain undone, in octave bands: v2 at −9 adds 3.2 and 1.6 dB under
    88 Hz and moves the bands from 250 Hz to 1 kHz by up to 0.6 dB; v3
    at −12 moves no band by more than 0.08 dB (at −9: +1.1 dB under
    88 Hz, up to 0.5 elsewhere).
  - *A tone's harmonics*: 220 Hz with a syllable's envelope (three
    60 ms bursts over a floor 20 dB under them, a crest of 12.6 dB as
    speech's), the harmonics over the fundamental in its bursts. v2:
    9.5% at −12, 12.8% at −9, 13.5% at −7. v3: 0.06% at −12 (the
    tone's own figure), 4.8% at −9, 7.9% at −7. At 997 Hz neither
    distorts (a gain that takes a millisecond to come back does not
    move inside a cycle that short): what v2 reshaped was the pitch's
    range.
- **After** (all 421 sounds, their MP3s, `ffmpeg -af ebur128`):

  | | sounds | LUFS min / median / max | true peak at most | limiter, median / most | of which shaping |
  |---|---|---|---|---|---|
  | `Mr. Clank`, v2 at −9 | 211 | −11.7 / −10.4 / −9.3 | −1.5 dBFS | 8.1 / 11.4 dB | all |
  | `Mr. Clank`, v3 at −12 | 211 | −14.3 / −13.2 / −12.0 | −1.4 dBFS | 2.3 / 6.8 dB | 0 / 3.8 dB |
  | `ガキンさん`, v2 at −9 | 210 | −11.8 / −10.7 / −9.5 | −1.4 dBFS | 14.9 / 18.4 dB | all |
  | `ガキンさん`, v3 at −12 | 210 | −14.8 / −13.6 / −12.2 | −1.5 dBFS | 3.8 / 7.3 dB | 0.8 / 4.3 dB |

  The limiter shapes no peak in 151 of `Mr. Clank`'s sounds and 61 of
  `ガキンさん`'s, and has nothing to do in 19 and 5. Every sound is as
  long as it was (273.7 and 266.8 s in all): the cut is v2's, so
  `here we go` does not move. Read back from their MP3s, all 421 are
  tidy already (0.39 to 0.47 dB asked for, 10 ms to cut at the most,
  none compressed again, the limiter catching 0.6 dB of one peak the
  encoder raised). Made twice from nothing, the same bytes.
- **The studio's takes** get the same chain, since it is the same code:
  a recording is rotated, compressed against its own level, brought to
  −12 dBFS and limited as above; one at the level already (within
  0.75 dB, `Measure::already_tidy`: a tidied take read back) gets that
  little gain and the limiter for a stray peak, and is not rotated or
  compressed again. `Measure::gain` is now the gain after the
  compressor, which keeps the voiced level: about how much louder the
  take comes out, which is what the studio says of it. A draft kept
  from before measured its takes for v2's level: they play and export
  at about −9 dBFS through v3's chain (its limiter riding and shaping
  more) until they are added again, since the shells measure a take
  once.
- **Opus, the studio's export.** A lossy encoder raises peaks. The
  same 421 sounds at 48 kHz through the game's own encoder (CELT at
  64 kb/s) and read back: v2 at −9 came back with true peaks to
  +0.5 dBFS (−0.7 at the median), its samples cut at full scale; v3 at
  −12 with −0.1 dBFS at the most (−1.2 at the median; the loudest
  sample −0.3), none over full scale, so the ceiling stays one for
  every export. Through `libopus`: v2 to +1.8 dBFS at 32 kb/s and +1.0
  at 64, v3 to −0.4 and −0.2. The margin is what the lower level
  gives: v3 driven to −9 comes back at +0.7 to +1.0. Should the level
  go up again, the ceiling for lossy exports would have to come down
  by about 2 dB (a ceiling given to `tidy::apply` by the export, which
  is not built). Read back, every one of them is tidy already (0.10 dB
  asked for at the most, 10 ms to cut). The check is
  `cargo test -p ddi-desktop --test studio tidied_sounds -- --ignored
  --nocapture` with `DDI_TIDY_OPUS` (a folder of tidied 48 kHz WAVs)
  and `DDI_TIDY_OPUS_OUT` (where what is read back goes, for a
  true-peak meter).
- **The study** stays in the code: `cargo test --release -p ddi-library
  --lib tidy::tests::study -- --ignored --nocapture` with
  `DDI_TIDY_STUDY` (a folder of folders of WAVs: a pack's speech as it
  was made), `DDI_TIDY_CHAINS` (settings to try, `name:key=value,…;…`)
  and `DDI_TIDY_LEVELS` prints what the compressor and the limiter did
  over each folder.
- **Louder than this, cleanly, is the mixer's to give.** The voice
  plays at the music's volume times the announcer's share, 0 to 1, and
  the music's default volume is 0.33 (−9.7 dB): the bus has the room
  the samples lack. Letting the share go to 1.5 (+3.5 dB, the
  announcer's volume shown to 150%), the product kept at 1 or under,
  would put v3's sounds at the music's loudness wherever the music's
  volume is 0.67 or less, with none of the limiting −9 costs. Proposed,
  not built: it changes a setting's range and Options.
