# Noteskin authoring in PhotoCraft

Native implementation, 2026-10-19. Run `tools/skin-studio/run.sh` with the
sibling `../photocraft` checkout on `codex/noteskin-preview`.

## Implementation status

The project workspace now includes a full fallback-chain file browser with
thumbnails, search and element filters, multiple independently layered
PhotoCraft documents, checkpointed closed drawings, and native source tabs
for Lua, INI, SVG and other text resources. Text drafts reload after a typing
pause through private candidate trees. Load errors keep the last working
preview; animation-time Lua failures roll back the actor tree while keeping
the draft editable. Saving a project checkpoints layers and text drafts;
export validates the generated runtime tree. Each source skin has a separate
workspace under the isolated studio profile.

The element inspector queries the actual built actors' texture indices,
including shared and Lua-redirected images and colour variants. Related
scripts remain accessible by filename: there is no execution-level dependency
trace for every dynamically loaded Lua module. File closing preserves undo
within the live session; restart restores layers and pixels.

`ddi_desktop::play::skins::prepare_authoring` uses the normal loader without
recording gameplay's persistent Lua strikes and exposes load errors to the
workspace. `NoteskinPlay::element_texture_keys` is read-only and platform
independent. Neither changes gameplay rules or rendering behaviour.

`tools/skin-studio/` is a separate Cargo workspace using PhotoCraft's live
artwork snapshots, the game's native noteskin loader and
`ddi_render::field_preview::FieldPreview`. The generic PhotoCraft panel
hook is in the sibling checkout; its optional desktop feature links this
tool. The launcher isolates preferences and recovery data, and remembers
layered documents, text drafts and their source bindings. The installed app is
unchanged. Usage and current limits: `tools/skin-studio/README.md`.

Implemented: editing an existing texture, updates during an unfinished
stroke, undo/redo, layout selection, looping playback, pause and scrubbing,
reverse and speed, backdrop brightness, the lane mapping and loader's
messages, PNG export into a new skin tree, and a generated onigiri child
skin preserving its inherited arrows and hold bodies. The child supplies
editable tap and receptor art, the onigiri heads, and its explosion.

Native checks cover creation/loading of the onigiri extension, live
pixels reaching the playfield with undo and export through the normal
loader, preservation of SVG display sizes after PNG export, and the actual
PhotoCraft panel hook over Alpha-Arrows with the onigiri extension.
Offscreen captures are generated under
`target/scratch/skin-studio/`. The small synthetic edit measured 0.8 ms;
larger composites cost more and the panel displays their latest update
time. This is not a general latency guarantee.

Remaining from the design below: sprite-sheet grid guides, separate
enlargement of a part inside the preview, explicit lift/fake demo cases,
and a full per-element coverage inventory. The existing drawing canvas
provides enlargement now. Export rewrites literal filename references
after rasterizing edited SVGs; arbitrary constructed references may need
manual changes. The following sections retain the implementation design.

Draw noteskin artwork in PhotoCraft and see the changes in an embedded
playfield, through Dito Dito Infinito's gameplay renderer. The first uses
are adding onigiri artwork to bundled skins and developing a more detailed
Dito skin. The maintainer prefers the preview inside PhotoCraft over a
separate window.

## PhotoCraft integration

The installed PhotoCraft reports version 0.10.0. The upstream source
examined is `storytold/photocraft` at
`f3fb5781ef432aaeff42656865f9dcc9c94d17b3` (also version 0.10.0).
Its [plugin contract](https://github.com/storytold/photocraft/blob/f3fb5781ef432aaeff42656865f9dcc9c94d17b3/docs/plugins.md)
supports sandboxed pixel filters only. It explicitly excludes panel and UI
plugins, and provides no filesystem, network or clock imports. An installed
filter cannot host an animated game preview.

The implementation is a separate PhotoCraft development build with
a native Noteskin Preview panel, accepted by the maintainer. Leave the
installed application intact. Isolate
the development build's preferences, recovery files and single-instance
identity as well; a different executable path alone does not isolate them.

PhotoCraft uses egui/eframe 0.36 with wgpu 30; the game uses wgpu 30 too.
The panel shares PhotoCraft's GPU device, draws with
`ddi_render::field_preview::FieldPreview`, and registers the result as an
egui image. Native offscreen checks exercise the shared device and texture.

Keep the PhotoCraft-specific adapter under `tools/`, outside the game's
normal build, and its generic panel hook in the sibling PhotoCraft branch.
No game window or audio
device is needed for this tool. PhotoCraft's source is MIT OR Apache-2.0;
retain notices for any source distributed with the integration.

## Drawing workflow

Open a working copy of a skin, choose the part to draw, and bind an open
PhotoCraft document to its texture. The drawing canvas remains PhotoCraft's
normal canvas, with its brushes, layers, masks, effects and undo. The panel
shows the resulting notes moving through the receptors as the art changes.

Keep a binding for each document rather than assigning whichever document
happens to be active. Bind the exact texture path and fallback folder, so
two files with the same name remain distinguishable. A whole sprite sheet
is one document, with a frame grid overlay for drawing. Display dimensions
and sprite-sheet hints retain their StepMania meaning.

The panel offers:

- Skin and element selection, including arrows, diagonals, centre and
  onigiri; taps, receptors, hold and roll parts, mines and explosions.
- A looping playfield and an enlarged inspection of the selected part.
- Layout selection, starting with dance-single and Dancing☆Onigiri 7 keys,
  so the onigiri is visible alongside the arrows; other built-in layouts
  use the same loader.
- Pause, restart, time scrubbing, note size, scroll speed and reverse.
- Dark and light backgrounds for checking translucent edges and contrast.
- A coverage report identifying dedicated artwork, named fallback artwork,
  and procedural or missing parts, with the resolved source path.

Use the game's existing demo for taps, judgements, missed notes, kept and
dropped holds and rolls, and mines. Lifts and fakes are a known gap in that
demo (`skin-preview.md`); include explicit cases before calling the
authoring coverage complete.

## Live artwork

Feed artwork from PhotoCraft's open documents, without requiring a save or
PNG export on each change. Compose the document's layers with PhotoCraft's
existing compositor, retaining straight alpha and converting through its
colour management to the sRGB texture values the game loads. Do not capture
the visible editor canvas: its checkerboard, selection, zoom, monitor
transform and drawing overlays are not noteskin artwork.

Read the displayed document snapshot, including transient paint and
adjustment previews. PhotoCraft's `canvas::display_doc` supplies this within
its UI crate. Its live stroke has a separate preview key: the committed
document's revision alone would update only after the stroke ends. A
generic host hook must expose both the snapshot and its change key. Verify
live strokes, undo, redo, transforms, layer visibility and adjustment drags.

Composite only changed source documents and coalesce work so the newest
snapshot replaces queued older ones. Keep the playfield animating between
art updates. Bound document dimensions and total pixels before allocating;
cap update work so painting remains responsive. Measure edit-to-preview
latency on the resulting build before promising a specific frame rate.

The skin's texts, models, fallback chain and image sizes still go through
the game's noteskin loader. An in-memory image overlay replaces textures
bound to PhotoCraft documents. Normal pixel changes update the corresponding
GPU texture and its mipmaps without resetting the demo. A size, filename,
grid, Lua or metrics change can require rebuilding the actors. Retain the
last valid preview and display a loading error while an edit is incomplete.

Keep the live override keyed to its source identity rather than its current
renderer texture number; rebuilding the skin can renumber textures. Release
textures when documents or projects close and when the device is lost.

## Onigiri and bundled skins

Preserve the game's lane mapping: `Onigiri`, then `Center`, then unrotated
`Down`, within the chosen skin's fallback chain (`danoni-skins.md`). Show
which choice actually loaded. A centre square or down arrow should not be
reported as dedicated onigiri artwork.

Adding an onigiri needs the appropriate actors and loading commands as well
as an image. Several skins supply their own Lua button redirects; merely
adding `Onigiri Tap Note.png` does not guarantee their Lua will use it.
Generate or edit an explicit adapter for the skin's own loading conventions
and check taps, receptors, hold/roll heads and explosions on the onigiri
lane. Keep pump's centre mapping independent.

Develop bundled-skin additions as separate working copies. Preserve upstream
assets byte for byte. On acceptance, record new art, loading changes,
licences and any conversions in the bundled skin's provenance. Dito's SVGs
are generated by `xtask gen-dito`; choose and document the new source of
truth before adopting hand-drawn replacements, so regeneration cannot
silently overwrite them. A more detailed Dito design is a later art task;
this tool supplies the means to draw and evaluate it.

## Saving and export

Keep layered PhotoCraft or PSD sources separate from runtime textures. Saving
the drawing project retains the bindings and layered source locations;
exporting writes game-readable PNGs, the skin's texts and any generated
adapter into an explicitly chosen working folder. Keep the current valid
skin until a complete export succeeds. An export must round-trip through
the game's normal loader with no in-memory overrides.

## Implementation order and acceptance

1. Embed the existing unmodified demo renderer in PhotoCraft on a synthetic
   skin. Verify shared-device texture display, blending, sizing and teardown
   offscreen before connecting editable documents.
2. Bind one open document to one texture and show changes during a stroke,
   then undo and redo. Keep the demo running through texture updates. Compare
   a paused preview with the same art exported and loaded in the game.
3. Add project bindings, element inspection and the coverage report, including
   explicit onigiri versus centre/down fallback. Exercise a skin whose Lua
   redirects only dance buttons.
4. Add project save/export and round-trip checks. Check colour profiles,
   transparency, sprite sheets, invalid dimensions, out-of-order updates,
   missing files, device loss and a failed export with synthetic fixtures.
5. Exercise the workflow by making an onigiri extension in a scratch skin.
   Confirm all its parts through the game loader and renderer, then use the
   tool for the bundled artwork changes as separate work.

Runtime checks isolate the game's `DDI_HOME` and PhotoCraft's application
data. Prefer offscreen rendering over a window that steals focus. Changes
to the game renderer or loader must preserve native and wasm compilation;
the authoring adapter itself may initially be native only.


## SkinCraft: Dito SVG proof

The native `skincraft` binary in `tools/skin-studio` embeds the UI libraries
of sibling PhotoCraft and VectorCraft checkouts in one eframe event loop.
The workspace owns the file browser, cross-editor tabs, project persistence,
text editor and game preview. File type selects the native editor: SVGs use
VectorCraft, raster images use PhotoCraft. Neither installed app is modified.
The old optional PhotoCraft extension remains a separate launch target.

Vector revisions, including in-progress anchor interactions, export clean SVG
with presentation attributes. Strip VectorCraft's temporary mask-editing
layer from the exported view of the document: the engine already synchronizes
its geometry into the real opacity mask after each edit. The game's own SVG checker and decoder gate
publication. Supported changes replace GPU textures and mipmaps directly,
without restarting the demo; dimension changes schedule an actor reload.
Invalid vector drafts remain editable in the native model while the last
accepted SVG and preview remain intact. Runtime export is blocked on errors.

Native `.vectorcraft` snapshots and PhotoCraft layered sources are staged
before a shared atomic project-manifest commit. Runtime SVGs carry no editor
metadata and are preserved as SVG in exported skins. Explicit source editing
releases the vector override to avoid two concurrent sources of truth.
Session undo survives tab close/reopen; disk checkpoints retain editable
geometry and layers, rather than the engines' undo stacks.

The proof verifies every bundled Dito SVG's dimensions and raster appearance,
live anchor previews/cancel/undo, mixed editor routing, rejected effects,
SVG export and native project restoration. The combined host now injects native asynchronous file dialogs and file codecs
into both editors. Shared Open routes by resource type, and native file writes
protect source folders. Independent reference documents are checkpointed with
the window so inactive unsaved files survive closing it. Preferences stay in
the isolated profile. One authenticated loopback endpoint serves the stdio MCP
facade (`skincraft mcp`, `mcp.sh`): native commands/pointer input, source drafts,
project saves, copies, runtime export and whole-window captures. A real MCP
client and native dialog continuations are covered by offscreen tests. Dito's generated
bundled sources remain untouched.
