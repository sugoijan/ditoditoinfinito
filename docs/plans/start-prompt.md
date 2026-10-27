# The start prompt, loading in the background (2026-10-24)

The maintainer's request (2026-10-24): a heavier default noteskin takes a
while to load, and Start on a song's prompt waits for it. So:

1. The prompt readies everything a play needs in the background and never
   blocks Start; a loading screen shows only when Start comes before that
   work is done.
2. The loading screen is discreet: "Loading" with animated dots over the
   play's background, whose transition into the play already runs there,
   so it idles on a ready background and only the dots move.
3. A song chosen in song select starts by itself after a countdown (3 s by
   default, set in Options only): Enter skips it, anything else cancels it.
4. The prompt offers a few play options for this play (and its retry)
   only, with a discreet way to keep them as the defaults.

Both shells, one screen: the UI core's prompt (`ddi_ui::screens::prompt`)
holds the countdown, the loading screen and the options' rows; each shell
says what still loads and starts the play once nothing does.

## What "ready" means

Ready is the play's own condition, now checked after Start instead of
before it. Nothing the play waits for changed:

- **Desktop** (`crates/desktop/src/play/host.rs`): the song read and its
  view made, the noteskin read and built (its Lua, SVG and models on its
  thread), the song's pictures (or 5 s, `PICTURES_WAIT`), the scripted
  backgrounds readied (their Lua in slices), the play's sound running (or
  3 s, `SOUND_WAIT`, after which a play that cannot run stops as
  interrupted). A work's layers and movies come as they come, as before.
- **Web** (`crates/web/src/components/game_canvas.rs`): the song read, the
  GPU, the noteskin readied, the music decoded (before Start on song
  select's context, else after it), and now the song's pictures and its
  scripts too, for at most 5 s after the song was read
  (`PICTURES_WAIT_MS`), so the loading screen's picture is the play's.
  Movies open as their changes come, as before (the opening still stands
  for the first).

A load that fails is what it was: the song's error replaces the loading
screen with the prompt's error; a skin that fails draws the built-in
arrows; pictures and scripts that never come are given up after their
wait. The loading screen therefore always ends, in the play or an error.

## The loading screen

`PromptStep::Loading` now means only this screen (it no longer stands
for "Start not offered"). The card fades out (`Motion.Screen`), the
picture finishes the entrance the play would have run at its first frame
(the framed copy to its place and the 300 ms unblur, `PLAY_SECONDS`, a
sharp layer added under the blurred ones as the play's renderer draws
them), a song without a picture keeps its one ground (the song list's
base fading out, as the play draws none), and "Loading" with three dots
shows in the lower right corner (`Prompt.Loading`, theme's `Muted`, 15 px)
once the unblur is done, so a short wait looks like the transition alone.
The dots step every 0.4 s. Under reduced motion everything settles at once
and the dots step once a second: still live, no motion beyond a glyph
appearing.

The play continues from where the screen left the backdrop
(`ddi_actor::background::Entrance::resume`): position and clarity both at
1 after a wait, so its first frame draws the same picture and nothing
moves. Escape (or a pad's Back) still leaves.

## The countdown

`Settings::start_countdown` (seconds, 0 off), Options → Display → Menus →
Start countdown: Off, 3 s (default, also for settings stored before it),
5 s, 10 s. There is no switch on song select or the prompt.

It runs from when the prompt shows Start, on the prompt of a play chosen
in song select, and is shown on the status line ("Starting in 3", and
under it "Enter starts now · any other key waits"; on touch screens "Tap
Start to start now, anywhere else to wait") with a thin bar under Start
that empties (none under reduced motion). At zero it presses Start for the player
(the loading screen follows if loading is not done).

- **Skips it** (starts now): Enter, Space or Numpad Enter; a pad's
  confirm or Start (the menu's Start); a tap or click on Start.
- **Cancels it** (Start then waits as before): any other fresh key press
  (but a modifier alone: switching windows is not a choice), a menu
  direction or Select from a key or a pad, a tap, click, long press or
  drag start anywhere but Start, a change to a play option, Save.
- **Leaves**: Escape, Backspace or a pad's Back, as before.
- **Not counted**: key releases (the screen never sees them), the OS's
  key repeats and our own direction repeats (`repeat`), mouse movement
  (hover), the wheel (a trackpad's inertia from song select's list can
  outlast the press that chose the song), pad axes inside the menu's
  dead zone (they never become a menu action). The press that chose the
  song was song select's: its release and repeats are among the above.

**Entry points.** Song select's play call asks for it (desktop's
`Request::countdown`; on the web, a play that took song select's audio
context, `web::audio::take_for_play`). Not for a direct `#/play?…` URL or
`--play` (the web makes the context at Start there: an automatic Start is
no user gesture), replays, the editor's playtests, calibration, the
prompt after the results' Retry or an interruption (the player just chose
to go again), nor where the testing flags start the play (`--start`
presses Start once the prompt is up, as before).

**Web gesture rule.** The countdown's Start is not a gesture. It only
runs where song select handed over a running context (`hand_over`, made
or resumed in the press that chose the song): the music decodes on it
before Start and the countdown's Start only starts the sound
(`BeginDecoded`, the context already running). If that context is not
running when the countdown ends (Safari suspended it), the prompt asks
for a tap or key as for a pad's Start (`NeedsGesture`).

## Play options on the prompt

Rows under the chart, as StepMania's player options come before each song
(`ScreenPlayerOptions`): Speed, Reverse, Turn, Appearance, Lane cover and
Arrows (the noteskin that draws the chart: the pump skin row for a pump
layout when pump skins exist; shown once every skin is listed, so a skin
from a folder not read yet never reads as Dito). These are the options a
player changes between songs; rules, timing and the display stay in
Options (a ruleset changes how the play is scored and its bests, a choice
for a session, not a song). They sit under a faint "THIS PLAY" heading, in
two columns where the card is wide (read down the first, then the
second), else one. Left and Right (or ◀ ▶) change the focused row, held
ones repeating, clamped as Options' rows are; a tap on the row cycles it.
Start keeps the focus first; Up from the buttons reaches "Save as default"
(when shown) and the last row, Up and Down go through the rows in reading
order, and Enter still starts from a row. Only on a song's prompt: a
replay is drawn with its own settings, a test with the game's.

They come from Options' own rows and logic (`OptionsModel` without
saving), so names, choices and ranges match. A change applies to this play
and the results' Retry, never to the stored settings; the summary line,
the picture's filter band and the noteskin (read again, its Lua with the
new direction) follow. When a row differs from the stored settings a
discreet "Save as default" button sits on the heading's line; it writes
those rows (and only those) over the settings stored now and then says
"Saved as default" there until the next change.

The play runs with the stored settings and these over them
(`PlayOverrides`), and records them so: the settings snapshot in the play
history has them (its replay is drawn with the skin and direction it was
played with), the engine's options (speed, turn, appearance, cover) are
those played, bests and the results' mod line are the play's.

## Tests and debug

- The UI core's harness: the countdown at zero, skipped, cancelled by each
  input class, not by the uncounted ones; the loading screen and its
  entrance; the rows applied and not saved, saved when asked, merged into
  a record's settings.
- Desktop offscreen: prompt → loading → play with a slow skin
  (`DDI_SKIN_DELAY_MS`, a test-only delay read by the skin's thread).
- Web: `window.__DDI_SKIN_DELAY_MS` delays the noteskin's readying the
  same way (read once a load starts). `__DDI_DEBUG.ui.theme.prompt`:
  `countdown` (seconds left, or null), `countdownState` (`running`,
  `cancelled`, `done`, `off`), `loading` (the loading screen shows),
  `label` (it says "Loading"), `options` (`id=value`), `optionsDiffer`,
  `saved`, `waiting`; `__DDI_DEBUG.prompt` `waiting` (what still loads; a
  change wakes a frame so it is current), `clarity`. The rows' places are
  `ui.rects["Opt.<id>"]`, `.Down`, `.Up` and `SaveOptions`.
- Headless Chrome (WebGPU and `gfx=gl`) and Playwright's Firefox, scripts
  kept in `target/scratch/prestart/` (`step1.mjs`: Start during a 3 s skin
  delay, the loading screen's picture pixel-equal to the play's first
  frames around the field; `step2.mjs`: the countdown left alone, skipped,
  cancelled by a key and by a click, none on a direct address, and with
  `gesture` under `--autoplay-policy=document-user-activation-required`;
  `step3.mjs`: options changed by keys and the mouse, played, kept in the
  record, not stored, saved; `step3b.mjs`: the noteskin changed on the
  prompt read again and played).

## Steps

1. Background loading, Start never blocked, the loading screen.
2. The countdown and its setting.
3. Per-play options.

## Controls preview (2026-10-27)

The card shows every receptor of the selected chart, with its primary
keyboard binding underneath and the per-play modifiers directly below.
There are no tabs, binding captions or start/quit/loading instructions on
this ready-to-start card. The separate loading screen after an early Start
and errors keep their status messages; an active countdown keeps its number.
The controls use the resolved saved keyboard table (including explicitly
unbound lanes), not the currently selected menu style. Save as default
remains beside the modifiers.

A brief demonstration presses each key and its receptor together, at 140 ms
per key, then loops after a one-second pause, without a replay button. It uses
the actual noteskin through the shared field preview renderer. The diagram
wraps large layouts into balanced rows; it never feeds events to gameplay.
Reduced motion keeps a still diagram. Start stays immediate and the existing
countdown/gesture contract remains; the automatic demo never delays or resets
it. The preview uses an independent skin instance so its clock and demonstration
presses cannot leak into the song.

Readied ahead (2026-10-27): the diagram's skin used to load when the prompt
opened, its receptors appearing a moment later (hidden until then). Now song
select's place report (the song, chart and style shown) has each shell
ready the diagram's skin for that chart (`Controls::for_chart`,
`ddi_shell::receptor_skin`; desktop's `Field::warm`, the web's
`FieldPreviewHost::warm` on the app's one preview, which outlives the
pages), so the prompt's first frame has it; a chart whose skin was not
ready yet (Start pressed at once) shows it as before, when it comes.

Coverage: the UI harness checks chart-specific defaults, saved primary and
unbound keys, the combined diagram/modifier layout, synchronized key travel, the looping clock, reduced motion
and auto-start taking priority while a skin loads. Native GPU tests check
transparent receptor images, per-lane presses, skin readiness and cleanup;
the existing prompt countdown/loading/options tests cover the play handoff.
Browser checks cover DanOni-7, saved mappings on WebGL2, wrapped Pump Double,
Retina sizing and phone layouts.
