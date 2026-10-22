# Blue-purple rolls with horizontal tap cues

Raster draft assembled in SkinCraft’s native editor from the approved Peters-DDR hold silhouettes. The user-supplied screenshot guides the blue-purple palette and straight horizontal bars; its pixels are not bundled.

- Nine lane silhouettes retain the existing gray outlines. Head colored areas receive the blue-purple material, with gray charge cavities.
- Body and cap interiors replace the hold’s repeated note motifs with horizontal bars: 16 logical pixels apart, 5 pixels thick, inset 4 pixels from the colored field’s sides.
- The steady gradient cycles blue → purple → blue over the 128-pixel body tile. Caps start at the same blue phase and progress toward purple at their tips.
- Four atlas frames change only the bars: gray → light gray → white → light gray. The repeat uses 75% of the active ruleset’s scaled roll window, shared across lanes and caps and independent of BPM. At default timing settings it is 262.5 ms for ITG, 375 ms for SM5 and 187.5 ms for DDR A. The remaining 25% provides tapping headroom before the drop deadline.
- The approved hold artwork is unchanged. The opt-in `RollAnimationTimingWindowRatio=0.75` applies only to roll bodies and caps. Hold animation keeps its original metrics.

Layered `.pcraft` masters preserve the original silhouettes, independent gradient fields and bar layers. Native copy/paste assembles the four-frame PNG atlases. `geometry.json` records the dimensions and animation choices. The runtime copy is under `target/scratch/peters/roll-charged-heads-ready/NoteSkins/`.

See the adjacent SkinCraft feedback for the remaining file-registration and timing-inspection limitations.

`preview-4x/` contains native nearest-neighbor half-size copies for the runtime: the full 8× atlases plus the existing skin exceed the loader’s 64-megapixel decoded-image budget. Logical dimensions and atlas frame counts are preserved.

The preview copy removes the original Down Roll PNGs/redirs, which otherwise compete with the new Lua actor files. Native export preserves the applied script drafts. `runtime-wiring.json` retains the final metrics and Lua text alongside the artwork.

Latest cap refinement: `caps-clean/` preserves the layered cap masters and full-size atlases with the final clipped stripe (logical y ≥ 57) removed. Its `preview-4x/` replaces only the cap PNGs in the current preview. The tip’s gradient, outline, alpha silhouette, all earlier bars and the ruleset-based timing are unchanged; `caps-clean/validation.json` records pixel-difference bounds.

Latest head/outline refinement: `charged-heads/` supplies all nine four-frame head atlases and repaired Up/UpLeft/UpRight caps. Head charge masks are extracted from the original note’s three white islands, ordered stub → middle → main arrow (center: inner → middle → outer). Their normal one-beat gray/white charge leaves the blue-purple field unchanged. The bar pulse remains based on 75% of the active roll timing window. The upper caps’ gradient is masked to an inset of the original outer silhouette, restoring the two-pixel gray outline at inward corners without restoring the removed terminal stripe. Hold heads remain static. Pixel validation is saved alongside the layered masters.
