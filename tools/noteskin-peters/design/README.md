# Raster square study

The latest [revised drafts](v2/README.md) compare the requested proportions,
remove the inner outline and derive matching receptors from the note regions.
The material below describes the first draft.

Editable SkinCraft/PhotoCraft construction for the Peters-DDR center note.
The comparison image shows the original tap arrow, its 45-degree rotation,
the square receptor and note, and cumulative charge states 1, 1+2 and 1+2+3.

At the original 64-pixel logical cell size:

- The square spans 48 pixels, matching the rotated arrow's approximately
  47.6-pixel bounds. Its hand-derived chamfer is about 5.75 pixels.
- The colored edge is about 4.2 pixels thick. The outer black note border
  and white receptor border are about 2 pixels thick. Corners and straight
  edge strips are copied from the rotated original art, with the straight
  strips extended along their length only.
- Two colored walls, each 3 pixels thick, divide the inside into a center
  and two rings. Their nominal areas at 8x source resolution are 10,332,
  10,816 and 10,860 pixels² (within 5% of each other). Antialiasing slightly
  changes the exact raster coverage. Charge uses the original arrow's
  light pixels and accumulates from inside outward.

`peters-square-raster.pcraft` retains separate outline, wall, uncharged and
charge layers. Toggle `Charge 1`, then `Charge 2`, then `Charge 3` to inspect
its progression. `peters-square-receptor.pcraft` retains the white borders.
`peters-square-comparison.pcraft` is the labeled review board.

These files are a raster design study. The runtime center remains a separate
draft while the geometry is being reviewed; animation atlases and the other
color mode have not been generated from this study.

Artwork editing used SkinCraft's native selection, copy/paste, transform,
mask and layer commands. An external mesh/texture bake supplied the tap-arrow
reference because SkinCraft cannot yet rasterize its model. Independent
copies exported by SkinCraft were copied here for review. Original bitmap
and model inputs are unmodified. See [SkinCraft feedback](skincraft-feedback.md)
for the authoring gaps encountered.

The copied Peters-DDR artwork is Peter Lawrence's, under the Unlicense.
New square composition, masks and review annotations are by すごいジャン,
under MIT; derivative study images and native documents carry both licenses.
