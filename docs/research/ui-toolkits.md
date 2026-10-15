# Drawing the menus ourselves: GUI toolkits, game UI, what to reuse (2026-10-15)

Scope: the menus are Yew components over the DOM (about 7,700 lines in
`app/src/components/` and `app/src/app.rs`: song select 1,780, the play
screen's component 1,630, options 1,320, the rest for the bind flow, the
skin preview, the device notices, credits and developer pages; plus a
1,050-line stylesheet). The aims are to draw every player-facing screen
on the GPU instead, so that (1) the game ships its own look, (2) StepMania
themes can be hosted later (`stepmania-themes.md`), and (3) the same
screens run on the web, on desktop and later on constrained or embedded
targets without a WebView. The question is what to build on.
Accessibility is deprioritized (the game itself is not accessible; menus
alone would not change that); Japanese text input must work, a hidden text
field being an acceptable way to get it.

Sizes are the gzip -9 size of a wasm32 build at `opt-level = "z"`, fat LTO
and `wasm-opt -Oz`, measured in scratch crates; a crate measured alone
counts dependencies the app already has (wgpu, cosmic-text), so it
overstates what it would add. The app's own wasm is 2,308 KB gzip -9
(release build of 2026-10-15). A scratch crate with wgpu (WebGPU and
WebGL2), glyphon and taffy measures 1,279 KB.

## 1. Constraints from the game

- **WebGL2 stays mandatory.** WebGPU is in Chrome and Safari; Firefox ships
  it on Windows from 141 and on Apple Silicon macOS from 145/147, not on
  Linux in release. A renderer that needs compute shaders (Vello's main
  renderer) leaves those players out.
- **One renderer.** The play field is wgpu with our own pipelines
  (`render/`); a second vector renderer beside it costs size and a second
  path to keep right on both backends.
- **StepMania's actor model.** Themes are scene graphs of actors with
  per-actor tween queues, blend modes, sprite sheets, draw order and a
  virtual screen (`stepmania-themes.md`); `noteskin/src/actor.rs` already
  runs that model for noteskins.
- **Gestures.** Audio and file pickers must start inside the input event
  that caused them (`AGENTS.md`), so the UI must handle input synchronously
  in the event, not on the next frame.

## 2. General-purpose Rust toolkits

| Toolkit | Findings | Verdict |
|---|---|---|
| egui 0.36.2 + egui-wgpu | On our wgpu device: +671 KB, +1,450 KB with its default fonts. Arrow-key focus; IME events (a hidden input written by us, without eframe). Immediate mode; styling is a theme struct, not arbitrary drawing per widget | Good for forms and a debug overlay; poor for a themed, animated song wheel |
| iced 0.14 | Focus is weak (iced #489), no IME on the web, releases about yearly, master on forked dependencies | Reject |
| Masonry / Xilem | Section 3 | Reject as a base; read its design |
| Makepad, Blitz (beta), GPUI | Each brings its own renderer or platform layer; GPUI's web target is unpublished | Reject |
| Kas, Floem, Vizia, Freya | No web target | Reject |
| Cushy | Dormant | Reject |
| Slint | The royalty-free licence excludes embedded use | Reject |
| Ribir 0.3 | General app GUI with its own painter | Reject |
| fyrox-ui 1.0.1 (MIT, 2026-03-28) | Builds standalone and emits draw commands (rendering is Fyrox's); text through fontdue (no shaping); no IME events in `OsEvent`; 184 crates on wasm, among them `notify` and `copypasta`; 886 KB with no renderer | Reject |
| bevy_ui 0.20 (2026-10-08) | Needs bevy_ecs, app, asset and camera; uses taffy and parley | Reject |
| yakui (git 3b99b90, 2026-10-14; crates.io 0.3.0 from 2024-10-21) | Flutter-style layout, immediate API, gamepad/arrow navigation (`yakui-core/src/navigation.rs`), preedit events, a wgpu backend; no tweens; 670 KB, about +171 KB over cosmic-text alone | Reject as a base; its navigation is worth reading |
| nannou 0.20 | Now built on Bevy | Reject |

## 3. Masonry (checked against linebender/xilem main @ 271a27a, 2026-09-14)

The question was whether to build on Masonry or fork it, rather than write
our own layer. An earlier look said it cannot target wasm; that holds for
the last release only.

- **Releases and activity.** `masonry`, `masonry_core`, `masonry_winit`,
  `xilem`, `xilem_core` 0.4.0 (2025-10-29), nothing since. Commits on main:
  77 in the six months to 2026-10 against 238 in the six before (January
  67, then 46, 27, 1, 6, 10, 1). Maintainers' statements: "We currently
  have bandwidth issues, and Xilem is the project that suffered the most"
  and "radical changes to the Xilem API" ahead (issue #1481, 2025-12-01);
  "The web is a fairly low-priority target for Masonry" (#1451); "WASM
  isn't a primary use-case" (#1848). "Linebender in 2026 Q1"
  (linebender.org/blog/tmil-25) gives no Masonry roadmap.
- **Crates.** `masonry_core` (8.1k lines of code: `RenderRoot`, passes,
  layout, properties, the `Widget` trait), `masonry` (20.6k, widgets),
  `masonry_imaging` (884, backend adapters), `masonry_winit` (1.9k: event
  loop, `accesskit_winit`, clipboard, surface). `RenderRoot` needs no
  winit: the host feeds pointer, text (keys, IME, paste) and window events
  and calls `redraw()`, which returns a layer plan; IME start/stop/area,
  redraw requests and actions come back as signals.
- **Painting.** Widgets paint into `imaging::record::Scene` (crate
  `imaging` 0.0.1, forest-rs/imaging): a Vello-style vector list (paths,
  peniko brushes, glyph runs, blurred rounded rectangles, clips, groups with
  blend, alpha and mask). Layers can be `External` placeholders the host
  draws itself. Backends: Vello, vello_hybrid, vello_cpu, Skia; on wasm32
  only vello_cpu is enabled. Main pins wgpu 28, vello 0.8, parley 0.8 (we
  are on wgpu 30). Drawing its output on WebGL2 means writing an adapter to
  vello_hybrid 0.2 (which has a `webgl` feature) or a vector rasterizer of
  our own.
- **Wasm, measured.** Masonry from git main with default features off,
  a Flex with a Label, a Button and a TextInput, an `Ime::Commit("ジャン")`
  and a `redraw()`: builds for wasm32 and runs (in Node via wasm-bindgen
  0.2.129, vello_cpu drew 8,035 pixels at 400×300). No threads, winit or
  system fonts needed (a font passed in). crates.io's 0.4.0 does not build
  there (`tracing_wasm`, `console_error_panic_hook`, fixed by #1450) and
  CI still lists `masonry_core` in `NO_WASM_PKGS`. Size: 485 KB without a
  rasterizer, 907 KB with vello_cpu; 149 crates (parley, fontique,
  harfrust, skrifa, ICU segmenter and normaliser data, resvg/usvg/tiny-skia
  for its Svg widget, tracing-subscriber with regex, accesskit).
- **What it gives.** Widget passes (event, update, layout, compose, paint,
  accessibility, animation frame); its own box layout with pixel snapping
  (Flex, Grid, ZStack, Portal scrolling, VirtualScroll); Tab focus only, no
  arrow-key or spatial navigation; text editing on parley's `PlainEditor`
  with IME preedit and commit; per-widget transforms; overlay layers;
  animation-frame ticks but no tweening; image, SVG and canvas widgets; a
  snapshot test harness.
- **What it lacks for us.** Tween and command queues, sprite sheets with
  frame animation, draw order beyond layers, per-actor blend modes (the
  paint list has them, widgets do not expose them), a virtual screen, menu
  navigation by arrows or pad. AccessKit is a hard dependency (no feature;
  54 references in 9 core files, `WidgetId` converts to its node id).
- **Forking.** Apache-2.0 (Linebender's dependencies Apache-2.0 OR MIT), so
  allowed; about 29k lines plus `tree_arena`, on a 0.0.x paint crate, with
  dependencies behind ours and the API changes the maintainers announced.

Inference: Masonry would cover the form half of what we need and none of
the theme half; we would run two systems (its widgets and an actor layer)
and two renderers (Vello's and ours), and track or fork an unreleased,
short-staffed project whose web support is a low priority.

## 4. Building blocks

| Crate | Findings | Use |
|---|---|---|
| cosmic-text 0.19 / glyphon 0.12 | Already in the app (`render/src/text.rs`). Shaping and swash rasterizing alone measure 499 KB. Its `Editor` has no composition (preedit) support | Keep for text |
| parley 0.12 (2026-10-09) | Layout plus `PlainEditor` with `set_compose`; 330 KB without a rasterizer | Only if it replaced cosmic-text everywhere, never both |
| taffy 0.15 (MIT, 2026-10-12) | Flexbox, block and grid; flex + block 40 KB | Optional layout frames |
| lyon 1.0.19, kurbo | Tessellate paths to triangles: fine on WebGL2 | When shapes are needed |
| vello_hybrid 0.2 (WebGL2 via `webgl`), vello_cpu | Vector rendering without compute shaders; +136 KB measured for vello_gpu 0.3, unverified on wgpu's GL backend | Later, if vector theme art matters |
| ui-events 0.3 / ui-events-web | A shared input vocabulary; the web backend has pointer and keyboard only, no composition | Reference; ours is small |
| accesskit 0.25 | — | Skip for now |
| rive-rs | Not on crates.io, last push 2025-07-04, wraps the C++ runtime, draws through Vello's compute renderer | Reject |
| velato 0.12 | Lottie into a Vello scene; no text, images or time remapping | Reject for now |
| keyframe, interpolation, tween, simple-easing | Easing functions only | Ours: StepMania's eases already exist (`noteskin/src/actor.rs`) |
| clay-layout 0.4 (Zlib) | Bindings to a C layout library | Reject (C toolchain on wasm) |

## 5. Design references

- **StepMania 5.1** (MIT): `Actor` keeps a queue of `TweenState`s with
  `BeginTweening`, `Sleep`, `QueueCommand`, `FinishTweening`,
  `HurryTweening` (`src/Actor.h`); positions are absolute in a virtual
  screen (`src/ScreenDimensions.cpp`); menus take semantic buttons from
  `InputMapper` and each screen owns its cursor (`WheelBase`,
  `MusicWheel`, `ActorScroller`), with no generic focus tree; themes
  resolve through a fallback chain (`src/ThemeManager.cpp`); actors react
  to broadcast messages (`MessageManager`, `<Name>MessageCommand`).
- **osu!framework** (MIT): transforms chain (`FadeInFromZero(500).Then()
  .Delay(200).FadeOut(500)`, sequences, loops, finish and clear;
  github.com/ppy/osu-framework/wiki/Transforms-and-tweening); keys map to
  actions through `KeyBindingContainer`; `TabbableContainer`, focus traps
  (PR #6286); `TextBox` runs IME composition itself (`AllowIme`,
  `FinalizeImeComposition`; a Korean IME fix in PR #6742, 2026-05). osu!'s
  skins answer lookups (`ISkin.GetDrawableComponent(lookup)`, textures,
  samples, config), `SkinnableDrawable` falls back to the default, and
  `LegacySkin` reads old skins.
- **Godot** (MIT): a Control inside a Container is laid out by it,
  elsewhere anchored; focus moves by explicit neighbours with a spatial
  fallback; themes carry per-type variations. **Flutter**: a directional
  focus traversal policy for arrows and gamepads. **Unity UI Toolkit**:
  Yoga flexbox plus style transitions. (From their documentation, not
  re-read for this note.)

## 6. What other rhythm games did

- **deadsync** (Rust, GPL-3.0; read, never copied): its own `deadlib-present`
  crate, an `Actor` enum with StepMania's property set (crop, fade, glow,
  blend, mask, skew, cell animation), an `act!` macro in StepMania's
  command names, immediate mode (each screen's update pushes a list of
  actors; tween state kept in a registry keyed by hashed ids), Simply Love
  ported to Rust by hand, Lua (mlua) only for song mod files, its own
  renderer backends.
- **Phira** (Rust, GPL): macroquad, lyon, glyph_brush and its own
  immediate `Ui`; text entry hands over to a separate input box.
- **Quaver** (MPL-2.0) on Wobble (MIT, a MonoGame extension: drawables,
  containers, animations), ImGui for debugging. **osu!lazer** on
  osu!framework. **Etterna** (MIT), a StepMania fork with Lua themes.
  Project OutFox's engine source is not public.

Every one built its own scene and tween layer and reused text rasterizing,
geometry and the platform layer.

## 7. Conclusions for the design

1. No library fits StepMania's actor model on WebGL2 at an acceptable
   size; Masonry fits forms only and would add a second renderer and
   system. The scene layer, the tweens and the menu navigation are ours,
   and have to match StepMania's semantics exactly anyway.
2. Retained, not immediate: Lua themes keep references to actors
   (`GetChild`, `playcommand`), which deadsync's immediate model avoids only
   by porting themes by hand.
3. Actors are placed absolutely in a virtual screen; layout is an opt-in
   frame (taffy where a form or a stats page wants flex or grid).
4. One tween engine with StepMania's semantics, with a Rust builder in
   osu!framework's style on top for our own theme.
5. Menus take semantic actions; focus moves by explicit neighbours with a
   spatial fallback, per screen.
6. Skinning is a lookup by component with a fallback, and our own theme
   goes through the same path StepMania themes will.
7. Reuse: cosmic-text and glyphon (text on desktop, and on the web
   unless the browser draws it: not measured here, `docs/plans/ui-core.md`
   step 0), taffy (optional layout), lyon (shapes when needed), winit,
   rfd, arboard and gilrs on desktop.
   Masonry's passes and test harness, yakui's navigation and
   osu!framework's transforms are reading.
