# Controller hints and reachability (2026-10-27)

Status: implemented 2026-10-27, including automatic families, overrides,
controller hints and the selected Dance Single defaults. The
[audit](../research/controller-reachability.md) records the earlier reachability
fixes and browser/OS boundaries. Hardware/transport coverage remains unverified;
see the implementation evidence below.
The maintainer requested automatic
Xbox, PlayStation and Nintendo hints, a generic fallback, per-controller
overrides, and a review of default bindings and controller access throughout
the app. Hints must describe the controls that actually work. Dance-pad and
arcade presentation profiles are later work.

## Outcome and scope

Both shells use the same action/binding resolver and hint model. A deliberate
keyboard or controller press selects the hint source, including which
controller supplied it. Recognized controllers show their own button legends;
unknown controllers remain usable with honest generic hints. Options can
override the automatic hint style without changing any bindings.

Controller access is part of this work, not an assumption made after drawing
the glyphs. Every enabled in-app action must have a discoverable controller
path, through focusable controls or an actions menu where a direct shortcut
would crowd or conflict with the existing controls. Audit every screen and
dialog, including the editor, not only the browse/play/retry loop.

**Free-text entry is the explicit exception.** The maintainer clarified during
the audit that this includes search, URLs, titles, artists and other free-text
fields; numeric editing must remain controller-operable. There is no virtual keyboard.
A controller can reach and leave the search field, clear a query, and operate
the search results after text has been supplied with a keyboard or IME. The
results view is classified as reachable after text input, not as a missing
controller route. Entering, changing or composing the text itself is not a
controller requirement. Test the handoff back to the controller without an
extra keyboard confirmation being necessary.

The same handoff applies to other free-text forms: reach, submit/cancel and
continue with a controller after typing. Numeric and compound timing values
need controller controls, not an exemption because they currently use text boxes.
Other discovered gaps must be recorded and resolved or explicitly scoped
with the maintainer. Browser activation requirements and OS-owned dialogs are distinct
platform boundaries, described below, not evidence that an app action works.

Out of scope: virtual keyboards, automatic dance-pad identification, dance-pad
or cabinet hint profiles, WebHID, Steam Input integration, replacing gilrs,
and changes to judging, replay inputs or input timestamps.

## Starting point before this work

- `crates/desktop/src/pads.rs` reads gilrs's name and optional vendor/product
  IDs. Mapped buttons become W3C positions (`w3c_button`); raw devices keep
  their own numbering. IDs currently combine the name and USB IDs when known.
- `crates/web/src/web/gamepad.rs` supplies `Gamepad.id`, slot and standard
  mapping status. `ConnectedPad` in `crates/engine/src/bindings.rs` currently
  carries only `id`, `index` and `standard`.
- `ControlBindings::menu_control` resolves directions, Confirm, dedicated Start,
  Back, contextual Select, More and Previous/Next. `menu_input` retains its
  Confirm interpretation of dedicated Start for existing play callers.
  Saved Start/Back controls take precedence over standard menu defaults;
  unmapped devices use their bound single-layout lanes and Start/Back.
- `MenuEvent` carries the controller slot. `Cx::menu_from_pad` identifies the
  action being handled, but is not a persistent controller identity for hints.
  The web creates new UI hosts across routes, so hint selection must survive
  that handoff as well as desktop screen transitions.
- `crates/ui/src/keycaps.rs`, the shared `key_hint` helper and
  `crates/theme/src/keycaps.rs` describe/draw keyboard caps. The prompt's
  `prompt_controls.rs::Controls` reads keyboard bindings only today.
- The search/controller handoff already exists in
  [song-select navigation](song-navigation.md). Results, Options and the
  editor already handle some contextual Select actions. Preserve their useful
  behavior while auditing the remaining paths.

The reachability audit has now checked these paths and exercised selected
ones in the shared harness. Hardware and full shell coverage remain pending;
the audit distinguishes source evidence from executed checks.

## Detection and saved overrides

Keep three facts separate: the reported device identity, how its controls are
mapped, and the artwork used to label those controls. A recognized brand does
not prove a raw button's physical position; a standard mapping does not prove
the controller is an Xbox model.

1. Preserve structured identification metadata from the desktop boundary;
   do not parse our formatted binding ID back into USB IDs. On the web,
   conservatively extract known identifier formats and recognizable names.
   Preserve the existing binding keys so saved mappings continue to match.
2. Classify known vendor/product pairs first, then specific known names.
   Avoid broad matches such as `Wireless Controller` or `Pro Controller`
   without supporting evidence, and never infer a family from button counts.
   Keep the classifier pure and shared; record why it selected a family so
   fixtures and diagnostics can explain a result.
3. Choose the effective style from the saved override, otherwise detection,
   otherwise Generic. Settings: **Auto, Xbox, PlayStation, Nintendo, Generic**,
   under the controller's existing Options entry. Auto shows the detected
   result; a change updates visible hints immediately.
4. Save the choice by the existing per-controller-model binding identity,
   not the ephemeral slot. Identical units sharing that identity share the
   override; they still have separate active slots and held-input state.
   Do not promise persistence across changed browser IDs or connection modes.
5. Keep presentation settings separate from lane binding data. Selecting a
   hint style must not create an empty saved lane table that suppresses
   defaults. Missing/new settings mean Auto. Resetting bindings leaves the
   hint preference alone; forgetting a controller removes both.

Automatic Sony artwork covers DualShock 4 and DualSense; older/unsupported Sony
models fall back to Generic rather than acquiring incorrect Share/Options art.
Initial Nintendo coverage means the conventional Switch-style button diamond
(Pro controller / paired Joy-Cons when mapped accordingly). Single Joy-Cons,
GameCube layouts and other nonmatching devices must not inherit that artwork
solely because Nintendo made them. Retain model detail when known for center
buttons such as Options versus Start; use a neutral position glyph when the
exact legend is uncertain.

Standard mapped controls can use family artwork. Unknown standard controllers
use positional glyphs (for example, a four-button diamond with its bottom
button highlighted) and neutral shoulder/stick/d-pad marks. The visual study
uses START / SELECT center caps as a familiar legacy convention in
place of the rejected left/right position markers; this does not assert that
the unknown hardware actually prints those names.
Raw/unmapped controls use stable numbered button, axis-direction or hat
labels, consistent with the binding screen. A family override alone does not
establish a raw device's mapping and must not turn `button:0` into an invented
Cross/A/B label.

Emulation may hide the physical device completely. A PlayStation controller
reported as an Xbox controller can only be corrected by the user's override
with the current APIs. Never claim detection has recovered hidden hardware.

## Actions, bindings and hints

Resolve hints in this order:

`screen action → effective binding(s) for this device/context → control glyph`

Use one source of truth for dispatch and lookup. Do not implement a second
table in the theme that assumes Enter = A, Escape = B or Tab = Y. Retain the
distinction between physical positions (South/East/West/North) and legends:

| Position | Xbox | PlayStation | Nintendo Switch-style |
| --- | --- | --- | --- |
| South | A | Cross | B |
| East | B | Circle | A |
| West | X | Square | Y |
| North | Y | Triangle | X |

The resolver must respect saved Start/Back precedence, context, disabled
actions, cleared bindings, alternative controls and two-controller lane
assignment. A control consumed by another action is not a valid hint. Pick a
stable primary binding, with alternatives only where useful and space allows;
missing bindings stay missing rather than falling back to a fictional default.
Repeat counts and hold/double-tap instructions belong to the action, outside
the glyph, as keyboard hint prefixes do today.

D-pad hints can combine a set of directions on one fixed-size body (one,
vertical, horizontal or all four), retaining the family's physical d-pad
shape. Song-list navigation can show all four directions without consuming
more space than one. Treat this as a set of alternative directions, never a
simultaneous-button chord. Build the set only from effective bindings on the
same device; conflicting/remapped directions must not remain highlighted.
Repeat counts and gestures remain separate. Triggers need their own artwork
(LT/RT, L2/R2, ZL/ZR), distinct from shoulders, even though this phase adds no
new trigger menu defaults.

Extend the shared UI's typed hint presentation to controller controls; the
theme owns their artwork. Keep keyboard caps intact. Cover screen footers,
dialogs, notices, countdown/help copy, binding rows, quit/cover instructions
and the prompt's receptor diagram. The diagram must use the same resolved
lane bindings and device assignment as the upcoming play, including two-pad
layouts, rather than simply reading a controller's single table. Identify
each controller's side where more than one is required. Preserve its skin
warmup, demo timing, reduced-motion behavior and automatic Start priority.

Switch the persistent hint source on a fresh intentional key/button press or
an axis crossing the existing activation threshold. Releases, repeats, idle
polls, drift, connection events and synthetic/autoplay inputs do not switch
it. Keep the last source across screens and dialogs. Losing focus or a device
must release held actions and invalidate stale slot identity; slot reuse must
never display the disconnected controller's family. Define and test a stable
keyboard fallback when the active controller disconnects, until another
device is deliberately used. Pointer visibility rules remain the UI's own.

## Default bindings settled by the audit

These defaults are implemented in the shared resolver and screens.
Keep positional Confirm/Back on all families: Xbox A/B, PlayStation
Cross/Circle, Nintendo B/A. This preserves existing behavior and makes a hint
override presentation-only. No automatic Nintendo swap and no new regional
confirm/back preference in this work.

### Shared menu vocabulary

| Control (W3C standard index) | Default action | Rule |
| --- | --- | --- |
| D-pad (12–15), left stick (axes 0/1) | Directions | Existing thresholds/repeats; screen-specific browse/value behavior below. |
| South (0) | Confirm focused item | Never mapped to a keyboard key for dispatch. |
| East (1) | Back / cancel | Text/dialog/pending edit owns cancellation before screen exit. |
| Center-right Start (9) | Confirm in ordinary menus | Dedicated Listen/Stop in editor grid; existing Start during play. Preserve its identity instead of collapsing it with South too early. |
| Center-left Back (8) | Back in menus | Existing bound quit gesture during play. |
| North (3) | Existing contextual Select | Preserve familiar shortcuts below; label the actual action, never merely “Select”. |
| West (2) | More actions | New, only on screens offering More. Opens the same menu as its visible focusable button; no action behind an overlay. |
| Left/right shoulder (4/5) | Previous/next secondary choice | New contextual shortcuts below; no global page or gameplay meaning. |
| Triggers, stick clicks, right stick, guide/system button | No new menu defaults | Keep gameplay bindings; do not require OS-reserved buttons or hidden chords. |

Use the existing saved Start/Back bindings and precedence. A reassigned
control wins over these default shortcuts, so a conflicting West/shoulder
shortcut disappears from hints but its visible focus/menu path stays usable.
No new generic remapping UI is needed for this phase. Explicitly cleared
gameplay tables stay cleared; standard menu fallbacks currently work
independently and keep doing so. No destructive migration of saved bindings.

Resolve controls in context before losing which physical control was pressed.
`menu_control` and `MenuAction::ControllerStart` preserve dedicated Start until
the screen context is known; ordinary menus normalize it to Confirm. The
editor grid keeps distinct Place and Listen actions. Both shells use this
contract. Raw capture and recording retain their own dispatch.

### Screen defaults and complete alternate paths

| Context | Directions | South / Start | North | West | Shoulders |
| --- | --- | --- | --- | --- | --- |
| Start, pages, ordinary dialogs | Focus/scroll | Activate focused item | No new shortcut | Only if a More menu exists | No default |
| Song list | Existing song navigation and double-tap chart gestures | Open pack / play selected chart | Next style with available charts, as today | More | Previous/next chart on selected song; clamp at ends |
| Song-select header | Move focus | Activate focused control | Existing next-style shortcut | More | No chart changes while using a field/dialog |
| Options | Up/Down row; Left/Right value or action-button selection | Change value / activate selected action | Next section | More, including Calibrate | Previous/next section, wrapping like North |
| Ready prompt | Up/Down option chain; Left/Right value on row | Start from rows; activate Back/Save when focused, as today | No new shortcut | No menu unless actions are added | No default |
| Loading prompt | No hidden shortcuts | Existing start/skip behavior only where offered | None | None | None |
| Results/history-result view | Move focus; reveal focused controls while scrolling | Activate focused item | Retry, or Watch replay for kept results, as today | More | Previous/next panel set, clamped |
| Editor grid | Up/Down snap row; Left/Right snap size | South places/removes; dedicated bound Start listens/stops | Existing editor menu | Same editor menu | Previous/next lane, clamped |
| Import dialog | Focus/report scroll; Left/Right on tabs | Activate | Next tab where URL import exists | None | No default |
| Binding navigation/review | Focus steps/actions | Arm Capture / activate selected action | None | None | None |

Back cancels the current modal/pending operation, then returns to the caller;
gameplay keeps its hold/double-tap quit. Controller results Left/Right now
become focus navigation because the audit found offset and replay controls
stranded by global panel switching. Keep the keyboard's existing Left/Right
panel shortcuts. On compact results, reveal focus automatically rather than
making the user scroll blindly before focus can move.

More has a visible ordinary focus path on each screen that uses it. Its
contextual entries include every otherwise keyboard-only operation, and
secondary choices also have focusable selectors/menu entries. Thus West,
North and shoulders are conveniences rather than requirements for a custom
controller that only has directions and Confirm/Back bound. A fresh Up from
the first or empty song row reaches the header, then its More/Search controls;
another Up can reach global notices. Preserve the established browse timings.

In the editor, hold/roll placement uses two explicit South presses for head
and tail; Back cancels an unfinished anchor. The actions menu exposes range
start/end/clear, nudge earlier/later, jump start/end/measure, zoom, tools,
undo/redo, timing, metadata, recording, playtests and saving. Tap tempo is an
explicit mode/dialog: South taps while its Tap control is focused, Apply
keeps the estimate and Back leaves. No special edit shortcuts leak into a
recording, which keeps the chart's real gameplay bindings and lane priority.
Numeric dialogs have controller increments/decrements, fine/coarse steps,
structured compound fields and explicit remove/reset actions. More also has
Place/remove for custom controllers without a separate South button; dedicated
Start confirms More while that button has focus. The shared action list pages
to fit the available height, including compact screens and chrome notices.

Binding capture must not reserve South/East/West as UI actions while recording
a candidate button. For standard controllers, separate navigable step/review
controls from explicitly armed capture so Skip/Redo/Save/Cancel work. Preserve
the existing unmapped-device learning path and hold-to-cancel; learned controls
can supply review navigation once available. Do not require family detection
or fabricate a standard mapping to navigate raw devices.

### Gameplay defaults

Use the selected shoulders/triggers for new Dance Single defaults. Existing
saved tables, including old defaults and cleared lanes, stay exactly as saved.
Other layouts retain their previous defaults. Menu coverage is not a promise
that every chart fits on one handheld controller:

| Layout | Default |
| --- | --- |
| Dance Single | Left/Down/Up/Right = LB/LT/RT/RB (PlayStation L1/L2/R2/R1; Nintendo L/ZL/ZR/R), followed by D-pad directions and West/South/North/East alternatives. |
| Dance Solo | D-pad/face controls plus left/right shoulders for upper diagonals. |
| Techno 8 | Solo controls plus left/right triggers for lower diagonals. |
| Dance Double | One controller: d-pad left side, face buttons right side. Two controllers: existing per-side assignment from `ControlBindings::plan`. |
| Pump Single | Down-left = d-pad Left/West; up-left = Up/North; center = either shoulder; up-right = Right/East; down-right = Down/South. |
| Pump Half-double / Double | Existing panel mapping split across d-pad/left shoulder and face buttons/right shoulder; retain saved/per-controller side assignment. |
| DanOni 5, 7, 9A, 9B | No automatic handheld defaults in this phase. Use per-layout binding setup; the existing generated Dance/Pump conversions keep their own defaults. |

The explicit DanOni decision avoids silently assigning arbitrary controls or
overwriting a player's setup. Options must remain reachable to bind those
layouts, the diagram must show unbound lanes honestly, and the game must not
advertise a default controller mapping where none exists. Retain supported
layout detection and every saved/cleared table unchanged.

## Reachability audit and completion matrix

The [completed baseline audit](../research/controller-reachability.md) contains
the action matrix, focus-search results, source references, test evidence and
fixes G1–G8. It found wide song-select/footer gaps; history/editor entry gaps;
an empty-results focus trap; inaccessible binding Skip/Redo; stranded results
controls; editor grid, tap-tempo and numeric-entry gaps. Update that matrix as
the fixes land, distinguishing source evidence, shared harness coverage and
real shell/hardware checks. Family hints now follow the last deliberate input source.

| Area | Required audit cases / known leads |
| --- | --- |
| Start and global chrome | Singleplayer, Options, desktop Quit; Back, first-run notices, calibration offers, errors, dismiss/confirm, focus restoration. Disabled future features are not actionable targets. |
| Song select | Packs, songs, every chart/difficulty/style, mods/Options, Add songs, management, history and best replay, editor entry, return to Start. Check the P/W/E/Delete paths against actual focus access; add an actions menu where needed. |
| Search | Reach/leave field, cancel typing without clearing, clear query, empty and nonempty results, alternate-style suggestions, choose/play a result, return retaining filter/place. Text entry alone is exempt. |
| Options | All categories/sections/rows, long-list scrolling, values, resets, preview-layout selector, folders, controller overrides, bind/rebind/clear/defaults, capture cancel, disconnected devices. |
| Prompt/loading | Start, cancel countdown, every modifier, save defaults, Back while loading, failures/retry; correct controller lane diagram. |
| Play/replay/calibration | Lane bindings, quit gesture, lane covers, calibration choices/outcomes, interruption/reconnect and return paths; no hint-only remapping. |
| Results/history | Retry, song list, replay, panel pages, compact scrolling, offset controls, saved-play details and any management/confirmation dialogs. |
| Editor | Entry and exit, cursor/lane/tool selection, placement/removal, hold creation, range selection, editing actions, timing, view/audio controls, record, listen, playtest, save/export and unsaved-change dialogs. Existing North opens an actions menu; number-key lane editing and modifier-only operations need explicit controller-path review. |
| Import, metadata, other pages | Folder offers, import tabs/progress/cancel/errors, new song, URL input, metadata/numeric fields, Credits and links. Free text may use a keyboard; numeric controls and post-text controller handoffs are required. Record external-dialog dependencies separately. |

The editor and import screens remain in scope. Free-text entry is explicitly
exempt; numeric editing is required, including compound timing values.
Unresolved app-owned reachability gaps prevent completion of this plan.

**Platform boundaries.** `UiRequest::needs_gesture` / `pad_may_start` already
distinguish controller-startable audio from operations requiring browser
activation (file pickers, clipboard, links, text focus); Safari audio may also
need a tap/key. Audit the actual request and refusal/recovery path on each
shell. Keep focus, communicate the required key/tap and allow controller
cancellation/continuation afterward. Report these limits separately; do not
fake trusted events, claim a virtual keyboard would solve them, or count an
OS-owned file dialog as app-provided controller navigation. Confirm any limits
remaining in the release with the maintainer before claiming full coverage.

## Implementation sequence

The selected [controller-button study](../../tools/controller-design/README.md)
now supplies `assets/ui/controllers.png`, original rendered art in shared
charcoal satin with family-specific faces, shoulders, triggers and D-pads.
D-pads are optically larger than face buttons; generic START/SELECT are smaller
than shoulders. Released/pressed cells provide approximately 1 px optical
travel, surface dimming, a trigger hinge cue and active-arm-only D-pad presses.
The retained HTML study includes the alternative binding rows and jump examples;
the running prompt keeps its existing per-lane demonstration and reduced-motion
behavior. Keyboard artwork is unchanged.

The maintainer prioritizes larger UX hurdles before dedicated accessibility
work. The study's accessibility references are background research; new
accessibility scaling/high-contrast features are deferred, not prerequisites
for controller hints. Preserve existing accessibility behavior while focusing
this phase on recognizable controls and correct binding-derived presentation.

1. **Audit and settle defaults — done 2026-10-27.** Baseline matrix and fixes
   G1–G8 recorded; defaults above are the implementation contract. No runtime
   behavior changed. All free-text entry is exempt by the maintainer's
   clarification; numeric editing stays in scope.
2. **Controller actions and reachability — implemented 2026-10-27.** Shared
   contextual resolver, More menus, shoulder actions, empty-search recovery,
   results focus/scrolling, armed standard-controller capture, controller
   editor operations and numeric forms cover G1–G7. Physical-edge regression
   journeys exercise the real theme at three sizes, text/IME handoff and raw
   minimal-controller editing. G8 retains explicit browser activation and
   OS-dialog boundaries; file-picker refusal keeps the dialog recoverable.
3. **Shared hint resolution — implemented.** `ControlBindings::menu_controls`
   reverses the effective dispatcher. `ui::controller_hints` selects actual
   action controls and play assignments; unbound/conflicting controls stay
   unbound. The active whole-field controller owns its diagram; split fields
   show both controllers with Pad 1/2 labels. Explicitly empty lanes never
   borrow another controller's whole-field binding.
4. **Identification and settings — implemented.** Desktop carries USB metadata
   without changing binding IDs. Shared conservative classification reads it
   or known browser identity formats/names. Generic fallback and Auto/Xbox/
   PlayStation/Nintendo/Generic overrides are separate from bindings. Unknown
   raw controls remain numbered/axis/hat controls under every override.
5. **Artwork and screens — implemented.** Shared footers, song-list style hint,
   pre-start receptor diagram, binding rows and quit/cover copy follow effective
   controls. Both shells retain the latest deliberate device across routes;
   fresh key/button/threshold edges switch source. Repeats/releases do not.
   Disconnect/slot replacement clears old UI directional holds and falls back
   to keyboard hints. Overrides take effect immediately.
6. **Automated verification — implemented; physical hardware checks pending.**
   Engine, UI, theme and session suites cover saved/empty tables, reset/override
   separation, inverse dispatch, raw devices, split fields, fresh/repeat/release
   source changes, host replacement and disconnect. Desktop offscreen GPU tests
   render Xbox/PlayStation/Nintendo/Generic footers and the real skin's prompt,
   including its pressed state. Native and wasm builds are checked. These are
   synthetic controller identities and inputs; they do not establish USB,
   Bluetooth, browser, Steam Input or Nintendo backend fidelity on real devices.

Verified in this checkout: `cargo test -p ddi-engine -p ddi-ui -p ddi-theme
-p ddi-session`; native desktop library and binding-flow tests with default
features off and an isolated home; the offscreen GPU test above; shared-crate
Clippy with warnings denied; and `cargo check -p ddi-web --target
wasm32-unknown-unknown`. The visual proofs use sample song-list data and an
actual bundled chart/skin in the pre-start host. No physical controller or
USB/Bluetooth browser matrix was available for this run.

## Verification and acceptance

- Pure tests: known and ambiguous IDs, unknown devices, masked identities,
  raw versus standard mapping, manual override precedence and Auto fallback;
  migrations preserve bindings and explicit empties. Fixtures record platform,
  transport, reported mapping and actual button positions when known.
- Resolver tests: hints activate the named action through the real dispatcher,
  saved precedence/conflicts, contextual actions, alternatives, unbound controls,
  holds/pairs, and single/two-controller lane assignments. A glyph-family change
  changes presentation only. Verify Nintendo positions on the actual backend
  mappings instead of assuming every SDL-derived mapping is identical.
- Input-state tests: multiple families, identical controllers, fresh input,
  drift, repeat/release, raw binding capture, focus loss, reconnect/slot reuse,
  overlays and web route transitions. No stuck or leaked actions.
- UI harness journeys: Start → browse → change chart → prompt/options → play
  → results → retry/history/replay → back; all matrix actions and dialogs,
  compact/wide layouts and reduced motion. Search tests explicitly supply text
  with a keyboard/IME, then use only controller input for results and return.
  Include no matches and suggestion states; never inject text and describe
  the whole journey as controller-only. Repeat that handoff for other free-text
  forms, and verify numeric editing without text injection or keyboard input.
- Shell checks: feed physical fake-pad controls through each shell's real
  binding dispatch as well as testing semantic `MenuAction`s. Verify browser
  activation behavior without the autoplay override; keep the documented
  published-URL Safari check. Check desktop native dialogs separately.
- Visual checks: Xbox/PlayStation/Nintendo/Generic, long/raw labels, small
  screens, high scaling, changed bindings, two-pad diagrams and live switching.
  Glyphs must remain recognizable without color and fit the existing hint
  spacing. Use original vector artwork or appropriately licensed assets.
- Hardware checks: representative Xbox, DualShock/DualSense and Switch-style
  devices, USB/Bluetooth where available, and a generic/unmapped device across
  desktop and supported browsers. Record unavailable combinations as untested;
  synthetic IDs alone do not establish real detection reliability.

Done means correct binding-derived hints, persistent overrides, conservative
fallbacks, and evidence for every enabled in-app action in the matrix. Search
results and other free-text workflows count as reachable after text entry.
Numeric editing must be controller-operable. Additional product exceptions
require an explicit scope decision; platform limits and untested hardware
remain visible in the completion report. Dance-pad artwork/profiles remain a
separate future plan; existing raw-pad bindings must continue to work.

## References

Reviewed 2026-10-27; these inform detection limits and positional semantics,
not a guarantee of a particular browser/controller combination:

- [W3C Gamepad specification](https://www.w3.org/TR/gamepad/): unspecified ID
  string format; standard mapping describes physical positions.
- [gilrs Gamepad API](https://docs.rs/gilrs/latest/gilrs/struct.Gamepad.html):
  OS/mapping names, optional vendor/product IDs and mapping source.
- [SDL3 migration notes](https://wiki.libsdl.org/SDL3/README-migration):
  positional face buttons versus printed labels; confirm/cancel preferences.
- [Valve's emulated-controller guidance](https://partner.steamgames.com/doc/features/steam_controller/steam_input_gamepad_emulation_bestpractices):
  emulated Xbox identity can hide the underlying controller family.
