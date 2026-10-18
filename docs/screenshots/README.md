# README gameplay media

Regenerate the inline, silent looping GIF from the repository root:

```sh
just animation
```

This requires **FFmpeg on `PATH`**, in addition to the native prerequisites
below. [`animation.json`](animation.json) captures four seconds starting at
song second 24.25, at 768 × 432 and 20 FPS (80 frames). The game and noteskin
still advance at 60 FPS; every third frame is captured. The committed GIF
is about 447 KB. It loops back to the start of the excerpt after four seconds.

Frames stream directly to FFmpeg as raw RGBA pixels. A single palette for
the whole clip, fixed ordered dithering and changed-frame rectangles keep
the background stable and the file small. The exact encoding arguments
are in `Capture::new` in the example below: `palettegen=stats_mode=diff`
and `paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle`, with
`-loop 0`. No audio is included. GIF is a
[GitHub-supported image format](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/attaching-files#image-and-media-files)
and is embedded with ordinary Markdown image syntax in the README.

## Full-size still

Regenerate `readme.png` from the repository root:

```sh
just screenshot
```

The scenario is [`readme.json`](readme.json): Beatucada's Hard dance-single
chart, autoplayed with Dito's Vivid arrows at 60 frames per second, captured
at song second 24.25 at 1280 × 720. The background brightness is 55% and
the field filter is 60%. Other settings use the game's defaults.

[`desktop/examples/screenshot.rs`](../../desktop/examples/screenshot.rs)
runs the actual desktop play session and renderer into an offscreen
texture. It advances a headless audio clock by fixed frame timestamps,
runs the noteskin's commands each frame, and reads the rendered pixels
directly into a PNG, or streams the captured frames to FFmpeg for a GIF.
It opens neither a window nor a sound device and uses no saved player
settings or history. There is no screenshot editing
or compositing step.

Choose another scenario or output file with:

```sh
just screenshot docs/screenshots/readme.json target/scratch/readme.png
just animation docs/screenshots/animation.json target/scratch/readme.gif
```

The command regenerates the bundled song and noteskin indexes first. It
requires the native desktop build prerequisites and a GPU, but does not
build the video decoder or web app. `DDI_HOME` is isolated under
`target/scratch/screenshot/home`. The same scenario on the same renderer
and GPU should produce the same pixels; different GPU drivers can differ
slightly in rasterization. GIF encoding can also differ across FFmpeg
versions (the committed GIF was generated with FFmpeg 9.0.2).

Custom animation scenarios add an `animation` object with `seconds` (over
zero, up to ten) and `fps`. The capture FPS must divide both the simulation
FPS and 100, so captures land on simulation frames and GIF centiseconds
exactly. Duration rounds up to a whole captured frame. Use a `.gif` output
for animation scenarios and `.png` for still scenarios.

## Sources and attribution

Both the still and GIF contain:

- Background art by **Jose Varela**, CC BY 4.0, from
  [`assets/songs/beatucada/Background.png`](../../assets/songs/beatucada/Background.png).
  It is scaled and dimmed by the game's normal background renderer.
- The **Hard chart by Chriszo**, CC BY 4.0, from
  [`Beatucada.ssc`](../../assets/songs/beatucada/Beatucada.ssc).
- Dito's art and the game's display, this repository's own MIT work.

The song is **Beatucada by Kurio Prokos**. Neither output contains audio.
The bundled files' upstream commit, source and licenses are recorded in
[`PROVENANCE.toml`](../../assets/songs/beatucada/PROVENANCE.toml), with full
attribution in [`CREDITS.md`](../../CREDITS.md).

The generated images combine MIT and CC BY 4.0 material, as recorded in
[`REUSE.toml`](../../REUSE.toml). Generation and modifications are exactly
the `just screenshot` and `just animation` commands above with their
checked-in scenarios. Original bundled files remain byte-identical.
