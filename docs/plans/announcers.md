# Announcers

Status: done 2026-11-03 (PLAN.md decision 81); planned the same day, the
maintainer's choices taken then (step 6's lines all wanted, `oni` on the
battery gauges, a first clear in place of a new record); our own packs,
`Mr. Clank` in English and `ガキンさん` in Japanese, bundled 2026-11-06;
since 2026-11-17 one built-in announcer, on by default, speaking the
game's language, and announcers as pools of takes ("One built-in
announcer, on by default" and "Pools of takes", the last sections).
Import the announcer packs made for StepMania
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
2. **Off by default** until 2026-11-17, when the built-in announcer
   came on by default ("One built-in announcer, on by default").
   Options → Sound → Announcer lists Off, ours and the imported ones.
   One choice for every layout (StepMania keeps one per game type; we
   have no game types to the player).
3. **Its own volume**, Announcer volume beside Volume (named Music
   volume until 2026-11-19), as a share of it (100% by default, so
   muting the volume mutes the voice).
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
- **A step's two lines** (2026-11-15, feedback round J): both packs
  gained `battle step up` and `battle step down` (said as a stage's new
  chart's notes begin under "Charts: Adaptive"), three texts each, made
  by `kokoro.py pack --bundle` from the model already in the cache
  (`HF_HUB_OFFLINE=1`): only the twelve new sounds were made (211 and
  210 kept), every sound bundled before byte for byte the same, the
  `PROVENANCE.toml`s as the tool writes them. As made: 0.76 to 0.97 s
  for `Mr. Clank`, 0.93 to 1.5 s for `ガキンさん`; −12.4 to −14.2 LUFS,
  true peaks −1.8 to −2.5 dBFS (`announcer-level-v3`, as the rest).
  217 sounds for `Mr. Clank`, 216 for `ガキンさん`, over 68 lines.

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

### Effects audition (2026-11-15; integrated 2026-11-16 below)

The maintainer wants the built-in voices to have the character of a stadium
or race announcer and wants to hear alternatives before choosing. The shared
tidy already provides dynamics control; this study adds tone, saturation and
space. No production treatment has been selected, and no bundled audio or
runtime behavior changes in this step.

`tools/announcer-design/effects.py` uses SoundCraft 0.20.1's documented local
control channel in a separate, disposable instance. It does not need an
installed MCP integration. `effects.json` records the complete candidate
recipes: current voice; race broadcast (presence, light saturation, 75 ms
slap, small room); compact arena (warm presence, 40 ms pre-delay, 0.75 s room);
stadium PA (260–5200 Hz speaker color, more saturation, 145 ms repeats,
1.25 s room). These are initial design choices, not an emulation of any
particular venue or announcer. SoundCraft supplies the DSP; none is copied
into the game.

Six unchanged takes of each bundled voice are used: ready, 100 combo, hot,
match begin, final stage and evaluation AA. The sources are the current MP3s,
decoded once, with identical timing and two-second gaps. All treatments stay
mono. The listening page also offers each over the same stereo excerpt of
*Into My Dream*, with a fixed mix and no ducking; source credits accompany it.
The generated files, source hashes, plugin snapshots, commands, measurements
and cue times stay in `target/scratch/announcer-effects/`.

Verification on this first set:

- Eight mono reels, all at −18.6 LUFS after a static audition gain; their
  true peaks range from −6.3 to −2.0 dBTP. The common loudness target was
  lowered to preserve headroom without adding compression or limiting.
- Eight stereo music comparisons, peaks at most −6.8 dBTP. Voice sources
  are duplicated at unity into both channels, as in the game; this fixed
  preview balance is not a runtime mixing test.
- All reels preserve their source timeline length: 18.60 s in English,
  19.74 s in Japanese. A second render reproduces all eight solo WAVs
  byte for byte. The actual plugin parameters were checked in the session
  inspection, and all audio was checked for duration, loudness and peaks.
- Listening preference remains the maintainer's next step; numerical checks
  do not establish that one treatment sounds better.

After the choice, apply it reproducibly to the unencoded generation output,
retaining the game's one production leveler. Check every short line and
the opening: `ddi_session::announcer::Announcer::open` schedules `here we go` by
the decoded file's full duration. A reverb tail makes its spoken word start
earlier and can reduce the room for `ready`; the full-length stadium setting
may need a shorter variant for those calls. The audition deliberately does
not change that scheduler or repeatedly re-encode production MP3s.

#### Stadium refinements (2026-11-16)

First listening feedback: race and arena were too subtle. Stadium was liked,
particularly its microphone/loudspeaker color, but might be too strong.
`effects-stadium.json` therefore preserves the dry and previous stadium
references and makes three new candidates:

| Candidate | Microphone color | Delay | Room reverb |
| --- | --- | --- | --- |
| A, closer stadium | Original stadium EQ and saturation | 110 ms, 10% mix, 10% feedback | 0.65 s, 16% mix, 45 ms pre-delay |
| B, balanced stadium | Original stadium EQ and saturation | 130 ms, 14% mix, 15% feedback | 0.9 s, 22% mix, 55 ms pre-delay |
| C, fuller microphone | Passband widened from 260–5200 to 180–6500 Hz; mid boost 2 instead of 3 dB; saturation drive 3.5 instead of 5 dB, mix 28 instead of 35% | Same as B | Same as B |

A versus B tests the amount of space while keeping the preferred microphone
character. B versus C tests gentler microphone color with identical space.
The first recipes and renders are preserved. These are still listening
candidates, not a chosen production chain.

The new page and ten solo/ten music WAVs are in
`target/scratch/announcer-effects-stadium/`. The common target is −18.7 LUFS
(measured within 0.1 LU); voice peaks are at most −2.0 dBTP and music-mix
peaks at most −6.8 dBTP. Durations and source takes are unchanged. All actual
insert parameters match the recipes; the dry and previous stadium renders
before the audition gain reproduce the first study byte for byte.

#### Selected effect and source endings (2026-11-16)

The maintainer selected C, `stadium-soft`, and reported that the dry reference
also sounds like "Are you readys?" and "A hundred in a rowth!". The effects
auditions used existing bundled MP3s. Their hashes match the recorded sources
and Git HEAD. Each source's fresh decode matches its segment of the input
reel sample for sample. The dry SoundCraft render is that input times one
constant gain, with a residual about −149 dB relative to the voice; silence
stays exactly silent. The delivered dry WAV differs only by the recorded
audition gain and 16-bit rounding. This excludes the audition effect/render
path as the source of an extra ending. The text and stored phonemes have
neither added consonant.

At the maintainer's request both lines were regenerated, always after a lead.
The existing "Oh yeah, here we go!…" lead reproduces both bundled MP3s byte
for byte using the pinned Kokoro model, `bm_george`, speed 1, the same recorded
phonemizer/runtime versions and `announcer-level-v3`. Identical inputs do not
provide a different take. Two alternatives, "All right, everybody.…" and
"Here comes the next challenge.…", generate new readings. Each lead is removed
by the normal model-duration/quiet-pause cut (`kokoro.render`), not by guessing
where the phrase begins. All six takes use the unchanged shared tidy.

`tools/announcer-design/pack-endings.toml` preserves all six text/lead pairs
for the ordinary `kokoro.py pack` workflow, to a scratch destination without
`--bundle`. The first run's raw and tidied WAVs, MP3s, generation script and
measurements are under `target/scratch/announcer-endings/`; the original-source
audit is under `target/scratch/announcer-source-check/`. Three short listening
reels each say ready then the hundred-combo line, at −16 LUFS (within 0.2 LU),
with peaks at most −3.5 dBTP and no creative effects. The exact original-led
control matched both then-current assets byte for byte. The maintainer heard
the second alternative, "Here comes the next challenge.…", as much better,
and requested that lead for every line and the selected effect in production.

#### Approved SoundCraft renders (2026-11-16)

All 217 English sounds now use "Here comes the next challenge.…". The 216
Japanese sounds retain their Japanese leads; all 433 are regenerated from
unencoded TTS with `effect = "stadium-soft"`. Voice, model revision, speed,
spoken texts and sound paths are unchanged. The lead is removed through the
existing duration/quiet-pause cut, then the shared `announcer-level-v3` tidy
cuts silence, fades and levels the dry speech as before.

The original recipe, now `tools/announcer-design/stadium-soundcraft.json`,
pins `announcer-stadium-v1` and
SoundCraft 0.20.1, with the exact selected inserts: 180–6500 Hz passband,
2 dB presence at 1500 Hz; saturation drive 3.5, 28% mix; 130 ms delay,
15% feedback, 14% mix; room decay 0.9 s, 55 ms pre-delay, 22% mix.
The original `stadium.py` launched an isolated instance without audio hardware, in a
disposable profile, checking its version and actual insert parameters.
Rendering runs at 48 kHz and stays mono. The recipe and renderer SHA-256
are part of the cache key and provenance. SoundCraft supplies its own DSP;
no runtime effect code or new runtime dependency is included in the game.

Ready, restart and every here-we-go variant keep their dry duration, because
the opening scheduler places them by that full duration. For those files,
a second render bypasses only delay and reverb to recover the colored direct
voice; only the spatial return fades in the last 60 ms. Other lines retain
0.6 s after the dry speech. This bounds the tail without moving the opening
calls earlier or fading their direct speech to make room for the effect.

`tidy::finished`, exposed as `xtask announcer finish`, is
`announcer-finish-v1`: the existing gain solver and true-peak limiter after
creative processing, with neither another phase rotation/compressor nor
another silence trim. It restores −12 dBFS voiced RMS over the original
speech span; the tail cannot raise the gain by extending the measurement.
It fades the file endpoints and reserves encoder headroom. The MP3 is encoded
once, after all processing; generation rejects an encoded true peak above
−1 dBFS or a final speech level more than 0.1 dB from the target.

The default `kokoro.py pack --bundle` regenerates both packs; then run
`xtask gen-announcers` and `xtask gen-credits`. The original generator had
`--soundcraft PATH`; the FFmpeg replacement below removes that option.
`--xtask PATH` can use this checkout's prebuilt tool.
The output's `state.json` records the dry hash, full recipe, cut, body/tail
durations, final level/limiting and encoded measurements per file. Both
packs retain their CC0 and Japanese training-source notices.

Verification: the six dry lead-study takes remain byte-identical after the
shared gain-solver extraction, as do their MP3s. The two approved challenge
takes match the production dry hashes. The production inserts match the
selected audition exactly. A signal check confirms the opening fade preserves
the direct voice and duration; Rust tests cover finish timing, tail-independent
level, dynamics below the limiter, peak protection and silence.

The full run validates all 433 encoded files: same text/path coverage and
licences, finite audio, speech RMS within 0.002 dB of −12 before encoding,
and decoded durations within 0.87 ms of the intended span (opening calls
within 0.25 ms; MP3 rounding/padding). The highest encoded true peaks are
−1.6 dBFS in English and −2.1 dBFS in Japanese; median integrated levels
are −12.9 and −13.2 LUFS. Twelve independently rendered probe clips match
the final MP3s byte for byte. A second full command keeps all 433 and writes
no bundled files; indexes and credits pass their generated-file checks.
The measurements and file hashes are in the run's `audit.json` under
`target/scratch/announcer-production/`.

The old audition helper now accepts `--source DIR` and refuses already
treated packs, so later comparisons cannot accidentally add another stadium
effect to these bundled files. Use dry exports for new comparisons.

#### FFmpeg generation (2026-11-16)

The maintainer asked to remove the SoundCraft generation dependency while
staying close to the approved sound. `stadium.py` now renders entirely with
FFmpeg (`announcer-stadium-ffmpeg-v1`), using `afir` with auto-normalization
disabled for two measured responses: `stadium-eq.wav` before the saturator,
`stadium-space.wav` after it. The nonlinear stage remains separate:
normalized tanh at drive 3.5 dB, a 7500 Hz one-pole tone filter, 28% wet mix,
−1.5 dB output. The shared tidy/finish and one final MP3 encoding stay as
above. Opening duration and wet-only fade policies are unchanged.

The responses were captured from SoundCraft 0.20.1's original settings after
one second of silence, using a half-scale impulse at 48 kHz. Division by
that amplitude and the dry mono-bounce gain removes the capture gain; that
gain is applied once after processing. The 0.2 s EQ and 2 s spatial files
omit no samples above −140 dBFS. `capture_stadium.py` can recapture them
with a separate disposable control session; both files were reproduced byte
for byte in an independent run. This optional authoring tool and the old
audition helper use SoundCraft; normal generation imports neither and
requires no installed app, control server or MCP. No SoundCraft DSP source
is copied into the project. The generated filter data is covered by the
project's MIT annotation in `REUSE.toml`.

This is an approximation: the reference room has internal delay modulation,
and its parameters smooth from defaults at the beginning of each bounce.
A fixed impulse response captures the selected tone, repetitions and room
decay without those time-varying details. On the two ending-study takes the
unmastered microphone-only waveform correlations were 0.99967 and 0.99985;
full-effect correlations were 0.9730 and 0.9842. These measurements describe
the difference, not a substitute for listening approval.

Fourteen clips (ten English, four Japanese) were generated through the full
FFmpeg pipeline, without launching SoundCraft. All dry hashes and intended
body/tail durations match the approved run; encoded true peaks are at most
−2.1 dBFS, final voiced RMS within 0.002 dB of −12, decoded duration error
under 1 ms. Per-clip integrated loudness differs by at most 0.3 LU. The
five-phrase English reels are both 14.99 s and −13.2 LUFS, at original
production levels: `target/scratch/announcer-ffmpeg/comparison-approved.wav`
and `comparison-ffmpeg.wav`, with cues and measurements in `comparison.json`.
Tests cover deterministic FFmpeg renders, bounded tails, opening-only spatial
fades and silence. The cache includes FFmpeg's version, the recipe, renderer
and response hashes; exported provenance records those versions and hashes.

At this comparison step, the 433 bundled SoundCraft MP3s were retained with
their original provenance. The maintainer then judged the FFmpeg version
identical by ear. That approved comparison remains under
`target/scratch/announcer-ffmpeg/`; production subsequently moved to shared
Rust as described below.


#### Shared Rust studio and generation processing (2026-11-16)

After approving the FFmpeg sound, the maintainer requested it in the studio,
using the same Rust processing as pack generation. The implementation is
`ddi_library::announcer::stadium` (`announcer-stadium-rust-v1`), with the
approved captured responses in `assets/announcer-filters/` and settings in
`tools/announcer-design/stadium.json`. The original FFmpeg graph remains
in `stadium_ffmpeg.py` for comparison only. The pack adapter now invokes
`xtask announcer stadium`, including the shared finishing pass; FFmpeg is
used only for encoding and metering. Recipe, Rust source and response hashes
participate in generation caches and provenance.

Convolution uses the library's radix-2 FFT with 2048-frame partitions;
response spectra are prepared once, and channel history is independent.
Saturation and the tone filter retain the approved normalized-tanh formula
and parameters. A centered windowed-sinc conversion brings other sample
rates to 48 kHz without timeline latency. The opening wet fade, other calls'
0.6 s tail and final body-only leveling remain as approved. On the two
ending-study takes, final Rust/FFmpeg correlations exceed 0.99999999999,
with waveform differences around −110 dB relative to the speech.

Studio Advanced → Stadium effect is optional and saved, off by default.
Both shells use `draft::Processing` for original recordings in previews,
scenes and exports, with bounded caches keyed by the original take,
measurement, assigned line and processing settings. Tidy remains separate;
Stadium includes its final leveling even when Tidy is off. Processed exports
encode anew, and duration warnings include the tail. The browser fetches
the responses only on first use, under their build-generated content revision,
so their sample data does not enter initial wasm. Desktop and xtask embed
those same files. See `announcer-studio.md` for behavior and checks.

The broader comparison covers ten English and four Japanese phrases through
both pipelines, including resampling and final leveling. Rust/FFmpeg waveform
differences range from −109.9 to −78.6 dB relative to the output, correlations
at least 0.999999993; the different resampler accounts for most of the larger
Japanese residual. Measurements are in
`target/scratch/announcer-rust/comparison.json`.

Both bundled packs were regenerated through Rust: 217 English and 216
Japanese sounds, all with the same dry-speech hashes as the approved run.
The final audit decodes all 433 and checks text/path coverage, licences and
training attribution, finite samples, exact intended body/tail policies,
source/response fingerprints and encoded peaks. Maximum encoded true peaks
are −1.8 dBFS (English) and −2.2 (Japanese); median integrated levels remain
−12.9 and −13.2 LUFS. Final speech RMS is within 0.002 dB of −12 before
encoding, and decoded durations within 0.87 ms of the planned span.
`target/scratch/announcer-rust-pack/audit.json` records the measurements and
all final MP3 hashes. Generated announcer indexes and credits pass checks.
A repeat generation kept all 433 sounds and wrote no bundled files; their
hashes still match the audit. The refreshed local web build includes these
Rust-generated packs.

## One built-in announcer, on by default (2026-11-17)

The maintainer: "The announcer should become one of our built-in
announcers … make the announcer language an argument similar to how it
is on the 'judgement words' but the default being 'Same as game
language'", and, for who is moved to it, "those with it Off or with one
of the built-in ones that is being replaced (without preserving the
language choice), but preserve custom announcer choices". The design
is `target/scratch/mp/notes/announcer-lua-proposal.md` with its
addendum, accepted with its picks.

- **One character, a voice a language.** `assets/announcers/Clank/`,
  key `bundled:clank` (`ddi_library::announcer::BUILT_IN`,
  `BUILT_IN_FOLDER`), its two voices moved there byte for byte (`git
  mv`): `en/` (was `Mr. Clank/`) and `ja/` (was `ガキンさん/`), each with
  its `PROVENANCE.toml`. Its `announcer.ini` declares its names and its
  languages (below). Shown as "Mr. Clank" where the UI is English and
  "ガキンさん" where it is Japanese: the pack's own names, data per
  language, not a translation (ガキン is the clank of metal). A third
  language is a folder and a tag in `Languages=`, with its pack file's
  `bundle` (`kokoro.py pack --bundle` writes `pack.toml` into `Clank/en`,
  `pack-ja.toml` into `Clank/ja`), no code.
- **On by default.** `Settings::announcer` is `Some("bundled:clank")` in
  new settings. Settings stored before carry no `announcer_moved` mark
  (`ANNOUNCER_MOVED`, a mark of its own, apart from the defaults reset
  of the same day): once, an announcer Off or one of the two old keys
  (`bundled:mr. clank`, `bundled:ガキンさん`) becomes the built-in one at
  its default language (the old pack's language is not kept: a player
  who had ガキンさん with an English UI hears English, one press of the
  Language row from Japanese); any other key (imported, made in the
  studio, a song folder's) stays. Off chosen after it stays Off.
- **Told once.** A player moved from Off sees one notice on song select
  ("An announcer now speaks in plays. Turn it off here, or later in
  Options → Sound.", `Notice::announcer_on`) with a link that turns it
  off at once (`ANNOUNCER_OFF`); it is taken when shown
  (`Settings::take_announcer_notice`). Fresh settings get none: song
  select's intro introduces the voice.
- **Gone falls back to the built-in, never to Off.** A chosen announcer
  no longer listed (`let_go_missing_announcer`), one removed in Options
  and the imported ones removed with the songs give the built-in one.
- **The Language row** (Options → Sound, under Announcer, where the
  chosen announcer offers two languages or more,
  `options_model::ANNOUNCER_LANGUAGE`): "Same as game language
  (English)", naming what it resolves to, the default and kept as
  nothing; each language named in itself (`ddi_l10n`'s name for a tag it
  knows, else the pack's `Labels=`, else the tag); "Mixed" where the pack
  offers it. Kept per announcer in `Settings::announcer_params` (the
  skins' `Values`, bounded as theirs are), a change speaking a line as
  choosing one does. The game's language resolves to a voice as: the
  same tag, its primary subtag (`ja-JP` → `ja`), English, the pack's
  first (`pool::pick_language`; the pseudo-languages get English).
- **Its sounds, for each choice.** An announcer is resolved before its
  sounds are chosen (`Kept::resolve`): what the session sees is the same
  `Kept` of lines it always saw, holding only the chosen language's
  takes, so `wanted`, `menu`, the shuffles, the timing and
  `battle-voice-v3` are unchanged. Its shuffles are kept by announcer and
  choice (`Kept::bag_key`, `bundled:clank#ja`), so the languages take
  turns apart; song select's voice is loaded anew when the choice or the
  game's language changes (`pool::voice_key`).
- **What the web fetches.** `announcers/index.json` lists the built-in
  once, its 433 sounds by their paths in it (`en/…`, `ja/…`) with their
  hashes and what each says, and its `announcer.ini` inline. Resolution
  runs over the index's names, so a play fetches only the chosen
  language's sounds, and only those `wanted` picks; Mixed fetches from
  every language it mixes. A play's music is read before its prompt
  shows, so the voice's fetches begin after it, and our sounds are
  fetched at the browser's low priority (`songs::fetch_bytes_low`,
  `priority: "low"`); Start never waits for them (as before: a line not
  decoded when due is left out). Measured from the packs as committed
  (MP3, 128 kb/s, about 31 KB a sound): a first song select 24 sounds,
  about 0.8 MB (its intro and comments, up to eight of each, decoded when
  first asked for), a play of a 300-note chart about 1.5 MB, of 1,500
  notes about 1.9 MB; the opening (ready, here we go) about 33 KB.
  Measured in headless Chrome and Firefox on a fresh profile
  (2026-11-17, Beatucada's 217-note chart): 47 sounds, 1.37 to 1.48 MB,
  in English or Japanese alone, the first asked for 0.05 to 0.4 s after
  the music's last byte; Mixed the same count from both.
  `__DDI_DEBUG.announcerLoads` lists the voices loaded, their choices
  and the sounds asked for.
- **The studio** keeps its examples: each set is the built-in's voice in
  that language (`Bundled::voice`, its paths and texts as a pack of its
  own), its packs still StepMania's plain layout.
- **`xtask gen-announcers`** reads a declared pack: every declared
  language a folder with its `PROVENANCE.toml`, named there as
  `announcer.ini` names it, every voice the same lines, no line folder
  outside them; `gen-credits` credits each voice as before.

## Pools of takes (2026-11-17)

An announcer is a pool of takes (`ddi_library::announcer::pool`): a take
is one sound file with the lines it may be said for, its language and
its tags. A pack's folders make the pool (`Pool::plain`): a line folder
is a line, and in a declared pack (a folder holding `announcer.ini`,
`pool::DECLARATION`) a language folder above line folders (`en/`,
`pt-BR/`, `pool::is_language`) is that language. A declared pack is one
announcer whatever its folders: its language folders are its voices,
never announcers of their own (without the file, `en/` and `ja/` would
each be one, as before). The player's choices filter the pool
(`Pool::resolve`): the takes of the languages chosen, and those of no
language in every one. What is left is each line's sounds, and the
engine does what it always did: the line's fallback chain where a line
has no take left, the shuffle, precedence and timing; one rule more,
that a take said for several lines is never said twice in a row through
two of them (`Voice::next`, by the file each sound came from,
`Voice::add_from`). A StepMania or Etterna pack (no `announcer.ini`)
reads exactly as before: `Kept::resolve` gives it back as it is.

`announcer.ini` is read as skins' `metrics.ini` is
(`ddi_noteskin::ini::Ini`, `params::Declared`), with a type of its own:

```ini
[Announcer]
Name=Mr. Clank
Name.ja=ガキンさん

[Parameters]
List=Language

[Parameter Language]
Type=Language
Languages=en,ja
Mixed=1
```

`Name.<tag>` names the pack in a UI language; `Languages=` lists the
languages offered in order (`Labels=` may name those the game does not
know), `Mixed=1` offers them mixed, each take as likely as any other.
The web keeps the file's text with an imported pack (`Kept::ini`),
desktop reads it in place, folders and zips.

## A pack's script (2026-11-17)

A pack may carry `announcer.lua` (`ddi_library::announcer::script`):
run once when its sounds are chosen (a play's voice, song select's,
Options' sample and rows), never during a play, it builds the pack's pool
and returns it. The engine keeps everything about when and whether a
line is said; the script only says which takes there are. A folder
holding `announcer.lua` is a declared pack, as one holding
`announcer.ini` is, and the sounds outside its line folders (its unknown
folders', `shared/awesome.ogg`) are kept for its script to name
(`Kept::others`; the web stores them with the pack).

```lua
local pack = Announcer.pack()
pack:take("shared/awesome.ogg", {
  events = { "gameplay 100 combo", "evaluation cheer" },  -- reuse
  lang = "en",
})
pack:take("en/gameplay 200 combo/damn.ogg", { tags = { "profanity" } })
pack:tag("profanity", { label = "Profanity", default = true })
pack:languages({ "en", "ja" }, { mixed = true })
return pack
```

- `Announcer.pack()`: the pack (one; the script returns it).
- `pack:take(file, { events, lang, tags, weight })`: a take of `file` (a
  sound of the pack, its path in it). `events`: the lines (folder names,
  any case and StepMania's aliases; none: its folder's line); `lang`: its
  language (none: its first folder's where that is a language's tag);
  `tags`: words of the pack's own; `weight`: its share when languages are
  mixed (1 by default, 0 never, at most 100; a weight above 1 makes it up
  to four sounds of its line).
- `pack:tag(name, { label, default })`: a tag the player may switch off,
  a row under the announcer ("Profanity: On / Off", the label the pack's
  words), On by default unless `default = false` (or `"off"`); a take
  with a tag switched off is not said, and a line left with no take
  falls back as a pack lacking it does (in a battle, the status line
  shows its moment).
- `pack:languages(list, { mixed })`: the languages offered, in order, and
  Mixed (none: those `announcer.ini` lists that its takes speak).
- What it reads: `Announcer.files()` (the pack's sounds, sorted;
  `Announcer.files("en/")` those under a folder), `Announcer.param(name)`
  (its parameters' values as `announcer.ini` declares them, a skin's
  types, their defaults where none is kept, and the Language chosen, a
  tag or `mixed`, nothing for the game's), `Announcer.language()` (the
  game's language). Nothing of the player, the play, the clock or
  anything outside the pack.
- **Its budget:** Lua 5.1's globals and nothing that reaches outside
  (`ddi_lua`), 1,000,000 fuel, 4 MiB, strings of 64 KiB, its source at
  most 64 KB, and where the shell gives a clock a quarter of a second
  (the backstop behind the fuel; the shells give theirs and a seed of
  their randomness for its tables' hashing, `script::set_host`).
  `math.random` has a fixed seed: the pool is a function of the files,
  the values, the game's language and the script.
- **Checked in Rust:** every take's file one of the pack's, every event a
  line the game knows, tags names (letters, digits, `_`, `-`, 24 at most),
  at most 8,000 takes, 16 tags. A script that fails (an error, its fuel or
  memory spent, a file outside the pack, a line the game does not know)
  is refused whole: its folders speak instead (`Pool::plain`), the log
  says why, and Options says "Its script stopped" beside the announcer.
- **Parameters** are declared in `announcer.ini` as a skin's are
  (`docs/plans/skin-parameters.md`: Choice, Toggle, Number, Color) and show
  as rows under the announcer, kept per announcer in
  `Settings::announcer_params` with its tags' switches (`tag.<name>`).
- **The built-in announcer runs on its script**
  (`assets/announcers/Clank/announcer.lua`): every sound of its voices a
  take of its folder's line in its folder's language, both offered alone
  or mixed. A test compares what it says with what its folders say, in
  every choice and every game language
  (`the_built_in_announcer_s_script_says_what_its_folders_say`), and
  `xtask gen-announcers` refuses a bundled script that stops or leaves a
  line out. The web's index carries the script's text; desktop reads it
  in place, folders and zips.
- **Not built** (the proposal's later stages): scripts told the game's
  moments during a play; a game-wide switch for well-known tags; "how
  often"; Opus copies of the built-in for the web. The studio writing
  multi-language packs, tags and a take on several lines came on
  2026-11-18 ("The studio makes pools", below).


## A line ends where its voice ends (2026-11-17)

The stadium processing ("Shared Rust studio and generation processing")
gave every line of our announcer a reverb tail of about 0.6 s, and the
voice counted a line as being said for its file's whole length, so it
stayed busy longer and passed over more lines. The maintainer, asked:
**"Tail doesn't block"**: "A line counts as finished when its voiced
part ends; the next line may start over the fading tail. I'd measure the
voiced end when the pack is made or loaded."

- **The rule, `voice-tail-v0`** (`ddi_library::announcer::tail`), over a
  sound's decoded samples, every channel together: its level over 10 ms
  frames and its voiced level as the leveler measures them (the RMS of
  the frames within 30 dB of the loudest, `tidy`'s); its voice ends at
  the end of the last frame no more than 20 dB under that level
  (`TAIL_DB`, compared as powers, no logarithm); what follows is its
  tail if it lasts a quarter of a second or more (`MIN_TAIL`), else it
  has none and its voice ends at its end. A silent sound has no voiced
  frame and no tail (counted whole, as before); the loudest frame is
  always voiced, so no sound is all tail.
- **Why those numbers**, measured on our pack's 433 sounds (decoded at
  48 kHz) as committed and as they were before the stadium commit (read
  from Git): 20 dB under the voiced level, the dry sounds end at most
  0.16 s (English) and 0.17 s (Japanese) after their last voiced frame
  (the leveler keeps 0.1 s after the speech), none of them a tail; every
  processed line has 0.52 s or more after it but the opening's (ready,
  restart, here we go: they kept their dry length), which have none. A
  processed line's voice ends a median 0.08 s (English) and 0.10 s
  (Japanese) after the same speech's dry voice did: the reverb's first,
  loudest moment counts as voice. At 15 dB a dry line's quiet last word
  measured 0.31 s (a tail where there is none); at 30 dB the reverb
  counted for 0.2 s of the voice.
- **Our pack's tails** as desktop decodes it (`voice::tests::tails`):
  English 205 of 217 sounds with a tail, median 0.69 s, from 0.62 to
  0.78 s; Japanese 204 of 216, median 0.62 s, from 0.53 to 0.72 s; the
  twelve of each language without one are ready, restart and here we
  go. Before the stadium commit: none of the 433. The longest voices
  are now 1.8 s (a battle line) and 2.5 s (a combo line), against 2.5
  and 3.2 s for the whole sounds.
- **Where it is measured:** as each shell decodes an announcer's sound
  for a play, a stage, song select or the studio's scenes (every pack,
  imported ones too, no pack change): desktop on its decode thread
  (`play::voice::tail`), the web on the main thread after
  `decodeAudioData` (`announcers::tail`, reading the buffer's channels
  once; `__DDI_DEBUG.announcerTails` counts the sounds and the
  milliseconds). Measured: a play of Beatucada's Beginner decodes 45
  sounds, their tails taking 6 ms (headless Chrome) and 8 ms (headless
  Firefox, Playwright's build) of the main thread in all, spread over
  the decodes behind the prompt; a practice stage's 57 and song select's
  24 together 10 and 15 ms; desktop, on its decode thread, 0.09 ms a
  sound in a debug build (20 ms for 217). The voice keeps each sound's tail beside it
  (`Voice::add_from`, `add_ranked_from`, `add_tailed`). Nothing is
  written into the bundled index: the web needs no tail before a sound
  is decoded, since no line is placed before its sound is in.
- **What the voice does with it** (`ddi_session::announcer::voiced_length`):
  everywhere it asked how long a line is said, it is now until its voice
  ends: `here we go` placed to end half a second before the first note
  and ready before it (both untailed in our pack: placed as before),
  cleared after the full combo's line, a stage's line being said
  (`StageVoice::busy`): a battle line that matters less passed over only
  during the voice, a step's line and a combo line waiting for the
  voice's end, a combo line or the end's lines dropped where a battle
  line is due before their voice would end, the stage's results' outcome
  after cleared's voice. Nothing else changed: which line wins, the
  windows, the gaps, the deadlines. Song select's lines and the results'
  (the cheer 2.5 s after the line) never asked; the studio's scenes'
  lengths (how long a scene waits for its sound) stay the whole sound.
- **Both buses mix:** each line is a sound of its own on the voice's
  bus (desktop's mixer adds every voice it plays; the web's bus is a
  gain node every line's source node joins), and neither ever stopped
  one sound for another: a line that matters more started over one that
  matters less before, and still does, the lower one playing on under
  it. So a tail plays to its end under the next line. A quit, a restart
  and leaving stop all of it as before.
- **The seeded stage** (`room_stage_voice`, Beatucada's Medium, seed 7):
  the same thirteen lines in the same order, four passed over, one of the
  four launches said (the other three come in the frame an attack
  against the player is said, which matters more) before and after; the
  difference is that the match's outcome starts where the full combo's
  voice ends, 0.75 s (English) and 0.64 s (Japanese) earlier, over its
  tail. Its quiet moment is now 5 s (a battle line's voice of 1.9 s at
  most after a combo line's of 2.6 s), not 8 s.
- **Checked in the browsers** (headless Chrome and Firefox, muted, the
  built-in announcer in English, `__DDI_DEBUG.announcer`'s `voicedEnd`
  and `soundEnd` beside `at`): an autoplayed play of Beatucada's
  Beginner, its sixteen lines with none passed over, ready and here we go with no
  tail (their voice ends with their sound), cleared beginning where the
  full combo's voice ends, 0.64 s before its sound does; a CPU practice
  stage with attacks, a battle line after a combo line's voice, the
  stage's win after the full combo's and (Chrome) a step's line after
  an attack line's voice, each over the tail before it.


## The studio makes pools (2026-11-18)

The proposal's stage 3, in `docs/plans/announcer-studio.md` ("Packs as
pools"): the announcer studio makes what packs allow, in the format the
game's reader takes and checked by it.

- **Languages:** a draft's languages, each with its own recordings,
  written as a folder a language beside an `announcer.ini` declaring them
  (`Languages=`, `Labels=` each named in itself, `Mixed=1` where the
  author turns Mixed on). One language with no tags and no shared
  recordings stays StepMania's plain layout, byte for byte as before.
- **Tags:** words on recordings; a tag made switchable, with its label
  and first state, is a `pack:tag` in the written `announcer.lua`, so a
  row under the announcer in Options → Sound.
- **A recording on several lines:** written once in `shared/`, listed in
  the script with its lines (`events`) and its language (`lang`).
- **The script the studio writes** uses the documented API alone
  (`Announcer.pack`, `Announcer.files`, `pack:take`, `pack:tag`,
  `pack:languages`), is readable and editable by hand, and is the same
  bytes for the same draft.
- **Every export is read back** through the scan, the declaration, the
  script and the pool (`ddi_library::announcer::draft::Draft::check`)
  and compared with the draft before it is saved or used; a pack the
  reader takes otherwise is the studio's mistake, refused and said.
- **Importing a scripted pack** into the studio reads its pool
  (`draft::read_pack`) and says what the draft cannot carry over.
- Nothing about how the game reads or speaks a pack changed: the pool,
  the script host, the Options rows and the voice are stages 1 and 2's.

