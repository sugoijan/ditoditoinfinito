# Noteskin SVG: one rasterizer, filters, quality and materials

Date: 2026-10-24. Inputs: the authoring reports on the
`codex/noteskin-authoring-reports` branch (`docs/reports/noteskin-engine-feedback.md`,
NE-xx below; `docs/reports/skincraft-authoring-feedback.md`, SC-xx) from the
Peters-DDR completion and the Spiral Base skin. The work starts from `main`;
the Spiral Base skin and its stencil materials (`HoldStencilMaterial`,
`AnimationOnChildren`, …) are rebased on top of it later, so steps that need
them come last. (Spiral Base, the SVG skin these measurements use, is
developed apart and not bundled yet; the stencil materials are.) SkinCraft's side follows in its own repository, on top of
these changes.

## Where things stand

SVG images are our extension (PLAN.md decision 38): a checked static subset
(`crates/noteskin/src/svg.rs`), rasterized once at load at four times its
size (`build::SVG_SCALE`), within the skin's budget of `64 << 20` decoded
pixels kept on the CPU and again on the GPU (`build::MAX_PIXELS`). Desktop
draws it with resvg (`crates/desktop/src/play/skins.rs`), one image after the
other; the web with the browser (`crates/web/src/web/image.rs`: an `<img>`
onto a canvas), one image after the other. The subset is what both draw
alike, so it has no filters: shadows are drawn as dozens of nested opacity
contours (NE-02), and the two shells can still differ in ways no test sees
(NE-04, NE-08).

Measured on an M1 Max for the 63 bundled SVGs (68.3 Mpx at 4×): usvg parses
them in 100 ms; resvg draws them in 800 ms on one thread; vello_cpu 0.3 in
298 ms on one thread and 94 ms on four, but through vello_svg 0.11 it drops
masks (Dito's receptor) and draws radial gradients wrongly. vello (the
compute renderer) needs compute shaders (no WebGL2), has no filters and is
now an experimental research crate; vello_gpu cannot draw masks yet. In wasm
(opt-level z, `wasm-opt -Oz`) resvg with usvg and tiny-skia is about
230–250 KB gzipped; vello_cpu with usvg about 257 KB with `simd128`.

## Steps

1. **One rasterizer for both shells.** The web stops drawing SVGs with the
   browser: a worker of its own (`crates/svg-worker`, `ddi-svg-worker`, built
   by Trunk beside the app as the Opus worker is, its files named by their
   hash) rasterizes them with the same code desktop runs
   (`ddi_noteskin::raster`, behind the `raster` feature, so the app's own
   wasm stays without it). Several workers at once, ended when the skin is
   loaded and idle; desktop rasterizes on threads. resvg moves to 0.49.
   Checked: pixels identical between desktop and the worker on every bundled
   SVG; load time against the `<img>` path in Chrome and Firefox (headless),
   recorded here.
2. **vello_cpu, measured.** The same `raster` entry point behind a second
   backend (a usvg tree walk of our own over vello_cpu: paths, gradients with
   their spread, clip paths, masks), measured natively and in the worker
   (with `simd128`) on the bundled skins and on a Spiral-sized set. Kept if it
   draws everything the skins use within a small difference from resvg and is
   clearly faster; otherwise recorded and left out. Whichever supports the
   features below and performs well is enough.
3. **What a skin's images cost, shown** (NE-03, NE-05, NE-08; P1). A load
   report per skin: each image's content hash, rasterizer, effective scale,
   decoded pixels and share of the budget, with the total; Options' preview,
   `#/skin-preview` and SkinCraft read it. Bundled skins' files are fetched
   by content hash, so a reload never shows an old texture.
4. **Filters, bounded** (NE-02). Once both shells draw with the same code,
   the subset admits `filter` with `feGaussianBlur`, `feOffset`, `feFlood`,
   `feComposite`, `feMerge` and `feMorphology` (whatever step 2's choice draws),
   with limits: primitives per filter, blur deviation and radius, and the
   filter regions' pixels at the rasterized size, counted like drawn
   elements. Checked against browsers for the shadows the reports describe
   (as a reference only).
5. **Raster quality apart from logical size** (NE-03). A skin declares the
   quality its images need instead of shrinking their `width`/`height` and
   naming them with resolution hints; images are rasterized at the size the
   lanes are drawn at (the arrow size and the device's scale, capped at
   today's 4×) and drawn again when that grows, cached by hash and size.
6. **A built-in spiral material** (NE-01). A fragment-shader material (so
   WebGL2 draws it too) masked by an SVG's alpha, its phase from the existing
   Vivid clock: origin, winding, zoom, palette, window and direction as
   bounded metrics. After the Spiral Base work is rebased.
7. **Material-part metadata and validation** (NE-06, NE-07). After step 6.

## Step 1: done (2026-10-24)

`ddi_noteskin::raster` (feature `raster`) draws an SVG with resvg 0.49;
desktop decodes a skin's images on up to eight threads (admitted in turn
against the budget first, each image's pixels known from its header or its
root, so the outcome does not depend on the threads), and the web sends
them to `ddi-svg-worker` (`web::svg`: up to four workers, one less than
the cores, ended 3 s after the last image; the module compiled once a
session, started as a skin with SVG begins loading). The workers answer
with an `ImageBitmap` made in the worker, so the page's thread does no
pixel work. Where the module cannot be fetched or no worker starts, the
browser draws them as before (one warning per load).

Checked with `target/scratch/svgbench/` (headless, M1 Max, 10 cores):

- The pixels a worker draws are byte-identical to desktop's for every
  bundled SVG (63) in Chrome, Firefox (Playwright's build) and Playwright's
  WebKit, and for Spiral Base's 199 in Chrome. The browser's own drawing
  differed from resvg's by at most 24 levels on Dito's edges in Chrome and
  Firefox, and up to 69 (DanOni: 147) in WebKit; the worker's does not
  differ between browsers.
- The worker is 917 KB (361 KB gzipped) with its rasterizer's crates at
  `opt-level = 3`; at `z` it was 677 KB (266 KB) and 2.5 times slower.
- Spiral Base (199 SVGs, 56 Mpx): 1.7 s in the workers, 3.5 s drawn by the
  browser one after the other (desktop: 3.0 s on one thread, 0.54 s on
  eight). The bundled skins' 63: 468 ms against 422 ms.
- A skin load (`__DDI_SVG_LOADS`, Dito's 14 images): the session's first
  133 ms against 95 ms (the module's fetch, compile and the workers'
  start), later ones 69–76 ms against 62–72 ms.

## Step 3: images' costs and versions (2026-10-24)

`ddi_noteskin::report::ImageReport`: each image a skin asked for, its
file's content hash (FNV-1a, 64 bits), the size the skin sees, the pixels
decoded, and what became of it (kept, drawn by the browser, left out over
the budget, failed and why), with the skin's total against `MAX_PIXELS`
and one hash over all its images. Desktop keeps it on `skins::Prepared`
(for SkinCraft) and prints its summary line; the web publishes it as
`window.__DDI_SKIN_IMAGES` with the same line in the console; both give the
same images hash for the same files. The bundled index (`xtask
gen-noteskins`) carries every file's hash; the web fetches bundled files
as `noteskins/<dir>/<file>?v=<hash>` and revalidates the index each
session, so a new build's texture never comes from an old one's cache
(NE-08). The clocks and resolved metrics per part (NE-05) belong with
SkinCraft's inspection and come with it.

## Step 4: filters (2026-10-24)

`ddi_noteskin::svg` admits `<filter>` with `feGaussianBlur`,
`feDropShadow`, `feOffset`, `feFlood`, `feComposite`, `feMerge`
(`feMergeNode`), `feBlend`, `feColorMatrix`, `feComponentTransfer` (its
`feFunc*`) and `feMorphology`, referenced by `filter="url(#id)"` only (no
CSS filter functions); `feImage`, `feTurbulence`, lighting, convolution and
displacement stay out. A filter has at most eight primitives
(`MAX_PRIMITIVES`), a blur's deviation at most 32 of the SVG's units and a
morphology's radius at most 8; a filter's primitives count as drawn
elements wherever it is drawn. What it costs depends on the size drawn, so
`ddi_noteskin::raster` bounds it after usvg has laid the image out:
`layer_work` counts the pixels of every isolated group's layer (opacity,
mask, clip path, filter; resvg clamps a layer to five times the image each
way) times its passes (one, one more each for a mask and a clip path, one
per primitive, three for a blur, a morphology's whole window per pixel,
which resvg visits naively, its radius scaled by the group's transform as
resvg scales it), and an image over sixteen passes of the
largest image (`MAX_LAYER_WORK`) is refused with the reason in the skin's
image report. The bundled skins and Spiral Base take at most 0.21 of one
pass.

A test SVG with a blurred, offset and composited shadow, a drop-shadow
glow and a dilated outline, drawn by the worker and by Chrome at 256
pixels: 0.28% of the premultiplied channels differ by more than 8 levels
(the outline's edge pixels; the browser is only a reference now).

## Step 5: drawn for the screen, at the author's quality (2026-10-24)

An SVG is drawn at `build::svg_scale(screen height)` times its size: the
largest arrow a field draws on that screen (a tenth of its height in
device pixels, 24 to 160, as `FieldGeometry` sizes it) over a skin's
64-pixel lane, times `ZOOM_HEADROOM` (1.25, for actors zoomed past their
lane), from 1 to 4. The screen, not the window, so resizing never draws a
skin again; desktop takes it from the window's monitor
(`skins::set_screen_height`, on creation and resize; a skin already loaded
keeps its own, tools that set none keep four times), the web from
`screen.height × devicePixelRatio` at each load; `#/skin-preview`'s
loads also cover its own arrow size (`noteskins::prepare_while`'s
`arrows`, for that load only). An SVG's
root may cap it with `data-raster-scale` (0.5 to 4, our extension), so a
large animated sheet keeps its logical size and frame layout and stays
within the budget without a smaller `width`/`height` and a resolution in
its file name (NE-03).

Dito's 14 images: 0.8 Mpx on a 720-pixel screen, 3.2 Mpx at twice that
density, 6.5 Mpx at four times as before; `#/skin-preview` at 128 pixels
draws them at 2.5 times, looking the same as at four (the sprites have no
mipmaps, so less is not blurrier). Left: keeping drawn images across loads
of a session, by hash and size.

## Step 2: measured, not adopted (2026-10-24)

One thread in V8 (Node), Spiral Base's 199 SVGs at `opt-level = 3`:

| | scalar | `simd128` |
|---|---|---|
| resvg 0.49 | 4834 ms | 3235 ms |
| vello_cpu 0.3 (through vello_svg, masks dropped) | 3611 ms | 1837 ms |

vello_cpu would be at most about 1.8 times faster than resvg with
`simd128`, before drawing masks (60 of Spiral's 199 files use them), which
vello_svg drops, so it needs a tree walk of our own; and it draws filters
only as single primitives (a shadow's blur, offset and composite is a
graph), panicking when multithreaded. resvg draws everything step 4 wants
and, in the workers, already draws Spiral in half the browser's time, so
it stays. `simd128` speeds resvg up 1.5 times too; Trunk cannot set
target features for one worker only, so since 2026-10-24 the whole web
build has it (`.cargo/config.toml`, the maintainer's choice; the video
module already needed it): a browser without wasm SIMD (Safari before 16.4)
is told the game cannot run (`no-wasm-simd`, site/index.html). Measured
in Chrome with it: Spiral Base's 199 SVGs in 1.23 s in the workers (1.71 s
without, 3.67 s drawn by the browser), the bundled 63 in 380 ms (468 ms);
still byte-identical to desktop in Chrome, Firefox and WebKit. The worker
is 328 KB gzipped (361 KB without), the app's wasm about the same size.

## Not pursued

vello (compute) and vello_svg as they are: no WebGL2, no filters, masks
dropped. Revisit vello_gpu once it draws masks, for rasterizing on the GPU at
device size.


## Infinito authoring port (2026-10-25)

The authored skin formerly called Spiral Base and Dito is now bundled as `Infinito`, the
default, replacing its original art and color variants. Its editable SVG, WGSL
and Lua sources live in `assets/noteskins/dance/Infinito/`; experimental
authoring tools and retired assets are kept separately. Taps, receptors, holds, rolls, lifts and
judgment effects use SVG components and skin-owned materials. Mines keep
actors for field-aligned neighboring-lane electricity; opt-in judgment rim
and link roles join matching receptor shocks. The original Spiral Base
measurements above remain historical; its old bundled name is retired.
