# Shell sharing (2026-10-30)

Three things each shell does its own way, decided with the maintainer
(2026-10-30) to be written once: the field preview's skin state with the
play's skin loads, the start prompt's info, and the web canvas's pointer.
Nothing here changes what a player sees, except where a step says so.

## 1. One field preview state, one skin load

### What exists

- The want/show/load machine twice: `crates/desktop/src/field_preview.rs`
  (`Field`: `wanted`, `shown`, `reading` with `crate::play::skins::Reading`;
  `ready_skin` 180-232, `draw` 243-284) and `crates/web/src/field_preview.rs`
  (`FieldPreviewHost`: `wanted`, `shown`, `loading`, `ready`, the
  `load_number` that gives a passed-over load up; `load_skin` 252-321,
  `show_skin` 323-359). Both: `set_settings`, `set_receptors` and `warm`
  through `ddi_shell::{field_preview_options, field_preview_skin,
  receptor_skin, ReceptorSkin}`, "draw only when `shown == wanted`",
  `for_new_device`/a new host on a new device, the painter's
  `set_external_image(SOURCE, …)` and `release_targets`. They differ in
  small ways: desktop's `ready()` is `wanted.is_none() || shown == wanted`,
  the web's also needs a texture; the web shows a load at its completion
  (so the prompt's first frame has it, `start-prompt.md`), desktop on the
  next frame's poll; desktop's `build` (the Lua's second pass) runs on the
  UI thread inside the poll.
- The editor field repeats a smaller copy (`shown`/`loading` of a
  `SkinWant` alone): `crates/desktop/src/editor_field.rs` 30-36,
  `crates/web/src/editor_field.rs` 43-46, 138-210.
- The web's play waits for the preview: `noteskin_ready`
  (`crates/web/src/components/game_canvas.rs` 3286-3328) polls
  `boot::revealed()` and the preview's `skin_loading()` every 100 ms (up
  to 150 times), then sleeps 800 ms if it waited at all, then calls
  `noteskins::prepare_first`. Its callers: the song's load (3272), a
  calibration (570), `restart_skin` (1688).
- `crates/web/src/noteskins.rs` shares file fetches (`FILES`, a promise
  per file, 1121-1124) but nothing after: each load parses the component
  files again on the main thread (`decode`, `part_documents`, 680-698)
  and has every SVG drawn again by the workers (`svg::rasterise`, 775,
  825).
- Desktop prepares a skin twice the same way and never waits: the
  preview's read for the diagram (`Field::warm` from song select, else
  `set_receptors` at the prompt) and the play's `Reading::first`
  (`play/host.rs` `read_skin` 1501-1510, called at 614, 810, 847). After
  a direct `--play` or a quick Start both run at once on their threads,
  each decoding and rasterizing every image; each `skins::build` runs the
  Lua's second pass on the UI thread. The cost is CPU on other cores, and
  the play's skin later than it could be, not a stalled frame.

### Can the diagram's prepared skin serve the play?

No. A prepared skin (web `noteskins::Prepared`, desktop `skins::Prepared`:
a `NoteskinPlay` and its images) depends on more than the lanes' buttons:

- Each lane keeps its layout lane's `scroll_sign`
  (`NoteskinPlay::native_lane`/`lua_lane` → `Lane::assemble`,
  `crates/render/src/noteskin.rs` 798, 1010) and plays `ReverseOn`/
  `ReverseOff` and `HitX`/`HitY` by it (1608, 1657). `diagram_layout`
  (`crates/ui/src/screens/prompt_controls.rs` 65-73) sets every lane to
  `scroll_sign = 1`, `row = 0`; Dancing☆Onigiri's two-row layouts have
  lanes at −1 (`crates/chart/src/layout/danoni.rs` 179). Columns and rows
  are not kept by the skin (the scene places lanes each frame).
- The skin's Lua VM stays with the play and was run with the load's
  `Env`. The diagram's is the demo song's at 150 BPM
  (`SkinWant::env`, `crates/render/src/field_preview.rs` 166-185); the
  play's is the chart's (`ddi_shell::play_skin`/`play_skins`:
  `chart_env` timing, difficulty, first and last beat, shifted by the
  song offset). A skin reading `GetTimingData` or the difficulty at load
  would be wrong for the play.
- The instance is the preview's: its actors and VM advanced by the
  diagram's frames, its textures uploaded to the preview's own
  `Renderer`, and the preview keeps drawing it on the prompt while the
  play readies. `start-prompt.md` already requires an independent
  instance so the demo's presses cannot leak into the song.

The same holds for Options' demo against a play (another `Env`, maybe
another layout). So "the play takes the preview's prepared skin" never
applies, and a shared future per (key, layout, env) would almost never
match either. What two loads of one skin share is everything before the
Lua: the files (shared now), the parsed component files, and the drawn
and decoded images, keyed by their content and raster size, which do not
depend on the layout (`checked.raster_size(scale)`, the scale from the
screen alone when no page asks for its own arrow size). The lanes'
buttons are the diagram's and the play's alike (`diagram_layout` keeps
the glyphs), so the play's load asks for the same images unless a skin's
Lua chooses by the `Env`, which then just misses the cache.

### Design: `ddi_shell::field_preview`

One machine in a new `crates/shell/src/field_preview.rs`, platform-free
and generic over a shell's load handle:

```rust
/// A skin load a shell runs its own way (desktop: a thread and the
/// UI-thread build; the web: a task). Dropped, it gives up.
pub trait SkinLoad {
    type Skin;
    /// The skin once in (`Some(None)`: none to draw with).
    fn poll(&mut self) -> Option<Option<Self::Skin>>;
}

pub struct SkinSlot<L: SkinLoad> {
    wanted: Option<(SkinWant, Layout)>,
    shown: Option<(SkinWant, Layout)>,
    loading: Option<(SkinWant, Layout, L)>,
    receptor_skin: ReceptorSkin,
}

impl<L: SkinLoad> SkinSlot<L> {
    pub fn set_settings(&mut self, s: &Settings) -> FieldPreviewOptions;
    pub fn set_receptors(&mut self, p: &Picture) -> (FieldPreviewOptions, ReceptorPreview);
    pub fn warm(&mut self, c: &Controls) -> FieldPreviewOptions;
    /// For a new device: the want kept (or the one shown), nothing loaded.
    pub fn for_new_device(&self) -> Self;
    /// Starts the load wanted unless it runs (an older one dropped), and
    /// polls it: `Some(skin)` is to go on the preview now (`None` inside:
    /// the built-in arrows, at once for a want without a key), and is then
    /// shown.
    pub fn step(&mut self, start: impl FnOnce(&str, &Layout, Env) -> L)
        -> Option<Option<L::Skin>>;
    pub fn ready(&self) -> bool;      // nothing wanted, or shown == wanted
    pub fn skin_in(&self) -> bool;    // the boot's preload waits on it
    pub fn loading(&self) -> bool;
    pub fn shown_layout(&self) -> Option<&Layout>;
    pub fn debug_wants(&self) -> String; // the web's format, kept
}
```

Each shell keeps its load starter, putting a skin on its `FieldPreview`
(uploads; the web's `close_after_submit`), the painter registration and
`release`:

- Desktop: `struct Load { reading: Reading, layout: Layout }`, `poll`
  is `reading.poll()` then `skins::build(read, &layout)`. `Field` keeps
  its public methods (callers in `shell.rs` 511, 655, 660, 671, 711, 1482,
  1503, 1580 unchanged).
- Web: `struct Load { cell: Rc<RefCell<Option<Option<Prepared>>>>,
  cancelled: Rc<Cell<bool>> }`; `Drop` sets `cancelled`, which the
  task's `Wanted` reads (replacing `load_number`), and closes a skin
  never taken. The task fills the cell, then upgrades the host's slot
  and runs its `step` and show at once, as `load_skin` does now. The
  host keeps `status`, `seek`, `poll_settings`, the texture check in its
  `ready()`. Callers unchanged: `ui_dev.rs` 2120-2170, `preload.rs`
  233-257.

Later, the editor field hosts can hold a `SkinSlot` too (their want is a
`SkinWant` and its layout); not part of these steps.

### Design: shared prepare work, and no poll

- `ddi_noteskin::recent::RecentImages<T>`: platform-free, keyed by
  `ImageKey { hash: String /* report::hash of the drawn document */, at:
  (u32, u32) }`, holding a shell's image `T` with its pixel count and the
  skin that last used it. Bounded by pixels, `RECENT_PIXELS` (one skin's
  whole budget, `MAX_PIXELS`: Infinito draws 531 SVGs, 23 to 60 Mpx by
  the screen, so a smaller cap kept almost none of one load for the next,
  an LRU thrashing on a scan larger than itself), and by time, an image
  unused for `RECENT_SECONDS` (30) let go. Room is made from other skins'
  images, least recently used first, never from the loading skin's own
  (past the room its images pass through uncached); `insert` returns
  what it let go so the web closes those bitmaps. Desktop trims on a
  thread waking as entries expire, the web on a timer.
- One load of a skin at a time per shell: a second load of the same key
  (after `first_there`) waits for the first to end or give up, then
  finds its drawings and parsed components in the caches. Web: a promise
  per key in `noteskins.rs`, released by a guard's `Drop` (a load given
  up releases at its next `wanted()` check); waiting at most 15 s, as the
  poll's cap does now, then loading anyway. Desktop: a `Mutex<HashSet>`
  and `Condvar` in `skins::read_inner`, the guard released on drop
  (`Reading::spawn` already catches panics).
- Web caches in `decode`: the SVG drawings (`RecentImages<ImageBitmap>`;
  a hit is handed over as a copy, `createImageBitmap(&bitmap)`, since a
  load's bitmaps are closed after their upload; a miss is drawn, its copy
  kept) and `part_documents` by the file text's hash (a few dozen files;
  `PartDocuments` made `Clone`). `__DDI_SVG_LOADS` entries gain `reused`.
- Desktop caches every SVG drawn, files and component parts, as
  `RecentImages<Arc<(Rgba, own size)>>` (`Rgba` is `Clone`; the build
  takes a copy), keyed by the raster size `check_svg` gives.
- The play asks and nothing else: `noteskin_ready` drops its loop and
  sleep and calls `noteskins::prepare_first(wants, &layout,
  Schedule::AfterEntrance)`. The loader awaits `boot::entrance_over()`
  before its first main-thread stage: a promise, its `resolve` kept as
  `screen::gfx` keeps its waiters, resolved by `ui_dev`'s frame once
  `boot::revealed()` and the host's `Ui::settled()` after the reveal
  (resolved already when no host is held: from song select, as today);
  a 12 s cap, the boot's own. The preview's loads use `Schedule::Now`.

### Steps

1. `ddi_shell::field_preview::SkinSlot` with its tests on a fake
   `SkinLoad` (a newer want drops the older load; nothing shown until
   `shown == wanted`; a warm then the same controls' picture loads once;
   a want without a key shows at once; `for_new_device` keeps the want).
2. Desktop's `Field` on it; `field_preview_offscreen.rs` unchanged.
3. The web's `FieldPreviewHost` on it; `load_number` goes.
4. `RecentImages` with its tests; desktop's per-key wait and raster cache,
   with a test that a diagram-layout read then a play-layout read of a
   bundled SVG skin rasterize each image once (a test counter in
   `skins.rs`) and give the same bytes.
5. The web's per-key wait, drawing and component caches.
6. `boot::entrance_over` and `Schedule`; `noteskin_ready`'s poll and
   sleep go. AGENTS.md's Deployment paragraph ("the play's own skin load
   waits for the fade, the preview's load, `skin_loading`, and the
   entrance") and `start-prompt.md` say what replaces them; the verify
   skill's `flow.mjs` line gains the reuse check.

### Checks

- `just test` (ddi-shell, ddi-noteskin, ddi-desktop), clippy on both
  targets after each step.
- Web, the verify skill's recipe in headless Chrome and through
  `firefox.mjs` (Firefox's slow workers are why the wait exists):
  `target/scratch/loader/flow.mjs` still sees the skin on the prompt's
  first frame with `loads` 1, the play's load second; after it
  `__DDI_SVG.drawn` is unchanged and the last `__DDI_SVG_LOADS` entry has
  `reused == images`; on a direct `#/play?…` the time from the fade to
  `prompt.waiting` losing `noteskin` is shorter than before (the 800 ms
  at least); `step1.mjs` with `__DDI_SKIN_DELAY_MS`; stepping through
  skins on Options still counts `loadsGivenUp`; `__DDI_LOSE_DEVICE()`
  during a load.

### Risks

- A cached bitmap copied for a WebGL2 upload: `createImageBitmap` of an
  `ImageBitmap` is in all three engines; the maintainer checks Safari
  after the push.
- Memory: the cap bounds it; the caches are dropped with a skin's files
  when it is deleted or imported again (`noteskins::delete`, the
  import).
- A load that never ends would hold its key: the 15 s cap, and the
  Lua's own deadlines, bound the wait.
- `Ui::settled` may come before the prompt card's own fade
  (`Cx::holding`); checked in step 6, else the gate adds that fade's
  length from the theme.

## 2. The prompt's info

### What exists

- Desktop `first_info` (`crates/desktop/src/play/host.rs` 243-271) and
  `full_info` (275-295); the web's `prompt_info`
  (`crates/web/src/components/game_canvas.rs` 2076-2157), four branches.
- The chart line and difficulty name: desktop 281-290, web 2081-2094 and
  2123-2133.
- The badge: desktop 244-250 (REPLAY for a replay or a refused one,
  else AUTOPLAY when autoplayed), web 2096-2102, 2109-2113, 2135-2139.
  The web's card before the song is read shows no REPLAY (its branch is
  for `replay.is_none()`; a replay falls to the last branch, no badge),
  desktop's shows it from the start: kept as is, each caller passing its
  badge. `PromptInfo::badge`'s doc names AUTOPLAY alone; it gains REPLAY.
- The filter band for the prompt's picture: desktop `filter` 1443-1462
  (the viewport's logical size, `None` for an empty one), web
  `prompt_picture` 2167-2183 (the canvas's CSS size, 1280×720 when
  unknown). Both are `ddi_render::scene::filter_fraction` of the chart's
  layout with `settings.field_filter`, `None` when it is 0 or the layout
  has no lanes.

### Design

In `ddi_ui::screens::prompt`, beside `chart_line` and `mods_line`:

```rust
pub enum Badge { None, Auto, Replay }
impl Badge {
    pub fn new(replay: bool, auto: bool) -> Badge;
    pub fn text(self) -> &'static str; // "", "AUTOPLAY", "REPLAY"
}
/// A chart as the card names it.
pub struct PromptChart { pub difficulty: Difficulty, pub meter: u32, pub style: String }
impl PromptChart {
    /// `song`'s chart `chart`, its style its layout's name.
    pub fn of(song: &ddi_chart::Song, chart: usize) -> Option<PromptChart>;
}
impl PromptInfo {
    /// A song's card: its chart's line and difficulty, the mods, the badge
    /// (the timing line stays the caller's).
    pub fn for_chart(title: String, artist: String, chart: Option<PromptChart>,
                     settings: &Settings, badge: Badge) -> PromptInfo;
    /// A test's card: its title and hint, the badge.
    pub fn for_test(title: String, hint: String, badge: Badge) -> PromptInfo;
}
```

In `ddi_shell`: `pub fn prompt_filter(song: &Song, chart: usize, settings:
&Settings, size: [f32; 2]) -> Option<([f32; 2], f32)>`, each shell
passing its size (desktop's `None` for an empty one stays its own).

Callers: desktop `first_info` (the library's title before the read, no
chart: `PromptInfo { title, artist, badge, .. }` as now, or `for_test`
for a calibration), `full_info` (`for_chart` with `PromptChart::of`),
`filter`; the web's four branches (the before-read one builds its
`PromptChart` from the manifest's entry and `shown_chart`'s layout) and
`prompt_picture`.

### Steps and checks

1. `Badge`, `PromptChart`, `for_chart`, `for_test` with tests (each
   badge; a chart's line and color name; a song without the chart);
   both shells switched. The prompt's screen tests and desktop's
   `play_mode`/prompt tests pass unchanged.
2. `prompt_filter` with tests (off at 0, none for a layout without
   lanes, the band `filter_fraction` gives, reverse); both shells
   switched. Web: `__DDI_DEBUG.prompt` and a prompt screenshot with the
   filter on, before and after, the same.

Risk: none beyond a branch passing another badge than before; the
tests name each.

## 3. The web pointer's owner

### What exists

The canvas's CSS `cursor` is the state hosts share:

- `ui_dev.rs` `new_host` 750-755 reads `cursor == "none"` to start
  hidden; `Host::show_cursor` 1724-1732 writes `none` or the host's
  shape; a key or menu press hides, a mouse pointer or the wheel shows
  (1550-1561); `UiRequest::SetCursor` sets the shape (1838-1848); a
  reattached canvas is written again (935).
- `Embedded`'s `Drop` (1023-1041) writes `default` unless hidden, so a
  shape the prompt asked for does not stay on a play.
- `web/touch.rs` `mouse_moved` 69-81 (called from the play's
  `pointermove`, `game_canvas.rs` 1356) writes `default` over `none`
  while playing.
- A lost device's fresh canvas copies the old one's `style` attribute
  (`web/gfx.rs` `replace_canvas` 152-170), so the cursor survives it.

### Design

In `crates/web/src/web/screen.rs`, beside the one canvas:

```rust
struct Pointer { hidden: bool, shape: &'static str }  // in `Screen`
pub(crate) fn hide_until_move();          // a key's or a pad's press
pub(crate) fn mouse_moved();              // shows a hidden pointer
pub(crate) fn set_shape(css: &'static str);
pub(crate) fn pointer_hidden() -> bool;   // debug
```

Each writes the canvas's `cursor` (`none` when hidden, else the shape)
and nothing reads it back. `Host` loses `pointer_hidden` and
`show_cursor`: its press handling calls `hide_until_move`/`mouse_moved`,
`SetCursor` and a reattach call `set_shape(self.cursor)`; `Embedded`'s
drop calls `set_shape("default")` (hidden stays hidden: the same as
today's guarded write); `touch::mouse_moved` calls `screen::mouse_moved`
while playing. `__DDI_DEBUG.screen` gains `pointer: {hidden, shape}`.

### Steps and checks

One commit. Web only: the verify skill's
`target/scratch/cursor-play/check.mjs` (a play's cursor `default` after
the prompt's pointer over Start, a key-hidden pointer showing on a mouse
move) and `target/scratch/pw/cursor.mjs` (shapes over song select's and
Options' targets), in Chrome and through `firefox.mjs`; a page change
after a key keeps the pointer hidden until the mouse moves.

Risk: two hosts alive during a page change write in the same order as
today (last writer wins); a later step could tie the shape to the host
holding the canvas (`screen::Attachment`), which is a change and not
part of this one.

## Order

1. Item 3 (one commit, web only, independent).
2. Item 2, steps 1 and 2.
3. Item 1, steps 1 to 3 (the shared machine, no behavior change).
4. Item 1, steps 4 and 5 (the caches and the per-skin wait).
5. Item 1, step 6 (the poll and the sleep go; the documents).

Each commit keeps both shells building (`cargo clippy` native and
`--target wasm32-unknown-unknown`, `just check`) and `just test` passing.

## Status

Done (2026-10-30). Item 3 done (2026-10-30): `screen`'s `Pointer`, `__DDI_DEBUG.screen.pointer`.
Item 2 done (2026-10-30): `prompt::{Badge, PromptChart}`,
`PromptInfo::{for_chart, for_test}`, `ddi_shell::prompt_filter`.
Item 1 steps 1 to 4 done (2026-10-30): `ddi_shell::field_preview::SkinSlot`
on both shells, `ddi_noteskin::recent`, desktop's per-skin wait and
drawings cache (`crates/desktop/tests/skin_cache.rs`).
Step 5 done (2026-10-30): `crates/web/src/noteskin_cache.rs`; the play's
load after the diagram's reuses all 404 of Infinito's drawings (214 ms
against 817 in Chrome, 1.4 s against 3.8 in Firefox).
Step 6 done (2026-10-30): `boot::entrance_over`, resolved by a host's frame
once revealed and settled (the card's fade included, so no theme length is
added); `prepare_first` (plays only) awaits it, `Schedule` not needed. On a
direct `#/play` the play's skin is in 0.56 s after the fade against 1.1 s
(Chrome), 2.0 s against 2.5 s (Firefox). Item 1 done.
