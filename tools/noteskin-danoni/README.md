# DanOni noteskin proof

`assets/noteskins/dance/DanOni` re-creates the **default CW Edition 5/7-key
appearance**, including Lua actors for its behavior. Choose **DanOni** under
Options → Arrows → Noteskin, or open that folder in SkinCraft. The proof does
not change Dito or the default noteskin. Its natural comparison size is a
50-pixel lane: upstream's 50 CSS pixels correspond to our 64 logical units.

Pinned source: [cwtickle/danoniplus at 39e8ae7](https://github.com/cwtickle/danoniplus/tree/39e8ae70249a845a69bf24249157b1735b331886).
The upstream MIT notice and four original SVGs are kept byte-identical under
`upstream/`. Reproduce their static-subset adaptations with:

```sh
python3 tools/noteskin-danoni/generate.py
python3 tools/noteskin-danoni/generate.py --check
cargo run -p xtask -- gen-noteskins
cargo run -p xtask -- gen-credits
```

The source geometry is retained exactly. Upstream uses the SVGs as **alpha
masks** over colored HTML elements; their black pixels therefore become white
in our tintable SVGs. Class styles become presentation attributes, required by
the game's SVG subset. The source's 500-unit paths, polygon points, stroke
widths, joins and miter limits are unchanged. `_bar active.svg` and
`_bar inactive.svg` reproduce its 40/50-wide rectangular freeze bar, including
its colors and .75 opacity. `_bar dropped.svg` is opaque gray, as upstream's
failed freeze. The host draws strips with field-owned colors.
Tap colors live on child sprites so the field's root tint cannot erase them.

Behavior references at that pin:

| Source | Lua reproduction |
| --- | --- |
| `js/lib/danoni_constants.js`: `g_dfColorObj`, `color5_0_0`, `color7_0_0`, `stepRtn*` | Blue cardinal arrows, cyan diagonals, white onigiri; static colors and source rotations |
| `css/danoni_main.css`: `main_stepDefault`, `main_stepKeyDown` and judgment classes | Gray receptors, cyan pressed overlay, corresponding judgment colors |
| `js/lib/mainWindow.js`: `makeStepZone`, `mainKeyUpActFunc` | Press/release toggles the overlay immediately, without scale, fade or beat pulse |
| `stepHitTargetArrow`, hit-counter update; `C_FRM_HITMOTION=4` | 80/50-size outline at the note's prior drawn position and alpha .75; a 4/60-second hold then abrupt disappearance; retrigger restarts the timer |
| `judgeMotionFunc.arrowON` | Centered gray `#999999` outline at alpha 1 for autoplay; no cyan key-down overlay |
| `makeFrzSprites`, `changeHitFrz` | Black silhouettes, cyan freeze ends and purple .75-alpha body; yellow ends and olive body while held; white 66/50 arrow silhouette (or yellow onigiri) at alpha .9 over the receptor |
| `judgeMotionFunc.frzOK`, `changeFailedFrz` | Holding/roll off hides the held overlay; no completion burst. Each dropped freeze gets outline `#cccccc`, fill `#333333`, opaque bar `#999999` |

With DanOni rules, W1/W2/W3/W4 map to Ii/Shakin/Matari/Shobon. For ITG's
five slots, W1 and W2 map to Ii, W3 to Shakin, W4 to Matari, W5 to Shobon. Misses
produce no flash. Lua uses elapsed song seconds to preserve the reference
60-Hz duration across display refresh rates. Bright/combo does not enlarge
or recolor the flash. No Lua calls reach outside the noteskin sandbox.

## Host interface

`HoldUseDroppedState=1` opts into `Hold/Roll Head`, `Tail`, `Topcap`, `Body`
and `Bottomcap` actors suffixed `Dropped`, alongside `Inactive` and `Active`.
Their selection uses each note's actual dropped flag, independently of every
other note in that column. Dropped artwork is drawn without the host's life
dimming. DanOni also sets `HoldLetGoGrayPercent=1` so the cyan/yellow state
stays exact until failure. Active art remains active during release grace;
the drop changes it to gray. Other skins retain their two-state behavior.

The tap's `ColumnJudgmentMessage` includes `Autoplay`, `HitX` and `HitY`.
`DanoniJudgments` identifies the four-tier ruleset independently of layout,
so its Shakin and Matari do not get ITG's different-slot colors.
`TapJudgmentsPerNote=1` selects each note's immediate hit event instead of
StepMania's completed-row event, so chord notes retain their own hit time,
position and judgment. Other skins still receive the original row effects.
Coordinates are in 64-unit lane pixels, relative to the receptor in the
explosion root's base orientation; reverse and lower-row scrolling are
included. The host matches a stable transformed-note identity to its last
drawn position, rather than using the nearest note in the lane. When that
note was not drawn, the fallback is its position at the input's visual time,
including scroll modifiers, per-note speed and visual calibration. Missing
position data remains compatible with older skins and manual test commands.
`Press` and `Lift` also include the autoplay flag. This metadata never changes
judging, scoring, input timestamps, or recorded replay edges.

SkinCraft's **Autoplay flashes** checkbox (also `set_preview`'s
`autoplay_flashes` MCP argument) compares the two presentations using the
same timing examples. Choose **Rules → DanOni** (MCP `ruleset: "danoni"`)
for the native four-tier judgment palette. The modes survive preview looping.

## Proof limits

This is a reproducible baseline, not every configurable DanOni work:

- Chart color timelines, custom shadows/gradients, custom JavaScript,
  directional changes and alternate image/color sets are not reproduced.
  The default 9A/9B palettes need column identity (two lanes can have the
  same arrow button and different colors); this proof targets 5/7 keys.
- Rolls use freeze art; lifts/fakes use taps. Mines use a red onigiri.
  These are explicit extensions for StepMania charts, with no upstream
  equivalent. A pump center uses the down-arrow fallback.
- Field spacing, scrolling, judgment windows and the game's HUD are supplied
  by Dito Dito Infinito, outside a noteskin's remit.

The bundled-skin tests cover loading every element, the dedicated onigiri
and diagonal buttons, press/release, judgment palette and flash duration,
retriggering, and held-overlay transitions through the real Lua VM. Renderer
checks cover concurrent inactive/held/dropped freezes, actual flash-quad
positions for every arrow angle, both scroll directions and lower-row lanes,
and exact manual/autoplay colors and alpha. Native image checks compare
every source mask's alpha and the three freeze-bar colors/opacities.
