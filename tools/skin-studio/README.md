# SkinCraft

Edit a whole noteskin in one native window: VectorCraft for SVGs,
PhotoCraft for bitmaps, an embedded text editor for scripts and metrics,
and the game's renderer for immediate feedback. This first combined build
opens Dito's receptor SVG by default.

## Run

```sh
tools/skin-studio/run.sh
```

It builds the `skincraft` binary from this standalone workspace. Both
`../photocraft` and `../vectorcraft` sibling checkouts are required. The
PhotoCraft checkout needs this development branch's generic panel hook.
VectorCraft is unmodified, pinned for this proof at
`1f80c2866222f1bd3f736e9a5730df26f55416dd` (0.13.1).

The launcher keeps project checkpoints under `.local/skincraft/`, independent
of the installed apps and surviving `cargo clean`. `DDI_STUDIO_HOME` selects
another project home, including an earlier `.local/skin-studio` workspace.
`run-photocraft.sh` retains the older PhotoCraft-only development host.

**Open file…** (⌘/Ctrl-O) opens images in the appropriate editor and project
scripts in the text workspace. **Open noteskin…** chooses a noteskin folder.
Both editors' native File menus now have Open, Save As, Save a Copy and
Export dialogs. They run asynchronously while the game preview keeps drawing.
Opening a noteskin resource focuses its managed document; independent artwork
opens as a reference, with its own native editing and file saves.

**Save text copy…** writes the active script draft to a separate file.
**Export skin…** chooses a parent folder and creates a new named export there;
the destination field below the preview still accepts an exact new path.
All file writers protect the current noteskin's original/fallback folders.

The project also checkpoints independent reference documents in both editors,
including unsaved new documents, and restores them when reopening the window.
Native Save As writes the separate chosen file; project checkpoints preserve
the workspace independently. Reference documents do not change a skin's art
until it is edited through its project resource. Raster/vector mode buttons
let you reach reference tabs even when that editor has no project file open.
Editor preferences are saved in the isolated SkinCraft profile.

## MCP connection

Start the GUI with `run.sh`. Register the stdio bridge once, from this repo:

```sh
codex mcp add skincraft -- "$PWD/tools/skin-studio/mcp.sh"
```

The [Codex MCP setup documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)
describes registration and client refresh. Other MCP clients can launch the
absolute path to `mcp.sh` as their stdio server command. It connects to the
same live SkinCraft window; it never substitutes a separate headless session.
The bridge can initialize without the GUI, but calls then ask you to start it.
`DDI_STUDIO_HOME` must match between the GUI and bridge when using another profile.
The wrapper uses the prebuilt `target/photocraft/debug/skincraft` executable;
run `run.sh` again after source changes to rebuild it.

| Tools | Purpose |
| --- | --- |
| `inspect_workspace`, `list_files` | Project, preview, errors, both editors, and fallback resources |
| `open_file`, `load_skin` | File-type routing and switching noteskin projects |
| `read_text`, `write_text`, `release_svg_source` | Script/metrics/SVG drafts and validated reloads |
| `editor_request` | Native command catalogs, object/layer inspection, tools, commands and pointer strokes |
| `save_project`, `save_file`, `export_skin` | Editable project, independent copy, runtime skin |
| `set_preview`, `screenshot` | Repeatable demo position and an inline capture of the whole window |

For native drawing, call `editor_request` with `method: "engine.commands"`
and `editor: "vector"` or `"raster"`, then use the returned command IDs and
parameter descriptions with `method: "engine.execute"` and
`params: {"command": "…", "params": {…}}`. `ui.pointer` uses document
coordinates. Opening a project image already selects the correct editor.
Native background jobs return their job ID; query `jobs.list` through the
raster command interface before depending on the result. Window screenshots
need a visible/presented window. Renderer-only previews use `ui.render`.

The app binds an authenticated loopback server on a free port. Its private
`control.json` descriptor names a user-only token file; tokens never appear
in the MCP configuration or process arguments. Calls expire before dispatch
if the window stops processing frames. A timed-out dispatched edit is never
replayed automatically: inspect state before retrying.

By default, MCP can open bundled noteskins, current project resources and
reference files in the project home. Writes go under that home; existing
noteskin sources remain protected. For another working tree, start the GUI
with `SKINCRAFT_READ_ROOT=/absolute/input` and
`SKINCRAFT_WRITE_ROOT=/absolute/output` (existing folders). GUI file dialogs
use the person's chosen paths. Script edits are drafts in the loaded project,
not direct writes over source files.

## Edit a project

1. Choose a bundled skin, or enter a skin folder and click **Load**.
2. In **Noteskin Files**, search filenames or use the element filters.
   Files are grouped by the skin that supplies them; inherited files are
   labelled. The list includes resources unused by the current layout.
3. Double-click an image or choose **Open image**. SVGs open in VectorCraft; bitmaps open in PhotoCraft. Shared tabs
   switch editors without leaving the window, retaining each document's own
   layers, objects and undo. **Open all
   images** opens them progressively within the project limits. Selecting
   an already-open image focuses its existing tab.
4. Paint, transform, change layers or undo. All managed documents update the
   preview, including unfinished brush strokes, whether their tabs are
   active or not. Closing a tab retains its project artwork and session
   undo history. Reopen it from the file browser.
5. Double-click a script/metrics file or choose **Edit source text**.
   The movable, resizable text window has file tabs, syntax colouring,
   independent text undo/redo and a line indicator. **Text tabs** reopens
   the window. Text changes reload after a 450 ms typing pause. Errors
   include the loader's file/line information where available. A failed
   reload keeps the previous actor tree; animation-time Lua failures also
   restore the previous working script version. Failed drafts remain
   editable and saved in the project.
6. **Save project** saves native vector sources, layered raster sources and text drafts, including
   closed image tabs. Changed committed artwork and draft text also get
   automatic project checkpoints after roughly a second. Reopening the
   launcher restores the project and previously open files. Native drawing
   undo history survives closing/reopening a tab within the same session;
   disk checkpoints preserve layers and pixels, rather than undo history.
7. **Export skin** writes game-readable files into a new destination.
   Import its `NoteSkins` folder into the game, or add the export root as
   a desktop StepMania root. Export refuses an existing destination.

Use **Save project** (⌘/Ctrl-S) to retain editable artwork and **Export skin** for the
game's runtime files. Native editor tab-close prompts can still appear;
closing a tab retains its captured source independently. Project checkpoints
preserve objects/layers across restarts; session undo survives tab closure,
but its history is not serialized to disk.

Edits never write the source noteskin or a shared fallback folder. Inherited
files become overrides inside this project's exported copy of the fallback
tree, preserving script paths that explicitly name their parent skin.
Different noteskin folders have separate project checkpoints.

## Find what affects an element

Expand **Inspect element and its files** below the preview. Choose a lane
and element (tap, receptor, hold/roll part, mine or explosion), then click an
image path to edit it. These texture paths come from the actual built actors,
including Lua redirects, shared textures, model textures and note-colour
variants. Related scripts and metrics are listed separately; arbitrary Lua
can load additional scripts, all accessible through the file browser.

The preview also offers dance, Dancing☆Onigiri and pump layouts, pause,
restart, time scrubbing, reverse, scroll speed and backdrop brightness.
**Autoplay flashes** compares the autoplay and manual hit effects without
changing the demo's timing examples; MCP `set_preview` accepts the optional
`autoplay_flashes` boolean. **Rules → DanOni** selects its native four-tier
judgments; MCP uses `ruleset: "danoni"` (also `itg`, `sm5`, `ddr-a`).
The element inspector includes dropped hold/roll
heads, tails and bodies when a skin opts into the third state.
The coverage disclosure reports lane mappings and loader messages.

## SVG editing

Double-click Dito's `_down receptor.svg`. Its arrow is an opacity mask:
select the artwork, open **Window → Transparency**, and click the mask
thumbnail to edit its paths with the direct-selection tool. Click the object
thumbnail to leave mask editing. Drag previews, commits, cancel and undo all update
the same game texture. The other Dito SVGs, including note sprite sheets,
open in the same vector workspace.

VectorCraft exports presentation-attribute SVG. The game's existing SVG
checker and decoder validate each changed document before its pixels reach
the preview. Masks, gradients, alpha and sheet dimensions remain SVG data;
the runtime export stays `.svg`. Native `.vectorcraft` checkpoints retain
the editable model separately. Unsupported effects such as SVG filters show
an error, preserve the last valid preview, and block runtime export until
fixed; they are never silently flattened.

To edit an SVG as text, select **Use SVG source** in the file browser. This
releases its native vector document and keeps the last accepted SVG draft.
The source-text view is read-only while a native vector override exists.
Reopen the image to import the edited source back into VectorCraft.

The older PhotoCraft-only host can still rasterize an SVG for painting;
its PNG export behavior is independent of SkinCraft's vector route.
Other editable texts include Lua, INI, redirects, legacy actor files and
MilkShape text models. Binary resources without an editor stay in exports.

## Add onigiri

**New onigiri working copy** uses the destination field beside Export. It
creates a child skin with dedicated onigiri tap, receptor and explosion,
preserving the project's image and text edits and its inherited arrows and
hold bodies. Its `_studio onigiri` textures are ordinary editable images.
A lane mapped to Center or Down uses that fallback rather than dedicated
onigiri art; check the loader messages as well as the visible result.

## Files and limits

Live composites retain colour management and straight alpha, converted to
sRGB for the gameplay renderer. They exclude checkerboards, guides,
selection outlines and monitor conversion. Only changed snapshots are
composited; normal painting does not restart the demo.

Raster documents are limited to 4096 pixels per side and 4 megapixels each;
a project retains at most 128 drawings and 64 megapixels of canvas area.
At most 128 vector documents are retained, with native checkpoints limited
to 64 MiB each. Text files are limited to 1 MiB and the project manifest to 32 MiB. The
browser lists up to 4096 files and caches up to 128 small thumbnails.
A differently sized drawing is fitted to the runtime texture's original
size; sprite-grid and logical-size hints retain the skin's meaning.

Exports contain native SVGs, PNG raster composites, edited texts and the
fallback tree. Raster-painted SVG/GIF/JPEG files become PNG; SVG resolution hints retain their logical
display size. Literal references are updated. Export checks the result with
the ordinary game loader before publishing the directory. Arbitrary computed
filename references may need manual updates after an extension changes.
Upstream notices are retained and `AUTHORING.json` records edited resources.
Update provenance/licensing before bundling an export. Dito's generated
SVGs in `assets/` remain the generator's output.

The looping demo covers taps, judgements, misses, holds, rolls and mines;
it does not yet include explicit lift/fake examples. GPU loss is reported:
save artwork and restart the development build.

## Verify

This separate Cargo workspace is outside the game's ordinary checks:

```sh
CARGO_TARGET_DIR="$PWD/target/photocraft" cargo build \
  --manifest-path tools/skin-studio/Cargo.toml --features skincraft --bin skincraft
CARGO_TARGET_DIR="$PWD/target/photocraft" cargo test \
  --manifest-path tools/skin-studio/Cargo.toml --features skincraft --lib
CARGO_TARGET_DIR="$PWD/target/photocraft" cargo clippy \
  --manifest-path tools/skin-studio/Cargo.toml --features skincraft --all-targets -- -D warnings
```

Dito SVG checks compare every bundled SVG before/after VectorCraft export.
A real stdio MCP client verifies initialization, tool discovery, both editors,
source changes, SVG/raster copies, runtime export and window capture. Native
Save As/Cancel continuations, source protection and independent document
restoration run offscreen too. The combined editor checks also exercise live anchor drags, unsupported effects,
undo after closing a tab, editor routing, SVG export and project restore.
Synthetic fixtures also cover multiple images, inactive/closed tabs, layers and
project restore, text reload failure and recovery, animation errors, export,
SVG display sizes, inherited/unused resources, and actual actor texture
inspection. GPU tests render the real PhotoCraft panel offscreen and save
captures under `target/scratch/skin-studio/`. Agent runs use scratch homes
and the memory watchdog; tests never start a sound backend.
