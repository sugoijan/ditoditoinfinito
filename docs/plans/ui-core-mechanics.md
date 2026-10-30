# UI core mechanics (2026-10-30)

Three changes to the UI core (`crates/ui/src/ui.rs`), decided with the
maintainer on 2026-10-30:

1. Screens are told when another screen covers them and when they are
   uncovered, so the shells stop guessing it from layer names every frame.
2. A screen takes input as soon as it starts coming in, not after its
   entrance; the start prompt's own fade becomes an ordinary entrance.
3. Work the UI repeats every frame for nothing goes: the hover's hit test
   and the cursor while nothing moves, and the web's `history.state` write.

Order: 3b, 2, 1, 3a (see "Order" at the end). Each step below is one
commit that keeps both shells building and `just test` passing.

## 1. Covered and uncovered screens

### Today

- Desktop resumes song select's preview by polling: every event-loop pass
  checks `ui.layers().last()` is `select` and not `out`, the sound paused
  and no play running (`crates/desktop/src/shell.rs` 1505-1523), then
  selects `env.state.song_id` again and resumes.
- The first start's folders offer waits for `ui.layers().last().name ==
  "select"` (`crates/desktop/src/songs.rs` 517). `last()` includes
  overlays, so this also means "no dialog over it".
- Both shells' preview movie hosts follow `select_shows(&ui.layers())`
  (`crates/session/src/movies/preview.rs` 52-58: the topmost non-overlay
  layer is `select`, not `out`), called each frame from
  `crates/desktop/src/preview_movie.rs` 90 and `crates/web/src/preview_movie.rs` 66.
- Song select's `tell` (`crates/ui/src/screens/select.rs` 2295-2333) and
  `update` (3212-3216, cancelling navigation every frame) are skipped while
  `!cx.is_current_screen()`; `told_song` is never cleared, so nothing is
  told again on return. Options clears `told` every frame it is not current
  (`crates/ui/src/screens/options.rs` 1233-1238) to republish its preview
  rect when it is again.
- `Ui::uncover` (`ui.rs` 393-410) plays the uncovered screen's `On`, but no
  screen hears of it. `Cx::is_current_screen` (`ui.rs` 1292-1297) is the
  only signal: true for the topmost screen that is not leaving or covering.
- Desktop pauses the preview by hand where song select pushes something
  (`songs.rs` `open_fn` 995, calibration 1015 and 1103, the editor 1025, a
  new song 1038, `Leave` 822) and when a play starts (`shell.rs`
  `play_mode` 1081).
  It does not pause under the start prompt: the prompt is pushed with the
  shell's immediate `Ui::push` (`crates/desktop/src/play/host.rs` 441), and
  the preview plays on until Start. The web stops it at the press
  (`select_preview::hand_over`, `select_page.rs` 543).
- The desktop prompt closes itself as the play starts (`Host::start`,
  `host.rs` 970, `PromptState::finish`), so song select is uncovered and
  current under the whole play (`crates/desktop/tests/play_mode.rs` 118
  relies on it), and its `update` runs every play frame (`shell.rs` 765).

### The hook

`Screen::shown(&mut self, shown: bool, root: &mut Actor, cx: &mut Cx) {}`,
default empty. The core calls it with `false` when the screen stops being
the current screen while it stays in the stack under another (covered),
and when it starts leaving; with `true` when it is current again
(uncovered). Not at its push (its `build` is its first showing), and never
for overlays: dialogs and popups cover nothing (decision). Leaving counts
as not shown so a shell has one edge to stop previews on, as
`select_shows` stops the movie at `out` today.

A `Layer::shown: bool` (true at `push_layer` for screens) makes each edge
fire once. `Ui::set_shown(id, shown, gesture)` flips it, builds a `Cx`
after the phase change (so `is_current_screen` agrees) and applies its ops.
Callers:

- `push_screen` (`ui.rs` 364), after the top goes `Covering` or `Covered`;
- `Ui::push` (342), when the top becomes `Covered`;
- `close` (1392), for a non-overlay layer still shown (`Back`, `Navigate`,
  `Replace`, `Cx::close`); a covered layer closed later stays silent;
- `uncover` (393), after the phase becomes `Entering` or `Active`.

### Song select and the shells

- `SelectCall::Shown(bool)`. Song select tells `Shown(true)` in its first
  `tell` and after each uncover, `Shown(false)` from the hook; a
  `told_shown: bool` keeps it to edges.
- `SelectScreen::shown(false)`: cancel navigation, `told_song = None`,
  `told_shown = false`, call `Shown(false)`. `shown(true)`: `tell`, which
  re-sends `Selected` (the song shown now; Options' sample may have changed
  the player's) and then `Shown(true)`: select, then resume, the order
  `shell.rs` 1515-1522 uses. `update` keeps its early return when not
  current, without the per-frame `cancel_navigation`.
- Desktop (`songs.rs` `Songs::call`, `shared_screen`, `sample_select` in
  `shell.rs` 1748): `Shown(false)` pauses the player and clears a new
  `Songs::select_shown: Cell<bool>`; `Shown(true)` sets it and resumes.
  Gone: the polling block in `shell.rs` 1505-1523, the hand pauses in
  `open_fn`, the editor, a new song, calibration and `Leave` (`Options`'s
  own pauses for its sample stay, `options.rs` 311, 433). `Songs::poll`
  offers the folders when `select_shown` is set; the "no dialog over it"
  part of `last()` goes (a dialog over a dialog is harmless, and the offer
  comes at the first poll after song select shows). `PreviewMovieHost::follow`
  takes `shown: bool` (the flag) for the layers.
- A play covers song select on desktop: `Host::start` pushes a stand-in
  screen `play` (draws nothing, takes nothing; the play owns keys and the
  pointer, `Shell::pointer`) with `Ui::push` before `finish()`, and it
  closes itself in `update` once a shared `Rc<Cell<bool>>` says the host
  left `Stage::Playing`; the results or an interruption's prompt are pushed
  over it first, so closing it uncovers nothing. Without it, the prompt's
  close at Start would uncover song select, whose `Shown(true)` would
  resume the preview for the few milliseconds before `play_mode` pauses it.
  It also keeps song select's per-frame work and the hover pick off the
  play's frames, and holds the folders offer. The preview now stops when
  the prompt opens, as on the web.
- Web: song select and Options are separate routes, each page its own `Ui`
  with one screen, so song select is never covered there; it only leaves.
  `shell_call` (`select_page.rs` 863) handles `Shown(false)` with
  `select_preview::stop()` (at the press instead of when `Navigate` goes
  out 0.14 s later, `ui_dev.rs` 1823; `hand_over` has already taken the
  context for a play) and a new `Shared::shown: Cell<bool>` the movie hook
  (`movie_hook`, 811) passes to `PreviewMovieHost::frame` instead of
  `select_shows`. `arrive` and `close` stay as they are.
- `select_shows` and its test (`preview.rs` 383-393) are deleted.

### Options

`OptionsScreen::shown(true)` sets `told = None`; the guard in `update`
keeps its early return (it skips the work while covered or leaving) but no
longer clears `told`. The screens that push over it (calibration's menu, a
share's song select) keep asking `FieldPreview(None)` themselves. Pages and
the start menu keep their `is_current_screen` returns: they only skip work.

### Steps

1. Core: the hook, `Layer::shown`, `set_shown` and its four callers. Tests
   in `crates/ui/tests/layers.rs` with a recording screen: `Cx::push` and
   `Ui::push` give one `false`; a modal dialog and a popup give nothing;
   the cover's close gives one `true` as `On` starts; `Navigate`, `Back`
   and `Replace` give one `false`; a covered screen closed gives nothing.
2. `SelectCall::Shown`, song select's hook and `told_shown`; every shell
   handles the call (desktop pauses and resumes, the web stops), the old
   paths still in place (a second resume or stop is harmless). UI tests in
   `screens/tests.rs`: covered by a pushed screen, the calls end with
   `Shown(false)`; uncovered, `Selected` then `Shown(true)`; a dialog over
   it, neither.
3. Desktop: the `play` stand-in; `Songs::select_shown`; the movie host on
   the flag; the polling block, the hand pauses and the `last()` check go.
   `play_mode.rs` changes premise: song select is covered under the play,
   and the test checks it stays covered and silent (no `Selected`, the
   player paused) through the mouse's moves; `play_flow.rs` and
   `calibration_flow.rs` see `play` on top during the play.
4. Web: `Shared::shown` and the movie hook; `select_shows` goes.
5. Options' `told` moves into its hook (`options_tests.rs`: covered and
   uncovered, `FieldPreview` is asked again once).
6. Docs: `ui-core.md` ("Layers"), `theme.md` ("Transitions"), the verify
   skill's preview notes.

### Checks and risks

Natively as above, and in `cargo run` with a small volume: the preview
keeps playing under Manage and the plays dialog, stops under Options, the
prompt and the editor, and resumes on return; after a play and its results
it resumes once. In Chrome the movie stops at the press that leaves song
select. Risks: a screen popped while covered (the core never does it
today) stays silent by design; the stand-in must close on every way out of
`Stage::Playing` (results, interruption, device lost, quit), which the
desktop flow tests cover.

## 2. Input during entrances

### Today

`Ui::active` (`ui.rs` 1266-1269) requires `Phase::Active`, and every input
path asks it or matches `Phase::Active` directly: `takes_input` (1272),
`prune_inputs` (1497), `wake_focus` (1610), `menu` (1746), direction
releases (1791), `raw_layer` (1855), key releases (1930), `key` (1952),
`pick` (2214, 2226) and `enter_chrome` (528). A screen coming in takes
nothing for its entrance: `Motion.Screen` 0.22 s, plus `Motion.Stagger`
0.05 s for the side parts (`crates/theme/src/infinito.toml` 201-205).

The prompt has no `On` so it is `Active` at once, and fades its card by
hand in `update` (`crates/ui/src/screens/prompt.rs` 1584-1601, the `faded`
field), with its own `Cx::holding` branch for the web's loader. Its
`rebuild` (1121) calls `finish_tweening`, so a rebuild during that fade
(the song read, a line changed) ends it at once.

### The rule

`Phase::takes_input()`: `Active` or `Entering`. Every check above uses it
(`active` renamed `live`). `Covering`, `Covered`, `Leaving` and `Left`
still take nothing. The results' `SETTLE` (0.6 s,
`crates/ui/src/screens/results.rs` 75) and the web's Retry guard (0.5 s,
`crates/web/src/components/game_canvas.rs` 63) stay: they keep a play's
last presses off the results, not an entrance.

A press in the crossover, the previous screen leaving (`Covering` under a
`Cx::push`, `Leaving` on `Back`, `Left` waiting for the shell), is dropped,
as now: it was aimed at a screen that already acted, and handing it to the
next screen would act on something not yet seen (a double Enter opening
Options and confirming its first row). The deaf window shrinks from leave
plus entrance (about 0.4 s) to the leave (`Motion.Screen.Out`, 0.14 s).
Held keys do not carry over (the repeater is cleared at each push) and OS
repeats never activate (`ui.rs` `key`).

Back or a push during an entrance plays `Off` or `Cancel` at once; the
theme's start with `finishtweening` (`screens::transitions`), so a part
mid-slide snaps to its place and leaves from there. That keeps the relative
offsets right; leaving from where a part is would need absolute places
(left for later if the snap shows).

### The prompt's entrance

- `tree` gives the card `transitions(root, env, &[("Prompt", Edge::None)])`
  (the same `diffusealpha,0;linear,Motion.Screen;diffusealpha,1` as the
  hand fade), but on the loading step, whose card fades out by its own
  command. Its `Off` and `Cancel` are set empty: the prompt still goes at
  once on Back and before the play's first frame, as now, and the empty
  `Off` keeps it a part for `screens::hold_entrances`.
- Under the web's loader, `push_layer` holds that `On` like any screen's
  (`Ui::hold_entrances`, `RevealMessage`) and `Ui::reveal` plays it.
- `rebuild` keeps an entrance going: `time_left` before, then
  `screens::relaid(root, left, cx)` instead of `finish_tweening`.
- `faded`, the hand fade and the prompt's `cx.holding()` branch go.
- Under reduced motion the card now fades as every screen's parts do
  (`transitions` keeps the fades); the hand fade had none.
- `Ui::settled` now covers the card's fade, which suits the web loader's
  wait and `boot::entrance_over` in `shell-sharing.md`.

### Steps

1. Core rule and the module doc (`ui.rs` 31-33). `harness.rs`
   `screens_come_in_and_back_out_with_their_transitions` (711-715) now
   expects the second screen to log the keys pressed while it comes in.
   New `layers.rs` cases: a key and a tap during an entrance act; a key
   during `Covering` and `Leaving` is dropped; Back during an entrance
   leaves. Run `just test` and fix any test that pressed during an
   entrance expecting nothing.
2. The prompt. `keys_work_from_the_first_frame` (`prompt.rs` 2064) expects
   phase `in` and Escape still going back; a new test holds entrances, sees
   the card unseen and taking keys, reveals, sees it fade in; a rebuild
   mid-entrance keeps the card's alpha below 1 until 0.22 s;
   `base_survives_loading_and_rebuilds_do_not_restart_the_reveal` stays.
   Desktop's `prompt_loading.rs` and `theme_offscreen.rs` run unchanged.
3. Docs: the verify skill line 119 ("a screen takes no input for its first
   0.22 s ... wait about 0.4 s") becomes "screens take input as they come
   in; presses during the previous screen's 0.14 s leave are dropped", and
   line 55 ("a key pressed in its first 0.2 s, while it comes in, is not
   taken", already untrue of the prompt) goes; `start-prompt.md` 62-65 and
   AGENTS.md's Deployment paragraph ("the prompt's card waits to fade in,
   `Cx::holding`") name the held entrance; `ui-core.md` "Layers" and
   `theme.md` "Transitions" get the rule. Chrome: `transitions.mjs` and the
   prestart scripts press during entrances.

## 3. Per-frame UI work

### 3a. The hover and the cursor

Today `Ui::update` re-picks under a still mouse every frame (`ui.rs`
817-826: `self.pick(pos)`, which walks the whole tree and collects every
hit, `hit.rs` 83), and `refresh_cursor` (831) runs `cursor_over` (2608),
which builds `screen.focusables()` (and asks `Screen::cursor`), after every
event and every frame. Frames rarely stop: the live wordmark and the
active outline keep `needs_redraw` true without reduced motion, and on
desktop `update` runs every play frame.

No existing signal fits. `Screen::animating` is true whenever the live
wordmark is lit (`screens/mod.rs` 436), and `Actor::in_motion` counts
effects and sprite states that move no hit area. So:

- `Ui::hover_stale: bool`, set wherever the core sets `dirty` for a
  change it caused (a `touch()` helper): any event in `handle`, `broadcast`,
  a screen's ops in `apply`, `push_layer`, `close`, `remove`, `uncover`,
  `reveal`, a phase ending in `update`, `set_design`. Cleared after the
  pick in `update`. A pointer move picks at once, as now.
- `Ui::moving()`: a layer in the input chain (or the chrome) in transition,
  with a tween left anywhere in its tree (a private `tweening(&Actor)`
  over the public `tween_time_left`, stopping at the first), or whose
  screen says so.
- `Screen::moving(&self) -> bool { false }`: content moving under a still
  pointer by the screen's own hand, without tweens. Song select (the
  navigation's glide, `wheel_motion`, `list.animating()`, the panel
  opening), song select's list dialog (`select_dialogs.rs` 1430), the
  editor (the field gliding or following the music) and the prompt (a
  rebuild pending, `version != built`). Options, pages and the results
  scroll inside their input handling, which an event already marks.
- `update` picks when `hover_stale || moving()`.
- The cursor: `cursor_for: Option<(u64, Option<Target>)>`; `refresh_cursor`
  calls `cursor_over` only when the hover differs from it or
  `hover_stale` is set, keeping the cheap `custom_pointer` part each time.
  `UiRequest::SetCursor` is still sent on change only, so the canvas
  cursor's owner in `shell-sharing.md` is unaffected.
- A debug-build check: when a frame does not pick, pick anyway and add a
  problem (`Ui::problems`) if the hover differs, so a screen that moves
  content in `update` without saying so fails the harness tests that check
  problems instead of lagging silently.

Steps: (1) `hover_stale`, `moving`, `Screen::moving` and the overrides,
with tests in `harness.rs`: a still mouse over a list row keeps its hover
while the screen's live art runs (a `#[doc(hidden)] Ui::picks()` count does
not grow), follows a wheel scroll and an inertial glide to the row now
under it, and follows a screen coming in under it; (2) the cursor cache,
with a test screen counting `focusables()` calls over idle frames; (3) the
debug check. Risk: a missed trigger lags the hover until the next event;
the debug check is there to catch it.

### 3b. The web's `history.state`

Today every frame of a non-embedded host calls `router::save_view`
(`crates/web/src/components/ui_dev.rs` 2040-2045; also on `Navigate`,
1817), which clones `history.state` into a new object, reads two keys and
replaces the entry when one differs (`crates/web/src/router.rs` 594-623).
A `None` never clears a key.

- `router.rs`: `thread_local!` `GENERATION: Cell<u64>` and `SAVED:
  RefCell<Saved { generation, focus: Option<String>, options: Option<(String,
  String, f32)> }>`. `history_changed()` bumps the generation.
- `save_view` returns when the generation matches and focus and the
  Options position (section, option id, scroll, compared without
  serializing) equal the last call's; otherwise it runs as now and stores
  them.
- Bumped by `visit` after its `push_state_with_url`/`replace_state_with_url`
  and before `go_with_delta`, by `back`, by `reset_start_focus` when it
  deletes `ddiFocus`, by `sync_history` after its marker `push_state` and
  its `go` (`ui_dev.rs` 1963-1985), and by window listeners in
  `crates/web/src/app.rs` 52: the existing `hashchange` (also the synthetic
  one `router::changed` sends) and a new `popstate` (Back, Forward, our
  `go`s).

Checked in Chrome: an init script wraps `history.replaceState` and counts
calls; idle on song select for 2 s, none; each focus move, one; Options
scrolled, one per changed frame; Options to a sub page and back, its place
restored; a dialog opened and closed by Back, then a focus move, written to
the right entry (reading `history.state` after each); the start menu
reloaded opens unfocused. Firefox through `firefox.mjs` the same. Risk: a
history write we forget to bump leaves a new entry without the current
focus until the focus moves; the listeners catch every entry change the
browser makes.

## Order

1. 3b: web only, one commit, no rule changes.
2. 2: the input rule and the prompt, with their doc and skill changes, so
   later steps test against the new rule.
3. 1: the hook, song select and the shells, the desktop stand-in (which
   relies on the prompt's empty `Off` from 2).
4. 3a: last, so its triggers and debug check are measured against the
   stack 1 leaves (song select covered during plays, no per-frame
   `layers()` polling on desktop).

## Left

- `sync_history` still builds `ui.layers()` (names cloned) every web frame
  for `back_stack::depth`; a layer revision in the core would let it skip.
- Covered screens still get `update` (their `is_current_screen` returns
  keep it cheap); not calling it at all is a separate decision.
- Parts leaving from where they are (absolute places in `transitions`), if
  the snap of Back during an entrance shows.
