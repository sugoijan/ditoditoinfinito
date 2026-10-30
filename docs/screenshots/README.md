# README gameplay media

Regenerate the animation and the still from the repository root:

```sh
just screenshot
```

This requires a GPU, **FFmpeg with SVT-AV1 on `PATH`** and the native
desktop build prerequisites, but not the video decoder or the web app.
The command regenerates the bundled song and noteskin indexes first and
runs with a fresh `DDI_HOME` under `target/scratch/screenshot/home`.

## What is recorded

The scenario is [`readme.json`](readme.json). The desktop shell runs
offscreen on a fixed clock at 1280 × 720 and 60 frames per second, from
its start menu through song select and Beatucada's Challenge single
chart, autoplayed. The animation [`readme.avif`](readme.avif) has these
parts:

1. The menus, pressed through briskly. Enter focuses Singleplayer, Enter
   again opens song select, which shows for 1.8 s. Enter opens the start
   prompt, and 0.9 s later Enter starts the play (no countdown). The part
   goes on into the play's first two beats.
2. Beats 14 to 28: a second before the first note, then the first
   measures with their mine rows.
3. Beats 88 to 100: holds and mines, and the combo reaching 100 (at
   about beat 96).
4. The results as they come in, for four seconds, the last 1.2 s fading
   to black.

The play's parts are placed in beats of the chart. Each crossfades into
the next over one beat, starting on a downbeat of both: beat 2 into
beat 14, beat 28 into beat 88. Both sides of a crossfade are at the same
point in their bars, so the arrows' and receptors' beat animations
continue across the cut. The generator prints where each side is to
check this. The last play part crossfades into the results the same way.

The still [`readme.png`](readme.png) is the full-size frame 0.05 s after
the note that reaches a combo of 100: the combo's hundred outline and its
pulse, and the receptor's flash on the hit. The generator refuses to
write a still whose combo is not exactly 100, or a scenario whose parts
never show it.

The desktop app has a Quit choice on its start menu. The recording hides
it (`ddi_desktop::shell::hide_quit`), so the menu looks like the web
app's.

## How it is made

[`crates/desktop/examples/screenshot.rs`](../../crates/desktop/examples/screenshot.rs)
builds the desktop shell on the bundled library, with in-memory settings
and the list's place set to the scenario's chart. It drives the shell
frame by frame (`Shell::offscreen`, `offscreen_poll`, `offscreen_frame`),
pressing keys as a player would. The frames are the real screens and play
drawn by the real renderers. The sound runs headless, heard by nothing.

The clock is integer nanoseconds derived from the frame number. When
something loads (the song, the prompt's skin, the preview's sound, an
image), the clock is held and the frame drawn again until it is in. So
nothing ever appears half-loaded, and the same scenario gives the same
frames.

Between two play parts, the frames are drawn but not recorded. The next
part starts as many frames later as its beat is after the last part's
crossfade. Frames never land exactly on beats, so one of those unrecorded
frames is stretched by the difference (a few milliseconds). This puts the
next part on its beat exactly.

Frames stream to FFmpeg as raw RGBA. Every other frame is kept (30 a
second) and scaled to 960 × 540 with Lanczos. They are encoded as AV1 in
an animated AVIF that loops (`libsvtav1 -crf 30 -preset 6`, yuv420p,
BT.709). The result is about 2.4 MB for 23 seconds. The scenario's
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

The animation also contains Beatucada's banner by Jose Varela, CC BY 4.0,
and song select's banners of the other bundled songs:

- *Into My Dream*, by Kevin Turner and Step Revolution, CC BY 4.0.
- *Some Things Must*, by Jack5, CC BY 3.0.
- *Sweeteners*, by Lirodon, CC BY 4.0.

Song select also names the songs' artists and the charts' step artists.
Neither output contains sound: *Beatucada* by Kurio Prokos is named, not
heard.

The still combines MIT and CC BY 4.0 material, and the animation also CC
BY 3.0, as recorded in [`REUSE.toml`](../../REUSE.toml). The bundled
files' upstream commits, sources and licenses are in each song's
`PROVENANCE.toml` (for example
[Beatucada's](../../assets/songs/beatucada/PROVENANCE.toml)), with full
attribution in [`CREDITS.md`](../../credits/CREDITS.md). Generation is
exactly `just screenshot` with the checked-in scenario. The original
bundled files stay byte-identical.
