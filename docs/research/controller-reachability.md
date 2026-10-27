# Controller reachability audit (2026-10-27)

Baseline for [controller hints and reachability](../plans/controller-hints.md).
This records the baseline before changing bindings or hints. The default
bindings chosen from it are in that plan. The findings below are historical;
the implementation follow-up at the end records the fixes and their evidence.

The maintainer clarified the scope during this audit: **all free-text fields
may use a keyboard/IME; numeric editing must work with a controller.** There
is no virtual keyboard. Entering a query, URL, title, artist or textual timing
label is exempt; reaching the field, submitting/canceling, and using the
resulting screen afterward are not. Capturing a binding for a particular
physical device necessarily requires that device; navigating its setup does
not. OS-owned pickers and browser activation limits are reported separately.

## Implementation follow-up (2026-10-27)

The shared resolver and both shell dispatchers now implement the settled
defaults. Saved Start/Back win over positional shortcuts, dedicated Start
survives until the UI knows the screen, and raw capture/recording retain their
own paths. Gameplay tables, lane assignment and quit gestures are unchanged.

| Gap | Implemented path |
| --- | --- |
| G1: app actions | Visible, focusable More on Select, Options, Results and Editor; West opens the same menu. Entries expose secondary selectors and previously keyboard-only actions; long lists have controller-operated pages. |
| G2: search | Fresh Up from an empty wheel reaches the header. More offers Clear and suggestions; Up while typing opens a controller-navigable suggestion list. Confirm returns to results after keyboard text input. |
| G3: bindings | Standard devices navigate Capture/Skip/Save/Redo/Cancel. Capture is explicitly armed, waits for its activating control to release, and accepts all face buttons. Unmapped devices retain guided learning and use provisional bindings for review. |
| G4: results | Controller directions move through replay/retry, offset and panel controls; compact layouts scroll focused controls into view. Shoulders switch panels. Keyboard panel shortcuts remain. |
| G5: editor | Current lane, South place/remove, two-confirm hold/roll anchors with cancellation, dedicated Start Listen/Stop; More supplies tools, ranges, nudges, jumps, zoom, tap tempo and Place/remove for minimal controllers. Recording still uses gameplay lanes. |
| G6: numeric values | Number forms provide increments/decrements, fine/coarse steps, units/modes, reset, remove and validation. Compound speed, time signature, combo and display BPM values have separate controls. Optional typing remains available. |
| G7: free text | Controller submit/cancel/navigation after keyboard or IME entry in value and URL forms; unfinished IME composition cannot be submitted by a controller. Search results need no extra keyboard confirmation. |
| G8: platform boundaries | Browser picker refusal explains the required keyboard/click/tap, keeps the dialog and focus, and permits controller Back. Existing browser gesture policies remain in force. OS-owned dialogs and Safari audio activation remain platform boundaries. |

Executed verification uses synthetic controllers, not family identification:

- Shared engine, UI, theme and session unit/integration suites: **740 passed,
  2 ignored** (the existing benchmark tests). `crates/theme/tests/controllers.rs`
  drives physical standard-button edges through the shared resolver and the
  real Infinito theme at 1280×720, 844×390 and 390×844. It covers text handoff,
  empty results, More, panel focus/scrolling, numeric editing, hold cancellation,
  tap tempo, IME composition and picker recovery. UI tests also cover capture
  of face buttons, Skip/Redo and a raw controller with only directions/Start/Back.
- Headless installed Chrome, stable release build, normal user-activation
  policy, fake Gamepad API: chart shoulders and modal isolation, keyboard
  search → controller recovery from empty results, Options sections, Results
  focus/shoulders, real editor place/remove and compound speed editing at all
  three sizes. Real import picker refusal retained the dialog and controller
  Back returned to Select. Screenshots checked More and numeric forms,
  including a short landscape window with device chrome reserving space.
- Desktop `--no-default-features` build and `bind_flow` integration test pass.
  The latter sends fake pad events through the desktop pad thread, input hub
  and router: shoulders reach Controls, directions select the controller,
  Capture records each step, and Start saves bindings which reload correctly.
- Clippy passes with warnings denied for UI, theme, engine, session and
  desktop (all targets, no default optional helpers). The Trunk release build
  checks the web dispatcher; the optional video decoder was omitted for this
  UI verification build.

Real Xbox/PlayStation/Nintendo hardware, USB/Bluetooth transport differences,
native OS picker navigation and the published-origin Safari activation check
were not exercised. Controller-family hints and presentation overrides remain
the next phase; this work does not claim detection coverage.

## Baseline method and evidence

Read the shared input dispatch, each screen's input/activation and focus paths,
the raw-input dialogs, play controls, and desktop/web routing. In particular,
`focusables()` is only a declaration: a screen that consumes a direction can
prevent the UI's focus navigation from reaching a declared button.

Executed:

- `DDI_IMPORTER_TEST_ONLY=1 cargo test -p ddi-ui --lib screens:: -- --test-threads=1`:
  **215 passed, 2 ignored** (paint-cost and large-library benchmark tests).
- `DDI_IMPORTER_TEST_ONLY=1 cargo test -p ddi-engine --lib bindings::tests -- --test-threads=1`:
  **11 passed**, including physical standard-button mappings, raw mappings,
  default tables, cleared tables and identical-controller side assignment.
- A temporary native `ddi-theme` example used the real Infinito look and
  `ddi_ui::testing::Harness`, without a pointer, forced focus or injected
  activation. It replayed direction/Select presses, separated by 700 ms, from
  fresh sample screens, recording reached focus targets. Sizes: 1280×720,
  844×390 and 390×844; reduced motion, no chrome overlay, desktop-style gesture
  policy. Song fixtures enabled management, history and editing; results had
  a replay callback and editable offset. The bounded search used at most 24
  presses / 300 distinct navigation states; results included scroll position.
  Options varied sections/rows, not every possible settings combination.
- A separate harness path entered `zzzzzznomatch` through the keyboard text
  mirror, pressed controller Confirm, then Up four times at all three sizes.
  The query remained, zero matches were shown, and focus stayed on `Wheel`.

These checks establish shared-screen behavior, not real-controller detection
or native/browser end-to-end coverage. Some existing tests enter dialogs by
keyboard or pointer, so they establish the dialog's behavior, not a controller
route to it. No live hardware, browser activation or OS picker test was run
in this audit. The temporary diagnostic example is not product code.

## Concrete focus findings

| Screen / fixture | Focus reached by directions and North | Finding |
| --- | --- | --- |
| Start, all sizes | Singleplayer, disabled Multiplayer, Options, Credits, desktop Quit | Enabled choices reachable. |
| Song select, wide | Wheel, Search, Mod0–Mod2 | Footer Add/Manage/Calibrate absent; history and editor have no controller action. |
| Song select, compact landscape | Wheel, AddIcon, SearchIcon, OptionsIcon | Import/Options reachable here; no history/editor/management entry. |
| Song select, portrait | Wheel, Chart0, StyleButton, Back, AddIcon, SearchIcon, OptionsIcon | Different focus graph again; same missing actions. |
| Options, wide | RowsClip, PreviewLayout | North changes section; row actions work. Calibration footer is stranded. |
| Options, compact | RowsClip, PreviewLayout, section tab(s), Back | Rows and section changes work; compact has no calibration footer. |
| Results, wide | SongList, WatchReplay, OffsetDown, panel tabs | OffsetUp, UseMean and ResetOffset not reached. North supplies Retry separately. |
| Results, compact landscape | SongList and first panel tab | Scrolling works but replay/offset buttons remain unreachable in this fixture. |
| Results, portrait | SongList, OffsetDown, Back | Same horizontal-focus problem; replay absent from reached targets. |

The results searches exhausted 18, 130 and 171 navigation states respectively;
the source confirms that Left/Right always change panels instead of moving
focus. The song-select source confirms all directions outside its header are
consumed as song navigation, except fresh Up when the first row is selected.
Thus the zero-results case cannot return to the search header at all. These
are reachability bugs, not missing controller artwork.

Useful reproductions from the sample's initial Some Things Must selection:

- Wide select: four spaced Up presses reach Search; Right reaches the first
  modifier. Down returns to the wheel, not the footer. No directional path
  reaches Add songs or Calibrate.
- Empty search: type a query with no matches, use controller Confirm to leave
  typing, then Up. It remains on the empty wheel. Back leaves the screen and
  preserves the filter, so it does not solve clearing it.
- Wide results after settling: Up, Down reaches OffsetDown. Right changes the
  panel set instead of reaching OffsetUp. On compact results, first account
  for the scroll consumption of Up/Down; horizontal movement still changes
  panels.
- Wide Options: North cycles sections; Up/Down move rows and Left/Right change
  values or the selected row action. Up on the first row enters chrome (if
  present), leaving no path to the calibration footer.

## Action matrix

R = a controller path exists in shared code; H = additionally exercised by
the harness/current controller tests; T = keyboard text prerequisite allowed;
G = missing or broken app-owned path; P = platform boundary. R does not claim
every hardware/browser combination was tested. Desktop and web share these
screens; differences are explicit in the last column.

| Area/action | Current controller path and status | Follow-up / platform distinction |
| --- | --- | --- |
| Start: singleplayer, Options, Credits, Quit | R/H: Up/Down, Confirm | Quit desktop-only; Multiplayer intentionally disabled. |
| Global device notices | R/H: fresh Up past supported screen's top, directions, Confirm; Down/Back return | Verify new More/header focus links preserve chrome access. |
| Desktop folder offer | R: Add/Decline focus buttons, Back declines | Real directory scan is a shell operation. |
| Songs/packs and start | R/H: all four directions browse, Confirm toggles pack/starts song | Preserve tuned taps, pairs, holds and transition ownership. |
| Chart difficulty and style | R/H: double direction taps choose charts; North cycles available styles | Add shoulder difficulty shortcuts; keep gestures and focusable style choices. |
| Options from song select | R/H: wide header modifiers; compact OptionsIcon | Also expose in More. |
| Add songs | G wide / R/H compact AddIcon | G1: reachable More/header path at every size, including empty libraries. |
| Calibrate from song select / Options | G: footer exists wide but directions do not reach it; absent compact | G1: More must expose calibration in both screens. |
| Manage selected song/pack, reread/remove | G entry; R dialog buttons and confirmations once opened | G1: More entry, preserving capability gating and destructive confirmations. |
| History, best replay, editor from song select | G: P/W/E; history also pointer long-press; no controller activation target | G1: More entries for Chart history, Watch best and Edit chart, when available. |
| History dialog: choose, page, watch, details, remove | R: focusable rows/buttons, Newer/Older, Confirm; removal armed then confirmed | Entry G1; retest full journey rather than keyboard opening alone. |
| Search reach/leave/clear with results | R/H + T: Up to field; Confirm/Down return; Right to Clear; Back cancels typing/popup | Browser may need key/tap to begin typing; results navigation afterward is required. |
| Zero matches / empty library | G: no selected first row, so Up never yields focus to header | G2: fresh Up must leave the empty wheel; More/Clear/Add remain accessible. |
| Search suggestions | G for controller selection of the popup; keyboard/pointer choose them | G2: make suggestion choices focusable or offer a controller-operable Suggestions list; retain direct return to results. |
| Options sections, rows, values, resets | R/H: North next section; Up/Down rows; Left/Right value/action choice; Confirm executes | Action rows require Confirm; movement alone must not reset/export/remove. |
| Options preview layout | R/H: Down beyond final row, Left/Right or Confirm | Existing P shortcut need not be mirrored globally; keep focus path. |
| Options folders/history import/export/demo | R to row actions; P for pickers/downloads/audio as applicable | Separate enabled app control from successful OS/browser completion. |
| Controller lane capture / adding a control | R/H: raw device press; Start saves after review; Back or hold cancels where supported | G3 below for Skip/Redo. Keyboard-binding capture needs a keyboard by definition. |
| Guided bind Skip / Redo | G: controller is routed raw for the whole dialog; only `Input::Activate` invokes these buttons | G3: navigable step/review controls outside explicitly armed capture. |
| Prompt Start/Back, countdown, modifiers, save defaults | R/H for menu/countdown paths; explicit up/down option chain, left/right values | Prompt diagram remains keyboard-only until hints work. Save must activate itself, not Start play. |
| Loading/errors/device-choice prompts | R: Back during loading, focusable choices on cards; prompt tests pass | P: browser audio activation can require key/tap. |
| Gameplay / replay exit | R: resolved lanes; bound Back hold/double-tap quits | No menu face-button aliases during play; preserve lane precedence. |
| Lane cover | R: bound Start tap toggles; hold Start with bound up/down lane adjusts | Shared session; those lane presses continue to be judged. |
| Calibration modes, resets, outcome choices | R: Page column choices and prompt buttons | G1 entry; audio-start P on affected browsers. |
| Results return/retry/panels | R/H: Back/Confirm return; North retry (kept-play results watch replay); Left/Right panels | G4: move controller panels to shoulders, free directions for focus. |
| Results replay and offset decrement/increment/mean/reset | G partial: wide replay/decrement reachable; other offset buttons not; compact worse | G4: explicit focus links + scroll-to-focus and More alternatives. All numeric changes must work. |
| Editor menu/listen/cursor/snap/exit | R: North menu, Confirm listens, Up/Down cursor, Left/Right snap, Back exits | G5: separate Confirm from dedicated Start before adding grid placement. |
| Editor note/lane/hold editing | G: number keys or pointer place; no controller lane selector or placement action | G5: shoulders choose lane, South edits; two-step hold/roll endpoints. |
| Editor selection, nudge, fast cursor movement, zoom | G partial: menu has Select all, copy/cut/delete/mirror for existing selection; range creation, nudge and zoom need keys/pointer | G5: explicit actions for selection endpoints, clear, nudge, jump and zoom. |
| Editor undo/redo/tool, timing/song/chart menus, save/export/playtests | R via North actions menu; dialogs have focusable choices | G6 for numeric fields; T for free text; P for export/save dialogs. |
| Editor record/listen/audio/view/metronome/clap | R via menu; recording uses actual play bindings and lane-first raw dispatch | Keep recording isolated from menu shortcuts; stop controls follow current resolver. |
| Editor tap-to-tempo | G: Y is the only way to collect taps; menu only applies an estimate that already exists | G5: Tap tempo mode with a focused Tap control, Apply and Back; use event timestamps. |
| Editor BPM/offset/meter and compound timing values | G: `ValueDialog` accepts strings only, including speed, signature and combo tuples | G6: numeric controls and separate units/tuple fields, explicit remove/reset; no keyboard requirement. |
| Editor titles/artist/credit/music paths/labels | T; field, OK and Cancel declared | G7: verify controller submit/cancel/focus handoff after real text/IME input, including validation failures. |
| Import tabs/options/report/cancel/retry/close | R/H for tab/field entry, R for other controls once dialog open | G1 entry wide; native URL importer only where supported. |
| URL field / New song | T + R surrounding controls; new song opens music picker | G7 handoff after typing; P for OS picker. |
| Credits text/folds/links | R/H column navigation/scroll, Confirm unfolds/opens, Back leaves | P for browser opening URLs; existing refusal says Enter/tap. |

## Fixes required before hints can claim coverage

**G1 — Reachable actions menus and header paths.** Add a visible More control
reachable through ordinary directional focus on song select and Options,
also bound to West. Song select lists available Chart history, Watch best,
Edit chart, Manage song/pack, Add songs, Options and Calibrate. Include Search,
Clear search when applicable, and a style chooser so raw controllers without
North/shoulders also have paths. Options includes Calibrate and its existing
screen-level actions. Keep destructive operations inside their confirmation
dialogs. Repair header/footer traversal rather than relying solely on West.

**G2 — Empty lists and suggestions.** Treat an empty wheel as its top for
escaping to the header. Keep Clear and More reachable with zero results and
zero songs. Provide an explicit Suggestions focus/list path after typing; no
virtual keyboard and no requirement to navigate a suggestion popup to get to
ordinary results. Back closes the suggestion layer first and preserves text.

**G3 — Binding flow control versus capture.** Use a navigable step/review mode
with Capture, Skip (where allowed), Redo, Save and Cancel. Only arming Capture
takes raw input; that press/release must not bind itself. Once a control is
captured and released, return to navigation so the user can choose the next
step or redo/skip. Raw capture reserves no ordinary button from assignment;
keep the existing hold-to-cancel escape. Do not route a raw South/East press
as both a candidate binding and Confirm/Back. Test a one-standard-controller
setup with no prior saved bindings. Preserve unmapped devices' automatic
learning flow and hold-to-cancel; use learned controls for review navigation
once available, without pretending their raw numbering is standard.

**G4 — Results focus.** Controller Left/Right move between buttons, shoulders
change panels. Explicit focus links and scroll-to-focus must reach replay,
Retry and every offset operation in all layouts. Preserve the keyboard's
existing panel shortcuts; distinguish source when resolving actions. More
also offers panel selection, replay and offset editing. North keeps Retry
(Watch replay on kept results). Settling still consumes accidental input.

**G5 — Controller editing.** Add a current-lane cursor and a controller grid
mode: Up/Down move rows, Left/Right change snap, shoulders select lanes, South
places/removes the current tool, dedicated Start listens/stops. Hold/roll
tools use explicit head/tail presses with a visible pending anchor; Back
cancels the anchor before leaving. West opens the editor actions menu; North
remains its existing alias. Add selection start/end/clear, nudge earlier/later,
jump to start/end/measure and zoom actions, plus Tap tempo with timestamped
South presses in its own mode. Existing menu actions remain available. A
recording keeps the play's bindings and does not enter this grid mode.

**G6 — Typed numeric dialogs.** Use increment/decrement controls with useful
fine/coarse steps and validation, not a text-field-only interface. Separate
speed ratio/duration/units, signature numerator/denominator and combo values.
Keep keyboard direct entry as an alternative. Removing a timing event must
be an explicit controller action, not require deleting text to an empty value.

**G7 — Text handoff checks.** Mirror the search contract for every free-text
dialog: keyboard/IME enters text; controller can submit, cancel, traverse
validation errors and operate the next screen. Do not consume a controller
press as a text character or require an additional keyboard Enter to exit.

**G8 — Platform completion/recovery.** Verify request-level refusal and
continuation for file pickers, clipboard, exports, URLs and media. Preserve
focus, show the key/tap requirement where imposed, and allow controller Back.
Check direct shell callbacks as well as `UiRequest`: for example web Options'
history-import callback explicitly rejects controller activation. Do not
claim these OS/browser-owned interactions are controller-only on every host.

## Source map

- Dispatch/defaults: [engine bindings](../../crates/engine/src/bindings.rs),
  [UI navigation](../../crates/ui/src/nav.rs),
  [desktop routing](../../crates/desktop/src/input.rs),
  [web routing](../../crates/web/src/components/ui_dev.rs).
- Song list/focus/shortcuts: [select](../../crates/ui/src/screens/select.rs);
  [import, management and history dialogs](../../crates/ui/src/screens/select_dialogs.rs).
- [Options](../../crates/ui/src/screens/options.rs),
  [binding dialogs](../../crates/ui/src/screens/options_dialogs.rs),
  [raw binding flow](../../crates/ui/src/state/bind.rs),
  [web Options callbacks](../../crates/web/src/components/options_page.rs).
- [Prompt](../../crates/ui/src/screens/prompt.rs),
  [results](../../crates/ui/src/screens/results.rs),
  [page navigation](../../crates/ui/src/screens/page.rs),
  [global notices](../../crates/ui/src/screens/notices.rs).
- [Editor](../../crates/ui/src/screens/editor.rs),
  [editor dialogs](../../crates/ui/src/screens/editor_dialogs.rs),
  [tap tempo/sound](../../crates/ui/src/screens/editor_sound.rs),
  [recording](../../crates/ui/src/screens/editor_record.rs).
- [Play](../../crates/session/src/play.rs),
  [lane-cover controls](../../crates/session/src/play/cover.rs),
  [gesture requirements](../../crates/ui/src/request.rs).

No gameplay bindings, navigation behavior or hint artwork changed in this
audit. Hardware and full shell journeys remain acceptance work for the fixes.
