# SkinCraft raster authoring feedback

Observed while revising the Peters-DDR square center (2026-10-22).

- The connected MCP advertises `editor_request.params` as a string, while the
  current application expects a JSON object. String arguments fail with
  `missing command`; raster `document.inspect` also reports an unknown method.
  Workaround: use the current binary's stdio MCP, with structured arguments,
  and invoke `document.inspect` through `engine.execute`.
- There is no model-to-raster command for a noteskin element. Peters-DDR's
  tap arrow is a MilkShape mesh with a small material atlas, so opening its
  PNG does not give an editable arrow silhouette. Workaround: bake the original
  mesh and bitmap texture to a temporary reference PNG outside SkinCraft,
  then open it in SkinCraft. Geometry, UVs and source pixels were reused;
  this step did not draw a new arrow. A native bake at a chosen beat, color,
  resolution and rotation would make this workflow accessible in the editor.
- `engine.commands` exposes file/new/export operations which SkinCraft's
  bridge subsequently blocks. A new document can be made by duplicating an
  existing layer into a new document, clearing it, then changing canvas size.
  Expose a supported create-artwork command and report unavailable commands
  with a useful reason.
- The bridge can save an independent raster copy but has no operation for
  registering it as a new named noteskin resource. `write_text` likewise only
  accepts existing project resources. Adding a new center image or Lua file
  currently requires an external file operation before reloading the skin.
- A long-running preview reported a Lua deadline failure while artwork was
  being opened. Editing continued and the preview was paused; a clear retry
  action and separation between artwork-editing cost and Lua load deadlines
  would help diagnosis.

The raster construction itself uses native selections, copy/paste, transforms,
layer visibility and masks. Corners and edge strips come from a 45° rotated
arrow; charge and wall pixels are sampled from that arrow. Editable `.pcraft`
files preserve the source-derived layers.

Further observed issues:

- Combining nested subpaths with `fillRule: evenodd` and `op: combine` did
  not leave the expected ring hole in the selection. Explicitly selecting
  and clearing each inner polygon produced the intended ring. The interaction
  between subpath boolean operations and fill rules needs clearer documentation
  or a direct ring-selection operation.
- The first `type.create` call blocked the UI while system fonts were scanned.
  A two-second macOS sample showed `TextEngine` initialization,
  `FontDb::load_system_fonts`, fontique scanning and repeated font registration
  on the main thread. Workaround: restart with the supported
  `PHOTOCRAFT_NO_SYSTEM_FONTS=1` and use bundled Inter for review labels.
- Restoring the saved multi-document session then aborted in
  `photocraft-ui-egui/src/chrome_ui.rs:157`: views length 8, active index 8.
  Workaround: preserve the original project home, launch a fresh isolated
  home, and open the exported masters. Restored reference documents need
  matching initialized UI views before the first status-bar draw.

Live-preview wiring (selected outer-width-2 draft):

- Native copy/paste, transforms and hue adjustments successfully produced a
  subdivision-color sprite atlas and the two-state receptor atlas. Existing
  Lua and metrics were edited through `write_text` in a separate preview copy.
- The two newly exported PNGs still needed an external copy into that working
  noteskin before reloading it: `save_file` refuses writing inside a loaded
  skin, and the MCP has no add-resource operation. A supported create/register
  resource operation would remove this remaining file-system step.
- Window capture may time out while the development window is occluded, even
  though project edits and preview controls respond. The capture failure does
  not indicate that the preview itself is frozen.

- The apparent live-clock stall was confirmed as an offscreen-window case,
  rather than a raster-editor defect. A process sample showed eframe's
  `update_logic_only`; macOS reported the window outside the onscreen set.
  `Panel::show` advances the preview clock only when UI drawing runs.
  Exposing `running` and window/draw visibility in `inspect_workspace` would
  distinguish a hidden window from a paused or failed preview.

Visible preview recovery and Lua-budget investigation:

- Launching the development binary directly left its window undiscoverable by
  the app controller and outside the onscreen set. A temporary macOS `.app`
  wrapper launched through Launch Services made SkinCraft discoverable in the
  Dock/app controller. The visible preview uses `studio-visible` and the saved
  `outer-2-live-ready` skin, in By subdivision mode on `danoni-5`.
- `took too long` is a Lua deadline failure, distinct from fuel exhaustion.
  Both the pinned DDI dependency and current source reset frame fuel/time in
  `LuaSkin::begin_frame`, and build a new deadline per Lua call. A regression
  test now exercises 128 frames separated by simulated one-hour idle gaps,
  consuming more than one frame's fuel in total; it passes. This does not
  establish which operation caused the observed runtime timeout.
- A deadline failure is deliberately latched and replaces the skin with the
  built-in renderer until reloaded. SkinCraft currently labels that as a
  script error even for clock-only failures. Use `lua_stopped_on_clock()` to
  explain the interruption and offer an explicit reload/retry. Keep fuel and
  memory failures distinct; do not disable or blindly extend the sandbox.

Vivid raster follow-up:

- Original texture inspection found opaque gray (`#525252`) and white charge
  regions in Vivid, versus zero-alpha empty regions in Quantized. The Vivid
  atlas uses native texture-column extraction, stretching and clipping onto
  the existing structural mask; no external pixel editing is needed.
- MCP still lacks resource creation/registration. New variant-specific Lua
  placeholders and exported atlases must be registered through the filesystem
  before editing their contents through `write_text`.
- `open_file` reuses an existing document for the same source path even after
  exporting it under a different name. The Vivid working copy retained the
  Quantized master’s source identity, so reopening the Quantized path reused
  the modified Vivid document. A temporary filesystem copy was needed to open
  the original master independently. Native `file.close` is blocked by the
  SkinCraft host; expose a safe close/revert/duplicate-document operation.

Center hold follow-up:

- Body, head and cap pixels were produced with native raster operations.
  New PNG resources still required external registration before MCP script
  wiring. Native source identity after Save As still required independent
  working documents.
- The shared demo only put long notes at the first/last lane of wider
  layouts. It now exercises holds and rolls in every lane, including center
  and onigiri, with regression coverage. A local DDI patch build was needed
  to see that change in SkinCraft; no dependency pin was published.

Note-derived center/diagonal follow-up:

- Native rotation, color-range extraction, clipped source gradients and
  contiguous Magic Wand selection were sufficient for all raster edits.
  Read-only contour measurement selected the largest white charge component;
  it did not render or modify pixels externally.
- Newly exported resources and head-script placeholders still needed external
  registration in a preview copy before MCP could edit their contents. This
  is the same missing add-resource capability recorded above.
- The element inspector should explain when a lane candidate is discarded
  because taps/receptors cannot load. Merely adding lower diagonal hold files
  did not select them: viable DownLeft/DownRight tap/receptor redirects were
  needed before the shared loader retained those lanes' own hold artwork.

Full-length cap/stacking follow-up:

- Full-cap silhouette envelopes and phase-aligned repeats were built natively.
  Exact outer/inner lower contours, main white components and seam/end checks
  required read-only analysis of exported PNGs. A native alpha/color-mask
  contour inspection API would avoid this file readback during authoring.
- New exported resources still needed external registration in a preview copy;
  the loaded noteskin was then reloaded/exported through MCP. This remains the
  previously recorded missing add-resource operation, not external art editing.

## Roll bar study

- All pixel construction, gradients, bar layers and four-frame atlases were authored through native raster commands. New resource registration and Lua placeholders still required filesystem copies before MCP could edit them.
- Roll cue timing would benefit from a preview overlay showing the active ruleset’s roll timeout and the measured flash period. The current draft uses a half-beat pulse: 0.2 s at the 150 BPM preview tempo, inside ITG’s 0.35 s roll timeout. This tempo-relative cue is not yet a ruleset-aware tapping guide at arbitrary tempos.
- A longer raster session exceeded the 128 reference-document checkpoint limit. This blocked loading another skin even though every finished master was saved. Native `file.close` is not available through the host; closing saved references or explicitly starting a fresh reference session should be possible through MCP. Workaround: restart after preserving the reference checkpoint directory, and keep the saved PNG/pcraft masters.
- The full 8× roll atlases pushed the complete skin over the loader’s 64-megapixel decoded-image budget, reported as several textures that “could not be read.” A decoded-image usage meter and explicit budget warning would distinguish this from missing files. Native half-size copies solve it without raising the limit.
- Resource replacement needs a candidate-conflict view: the legacy Down Roll PNGs/redirs competed with the new Lua actors. Removing those six legacy files in the isolated preview copy was another registration-only filesystem workaround. The original bundled skin is preserved.

### Roll cue timing follow-up

The shared renderer now supports `RollAnimationTimingWindowRatio`: the study uses 75% of the active scaled judging window, independent of BPM, without a Lua frame callback. The earlier tempo-relative cue limitation is resolved. SkinCraft was rebuilt with local DDI patches and its real ruleset selector exercises the new cadence; a timing/texture-budget readout is still useful authoring feedback.

The terminal-roll-stripe refinement was edited on the native bar layer in SkinCraft and the cap atlases were rebuilt with native commands. Registration again required copying those exported PNGs into a fresh runtime tree, because MCP protects the loaded source folders and cannot register replacements directly.

The upper-roll-ending outline fix and normal head charge were built with native masks, wand selections of the original charge islands, and native atlas assembly. Creating a layer mask consumes its selection; MCP reports deselect as unavailable afterward, so clients must respect the resulting command state. New atlas registration still uses an isolated runtime copy; no bundled art was edited.

## Layered mine reconstruction and restore crash

- SkinCraft crashed on reopening the 95-reference session: PhotoCraft’s status bar indexed view 95 while only 95 views existed. Restoring/opening documents added session tabs without synchronizing native views before chrome painted. Fixed in SkinCraft’s `src/references.rs` and `src/host.rs` by synchronizing after restore and immediately before raster UI. The existing reference-save/restore integration test now asserts view/document counts match; it passes. The complete earlier profile was reopened successfully before moving this study to `studio-mines`, preserving its checkpoints.
- The reference cap of 128 still makes a fresh profile useful for longer art sessions; MCP should offer closing saved reference tabs or a safe fresh-session action.
- Mine pixel work uses native Darken stacking, selections from the original white bolt frames, masks, bevels, Gaussian blur, rotation and native atlas assembly. Read-only file copies provide fresh reference identities. New Lua placeholders/PNG registration still require an isolated runtime filesystem copy because MCP cannot register new resources.
- A native comparison board makes source fidelity much easier to judge. The generic link extension and all source art remain independent of the editor; the preview uses DDI’s real renderer.

### Adjacent mine refinement

Reduced the center mine inset bevel width/depth with native PhotoCraft layer-style controls and regenerated its padded raster and comparison board in SkinCraft. Registering that exported PNG in a fresh preview folder still required a filesystem copy, because the current MCP cannot replace an image resource in the loaded skin directly. Mine links now stop at empty lanes; the spaced pair in the demo is a visual regression check.
