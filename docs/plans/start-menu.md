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

A Dosis ExtraBold wordmark above one short vertical menu, using Infinito's
text face for the choices, pink accent, focus treatment and dark ground.
The title's D/D/I have Vivid outlines with a 12-unit outline and 12-unit
clearance around every letter. Both palette phase and the texture's folds
evolve continuously; reduced motion freezes both. See
[the wordmark implementation](theme.md#dosis-title-wordmark).

The web page's loader shows the same title, rain and a bar painted
alike from the page's first bytes (2026-10-28; `site/boot-art.js` on
the data `tools/title-design/boot_title.py` writes to
`site/boot-art-data.js`: the study's shaped geometry, seven distinct
outlines placed per glyph, and the rain atlas's two outlines with its
stroke weights). On one canvas: the rings cut by the same clearance as
the atlas and painted by the Vivid material ported from its WGSL (the
prelude's hash and noise, the drift, warp, folds and palette window;
CPU doubles for the GPU's floats, each initial's field centered as
`wordmark_geometry.rs` has it, at most half the device's pixels), the
rain as `crates/theme/src/rain.rs` computes it particle for particle
in the menu's layout (its stationary field sampled at each outline's
middle, at 35% saturation), and the progress bar's done part painted
as the focused button's outline is (`crates/theme/src/active.rs`: the
field a square as wide as the viewport, its seconds eight times the
clock's). The boot script lays them out where the menu's title,
Singleplayer and rain land, mirroring the virtual screen, the narrow
fit and this screen's layout, and the art runs on the wall clock
(seconds since the epoch: the page's `performance.timeOrigin` plus the
frame's time), which the screens' live art, the focus outline and this
screen's rain now run on too (`ddi_ui::screens::Live`, `Ui`'s active
time and the background time take the shell's clock, `Ui::set_clock`,
rather than counting from each screen's start: the wordmark keeps its
phase from screen to screen as well, and neither a reload nor a
desktop start begins the art over from the same state), so the title's
colors, its folds and the rain's particles are the game's at the
handoff.
The menu built under the shell stands its title in place from its first
frame (`StartScreen::in_place`: its `On` emptied for that one entrance,
the choices still coming in from below; the next entrances, coming back
from another screen, slide it in as before), and the shell fades once
that screen is at rest with everything the first screens need in
(`boot::ready_when_settled`; the web's `preload` loads, from the first
host's first frame, the theme's essential images at the sides the
screens draw them, `ddi_ui::screens::essential_images`, the song
manifest, the skins' index and the skin the settings name readied for
their style on the app's field preview, the SVG worker warmed at boot
so the rain's atlas, an SVG, is not the first to pay for it; an 8 s
cap), the bar's last stretch running on those loads and the screen's
rest so that it fills as the fade begins (`boot::settling`; the
maintainer chose to wait for the skin too: a snappy first play over
showing the menu a moment sooner; the skin, many files and hundreds of
SVG drawings, is waited for 4 s at most, and not at all where it
should take longer, loading on behind the menu from the fade where a
link is slower; frames keep coming while the shell waits, a still
screen under reduced motion asking for none). The maintainer then
asked for a bar that moves evenly, a little pessimistic, its last bit
animated, wondering how much its pauses depend on the link and the
device: they depended on both, the bar's parts being fixed shares
(the download four fifths, the skin four of twelve units) of stretches
whose times vary with each (on a fast link the download takes 0.4 s
and the first screen 1.3 s, most of it the skin's drawings; at
6 Mbit/s the download 12 s and the screen 5 s). So each part is now
weighted by the milliseconds it should take: the boot script measures
the link over the download's first half and sizes the download's share
against the rest's expected time (the rest's bytes and own time as the
app left them in `localStorage` at the last fade, `ddi.boot.v1`); the
app weighs the skin's files by their bytes (the index sizes them; they
are all asked for at once once it is in, texts first,
`noteskins::prefetch`, where before the load fetched them one round
trip after another) on that link and its drawings by the count its
last load took (`ddi.preload.v1`), a texture and a round trip at fixed
costs; and the bar as drawn (`site/boot-art.js`) follows what is
reported at a bounded pace under 0.95 of it, creeps a little while a
report is late, and fills the rest over 0.24 s once the app is ready,
the shell fading after. Measured cold with Chrome's throttling: the
longest hold between percent steps is about 0.13 s on a fast link,
0.4 s at 40 Mbit/s with 100 ms of latency, and about 1 s at 6 Mbit/s
with 300 ms (the first bytes' wait, then the SVG worker's module
behind the theme's atlases). The menu then just appeared under the
fade, its choices' entrance having played under the shell while it
waited, and a direct play route showed its prompt before the skin was
in, the shell fading with no UI host up yet; so the screens built
under the shell keep their entrances (`Ui::hold_entrances`: each
part's `On` kept as its `RevealMessage` and the part unseen, through a
relayout too, `screens::relaid`) and play them from the frame the fade
begins (`Ui::reveal`, `boot::revealed`, the host woken for it), the
title standing in place meanwhile (`Cx::holding` holds the restoring of
its entrance back) and the prompt's card waiting to fade in; the play
route tells the shell a prompt is coming (`boot::expect_host`), so the
shell waits for the prompt's skin and textures as for a page's (10 s
for the skin rather than 4, as the diagram shows it from the first
frame; the play's own skin load waits for the fade, the preview's load
and the entrance, where it drew the same skin twice at once, 810
drawings for 405, which Firefox's slower SVG workers paid for; since
2026-10-30 it takes the preview's drawings instead,
`docs/plans/shell-sharing.md`), and fades at once on a song that fails to
load. The maintainer then saw the menu's entrance stutter at the fade,
smooth between the screens: a 150 ms task landed on it, the skin warm
`lib.rs` starts 1.5 s after boot loading the skin a second time and
parsing its components again on the main thread; the warm now only
fetches the rest of the skin chain's files, 1.5 s after the fade, a
skin put off on a slow link is readied 1 s after the fade, and no long
task or dropped frame follows the fade in Chrome, so the menu
appears whole under the title rather than its title sliding in or its
rain arriving after the loader's left (the maintainer saw it come
late on a cold load). Reduced motion
holds both titles at time zero. The first run's device notice lays the
menu out lower than the loader placed the title; the loader cannot know
it, and that one visit shows the title move.


| Entry | Behavior |
|---|---|
| **Singleplayer** | First-key focus; opens the existing song list. |
| Multiplayer · Coming soon | Muted but readable; focusable by keyboard/pad, but cannot activate. |
| Options | Opens the same Options screen used from song select. |
| Credits | Opens the credits and returns to its caller. |
| Quit | Desktop only, last; uses the ordinary application shutdown path. |

Multiplayer has no destination, tooltip describing a mode, lobby, network
setup, player count or new session model. Pointer/touch activation does
nothing. Singleplayer keeps all existing layouts, including Double;
the label describes the player flow, not a chart layout.

Since 2026-10-27 the theme's background is slow outline rain: the four
cardinal Infinito arrows, with a 1% chance of an upright onigiri on each
offscreen respawn. All outlines sample one stationary, muted Vivid field
across the viewport. Directions stay fixed during each fall; the center
is quieter behind the menu. The theme owns the artwork and motion through
`TitleMenu.Background`; the shared start screen supplies elapsed time,
preserves it on resize and pauses it for reduced motion. Both shells use
the ordinary 2D painter, with no extra browser layer. `TitleMenu.RainSpeed`
and `TitleMenu.RainStrength` tune its pace and visibility.

A fresh menu, including a browser reload, starts with no focused choice or
active outline. The web shell clears the start route's saved focus at app
boot; returning to it within the running app still restores focus. The first key
or controller/menu-button press focuses Singleplayer and is consumed without
activation or navigation. Subsequent input behaves normally. Mouse hover
moves focus to a button without activating it; leaving the buttons retains
the last focus. Clicking/tapping a button still activates it directly.
Explicit return-focus requests (such as Options) retain their place.

The choices use neutral plates, including Singleplayer. Focus uses an
Infinito flowing outline with the title's color-cycle pace and a faster
spatial evolution (8× initially), replacing the pink ring. Its 3 px line
has a 3 px gap to the plate and reveals one viewport-anchored field shared
by all choices. Singleplayer keeps its larger, heavier label. Reduced motion freezes the live paint.

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
| Start menu | Back | Remain on start menu; on desktop, focus Quit (2026-10-22) |

Gameplay's existing quit gesture stays separate from menu Back. Quit
requires activating its explicit entry; repeated Back never quits: on
desktop the first Back moves the focus to Quit and further ones do
nothing.

### Back means leaving, with local interaction first

Map unmodified Escape and Backspace to the shared menu Back action, after
text input and binding capture have had their turn. Controller Back and
the visible Back button follow the same navigation policy. Consume each
press once; repeats or a held button must not cross multiple screens.
Entering Singleplayer also must not pass the activating Enter/Start on
to the song list and accidentally start a song.

An open dialog, popup, binding flow or focused text editor handles its
own cancellation first; Back with song select's search field focused
(reached by Up from the list's top) returns to the list where it was
(`song-navigation.md`). Backspace still deletes text; IME composition
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
selection routes.

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
