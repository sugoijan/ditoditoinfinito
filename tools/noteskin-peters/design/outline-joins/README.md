# Continuous outline at diagonal head/body joins

This revision follows `../joins/`, retaining the flat yellow-green caps and
all existing body/cap phases and stacking modes.

The previous connector filled only the gap after the last opaque head pixel
above the body's start. That missed an inner notch between two separate
parts of the upper diagonal head. It also filled new exposed connector edges
with green without preserving a complete gray border on the lower diagonals.

SkinCraft now constructs the **union of head and body silhouettes**. For
columns that contain head pixels above the body center, the connector covers
all exposed intervals below the first such pixel, including inner notches.
A temporary full-height body extension ensures the union's inset does not
mistake the body start for an artificial end cap.

Native Magic Wand selects the union silhouette; a 16-source-pixel contraction
(the original 2px logical outline) determines its interior. Gray fills the
outer connector and the original upright green/yellow field fills that inset.
Both sit behind the untouched original head. The visible connector overlaps
the body by 2 logical pixels so new exposed border segments remain complete.
The temporary extension is trimmed after computing the inset.

`*-head.pcraft` files retain the original head, union outline, contracted
interior mask and clipped color field. Active/inactive PNGs are the exports.
Read-only source-image analysis records the holes in `geometry.json`;
`validation.json` confirms zero remaining gaps and checks the newly exposed
lower-diagonal top border is original gray.

The saved SkinCraft review export is
`target/scratch/peters/outline-joins-ready/NoteSkins/dance/Peters-DDR`.
Roll heads reuse the same corrected images. All pixel operations used native
SkinCraft commands; external work only measured pixels and registered exports
in the separate preview copy, reflecting the MCP's existing limitation.

Sources: Peter Lawrence's Peters-DDR materials (Unlicense), plus the project's
square study/native editing work (MIT), recorded in REUSE.toml.
