# Announcer studio

Status: steps 1 and 2 done 2026-11-06 (the draft; the screen on the web), step 3
2026-11-07 (the scenes on song select and the results), step 4 2026-11-08
(the scenes in play, on the web), step 5 2026-11-08 (desktop), step 6
2026-11-08 (the encoder): done. A screen for someone recording an announcer in
an app of their own:

- Their recordings come in through a picker or by dropping them.
- Each recording is put on the line it is.
- Each line is heard where the game speaks it.
- The pack goes out as a zip that Add songs imports as it is.

**The web comes first, and the pack is for this game** (the maintainer,
2026-11-06). StepMania reading it too is at most an Advanced checkbox,
off by default (decision 6). Its authors are
expected not to know how Dito Dito Infinito or StepMania work beyond
playing them, so it must be easy to use. Desktop runs the same screen
with its pickers and no drop, as its importer has none.

It builds on `announcers.md`. The lines and when they speak are
`docs/announcer-lines.md`'s.

## Feasible: what exists, what does not

Exists:

- **The lines.** Their folders, aliases, where each is spoken and what
  it falls back to (`ddi_library::announcer::Line`, `classify`,
  `chain`), and the scan an import runs (`find_announcers`).
- **When a line speaks.** This is pure and runs on any clock
  (`ddi_session::announcer::Announcer`: `tick` over a `Moment`,
  `results` over an `Ending`). Each shell has the voice's own bus.
- **Stereo.** Sounds keep their channels: the web keeps the file's,
  and desktop keeps mono and stereo, folding more channels to stereo.
- **Decoding,** the platform's first with our own behind it. The web
  uses the browser's decoder, then Symphonia, with Opus through the
  worker. Desktop uses `decode-all`.
- **Plays begun partway through a chart.** The editor's playtest from
  its cursor (`SessionConfig::start`), autoplay and its bias, and
  playtests that go back to their caller and are never kept
  (`#/play?…&edit=1`).
- **Song select and the results on sample data** (`screens::sample`).
- **Files on the web.** Pickers inside the press, drops of files and
  folders (`files::drop_entries`), and downloads made inside the press
  (the editor's export). Desktop has `rfd`'s pickers and save dialog.
- **Our two packs.** Every line said in English and Japanese, from the
  texts in `tools/announcer-design/pack.toml` and `pack-ja.toml`.

Missing:

- **A zip writer.** The library only reads zips.
- **WAV writing.**
- **The draft.** Recordings put on lines, tidied, kept and written out.
- **Drops that land on the canvas.** The UI hears that files were
  dropped and where, so a recording can be dropped on its line.
- **A play staged at a moment.** Three things are missing:
  - a start that brings the combo, life and counts so far (a start now
    begins from nothing);
  - autoplay that misses chosen notes, or presses them late;
  - the draft as the play's voice (a play now speaks only stored or
    bundled announcers).

None of it touches judging, the clock or the history: a staged play is a
playtest.

## Decided here (the maintainer may change any)

1. **One screen of the theme** (`ddi_ui::screens::studio`), as the
   editor is, built for the web first.
   - It is at `#/announcer`, and Options → Sound → Your own announcer →
     Make one… opens it.
   - Desktop gets the same screen at the end (step 5): `--screen
     announcer`, its pickers, no drop.
2. **Plain names, every line equal.**
   - Lines are named by what they are and when they speak ("Ready: as a
     song starts", "100 combo: a hundred notes in a row"). Judgments use
     the ruleset's names.
   - Folder names appear only in the export, and with Advanced's
     Details for those who know them.
   - Every line is listed in the game's order and none is pushed. A
     line left out is shown with what speaks instead, or as silent.
3. **Examples of what to say,** in English and Japanese.
   - An Examples switch picks the set, and the choice is remembered.
   - Every variant our packs have is an example, each with its sound
     (Mr. Clank's, ガキンさん's).
   - More examples are written for the studio where our packs have only
     one or two, as text only, in `crates/library/src/announcer/examples.toml`
     (a set's table, a line's folder, its examples; `{n}` for every
     combo), which anyone may add to: the library's build turns it into
     a table, and its tests check it names real sets and lines.
   - Examples are suggestions, never checked against the recordings.
4. **Recordings come in without anyone naming files.** Recording apps
   name them "New Recording 12".
   - **Each line has Add recording,** a picker for that line that takes
     one recording or several (its variants).
   - **A recording dropped on a line goes on it.** The row under it
     lights up while it is dragged.
   - **Recordings dropped anywhere wait under "Not sorted yet".** A
     sorter plays each one and offers the lines in the game's order, the
     line after the last one used suggested.
   - **Names are matched where they help.** A path through a line folder
     (a whole pack), or a file named for its line (`gameplay ready.wav`,
     `Gameplay_Ready_2.m4a`).
   - **A pack made before** (here, or for StepMania) is imported from
     Advanced, as a zip or a folder: its recordings on their lines, its
     name, and the author and license its README gives, where the draft
     has none.
5. **Recordings are tidied, by default.**
   - The silence at both ends is cut, since the game times `here we go`
     and the cheer from a sound's first sample. The volume is evened out
     to our packs' level.
   - Advanced → Tidy recordings turns it off for every take; Advanced →
     Details shows what it did to each (the silence cut, the volume
     change).
   - The draft keeps the original untouched, and a take keeps its
     channels.
6. **The formats are what this game reads.**
   - **Untidied takes keep their bytes.** That covers MP3, WAV and
     Ogg (Vorbis or Opus), plus `.opus` and FLAC files, which the import
     learns to read: both shells already decode them.
   - **Tidied takes, and everything else,** are written as Opus (step
     6), about a tenth of a 16-bit WAV's size. M4A/AAC is converted
     because desktop does not decode it. A take of more than two
     channels stays a WAV. A compressed take that was tidied before
     keeps its bytes, so a pack opened again and exported loses nothing.
   - **Advanced → Also for StepMania** (off by default) keeps the
     formats StepMania reads: MP3, WAV and Ogg Vorbis. Anything else is
     written as WAV.
7. **The previews are the game's own, staged, never a mock-up.** Each
   line has one button, Hear it in the game. Each scene has:
   - the real screen the line is spoken on, with a caption saying what
     is happening ("The combo reaches 100");
   - the real announcer deciding when the line speaks: the gaps, the
     drops, the fallbacks and the cheer's delay;
   - the music at the player's volume, and the voice at its share on its
     own bus;
   - a bundled song.

   A scene starts a few seconds before its moment and ends after its
   line, back in the studio.
8. **Nothing is lost.**
   - The draft is saved as it changes, on the web in IndexedDB with
     persistent storage asked for, and opens where it was left. While
     an import's takes are decoded it is saved once a second and at
     the end, and the page shown again once a frame, so a few hundred
     takes do not write and lay out the whole draft for each.
   - Use in the game adds it as an announcer to choose under Options,
     and to hear in real plays.
   - Any imported announcer opens in the studio again, to carry on with
     it.
9. **Export.** It downloads `<Name>.zip`:
   - the sounds in `Announcers/<Name>/<line folder>/`, the layout our
     import takes whatever lines a pack holds (it is also StepMania's);
   - entries stored rather than compressed, names in UTF-8, and fixed
     times, so the same draft gives the same bytes with the same encoder
     (each browser's own differs; ours is the same everywhere);
   - a `README.txt` with the name, author and license typed in (the
     author and license under Advanced), which the import passes over
     and the studio reads back.
10. **Notes on a line, never blocking:** a take longer than its moment
    allows (a combo line over 2 s, a comment over 10 s, a results line
    over 2.5 s, or `ready` and `here we go` together longer than most
    lead-ins), a take that clips, a file that cannot be read, and the
    import's caps (2,000 sounds, 128 MB).

## Step 1. The draft (`ddi-library`)

- **`announcer::draft`.**
  - `Draft`: name, author, license, takes by line, and the unsorted
    ones.
  - `match_line(path)` (decision 4).
  - The tidy: the ends found by level, and the gain to a target
    loudness, from the samples the shells decode.
  - Decision 10's notes.
  - The paths the zip holds.
- **The catalog, for people.** Each line's plain name and when it
  speaks, and the example sets. The sets are our packs' texts and each
  text's sound, carried from `pack.toml` and `pack-ja.toml` at build
  time, plus the studio's own examples.
- **`zip::write` and WAV writing.** Stored zip entries with the UTF-8
  flag, the CRC-32 the reader already computes, and a fixed DOS time.
  WAV in 16-bit PCM, any channel count.
- **`.opus` and `.flac` in `AUDIO_EXTENSIONS`.** Both shells decode them
  already, and the import's report names them as it names the others.

Check:

- Matching on the names recording apps give: `New Recording 3.m4a` stays
  unsorted, `Gameplay_Ready_2.m4a` is matched, and a pack opens from
  each of its root shapes.
- The tidy, on generated takes with silence and noise, mono and stereo.
- The export, read back by `find_announcers`, gives the same lines and
  passes nothing over.
- The same draft gives the same bytes twice.
- Japanese names survive the round trip.

**Done 2026-11-06.** `ddi_library::announcer::draft` (the draft, kept
as JSON; `Format::sniff`; `match_line` and `by_name`; the notes and
`limit`; `opening`; `size`; `export`'s plan), `announcer::tidy` (a port
of kokoro.py's `trim` and `finish`: the floor rises over a recording's
noise, never within 20 dB of the loudest frame, and the cut keeps 30 ms
before and 100 ms after), `announcer::examples` (the English and
Japanese sets, more for the combos and full combos, from
`examples.toml` since), `zip::write_stored`
and `wav::write`. The timings the notes measure against moved into the
library (`COMMENT_GAP`, `COMBO_GAP`, `CHEER_DELAY`, `GO_BEFORE`; the
session re-exports them). Our packs' sounds' texts are in their
`PROVENANCE.toml` (`[said]`, which `kokoro.py pack --bundle` writes and
`gen-announcers` checks against the sounds) and in the web's index
(`Bundled::said`). The import reads `.opus` and `.flac` too. Choices
made on the way:

- **The studio lists every line but Etterna's two** (`evaluation
  aaaaa`, which no rules give, and `evaluation f`, whose moment is
  `evaluation e`'s), and any line a take is on (combos past 1000 too).
- **`README.txt` sits at the zip's root,** beside `Announcers/`, so the
  import passes nothing over in the announcer's folder.
- **`ready` and `here we go` together** are noted past 4 s with the half
  second before the first note (`OPENING_ROOM`).
- **The lines' plain names and when they speak** are the screen's
  words, so they come with it (step 2); the examples are data, here.

Checked: the library's tests (formats, names, sorting, writes, notes,
sizes, the draft as JSON, an export zipped and read back by
`find_announcers` with the same lines and nothing passed over, the same
bytes twice, Japanese names; the tidy on generated takes with noise,
mono and stereo, clipping and silence; the zip and WAV writers).

## Step 2. The screen on the web

- **Every line, grouped as the game speaks them:** song select, in
  play, the results.
  - Each row shows the line's name and when it speaks, the examples,
    its takes (each with its waveform, a play button and Remove), Add
    recording, Hear it in the game (steps 3 and 4), and its notes.
  - "Not sorted yet" sits at the top, with its sorter, while it holds
    recordings.
- **Drops.** The page tells the UI that files were dropped, and where,
  while they are dragged (a new `UiEvent`). The screen hit-tests the
  row under them.
- **Hearing a take.** It plays on the screen's own audio context,
  started from the press (the web's rule).
- **Saving and exporting.** Name, author and license; the draft saved
  as it changes; Use in the game; Export (decisions 8 and 9).
- **Advanced,** folded away: the author and the license, the pack
  imports (decision 4), Tidy recordings (decision 5), Also for StepMania
  (decision 6) and the Details (decision 2).

Check:

- UI tests on sample takes.
- Headless Chrome and Firefox (`firefox.mjs`):
  - recordings picked for a line, dropped on a row and dropped anywhere
    (a synthetic `DataTransfer`), then sorted;
  - a reload that keeps the draft;
  - the export downloaded and imported through Add songs in a fresh
    profile, giving the same lines.
- A pass by the maintainer.

**Done 2026-11-06.** The page is the theme's page screen
(`ddi_ui::screens::page`) with blocks of its own for the studio: rows of
buttons, a line pressed as a link (`Action`), fields (`Field`, typed as
song select's search is), notes in the look's `Gold`, and zones
(`Zone`: the actor `Zone.<id>`, lit by `PAGE_TARGET`); Left and Right
move along a row, and the item focused stays so when the page changes.
`ddi_ui::screens::studio` makes the page from the draft (each line's
plain name, when it speaks, what is said without it, its examples
folded, its takes with Tidy, Move… and Remove, Add recording; the
unsorted on top; Advanced folded) and reads its presses (`act`); its
line picker is the editor's actions dialog. The web's page
(`components/studio_page.rs`, `#/announcer`; Options → Sound → Your own
announcer, Make one…, on the web) keeps the draft in IndexedDB's `files`
(`studio/draft.json`, `studio/take/<id>`), decodes on an offline context
at 48 kHz, plays on a context made in the press (waiting for it to run),
finds the zone under a drop (`ui_dev::virtual_point`,
`Ui::visible_bounds`), opens zips, writes the export (Export, then Save
in its press) and stores it as an imported announcer (Use in the game).
Choices made on the way:

- **Export takes two presses:** making the zip waits on decoding, and a
  download must start in a press, so Export makes it and Save
  downloads it. Any change to the draft lets the zip go.
- **Files that are no recordings** (a readme, pictures) are passed over:
  a file is taken when its first bytes are a format the game reads, or
  its extension a recording app's (`.m4a`, `.aac`, `.caf`, `.aiff`,
  `.webm`…).
- **The examples' set** is remembered in `localStorage`
  (`ddi.studio.examples`).

Revised after the maintainer's pass (2026-11-06), to keep the page
simple for players:

- The top holds the name, the examples' set, Use in the game and
  Export. By and License, the imports of a pack made before (Import a
  pack from a zip…, Import a pack folder…; they were Add recordings… and
  Add a folder…, which did not say what they were for), Tidy recordings
  and the Details (each line's folder, what tidying did to each take)
  moved under Advanced. Takes lost their own tidy switch.
- The sections are "On the song select screen", "During gameplay" and
  "On the results screen".
- A line's zone is a card: an item in it puts the active outline around
  the whole card (`PageScreen::active_element`), and a move onto it
  scrolls the whole card into view where it fits.

Checked: the page's and the studio's tests (fields typed into, buttons
side by side, zones lit; a zone for every line and the unsorted, the
presses read back, what speaks without a line, the examples joined);
Options' row on the web and not on desktop; headless Chrome and Firefox
(`target/scratch/ann/studio.mjs`, `studio-ff.mjs`): four recordings and
a readme picked (the readme passed over, the names matched), one
dropped on a line's zone (lit, then on it), a take played from its
press (on a running context, tidied), an unsorted one sorted, a
Japanese example played, the pack named, exported and saved (stereo
kept), the zip imported through Add songs in a fresh profile, Use in the
game listed in Options, the draft kept over a reload; no errors.

## Step 3. Scenes on song select and the results (web)

- **Song select** runs on sample data with its preview music playing.
  - `select music intro` speaks as it shows.
  - Each comment speaks on choosing a chart that gets it: meter 10 or
    more for `hard`, one played in this run for `repeat`, and so on.
- **The results** run on sample data shaped by the line.
  - The game's precedence picks the line (`Announcer::results` over a
    staged `Ending` and `Prior`).
  - The cheer follows 2.5 s later for AA or better.
  - `quit` gets a quit's results.
- **The draft is the voice,** tidied, decoded from the draft as for a
  play.

Check:

- Session tests on the scenes' facts.
- Headless Chrome: each scene speaks its line (`__DDI_DEBUG.menuAnnouncer`,
  `__DDI_DEBUG.announcer`).

**Done 2026-11-07.** Every line of song select and the results has a
scene (`studio::hearable`; not `evaluation aaaaa`, which no play gets).
Where the draft has something to say for the line (a recording on it,
or on what speaks in its place, `studio::recorded`), Hear it in the
game says the scene with the draft. Elsewhere Hear an example in the
game says the whole scene with our pack of the examples chosen (Mr.
Clank, ガキンさん), so a line is heard in its place before it is
recorded (the maintainer, 2026-11-07); the sounds it needs, one of each
line the scene may say (`announcer::results_lines`, `cheers`), are
fetched and decoded before it shows, once a session, and its caption
says whose example it is. The scene is the real screen pushed over the
studio once its voice is ready (`components/studio_page.rs`,
`Ui::push_over`: the studio leaves first, as for a screen's own push),
what it is shown with made in `ddi_session::announcer::scene`:

- **Song select** over the bundled songs, three of their charts played
  before (a day before, one this session), opening on the chart that
  gets the comment. Its preview plays on song select's own sound, made
  in the press. Its lines come from the draft in place of the announcer
  chosen (`announcers::stage`). The intro is said as it shows (the
  welcome's scene comes from the start menu, Singleplayer chosen on it);
  a comment's chart is pointed at once the intro is said, then chosen
  (Start), the preview and the intro stopping as a play opening would
  stop them, and the comment is said.
- **The results** of a play staged for the line: a chart made with the
  note count of Some Things Must's Hard, played through the engine
  (`Player`, as the noteskin demo plays) under the player's ruleset
  where it can give the line, else ITG, else DDR A (the plain full
  combo is only DDR A's: ITG's combo ends at a Decent). Each step is
  pressed off by a fixed draw from a normal spread: its width is
  searched for the grade wanted, one step always missed so the play is
  no full combo, and the grade then lies mid-band; for a full combo,
  every step inside its tier's window; for a fail, every step missed
  from a third of the way; a quit at two fifths. So its counts, score,
  grade, lamp, life and panels are the engine's own. Under failing as
  the ruleset has it, where a low grade cannot be reached alive, its
  play has failing off. The history before it says played and cleared,
  the best out of reach, but for a new record and a first clear. The
  cheer's scene has a grade of AA or better the draft says, else an AA
  play that is a new record or a first clear the draft says. The line
  is `Announcer::results`' choice over it, on a sound of the scene's
  own made in the press, as the results open. A new record's and a
  first clear's come from song select first, the chart's best before
  the play on it (the score beaten, cleared; failed), the chart chosen
  on it; song select says nothing there.
- **In a frame** (the maintainer, 2026-11-07: a scene should read as
  something shown, a video of it but not quite, with what comes before
  it and what the line is said for pointed at). The game's screens are
  drawn smaller inside a frame over the studio, laid out as on the
  whole screen (`ddi_ui::screens::stage::StageFrame`, a chrome whose
  `Screen::stage` places the other layers; it grows back to the whole
  screen as the frame goes). Above the picture, the line and whose voice
  says it ("Your recording", "An example by Mr. Clank"); below it, a
  bar filling until the scene ends, what happens now (each step's
  caption: "The start menu: Singleplayer is chosen.", then "Song select
  comes in.") and Back to the studio; over it, the words said as they
  are said (an example's, from its pack's `[said]`; a recording's line
  by name, "[Hard chart]"). What the line is said for is ringed in red
  (`Ui::mark` over a screen's `Screen::spot`, the theme's `Mark`):
  the screen's name for the welcome, the chart's level for the hard
  comment, its best (or the empty place of one) for the new and the
  repeat comments, the chart for the general one, the best before a
  record or a first clear, then the score beaten, the grade (QUIT for
  a quit), the full combo's lamp, FAILED.
- **Each scene ends** 1.5 s after its last line, and lasts 3.5 s at
  least, by the input a player would give (Back); Back, the frame's
  button, the results' buttons and the browser's Back leave it sooner
  (on the start menu, which Back does not leave, as Singleplayer is
  chosen). The studio
  then says what was heard ("Heard in the game: “Grade AA”, then
  “Cheer”."); a line scheduled and cut by leaving is not counted.
- **Each scene of a line takes its next recording (or example) first,**
  so pressing again goes through its variants.

Checked: the scenes' tests (every comment's chart gets it; every
results line is said first in its scene under ITG, DDR A, StepMania 5
and FFR or an unknown ruleset falling back; the cheer after a grade, a
record or a first clear; the staged plays' lamps, quit and fail; the
same play each time); the studio's (which lines can be heard); headless
Chrome under the user-activation autoplay policy and Firefox
(`target/scratch/ann/studio-scenes.mjs`, `studio-scenes-ff.mjs`,
`studio-scene-exits.mjs`): every scene says its lines (the preview
playing on song select, the comment after the intro, the cheer 2.5 s
after its line) and returns, and each way out works; the examples
(`studio-examples.mjs PORT [firefox]`): every line offers one in a fresh
profile, Mr. Clank says the scenes, then ガキンさん, and once a pack is
imported its lines say the draft and the rest stay examples. The frame:
its tests (the screens drawn smaller and given back, taps reaching
them through it, its bar and words; the mark on a spot once its screen
is in, on the stage where the picture puts it; each scene's spot found
on its screen, song select's being its parts in our theme; the record's
and the first clear's best before), and `studio-frame.mjs PORT
[firefox]`: every scene's screens in order, what it rings and when.
The welcome goes from the start menu to song select, the screen's name
ringed; a comment's chart part is ringed once the intro is said, then
chosen; the record's old best is ringed on song select, then its new
score on the results.

## Step 4. Scenes in play: staged playtests (web)

The real play screen (field, skin, HUD, background), on a bundled chart,
from a few seconds before the moment. It is autoplayed with the draft as
its voice. Afterward it goes back to the studio, or on into the results
for the scenes at a play's end.

- **A staged start.** The play begins at a song second with what came
  before it given: the judgments so far, the combo, the life and the
  lives. The engine's player takes them as if played. This is
  presentation only and never kept: a 1000 combo is a 995 combo given,
  not 1000 notes played.
- **Scripted autoplay.** Chosen notes are missed, or pressed into a lower
  window: a list beside the autoplayer, where `auto_bias` is one
  constant. It serves `combo stopped`, a life lost, danger, a fail, and
  the W2 and W3 full combos.
- **The scenes:**
  - `ready` and `here we go`, from the chart's start (a chart with a
    short lead-in for `here we go` alone);
  - `restart` (R twice);
  - the comments at full life, below full, in danger, and on a battery
    gauge;
  - each hundred, the overflow and `combo stopped`;
  - `life lost` and `last life`;
  - the full combos and `cleared`;
  - `failed`, carrying into the results.
- **The rest of the draft speaks too,** as the game would have it. A
  comment can take the gap a combo line wanted, as in a play. The
  caption then says what was heard and why.

Check:

- Engine tests on the staged start: the counts, combo and life as given,
  and judging after it unchanged.
- Session tests on each scene's lines on the fake clock.
- Headless Chrome.

Risk: the staged start is the one change inside the engine. If the player
cannot take a state cleanly, a scene starts earlier and autoplays up to
its moment. That is exact but longer, and combos past a chart's note
count then have no scene.

**Done 2026-11-08.** Every line said in play has a scene (all of them
but `evaluation aaaaa` are hearable now, `studio::hearable`), the real
play on Beatucada's Medium in Single, shown in the studio's frame:

- **The staged start** is what the risk above proposed, done at once:
  `Player::start_staged` judges the presses before the start in one go,
  as the play from the song's start would have (its tally, combo, life,
  lives and score exactly that play's, a test against it), then the
  clock starts there; a combo past what those notes make is given on
  top (at least that combo), so 1000 and the overflow have scenes on a
  217-step chart. Nothing about a gauge is set by hand: a life in danger
  or two lives left come from notes missed before the start.
- **The staging** (`ddi_session::play::Staging`, `SessionConfig::staging`):
  where the play starts, the notes missed or pressed off (`Touch`: a
  span of song seconds; off into a lower window for W2 and W3's full
  combos), the combo given, where the announcer's comment gap stands
  (`Announcer::staged`: no ready or here we go once a note came before
  the start), the song second the scene is over at
  (`SessionEvent::SceneOver`) and one the player restarts at (R twice).
  Always autoplayed, never kept.
- **The scenes** (`ddi_session::announcer::scene::in_play`): `pick` says
  the chart and what of the settings the scene changes (a battery of
  four lives for the oni comment and the lives' lines, failing at once
  for danger and the fail, DDR A's rules for the plain full combo; the
  player's rules otherwise, ITG where they are another game's), `stage`
  the staging under the rules and options the play has: ready and here
  we go from before the first note (here we go's starting late enough
  that ready has no room), the restart's played past the first note,
  then again from the same place opened by its line, the comments with
  their gap given and the life as each wants it (full where a play kept
  full is full, below it after a few misses, in danger after enough of
  them, found by simulating the play), each hundred and the overflow
  crossed a few steps in, a combo of 60 broken by a lone step missed, a
  life lost and the last life (two lost before), the full combos of each
  kind and a clear on the last steps, and the fail; those at a play's
  end go on into its results. A session test plays every one on the
  fake clock under ITG, DDR A and StepMania 5 and finds its line said
  before the scene is over.
- **On the web** the studio's press makes the play's sound and keeps it
  for the play (`keep_for_play`), the voice (the draft's play and
  results lines, decoded first where the page opened again, or our
  pack's) is handed over (`crate::studio_scene`), and the play route
  opens on `scene=<line>`: its settings the pick's (never stored), its
  start prompt in the frame a second, then the staged play, drawn into
  a texture of its own (as Options' field preview draws) that a small UI
  of the game loop's shows in the frame (`ddi_ui::screens::stage::Picture`
  under the same `StageFrame`), the words of each line as it starts, the
  caption the scene's. Over, or Escape, or the frame's button, goes back
  to the studio, which says what was heard (the line asked for, and how
  many more were said); a scene at a play's end shows its results in the
  frame first, their line said, for 6.5 s. Each line's next recording or
  example, and the decoded sounds, are kept for the session across the
  trip.
- **What a line is said for is ringed, in play too** (`in_play::mark`):
  the life gauge for what is said of the life (the comments, a life
  lost, the last life, a clear, a fail), the combo for what is said of
  it (a broken one vanishes inside the ring), the receptors for how a
  play opens. The renderer says where it drew them
  (`Renderer::hud_spots`, as shares of the frame); the picture
  (`ddi_ui::screens::stage::Picture`) keeps actors there for the ring
  (`Mark::Life`, `Combo`, `Receptors`). It shows from the play's start,
  so the eye is on the place before the moment.
- **Why another line came first** (`director::SaidWords`): while a line
  other than the one asked for is said before it, the frame's caption
  says why (a play opens with it, the announcer comments when it has
  been quiet, the combo got there on the way), and the scene's caption
  comes back with the line. Where the line asked for has no recording and
  what speaks in its place is said (`Line::chain`), the caption says that,
  on song select and the results too.
- **The page comes back where it was:** after a scene in play, which
  leaves the page on the web, its folds, scroll and focus are as they
  were (`PagePlace`, `PageScreen::with_place`).

Checked: the engine's staged start (`crates/engine/tests/staged.rs`);
every play line's scene on the fake clock under three rulesets; headless
Chrome under the user-activation autoplay policy and Firefox
(`target/scratch/ann/studio-play.mjs PORT [firefox] [line…]`, `DRAFT=1`
with `gen/voice.zip` imported): each scene opens the play in the frame,
says its line, and comes back with it heard.

## Step 5. Desktop

- The same screen, opened with `--screen announcer` or from Options.
  - Recordings come in through pickers only: Add recording on a line,
    and Advanced's imports of a pack. There is no drop, as its importer
    has none.
  - Export uses a save dialog.
- Use in the game writes `<data>/announcers/<name>/`, which desktop then
  lists beside the song folders' announcers. The draft lives in the same
  data folder.
- Scenes run in its play as on the web.

Check: desktop's offscreen tests through the shell.

**Done 2026-11-08.** Desktop runs the same page (Options → Sound → Your
own announcer, or `--screen announcer`) and the same scenes
(`crates/desktop/src/studio.rs`).

- **What both shells share.** What a scene does and when
  (`ddi_session::announcer::scene::director`: `Director`, over a shell's
  `SceneHost`: its clock, its UI, song select's sound, the voice), the
  draft as a voice (`draft_voice`), which files are recordings
  (`draft::takes`), a scene in play's staging and length
  (`in_play::staged`) and the words a play's lines show
  (`SaidWords`). The web's page moved onto it, what it does unchanged.
- **Kept** in the data folder: `studio/draft.json`, each recording under
  `studio/take/<id>`, written in order on a thread of their own (flushed
  before the game goes). Takes are decoded at 48 kHz on another; those
  read before are decoded when a scene needs them.
- **Recordings** come through pickers alone: Add recording (any file the
  game or a recording app makes), Advanced's imports of a pack, read on a
  thread. M4A/AAC cannot be decoded here (`decode-all` has no decoder
  for it): such a take stays on its line with the page's note that it
  could not be read.
- **Export** is one press: the save dialog first, then the zip made and
  written on a thread (on the web a download must start in its own
  press). **Use in the game** writes `<data>/announcers/<name>/` (whole or
  not at all, its README beside its lines), listed after ours and before
  the song folders', and Options lists them again; only those can be
  removed from Options, what is in a song folder staying the player's.
- **Scenes on song select and the results** run as on the web: the page's
  timers and steps are polled by the shell's loop (`Studio::poll`), the
  voice on the preview's sound in place of the announcer chosen
  (`Player::stage`).
- **A scene in play** is a `Request` with a `SceneOrder` on
  `play::host`: one UI, so the frame (`StageFrame`, a chrome) is pushed
  over the prompt, the play and its results alike; the play is drawn
  into a texture (`Formats::picture`, encoded values as Options' field
  preview) that `Picture` shows through the 2D pass. Its pointer is the
  frame's (`Host::framed`) and Escape leaves it. Never kept: no record,
  no ghost, no prior.
- **Found on the way:** a chrome pushed while another was still going
  landed under it, so `push_chrome` flagged the wrong layer and the frame
  did not show (`Ui::push_layer`: the chrome goes on top).

Checked: `crates/desktop/tests/studio.rs`, offscreen through the shell on
a fake clock with no sound device: recordings picked for a line and a
pack imported (name and credits), opened again as they were, a take
heard, the export cancelled and saved and read back by
`find_announcers`, Use in the game listed and removed, then song
select's scene and the results' by the draft, a line not recorded by our
pack, each kind of line in play in its frame (a combo, a restart, a fail
into its results, one left by the frame's way back). The director's
tests over a fake shell (`scene/director/tests.rs`); web scenes in
headless Chrome again after moving onto it (`studio-examples.mjs`,
`studio-frame.mjs`, `studio-scene-exits.mjs`, `studio-play.mjs`).

## Step 6. The encoder

Tidied takes are written as Opus, a tenth of WAV's size or less, which
makes Opus the default. It works the way decoding does, the platform's
encoder first and our own behind it:

- **The browser's encoder.** WebCodecs' `AudioEncoder` gives Opus where
  the browser has it (it never gives MP3 or Vorbis).
- **Our own encoder is in hand.** `rusty-opus`, which already decodes the
  web's fallback and desktop's Opus (BSD-3-Clause, pure Rust), has an
  encoder too. It serves browsers without `AudioEncoder`, and desktop
  (the system encoders give AAC, and MP3 on Windows, which are no
  better here). So the fallback costs no new dependency.
- **The Ogg pages around it** are a small writer of our own, written as
  `.opus`.
- **Also for StepMania** keeps writing WAV. A small pack StepMania reads
  would need a bundled Vorbis or MP3 encoder: not planned.

**Done 2026-11-08.**

- **The file** (`ddi_platform::ogg_opus`, on every target, no codec):
  `OpusHead`, `OpusTags`, then pages of at most a second, the last one's
  granule ending the sound on its exact frame. A packet's length comes
  from its own first bytes, not from what an encoder says of it. Packets
  wholly past the end are left out, as a browser's encoder runs on after
  its input. The serial number and vendor string are fixed, so the same
  packets give the same bytes.
- **Rates:** 64 kbit/s mono, 96 stereo, variable; 20 ms packets. Both
  shells decode takes at 48 kHz, which is Opus's own, so nothing is
  resampled for it.
- **Ours** (`ddi_platform::opus::encode`, desktop on its export's thread
  and the web in the Opus worker) asks for CELT alone
  (`Application::RestrictedLowDelay`). Its delay is then 120 frames
  whatever is said, measured; `rusty-opus` has no call that says it. On
  these voices the automatic mode chose CELT for every packet and gave
  the same bytes.
- **The browser's** (`web::opus::encode`) is asked for the same rates;
  its pre-skip is the `OpusHead` it gives in its metadata (312 in Chrome
  and Firefox), or libopus's usual 312 where it gives none. A browser
  without one, one refusing the configuration and one failing midway all
  go on to the worker, with a warning in the console for the last. The
  worker grew from 399 to 672 KB raw (178 to 298 KB gzip, 151 to 251 KB
  brotli) and is still fetched only when it is needed; the app itself
  gained only the file's writer and the calls to `AudioEncoder`.
- **Size told beforehand** (`Draft::size`, for the import's 128 MB) is
  estimated from the length and the bit rate.
- **Encoded once.** Tidying and encoding a take and measuring the result
  again finds at most 10 ms to cut at the start, 30 ms at the end and
  0.36 dB to change, over the 334 sounds of our packs and test voices.
  A take within a little more than that (`Measure::already_tidy`) that
  is compressed already keeps its bytes.

Checked: `ogg_opus`'s and `opus`'s tests, among them each page's CRC,
the lacing at whole 255s, the second-long pages, the end's trimming,
every length around the frames' and the delay's edges, the start found
at lag 0, no panics on samples no sound has, the same bytes each time.
`ffmpeg`'s own decoder and libopus's read our encoder's files
frame-exactly, at lag 0, as near the original as libopus's encoder at the
same bit rate (22 to 27 dB of SNR, mono; correlation 0.996 to 0.999) and
about as large or smaller. Desktop's offscreen test exports, decodes each file back at
its tidied length and a tenth of its WAV, and with Also for StepMania
gets WAVs alone. In headless Chrome and Firefox
(`target/scratch/ann/studio-opus.mjs`, not committed), each by its own
encoder and each by ours (`__DDI_FORCE_WASM_ENCODE`), and in Chrome with
`AudioEncoder` removed, refusing, throwing on configure and on encode:
every file the right length, starting on the recording's own frame, read
by `decodeAudioData` and by `ffmpeg`, 10 to 16 times smaller than its
WAV. Ours gives the same bytes in both browsers. A pack from each encoder,
imported through Add songs and chosen, spoke its lines in a real play
(Chrome and Firefox). Playwright's WebKit could not be run here (the
installed revision no longer matches its driver), so Safari is the
maintainer's to try.

## Not taken

- **Recording inside the game.**
- **Editing beyond the tidy.**
- **A phone layout of its own, and advice on recording or sharing.**
- **Drops on desktop,** until its importer takes them.
- **The lines the game reads but never speaks** (StepMania 3.9's stage
  lines, the screen lines). The import does not keep them either, and the
  studio neither shows nor exports them.
