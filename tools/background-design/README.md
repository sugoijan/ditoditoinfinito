# Generative backgrounds study

A browser playground for choosing which generative backgrounds the game
supports (`docs/plans/phase-8-gaps.md`, step 4). They stand in for a
missing song background, and can optionally be drawn over a song's
picture. Serve the repository root:

```sh
python3 -m http.server 8768 --bind 127.0.0.1
```

Then open <http://127.0.0.1:8768/tools/background-design/>. It needs no
build or packages. A bundled song loads first (`assets/songs/`); **Open
files…** takes any song's music, its `.sm`/`.ssc` and its pictures. Opened
directly from the file system, only **Open files…** works (browsers fetch
no local files).

## What to try

- **Candidates**, one at a time or all four at once (**View**):
  - **Spectrum bars:** bars of the music's bands across the whole
    screen, behind the field's band too: along the bottom, mirrored in
    the middle or in a ring. Segmented (a stack of cells, faint unlit
    ones optional) or solid, with square ends, and peaks held then
    falling. They draw over a base: the plain ground, the field, the
    kaleidoscope or the rain.
  - **Kaleidoscope:** still mirrors over a small piece of a picture.
    Three mirrors make triangles tiling the screen, the classic
    kaleidoscope; two make a rosette, mirrored outward in rings. The
    mirrors never move: the piece under them turns a step each beat or
    measure, or steadily, grows with a signal of the music and wanders
    over the picture. The picture is the song's background, its banner,
    or generated pieces of glass in the palette when there is none: in
    its default role a song has no background.
  - **Field and waving lines:** a quiet domain-warped field in the
    palette, with lines across it. Each line waves with its share of the
    bands, or with the waveform itself.
  - **Outline rain:** the start menu's rain, particle for particle
    (`crates/theme/src/rain.rs`, as `site/boot-art.js` ports it). A
    share of its outlines move, each chosen afresh as it comes in at the
    top. Some zoom with the bass; the rest wave round their outline
    with the treble.
- **Drivers:** every motion follows the music alone, never the play.
  The signals are loudness, the bass, middle and treble bands, onsets,
  brightness, stereo width, sections, and the beat and measure from the
  simfile. The rain's motions take what stands out of the song: a
  signal above its median over the song, its 95th percentile full
  (the bass sits between 0.5 and 0.8 most of the time).
- **Palettes:** mostly dark and of few hues. Vivid is one choice, not
  the default: the screens already lean on it.
- **Context:**
  - **Over the song's picture** puts a candidate over the background, at
    **Picture brightness** (the game's Background brightness, 40% by
    default).
  - **Show the field** draws the field's band and receptors with its
    filter, to judge what stays readable behind the notes.
  - **Reduced motion** holds the frame still, as the game would.
- **What the music gives:** each signal over the last 8 seconds and the
  next 2, with the waveform at the playhead. Beats and measures come
  from the simfile.

## Choosing

**Choosing** records:
- which candidates to keep, and which work over a picture, with a note
  each;
- how a song gets one: the player's choice, one the game picks for each
  song, or always the same.

**Export choices** downloads `approved.json`, those choices with every
candidate's parameters and the palette. The accepted file goes beside
this README, and step 5 ports what it keeps: the one here (2026-11-05)
has the player choose among all four, and the game's looks follow its
values (`ddi_library::generative`). **Import choices** takes a
file back; only known, bounded values are applied. Settings save in
`localStorage` under `ddi.background-study.v1`.

## Notes

- **The analysis** (`analysis.js`) is a design approximation of the
  game's, made once at load from Web Audio's decode:
  - a 2048-point FFT every 20 ms;
  - 32 log-spaced bands from 30 Hz to 16 kHz;
  - spectral flux for onsets, the centroid for brightness, mid and side
    levels for width, and an 8-second loudness for sections.

  The game's will come from the editor's analysis
  (`ddi_ui::state::editor::audio`) on the samples it plays.
- **The timing** (`timing.js`) reads a simfile's song-level offset,
  BPMs, stops and time signatures. Warps, delays and a chart's own
  timing are left out.
- **Rendering:** WebGL2 draws the pictures and fields, a 2D canvas the
  bars, lines, outlines and field. A frame is an approximation, not a
  GPU reference.
- **Checks:** `window.__BG_STUDY` exposes the state, the song,
  `play`, `stop`, `seek` and `now`, for headless checks.
