# Infinito

**Note colors** offers Vivid (default), Rainbow and Flat within this skin.
Vivid shifts the color palette's four-beat cycle by note timing. Flat shares
one cycling palette across all notes. Rainbow uses four timing-color families,
with a light cycle flowing once per beat inside each family. The choice also
colors lifts' charge areas; their charge sequence stays independent.

The default SVG noteskin, inspired by Peters-DDR behavior. Its gradient
pattern evolves smoothly with song seconds, independently of the synchronized
palette phase. Field settings are scale 0.021, warp 1.35, speed 0.3 and contour
density 2; the SVG silhouettes and charge regions stay fixed.
The SVG, WGSL, Lua and metrics in this folder are the editable skin sources.
Peters-DDR remains its loader/metrics fallback; Infinito supplies the taps,
lifts, fakes, receptors, holds, rolls, mines and hit effects for arrows, center and onigiri.

Infinito is the default under Options → Arrows. Saved SVG Dito / Spiral Base selections and controls migrate to Infinito.
The original procedural Dito and its
color-variant skins have been retired; procedural arrows remain an internal
fallback, without a picker entry or a Lighter control. They draw this skin's
shapes and palette in a light form (`docs/PLAN.md` decision 80).

Mines use a slowly spinning hollow spiked band with a reddish contour charge
that alternates between full and almost-full. Electricity and links stay fixed
in the field while the body turns. Enable
`Shock arrows` to restore the lane-shaped mine designs. The option is off
by default and changes artwork only; both styles have linked electricity.

A multiplayer battle's shocks are drawn by the skin's `Tap Shock` element
(`UseTapShock=1` in its metrics, `docs/plans/noteskin-authoring.md`): the
lane-shaped design, always. In a stage that may be given shocks
(`NOTESKIN:PlayHasShocks()`), a chart's own mines are the spiked band
whatever `Shock arrows` says, so an attack's shocks and the chart's mines
never look the same; a row's links join mines with mines and shocks with
shocks.

Taps/fakes and receptors now use `_spiral components.svg`, with continuous
holographic-flow paint in `_spiral material.wgsl` and load-only bindings in
`_spiral component.lua`. `[Global] Buttons` declares all ten supported buttons
without forwarding resources named for each button. Holds and rolls use `Def.LongNote`
with named head/body/cap roles and `_spiral wave.wgsl`: broad green/purple
waves evolve on elapsed seconds, independently of the palette cycle, with one
continuous field across all pieces. Roll pulse cadence is unchanged; lifts use `Def.Lift`
with a faded leading trail, a white/green band and continuous reversed-charge
holographic paint. Judgment effects use `_spiral explosions.svg`, per-hit bindings
and original artwork, preserving the selected flash gain and cadence. Mines
retain actors for adjacent-lane electricity. Experimental authoring tools and
retired sprite sheets are kept separately from the bundled skin.
