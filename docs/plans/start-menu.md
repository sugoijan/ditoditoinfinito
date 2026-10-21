# Start menu and returning between screens (2026-10-20)

Status: implemented 2026-10-21. This records the maintainer's goals and
the first version; verification notes follow the acceptance checks.
Builds on [the UI core](ui-core.md),
[Infinito](theme.md), [song navigation](song-navigation.md) and
[the theme research](../research/stepmania-themes.md).

## Goals

- A start menu before song select, with Singleplayer as the main entry.
- Escape and Backspace perform the same menu Back action; backing out of
  song select reaches the start menu and stops its preview.
- A disabled Multiplayer entry hints at future support without defining it.
- Options is obvious here as well as on song select, and returns to its caller.
- Desktop can quit from inside the app.
- The screen fits the named-screen and actor structure needed for future
  imported themes.

## Screen

A game title above one short vertical menu, using Infinito's text face,
pink accent, focus treatment and dark ground:

| Entry | Behavior |
|---|---|
| **Singleplayer** | Default focus; opens the existing song list. |
| Multiplayer · Coming soon | Muted but readable; focusable by keyboard/pad, but cannot activate. |
| Options | Opens the same Options screen used from song select. |
| Credits | Opens the credits and returns to its caller. |
| Quit | Desktop only, last; uses the ordinary application shutdown path. |

Multiplayer has no destination, tooltip describing a mode, lobby, network
setup, player count or new session model. Pointer/touch activation does
nothing. Singleplayer keeps all existing layouts, including Double;
the label describes the player flow, not a chart layout.

The menu is silent, including after coming back from song select. No
title music or attract-mode autoplay in this version. Use the theme's
own background, independent of the last selected song. Keep transitions
brief and honor reduced motion; they must not delay stopping audio.
No extra “press Start” gate before this menu.

Keep the main choices large and readable from a pad, with at least 44 px
touch targets. The same column fits portrait and landscape. A secondary
Credits entry precedes desktop-only Quit at the bottom; calibration remains reachable through
Options and the existing device notices. The existing song-list shortcuts
remain available. Returning from Options restores focus to Options;
returning from song select focuses Singleplayer.

## Navigation contract

| From | Action | Destination |
|---|---|---|
| Normal launch | — | Start menu |
| Start menu | Singleplayer | Song list, restoring its place |
| Song list, ordinary browsing | Back | Start menu, preview stopped |
| Start menu or song list | Options | Options, with caller remembered |
| Options | Back or its visible Back button | The caller |
| Options → calibration menu | Back from that menu | Options, then its original caller |
| Start menu → Credits | Back | Start menu |
| Song list → Credits | Back | Song list |
| Pre-start or results | Existing return-to-songs action | Song list |
| Start menu | Back | Remain on start menu |

Gameplay's existing quit gesture stays separate from menu Back. Quit
requires activating its explicit entry; repeated Back never quits.

### Back means leaving, with local interaction first

Map unmodified Escape and Backspace to the shared menu Back action, after
text input and binding capture have had their turn. Controller Back and
the visible Back button follow the same navigation policy. Consume each
press once; repeats or a held button must not cross multiple screens.
Entering Singleplayer also must not pass the activating Enter/Start on
to the song list and accidentally start a song.

An open dialog, popup, binding flow or focused text editor handles its
own cancellation first. Backspace still deletes text; IME composition
retains its keys. Existing modified shortcuts, including Cmd/Ctrl+
Backspace for management, keep their meaning.

Once ordinary browsing has focus, **one Back leaves song select**, even
with an expanded pack or an active search filter. It no longer clears
the filter or collapses the pack first. Pack headers still toggle with
activation; search needs an explicit clear control available to pointer,
touch and keyboard users. Keep the filter and browsing position when
leaving and restore them on reentry. Cancel provisional arrow pairs,
holds and wheel gestures before leaving so no delayed selection fires.

The compact Back button and a visible desktop Back affordance say where
they lead; update hints to show Esc / Backspace → Start menu. Options'
return text becomes generic Back, rather than promising the song list.

### Caller and browser history

Keep caller/return information in navigation state, not in saved settings
or in one global “last screen” variable. Returning through calibration
or another child page must preserve the outer Options caller and the
Options tab/scroll/focus. Model the menu destinations and return rules
in shared, platform-free code; let each shell execute them using its
existing screen stack or routes. Avoid rebuilding the whole router.

Web addresses: `#/` becomes the start menu and `#/select` is
song select. Existing play, replay, Options, calibration and developer
deep links continue to open directly. A fresh direct Options or Credits
link has Start as its fallback parent; a direct play still returns to
Songs. A fresh direct song-list link has Start as its logical parent.
Old root bookmarks now land on the start menu deliberately.

Browser Back/Forward must agree with visits made inside the app. An
in-app Back pops the matching app-owned history entry when available;
otherwise it replaces the route with the logical parent. It must not
push a new parent entry and create a Back loop, nor call browser Back
blindly and send a direct-link visitor to an unrelated site. Ordinary
browser navigation beyond the app's history remains browser behavior.
Retain return context in app-owned history state where reload needs it;
missing context uses the fallbacks above.

Audit every existing `Route::Home` use: some mean the root, but most
current play/results/error returns mean Songs. Preserve the session's
`gfx`, `auto`, `bias` and `slow` parameters across the new root and
selection routes, including the autoplay switch's address updates.

Desktop defaults to Start, gains `--screen start`, and keeps explicit
`--screen select`, `--play`, `--replay`, calibration and developer
launches. Returning to the menu preserves settings, imported songs,
history and the existing persisted song/chart/pack selection. Search
remains session state, following the current persistence contract.

## Audio and lifecycle

Preview playback belongs to active song select. Leaving for Start or
Options stops the source, cancels settle/loop timers, and invalidates
pending fetch/decode results before the outgoing screen finishes its
transition. Late completions, library refreshes and device recovery must
not restart sound behind Start. Options' explicitly requested volume
sample belongs to Options and stops when it leaves.

Web already has `select_preview::close()` for full cleanup, called by
`SelectPage::destroy`; its `stop()` alone does not invalidate pending
loads. Use the full lifecycle at navigation time and keep destruction
idempotent. Do not start or resume preview audio from a generic start-menu
listener. Singleplayer's activation must establish/resume the audio
context synchronously in the gesture, before route mounting or any
await; playback can then follow the selected song's normal loading path.
Keep the existing tap/key fallback where a browser does not accept pad
activation. A non-gesture return must never bypass browser restrictions.

Desktop currently resumes paused preview audio when the top layer is
`select` (`shell.rs`). Gate starts, resumes and pending decode completion
on active song select, including when screens are retained underneath
another screen. The menu can appear while the library scans, but scan
completion must not select a screen or start preview audio.

Quit emits a shell request and follows the same shutdown routine as the
window's CloseRequested event, including `Songs::shutdown()` and waiting
for the importer to drain. Stop audio immediately and prevent duplicate
requests while closing. No extra confirmation in the ordinary case.
Hide Quit on the web, and reject an unsupported quit request there too.
First-run notices remain app-wide; the song-folder offer belongs to
entering Singleplayer so it does not obscure a fresh start menu.

## Theme boundary

StepMania's fallback declares a `ScreenTitleMenu`, derived from
`ScreenSelectMaster`, with named choices for starting, options and exit;
the exit choice targets `ScreenExit`. This is a useful structural
precedent, rather than a requirement to reproduce its full boot flow.
Sources checked 2026-10-20: [fallback metrics](https://github.com/stepmania/stepmania/blob/5_1-new/Themes/_fallback/metrics.ini#L1816-L1895)
and [ScreenTitleMenu](https://github.com/stepmania/stepmania/blob/5_1-new/src/ScreenTitleMenu.cpp).

Implement our start screen in `ddi-ui`, dressed through `ddi-theme`,
with a documented correspondence to `ScreenTitleMenu`. Give choices
stable semantic IDs, separate from labels and platform destinations;
make enabled/visible state explicit. Use the existing actor lifecycle,
layering and transitions, with title-menu component names and tunables
in `infinito.toml`. Theme data controls presentation; the host controls
available actions, return behavior, audio ownership and shutdown.

This prepares a title-screen boundary for the StepMania/ITGmania theme
work described in the existing research. It does not implement imported
metrics, Lua screen branching or whole-theme compatibility. In
particular, do not reproduce StepMania's game-state reset on title-menu
construction: backing out here must preserve the player's choices.

## Implementation sequence

1. **Navigation and input.** Add the shared destination/return contract
   and semantic Back alias, rename `options_shell::Leave::Songs` to a
   caller-relative return, and update song select's Back/search behavior.
   Cover local cancellation, nested returns and input carryover first.
2. **Shared start screen.** Add the choices, platform capability, focus,
   responsive Infinito layout and sample/developer-screen support. Use
   stable IDs in the harness and debug state.
3. **Shell integration.** Add web routing/history and the desktop root;
   audit existing Home returns and command-line entry points. Connect
   synchronous web audio activation, preview cleanup and native Quit.
4. **Verification and documentation.** Check both shells and update
   AGENTS.md's launch/routes, the verification skill's recipes,
   `song-navigation.md`, `theme.md` and the main plan's current behavior
   when this is implemented. Keep this plan and its index status current.

## Acceptance checks

- Shared harness: default focus; disabled Multiplayer cannot activate;
  desktop/web choices; keyboard, pad and pointer navigation; local text,
  binding and dialog behavior; exactly one Back per press; returning
  focus; preserved song/chart/search/pack; no stale arrow-pair callback.
- Navigation: Start → Options → Back; Songs → Options → Back;
  both with a calibration detour; Credits from each caller; direct
  Options and direct play; repeated visits, reload and browser
  Back/Forward without loops or unexpected exits.
- Audio: leave during settle, fetch, decode and looping preview; finish
  the pending work while on Start and assert silence; reenter and play
  the restored selection; leave Options with a volume sample active.
  Include library refresh and device recovery while on Start.
- Desktop: default and explicit launches; Quit emits one shutdown
  request, waits for active import cleanup, and exits through the normal
  path. Use fake audio/offscreen tests where possible, and scratch
  `DDI_HOME` plus the watchdog for any app run.
- Browser: static-bundle Chrome and Firefox checks using the verification
  skill, including WebGPU/WebGL2 and portrait layout. Exercise real
  activation rules without the autoplay override. The maintainer checks
  Singleplayer activation and returns in Safari on the published origin.
- Visual checks: desktop, narrow landscape and portrait; notices visible;
  readable disabled text; reduced motion; no last-song backdrop or pulse
  leaking into Start. No engine, judging or replay changes are required.

## Implementation and verification (2026-10-21)

Implemented in `ddi-ui`'s `StartScreen`, dressed by the `TitleMenu.*`
components and metrics. Native keeps callers on the UI stack; web keeps
app-owned ancestry and view position in each history entry. Covered screens
no longer publish selection/audio updates; Options republishes its field
preview when uncovered. Native Quit and window Close share an idempotent
shutdown request and the existing importer drain.

Checks passed:

- `cargo test -p ddi-ui -p ddi-theme`, including title focus, disabled
  choices, held input, responsive touch targets, search and caller returns.
- Desktop no-default-features unit tests and `--test start_menu`, with a
  scratch home and silent audio: library updates under Options, pending
  preview cancellation, retained song/chart, calibration detour and field
  preview restoration, and Quit request/idempotence.
- Clippy with warnings denied for UI/theme/desktop (all targets, without
  video), and the wasm web shell; formatting and whitespace checks.
- Static-bundle Chrome WebGPU/WebGL2 and isolated Firefox WebGL2: both
  Options callers with calibration detours, reload, Back/Forward, direct
  links, search preservation and Clear, disabled Multiplayer, silence on
  Start, and portrait/landscape layouts (1280×720, 390×844, 844×390).
- Chrome with normal audio activation: Singleplayer starts the preview;
  leaving during a held fetch or decode discards its late result; leaving
  an active Options sample stops it. Direct play → Songs → Start returns
  without a history loop. The song-list header was visually checked too.

The test bundle used `DDI_NO_VIDEO=1`, release LTO off and 16 codegen units
under the memory watchdog (the default fat-LTO link exceeded 2 GiB).
No production build-profile setting changed. Safari activation on the
published HTTPS origin remains the maintainer's manual check; active-import
Quit cleanup reuses the existing drain path and was not exercised against
a live importer in this run.

Refined after the maintainer's web/desktop check: Quit follows Credits, and
Multiplayer has a muted button plate with the ordinary focus treatment.
Keys and pads can select it; Enter, Start and pointer/touch activation do
nothing. Its label continues to say Coming soon.
