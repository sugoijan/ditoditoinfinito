# Additional charts without onigiri (2026-10-18)

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

`chart/src/simplify.rs` appends these versions after all original charts,
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
