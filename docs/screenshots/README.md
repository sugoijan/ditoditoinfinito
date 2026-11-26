# README gameplay media

Regenerate the animation from the repository root:

```sh
just screenshot
```

This requires a GPU, **FFmpeg with SVT-AV1 on `PATH`** and the native
desktop build prerequisites, but not the video decoder or the web app.
The command regenerates the bundled song and noteskin indexes first and
runs with a fresh `DDI_HOME` under `target/scratch/screenshot/home`. It
writes [`readme.avif`](readme.avif) from [`readme.json`](readme.json) and
leaves the still alone: that scenario has none.

The still [`readme.png`](readme.png) is made by its own scenario,
[`still.json`](still.json), and is not regenerated with the animation
(it is also the share card's image). To make it again:

```sh
just screenshot docs/screenshots/still.json target/scratch/still.avif docs/screenshots/readme.png
```

## What is recorded

The desktop shell runs offscreen on a fixed clock at 1280 × 720 and 60
frames per second, with no window and no sound device, pressing keys as
a player would. A scenario is a list of shots: each recorded from where
it starts (at once, a screen's first frame, or a beat of the play) to
where it ends (a beat, a screen, seconds recorded, or seconds after its
last key). The frames between shots are drawn but not recorded, and each
shot crossfades into the next.

The animation, [`readme.json`](readme.json), about 36 seconds:

1. **The menus and a solo play.** The start menu, Singleplayer, song
   select on Beatucada's Challenge single chart, its start prompt, into
   the play's first two beats; then beats 14 to 26, the first measures,
   with the dancing hand beside the field and the song's own background.
2. **Into multiplayer.** The play is quit, unrecorded, and the shot
   starts back on the start menu: Multiplayer, the multiplayer home, CPU
   practice, the practice room's song select with its players panel,
   Beatucada picked for stage 1, and the stage's Ready card with
   everyone's names, Ready pressed.
3. **The stage, beats 82 to 96.** An attack from an opponent comes in
   (its line to the warning by the receptors, its shocks coming down the
   player's lanes), and the player's own attack bar fills and launches at
   another opponent (the line from the field to their miniature, the
   shocks arriving in its field).
4. **The stage, beats 182 to 194.** The weakest opponent's life runs out
   at beat 191.5: its field empties and says Out.
5. **The stage's Standings**, as its results open on them, for four
   seconds, the last 1.2 s fading to black.

The stage's two stretches cut in step: each crossfades into the next over
one beat starting on a downbeat of both, so the arrows' and receptors'
beat animations continue across the cut (the generator prints where each
side is). A frame never lands exactly on a beat, so one of the unrecorded
frames between them is stretched by the difference.

### The player's presses

The player's seat plays a real person's presses: [`readme-seat.json`](readme-seat.json)
is the input log of one of the maintainer's own kept plays of this chart
(a full clear played under the ITG rules at 1×, no options changing the
notes; judged under DDI, an A at 86.9 %). It holds only what a play needs to take
the presses again: its format, the song and the chart's content digest
and layout, and each press and release as `[second, lane, pressed]`.
Nothing else of the play was kept: no device, profile, offset, name, id
or date.

Every play of that chart in the run takes its presses from the log
(`Host::seat`, `SessionConfig::seat`, test-and-tool only): fed at their
judged seconds, as a replay's are, and judged by the play as its own,
under the play's own rules. Nothing presses for it: it is neither an
autoplay nor a replay, and the screens say neither. In the stage the
presses meet what the person never saw: the stage's ruleset (DDI, the
match's), the opponents' attacks and their shocks in the player's lanes,
which the presses may step on; in this stage two attacks put eight
shocks in, and none is stepped on.

### The practice room

CPU practice runs with no server and no network. Its CPUs are drawn from
the scenario's seed (`practice.seed`), and the room is made and ticked
on the run's clock, so the same scenario plays the same stage. Its roster
is the one a player's last practice room was left with (the settings'
`practice_roster`): Saltglass Rook at level 8, Copper Fennec at 6 and
Pocket Thunder at 2. Two of the match's rules differ from a new room's:
Charts **As picked** and When life runs out **Out** (`practice.rules`).
Under the defaults (Adaptive, playing on at the easiest chart) an
opponent steps down to an easier chart rather than going out, so no
opponent ever leaves the field. The other rules are the defaults (DDI,
Normal timing, Attacks on, four shocks an attack, four stages).

### The settings

As the scenario gives them: speed 2.5, Vivid notes, the background at
0.55 and the field filter at 0.6, no announcer (the animation is silent).
From the second shot on, Options → Backgrounds' generated background is
**Bars, over song backgrounds**: the multiplayer part shows the music's
bars over Beatucada's picture, the solo part the picture alone. Bars was
chosen from the four looks by stills of each in the stage at beat 91: it
reads at the animation's size, stays at the screen's bottom edge, and
neither the notes nor the miniatures compete with it (Rain's outlines are
arrows, faint at this size; Field's lines nearly vanish; Kaleidoscope
washes the picture grey there: its piece then folds the picture's plain
sky, which its desaturation and darkening make a flat blue grey, the
look as designed, round HY).

### The still

[`still.json`](still.json) is the animation as it was before the
multiplayer part: the menus, Beatucada's Challenge chart autoplayed,
beats 14 to 28 and 88 to 100, and the results. Its still is the
full-size frame 0.05 s after the note that reaches a combo of 100: the
combo's hundred outline and its pulse, and the receptor's flash on the
hit. The generator refuses to write a still whose combo is not exactly
100, or a scenario whose shots never show it. The committed still was
made on 2026-11-06; the game's display has changed since (the score
shown, the judgment's word), so regenerating it gives another picture.

The desktop app has a Quit choice on its start menu. The recording hides
it (`ddi_desktop::shell::hide_quit`), so the menu looks like the web
app's.

## How it is made

[`crates/desktop/examples/screenshot.rs`](../../crates/desktop/examples/screenshot.rs)
builds the desktop shell on the bundled library, with in-memory settings
and the list's place set to the scenario's chart. It drives the shell
frame by frame (`Shell::offscreen`, `offscreen_poll`, `offscreen_frame`),
pressing keys (`offscreen_key`: a running play's own, else the screens').
The frames are the real screens and play drawn by the real renderers. The
sound runs headless, heard by nothing.

The clock is integer nanoseconds derived from the frame number. When
something loads (the song, the prompt's skin, the preview's sound, an
image), the clock is held and the frame drawn again until it is in. So
nothing ever appears half-loaded, and the same scenario gives the same
frames: the generator prints a hash of every frame it encodes.

While a stage plays, the generator prints what its battle does, by beat:
attacks sent and received with when their shocks come, shocks put into
the player's play and stepped on, misses, each opponent's life every
eight bars and when one goes out. The scenario's stage shots were chosen
from it.

Frames stream to FFmpeg as raw RGBA. Every other frame is kept (30 a
second) and scaled to 960 × 540 with Lanczos. They are encoded as AV1 in
an animated AVIF that loops (`libsvtav1 -crf 30 -preset 6`, yuv420p,
BT.709). The result is about 5.9 MB for 36 seconds. The scenario's
`animation` sets the size, the rate, the CRF and the preset.

Choose another scenario or other outputs with:

```sh
just screenshot docs/screenshots/readme.json target/scratch/readme.avif target/scratch/readme.png
```

The same scenario on the same renderer and GPU gives the same pixels.
Different GPU drivers can differ slightly in rasterization, and encoding
can differ across FFmpeg and SVT-AV1 versions (the committed animation
was made with FFmpeg 9.0.2 and SVT-AV1 4.2.0).

## Why an animated AVIF

GitHub plays no video committed to a repository inline in a README; only
an upload through its editor gets a player. An image committed beside
the README updates with the commit that regenerates it, and animates
wherever the browser decodes it. AV1 compresses the gameplay without the
blocky artifacts of H.264.

Before choosing, the whole sequence was encoded from a lossless render:

| Format | Size |
|---|---|
| GIF, 960 × 540, 30 fps (256 colors, dithered) | 18 MB |
| APNG, 960 × 540, 30 fps (lossless) | 98 MB |
| AVIF, 960 × 540, 30 fps | 2.4 MB |
| AVIF, 1280 × 720, 60 fps | 5.1 MB |
| MP4 (H.264), 1280 × 720, 60 fps, CRF 16 | 6.2 MB |

(Those sizes are of the earlier, 23-second animation.)

The README shows it in a `<picture>` with the AVIF as its source and the
still as the fallback image, for a browser that cannot decode AVIF.

Trunk also copies the still unchanged to `images/readme.png` in the web
distribution. `site/seo/metadata.toml` advertises it in Open Graph and
Twitter cards, the game's structured data and the image sitemap. Keep the
declared dimensions in sync when changing the capture scenario. The
attribution below applies to that published copy too. The game's Credits
screen credits the bundled songs.

## Sources and attribution

Both the still and the animation contain:

- Background art by **Jose Varela**, CC BY 4.0, from
  [`assets/songs/beatucada/Background.png`](../../assets/songs/beatucada/Background.png),
  scaled and dimmed by the game's background renderer.
- The **Challenge chart by Timo Kitsune**, CC BY 4.0, from
  [`Beatucada.ssc`](../../assets/songs/beatucada/Beatucada.ssc).
- The game's display and its Infinito noteskin, this repository's own
  MIT work.
- The dancing hand ([`assets/hand/`](../../assets/hand/NOTICE.txt)): its
  mesh adapted from **Dan Ulrich**'s hand in Blender's Human Base Meshes,
  with **MZiemys**' skeleton as a reference for its bones, both CC0 1.0;
  its rig, corrective morphs and calibration are this repository's own MIT
  work, its motion planned on Beatucada's charts (CC BY 4.0, above).

The animation also contains Beatucada's banner by Jose Varela, CC BY 4.0
(song select, the room's song select and the stage's results), and song
select's banners of the other bundled songs:

- *Into My Dream*, by Kevin Turner and Step Revolution, CC BY 4.0.
- *Some Things Must*, by Jack5, CC BY 3.0.
- *Sweeteners*, by Lirodon, CC BY 4.0.

Song select also names the songs' artists and the charts' step artists.
The multiplayer part shows the game's own work alone besides: the
multiplayer screens, the CPUs' invented names
([`cpu_roster.json`](../../crates/ui/src/state/cpu_roster.json)), the
opponents' miniature fields in the Infinito noteskin, and the generated
background's bars, drawn from the music's analysis
(`ddi_library::generative`). Neither output contains sound: *Beatucada*
by Kurio Prokos is named, not heard.

The seat's log, [`readme-seat.json`](readme-seat.json), is the
maintainer's own presses (MIT, as the repository); it holds no part of
the chart.

The still combines MIT, CC BY 4.0 and CC0 material, and the animation also
CC BY 3.0, as recorded in [`REUSE.toml`](../../REUSE.toml). The bundled
files' upstream commits, sources and licenses are in each song's
`PROVENANCE.toml` (for example
[Beatucada's](../../assets/songs/beatucada/PROVENANCE.toml)), with full
attribution in [`CREDITS.md`](../../credits/CREDITS.md). Generation is
exactly `just screenshot` with the checked-in scenario. The original
bundled files stay byte-identical.
