# Layered Peters-DDR mines

Faithful raster reconstruction authored in SkinCraft, using the original eight-frame Peters-DDR Down mine texture as the material and electricity source.

## Source and layers

The gray metallic arrow stays in the same place in the eight original frames. Native Darken stacking recovers its bevel without the moving white bolts; the original note silhouette masks the core. No source pixel is white in all eight frames. Rotated copies supply the cardinal and diagonal variants. The square retains the approved center silhouette and its three inset regions, reconstructed with the original gray-blue palette. The inset bevels use a narrower, softer cut (7 source pixels, depth 70, soften 1) to avoid heavier dark edges than the arrows. Native Levels lifts the flattened outer face’s deepest shadows (output black 24), retaining the light metallic faces.

`refined/` contains the final core masters. Their cyan halos come from expanded silhouette masks and Gaussian blur, with enough padding to taper to zero alpha at the image boundaries. These replace the first halo experiment at this directory’s root. `refined/preview-2x/` contains the nine runtime cores.

The actual white bolt silhouettes are selected from the eight original frames. Each gets a soft radial alpha falloff and the original cyan color, then is assembled natively into `_mine electricity 4x2 (res 256x128).png`; `preview-2x/` contains the runtime sheet. The effect loops in 0.5 seconds, matching the original Sprite.LinearFrames animation. All pixel construction, masking, filtering, atlas assembly and the comparison board use native SkinCraft commands.

Each mine actor contains three separate sprites: `MineCore`, complete `LocalElectricity`, and `MineLink`. The last is reserved by `[NoteDisplay] MineConnectRows=1` for stretches between actual drawn mine centers. Connections are between visible mines at the same chart beat and physical y row, at most one lane width apart. Empty lanes break the effect; lane names, indices and ordering do not define adjacency. See `docs/plans/noteskin-authoring.md` for the renderer contract. Single mines retain their complete local effect.

## Preview and checks

The current runtime is `target/scratch/peters/mines-adjacent-ready/NoteSkins/`, opened by the `studio-mines` SkinCraft profile. The earlier `studio-visible` profile and all its reference checkpoints are preserved. Wider demo phrases show an isolated center mine, a spaced pair, and a full row after the holds and rolls.

`validation.json` records zero alpha on the boundaries of all nine core images and every electricity frame, preventing a rectangular cut-off. Renderer tests cover single mines, hidden mines, shocks, different beats/physical rows, restored actor visibility, stretch thickness, reordered lanes and reverse. Topology is checked on dance-single, dance-solo, dance-double, pump-single and danoni-9B.

`refined/comparison.png` compares the original with the layered reconstruction at the same core size. `runtime-wiring.json` preserves the applied Lua and metrics; the approved runtime is now copied into the bundled Peters-DDR skin.
