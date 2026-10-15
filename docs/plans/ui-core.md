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

Status: draft, nothing built. Decided by the maintainer (2026-10-15): our
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

### Step 5. Our theme, first version

A design brief and mockups with the maintainer first: the look is theirs
to choose. Their starting point (2026-10-15): close to today's pages,
clean on a black or dark background, but laid out more as rhythm games
lay out their screens. Then the theme:
its tunables file, art with `PROVENANCE.toml` where it is not ours,
components for the screens of step 6, its fonts (P4, P6).

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
3. **Options:** option rows, bindings and the bind flow, the volume
   sample, resets behind a confirmation dialog, the noteskin choice with
   its preview drawn in the scene.
4. **The rest:** calibration's menu, credits (links open through a
   request), the app-wide notices (device changes, storage), and the play
   screen's DOM parts: the start prompt, the touch quit button, and
   Dancing☆Onigiri lyrics (decision 29), which move into the scene once
   P4 draws Japanese.

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
