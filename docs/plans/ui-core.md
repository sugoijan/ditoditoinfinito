# Implementation plan: our own UI core, on web and desktop (2026-10-15)

Draw every player-facing screen with the game's own scene on the GPU
instead of Yew over the DOM, on the web and on desktop from the start
(decision 45). It is the first part of phase 9: the desktop shell starts
here as a window that runs the menus, and the rest of the desktop build
(audio, controllers in play, play itself, the library on disk) follows in
its own plan. Research: `docs/research/ui-toolkits.md` (toolkits
measured, Masonry checked in depth, game UI and rhythm games, what to
reuse) and `docs/research/stepmania-themes.md` (what a StepMania theme is
and what hosting one takes).

Status (2026-10-15): steps 0–4 done (results under each step); the menus
are still Yew until steps 5 and 6. Decided by the maintainer: our
own UI core rather than a toolkit; web and desktop both kept in view;
StepMania 5 themes later, gameplay and evaluation decorations first;
Japanese input, a hidden text field being acceptable; accessibility
deprioritized; on the web the browser draws the text (P4); fonts under
OFL-1.1 may be bundled (P6); no portrait layouts for now. Everything else
below (the other P's, the steps) is proposed and settled as the steps go.

## Goals

- **Every player-facing screen drawn by the game:** song select, options
  (with bindings and the bind flow), calibration, the play screen's
  prompt and HUD, results, credits, the app-wide notices. Proposed to stay
  DOM: file and folder pickers, a hidden text field for typing, drag and
  drop, the crash page, a fallback page for browsers with no usable GPU,
  and developer pages (`#/video-check`).
- **Web and desktop from the start, embedded later.** Nothing in the
  core may assume threads, system fonts, a DOM or a window system; memory
  for textures and text is bounded; what the core produces is a plain draw
  list any renderer can draw.
- **Our own look.** The game ships a theme of its own, not StepMania's.
- **StepMania themes later** through the same scene (gameplay and
  evaluation decorations first, `stepmania-themes.md` section 7), in a
  plan of their own.
- **What the maintainer asked our theme to do (2026-10-15):** a song list
  that scrolls smoothly with the wheel, loads banners lazily and stays
  fast on a large library; touch; autocomplete under the search field;
  results whose statistics and layout vary with the ruleset and the
  player's choice.
- **Input:** keyboard, controllers and dance pads (menus by semantic
  actions, as today), mouse, touch and pen; Japanese text shown and typed.

Not goals for now: accessibility (deprioritized until the game's own play
can be made accessible, with real feedback), whole StepMania themes (tier
(c)), multi-line text editing, selecting text with the mouse.

## Architecture

```
actor/    ddi-actor    actors as StepMania runs them (moved out of noteskin/, with the model
                       data types), plus quads, text, clipping, draw order, messages, the
                       virtual screen → draw list
ui/       ddi-ui       screens, input routing, focus, widgets, layout frames, theme lookup,
                       our theme's components; UiEvent in, UiRequest out
render/   ddi-render   draws a draw list (2D pass) besides the play field; texture cache
noteskin/ ddi-noteskin skins built on ddi-actor (MilkShape and legacy parsing stay here)
app/      web shell    one canvas for the app, DOM events → UiEvent, hidden textarea, file
                       inputs, storage; Yew shrinks screen by screen, then goes
desktop/  ddi-desktop  winit window, wgpu surface, input → UiEvent, rfd, arboard, settings
                       file, system fonts; audio and play come with the rest of phase 9
```

(`ddi-actor` rather than "scene": `render/src/scene.rs` already names the
play field's sprite builder.) `actor` and `ui` are platform-free and
tested natively, like `engine` and `noteskin`. Dependency direction: `ui`
→ `actor`, `library`, `engine`, `platform`; `noteskin` → `actor`;
`render` → `actor`; the shells → everything. `PLAN.md` §3.1 and
`AGENTS.md`'s layout change in the commits that add the crates.

**Per event.** The shell turns a DOM or winit event into a `UiEvent`
(with its host timestamp) and calls `Ui::handle`, which returns
`UiRequest`s. Requests that need a user gesture (create the
`AudioContext`, start a preview, click a hidden file input, focus the
hidden textarea, open a link) are carried out before the event handler
returns; others may be queued (P2).

**Per frame.** `Ui::update(dt)` steps tweens, scrolling, gestures and key
repeat and plays queued commands; the actors fill a draw list; `render`
draws it. Menus redraw only while something moves or input arrives (P13).

**Draw list.** Quads (texture, texture rectangle, transform, corner
colours, blend mode; a full 3D transform with perspective when an actor
uses `rotationx`/`rotationy` or `fov`, which themes do), text runs, clip
push/pop (scissor rectangles; masks later through the stencil), models
(the noteskins' meshes, as now), and `External` slots the shell fills
with the play field, so a theme's layers can lie under and over it.

**Coordinates.** A theme declares its design height; the width follows
the window's aspect (StepMania's rule, `src/ScreenDimensions.cpp`).
Physical pixels per unit follow the device pixel ratio, which changes at
run time (a window moved to another monitor, browser zoom, winit's
`ScaleFactorChanged`); textures of text and art are keyed by the scale
they were drawn at.

## Decisions proposed

- **P1. One actor model.** `noteskin/src/actor.rs` and `commands.rs`
  (StepMania's tween queue, effects, sprite states, commands, eases) and
  the model data types they hold (`noteskin/src/model.rs`: `Geometry`,
  `Animation`, `AnimatedTexture`) become `ddi-actor`, shared by noteskins,
  our screens and later themes; parsers stay in `noteskin`. The scene is
  retained (Lua themes keep actor references) and actors are placed
  absolutely; layout is an opt-in frame.
- **P2. Events are handled synchronously in the platform's handler**, and
  gesture-bound work is a request carried out there. What counts as a
  gesture differs: a key press, a mouse `pointerdown`, a touch or pen
  `pointerup` (a touch `pointerdown` does not), a click. So touch actions
  fire on release; a long press, which fires from a timer, never starts
  sound or a picker. Pad presses come from the poll loop (`web/gamepad.rs`),
  which Chrome and Firefox treat as a gesture and Safari does not: there,
  as today, a prompt asks for a tap or key.
- **P3. Screens own their navigation**, as StepMania's do: menu actions
  (up, down, left, right, start, back, select) come from the existing
  bindings and the keyboard; held directions repeat (pads never repeat on
  their own; keys are repeated by us, not by the OS, so both behave the
  same); focus moves by explicit neighbours with a spatial fallback where
  a screen is a form; the focus ring shows after a key or pad press and
  hides on pointer use. Navigation reads physical keys (`event.code`,
  winit's `KeyCode`), shortcuts such as `/` the logical key (it moves on
  JIS and AZERTY layouts). Losing focus (`blur`, `visibilitychange`,
  `Focused(false)`) releases held input and cancels pointer captures. The
  browser's Back and Android's back gesture pop the screen stack.
- **P4. Text by platform** (decided 2026-10-15). On the web the browser
  draws text runs (system fonts and their fallback, Japanese included, no
  download), packed into atlas pages on a canvas and uploaded
  synchronously (`copyExternalImageToTexture` from the canvas), measured
  with `measureText`; on desktop and embedded, cosmic-text with system
  fonts and bundled fallbacks. Both sit behind one trait that measures and
  draws a run, and cosmic-text stays buildable on the web behind it, as a
  fallback and so the choice can be changed later (with a CJK font fetched
  on demand: identical layout everywhere, megabytes on the first Japanese
  title). The Dancing☆Onigiri layers draw text through the browser
  already (`app/src/web/image.rs:122`), but with one `createImageBitmap`
  promise per run, too slow for a long list. The play HUD keeps glyphon
  and its embedded font.
- **P5. Text input through the platform's own editor where there is one.**
  On the web a hidden `<textarea>` placed at the caret (so the IME's
  candidate window and a phone's keyboard appear in the right place)
  holds the text: the field mirrors its value, selection and composition
  (`compositionstart`/`update`/`end`, `beforeinput`), and keys belong to
  the IME while `isComposing`. Clipboard and undo come with it. On
  desktop, winit's `Ime` events (`Preedit`, `Commit`) and
  `set_ime_cursor_area` drive a small field model of ours (single line,
  grapheme-wise editing). cosmic-text's `Editor` has no preedit, so it is
  not used.
- **P6. Our theme is a theme.** Screens ask the theme for components by
  name (`SongList.Row`, `Results.Panel.TimingHistogram`), with a fallback
  to a plain built-in look, like the noteskin resolver; StepMania themes
  will answer the same lookups. Our theme's components are Rust over the
  actor API (a builder in osu!framework's style:
  `.linear(0.2).zoom(1.2).then().fade_out(0.1)`), its tunables (colours,
  fonts, sizes, timings) in a data file beside its art. Fonts under
  OFL-1.1 are fine (decided 2026-10-15; `AGENTS.md`), as the HUD's
  Iosevka subset already is (`assets/fonts/PROVENANCE.toml`), with any
  subsetting recorded as a modification. On the web a bundled font
  reaches the browser's text drawing as a `FontFace`.
- **P7. Messages, generalised.** Noteskins' Lua already receives messages
  (`noteskin/src/lua/mod.rs`, `HandleMessage`); screens broadcast named
  messages with parameters to every actor (StepMania's `MessageManager`,
  `<Name>MessageCommand`), so StepMania themes' `CurrentSongChanged` and
  the like map onto what our screens send.
- **P8. Layout frames via taffy**, behind a feature of `ui`, where a form
  or the results' panels want flex or grid; everything else is absolute.
- **P9. Textures by source, cached.** Banners, jackets, theme art and
  backgrounds load through one cache keyed by source (bundled path,
  imported blob, theme file) and scale, decoded off the main thread (the
  web's `createImageBitmap`, as now; a thread on desktop), downscaled to
  the size drawn, with a placeholder until ready and least-recently-used
  eviction under a byte budget the platform sets. Texture ids stay stable
  across a device loss.
- **P10. One canvas and device for the whole web app**, created at boot
  and kept across routes. Today the play screen and the skin preview each
  call `Gfx::new` (`app/src/web/gfx.rs:151`), which may replace the canvas
  when WebGPU fails (`gfx.rs:124`), and `&gfx=gl` is read per route; both
  become boot-time choices (`gfx=gl` a boot parameter and a setting). The
  first paint waits for the adapter (the boot shell shows meanwhile); a
  lost device or WebGL context is recreated (a new instance, perhaps a new
  canvas) and textures reloaded from their sources; a browser with no
  usable GPU gets a DOM page saying so.
- **P11. Colour space of the 2D pass** (settled by step 0's results:
  encoded values for menus). Decision 17
  draws in linear light and blends skins in encoded values, which on
  WebGL2 sends every frame through an offscreen float texture
  (`render/src/offscreen.rs`). Theme art and browser-drawn text expect
  encoded blending, and an offscreen pass every frame costs power on
  phones; menus may draw directly in encoded values.
- **P12. State the screens need moves out of `app`.** Settings
  (`app/src/settings.rs`, `localStorage` through gloo), the song and
  noteskin libraries (`songs.rs`, `noteskins.rs`) and the song list's
  place (`menu.rs`, `sessionStorage`) become plain data behind `platform`
  traits, with the web's storage keys unchanged so players keep their
  settings; desktop stores them in files.
- **P13. Render on demand.** rAF stops when nothing moves, but the pad
  poll keeps running while a pad is connected and wakes a frame on an
  edge; on desktop `ControlFlow::Wait`, with gilrs on a thread waking the
  loop through an `EventLoopProxy`.

Left out for now (2026-10-15): portrait layouts (phones held upright),
which StepMania themes never have either; to take up after the desktop
build and the known gaps (`PLAN.md`, section 5). Nothing in the core
should rule them out: the virtual screen's width already follows the
aspect.

## Steps

Each step is its own commit (or a few), verified natively, in headless
Chrome (WebGPU and WebGL2) and Firefox, and on desktop from step 3. The
wasm size is measured at every step against today's 2,308 KB (gzip -9,
release build of 2026-10-15) and reported in the step's notes.

### Step 0. Spikes (scratch, not committed)

1. **Text on the web** (P4): browser-drawn runs packed into atlas pages
   and uploaded synchronously; quality at 1× and 2×, the time to draw a
   song list's worth of titles, `measureText` against the drawn size in
   Chrome, Firefox and WebKit, a bundled font through `FontFace`, and the
   upload on WebGL2 as well as WebGPU.
2. **Typing over a canvas.** A hidden textarea at a drawn caret in Chrome
   (composition scripted with the DevTools protocol's
   `Input.imeSetComposition`) and in Firefox with macOS's Japanese IME by
   hand; a phone's keyboard by the maintainer; Safari after the first
   push that carries it (step 3).
3. **Desktop window.** winit 0.30.13 and wgpu 30 drawing our renderer on
   macOS, with `Ime` events from the Japanese IME, resizes and scale
   changes. Windows (TSF) and Linux (ibus or fcitx; Wayland's text input)
   differ and are checked with the rest of phase 9.

Results (2026-10-15; Chrome 155 headless, Firefox 151 and 158 through
`firefox.mjs`, Playwright's WebKit 26.5 and 27.2, an M1 Max):

- **Text.** Canvas2D runs packed into atlas pages and copied with
  `copy_external_image_to_texture` work on every backend tried (Chrome
  WebGPU and WebGL2, Firefox WebGL2, WebKit WebGPU and WebGL2; Firefox
  gave no WebGPU adapter headless). 2,000 song titles (Latin, kana,
  kanji, the bundled Iosevka with system fallback) take 63–132 ms cold,
  25–50 ms warm, and a scroll's 30 new runs 0.4–2 ms (WebKit's WebGL2
  11–14 ms). Findings that shape step 2:
  - On WebGL2 wgpu runs the copy at the next `submit`, refuses an
    `OffscreenCanvas` source and any source origin but zero; reusing one
    staging canvas showed the last run everywhere. `getImageData` of the
    dirty band plus `write_texture` works on every backend at any origin:
    the WebGL2 path. WebGPU copies dirty rectangles. Never one copy per
    run (0.2 ms each).
  - White text on single-channel (`R8Unorm`) pages is pixel-identical to
    RGBA and a quarter of the memory (2,000 titles: 4 pages of 2048² at
    1×, 13–14 at 2×). The atlas caches what is shown, not the library.
  - Ink exceeds `measureText`'s bounding box by up to 1.7 px: cells get
    2 px of padding. `fontBoundingBox` covers the primary font only, so
    line height is our own metric. The bundled font measures the same in
    every browser; system fonts differ by about 2 %.
  - Text blended in encoded values matches the DOM within 3 levels; an
    sRGB view makes light text bolder and dark text thinner. Tinting
    white text differs from text drawn in its colour by up to 53 levels
    (browsers adjust contrast per colour); tinting is acceptable for our
    theme, drawing in colour stays possible for an exact match.
  - `FontFace` from bytes: 1–3 ms (WebKit's first, 17 ms).
- **Typing.** With composition scripted in Chrome, the preedit lives in
  the textarea's value, Enter while composing arrives as key code 229
  with `isComposing`, Escape cancels the preedit, and arrows, Backspace,
  select-all and undo work natively; Chrome fires no `selectionchange` on
  arrows. Firefox orders `input` after `compositionend`, the opposite of
  Chrome. Rule: re-read the value and selection on every composition,
  input, key and selection event, never infer them from the order. What
  worked: `opacity: 0` at the caret, 16 px, autocomplete,
  autocapitalize and autocorrect off, no spellcheck, `inputmode`,
  `enterkeyhint`, `focus({ preventScroll: true })` in `pointerup`. Not
  checked: the candidate window's place, phones, Safari.
- **Desktop.** The window opened (resize and scale-factor events
  arrived) but the screen was locked, so nothing was drawn on it; the
  play scene and system-font text (Japanese, Korean, Greek through
  cosmic-text, 1,762 faces found in 25 ms) rendered offscreen on Metal,
  0.5 ms a frame. winit's IME calls compile; `Ime::Preedit` ranges are
  byte offsets. On `Occluded` the window stops drawing. rfd 0.17.2
  (`xdg-portal` by default, no GTK) and arboard 3.6.1 build on macOS;
  arboard needs `wayland-data-control` on Linux. Typing with a real IME
  waits for the maintainer.
- **P11, settled by these:** menus draw in encoded values through a
  plain view (on WebGL2 the surface also offers plain `Rgba8Unorm`), so
  no offscreen float pass while no play field is shown; the play field
  keeps decision 17.

### Step 1. `ddi-actor`

1. **The move, changing nothing.** Actors, tweens, effects, sprite
   states, commands, eases and the model data types into `ddi-actor`;
   `noteskin` keeps the parsers and the Lua. Verified: every noteskin
   test as before; the local skin corpus with Lua and play
   (`DDI_NOTESKINS_LUA=1 DDI_NOTESKINS_LUA_PLAY=1`) reports identical
   elements, load fuel and fuel per frame per skin; `#/skin-preview`
   screenshots of the bundled skins at fixed beats
   (`__DDI_PREVIEW_BEAT`) identical pixel for pixel before and after;
   bundled skins autoplay all top tier on both backends.
2. **Additions.** Quads, text runs, clipping, the virtual screen, and
   what the noteskins accept and ignore today (`draworder`, `fov`,
   `MaskSource`/`MaskDest`, `shadowlength`, `actor.rs:1676-1690`) made to
   work. Skins that call them will draw differently: the corpus run lists
   which skins changed, and each change is checked against StepMania's
   behaviour before it is accepted. Settle how actors are held (today a
   tree of owned children that the Lua stage lends,
   `noteskin/src/lua/stage.rs`; an arena with stable ids if screens need
   to reach actors by id).

Done (2026-10-15):

- **The move.** `actor/` (`ddi-actor`) holds actors, commands and the
  model data types; `noteskin` keeps the parsers (MilkShape's as
  `ddi_noteskin::model::animated_texture` and friends) and the Lua. The
  local corpus (348 skins with Lua and play) reported identical elements,
  load fuel and fuel per frame, and 84 skin-preview screenshots (14 skins,
  both backends, three beats) were identical pixel for pixel.
- **Additions.** Quads (`ddi_actor::UNTEXTURED`; Lua's `Def.Quad` is now
  StepMania's solid quad, not the built-in square art) and per-corner
  colours (`TweenState.diffuse` is four corners, `Actor.h:460-472`); text
  actors (`Kind::Text`, measured in a pass of their own through
  `ddi_actor::text::TextMeasure`, drawn as `Drawn::Text`); clipping, our
  own frame property, drawn as clip push and pop with a scissor and
  whether it is exact; the virtual screen (`ddi_actor::screen`,
  `ScreenDimensions.cpp:34-55`, plus a viewport to physical pixels);
  `draworder` (a stable sort after the children's `Init`,
  `ActorFrame.cpp:548`) and `SetDrawByZPosition`; shadows
  (`Sprite.cpp:647-656`); `fov` and `vanishpoint` (`LoadMenuPerspective`,
  `RageDisplay.cpp:573-611`), drawn quads carrying projected corners; masks
  and the z-buffer calls as data (`Depth`, `Blend::NoEffect`; `02
  Actor.lua:234-250`). `cullmode` and `SetTextureFiltering` stay ignored.
- **Actors stay a tree.** The Lua stage's map from ids to paths is the
  stable-id layer (re-indexed after a sort); nothing planned outside Lua
  needs ids.
- **Corpus.** Elements, warnings, failures and fuel per frame unchanged;
  load fuel rose slightly on every Lua skin (the sort at `Init`). Drawn
  differently, as StepMania draws them: Pulsen's eight shape skins (their
  receptor frames call `fov(60)`), the seven STARLiGHT-NEXXT skins (mask
  sprites no longer drawn opaque over the notes; quads solid). The bundled
  skins' previews are unchanged pixel for pixel.
- **Left for the renderer** (`render/src/noteskin.rs` approximates them
  meanwhile): per-vertex colour in the textured pass (corner gradients
  are cut into 8 cells a side), perspective-correct quads from projected
  corners (drawn flat from the corners' mean), masks through the stencil
  (sources skipped, destinations drawn whole), blend modes beyond normal,
  add, subtract and modulate.

### Step 2. Drawing a scene outside play

`render` gains the 2D pass over a draw list (P11) and the texture cache
(P9); the limit that drops quads beyond one device buffer in a frame
(`render/src/textured.rs`) goes. The web app gets one canvas and device
for every route (P10), drawn behind the Yew pages while they last. A
developer route draws a test scene (quads, text, clips, blend modes, a
texture loading late).

Verified: the test scene's pixels in headless Chrome on both backends and
in Firefox; a play after visiting other routes still all top tier; a lost
context (`WEBGL_lose_context`) recovers; texture-cache bytes in
`__DDI_DEBUG`.

Done (2026-10-15):

- **One canvas** (`app/src/web/screen.rs`): canvas and device made in
  `main` before Yew mounts (the boot shell waits for the adapter, at most
  4 s); the backend chosen at boot (`gfx=gl` in the address, or the new
  Graphics setting), a route asking for WebGL2 later switching it once for
  the session. Pages that draw attach the canvas into their own element
  (play, skin preview); the 2D pass can show it fixed behind the page. A
  lost device (WebGPU's callback, WebGL2's `webglcontextlost`) is made
  again on a fresh canvas in the old one's place (the old WebGPU device
  destroyed); a play goes back to its start prompt, except on its results,
  which stay until retry; the skin preview rebuilds. wgpu's errors are
  logged, not fatal; nothing configures a lost device. Without a usable
  GPU the drawing pages say so and the Yew pages keep working.
- **Texture cache** (`render/src/texture_cache.rs`): keyed by source and a
  size bucket (64 … 2048), stable ids, queued loads decoded by
  `createImageBitmap` (`app/src/web/textures.rs`), least-recently-used
  eviction under a byte budget, reloaded after a lost device.
- **Text** (`render/src/text_atlas.rs`, `app/src/web/text.rs`,
  `render/src/cosmic.rs`): runs drawn white by Canvas2D on one staging
  canvas into 2048² `R8Unorm` pages with 2 px padding; one upload per page
  and stretch of a shelf each frame (a union rectangle would copy the
  staging canvas's blank parts over earlier runs); budget in pages,
  least-recently-used page cleared whole, never one in use this frame;
  `compact` and `trim` keep pages few; `fillText` is clipped to its cell.
  Runs are drawn at their shown scale (device pixel ratio times the
  transform's, in quarter octaves) and snapped to whole pixels where they
  map one to one; turned runs are sampled. The bundled font is the `FontFace`
  "Iosevka Custom". Natively, cosmic-text with bundled and optionally
  system fonts.
- **The 2D pass** (`render/src/draw2d.rs`, `painter.rs`, `painter.wgsl`):
  quads (textured, coverage, untextured; corner colours; four blend
  modes), text runs, clips as scissors, `External` slots that end the
  pass for the shell to draw into; drawn in encoded values through a plain
  view, WebGL2's surface switched to plain `Rgba8Unorm` while no play
  field is shown. Instance buffers are chunked (`render/src/instances.rs`),
  so the textured pass no longer drops quads beyond one buffer.
- **Checked:** 25 pixel checks of `#/ui-test` with no failure in Chrome
  (WebGPU, WebGL2) and Firefox (WebGL2), also after a lost device (ids
  kept, textures reloaded, text redrawn); a late texture shows its
  placeholder first; scrolling 300 rows a second within one text page
  (two at 2×) costs 0.4–3 ms a frame (4.4 ms at worst on WebGL2), 2,008
  runs every frame 1–2 ms on three pages; a play after the other routes,
  and after a lost device mid-play, all top tier on both backends; skin
  previews identical pixel for pixel; wasm 2,350 KB (+41 KB with step
  1.2). An independent review found eleven defects (among them the
  touch quit button cut off from its handler, waiters dropped when the
  backend switched during start-up, a lost WebGL2 context reconfigured);
  all fixed and checked again.
- **Left:** runs drawn in colour (tinted white only); nothing draws the
  play field into an `External` slot yet; the test scene redraws every
  frame (P13 is step 3); WebGPU in Firefox and Safari unchecked.

Done, actors in both passes (2026-10-15):

- **The bridge** (`render/src/actors2d.rs`) turns actors' `Drawn` into the
  2D pass's list: corner colours, glow as a second quad, text on its line
  box, exact clips as scissors and turned ones as `PushClipQuad`, masks
  and perspective; models are left to the play field.
- **Both passes** (play field and painter) draw corner gradients (quads of
  one colour keep the old path exactly), perspective-correct quads from
  projected corners, and StepMania's masks through a
  `Depth24PlusStencil8` attachment made only when a frame needs it (the
  high bit is the mask, the low seven count turned clips): only a
  `NoEffect` actor writing depth writes the mask (StepMania's z-bias,
  `Actor.cpp:772-775`), `clearzbuffer` clears it whole, and tests follow
  `ZTest`. Every draw drops fragments of alpha 1/256 or less, as
  StepMania's alpha test does (`RageDisplay.cpp:275`,
  `RageDisplay_OGL.cpp:2731`). On WebGL2 a pass is split only before one
  that uses the stencil.
- **Blend modes:** `ddi_actor::Blend` gains StepMania's CopySrc,
  AlphaMask, AlphaKnockOut, AlphaMultiply, WeightedMultiply and
  InvertDest, drawn with its factors (`RageDisplay_OGL.cpp:1869-1934`),
  the target's alpha always kept (AlphaMask and AlphaKnockOut therefore
  draw nothing); Subtract follows StepMania 5.1 (`dst·(1−a) − src·a`).
- **Skins:** STARLiGHT-NEXXT's scroll texture is cut to the arrow by its
  mask and multiplied as in StepMania; Pulsen's shape skins draw their
  blobs in perspective; the bundled skins' previews are unchanged pixel
  for pixel and the corpus reports nothing else changed. `#/ui-test`
  gains a row of the new blend modes and a part built from actors (draw
  order, corner colours, shadows, a clip turned 30°, an `fov` frame, a
  mask with a hole). An independent review found eight defects (among
  them no alpha test outside mask sources and an extra pass per frame on
  WebGL2); all fixed.

### Step 3. `ddi-ui` core, shared state and the desktop shell

- **Core:** events and requests (P2), hit-testing through actors'
  transforms, pointer capture, a gesture recogniser (tap, drag, flick with
  velocity, long press), wheel deltas normalised (lines and pages to
  pixels: Firefox reports lines), menu actions and key repeat (P3), focus
  and the focus ring, a screen stack with in and out transitions, an
  overlay layer for popups and dialogs, the text field model (P5),
  messages (P7), on-demand rendering (P13).
- **Harness:** drives a `Ui` with scripted events and frames and asserts
  on the actors and the draw list, natively.
- **Shared state** out of `app` (P12). `ddi_library::search` folds text
  with the browser's NFKD (`library/src/search.rs`); desktop gets the
  same through `unicode-normalization`, checked against the browser's on
  the search tests.
- **Web:** the canvas takes pointer events (`touch-action: none`, no
  context menu or iOS long-press callout), wheel (ctrl+wheel and pinch
  zoom kept from zooming the page) and keys; the hidden textarea and file
  inputs are created by the shell; on phones the visible area shrinks
  while the keyboard is open (`visualViewport`), safe-area insets apply,
  and orientation can flip at any time.
- **Desktop:** `desktop/` opens a window, feeds winit's events, draws,
  and answers the requests it can: file dialogs through rfd (on Linux with
  the xdg-portal feature, not GTK), the clipboard through arboard, links
  through the OS. File drag and drop where winit has it (not on Wayland).

Verified: the harness; a developer screen with every primitive, driven by
Playwright's mouse, touch and keyboard in Chrome and Firefox and read
back through `window.__DDI_DEBUG.ui`; the same screen in the desktop
window by hand, and drawn offscreen in a native test (wgpu on the
machine's GPU) against the web's pixels; a gesture-bound request (a
sound) from a click, a key and a touch release under
`--autoplay-policy=document-user-activation-required`; Safari by the
maintainer after the push.

Done, the core (2026-10-15; the shells, shared state and the desktop
window follow):

- **`ui/` (`ddi-ui`)**, platform-free, on `ddi-actor`. `Ui::handle(event)`
  returns `Outcome { consumed, requests }` synchronously (`consumed` tells
  the web shell when to `preventDefault`); `update(dt)`, `draw(measure)`
  into `Drawn`, `needs_redraw()` for rendering on demand.
- **Gestures (P2).** Activation as the HTML standard has it: a key press
  but Escape, a mouse press, a touch or pen release (a mouse release
  within 1 s of its press). `Ui::new` says whether gestures are required
  (web) or not (desktop). A gesture-bound request made outside one is
  refused, never queued, and the screen hears `Input::NeedsGesture` so it
  can prompt; queuing let a stale preview or link fire on a later,
  unrelated tap. A long press never yields one, lift included.
- **Pointers.** Hit-testing through actors' transforms (clips, draw
  order, hidden and fully transparent subtrees skipped), capture, and a
  tracker: tap, long press (touch and pen, 0.5 s), drag (slop 10 px touch,
  4 px mouse), flick (300 px/s over 100 ms); at most 0.1 s counted a
  frame, so an idle shell's first frame fires nothing. Wheel lines are 40
  px, three a notch on desktop.
- **Navigation (P3).** Physical codes for navigation (arrows, Enter or
  Space for Start, Escape for Back, Tab for Select); repeats by us,
  StepMania's 0.375 s then 8 a second (`InputFilter.cpp`), per key and per
  pad button, only the last-pressed direction; explicit neighbours, then
  the nearest by edge distance plus twice the cross-axis gap; a plain focus
  ring after key or pad input, hidden on pointer use.
- **Layers.** Screens with StepMania's `On`/`Off`/`Cancel` and `In`/`Out`
  children (a transition lasts the tweens queued at its start), and
  overlays owned by a screen or dialog, drawn right above it and closed
  with it; dialogs trap focus, take Back and report presses outside; a
  layer leaving the input chain has its pointers cancelled.
- **Messages (P7)** play `<Name>MessageCommand` through the actors'
  commands, as noteskins' Lua does, and reach screens with parameters.
- **Text field (P5).** Mirror mode re-reads the textarea (UTF-16 offsets,
  the composition range diffed against the value at `compositionstart`)
  and enforces its character limit by `SetText`; own mode edits by
  grapheme with winit's preedit and commit (control characters dropped).
  The web shell sets `composing` from `isComposing` or key code 229
  (Safari's committing Enter); a blurred textarea or a press outside ends
  typing, keeping the text.
- **Checked:** 25 unit and 23 harness tests (scripted keys, pads, mouse,
  touch, wheel and typing against a menu, a transitioning screen and a
  dialog). An independent review found eleven defects, four reproduced
  (among them the queued requests above, popups outliving their screen, a
  held press reaching a layer under a new dialog); all fixed, with its
  cases kept as tests.

Done, the shells (2026-10-15):

- **Shared state (P12).** `ddi_platform::storage` is a store each shell
  installs (`localStorage`/`sessionStorage` under the same keys on the
  web, `app/src/web/kv.rs`; `storage.json` in the user's config folder on
  desktop, an unreadable one kept as `storage.json.bad`). Settings and
  their migrations, the song list's place and the library views screens
  read live in `ui/src/state/`; the app re-exports them, and old settings
  load as before (checked natively and in Chrome). Search folds with
  `unicode-normalization` natively (`ddi-library`'s `nfkd` feature).
- **Web.** `#/ui-dev` runs the developer screen (`ui/src/dev.rs`, shared
  with desktop) on the canvas: pointer events with `touch-action: none`,
  no context menu or callout, every wheel prevented; keys prevented when
  `consumed`; the hidden textarea (step 0's recipe; state re-read on every
  composition, input, key and `selectionchange`; re-placed when the visual
  viewport moves; pastes flattened to one line); frames only while
  something moves; browser Back closes a dialog, with history entries
  unwound on leaving. Pads are gestures for sound only: a file picker, a
  link, typing or the clipboard from a pad asks for a tap or a key. The
  gamepad poller rests while no pad is connected.
- **Desktop.** `desktop/` (`ditoditoinfinito-desktop`): winit window, the
  painter in a plain format with cosmic-text and system fonts, keys
  mapped to the DOM's names, typing in own mode (IME allowed while
  typing, the caret's area sent as it moves; `Ime::Disabled` only clears
  the preedit), file dialogs, clipboard, links, `ControlFlow::Wait`, no
  frames while occluded. It builds on Debian with no system libraries and
  runs in CI. An offscreen test draws the developer screen on the GPU and
  matches the web's pixels within 2 levels. The window itself waits for
  the maintainer (the screen was locked during the runs).
- **Checked:** Playwright in Chrome (WebGPU, WebGL2) and Firefox: focus,
  dialog, popup, typing and Japanese composition (scripted in Chrome),
  sound from a click, a key and a touch release under the strict autoplay
  policy (refused from a long press), drag and flick with inertia, wheel,
  Back; the Yew pages, old settings and plays (also through a fake pad)
  unchanged. An independent review found eleven defects (among them
  `Ime::Disabled` ending typing after each word on Windows, Back pushing
  history during a dialog's close, pads opening pickers); all fixed.

### Step 4. Widgets

- **Lists:** rows built only for what is visible plus a margin and reused
  (StepMania's wheel does the same), smooth scrolling with inertia and
  snapping, a scrollbar to drag, banners through the texture cache.
- **Buttons, toggles, option rows** (left and right change the value, as
  StepMania's option rows do), a slider for volumes.
- **The text field** with preedit drawn, and a **popup list** under it for
  autocomplete (Up, Down, Enter, Escape; never while composing).
- **Layout frames** (P8) and **graph primitives** (lines, bars, points)
  for results.

Verified: the harness per widget (a list of 10,000 rows scrolls with a
bounded number of actors, text runs and texture bytes); the developer
screen in both browsers and on desktop.

Done (2026-10-15):

- **Look seam (P6, `ui/src/look.rs`).** Widgets ask a `Look` for
  components by name and only fill and move their named parts and play
  commands on them; a built-in dark look answers what a theme does not.
- **Images.** `Cx::texture(source, side)` gives a stable actor texture
  id; the shell maps it to the render's texture cache, so only rows drawn
  ask for images, a placeholder showing until they load.
- **List** (`widgets/list.rs`): rows and variable-height headers, actors
  only for what shows plus a margin (13–17 for 10,000 rows), inertia,
  snapping, wheel and key glides, selection by keys and pads that the list
  follows, tap to select and again to activate, a draggable scrollbar, and
  a wheel style (the selected row on a fixed line).
- **Controls** (`widgets/controls.rs`): button, toggle, StepMania-style
  option row, slider. **Search field** (`widgets/field.rs`) with a
  suggestion popup that never shows while composing, fed by
  `ddi_library::suggest` (prefix, then word start, then anywhere; titles
  before artists before packs; folding and romaji as search has them).
- **Layout frames** (`ui/src/layout.rs`): taffy behind the `layout`
  feature, off in both shells for now: flex and grid cost 77 KB of wasm,
  about twice the estimate, and a hand-made grid serves so far; to decide
  again with the results screen (step 6.1). **Graphs** (`graph.rs`):
  segments, polylines, bars, points, a histogram, a line over time.
- **Widget screen** (`ui/src/dev_widgets.rs`; `#/ui-dev?screen=widgets`,
  `--widgets` on desktop; lists take a component prefix, `SongList.Row`
  falling back to `List.Row`): 10,000 songs under 200 packs (the library's
  own first, with banners), search with suggestions, option rows, a
  toggle, a slider, and a results-like grid whose panels an option row
  changes.
- **Checked:** harness tests per widget; Playwright in Chrome (WebGPU,
  WebGL2) and Firefox, banners loaded only for rows shown; wheel
  scrolling at 0.6 ms a frame on Chrome's WebGPU, 1.1 ms on WebGL2,
  5.3 ms in Firefox (means); desktop drawn offscreen. Suggestions over
  23,400 entries take 1.3–1.9 ms for a first letter and 0.1–0.2 ms as the
  word grows (results narrowed from the last ones). Wasm 2,391 KB gzip
  (2,309 KB before step 1.2). An independent review found five severe
  defects (a glide that never ended past 2 million pixels of list, a list
  left blank after shrinking mid-glide, a panic in a short list's
  scrollbar, a tap that caught a moving list starting a song, the wheel
  style never settling) and ten lesser ones; all fixed, its cases kept as
  tests (`ui/tests/review.rs`). Lists keep the selected item across a
  reload (`ListModel::key`), positions are kept in `f64`, banners are not
  asked for above 1,500 px/s, the popup is an overlay that flips above
  the field near the visible bottom, and pointers reach widgets in their
  own frame's space.

### Step 5. Our theme, first version

A design brief and mockups with the maintainer first: the look is theirs
to choose. Their starting point (2026-10-15): close to today's pages,
clean on a black or dark background, but laid out more as rhythm games
lay out their screens. The brief, with three song select layouts and
the other screens mocked up, is `docs/plans/theme.md` (2026-10-15); the
maintainer chose layout A, Iosevka Term, the pink accent, the blurred
backdrop, the beat pulse and the style tabs beside the ladder. Then the
theme: its tunables file, art with `PROVENANCE.toml` where it is not
ours, components for the screens of step 6, its fonts (P4, P6).

Done (2026-10-15):

- **The theme** (`theme/`, `ddi-theme`): `Infinito`, a `Look` restyling
  every plain component (button, toggle, option row, slider, list rows,
  headers and bar, text field, popup item, panel) and adding the
  screens' own (`Screen.Header`, `Screen.Footer`, `KeyHint`, `Pill`,
  `Link`, `SongList.Row` and `.Header`, `SongInfo`, `StyleTab`,
  `ChartLadder.Row` and `.Empty`, `ChartButton`, `PlayButton`,
  `IconButton`, `StyleButton`, `Results.Judgement`, `Results.Score`,
  `Options.Section`, `.Row`, `.Description`, `.Preview`), its wheel
  curve (the selected row taller on its line, `WheelPlace::y` making
  room), and named colours (difficulties lifted for the dark ground,
  judgements as the HUD draws them). Every colour, face, size, region,
  timing, the backdrop's strength and the pulse are in
  `theme/src/infinito.toml`, which `build.rs` turns into tables (no TOML
  parser in the wasm); a test runs every screen and checks that each key
  is read and each name asked for is there.
- **The screens** (`ui/src/screens/`), theme-agnostic (metrics with the
  mockups' numbers as defaults, components falling back to the plain
  look's), fed by plain data (`screens/data.rs`) that step 6 fills:
  song select (the wheel with pack headers that open and close, every
  song in every style, greyed where it has no chart; style tabs lit with
  their counts, Tab to the next lit one; the ladder or its message;
  Left and Right keep a preferred difficulty, the closest chart picked,
  the easier on a tie as StepMania does; search in the header, the wheel
  showing the matches, suggestions never while composing; hints that
  change with the state), results (Overview and Timing panel sets, the
  judgement table, the offset editor, Song list and Retry) and options
  (sections, rows with choices or a stepper, the description bar, a
  placeholder for the field's preview). Song select has a compact layout
  below 480 CSS pixels of height (`Compact.Below`; phones held sideways
  are 360–430, tablets and laptops 600 and up): the design becomes
  844×390 (`Ui::set_design`), with chart buttons, a Play button, icon
  buttons and no hints; results and options are scaled down there until
  they get their own.
- **Backdrop:** the selected song's background (else its banner) asked
  for at 64 px, so the texture cache decodes it tiny, and drawn
  stretched over the screen five times a little apart at alphas adding
  up to `Backdrop.Alpha` (0.2; results 0.14): the linear filter and the
  copies blur it with no extra pass, the same on WebGPU, WebGL2 and
  desktop; a new one cross-fades in.
- **Beat pulse:** the selected row's and chart's ring brighten on each
  beat from a beat function the screen is given (the developer routes
  count from the display BPM; step 6 reads the preview's time).
  `Settings::reduced_motion` (Auto, On, Off; old settings load as Auto)
  with the web's `prefers-reduced-motion` stops the pulse and the slides
  (the wheel jumps); frames then stop while idle.
- **Fonts:** the web registers Iosevka Term's three weights as
  `FontFace`s from `fonts/` (copied by `index.html`, not in the wasm) when
  a theme screen starts, and pages drawing on demand are woken to draw
  their runs again (`on_fonts_loaded`); desktop loads them, and the HUD's
  face, into cosmic-text under the families styles name
  (`CosmicRasterizer::with_families`: the SemiBold's own family name is
  "Iosevka Term Semibold").
- **Developer routes:** `#/ui-dev?screen=select|results|options`
  (desktop `--screen select|results|options`; the binary opens song
  select when given no screen, `--dev` the developer screen) run the
  three screens over the four bundled songs with their real charts and
  art and five made-up packs (pump-only songs, Japanese titles; no art,
  since a bundled banner on a made-up title reads as a mislabelled
  song), Start showing results and the
  Options link options; `__DDI_DEBUG.ui.theme` publishes what they did.
  Desktop now decodes banners and backgrounds (`desktop/src/images.rs`,
  the `image` crate, PNG and JPEG).
- **Checked:** harness tests per screen (greying, style changes keeping
  the difficulty, packs, the popup never while composing, the pulse and
  idle frames under reduced motion, the compact layout) with the plain
  look and again with the theme; Playwright in Chrome (WebGPU, WebGL2)
  and Firefox at 1280×720 and 844×390; a play still all top tier on
  both backends; desktop's select drawn offscreen
  (`desktop/tests/theme_offscreen.rs`). Work a frame with the backdrop
  and the pulse running: 0.6 ms on Chrome's WebGPU, 1.0 ms on WebGL2,
  5.1 ms in headless Firefox (means); none while idle with reduced
  motion. Wasm 2,460 KB gzip (+69 KB over step 4: the screens, the
  theme, the sample data and the look additions).
- **Review fixes** (an independent review; its probes kept as tests in
  `ui/src/screens/tests.rs` and `theme/src/tests.rs`): the ladder's
  window is computed once and used for drawing, picking and the pulse
  (rows shrink to fit six charts at 720); Start, Left, Right and Tab act
  on the song the selection is on even in the poll that moved it; Enter
  in the search goes to the selected match instead of filtering again;
  the chosen chart survives a new layout; clearing a search opens the
  shown song's pack; a header's Start does nothing while searching; Esc
  says "Clear search" while a search is on; the phone's field hides
  whenever typing stops; only the selected row pulses; the compact
  decision is one function over `Compact.*`; the theme's name tracking
  and the screen's debug state cost nothing in production (tests only;
  published when something changed); font wakers are dropped with their
  page; desktop decodes images under limits (8192 px, 256 MiB) and
  takes only plain relative paths.
- **Look additions** (our extensions to actors, not StepMania's, drawn
  by the 2D pass on WebGPU, WebGL2 and desktop; the plain values take
  exactly the old path, and the 84 noteskin preview screenshots are
  byte-identical): sprites' `cornerradius`, `softedge` (a shadow is a
  soft rounded quad), `outline` (rings that follow the radius) and
  `saturation` (greyed banners are grey), as a signed-distance cut in
  the fragment shader; text's `ellipsis` (squeezed to 0.85 at most, then
  cut with "…") and `TextStyle::tracking` (letter spacing, through the
  canvas's `letterSpacing` on the web, where a browser without it shows
  none, and cosmic-text's on desktop). The theme uses the mockups'
  radii (rows 6, panels 8–10, chips 4, pills round), the selected row's
  shadow, and spaced small capitals.
- **Left:** the field preview in options, results and options laid out
  for phones, square dashes on unlit tabs and the empty ladder, and
  decoding desktop images off the draw path (they are decoded when a
  frame first asks, on the main thread).

### Step 6. Screens, one at a time

Each replaces its Yew view, keeps its route, its URL parameters and the
headless tests that drive it, and is checked by the maintainer in
Firefox.

1. **Results**, first: the smallest screen, and it proves layout frames.
   Today it is a state of the play route (`results_view`,
   `app/src/components/game_canvas.rs`), and the engine keeps only totals,
   the mean and the deviation (`engine/src/player.rs`, `Results`). The
   panels need more, recorded by the engine first, with replay-fixture
   tests: every judgement with its time, offset and lane; the life over
   the song; counts per lane. Then the panels (judgement counts, timing
   histogram, life graph, offsets over the song, per-lane breakdown),
   chosen by the ruleset and the player; the per-song offset editor.
2. **Song select:** packs and songs (accordion or wheel, as the theme
   says), the style selector, search with autocomplete (the matching
   exists in `ddi_library::search`; suggestions, ranked titles, artists
   and packs, are new and go in `library`), previews started in the
   gesture, the import panel (its buttons click the DOM's hidden file
   inputs; drag and drop stays the page's), the storage prompt, keeping
   the list's place.
   - **First, the play history** (the maintainer, 2026-10-15), which
     the best scores need. Every play is kept: its raw data (the chart's
     identity and content hash, the ruleset and options, the seed, the
     offsets in effect, the input log with its timestamps: enough for
     the engine to judge it again exactly) and what was worked out from
     it (score, grade, lamp, counts, the timeline's summary), worked out
     again from the raw data when the engine or a ruleset changes (a
     version on each record). On the web in IndexedDB, on desktop in
     files, behind a `platform` store; a few kilobytes a play, with an
     export. It feeds the best scores on song select and, later,
     replays (phase 10). A plan of its own first
     (`docs/plans/play-history.md`, to write).

   From the maintainer's try of step 5 (2026-10-15):
   - **The selected row's ring is cut on its left edge** (the pink
     outline of the wheel's selected song): fix in the theme, and keep
     a test that the ring lies within what is drawn.
   - **Trackpads:** two-finger scrolling moves the list only when fast;
     slow scrolls do nothing (to find out why; perhaps small wheel
     deltas are dropped before they add up to a row). The list follows the fingers one for one, as a drag, and
     springs back onto a row ("rubber snap") once they stop; a mouse
     wheel's notches keep moving a row each. Checked on a MacBook
     trackpad in Firefox, Chrome and Safari.
   - **Motion blur:** a vertical blur on the rows while the list moves
     fast, growing with the speed, none at rest (and none under reduced
     motion).
   - **Banners:** each kept at a few sizes (mipmaps, or a smaller copy
     for rows), the cache's count and bytes bounded and measured on a
     large library (P9 keys images by the size drawn; a wheel row
     zooming between sizes must not decode a banner twice).
   - **Best scores on the chart ladder**, from the play history.
3. **Options:** option rows, bindings and the bind flow, the volume
   sample, resets behind a confirmation dialog, the noteskin choice with
   its preview drawn in the scene. The preview (the panel beside the
   rows, a placeholder since step 5) plays the demo chart
   (`engine/src/demo.rs`) as play would draw it with the options being
   chosen, changing as they change (the maintainer, 2026-10-15): the
   direction (Reverse), the speed and whether charts keep their own, the
   scroll action, Turn, the appearance (Hidden, Sudden, Stealth), the
   simplified notes, the note colours, the arrows (noteskin) and the
   style chosen, receptor snap, the field filter and the timing error,
   and anything later added that changes how the field looks.
4. **The rest:** calibration's menu, credits (links open through a
   request), the app-wide notices (device changes, storage), and the play
   screen's DOM parts: the start prompt, the touch quit button, and
   Dancing☆Onigiri lyrics (decision 29), which move into the scene once
   P4 draws Japanese.

Done, 6.1 results (2026-10-15):

- **The engine** keeps every judgement in play order with its time,
  lane and offset, and the life after each change
  (`Results::timeline`, `Results::life`, `Results::taps_by_lane`; a
  replay test), and a gauge says where its danger line is
  (`GaugeRules::danger_line`).
- **The mapping** (`ui/src/screens/play.rs`, natively tested): counts from
  the ruleset's tally (a per-row ruleset counts a jump once), panels from
  the timeline; the ruleset's tier names (tiers it does not have, named
  "—" or empty and never judged, left out: DDR A and Dancing☆Onigiri
  have no fifth), holds under its names (O.K. / N.G.), hold ticks when
  ticked, mines, stray presses when any, max combo, fast and slow, the
  mean and σ; the score as money, percent or "—"; EX where the ruleset
  has it; FAILED; the lamp, with or without "· ASSIST"; the mods with a
  shuffle's order (`mods_summary`, moved here from the app); the
  device-changed and video notes; the per-song offset editor as before
  (a millisecond either way, this play's mean error when worth taking,
  reset; same storage).
- **The screen** (`ui/src/screens/results.rs`) draws them with the
  ruleset's windows colouring the histogram, the gauge's danger line on
  the life graph, misses on the offsets, and the layout's lanes (4, 5, 6,
  8, 10 and Dancing☆Onigiri's, by name when they fit, by symbol when
  not). Panel sets (Overview, Timing, Song) are the ruleset's to offer
  (one list for all today) and the player's to choose, kept in
  `Settings::results_panels`. Keys as the mockup and StepMania's
  evaluation screen have them: Start (Enter, a pad's Start) goes to the
  songs (Song list has focus at first), as do Back, Escape and
  Backspace; R, the Retry button and a pad's Select retry. A pad's Start
  retried on the old card; that moved to Select, a standard pad's Y (new
  `MenuInput::Select`; its Back/Select button stays Back). Not Tab, the
  keyboard's Select: it changes the style on song select, and a key that
  does that must not restart a song here (`Cx::menu_from_keyboard`).
  Menu input and taps in the first 0.6 s, and held keys' repeats, are
  let go, so keys held, taps on a phone's touch lanes or mashing as the
  song ends neither leave nor retry; then Song list has focus again. A
  start right after Retry is not taken (the click a touch makes lands on
  the prompt that replaced the results).
  The grade's box is sized to the grade (no wider than 120 px, the score
  after it, shrunk if it would pass the column), tested for every
  ruleset's grades at both sizes.
- **On the play route**: when a song's play ends, the play's frames stop
  and the screen takes the canvas, the keys and the pads
  (`app/src/components/ui_dev.rs`'s host, embedded: its size the
  canvas's, history the route's, its debug state added to the page's as
  `__DDI_DEBUG.results`, removed when they go). The backdrop is the
  background the play read, kept in memory (`web::textures::keep`) under
  a name of its own for each song loaded (the texture cache keeps what a
  name showed, and a name that failed), never fetched twice. A lost
  device draws the same screen on the new canvas, panels and offset as
  they were; Retry starts over as before. Calibration's results stay a
  DOM card (step 6.4); the new-device card was never shown over results.
- **Review fixes:** images asked for by a frame keep frames coming until
  they are in (read after the draw: a lost device or a resize left them
  unloaded until the next input); the offset suggestion shows while it
  is not the offset set, as the old card offered it; life samples stay in
  time order (a replay test); the histogram's bins are centred on 0; the
  calibration card has its old fields back; the pad poller looks for pads
  by a timer while none is connected, so a page at rest draws no frames
  (2 a second before); `__DDI_LOSE_DEVICE` loses a WebGL2 context too.
- **Layout frames:** taffy stays out. The panels are a grid of two
  columns the screen computes in a few lines; nothing configurable here
  needs flex or grid yet, and taffy costs 77 KB of wasm. To be decided
  again if players arrange their own panels.
- **Checked:** mapping tests per ruleset (names, missing tiers, tally
  against timeline, lamp and assist, failed, money and percent, lanes of
  4, 5, 7, 8 and 10), the screen's actions in the harness; autoplay to
  the end on Chrome WebGPU, WebGL2 and Firefox under ITG (single), SM5
  (double, 8 lanes; pump double, 10 lanes, at 844×390), DDR A (pump
  single, 5 lanes, 140 counted against 155 judged) and Dancing☆Onigiri
  rules, all top tier, and a failed ITG play; Song list by Enter,
  Escape, a pad's Start and a tap; Retry by R, a click and a pad's
  Select; the offset saved per
  song; panels kept; a lost device (WebGPU's hook, WebGL2's lost
  context) and a resize keep the screen. Wasm 2,474 KB gzip (+14 KB).

Yew goes when no screen needs it; `#/video-check`, the crash page and
the no-GPU page stay plain DOM.

### Step 7. Desktop parity for the menus

Settings in files, the library read from folders on disk (no IndexedDB),
file dialogs, system fonts. This joins the rest of phase 9 (audio through
web-audio-api, gilrs, play, IME and file dialogs on Windows and Linux),
planned separately.

## Later

- StepMania themes, gameplay and evaluation decorations first, then
  decorations around our song list, then 3.9 and OpenITG themes where
  cheap (`docs/plans/themes.md`, to write): `metrics.ini` and the fallback
  chain, `BGAnimations` layers, bitmap fonts, the Lua host for screens
  (`GAMESTATE`, `SCREENMAN`, `MESSAGEMAN`; about 300–500 methods). Songs'
  scripted backgrounds and `#FGCHANGES` use the same machinery.
- Embedded targets: a software renderer for the draw list, if a target
  has no GPU wgpu can drive.
- Accessibility (AccessKit), if the game's own play becomes accessible.

## Risks

- **What the browser gave for free goes:** find in page, selecting and
  copying text (credits), screen readers (deprioritized), autofill (not
  needed). Links and the clipboard go through requests; browser zoom
  changes the device pixel ratio, which the scene follows, and a UI scale
  setting comes on top.
- **Text.** Browser-drawn text may measure or look slightly different
  between browsers and from desktop's; step 0 measures it before anything
  depends on it, and the cosmic-text path stays as the way back.
- **IME over a canvas** is the most fragile part; it is tested by hand in
  Firefox and Safari with a real Japanese IME, on a phone, and on each
  desktop platform.
- **Two UI systems during the move.** Each screen is replaced whole; the
  canvas stays behind the Yew pages until the last one goes.
- **Size.** The core, widgets and theme cost wasm; Yew and its styles
  leaving pay some of it back. Measured every step.
- **Power.** Menus that redraw every frame drain laptops and phones;
  rendering on demand (P13) is part of step 3, not an afterthought.
