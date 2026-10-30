# Controller input in the UI core (2026-10-30)

The maintainer's decision (2026-10-30): controller input is handled by the
UI core (`ddi_ui::Ui`) instead of a thread-local that many places feed.
The `Ui` owns the controller bindings, the connected pads and the input
source the hints follow. It takes a pad's edges as they are, resolves them
to menu actions or raw input itself, and remembers what each held control
produced, so a release or a disconnect releases exactly that. Screens read
hints through `Cx`. Loading or saving the settings no longer configures
anything. What the hints show and when they switch stays as
`controller-hints.md` describes it, with one exception, given below.

## Today

`crates/ui/src/controller_hints.rs` keeps `thread_local! STATE` (pads,
active source, bindings, styles, a `held` set, `revision` and
`devices_revision`), at lines 225-240. These places feed it:

- `Ui::handle` (`crates/ui/src/ui.rs:622-627`): `keyboard()` on a key's
  first press, `pad_press(m.source)` on **every** `UiEvent::Menu` press
  (the web's browser Back with source `u32::MAX` and desktop's `--start`
  press with source 0, `crates/desktop/src/play/host.rs:544`, included),
  `edge()` on `UiEvent::Raw`, and `focus_lost()`.
- The web: `keyboard()` in the play's `WebKeyboard`
  (`crates/web/src/web/keyboard.rs:50`); `devices()` and `edge()` on every
  poll of every `Gamepads` hub (`crates/web/src/web/gamepad.rs:289, 301`).
  The page host and the play route each make a hub (`ui_dev.rs:677`,
  `game_canvas.rs:619`).
- Desktop (`crates/desktop/src/input.rs`): `keyboard()` in `key_time`
  (57), which also runs during a play; `devices()` (117) and `edge()` (136)
  in `poll`; `edge()` again in `route` (149). An edge sent raw reaches
  `edge()` a third time in `Ui::handle`. The web's raw edges and the
  harness's edges reach it twice. The `held` set is what makes the repeats
  harmless.
- `configure()` in `Settings::load` and `Settings::save`
  (`crates/ui/src/state/settings.rs:572, 710`). `reset()`, `devices()` and
  `configure()` in the harness (`crates/ui/src/testing.rs:56, 304, 310`).

The `Ui` polls this state. `Ui::new` snapshots the revisions and the
connected identities (`ui.rs:270-272`). `Ui::update` (707) and
`Ui::handle` call `refresh_hints` (666-695), which broadcasts
`controller_hints::CHANGED` when the revision moved. For a pad that went
away, it also makes up releases of Up, Down, Left and Right, because the
pad's real releases may no longer resolve: desktop computes `standard`
from the pads still connected. `needs_redraw` compares both revisions
(1035-1036).

The code that turns a pad's edge into a UI event (`Raw` if
`ui.wants_raw()`, else `ControlBindings::menu_control` → `UiEvent::Menu`)
exists three times:

- the web's `ui_dev::pad_edge` (`crates/web/src/components/ui_dev.rs:1601`;
  `standard` read live from `navigator.getGamepads()`, `gesture` false on
  WebKit; also reached from `game_canvas.rs:626` for the prompt and the
  results);
- desktop's `Input::route` (`input.rs:148`; `standard` from the last poll,
  bindings from `Input::controls()` 97-103, which reads the options as
  edited, else `Settings::load()`);
- the harness's `Harness::controller` (`testing.rs:298-335`).

The web host loads its bindings once, at `new_host` (`ui_dev.rs:776`), and
never updates them. On the web, a pad bound in Options keeps the old menu
bindings until the route changes.

These read the state:

- `active()`, `actions()`, `activate()`: `screens/start.rs:203-218`,
  `screens/select.rs:1969-1970`, `screens/prompt.rs:1306`,
  `screens/options.rs:287`, `screens/mod.rs:568-577, 602` (the footer).
- `cover_instruction()`: `screens/options_model.rs:489-490`.
- `lanes()`: `screens/prompt_controls.rs:49` (`Controls::caps`).
- `CHANGED`: `start.rs:251`, `results.rs:1448`, `select.rs:3465`,
  `prompt.rs:1714`, `options.rs:1259`, `page.rs:795`, `editor.rs:2713`.
- `quit_hint()`: `crates/session/src/play.rs:688`.
- Tests: `crates/theme/tests/controllers.rs:311-400` and
  `crates/desktop/tests/controller_hints_gpu.rs:69-134`.

These are already pure and stay as they are: `detect`, `presentation`,
`Cap`, `Glyph`, `Family`, `Style` (`options_model.rs:1262-1270, 1539`,
`keycaps.rs:89`, `crates/theme/src/keycaps.rs:209`).

## The new API

In `crates/ui/src/controller_hints.rs`, with no thread-local:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Source { #[default] Keyboard, Pad { slot: u32, id: String } }

/// What hints are drawn from. A plain value: the Ui keeps it as Rc<Hints>
/// and swaps it on change, and Cx holds a clone of that Rc.
#[derive(Clone, Debug, Default)]
pub struct Hints { controls: ControlBindings, styles: BTreeMap<String, Style>,
                   pads: Vec<ConnectedPad>, source: Source }
impl Hints {
    pub fn new(controls: ControlBindings, styles: BTreeMap<String, Style>,
               pads: Vec<ConnectedPad>, source: Source) -> Hints;
    pub fn active(&self) -> bool;          // a Pad source that is connected
    pub fn source(&self) -> &Source;
    pub fn pads(&self) -> &[ConnectedPad];
    pub fn actions(&self, actions: &[MenuInput]) -> Vec<Keycap>;
    pub fn activate(&self) -> Vec<Keycap>;
    pub fn cover_instruction(&self) -> Option<String>;
    pub fn lanes(&self, layout: &Layout, controls: &ControlBindings)
        -> Option<Vec<Option<Keycap>>>;
}
pub fn quit_hint(raw: &RawInput, pads: &[ConnectedPad],
                 styles: &BTreeMap<String, Style>) -> &'static str;
pub const CHANGED: &str = "ControllerHintsChanged"; // kept
```

In `Ui` and `UiEvent`:

```rust
pub enum UiEvent { /* … */ Pad(RawInput) /* a controller's edge as read */ }
impl Ui {
    pub fn set_controls(&mut self, controls: ControlBindings,
                        styles: BTreeMap<String, Style>);
    pub fn set_pads(&mut self, pads: Vec<ConnectedPad>);
    pub fn set_source(&mut self, source: Source); // carried in, not a press
    pub fn set_pad_gesture(&mut self, gesture: bool); // web: !is_webkit()
    pub fn hints(&self) -> &Hints;
}
impl Cx {
    pub fn hints(&self) -> &Hints;
    pub fn set_controls(&mut self, controls: ControlBindings,
                        styles: BTreeMap<String, Style>); // an Op, applied after dispatch
}
// Ui, private:
held: HashMap<(u32, String), Held>, // enum Held { Menu(MenuAction), Raw, Unbound }
hints: Rc<Hints>,
pad_gesture: bool, // true by default
```

How `UiEvent::Pad(e)` is handled (gamepad devices only):

- **A press** whose `(slot, control)` is already held is a duplicate and is
  dropped. Otherwise it is fresh. If `(slot, id)` is a connected pad, the
  press claims the source, and the hints change if it is a different pad.
  It is then dispatched as `Input::Raw` if `wants_raw()` (held `Raw`).
  Otherwise it goes through `menu_control(id, standard of that slot,
  control)` as `menu_event` does today, with `pad_gesture` (held
  `Menu(action)`). Without a binding it is held `Unbound`. A pad the Ui
  does not list still dispatches with `standard` false, as today, but
  never claims the source.
- **A release** removes the held entry. `Menu(a)` releases `a`
  (`repeater.release`, and `menu_release` for a direction). In every case
  the release also goes to the raw layer if one is on top now, as today:
  `BindFlow` clears `held`/`waiting` on releases of controls held when it
  opened. A release with no entry goes only to the raw layer.
- **`set_pads`** works out which `(slot, id)` pairs are gone and releases
  each of their held entries as above, from the action that was actually
  pressed. If the source was one of them, it falls back to `Keyboard`. It
  broadcasts `CHANGED` when the source is a pad and the pads changed (the
  revision rule today). No releases are made up.
- **`set_controls`** does nothing when the bindings and styles are
  unchanged. Otherwise it swaps them, and broadcasts `CHANGED` if a pad is
  the source. Held entries keep the actions they were pressed as.
- **A key's first press** (`UiEvent::Key`, not a repeat) sets `Keyboard`.
  `Focus(false)` clears `held` after `blur()` has cancelled the directions.
  `UiEvent::Menu` is still accepted, for synthetic actions only, and no
  longer touches the source.
- Each change sets `dirty` and broadcasts at once. `refresh_hints`, the
  revisions and the hint clause of `needs_redraw` are deleted.

`UiEvent::Raw` is removed once no shell sends it. Keys are already handed
on raw by the Ui itself.

## The shells

**Desktop** (`crates/desktop/src/input.rs`, `shell.rs`):

- `Input::poll` calls `ui.set_pads(connected)` when the news changed them.
  `route` becomes a plain `UiEvent::Pad(e)`. The menu/raw counters in
  `summary` are counted from `ui.wants_raw()` at send time. `controls()`,
  `Input.controls`, `standard` and the `controller_hints` calls go.
- At startup, `ui.set_controls(settings.controls, settings.controller_styles)`.
  `take_edited` keeps feeding the field preview, but Options gives the Ui
  its bindings itself, through `Cx::set_controls` (below).
- During a play, keys and edges don't reach the Ui (`shell.rs:1368`,
  `1491-1495`). The shell calls `ui.set_source(Source::Keyboard)` on a
  key's first press, and `ui.set_source(Source::Pad { .. })` on a pad's
  press edge, so the results show what was played with. gilrs edges are
  transitions, so each press is fresh.

**Web**:

- The hub's poll stops calling into `ddi_ui`. Its listener carries
  `PadNews::Edge(RawInput)` and `PadNews::Pads(Vec<ConnectedPad>)`, the
  latter sent when the list changes. `ui_dev::pad_edge` becomes
  `ui.set_pads` / `dispatch(UiEvent::Pad(edge))`. `Host.controls`,
  `pad_standard` and the menu resolution go.
- `new_host` (`ui_dev.rs:748`) calls `set_controls` from the settings it
  already loads, `set_pads` from the hub, `set_pad_gesture(!is_webkit())`,
  and `set_source(last_source())`.
- Route hosts come and go, so the source is carried between them in one
  shell-owned cell beside `HOSTS` in `ui_dev.rs` (`LAST_SOURCE`). It has
  three writers: `stop_ui`, the drop of `Embedded` (each saves
  `ui.hints().source()`), and the play route's own listeners during a play
  (`game_canvas.rs:595-632`: a key's first press, a pad's press edge),
  which replace `keyboard.rs:50`. `new_host` is its only reader. It is not
  a hint state: it holds no pads, bindings or revisions.
- `game_canvas.rs:1150` (`Msg::PadPress`, a calibration card's Start and
  Back with no Ui) is not a menu path and stays.

**The harness** (`crates/ui/src/testing.rs`): `with_gestures` no longer
resets anything. `controller(controls, id, standard, control, pressed)`
calls `ui.set_controls` and `ui.set_pads` when they differ, then sends
`UiEvent::Pad`, so the three copies of the routing become one, inside the
Ui. New helpers: `pads(Vec<ConnectedPad>)` and `edge(RawInput)`. `pad()`
(the `UiEvent::Menu` helper, 59 callers) stays.

## The play, the prompt and the bind flow

- **Play**: `PlaySession` already has what `quit_hint` needs. The pads
  come from `Inputs::connected_pads()`, read at start and again in
  `adopt_pads` (`play.rs:418, 1010`), and the styles from the `Settings`
  passed to `start`. It keeps the `ConnectedPad` list next to `known_pads`
  and `settings.controller_styles.clone()`, and calls
  `quit_hint(&raw, &self.pads, &self.styles)`. The shells need no new
  feed. The claim that they would is wrong.
- **Start prompt**: `Controls::caps(&self, hints: &Hints)`. `draw` passes
  `cx.hints()`. Screens whose `publish(&self)` has no `Cx` (`prompt.rs:1331`,
  `start.rs:198`) keep the `Rc<Hints>` they last built with, and refresh it
  on `CHANGED`. `Controls::for_chart` and the shells' skin warming never
  read hints and are untouched.
- **Options and the bind flow**: `BindFlow` stays pure. It takes edges as
  `Input::Raw` because the bind dialog's `raw_input()` makes `wants_raw()`
  true, and the Ui now decides that per edge, as the shells did. Releases
  reach it as before (see "A release" above). `options_shell::act`
  compares the model's `controls`/`controller_styles` with `cx.hints()`
  after each act, and calls `cx.set_controls` when they differ. This covers
  every place that changes them: binding, the style choice, resets,
  `options_model.rs:1544-1546, 1699, 1789-1923`. A new Back works in the
  menus at once, on both shells. The lane-cover help
  (`options_model.rs:489`) reads a `hints: Rc<Hints>` in `World` that
  `options_shell` refreshes from `cx`. As today, it updates on the next
  rebuild of the data, not on `CHANGED`.
- **Settings**: `Settings::load` and `save` lose their `configure` calls.
  `controller_styles` and `controls` are plain data.

## Steps

Each commit keeps `just check` and `just test` green and both shells
building (`cargo check -p ddi-web --target wasm32-unknown-unknown`, with
`DDI_IMPORTER_TEST_ONLY=1`).

1. **`Hints` as a value.** Move `actions`, `activate`, `cover_instruction`
   and `lanes` onto `Hints`, add the pure `quit_hint`, and keep the
   thread-local's functions as wrappers over a `Hints` in `STATE`. The
   session switches to the pure `quit_hint` with its own pads and styles.
   No behavior change.
2. **The Ui owns `Rc<Hints>`.** Add `Cx::hints`, `Cx::set_controls`,
   `UiEvent::Pad`, the `held` map, `set_pads`, `set_controls`,
   `set_source` and `set_pad_gesture`. Screens and `Controls::caps` read
   `cx.hints()`. A temporary bridge keeps the shells working: `Ui::new`,
   and `refresh_hints` when a revision moved, copy the thread-local into
   the Ui through the new setters. `Ui::handle` still calls `keyboard()`,
   so the thread-local stays the shells' truth. Convert the harness, the
   theme tests and the GPU test to the Ui API, and add the tests below.
3. **Desktop** sends `UiEvent::Pad` and calls `set_pads`, `set_controls`
   and `set_source` (during play). Its `controller_hints` calls and
   `route` go. The bridge is inert for it now, because nothing it does
   bumps a revision.
4. **Web**: `PadNews`, `UiEvent::Pad` from `pad_edge`, `new_host`'s
   setters, `LAST_SOURCE`, the play route's listeners, and no hint call in
   `gamepad.rs` or `keyboard.rs`. Checked with the verify skill's fake pad
   across routes and a play.
5. **Remove the thread-local**: `STATE`, `reset`, `configure`, `devices`,
   `keyboard`, `pad_press`, `edge`, `focus_lost`, the revisions, the
   bridge, `refresh_hints`, the hint clause in `needs_redraw`, and the
   settings' `configure` calls.
6. **Remove `UiEvent::Raw`.** Tests send `UiEvent::Pad`
   (`options_tests.rs:407, 512, 776`, `editor_record_tests.rs:148`,
   `crates/desktop/tests/editor_record.rs:103`, which then checks
   `wants_raw()` instead of `route`). Update AGENTS.md (the `crates/ui/`
   paragraph on `controller_hints.rs` and the shells' input),
   `controller-hints.md`'s step 5 wording, and this plan's status and
   line in `README.md`.

## Tests

All in `crates/ui`, on the harness unless noted:

- **A held pad that disconnects releases its action.** Hold Down
  (`button:13`) on song select, then `set_pads(vec![])`. The screen gets
  exactly one `MenuRelease { Pad(0, Down) }`, the song stops moving over
  1.5 s, and a late release edge for `button:13` dispatches nothing. The
  same with a standard pad whose release would no longer resolve (the
  case that needed the made-up releases), and with a raw press under the
  bind dialog (`waiting` clears).
- **A release goes where its press went.** Press Down, push a raw-input
  screen, then release: the menu gets the release, the raw layer sees it
  too, and nothing repeats. Press under the raw screen, pop it, then
  release: no menu release.
- **Hints follow the last deliberate source across screens.** A pad press
  makes it active, and it stays so through push, pop, a dialog and a
  transition. A key's first press switches to the keyboard, a repeat does
  not, and a held pad does not reclaim it. A release then a press does.
  `set_source(old_ui.hints().source().clone())` on a fresh Ui keeps it
  (the web's handoff), unless that pad is not in the new Ui's pads. A slot
  taken by another id falls back to the keyboard, and the old id's stale
  edge cannot claim. A synthetic `UiEvent::Menu` changes nothing.
- **No redraw polling.** After a drawn frame, `needs_redraw()` is false
  and stays false over repeated `update(0.0)` calls. `set_controls` with
  equal bindings, or any bindings while the keyboard is the source, does
  not dirty and broadcasts nothing. Different bindings while a pad is the
  source broadcast `CHANGED` once and make `needs_redraw()` true. The same
  holds for `set_pads`.
- **Options**: bind a new Back in the dialog, and the next pad edge in
  Options resolves with it. `Settings::load()`/`save()` leave a `Ui`'s
  hints alone.
- `quit_hint` with an override style and an unknown pad (in
  `controller_hints.rs`). The existing source and lanes tests move onto
  `Hints` and `Ui`.
- Desktop: `input.rs` tests that `poll` hands `set_pads` before the edges.
  The editor-record test goes through `UiEvent::Pad`.

## Risks

- The web's handoff. A host that closes without writing `LAST_SOURCE`
  shows keyboard hints on the next route. One writer per kind of host,
  plus the verify skill's route journey, cover it.
- Releases into the bind flow. Keeping the rule "every release also
  reaches a raw layer on top" preserves its `waiting` and `held` exactly.
  Its existing tests stay unchanged as the check.
- The gesture policy moves from the event into the Ui. A missed
  `set_pad_gesture(false)` on WebKit lets a pad press ask for audio that
  Safari refuses. It needs the maintainer's published-URL Safari check
  (AGENTS.md).
- One deliberate behavior change: synthetic menu actions no longer claim
  the source. Today desktop's `--start` claims slot 0 when a pad sits
  there. `controller-hints.md` already says synthetic input must not.
- Two hubs poll on the play route. Only the listener of the hub a Ui
  belongs to feeds that Ui, so the other hub can no longer move the hints.

## Left out

- Options' own controller view (`World::connected`, `OptionsPlatform::held`,
  desktop's `o.set_pads`/`o.held`, the web's 300 ms `PadsTick`). It could
  read `cx.hints().pads()` and the Ui's held map later.
- Desktop's `input::CONNECTED` and the web's `page_pads()`, which the
  editor's recording and the play's `Inputs` read. They are pad lists for
  plays, not hint state.
- Play input, which still goes shell → session, never through the Ui.
  Also left out: keyboard menu mapping, dance-pad hint profiles and
  physical hardware checks.

## Status

Done (2026-10-30), in the six steps above. As built:

- `Hints` has `pad_at`, `controls` and `styles` besides the plan's, and
  `Cx::hints_rc` hands screens the `Rc` to keep. The start menu and the
  prompt pass `cx.hints()` to their `publish` rather than keeping one.
- `set_pads` also sends a binding screen on top a release for each held
  control of a pad gone, so the bind flow's `waiting` clears.
- The web keeps a carried pad source pending in the new host
  (`Host::pending_source`) until its hub's first list of pads: a hub just
  made has read none. `LAST_SOURCE` has the plan's writers, and the
  `ui_dev` component's `destroy` too.
- Tests: `crates/ui/tests/pads.rs`, `options_tests::a_back_bound_works_in_the_menus_at_once`,
  `controller_hints::tests` on `Hints`; the web checked with
  `target/scratch/pw/hints-journey.mjs` (a pad on the start menu, song
  select and back, a key, a pad through the prompt and a play to its
  results) in Chrome and Firefox.
- Left for the maintainer: Safari on the published URL (a pad's press is
  no gesture there, `Ui::set_pad_gesture(false)`).
