# Additional chart versions (2026-10-18)

Two simplifications offer generated versions of charts beside their
originals: **Without onigiri** (2026-10-18, below) and **Without center**
(2026-10-24, "Pump charts without the center"). Both are kinds of
`Chart::derivation` made by the same code (`crates/chart/src/simplify.rs`)
and listed, kept and replayed the same way.

## Without onigiri

Options → Notes → Simplify → **Without onigiri** exposes additional
versions of Dancing☆Onigiri charts. Off by default. Originals stay in
their original layouts and are unaffected by this setting. Existing
timing cut, No jumps and No holds options retain their behavior.

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
on top of it gives the other rotation.

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
