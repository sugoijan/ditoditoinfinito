# Additional chart versions (2026-10-18)

Two simplifications offer generated versions of charts beside their
originals: **Without onigiri** (2026-10-18, below) and **Without center**
(2026-10-24, "Pump charts without the center"), and two conversions
between pump single and Dancing☆Onigiri's 5 keys (2026-10-25, "Pump
single and 5 keys"). All are kinds of `Chart::derivation` made by the
same code (`crates/chart/src/simplify.rs`) and listed, kept and replayed
the same way.

## Without onigiri

Options → Notes → Simplify → **Without onigiri** exposes additional
versions of Dancing☆Onigiri charts. Off by default. Originals stay in
their original layouts and are unaffected by this setting. Existing
timing cut, At once and No holds options retain their behavior.

| Original | Additional layout | Notes |
| --- | --- | --- |
| 5 keys | Single | Four arrow lanes |
| 7 keys | Solo | Six arrow positions; Solo uses an up-left diagonal |
| 9A keys | Double | Two sets of four arrows |
| 9B keys | Techno 8-panel | Eight directions; lanes reordered to Techno track order |

Onigiri taps, holds and visual notes are removed from the additional
chart. Remaining notes retain their exact timing, holds, colors and
speed multipliers. Timing, ruleset hints, start/end/fade, lyrics, back
and mask layers still come from the original. An empty result is not
offered. Imported files and original charts are never rewritten.

`crates/chart/src/simplify.rs` appends these versions after all original charts,
with explicit `Chart::derivation` provenance. Importers themselves still
return the original charts. Shared library loading adds versions for
both native works and simfiles with `danoni-*` layouts, including SSC exports,
independently of the display setting, so original indexes and digests
stay compatible and a converted chart's URL or replay works with the
option off. Song select filters the additional versions by the setting.

The engine marks a play assisted from its chart provenance, independently
of the global options. Original plays keep their normal results and
bests; converted plays have their own digests and assisted bests. Replays
recreate the same chart and mark. Autoplay also shows ASSIST when the
played chart was simplified. Ghosts use the matching assisted best.

Old browser imports gain versioned chart metadata from their stored chart
data when the library loads; no reimport or music read is needed. The
desktop library cache advances to version 5 and rescans its work metadata.
The URL SDK keeps its admitted catalog and receipts original-only; playback
consumers enrich a cloned entry from the verified SSC, preserving those
publication bytes and every original chart’s metadata.

Techno single8 follows StepMania's track order and keyboard defaults
([GameManager.cpp at 825467b](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/src/GameManager.cpp)).
It adds arrow-key aliases alongside the spatial Q/W/E, A/D, Z/X/C and
numeric-keypad bindings. Standard controllers use the d-pad/face buttons
for cardinal arrows, shoulders for upper diagonals and triggers for
lower diagonals. The chosen dance noteskin draws it through the existing
lane-to-button fallback. The adapted data is credited in
`THIRD_PARTY.toml` and `REUSE.toml`.

Verification covers all four conversions, original index/digest
preservation, empty-result handling, idempotence, imported metadata
upgrades, assisted results, separate bests, replays, lyrics and layers;
browser checks import a synthetic work and exercise Options and play.
The opt-in local autoplay corpus also reads generated versions and checks
their assisted flags, judgements and replay results; third-party inputs
remain in local scratch storage.

## Pump charts without the center (2026-10-24)

The maintainer's request: a pump chart played as a dance chart, its
center removed and its diagonals turned onto the four arrows. Options →
Notes → Simplify → **Without center** (`Settings::simplify_center`)
exposes these versions. **On by default**: settings stored before it
existed get it on, since the settings' struct fills what a stored value
lacks from `Settings::default()` (no migration step). Turn Left or Right
on top of it gives the other rotation. Since 2026-10-25 the option has
three values and lists, by default, only the versions a style lacks a
chart for ("Where missing", below).

| Original | Additional layout | Lanes |
| --- | --- | --- |
| Pump single | Single | DL → Left, UL → Up, UR → Right, DR → Down |
| Pump double | Double | each pad the same way, onto its own pad |
| Pump half-double | Single | pad 1's UR → Up, pad 1's DR → Left, pad 2's DL → Down, pad 2's UL → Right |

The pad is turned an eighth clockwise: each corner panel goes to the
arrow 45° clockwise of it, so a step keeps where it is around the
player (`ddi_chart::simplify::without_center`).

**Half-double** (decided here): its six panels are pad 1's C, UR, DR
and pad 2's DL, UL, C (StepMania's `pump-halfdouble`, `docs/plans/pump.md`).
Without the two centers, its four corner panels stand around the gap
between the pads as one pad's corners stand around its center: pad 1's
UR upper left, pad 2's UL upper right, pad 1's DR lower left, pad 2's
DL lower right. They are turned by that position, as Single's are,
which keeps every movement between them (a step right stays a step
right, a crossover a crossover). Turning them by their arrows'
directions instead would send pad 1's UR (an up-right arrow on the
left of the four) to Right and pad 2's UL to Up, reversing left and
right. The precedent offers every played layout a version
(Dancing☆Onigiri's 7 keys went to Solo), so half-double is offered on
Single rather than left out.

Every note on a center lane is removed: taps, holds and rolls, mines,
lifts, fakes and anything else there. Every other note keeps its row,
kind, hold or roll end and attributes; the chart keeps its timing
(split timing, `#TICKCOUNTS` included), meter (pump's scale, as its
author rated it), difficulty, credit and chart-scoped effects
(`Song::effects_chart`). Its name is the original's followed by
"· without center", or "Without center". A version without a judged
note (a tap, a hold or roll head, a lift) is not offered.

**Rules.** The version is played by its source's game, pump, as the
onigiri-free versions keep their source's Dancing☆Onigiri ruleset hint.
Pump differs from dance in its holds only: StepMania 5.1 sets
`m_bTickHolds` for the `pump` game alone ([GameManager.cpp at
825467b](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/src/GameManager.cpp)),
and taps, mines and lifts are judged as dance judges them. A pump
chart's holds are written for ticks: judged at the chart's `#TICKCOUNTS`
checkpoints, never dropped before their end, a release recovered by
pressing again, a head not required (`docs/plans/pump.md`, "Ticked
holds"). So under Rules → Holds "by the chart's game", the default, the
version's holds are ticked (`Chart::pump_game`, read by
`HoldStyle::ticks`); "dance-style" plays them as dance holds, as it does
the originals. The list's best key reads the provenance too
(`Settings::best_key_for`), so its bests are found under the rules
played. Under "each chart's own game's rules" the StepMania presets
apply, as to the originals.

**Drawn and played as dance.** The layout is `dance-single` or
`dance-double`, so the chosen dance noteskin draws it (the Pump arrows
row does not apply), its bindings, controllers and touch columns are
the dance layout's, and song select lists it under Single or Double.

**Listing, keys and storage**, as for the onigiri-free versions: the
shared reading (`ddi_library::scan::parse_simfile`) appends the
versions for every simfile (`.sm`, `.ssc`; DWI, FFR levels and works
have no pump charts), on both shells, in the chart editor's songs and
in the bundled songs' index (`xtask gen-songs` now reads through it;
three bundled songs have pump charts). Every onigiri-free version is
appended before every center-free one, so the indexes of versions made
before keep their places. The versions carry the `without-center`
digest section and their own assisted bests; replays recreate them by
index and digest; ghosts use their assisted best. The manifests'
`VARIANTS_VERSION` is 2: browser imports with pump charts listed before
gain the versions from their stored chart when the library loads, with
no reimport. The desktop library cache advances to version 11.

Verification: native tests for the three mappings, center removal
(holds, mines, lifts, fakes), the empty result, idempotence, the order
after onigiri versions, preserved original indexes and digests, the
settings' default for old settings, the list's filter and best key,
ticked assisted plays and their replays and bests
(`crates/ui/src/state/history/tests.rs`), the desktop library and its
play (`crates/desktop/src/library/tests.rs`), and an offscreen desktop
play of Beatucada's pump-single version autoplayed all top tier with
AUTOPLAY · ASSIST on its results (`crates/desktop/tests/center_free_offscreen.rs`).
In headless Chrome (WebGPU) and Firefox (WebGL2) a synthetic pump pack
imports (the center-only chart without a version), its versions are
listed with the option on and not with it off, and autoplay all top tier
with AUTOPLAY · ASSIST and every hold tick hit, no console errors; a
record stripped of its versions at variants version 1 regains them on
reload. The local autoplay corpus on a local Pump It Up pack (18
simfiles, 216 charts with their versions) plays all top tier under every
preset.

## Where missing, and the mark in song select (2026-10-25)

The maintainer's request, on seeing the bundled songs gain 16 charts:
a generated version's assisted mark was only on the results, and a
version beside an original of the same difficulty adds little. Without
center now has three values (`CenterVersions`, the maintainer's choice
over always hiding them):

| Value | Song select lists |
| --- | --- |
| Off | none |
| Where missing (default) | a version only where its style has no original chart of its difficulty; Edits (each its own chart) always |
| All | every version, as before |

Stored settings from before keep their meaning: `true` (the old default)
reads as Where missing, `false` as Off. Generation, indexes, digests,
bests and replays are unchanged: hiding is the song list's
(`ddi_ui::state::select_view`), so a link or replay to a hidden version
still plays it. Without onigiri stays On/Off: a Dancing☆Onigiri work's
charts are all Edits in layouts of their own (`danoni-5`, …), so Where
missing would never hide one. With the bundled songs, Where missing
leaves 3 of the 16 versions, all on Double (Beatucada's Easy 6, Some
Things Must's Medium 11 and Hard 17).

**The mark.** Song select marks a generated version subtly where a chart
is picked (`ChartEntry::version`, `Derivation::name`): the ladder's muted
line under it begins with what it leaves out ("Without center · 412
notes · steps by …"; a named version says it in its name), a phone's
chart button has a small muted dot in its corner (`ChartButton`'s
`Assist`, commands `Assisted`/`Original`), and its Play label ends in
"· ASSIST", as the results mark the play. The wheel's meter cells show
the first chart of each difficulty, an original before a version, and
stay unmarked. The plays list's title names the version.

## Pump single and 5 keys (2026-10-25)

The maintainer's request: beside the simplifications, optional
conversions between pump single and Dancing☆Onigiri's 5 keys, which
have as many lanes, marked in the song list as not original. Options →
Notes → Convert:

| Row (`Settings`) | Adds | Default |
| --- | --- | --- |
| Pump on 5 keys (`convert_pump_single`) | every pump single chart on 5 keys (`Derivation::FromPumpSingle`) | On |
| 5 keys on Pump (`convert_five_keys`) | every 5-key chart on pump single (`Derivation::FromFiveKeys`) | On |

**Lanes.** The corners are turned as Without center turns them, the
center put on the onigiri:

| Pump single | DL | UL | C | UR | DR |
| --- | --- | --- | --- | --- | --- |
| 5 keys | Left | Up | onigiri (Space) | Right | Down |

5 keys onto pump single is the reverse. So the conversions agree with the
simplifications: a pump chart's 5-key version without its onigiri is its
version without the center, and a 5-key chart's pump version without its
center its version without the onigiri. Every note moves, of every kind,
with its row and attributes; timing, meter, difficulty, name (followed
by "· from Pump single" or "· from 5 keys", else "From Pump single",
"From 5 keys"), credit and chart-scoped effects (`Song::effects_chart`)
are kept. Only pump single and 5 keys: half-double's six panels and
double's ten (two centers) have no key mode of as many lanes with as
many centers. A generated version is never converted again.

**Rules: the original's.** As for the simplifications, the version keeps
its source's game. A pump chart on 5 keys ticks its holds under "by the
chart's game" (`Derivation::pump_game`); a 5-key chart on pump single
does not (`Chart::pump_game` reads a version's provenance before its
layout), and a Dancing☆Onigiri work's keeps its `Chart::danoni`
headers, so "each chart's own game's rules" plays it by
Dancing☆Onigiri's. 5 keys was written for fingers, the onigiri a thumb's
key pressed with the arrows, so many such charts are hard or impossible
on a pad's feet; the meter stays its source's scale.

**Not assisted, but marked.** No note is left out, so a conversion's
plays are not assisted (`Derivation::assisted`: the simplifications
only; the engine's `chart_assist`, the song list's best key and the
ghost's read it). Its digest is its own (sections `from-pump-single`,
`from-five-keys`), so its bests and kept plays are its own too. The
song list marks it as a version: the ladder's line begins "From Pump
single ·", a phone's chart button has the same dot (command
`Converted`), and its Play label has no "· ASSIST", as its results
have none. Drawn, bound and listed as the target layout: under 5 keys
(arrows and Space, the 5-key skin) or Pump single.

**Order and storage.** The conversions are appended after every
onigiri-free and center-free version, so earlier indexes hold. Imported
manifests' variants version 3 and desktop's library cache 12 bring them
to songs stored before.

## At once (2026-10-26)

The maintainer's request: a jump is fine, but not more than two notes at
once, for charts written for a keyboard or for a hand beside both feet.
Options → Notes → **At once** (`TransformOptions::at_once`) replaces No
jumps with three values:

| Value | Keeps | StepMania |
| --- | --- | --- |
| Any (default) | every note | |
| Up to two | jumps; a third panel and more at once go | NoHands, `RemoveSimultaneousNotes(2)` |
| One | one panel at a time | NoJumps, `RemoveSimultaneousNotes(1)` |

The rule is StepMania's (`docs/research/rules-ddr-itg-dwi.md` §4): on
each row, its taps, lifts and hold heads and the lanes an earlier hold
still holds (one ending on that row included) count; taps and heads go
from the leftmost lane until the limit is met. Lifts count but stay,
mines and fakes are left alone, and a removed hold no longer holds the
rows after it.

Unlike the versions above, it is a play option, as the other cuts are
(the maintainer's choice): it applies to whatever chart is played, a
generated version included (a 5-key chart without its onigiri, or on
pump single), lists nothing more in song select, and marks the play
assisted on its results and among the bests. Settings and kept plays
from before store `no_jumps`, which still reads as One; replays judge
again as played.
