Latest cap length, join and stacking work: [../stacking/](../stacking/README.md).

# Note-derived center and diagonal holds

This raster study applies the approved down-arrow reconstruction to Center,
DownLeft, UpLeft, UpRight and DownRight. Onigiri uses Center. SkinCraft performs
all pixel edits, rotations, selections, texture stretching and exports.
Read-only Python analysis measures the source masks and records the contours
in `geometry.json`.

## Construction

- The center uses the chosen square mask. Diagonals use the original
  `Down Hold Head Active (res 64x64).png`, rotated 45/135/225/315 degrees
  within a padded 96px logical canvas, retaining the original pixel scale.
- The main outer white charge region supplies the repeated body motif.
  Its lower interior edge contributes a 7px vertical band, repeated every
  16px. Smaller charge islands stay in the head and cap; repeating them in
  the body produces stray fragments in diagonals.
- Body outer widths come from each note's silhouette, with 2px gray side
  edges. The cap uses the corresponding bottom half of the head.
- The original green/yellow column from `Down Hold Body Active` fills every
  colored region of heads, bodies and caps. It stays upright in the field
  rather than rotating with a diagonal head. Caps restart at the body tile's
  boundary phase. This supplies a common material; it is not a dynamic
  world-space texture projection across holds of arbitrary lengths.
- Active markings use the original off-white; inactive markings become
  original gray (`#787878`). Roll heads reuse hold heads as the source skin
  does. This study changes hold bodies/caps, retaining the existing roll body
  artwork.

Each lane has `*-head.pcraft`, `*-body.pcraft` and `*-cap.pcraft` editable
masters, plus active/inactive PNG exports. Headers keep their original colored
mask and a clipped source gradient layer. Body masters retain individual
repeated edge layers. Diagonal canvases are transparent outside the artwork.

## Runtime preview

The saved review export is
`target/scratch/peters/derived-holds-final/NoteSkins/dance/Peters-DDR`, loaded
in SkinCraft on `danoni-9B`, with Vivid colors. Its scripts use pre-oriented
heads without a second rotation and direct body/cap resources. Lower diagonal
lanes also have valid tap/receptor redirects so the loader retains their own
hold resources rather than discarding the lane candidate in favor of Down.
The 12-beat demo keeps rolls 2.5 beats after hold ends and omits mines.

Sources are Peter Lawrence's Peters-DDR hold textures (Unlicense), combined
with the square study and native editing work (MIT); REUSE.toml records both.
This is the current visual study, not an update to the bundled game assets.
