# Implementation plan: overlays (2026-10-14)

Phase 8's last data-driven layers: StepMania's `#BGCHANGES2` (a second
background layer) and `#FGCHANGES` (a foreground over the field), and
Dancing☆Onigiri's `back_data` and `mask_data` (layers under and over the
play window). Research first, as the premise ("data the existing
background layers can draw") held only in part.

## What the sources say

**StepMania 5.1** (MIT; `src/Background.cpp`, `src/Foreground.cpp`,
`BackgroundEffects/`):

- `#BGCHANGES2` is a full background layer like the first
  (`BACKGROUND_LAYER_2`), drawn over it and under the field: the same
  file lookup (the song's folder, its BGAnimations, movies and bitmaps,
  then the shared folders) and the same effects. An effect is a theme
  Lua file (`BackgroundEffects/*.lua`: `StretchNormal`, `Centered`,
  `UpperLeft`, `StretchNoLoop`, `StretchRewind`, `StretchPaused`,
  `Checkerboard*`, `Kaleidoscope2x2`, `OverlayAdd`, `File1FlashOverlay`,
  `File2Flash`, `File2Normal`, `StretchNormal{Red,Green,Blue}`,
  `Visualization*`), with `Color1`/`Color2` and a second file. A still in
  layer 2 under `StretchNormal` covers layer 1 entirely; layer 2 shows
  through only with art that has transparency or an effect that blends.
  A missing file in layer 2 is never loaded, so the layer keeps what it
  showed (layer 1 gets a random movie instead); layer 2 shows nothing
  before its first change (2026-10-16, decision 48: read again in
  `UpdateCurBGChange`).
- `#FGCHANGES` entries are actors (`ActorUtil::MakeActor` on the song's
  folder: a `default.lua` or XML BGAnimation, else the file), shown from
  their beat **until their tweens end** (`GetTweenTimeLeft`): a plain
  still or movie has no tweens and is hidden the frame it appears. In
  practice the foreground is scripted animations only.
- Usage: no `#BGCHANGES2` or `#FGCHANGES` in the 735 local DDR/DWI charts
  nor in the bundled songs (whose `#FGCHANGES` are empty); one bundled
  song names a BGAnimation (`white reverse flash`) on layer 1, which we
  show as the song background.

**danoniplus** (MIT; `js/lib/dosConverter.js` `makeSpriteData`,
`drawBaseSpriteData`, `makeSpriteImage`, `makeSpriteText`;
`js/lib/mainWindow.js`; `css/danoni_main.css`; at `500f794`):

- `back_data` and `mask_data` are lines
  `frame,depth,source,class,x,y,w,h,opacity,animation,frames,fillMode`.
  Each depth is a layer (`backSprite<n>` under the play window,
  `maskSprite<n>` over it); a line sets its depth's content at its frame,
  replacing what was there, `frame,depth` alone clears it (`ALL` clears
  every depth).
- A source is an image (by extension; an `<img>` at `x,y`, `w,h` when
  positive, else its own size), else text (a `<span>`: `w` is the font
  size, `h` the color), `[c]` a gradient box, `[t]` a CSS transform of
  the whole depth, `[loop]`/`[jump]` for the title and results screens.
  `x,y` may be expressions in braces. `opacity` is CSS opacity; the
  animation is a CSS animation by name over `frames / 60` s; `fillMode`
  reaches a `[c]` box's style alone, so images and text keep CSS's
  `none`. The stage clips its layers (`overflow: hidden`). A cue fires
  on its frame only: a play from `startFrame` shows no earlier one. A
  layer's `Rev` data takes the other's place whenever its header is
  there, empty or not. Escapes become HTML references that the page
  decodes after its tags.
- The stage is `g_sWidth × g_sHeight` (600 × 500 unless a work or key
  mode widens it); the layers are shifted by `backX` with the play
  window.
- danoniplus's stylesheet defines 23 animations (`leftToRight`,
  `upToDown`, `leftToRightFade`, `upToDownFade`, `slowlyAppearing`,
  `fromBig`, `smallToNormalY`, `spinX`, `spinY`, `spinZ`, `blur`,
  `brightToDark`, `fadeIn0`, `fadeIn1`, `fadeOut0`, `fadeOut1` and the
  `ex-*` play effects); works define others in their own CSS.
- Usage in the local Dancing☆Onigiri corpus: 36 of 79 captured works'
  files (13 works) use back or mask layers: 3,035 images, 2 texts, no
  `[c]` or `[t]`, no expressions, 670 with a class; the animations named
  are mostly the works' own (`illustIn`, `nameIn`, `illustOut`,
  `nameOut`, `editorIn`, `editorOut`: 899) and a few built-ins
  (`fadeIn0`, `fadeOut0`, `leftToRight`, `fromBig`, `smallToNormalY`:
  156). A work's stylesheet is not part of a capture, nor of what a page
  import reads.

## Decisions

1. **Dancing☆Onigiri's back and mask layers first**: half of the works
   at hand use them, where StepMania's second layer and foreground have
   no use at hand.
2. **Drawn in the wgpu scene**, not as HTML: back layers lie under the
   arrows, which the canvas draws. The stage maps onto the screen as the
   lyrics' field does (decision 29): a `g_sWidth × g_sHeight` rectangle
   centred on the field, scaled by the screen's height over 500.
3. **Images and text**: images from the work's files (stored with an
   imported work, as background images are, with a cap), text drawn by
   the browser into a texture (the HUD font has no Japanese, decision
   29). `[c]` boxes later if a work needs them; `[t]` transforms and
   brace expressions are left out (none at hand) with a note in the
   import report.
4. **Animations**: danoniplus's built-ins reimplemented from its
   stylesheet (keyframes as data, interpolated per frame, without fill
   modes, as danoniplus gives images and text none); a work's own animations, whose definitions we do
   not have, play as no animation (the image shows as placed), counted in
   the import report. `ex-*` (play effects on the field) are not layers'
   and are left out.
5. **StepMania's second layer next**, with the effects that need no
   script (the stretch, centre and corner placements, the color tints,
   `OverlayAdd`'s additive blend, the checkerboards and kaleidoscope) on
   both layers, stills and movies; BGAnimations stay the song background
   as on layer 1.
6. **`#FGCHANGES` joins scripted backgrounds**: a foreground is a script
   or nothing, so it needs the actors and theme effects scripted
   backgrounds need (a sandboxed VM per song, on the skins' Lua runtime).

## Status (2026-10-14)

Done, steps 1 to 5 (Dancing☆Onigiri's back and mask layers):

- `chart`: `back_data`/`mask_data` (and `Rev`) read as `makeSpriteData`
  reads them into `danoni:back`/`danoni:mask` events; the stage's size
  per chart (`DanoniChart::stage`: the 500 px frame, `windowWidth`,
  the key modes' minimum widths, `windowHeight`).
- `library`/`app`: a page's layer images found where danoniplus loads
  them (`preloadImgFile`: the page's folder, `(..)`, `syncBackPath`),
  stored or linked as `layer/<name>` (128 at most a song), the missing
  and SVG ones reported; a capture's layers have no images. The import
  report notes animations from a work's own stylesheet, color boxes,
  layer transforms and script-computed positions.
- `crates/engine/src/sprites.rs`: each depth's content at a time, clears and
  `ALL`, Reverse's data, danoniplus's 16 layer animations with CSS's
  `ease` per keyframe segment, transforms about the centre (`spinX`/
  `spinY` as a squeeze without perspective; `blur` unblurred;
  `brightToDark` darkens only); no cue before `startFrame`; text with its
  tags dropped, then its references decoded.
- `crates/render/src/stage.rs` and the scene: back quads first, mask quads
  before the HUD (danoniplus puts masks over its own judgements and
  information; ours stay readable over them); clipped to the stage (a
  turned quad, `spinZ`'s, drawn whole); images and text (drawn by the browser in danoniplus's
  base font, the templates' `#cccccc` by default) as textures registered
  with a skin's, again after a skin replaces them; shown with
  backgrounds (brightness above 0).

Checked: native tests (parse, stage size, image paths, timeline,
animations, quads); the local corpus imports as before (13 charts match
danoniplus), and its one playable work with layers (Dancing at Dusk, 5
keys, 60 cues) builds its timeline; danoniplus's sample work 1 imported
with its images and played in headless Chrome (WebGPU and WebGL2) and
Firefox, and compared with danoniplus's own page at the same frames
(410: the `fromBig` text and the grey mask over the stage's bottom half;
820: `spinZ` a quarter turn on): the same places and phases.

After an independent review: fill modes left to color boxes, clipping
to the stage, Reverse's empty data, `startFrame`, the order of tags and
references (`*comma*` is `‚`), a depth that does not evaluate as 0,
`trimStr`'s trimming, budgets (128 images, 64 texts, 48 million decoded
pixels, only the images the import kept) and set lookups.

Known: textures of a song's layers stay registered until the next skin
install (a few per song). The stage is scaled to the screen's height,
the lanes by the field's own rule (at most 160 px an arrow), so on very
tall screens the layers grow against the lanes. Cues before the music
starts keep the importer's frame-to-time mapping (danoniplus clamps
negative frames to 0).

Lyrics (2026-10-16, decision 29) are drawn by the 2D pass on the same
stage, fitted to the screen (its width on a portrait phone, so lines
stay on it), with two adjustments of ours: a top line moves below our
HUD's gauge and score, a bottom line narrows past the touch quit button.
Not read yet: `bottomWordSet` (bottom lines under the step zone at
`distY + 10` instead of 60 px up from the bottom), `playingWidth`,
`playingX` and `playingY` (a playing area narrower than or moved within
the stage, which places the labels and the layers): the lines and the
layers keep the stage's own box. A text with commas loses the spaces
after them, as danoniplus's `trimStr` trims every field before joining
them again (`makeSpriteWordData`).

## Steps

1. **Model** (`chart`): `back_data`/`mask_data` and their variants as the
   play reads them (`Rev`, `Alt`, `A`, `En` suffixes chosen as for
   `word_data`), into a timeline of `(seconds, layer, depth, content)`
   with content = image (path, x, y, w, h, opacity, animation, duration,
   fill), text (string, size, color, …) or clear; parsed as
   `makeSpriteData` parses, natively tested on the sample work and the
   corpus (`DDI_DANONI_CORPUS`).
2. **Import** (`app`, `library`): the images a work's layers name, stored
   with it under a cap, the rest reported.
3. **Engine** (`engine`): the layers' state at a time (each depth's
   content, its animation's progress), with the built-in keyframes.
4. **Render** (`render`, `app`): back layers between the background and
   the field, mask layers over the field and under the HUD; images as
   textures, text through the browser.
5. **Checks**: native tests; the sample work and corpus works in headless
   Chrome (WebGPU, WebGL2) and Firefox, compared with danoniplus's own
   page at the same frames.
6. Then StepMania's second layer and the script-free effects (decision 5),
   with a synthetic pack. **Deferred** (2026-10-14): wanted, but none of
   the 1,489 local chart files (StepMania, DWI and pump packs) uses
   `#BGCHANGES2`, `#FGCHANGES`, Lua or BGAnimation folders, so other work
   comes first. **The second layer done** (2026-10-16, PLAN.md decision
   48): stills (with their transparency) and movies over the first
   layer, with the rules StepMania gives that layer alone
   (`backgrounds::second_layer`); still none of the 5,094 local simfiles
   uses it. The script-free effects (`OverlayAdd`, tints, placements) are
   not done, on either layer.
