Latest cap color and diagonal head/body joins: [../joins/](../joins/README.md).

# Connected full-length hold caps and stacking direction

This revision calibrates the cap against the original Down hold before
applying the rule to Center and all four diagonals. Heads retain the colored
masks from `../derived/`. SkinCraft performs all pixel edits and exports;
read-only pixel analysis measures silhouettes, white-region components,
phase and endpoints. No external pixel rendering is used.

## Down reference and length

The original Down head's opaque bottom is source row 498. Its cap's opaque
bottom is row 482: the terminal note is shifted up 16 source pixels (2 logical
pixels) inside a 64px logical cap. Our reconstructed `DownReference` follows
that placement. Its white-mask intersection-over-union with the original cap
is 96.78%; small corner/rasterization differences remain.

All new caps use a 64px logical height and the same opaque endpoint, source
row 482 at 8× resolution (60.25 logical pixels). The preceding half-note caps
only extended about 24–26 logical pixels. A terminal note's vertical offset
is `482 - its opaque bottom`; this preserves its shape without stretching.

## Body/cap continuity

Both gray and colored lower silhouette envelopes extend to the cap's top.
The original green/yellow field fills the colored envelope, and repeated
white edge masks continue through the cap. Thus a terminal arrow does not
leave an unpatterned strip beside it, including the upper-left cap's right
side. The cap restarts the source color tile at its boundary phase.

The terminal-note placement determines each body's repeat phase. The cap's
stamp phase is one source pixel after the body's, matching the original
asset relationship. The body's last white-mask row and cap's first row match
exactly in all six generated cases; `validation.json` records the check.
The opaque endpoints also match exactly. The reference remains a study;
original cardinal assets are not replaced in the runtime export.

## Stacking modes

- Center/Onigiri, DownLeft and DownRight use bottom-up stacking: repeat a
  7px-deep band along the main white charge region's lower edge every 16px,
  clipped by the terminal note's lower silhouette. Smaller interior white
  islands are omitted at the cap, leaving the lower-edge motif, like Down's V.
- UpLeft and UpRight use top-down stacking: take the main white region's
  upper edge for the body and continuing repeats, then place the final note's
  full interior over earlier repeats. The full arrow interior remains visible
  at the end. Repeats outside that note's interior remain visible beside it.
  Both layers are clipped to the shared colored silhouette, preserving the
  gray outer border without translucent leftovers.

Each case has active/inactive PNGs and native `*-body.pcraft` / `*-cap.pcraft`
masters. The native documents retain field, silhouette, repeated-edge and
terminal-note layers. `geometry.json` records modes, phases and dimensions.
Inactive white markings become the original gray; the color field remains.

## Preview

`target/scratch/peters/stacking-holds-ready/NoteSkins/dance/Peters-DDR` is the
saved SkinCraft review export. The nine-lane Vivid preview keeps the larger
hold-to-roll gap and no mines. Roll heads still share hold heads, while roll
body/cap artwork remains the existing source artwork. These are visual study
assets, not yet installed in the bundled game noteskin.

Sources: Peter Lawrence's original Peters-DDR hold materials (Unlicense),
combined with this project's square study and native editing work (MIT),
recorded in REUSE.toml.
