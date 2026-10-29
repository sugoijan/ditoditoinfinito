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

Status (2026-10-16): steps 0–6 done (results under each step), Yew
removed; step 7 done through `docs/plans/desktop.md`. Decided by the maintainer: our
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
web/      ddi-web      one canvas for the app, DOM events → UiEvent, hidden textarea, file
                       inputs, storage; Yew shrinks screen by screen, then goes
desktop/  ddi-desktop  winit window, wgpu surface, input → UiEvent, rfd, arboard, settings
                       file, system fonts; audio and play come with the rest of phase 9
```

(`ddi-actor` rather than "scene": `crates/render/src/scene.rs` already names the
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
colors, blend mode; a full 3D transform with perspective when an actor
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

- **P1. One actor model.** `crates/noteskin/src/actor.rs` and `commands.rs`
  (StepMania's tween queue, effects, sprite states, commands, eases) and
  the model data types they hold (`crates/noteskin/src/model.rs`: `Geometry`,
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
  already (`crates/web/src/web/image.rs:122`), but with one `createImageBitmap`
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
  `.linear(0.2).zoom(1.2).then().fade_out(0.1)`), its tunables (colors,
  fonts, sizes, timings) in a data file beside its art. Fonts under
  OFL-1.1 are fine (decided 2026-10-15; `AGENTS.md`), as the HUD's
  Iosevka subset already is (`assets/fonts/PROVENANCE.toml`), with any
  subsetting recorded as a modification. On the web a bundled font
  reaches the browser's text drawing as a `FontFace`.
- **P7. Messages, generalised.** Noteskins' Lua already receives messages
  (`crates/noteskin/src/lua/mod.rs`, `HandleMessage`); screens broadcast named
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
  across a device loss. Images keep mipmaps, and a size not loaded is
  drawn from a larger one of the same source when there is one (step
  6.2).
- **P10. One canvas and device for the whole web app**, created at boot
  and kept across routes. Today the play screen and the skin preview each
  call `Gfx::new` (`crates/web/src/web/gfx.rs:151`), which may replace the canvas
  when WebGPU fails (`gfx.rs:124`), and `&gfx=gl` is read per route; both
  become boot-time choices (`gfx=gl` a boot parameter and a setting). The
  first paint waits for the adapter (the boot shell shows meanwhile); a
  lost device or WebGL context is recreated (a new instance, perhaps a new
  canvas) and textures reloaded from their sources; a browser with no
  usable GPU gets a DOM page saying so.
- **P11. Color space of the 2D pass** (settled by step 0's results:
  encoded values for menus). Decision 17
  draws in linear light and blends skins in encoded values, which on
  WebGL2 sends every frame through an offscreen float texture
  (`crates/render/src/offscreen.rs`). Theme art and browser-drawn text expect
  encoded blending, and an offscreen pass every frame costs power on
  phones; menus may draw directly in encoded values.
- **P12. State the screens need moves out of `app`.** Settings
  (`crates/web/src/settings.rs`, `localStorage` through gloo), the song and
  noteskin libraries (`songs.rs`, `noteskins.rs`) and the song list's
  place (`menu.rs`, `sessionStorage`) become plain data behind `platform`
  traits, with the web's storage keys unchanged so players keep their
  settings; desktop stores them in files.
- **P13. Render on demand.** rAF stops when nothing moves, but the pad
  poll keeps running while a pad is connected and wakes a frame on an
  edge; on desktop `ControlFlow::Wait`, with gilrs on a thread waking the
  loop through an `EventLoopProxy`. A screen with something to do later
  while nothing moves (the editor's autosave) gives a deadline
  (`Screen::wake_at`) instead of frames until it: the idle shell sets a
  timer (`Ui::idle_wake`; desktop's `WaitUntil`, a timeout on the web)
  and the UI's clock moves on by the time idle, so the frame meets it.

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
    white text differs from text drawn in its color by up to 53 levels
    (browsers adjust contrast per color); tinting is acceptable for our
    theme, drawing in color stays possible for an exact match.
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
   `crates/noteskin/src/lua/stage.rs`; an arena with stable ids if screens need
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
  colors (`TweenState.diffuse` is four corners, `Actor.h:460-472`); text
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
- **Left for the renderer** (`crates/render/src/noteskin.rs` approximates them
  meanwhile): per-vertex color in the textured pass (corner gradients
  are cut into 8 cells a side), perspective-correct quads from projected
  corners (drawn flat from the corners' mean), masks through the stencil
  (sources skipped, destinations drawn whole), blend modes beyond normal,
  add, subtract and modulate.

### Step 2. Drawing a scene outside play

`render` gains the 2D pass over a draw list (P11) and the texture cache
(P9); the limit that drops quads beyond one device buffer in a frame
(`crates/render/src/textured.rs`) goes. The web app gets one canvas and device
for every route (P10), drawn behind the Yew pages while they last. A
developer route draws a test scene (quads, text, clips, blend modes, a
texture loading late).

Verified: the test scene's pixels in headless Chrome on both backends and
in Firefox; a play after visiting other routes still all top tier; a lost
context (`WEBGL_lose_context`) recovers; texture-cache bytes in
`__DDI_DEBUG`.

Done (2026-10-15):

- **One canvas** (`crates/web/src/web/screen.rs`): canvas and device made in
  `main` before Yew mounts (the boot shell waits for the adapter, at most
  4 s; since 2026-10-22 also for the theme's faces, preloaded beside the
  wasm, each moving the bar, at most 8 s); the backend chosen at boot (`gfx=gl` in the address, or the new
  Graphics setting), a route asking for WebGL2 later switching it once for
  the session. Pages that draw attach the canvas into their own element
  (play, skin preview); the 2D pass can show it fixed behind the page. A
  lost device (WebGPU's callback, WebGL2's `webglcontextlost`) is made
  again on a fresh canvas in the old one's place (the old WebGPU device
  destroyed); a play goes back to its start prompt, except on its results,
  which stay until retry; the skin preview rebuilds. wgpu's errors are
  logged, not fatal; nothing configures a lost device. Without a usable
  GPU the drawing pages say so and the Yew pages keep working.
- **Texture cache** (`crates/render/src/texture_cache.rs`): keyed by source and a
  size bucket (64 … 2048), stable ids, queued loads decoded by
  `createImageBitmap` (`crates/web/src/web/textures.rs`), least-recently-used
  eviction under a byte budget, reloaded after a lost device.
- **Text** (`crates/render/src/text_atlas.rs`, `crates/web/src/web/text.rs`,
  `crates/render/src/cosmic.rs`): runs drawn white by Canvas2D on one staging
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
- **The 2D pass** (`crates/render/src/draw2d.rs`, `painter.rs`, `painter.wgsl`):
  quads (textured, coverage, untextured; corner colors; four blend
  modes), text runs, clips as scissors, `External` slots that end the
  pass for the shell to draw into; drawn in encoded values through a plain
  view, WebGL2's surface switched to plain `Rgba8Unorm` while no play
  field is shown. Instance buffers are chunked (`crates/render/src/instances.rs`),
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
- **Left:** runs drawn in color (tinted white only); nothing draws the
  play field into an `External` slot yet; the test scene redraws every
  frame (P13 is step 3); WebGPU in Firefox and Safari unchecked.

Done, actors in both passes (2026-10-15):

- **The bridge** (`crates/render/src/actors2d.rs`) turns actors' `Drawn` into the
  2D pass's list: corner colors, glow as a second quad, text on its line
  box, exact clips as scissors and turned ones as `PushClipQuad`, masks
  and perspective; models are left to the play field.
- **Both passes** (play field and painter) draw corner gradients (quads of
  one color keep the old path exactly), perspective-correct quads from
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
  order, corner colors, shadows, a clip turned 30°, an `fov` frame, a
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
  with the browser's NFKD (`crates/library/src/search.rs`); desktop gets the
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
  into `Drawn`, `needs_redraw()` and `wake_in()` for rendering on demand.
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
  ring after key or pad input, hidden on pointer use. A tap a screen does
  not take, on a focusable actor or inside one, focuses and activates
  it; a list (`Focusable::new_list`: Options' rows, a page's column, the
  wheel), whose focus is on one of its own items, is only focused by a
  tap on its empty space (since 2026-10-25: such a tap had pressed the
  focused row's button, Export in Options → Data).
- **The pointer's look (since 2026-10-25).** Over what a tap acts on the
  mouse shows a pointer, over a text field a text cursor
  (`UiRequest::SetCursor`, sent when it changes: the web's canvas
  `cursor`, winit's `CursorIcon`): a screen's `Screen::cursor` (Options'
  choices, buttons and slider; the pages' items; the search fields), else
  a pointer over a focusable actor or a list's item (`Wheel#3`), else the
  default, a list's empty space and a row a tap only focuses included.
  The hover, and with it the cursor, follows what moves under a still
  mouse (a list scrolled, a screen come in). A play's surface is not the
  UI's (2026-10-27: the maintainer saw a pointer over a play at places
  with nothing, song select's items under the prompt that closed):
  desktop sends the UI the mouse's leave as the play starts and keeps
  its moves, presses and wheel from the UI while it runs
  (`Shell::pointer`), so the menus drop their hover and the pointer's
  look with it, and tell it again on the results; the web's host puts
  the canvas's cursor back to the default as it closes, a hidden one
  showing again as the mouse moves over the play (`touch::mouse_moved`).
- **Layers.** Screens with StepMania's `On`/`Off`/`Cancel` and `In`/`Out`
  children (a transition lasts the tweens queued at its start), and
  overlays owned by a screen or dialog, drawn right above it and closed
  with it; dialogs trap focus, take Back and report presses outside; a
  layer leaving the input chain has its pointers cancelled. Screens
  follow each other, never cross (2026-10-26, `docs/plans/theme.md`,
  "Transitions"): a screen pushed from a call waits for the one under it
  to play `Off` (`covering`, then `covered`), which plays `On` again when
  the screen over it leaves; a `Navigate` or `Back` request leaves the
  current screen first and goes out once it has (`out`, the layer kept
  as it ended for the shell to act); the shell's own `Ui::push` is
  immediate.
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
  web, `crates/web/src/web/kv.rs`; `storage.json` in the user's config folder on
  desktop, an unreadable one kept as `storage.json.bad`). Settings and
  their migrations, the song list's place and the library views screens
  read live in `crates/ui/src/state/`; the app re-exports them, and old settings
  load as before (checked natively and in Chrome). Search folds with
  `unicode-normalization` natively (`ddi-library`'s `nfkd` feature).
- **Web.** `#/ui-dev` runs the developer screen (`crates/ui/src/dev.rs`, shared
  with desktop) on the canvas: pointer events with `touch-action: none`,
  no context menu or callout, every wheel prevented; keys prevented when
  `consumed`; the hidden textarea (step 0's recipe; state re-read on every
  composition, input, key and `selectionchange`; re-placed when the visual
  viewport moves; pastes flattened to one line); frames only while
  something moves; browser Back closes a dialog, with history entries
  unwound on leaving. Pads are gestures for sound only: a file picker, a
  link, typing or the clipboard from a pad asks for a tap or a key. The
  gamepad poller rests while no pad is connected.
- **Desktop.** `desktop/` (run by `app/` natively, binary
  `ditoditoinfinito-desktop` until 2026-10-16): winit window, the
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

- **Look seam (P6, `crates/ui/src/look.rs`).** Widgets ask a `Look` for
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
- **Layout frames** (`crates/ui/src/layout.rs`): taffy behind the `layout`
  feature, off in both shells for now: flex and grid cost 77 KB of wasm,
  about twice the estimate, and a hand-made grid serves so far; to decide
  again with the results screen (step 6.1). **Graphs** (`graph.rs`):
  segments, polylines, bars, points, a histogram, a line over time.
- **Widget screen** (`crates/ui/src/dev_widgets.rs`; `#/ui-dev?screen=widgets`,
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
  tests (`crates/ui/tests/review.rs`). Lists keep the selected item across a
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
  room), and named colors (difficulties lifted for the dark ground,
  judgements as the HUD draws them). Every color, face, size, region,
  timing, the backdrop's strength and the pulse are in
  `crates/theme/src/infinito.toml`, which `build.rs` turns into tables (no TOML
  parser in the wasm); a test runs every screen and checks that each key
  is read and each name asked for is there.
- **The screens** (`crates/ui/src/screens/`), theme-agnostic (metrics with the
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
- **Backdrop:** the selected song's background, with a separate Gaussian
  cache entry (`TextureKey::blurred`, `Cx::blurred_texture`, 2026-10-18).
  Decoded at 256 physical pixels long, regardless of display scale, and
  filtered once before its mipmaps through two GPU passes (σ = 6 texels,
  radius 18, clamped edges, premultiplied alpha). A layer then draws a
  single sprite at `Backdrop.Alpha` (0.2; results 0.14); a new one
  cross-fades in. Sharp pictures and other blur sizes never substitute
  for it. The start prompt carries the same cover-fitted base and introduces a
  softly blurred framed copy above it (see the transition below).
  No background means `Play.Field`.
  `__DDI_DEBUG.pass2d.blurred` counts cached filter jobs; idle frames and
  cached revisits add none. Eviction or a lost device regenerates the image.
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
  Desktop now decodes banners and backgrounds (`crates/desktop/src/images.rs`,
  the `image` crate, PNG and JPEG).
- **Checked:** harness tests per screen (greying, style changes keeping
  the difficulty, packs, the popup never while composing, the pulse and
  idle frames under reduced motion, the compact layout) with the plain
  look and again with the theme; Playwright in Chrome (WebGPU, WebGL2)
  and Firefox at 1280×720 and 844×390; a play still all top tier on
  both backends; desktop's select drawn offscreen
  (`crates/desktop/tests/theme_offscreen.rs`). Work a frame with the backdrop
  and the pulse running: 0.6 ms on Chrome's WebGPU, 1.0 ms on WebGL2,
  5.1 ms in headless Firefox (means); none while idle with reduced
  motion. Wasm 2,460 KB gzip (+69 KB over step 4: the screens, the
  theme, the sample data and the look additions).
- **Review fixes** (an independent review; its probes kept as tests in
  `crates/ui/src/screens/tests.rs` and `crates/theme/src/tests.rs`): the ladder's
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
   `crates/web/src/components/game_canvas.rs`), and the engine keeps only totals,
   the mean and the deviation (`crates/engine/src/player.rs`, `Results`). The
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
     replays (phase 10). Planned in `docs/plans/play-history.md`
     (2026-10-16).

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
   (`crates/engine/src/demo.rs`) as play would draw it with the options being
   chosen, changing as they change (the maintainer, 2026-10-15): the
   direction (Reverse), the speed and whether charts keep their own, the
   scroll action, Turn, the appearance (Hidden, Sudden, Stealth), the
   simplified notes, the note colors, the arrows (noteskin) and the
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
- **The mapping** (`crates/ui/src/screens/play.rs`, natively tested): counts from
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
- **The screen** (`crates/ui/src/screens/results.rs`) draws them with the
  ruleset's windows coloring the histogram, the gauge's danger line on
  the life graph, misses on the offsets, and the layout's lanes (4, 5, 6,
  8, 10 and Dancing☆Onigiri's, by name when they fit, by symbol when
  not), and the notes' places in the beat (By beat, 2026-10-25,
  `docs/plans/theme.md`). Panel sets (Overview, Timing, Song) are the ruleset's to offer
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
  (`crates/web/src/components/ui_dev.rs`'s host, embedded: its size the
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

Done, 6.2 from the maintainer's try: trackpads, motion blur, banners
(2026-10-16):

- **Trackpads.** Why slow scrolls did nothing: the wheel style moved
  its target to the row under `target + delta`, and a few pixels stay
  in the same row. Wheel events now say what the platform counted
  (`WheelEvent::source`: the DOM's `deltaMode`, winit's `LineDelta` or
  `PixelDelta`), and `gesture::WheelStream` tells a wheel's notches from
  a precise stream (`Input::Wheel::kind`): `PixelDelta` is precise;
  anything continues an open stream (an event within 120 ms, so a
  swipe's momentum stays one stream however large its deltas, and a
  precision touchpad's whole lines among its fractions too); lines are
  notches when whole, precise when a fraction (Windows' precision
  touchpads through winit, high-resolution wheels); pixels of unknown
  origin are a notch when a whole number of 4.000244140625 (macOS's
  mouse wheels in Chrome and Safari) at any of the browsers' page zooms
  (the delta is divided by the zoom) or 50 px and more (Windows' 100,
  Linux's 53 to 120), unless soon after a notch and neither a multiple
  nor a fraction of it (a touchpad swipe that starts large: 60, then
  90), and start a stream when smaller (a trackpad's first event is a
  few pixels). Wheels with Ctrl or ⌘ (a trackpad's pinch) are the
  page's zoom: the UI leaves them and the web shell lets the browser
  have them. A list ignores sideways wheels (Shift, a tilt). It follows
  a stream one for one, as a drag (a glide under way, a notch's or a
  key's, carries the fingers' moves on); once no event came for 0.12 s
  it snaps as after a drag (the wheel style onto its line, selected).
  Keys, taps and the scrollbar end a stream, so its end does not undo
  them. A notch moves a row in the wheel style (a few for a large
  delta) and 120 virtual pixels plain, whatever the platform calls a
  notch. A stream of one or two events moving under half a row, which
  no trackpad gesture is, counts as a notch: a mouse whose notches the
  platform reports as a few pixels still moves a row. Dragged or
  scrolled past an end the list gives less and less (iOS's rubber band,
  `d·c·x / (c·x + d)` with `d` half its height, `c` 0.55) and glides
  back when let go; a flick into an end still stops there.
- **Motion blur.** Actors have `motionblur` (our extension, like
  `cornerradius`): what they draw and everything inside them smeared
  along their y axis, in their units. It reaches `Drawn` quads and
  runs as a vector on the target, the bridge turns it into each quad's
  and run's own units (`Quad::blur`, `TextRun::blur`), and the painter
  grows the quad by half the blur each way and averages taps along it
  (at most 2 pixels apart, 16 at most, dithered so long blurs do not
  band, with their alpha; the mipmap a plain sample would take; nothing
  outside the quad). Instances grew to 144 bytes; with no blur the old
  path runs unchanged (the 84 noteskin previews are identical on WebGPU
  and WebGL2). The list blurs its items by `List.MotionBlur` (0.5) of a
  60th of a second's travel past `List.MotionBlurFrom` (600 virtual
  pixels a second), at most a row, from its glide or inertia or, for
  drags and streams, the speed measured over its last frames; none at
  rest (frames go on until the measured speed is gone) and none when it
  does not glide (reduced motion). Cost, the widget list at 2560×1440
  on an M1 Max: 2.0–2.4 ms a frame at rest, 3.1 ms blurred by 22
  virtual pixels, 4.6 ms by 28; flicking 10,000 rows in Chrome (WebGPU
  and WebGL2) and Firefox drops no frame.
- **Banners.** The painter's images have mipmaps (filled at the next
  draw, `mipmaps.rs`; text pages have none), and the texture cache
  resolves a size it lacks to one as large or larger of the same source
  (its mipmaps draw it smaller), waits for one being decoded, and only
  otherwise asks for its own, a smaller one standing in meanwhile
  (`TextureCache::resolve`; a size that failed is drawn from another
  that loaded, and one waits for a larger decode 120 frames at most).
  Mipmaps are recorded after the draw's eviction, and what is evicted
  is dropped, not destroyed, so commands not yet submitted keep it. So a banner is decoded once for a row and
  at most once more for the large panel, and a row zooming between
  sizes decodes nothing again (its key was never the zoom: the size
  asked for is the component's). What stays loaded is bounded by bytes
  (mipmaps counted) and by count (`MAX_LOADED`, 768). Measured over
  10,000 rows, two in three with a banner of their own
  (`crates/desktop/tests/banners.rs`, a budget of 100 banners): a slow
  trackpad scroll through 300 rows loads 188
  and evicts 88, never more than 100 loaded (10.7 MB); a long flick
  loads only where it stops (14). Desktop decodes on two threads
  (`images::Loader`; a decoder that panics fails its image only; at
  most 96 MB a decode), and the shell polls for them while any is out;
  the web already decodes with `createImageBitmap`.

Done, 6.2 song select (2026-10-16):

- **`#/` is our theme's song select** on the app's canvas over the whole
  window (`crates/web/src/components/select_page.rs`, the UI core's host from
  `ui_dev.rs` started as a route of its own), on the real library: the
  bundled manifest and the imported songs (`crates/web/src/select_data.rs`),
  every song in every style, greyed where it has no chart in it, packs
  as the wheel's headers (bundled first, then each imported pack;
  closed or open as left), banners and backgrounds through the texture
  cache. The Yew song list (`song_select.rs`), its DOM navigation
  (`menu.rs`) and its CSS are gone.
- **Charts:** the ladder per style with difficulty (an Edit by its name,
  as Dancing☆Onigiri charts are), meter, notes and step artist; Start
  plays the chart (`#/play?song=&chart=` by its index, with `gfx`,
  `auto`, `bias`, `slow` carried from `#/?…`); the song's BPM and length
  (`ManifestEntry::length`, the end of the last note of the playable
  charts, from `xtask gen-songs` and the import; songs imported before
  get it, and their charts' digests, when their chart is next read).
- **Bests** on the ladder rows (grade, score, the lamp as a word in its
  color and a bar at the row's end; "…" after the score while pending),
  under the wheel rows' meters (a line in the lamp's color) and on the
  compact chart buttons, from the play history by chart digest and the
  best key of the rules the settings choose for that chart (autoplay
  never counts); they follow the history as it changes (a play kept,
  stale plays worked out again).
- **Search** in the header with suggestions (titles, artists, packs,
  kana and romaji); "found in other styles" no longer applies, since
  every style shows every song: the footer says how many songs match
  ("3 songs match.", "No song matches."). Escape closes the
  suggestions, stops typing, then clears the search, back on the song
  shown.
- **Previews:** one audio context for the page, made or resumed in
  every key, click and tap (capture listeners), never later, so Safari's
  and the autoplay policies' rule holds; the song shown starts its
  preview about 0.3 s after the selection settles (the music and the
  chart read then), looping, and stops when the page goes. A pad's move
  in WebKit, which counts no pad press as a gesture, waits for a tap.
  The pulse follows the preview's music through the chart's timing
  (`Env::beat`), at the display BPM when silent.
- **Import:** "+ Add songs" (compact: "+") opens a dialog
  (`crates/ui/src/screens/select_dialogs.rs`) whose buttons click the page's
  real inputs inside the gesture (a folder through `showDirectoryPicker`
  where it exists, else the `webkitdirectory` input; zips and files),
  with the import options as toggles, the status, and the report in a
  scrolled list; drag and drop anywhere on the page as before. Manage…
  (and Delete) on an imported song or pack: read the folder again,
  remove the song or the pack, each removal confirmed in the dialog.
- **Notices** in a line over the song column: imported songs
  unavailable, stored songs that could not be read (remove them), a
  controller without bindings (bind it in Options), the storage prompt
  ("This site stores about … keep them", asked in the gesture). The
  mods ("Playing with") open the options; the footer's links are
  Add songs, Options and Calibrate (through requests; since 2026-10-22
  the plays are P's alone, the credits the start menu's, the source a
  link at the top of the credits; Options' footer has Calibrate too).
  Fewer than ten songs show as one list, without pack headers
  (`PACKS_FROM`). No "demo"
  tag: the title says Dito Dito Infinito as the theme's header has it.
- **The ring:** the wheel's rows are drawn 5 px in (`ROW_INSET`, the
  metric `Select.Wheel.Inset`) and the list starts as far out, so the
  selected row's ring and pulse lie within the list, which clips;
  `the_selected_rows_ring_lies_within_the_wheel` checks every frame of
  the glide and both layouts.
- **A flick leaves no gap:** the wheel selects what is on its line only
  at rest, so while rows rush past (faster than 6 rows a second) or the
  selected row is off the line, its room on the line closes and the row
  shrinks (`Look::set_wheel_open`), both opening again as it settles
  (`a_flick_leaves_no_gap_on_the_wheels_line`). The wheel's events carry
  their source (the host's, shared with `#/ui-dev`).
- **The place** (`ddi.menu.v1`: the song, its chart, the style, the
  search, the packs closed) comes back after a play and a reload; the
  style chosen is the settings' too. Since 2026-10-20 browsing also
  persists in local storage across launches on both shells. Search stays
  per session and does not overwrite that browsing position. Only one
  pack expands at a time outside search; saved lists with multiple open
  packs favor the selected song's pack. Desktop waits for its initial
  library scan before restoring an imported song.
- **Checked:** harness tests on library data (the shell told the song
  and the place within the input; the place back; bests, pending; new
  data keeping the song; the search's count; links and notices; the
  import dialog's calls, progress and report; removal confirmed); theme
  tests (the ring, the bests' colors, the footer's hints clear of its
  links, the dialogs at both sizes); Playwright in Chrome (WebGPU,
  WebGL2) and Firefox: the bundled library, styles and greying, a typed
  search and a composed Japanese one (Chrome), a preview starting with
  the first key under `document-user-activation-required`, play from
  the screen and back with the place kept, a reload, a folder and a zip
  imported, the storage prompt, a counted play's best on the ladder, a
  pack removed after its confirmation, 844×390 by touch, and no frames
  at rest under reduced motion.
- **From the review** (the old page's features back, and its defects):
  search covers transliterations (`#TITLETRANSLIT`, `#ARTISTTRANSLIT`)
  and credits as well; each pack header says "N of M songs" while
  searching; the song's offset and credit beside its artist, "–" for an
  unknown length; named charts by their names on the ladder and the
  play button ("PLAY HYPER EDIT"), their meter shown only when they
  have one, counted on the wheel's row as "+N"; style tabs that do not
  fit are windowed around the chosen one ("‹", "›"); a Dancing☆Onigiri
  chart's best under the key its mode and excess play under
  (`Settings::best_key_for`); a removed song gives way to its
  neighbour; nothing is removed or read again while an import runs;
  files dropped open the import dialog with its progress; "remove
  them" for unreadable songs is confirmed; the notices line opens every
  notice in full; the import report wraps its lines over text rows; a
  toggle takes a tap across its row, and keys go down the dialog (the
  report takes Up and Down only while focused, and lets go at its
  ends); Manage's buttons wrap; ⌘ Backspace (Ctrl elsewhere) opens
  Manage too, and Backspace alone is Back, as Escape (the maintainer's
  call: Back where a Mac's delete key is; typing, it deletes); a pad's
  move in WebKit still chooses the preview (it starts at the next
  tap); the device banner moves the screen below it (`Host::avoid`,
  a `MutationObserver` on the page); a window narrower than 600 CSS
  pixels gets the compact layout (`Compact.Narrower`); testing
  parameters stay for the session's plays and the way back
  (`router::keep_testing`). Typing in a 5,000-song library takes
  11–38 ms a key in Chrome (84–109 before: the page's debug state
  lowercased every song each frame); at rest the screen draws at the
  display's rate while the pulse runs, none under reduced motion.
- **Pack covers** (2026-10-20) appear on the wheel's headers: the web
  reads the images already kept by imports; desktop discovers the same
  pack banners in folders and zips and keeps their root-relative paths
  in library cache version 6. Headers without artwork remain text-only.
- **Left for later:** the device watcher's notices stay DOM (6.4); a
  Dancing☆Onigiri work was not imported in the browser test (the import
  path is the one 5.x tested).

Done, 6.3's field preview (2026-10-16):

- **What it is** (`crates/render/src/field_preview.rs`, platform-free): each
  frame it plays the demo chart (`crates/engine/src/demo.rs`, now playable
  with any play options and ruleset, `Demo::with_options`) and draws the
  frame as play draws it, HUD and all (life bar, score, combo, the
  ruleset's judgement names, the timing error), with the play
  renderer, into a texture of its own. The 2D pass draws that texture
  as an image under the source `field-preview`
  (`Painter::set_external_image`: by source, whatever size the sprite
  asks for, never loaded by the platform, loads already asked for
  cancelled), so no pixel goes through the CPU. The options screen's
  sprite `Preview.Field` shows it, in its place in the draw order (under
  a dialog's dim).
- **How it is drawn:** as on an sRGB target (linear light), offscreen in
  half floats, skins' quads blending as StepMania's, and encoded into a
  plain `Rgba8Unorm` texture at the end (`SkinBlending::Encoded`, the
  WebGL2 play path's offscreen with the final encode the plain runs
  already had), so one path serves WebGPU, WebGL2 and desktop; without
  half floats the device renders and blends, straight into the plain
  texture, blending in encoded values.
- **What it follows** (`FieldPreviewOptions`, built from the settings by
  the shell as play builds its own, `crates/web/src/field_preview.rs`): the
  style's layout (pump and Dancing☆Onigiri lanes), speed and Reverse,
  Boost/Brake/Wave, Turn, Cut and the simplifications, Hidden, Sudden,
  Stealth, receptor snap (the player's `PlayOptions`, the demo played
  with them), the ruleset (its windows judge the demo's hits, its names
  show), the note colors, the field filter, the timing error, and the
  background's brightness and frame. The demo has no song background,
  so the preview shows a plate of ours (a dark 16:9 gradient with soft
  light bands, `field_preview::plate`) at the chosen brightness, fitted
  into the chosen frame (Auto fits the plate's 16:9), so brightness,
  frame and filter all show; the panel stays dark at brightness 0. The
  demo has no speed of its own, so "charts with their own speed"
  changes nothing here (as on such a chart in play); offsets are not
  applied. The arrows are the noteskin play would draw the layout with
  (`Settings::play_skin`, Dito in the note colors' variant without a
  choice), loaded as play loads it (`noteskins::prepare` with the
  demo's Lua environment, in slices, off the frame) and shown from the
  frame after it is in; the last one shows meanwhile. A change shows
  at the next frame; the demo is played again (from the same point of
  its loop) only when what it plays changes; a skin loads again only
  when its key, the layout or the direction changes.
- **The shell** (the web host, `ui_dev.rs`, for `#/options` and the
  developer routes):
  `UiRequest::FieldPreview(Some([x, y, w, h]))` makes a
  `FieldPreviewHost` on the device (again after a lost one), takes the
  settings (handed over by the options page as they are edited,
  `ui_dev::field_settings`; read from storage every 0.4 s on the
  developer routes), and records the preview's
  frame at `w × h` canvas pixels into the frame's encoder before the 2D
  pass; frames go on while it shows, and stop with `None`.
  `__DDI_DEBUG.ui.fieldPreviewDrawn` has `{frames, status, skin, layout,
  demoTime, texture}`.
- **Checked:** native tests (options, the plate, the demo with options,
  the cache's cancel); `crates/desktop/tests/field_preview_offscreen.rs` on the
  GPU (Reverse puts the receptors at the bottom, Hidden blanks the notes
  near them and not further, Mirror swaps the lanes' notes, the
  background's brightness and the filter, the 2D pass drawing the
  texture pixel for pixel and asking nothing of the platform). In the
  browser on `#/ui-dev?screen=options`, Chrome WebGPU, WebGL2 and
  Firefox: seven option sets (Dito, Reverse with Hidden at ×2.5, the
  built-in arrows in quantized colors with the filter and the timing
  error, CF_CHROME with Boost in a 4:3 frame, pump single, 7 keys with
  Mirror and Wave, DDR A with Sudden and Cut), a live change, a lost
  device; the 84 noteskin previews identical; beatucada autoplayed all
  top tier. Frame cost of the page with the preview (344 × 500) against
  song select's: Chrome WebGPU 1.0 ms against 0.6, WebGL2 1.6 against
  1.1, at 60 fps; headless Firefox (WebGL2) 9 against 5 ms. In
  headless Firefox the preview's own share is about 5 ms (options with
  the preview's frame skipped: 6 ms): not the half floats or the encode
  pass (the direct path into the plain texture costs the same, 10
  against 11 ms), nor the skin (the built-in arrows cost the same);
  about 2 ms of it was the HUD's text, the judgement's pop drawing it
  at a new size each frame (glyphon shapes and rasterises each size),
  which play paid too. Fixed for both: the pop's overshoot goes in half
  pixels (`scene::popped`), a few sizes shaped and rasterised once and
  cached, the resting size exact. Not by scaling one raster: glyphon
  samples its atlas pixel for pixel at whole-pixel places, so a scaled
  raster would show its pixels. The look is the same
  (`crates/desktop/tests/hud_text_offscreen.rs`: the same box within a pixel
  and the same ink within 4 % at five points of a pop; a second pop
  shapes nothing). Shapings a second in a beatucada autoplay
  (`__DDI_DEBUG.text_shaped`): 31.7 → 6.6 in headless Firefox at 120
  frames a second, 17.1 → 4.6 in Chrome at 60; the preview's page in
  headless Firefox 10 → 8 ms. Play's pacing in headless Firefox has
  headroom either way (8.4 ms mean, p99 10.3, at 120 Hz); to be read
  off the debug overlay in a real Firefox.
- **Desktop** (`crates/desktop/src/field_preview.rs`, `Gpu::field`): the
  window answers `UiRequest::FieldPreview` the same way, with the
  settings read from `storage.json` when the panel shows (the options'
  edited ones, on the layout picked for the preview, while the screen is
  open), frames going on while it does; the built-in arrows only until
  desktop loads noteskins (step 7). On both shells (2026-10-27) the
  preview draws nothing until the skin wanted is in, readied for the
  layout wanted, the last frame holding while another layout's loads:
  never the built-in arrows on the way; P on Options cycles the
  preview's layout from anywhere (Shift+P backwards). `cargo run -- --screen options`;
  `crates/desktop/tests/options_offscreen.rs` draws the screen with the
  preview (Reverse moving the receptors to the panel's bottom).

Done, 6.3 options (2026-10-16):

- **`#/options` is our theme's options screen** on the app's canvas
  (`crates/web/src/components/options_page.rs`, the same host as song select),
  on the player's settings: every setting the Yew page had, in nine
  sections (Scroll, Notes, Rules, Timing, Sound, Display, Arrows,
  Controls, Data: History and Reset merged on 2026-10-16, with a
  "Reset play history" asked first), saved to `ddi.settings.v1` as each changes,
  through the same `Settings` and its migrations. The Yew page
  (`options.rs`), its bind flow (`bind_flow.rs`) and their CSS are gone;
  calibration and credits stay Yew until 6.4 (Options' Calibrate and
  the autoplay demo go to their routes).
- **Arrow colors** (2026-10-18): Note colors and Songs' own colors
  live directly under the Arrows choice, shown for Dito and Built-in
  only. Other skins supply their own colors; changing skins keeps the
  saved color choices. Lyrics stays in Display.
- **Preview layout** (2026-10-18): the layout selector sits directly
  above the preview on every tab, separate from Arrows' rows. Tap its
  arrows, press P to focus it, or move Down past the section's last row;
  Left/Right change the layout and Up/Down return to the options. It
  affects only the preview and is not saved as the play style.
- **Pure and shared:** `crates/ui/src/screens/options_model.rs` makes the rows
  from the settings and what the platform knows (`World`: the
  noteskins, the controllers connected, the library's counts, the
  volume sample) and applies each change; `options_shell.rs` is the
  screen's shell for any platform, opening the dialogs and handing the
  rest to an `OptionsPlatform` (the sample, the export, removing songs
  or a noteskin, going elsewhere); `options_dialogs.rs` has the
  confirmation, the guided binding and taking one key. Desktop can use
  them as they are (step 7).
- **Rows:** choices underlined, or stepped one at a time with ◀ ▶ where
  they do not fit the row (Ruleset, the gauge, the noteskin, the note
  colors); numbers stepped (speed by ×0.25, offsets by 1 ms, volume
  by 1 % of the slider, background brightness by 5 %); rows of buttons
  (`OptionKind::Actions`: Left and Right move between them, Start or a
  tap presses one); rows that only say something (a controller's lanes,
  "1 play kept"). The rows scroll: keys keep the focused one in view,
  a wheel or a drag moves them. Headings group them (SIMPLIFY, KNOWN
  DEVICES, KEYBOARD, each controller by name).
- **Bindings:** the layout whose bindings show (StepMania's and the
  library's), "Any device" (the first press picks it), the keyboard
  (bind, reset its keys, per lane: add a key, clear), each controller
  connected or saved (what it plays: saved, defaults, one pad of a
  double; bind, clear, use defaults, forget; per lane, where it plays
  the layout with a table of its own: add a button, clear, 2026-10-22).
  A controller's Add button takes the next button, stick or hat
  direction that controller presses (`KeyDialog::for_pad`), moved from
  any other lane of the table as a key is; its Start is refused (Start
  and Back act before any lane), its Back, another controller's Back,
  Escape or holding a button cancels, and a key asks for the
  controller. The bind flow
  (`crates/ui/src/state/bind.rs`, natively tested) is the Yew one's: each lane,
  then a controller's Start and Back (skippable), a press released
  before the next, one device only, a control used twice or a lane of
  another style's table refused with the reason, opposite ends of one
  axis warned about, a live test that lights what is held; Enter (or
  the controller's Start) saves, Escape cancels. It takes keys and
  controls as they are: a screen that says so (`Screen::raw_input`)
  gets `Input::Raw` for every key (none with Ctrl, Alt or Meta, no
  function keys), and the shell sends controllers' edges as
  `UiEvent::Raw` while `Ui::wants_raw` (`ui_dev::pad_edge`).
- **Resets** (settings; imported songs, noteskins and shared images;
  everything) and removing an imported noteskin ask first in a dialog
  whose first button keeps everything. **The volume sample** starts
  inside the press (an audio context made there) and follows the
  volume; **the play history's export** downloads as before.
- **The field preview** (above): the panel's sprite `Preview.Field`
  over the placeholder; the screen tells the shell where it is in
  surface pixels when it shows and when it moves
  (`UiRequest::FieldPreview`), `None` when it leaves; the page hands the
  edited settings over at once.
- **Layouts:** the full design (sections down the left, rows, the
  preview, what the focused option does); a phone held sideways
  (844×390: a tab strip, rows, a narrower preview, one line below); a
  phone upright (390×844: the layout 480 wide zoomed to the width,
  rows, what the option does, the preview below). The chosen tab is
  kept in the middle of the strip.
- **Fixed on the way:** a pop of the browser's history that leaves the
  host's route is a navigation away, not Back (Firefox fires `popstate`
  at once when the hash is set: Back went to the screen being left and
  the listener ran inside itself); the device banner is watched by
  both canvas pages (`ui_dev::watch_avoided`), so the screen follows it
  as it changes size.
- **Checked:** the model (every row's every value applied and shown
  again, ids unique, keys moved between lanes, controllers bound,
  resets keeping songs' offsets), the bind flow, harness tests on the
  shared shell with a fake platform (rows saved; the sample in the
  press; the export; Back and `FieldPreview(None)`; the preview told
  once and again when it moves; the keyboard bound by keys without
  moving the focus, Escape cancelling; a controller bound by its own
  presses, its new Start saving; resets confirmed, Cancel first;
  scrolling to the focused row, by wheel and by drag), a theme test of
  every row at 1280×720, 844×390 and 390×844. Playwright in Chrome
  (WebGPU, WebGL2) and Firefox: speed, Reverse (a tap), Turn, a bundled
  noteskin and the volume changed on the canvas and seen in a play
  (Reverse and the skin drawn, "mirror" on the results); the keyboard
  bound by keys and a fake controller by its presses, its new button
  playing Up in a play; a reset cancelled, then confirmed; the export
  downloaded; tabs, choices, buttons and dragging by touch at 844×390
  and 390×844; a standard pad's d-pad, A, Y and B; the preview dimmed
  under both dialogs; seven option sets and a live change in the
  preview.

Done, 6.4 part one (2026-10-16):

- **Pages to read** (`crates/ui/src/screens/page.rs`): headings, paragraphs,
  links, buttons and folded sections in one column that scrolls (keys go
  from one link or button to the next and scroll it into view, Page Up
  and Down, Home and End; a wheel or a drag), under the theme's header,
  or a back icon on a compact screen; laid out as options are on a
  phone upright (480 wide, zoomed). Links open through
  `UiRequest::OpenUrl` from the press; a link's hit area lies over its
  text.
- **Calibration's menu** at `#/calibrate` (`crates/ui/src/screens/calibration.rs`,
  the page `crates/web/src/components/page_routes.rs`): what the three tests do
  and a button to start each (the run is still the play route's
  `#/calibrate/run?mode=…`, unchanged, its autoplayer still following
  the physical cue), the default offsets and every device's, and "Reset
  offsets to 0" (asked first: the defaults and each device's offset to
  0, the devices kept as not calibrated), Options. `#/calibrate?auto=1&
  bias=<ms>` carries `auto` and `bias` to the run it starts.
- **The credits** at `#/credits` (`crates/ui/src/screens/credits.rs`, the same
  page): `credits/credits.json` as the Yew page showed it, every source,
  licence and the video decoder's licence texts a link, the configure
  flags and each copied code's notice folded.
- **The app's notices in the scene** (`crates/ui/src/screens/notices.rs`): the
  device watcher's banners ("First time here? …", "New audio path: …
  New display: … No calibration for it yet.", with calibrate, use
  defaults, use another device's offset, later) and its "devices: …"
  line, worded as before (`device_notice`, natively tested), drawn by
  our theme across the top of every canvas page. They are the UI's
  chrome (`Ui::push_chrome`): above every screen and dialog, which the
  UI core lays out below it while it shows (`Screen::reserve`: its
  height; every other layer gets the viewport under it, drawn and hit
  there, and moves back up when it goes), taking the pointers on it,
  no keys or menu actions unless reached: F2, or a fresh Up past a
  screen's top (options' first row, a page's top, the wheel's first
  row and then what is above it on song select; a held Up never gets
  there), moves the menu's focus into its actions (Left and Right
  between them, Start presses one, Down, Back or F2 go back; a hint
  "F2" on the banner); pushed by every theme host (`ui_dev::start_ui`)
  and fed by `crates/web/src/notices.rs` from the watcher, which no longer
  draws anything. Same persistence:
  "later" until a new device shows up, "use defaults" and "use … from …"
  keep the devices, held back in play, calibration and the developer
  routes; a change of devices mid-session probes again as before. The
  DOM banner, its CSS and the canvas's DOM inset (6.2) are gone; the
  room is the UI core's now. Song select's storage notices were already
  the scene's (6.2).
- **Desktop:** `--screen calibration` and `--screen credits` (the
  generated file when `target/credits/credits.json` is there, else a
  sample); no device watcher there yet.
- **The Yew pages** (`calibrate.rs`, `credits.rs`) and their CSS are
  gone.
- **From the review of 6.3:** a controller alone gets out of every
  options dialog (a controller's Back, as the menus read it, cancels the
  key dialog and a binding of another device or of none yet; in review
  the device's recorded or saved Back cancels; holding any controller's
  button a second cancels, `state::bind::Holds`, lanes being tested
  exempt); Space saves a keyboard binding whose lanes took Enter; a
  click on a rail section opens it (the sections had no hit area where
  their plate is not drawn); the browser's Forward into the entry of a
  dialog already closed goes back again quietly, a pop counts as Back
  only when it leaves a dialog's own entry, and navigation leaves the
  handler that asked (`Timeout(0)`: Firefox fires `popstate` at once);
  descriptions wrap over as many lines as the longest needs (the panel
  sized to it; at 844×390 under the preview); under reduced motion the
  preview is one still frame (held at 2.5 s of the demo), drawn again
  when an option changes, no frames otherwise; the preview follows a
  chosen skin let go; "Preview on" shows the arrows on another layout
  (not kept); a speed off the ×0.25 steps shows as stored until
  changed; the banner is thinner on a phone (its buttons beside the
  text when they fit).
- **Checked:** the chrome layer (on top of screens pushed later, taps on
  it only, keys to the screen), the banner's wording and actions, the
  pages (links opened from keys and taps, folds, scrolling, Back), the
  calibration buttons and reset, a theme test of the pages and the
  banner with every action at 1280×720, 844×390 and 390×844. Playwright
  in Chrome (WebGPU, WebGL2) and Firefox: the first-run banner on song
  select and options, "later", a key starting the probe and the new
  devices' banner, "use defaults" keeping them, a `devicechange` with
  known devices showing none, the banner's calibrate leading to the
  menu; the menu at three sizes, its reset confirmed, a run started with
  `auto` and `bias`; the credits at three sizes without overflow, a link
  opening a tab, End and Escape; song select's and options' checks
  again.

Done after 6.4 part one's review (2026-10-16):

- **Past the top, explicitly:** a screen sends the focus up into the
  chrome with `Cx::enter_chrome` on a fresh Up at its true top (options'
  first row, or its tab strip and Back on a phone; a page's top; song
  select's header row), instead of the menu's default moving through
  what lies above first; the screen's focus is kept and given back on
  Down, Back or F2 (`Ui::chrome_return`). A screen that does not ask
  still reaches it where nothing is above. F2 does nothing while a bind
  dialog takes keys raw.
- **Overlays laid out again** when their viewport changes (the banner
  coming or going, a resize): built again, the focused actor lit again
  (`Layer::built`). The IME's caret and the hidden textarea follow a
  layer's shift and scale.
- **Focus looks:** an accent button's ring is drawn inside it in the
  label's color (on the accent it would vanish); the banner lights its
  focused action again when built anew. Its "F2" shows only after a key
  or a pad (`Cx::by_keys`), in room kept for it. On a phone it offers one
  other device's offset, the devices' line lets taps through
  (`Screen::lets_through`), "later" holds for the devices it was said
  for.
- **Links from a pad:** refused (`NeedsGesture`), the page's row and
  song select's footer say "press Enter or tap to open".
- **Small ones:** a commit cut by characters, long words cut in folds,
  `ui.rects` only what is on view (`Ui::visible_bounds`), hold-to-cancel
  counting only buttons pressed alone in the dialog (no axes, none a
  revealed pad reports at once), desktop's credits links opened as the
  checkout's files (`target/…`), its pages without song select's
  preview.
- **The autoplay switch** (song select's header pill, a phone's
  `AutoIcon`): per session, never saved, the address and the plays'
  hashes follow it (the testing parameters' `auto`); on song select only,
  where a play's options are summed up and plays start, not in Options,
  which holds saved settings. Plays started with it are autoplays as
  `&auto=1` is: flagged in the history, not counted, their results say
  so. Removed 2026-10-26 (`docs/plans/theme.md`, "The screens'
  header"): `auto=1` in the address alone. **Results on a phone:** one scrolled column (score, the chosen
  set's panels, the notes) under a bar with Back and the panel sets' tabs,
  the offset and the buttons fixed at the bottom; sideways the score
  beside the panels.
- **Song select upright on a phone** (390×844, 360×740; it had the
  sideways layout scaled to the width, the lower part empty):
  `Layout::upright`, 480 wide zoomed to the width as options and
  results are; the bar on top (Back, the pack, AUTO, +, /, •••), the
  song's banner, title and line, the style button and the chart buttons
  (52 high), the wheel filling the middle (52-high rows, flicked and
  tapped; the pointers' moves scaled by the zoom so it follows the
  finger one for one), the notices' line above Play, and Play (60 high)
  at the bottom within the thumb's reach. Keys and pads as sideways.
  Theme tests at 390×844 and 360×740 (fit, order, sizes in CSS pixels,
  touch, keys, a pad).
- **Wording by platform:** the shell says where the game runs
  (`data::Platform`, in the import view, Options' `World` and the
  credits), never a `cfg` in `ui/`. On desktop the import dialog speaks
  of song folders and zips read in place (no browser options, "Choose
  zip"), Manage hides and forgets ("Hide pack", nothing deleted, reading
  the folder again brings them back), Options offers no noteskin removal
  (skins are read in place) and saves the export to a file, the
  credits' video module is not "served"; a test checks no desktop
  screen says "browser", "this page", "site" or "download".
- **The maintainer's notes:** Backspace on song select is Back (as
  Escape; typing, it deletes), Delete or ⌘/Ctrl Backspace manages; the
  volume is a slider (dragged, tapped, stepped; the sample plays as it
  changes, in the gesture); History and Reset are one "Data" section
  with "Reset play history" (asked first, Export offered); the pointer
  hides on keys and pads; a song without a background shows the plain
  backdrop (no blown-up banner).

Done, 6.4 part two (2026-10-16):

- **One start prompt for both shells** (`crates/ui/src/screens/prompt.rs`,
  dressed by the theme): the song, its artist, the chart in its
  difficulty's color, AUTOPLAY, the mods it will play with (the speed,
  Reverse, every option off its default, `mods_line`) over its picture;
  steps the shell sets through a shared `PromptState` (its `version`
  rebuilds the screen): loading, ready (Start, Back; Enter, a pad's
  Start or a tap anywhere but Back starts), preparing audio, the linked
  folder to allow, a new device (each calibrated device's offset
  offered, "Calibrate now", "Play with defaults" focused first, as a
  pad's Start played on), interrupted (Retry, Back), an error; lines
  under it (the video decoder's download, "this browser needs a tap…"),
  the quit hint for keys or touch, key hints in the footer. Choices go
  to the shell inside the input that made them (`PromptFn`, with
  `Cx::in_gesture` and the new `Cx::menu_from_pad`): on the web the
  audio context is made and resumed there, and the folder asked for
  there, with no `await` before.
- **Its picture is the prompt's own**, drawn through the 2D pass from a
  file the shell keeps: the opening still, else the background (the
  web keeps both under the play's background name), cover-fitted into
  the background frame (the play's automatic shape too,
  `GameLoop::prompt_frame`), dimmed by the backgrounds' brightness, a
  blurred, dimmer copy over the rest as the play's extension, and the
  field filter's band where the chart's lanes will be
  (`ddi_render::scene::filter_band`, shared with the play). The 2D pass
  cannot draw over the play's sRGB surface on WebGL2, so the play's
  renderer no longer draws a prompt (`GameLoop::draw_prompt` is gone);
  while the prompt shows, the play's loop idles (`GameLoop::idle`:
  measuring the refresh rate, uploading backgrounds, no frames drawn).
  Shaped for the transition from song select (`docs/plans/theme.md`):
  its cached base stays visible during loading. A second copy contracts
  from full screen into the frame over 450 ms, retaining a soft Gaussian
  blur at up to 1024 pixels. Start finishes the unblur over 300 ms in the
  play renderer, continuing from `PromptState.reveal` when started early.
  Reduced motion skips both animations. Geometry and progress live in
  `ddi_actor::background`; audio starts in the original gesture.
- **The web's play route** shows it as it shows the results
  (`ui_dev::embed`), keys and pads to it while it shows; the DOM prompt,
  its folder, new-device and interrupted cards and their CSS are gone.
  `__DDI_DEBUG.prompt` (`step`, `notes`, `picture`, `frame`, `reveal`) and
  `prompt_backdrop` say what it shows; `#/ui-dev?screen=prompt` shows
  it on sample data.
- **The touch quit button is the scene's** (`scene::quit_button`,
  `Renderer::set_quit_button`): drawn over the play at the left (where
  since part three's review), kept clear of the safe area; `web::touch`
  takes presses on it as the cancel control. Since 2026-10-26 it is the
  play's only way out by pointer on both shells: shown throughout on a
  touch screen, and for two seconds after the mouse moves over the play
  (`web::touch::mouse_moved`, desktop's `Feed::pointer_at`); the web's
  "← back" link over the play is gone (PLAN.md decision 68). Lyrics stayed DOM here
  (moved in part three, below).
- **Desktop** swaps its stand-in (`crates/desktop/src/play/prompt.rs`) for the
  shared screen: `PromptScreen::new(env, state, act)`, its picture drawn
  by the screen (the desktop's prompt picture under the UI can go).
- **Checked:** the screen's choices and steps natively (Enter, a pad's
  press counted or not as a gesture, taps, the steps as the shell
  changes them, a new device's offers), the theme at 1280×720, 844×390
  and 390×844 at every step. Playwright: Chrome (WebGPU and WebGL2) and
  Firefox from song select through the prompt to an autoplayed play,
  its results, Retry (the prompt again) and back; manual starts with the
  new-device step in Firefox and by touch at 844×390 in Chrome, quitting
  by a double tap on the scene's ✕; Chrome with
  `--autoplay-policy=document-user-activation-required` (the audio
  running after Enter); a linked song's folder allowed from the prompt
  (an OPFS folder whose permission answers `prompt`, granted only with
  the user's activation); Bad Apple's prompt on its opening still.

Done, 6.4 part three (2026-10-16):

- **The 2D pass over a play, on every backend:** the painter draws to an
  sRGB target too (`Painter::new` with an sRGB format: its colors are
  decoded before blending and the target encodes them again; the same
  colors, the edges of text and soft shapes blended in linear light),
  so the play's loop draws a `DrawList` after the scene: through the
  surface's plain view on WebGPU (blended encoded, as the menus), through
  the sRGB surface on WebGL2. Its own painter (made when first needed,
  with the browser's text), a pass a frame only while something shows;
  the uniform padded to 32 bytes (WebGL2 binds in sixteens; a test
  checks both sides).
- **Dancing☆Onigiri's lyrics in the scene** (decision 29): the track's
  lines (timing, fades, alignment, sizes as before) drawn through the 2D
  pass (`ddi_render::lyrics`): on the work's stage mapped onto the
  screen as its back and mask layers are, in danoniplus's `lblword` box
  (100 px in from either side; 10 px down for even depths, 60 px up from
  the bottom for odd ones), wrapped to it (between words, anywhere in
  Japanese) and aligned in it, in its `#cccccc` with a soft shadow; the
  browser's text on the web (Japanese without a download), cosmic-text
  with the system's fonts for desktop's play to call. The DOM overlay
  (`crates/web/src/lyrics.rs`) and its CSS are gone; `__DDI_DEBUG.lyrics`
  lists the texts drawn.
- **The calibration run's outcome on the canvas:** a card on the start
  prompt's screen (`PromptStep::Card`: a heading, colored lines,
  actions; `PromptChoice::Action`), worded as the DOM card was: the
  offset (and frames at the refresh rate), the hits, ignored, trimmed
  and missed, the verdict, the offsets in effect, the devices, a device
  changed; Save, the next test, Again, Done as before (the autoplayer
  still follows the physical cue). A calibration without an outcome and
  a song's score when the theme's results cannot start are cards too.
- **No Yew:** the web shell's pages are components of our own
  (`crates/web/src/component.rs`: a page's messages handled one after another
  in a microtask after they are sent, as Yew did, its few DOM elements
  made and updated by itself under its root), the router mounts the
  page of the address (made again when its key changes: the route, a
  lost device for the pages keyed by it, the play starting over) and
  the device watcher hears the route by a message. `#/video-check`, the
  crash page and the no-GPU page are plain DOM. The wasm went from
  6,679,767 to 6,582,778 bytes (2,737,061 to 2,696,965 gzipped at -9);
  6,592,906 (2,700,787) with the lyrics and the review's fixes.
- **From the prompt's review:** a play's kept files let go take their
  textures with them (`Painter::forget_textures`; each load has names of
  its own, and sixteen trips had kept 80 MB); the touch quit button sits
  at the left, beside the field where there is room, at the end away
  from the receptors (it sat by the Left receptor at 390×844), the safe
  area read again on a resize; the backgrounds' brightness multiplies
  the encoded values on every target (on an sRGB one the backdrop's
  shaders encode, multiply and decode, where they took the brightness
  to the power 2.2 in linear light: dark tones came out darker than on a
  plain target), so the prompt's picture and the play's first frame are
  the same pixels (`crates/desktop/tests/backdrop_brightness_offscreen.rs`, and
  in Chrome); the shared calibration card (`ddi_session::calibration::
  card`) on the web too.
- **Checked:** the painter's sRGB target on the GPU (a grey and a
  half-transparent white read back as on a plain one,
  `crates/desktop/tests/painter_srgb_offscreen.rs`), the lyrics' layout
  natively; Playwright: a synthetic work (danoniplus's 7-key sample with
  a `word_data` of Japanese and Latin lines, fades, alignment, a line
  break) in Chrome on WebGPU and WebGL2 and in Firefox; every route
  without Yew (song select, options, calibration's menu and a run to its
  card and Again, credits, the developer routes, the skin preview with
  the demo, the video check on a generated clip, an unknown route); the
  earlier prompt checks again (the flow in Chrome and Firefox, a manual
  start by touch, the gesture rule, a linked folder, an import of a
  pump skin).

### Step 7. Desktop parity for the menus

Settings in files, the library read from folders on disk (no IndexedDB),
file dialogs, system fonts. This joins the rest of phase 9 (audio, gilrs,
play, IME and file dialogs on Windows and Linux), planned and done in
`docs/plans/desktop.md` (audio through cpal with our own mixer rather
than web-audio-api).

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


### Shared active decoration (2026-10-27)

The UI asks `Screen::presentation` for its look and motion preference, and
`Screen::active_element` for its one primary target (focus by default).
`Look::active_element` updates the decoration in the actor tree at draw time,
after layout and measurement. Only the current input layer receives a target;
other layers receive None, including when a modal dialog takes focus. The UI
owns an unwrapped clock and requests frames only while the active look needs
them and motion is not reduced. Infinito uses this for its screen-aligned
flowing outline; song selection remains the primary target independently of
chart selection. Visual-only actors (`Actor::decoration`) are drawn normally
but do not take input or enlarge their containing control's bounds.


### Theme pointer overlays

`Presentation` carries the environment's cursor style and logical-pixel size.
`Ui` observes its existing hit test and cursor semantics to drive a separate
`cursor::Spec`. `Look::pointer` supplies visual-only actors, whose named sprite
sources resolve through `Look::image`. The overlay is drawn after all layers,
without entering hit testing, focus, screen bounds or input capture. It uses
CSS-to-virtual scaling; the shell hides the native pointer only for custom
default/pointer shapes. Text and grab variants remain native. A primary
mouse-down snapshots the last painted charge for feedback, while activation
continues through the ordinary release/gesture path. Reduced motion and idle
redraw follow the shared UI policy used by the active outline.
