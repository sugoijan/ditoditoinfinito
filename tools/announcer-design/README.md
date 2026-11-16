# Announcer audition

## Regenerating the bundled packs

Both packs use `effect = "stadium-soft"`. Every English line uses the approved
lead **"Here comes the next challenge.…"**; Japanese retains its Japanese
leads. The lead is spoken to give Kokoro context for the short line, then
removed using the model's predicted durations and the quiet pause.

From the repository root, with uv, Cargo and FFmpeg (encoding/metering only):

```sh
uv run tools/announcer-design/kokoro.py pack --bundle
cargo run -p xtask -- gen-announcers
cargo run -p xtask -- gen-credits
```

`--xtask PATH` before `pack` can select a prebuilt xtask from this checkout.
The studio and pack generation share `ddi_library::announcer::stadium`:
`announcer-stadium-rust-v1`. No external effects engine is required.

The production path is unencoded TTS → shared `announcer-level-v3` tidy →
shared Rust stadium processing and `announcer-finish-v1` gain/limiter →
one MP3 encoding. `stadium.py` calls `xtask announcer stadium` on the dry
WAV. `stadium.json` supplies the versioned settings. Rust performs partitioned
FFT convolution with the captured responses in `assets/announcer-filters/`,
with normalized tanh saturation and a 7500 Hz tone filter between them.
The adapter fingerprints the recipe, response data and Rust sources so a
processing change invalidates cached audio. Provenance records the processor
version and response/renderer hashes. The game plays ordinary audio files.

The studio offers **Advanced → Stadium effect**, off by default and saved
with the draft. It processes original recordings, with the same settings in
playback, staged scenes and exports; processed copies are cached within a
memory cap. Tidy recordings remains independent: Stadium includes final
leveling even when trimming/tidying is off. Stereo channels stay separate.
The browser loads the response files on first use, outside its initial wasm
payload; desktop and the pack tool embed the same data.

The responses were captured from the selected SoundCraft 0.20.1 recipe in
`stadium-soundcraft.json`: separate EQ and delay/room responses, measured
with a half-scale impulse after one second of silence. Capture gain is removed
and the measured mono-bounce gain applied once after processing. The files
retain 0.2 s of EQ and 2 s of spatial response; omitted samples are below
−140 dBFS. These are generated filter data; no SoundCraft DSP source is used.
The fixed room approximates the original's subtle modulation/startup smoothing.

`stadium_ffmpeg.py` retains the approved FFmpeg implementation for comparison
only; generation uses Rust. `capture_stadium.py --port PORT` can recapture
the responses into `target/scratch/stadium-responses/` with a disposable
SoundCraft 0.20.1 control session. This optional development tool and the
historical auditions below use SoundCraft; the studio and pack generation
never call them. The capture reproduced both responses byte for byte.

The selected treatment combines a 180–6500 Hz microphone/speaker tone,
gentle saturation, 130 ms repeats and a 0.9 s room. Other calls retain a
bounded 0.6 s tail. Ready, restart and every "here we go" keep the dry clip's
duration, with only the spatial return faded over the last 60 ms, because
their full duration determines their place in the opening countdown.
Finishing measures the original speech span, restores −12 dBFS voiced RMS,
and limits peaks without another phase rotation, compressor or silence trim.
Every encoded file must have true peaks at or below −1 dBFS.

The run's `state.json` records each text, lead, cut, dry audio hash, complete
effect recipe, duration, finishing measurements and encoded loudness/peak.
`PROVENANCE.toml` and the generated indexes retain the existing CC0 licence
and Japanese training-source attribution. Audition recipes below remain
available for further comparisons.

## Effects audition (SoundCraft)

`effects.py` compares dry takes through SoundCraft's built-in
effects. This is a listening study; it does not change the packs, the TTS
generation, or the game's shared leveler. Python 3.11+ and FFmpeg are needed.
`--source DIR` selects the folder containing both dry packs and their
provenance. Set `DRY_PACKS` below to that folder: use a pre-effect export,
or generate one from temporary copies of the pack TOMLs with `effect`
removed. The current bundled files already contain effects and are refused
as sources, to avoid applying the treatment twice.

Start a **separate, disposable** SoundCraft instance: the script replaces its
session for every candidate. Its control port must not belong to a working
session you want to keep. On macOS, from the repository root:

```sh
mkdir -p target/scratch/announcer-effects/soundcraft-profile
SOUNDCRAFT_CONFIG_DIR="$PWD/target/scratch/announcer-effects/soundcraft-profile" \
  /Applications/SoundCraft.app/Contents/MacOS/SoundCraft --no-audio --control 0
# In another terminal, use the port SoundCraft prints:
python3 tools/announcer-design/effects.py --port PORT --engine-version 0.20.1 --source "$DRY_PACKS"
open target/scratch/announcer-effects/index.html
```

No installed MCP integration is required: the script uses the same engine
commands through SoundCraft's documented local JSON-lines control channel.
`--no-audio` prevents playback; rendering still works. No recording, plugin
discovery, preference changes to the personal profile, or outside services are
needed. Close the isolated instance after rendering.

`effects.json` holds three treatments alongside the current voice:

- **Race broadcast:** presence EQ, light saturation, a quiet 75 ms slap,
  and 0.35 s room reverb.
- **Compact arena:** body and presence EQ, light saturation, 40 ms pre-delay
  and 0.75 s dark room reverb.
- **Stadium PA:** 260–5200 Hz speaker color, stronger saturation,
  145 ms echoes and 1.25 s room reverb. Deliberately more pronounced.

Each reads the same six lines of each language with two-second gaps. The
voices remain mono, as shipped. The music comparisons duplicate the voice
into both channels and retain the music's stereo image. They use one fixed
mix, with no ducking; this is a listening example, not an in-game mix test.

After rendering, FFmpeg measures EBU R128 integrated loudness and true peak.
One static gain per reel matches all candidates at a common target (at most
−16 LUFS, reduced if needed to keep every voice below −2 dBTP). Nothing adds
another compressor or limiter. This gain is for auditioning only, not a new
production leveler. The script verifies the resulting loudness, headroom,
duration and mixed peaks. The first study, SoundCraft 0.20.1, used −18.6 LUFS.

The output includes the listening page, WAVs alone and over music, `study.json`
(recipes, complete plugin parameter snapshots, source hashes, tool versions,
levels and cue times), `commands.json`, and source credits. All remain under
`target/scratch/`. `--dest` can choose another folder there. The music excerpt
is *Into My Dream* by Lagoona (Andreas Viklund), CC BY 4.0; the page and
`CREDITS.txt` credit its source and describe the edit. Both voices' provenance
is retained, including the Japanese voice's training-source attribution.

The first listening feedback (2026-11-16): race and arena were too subtle;
stadium's microphone color was welcome, but the overall effect might be too
strong. `effects-stadium.json` keeps the first dry/stadium references and
adds three refinements: A keeps the microphone with much less space, B keeps
it with moderately less space, and C uses B's space with gentler microphone
color. Render this second set separately to preserve the first comparison:

```sh
python3 tools/announcer-design/effects.py --port PORT --engine-version 0.20.1 \
  --source "$DRY_PACKS" \
  --recipes tools/announcer-design/effects-stadium.json \
  --dest target/scratch/announcer-effects-stadium
```

The maintainer selected `stadium-soft` (C) on 2026-11-16, recorded in
`effects-stadium.json`. Before production integration, two source endings
were reviewed: "Are you ready?" (heard as "readys") and
"A hundred in a row!" (heard as "rowth"). The audition used the existing
bundled MP3s, not new speech. Its dry render differs from the decoded source
only by a constant gain and floating-point rounding (null residual about
−149 dB relative to the voice); final WAV conversion adds 16-bit rounding.

Regenerating both with their existing lead, "Oh yeah, here we go!…", reproduces
the bundled MP3s byte for byte. `pack-endings.toml` retains that control and
tries "All right, everybody.…" and "Here comes the next challenge.…".
Every take has a lead, cut through the normal generator, and the normal shared
tidy. Reproduce this small study without bundling it:

```sh
uv run tools/announcer-design/kokoro.py --out target/scratch/announcer-endings-pack \
  pack tools/announcer-design/pack-endings.toml
```

The initial six raw/tidied takes, encoded files and two-line listening reels
are under `target/scratch/announcer-endings/`, with the generation script and
`study.json`. The reels are matched to −16 LUFS, with no creative effects.
The maintainer preferred the second alternative, "Here comes the next
challenge.…", and requested it for all English lines. Both bundled packs
now include the selected effect through the production path above.

References: SoundCraft's [control protocol](https://github.com/storytold/soundcraft/blob/main/docs/control-protocol.md)
and [MCP documentation](https://github.com/storytold/soundcraft/blob/main/docs/mcp.md),
FFmpeg's [EBU R128 meter](https://ffmpeg.org/ffmpeg-filters.html#ebur128),
and Universal Audio's [reverb explanation](https://www.uaudio.com/blogs/ua/the-basics-of-reverb)
(pre-delay separates the voice's attack from the reverb).

## Voice generation

Tries voices for an announcer pack of our own made with ElevenLabs
(`announcer.py`) or here with Kokoro (`kokoro.py`, below;
`docs/plans/announcers.md`, "Packs of our own"), and makes our packs
with Kokoro (`pack.toml` in English, `pack-ja.toml` in Japanese; "Our
pack" below), which the game ships.
`announcer.py` takes Python 3.11 or later, standard library only.

- **ElevenLabs' audition runs on the free plan.** Its audio is non-commercial and
  must credit ElevenLabs, so nothing made here is committed, bundled or
  shared: it all stays in `target/scratch/announcer-audition/`.
- **A pack to bundle is made again during a paid month** (Starter is
  enough), from the voices and styles picked here, and released as CC0.
- **Voices** come from Voice Design (or ElevenLabs' own default voices),
  never from the Voice Library's clones of real people, and no
  description names or imitates a real person, another game or its
  announcer.
- **The key** is read from `ELEVENLABS_API_KEY` and never printed or
  written.

## Steps

```sh
export ELEVENLABS_API_KEY=...          # fish: set -x ELEVENLABS_API_KEY ...
A=tools/announcer-design/announcer.py
python3 $A status                      # characters and voice slots left
python3 $A design --dry-run            # what a round costs
python3 $A design                      # three previews of each voice
open target/scratch/announcer-audition/index.html
python3 $A design stadium              # another round of three, one voice
python3 $A keep stadium r2-1           # save a preview as a voice (a slot)
python3 $A speak --dry-run             # the lines in the kept voices: the cost
python3 $A speak --styles v3           # ...and made, in one style
```

1. **Designs.** Each round gives three previews of a voice from its
   description, all reading `[design] text` (about 200 characters a round
   as configured). Change a description in `audition.toml` and run
   `design` for that voice again; earlier rounds stay on the page.
2. **Keep** the previews worth hearing more of. Keeping one saves it as a
   voice in the account, which takes a voice slot (`status` says how many
   are left) and works only while the preview's id lasts: run another
   round when it has gone.
3. **Speak** the lines in the kept voices, in each style of
   `audition.toml` (a model and its settings, a few takes each with their
   own seeds). Clips already made are kept; edit a line's text and only
   that line is made again. A style the service refuses (a model the
   plan lacks, say) is reported and skipped.
4. **Compare** on the page: the previews, then the lines by voice and
   style. ▶ on a column plays a voice down the lines, on a row a line
   across the voices; star what you like and copy the picks.

As configured, a kept voice's lines in the three styles are 95 clips,
about 2,200 characters; the free plan has about 10,000 a month.
`--voices`, `--styles` and `--lines` narrow a run.

## Kokoro, made here

`kokoro.py` makes the same lines with Kokoro-82M, an open-weights model
(Apache-2.0) small enough to run on the CPU: no account, no key, nothing
to pay, and what it makes is ours to release as CC0, so a pack from it
could be bundled as the placeholder until there is a better one.
[uv](https://docs.astral.sh/uv/) makes its environment from the script's
header; the first run fetches the model (about 310 MB, at the pinned
revision) into the Hugging Face cache.

```sh
K="uv run tools/announcer-design/kokoro.py"
$K sample                     # Kokoro's voices and ours read [design] text ([kokoro.texts] j in Japanese)
$K sample af_heart heart-bella
$K speak                      # the lines in each [[kokoro.voice]], both styles
$K speak --voices heart --styles lead --lines combo
$K context                    # lines said around [[kokoro.context]]'s leads and closers (below)
$K pack                       # our announcers, pack.toml's and pack-ja.toml's (below)
$K pack tools/announcer-design/pack-ja.toml
$K pack --bundle              # ...and put in the game, assets/announcers/ (below)
$K loudness --dest target/scratch/announcer-loudness   # a few lines at several levels, measured
```

- **Voices** (`[[kokoro.voice]]`) are one of Kokoro's (`af_heart`: US
  female; `am_`, `bf_`, `bm_`, `jf_` and `jm_` the others) or a blend of
  their styles by weight, which makes a voice of our own. A voice speaks
  its own language (its first letter: `a`, `b` or `j`), or the one `lang`
  gives (`george-ja`: `bm_george` speaking Japanese). Most of Kokoro's share their
  names with other services' stock voices, and its model card says some
  of its training audio was closed services' synthetic speech, so they may
  imitate those: a pack to bundle would rather take a blend. Our pack
  takes `bm_george` as it is, the maintainer's choice (2026-11-04).
- **Lines** lose their audio tags: Kokoro takes no direction, and its
  delivery follows the punctuation. It is weakest on utterances as short
  as these, so the `lead` style says `[kokoro] lead` first and cuts it
  off in the pause before the line (found by the model's own durations,
  cut at its quietest); `plain` says the line alone. Compare them on the
  page.
- **Words** its dictionary lacks stop the run until `[kokoro.words]` gives
  them in misaki's phonemes (the game's name is there).
- **The sound** is tidied by the game's own code, the announcer
  studio's (`crates/library/src/announcer/tidy.rs`, run a batch at a time
  through `cargo run -q -p xtask -- announcer tidy`): the silence at its
  ends cut, short fades, and its level evened by a phase rotator and a
  compressor, then to a voiced RMS of −12 dBFS under a limiter that
  keeps true peaks under −1 dBFS (`announcer-level-v3`; about
  −13.4 LUFS, 3 LU under a song's music, the loudest at which the
  limiter has little to do: `docs/plans/announcers.md`, "Smoother
  leveling").
  This script only cuts a lead or a closer off before that and encodes
  the result after, at 44.1 kHz in 128 kb/s MP3. So the tool takes
  cargo (the repository's Rust toolchain) beside uv and ffmpeg. The same
  configuration makes the same bytes on the same machine, so a pack can
  be made again exactly: what each was made from, and what the tidy did
  to it, is in `state.json`, beside the versions of misaki, spaCy and
  onnxruntime. A clip is made again only when that changed (or the
  script's `PROCESS`: raise it when the tidy changes what it makes).
- **How loud** is heard with `kokoro.py loudness --dest DIR`: six lines
  of each pack at several levels (`--levels`, voiced RMS in dBFS; −12,
  −9 and −7 by default), as `DIR/<level>/<pack>/<line>/`, with a table
  (`DIR/table.txt`) of each one's LUFS, true peak and what the
  compressor and the limiter took (the limiter's largest reduction, and
  how much of it was the shaping of a peak). Nothing of it is kept in
  the state or bundled.

### Lead and closer

`kokoro.py context` hears how what is said around a line changes how
Kokoro says it. Each `[[kokoro.context]]` study has a voice say its
`lines` plainly, then after each `[[kokoro.context.mood]]`'s `lead`,
before its `closer`, and between both, all cut off: a closer is cut in
the pause the line's own `.`, `!` or `?` leaves before it, as a lead is
in its own. On the page as a grid, lines down and ways across; ⚠ marks
a cut that landed in a sound (the clip's first or last 10 ms within
35 dB of its loudest), which says more about the cut than the tone.

`george`, the first (2026-11-05), has `bm_george` say six lines around
a neutral, an excited, a sarcastic and a disappointed lead and closer.
Measured (pitch by autocorrelation; `state.json` keeps each clip's
cuts), where the context is moves the line more than what it says:
before a closer its pitch sits 20–60 Hz higher than after a lead, and
the short lines end level or rising rather than falling, as if going
on; between both, its pitch moves least, and least of all between the
disappointed ones. Within a place, the moods move it by 15 Hz at most;
the sarcastic texts do nothing measurable the others do not, and
whether any sounds sarcastic is for the ear. The sarcastic closer,
"Yeah, sure.", ran into "Ready?" with no pause to cut in.

### Our pack

`kokoro.py pack` makes the announcers `pack.toml` (English) and
`pack-ja.toml` (Japanese) write, each as `Announcers/<name>/`, beside the
auditions and on the same page, where each line's variants play in turn:

- **Every line the game speaks** (`docs/announcer-lines.md`): in play,
  on the results and on song select, our own lines too, and the combos
  past 1000 at 1500 and 2000 (the overflow says the other hundreds).
  Left out: `evaluation aaaaa`, which nothing gives yet, and
  `evaluation f`, which falls back to `evaluation e`.
- **Variants** where a line comes again: eight for the comments heard
  every 10 s, two to six elsewhere. Kokoro says a text the same way each
  time, so a variant is another text, or the same one after another
  lead or in the other style: a folder may be given in several
  `[[line]]`s, each with its own `style` and `lead`, its sounds theirs
  together (the same sound twice stops the run).
- **No grade letter or judgment name** in a text: the game maps every
  ruleset's grades onto StepMania's seven by place and names judgments
  by ruleset, so a line says how it went (`evaluation aaa`: "Outstanding!").
- **One voice and style** for the whole pack (`voice`, `speed`,
  `style`), a `[[line]]` may take its own style. George's two styles are
  side by side in the page's Lines table, from `[[kokoro.voice]]` in
  `audition.toml`.
- **A lead for each place**, since how Kokoro says the lead carries into
  the line: `[lead]`'s `select`, `gameplay` and `evaluation` (a folder's
  first word) set song select's tone, play's and the results', or
  a `lead` string one for every line; a `[[line]]` may take its own,
  which puts it in the `lead` style in a `plain` pack too. The cut goes
  in the pause between lead and line, so a lead ends as a sentence does:
  `bm_george` pauses 150–250 ms after `.` or `!`, a little longer and
  steadier with `…` after it (`.…`); with no ending, a comma or `…`
  alone, the cut often lands in sound. Japanese voices pause less
  (`jm_kumo` some 60 ms after `。`, 100 after `。…` or `…`), so end
  their leads with `…`. The page shows the lead most of a card's lines
  take, and beside a line any other.
- **Files are named by their text** (`gameplay ready/get-ready.mp3`; in
  Japanese by its sound, `gameplay ready/jumbyi-wa-i.mp3`; the same
  name again in a folder takes `-2`, `-3`, in the file's order): a text
  changed makes its sound again under its new name and removes the old
  one, and anything else in the pack's folder goes too. What a pack file
  made is its own: making one leaves the other's alone.
- **In the game** with `--bundle`: each pack as it was made, in
  `assets/announcers/<name>/` (what else is there goes, and a folder the
  pack file bundled under another name), with a `PROVENANCE.toml`
  written from the records: the pack file, model, revision, voice,
  versions, CC0 held by its `author`, and what a voice in it learned
  from where that asks for credit (`[[trained_on]]`, from VOICES.md's
  CC BY column), and what each sound says (`[said]`, the announcer
  studio's examples; its examples without a sound are in
  `crates/library/src/announcer/examples.toml`). Then `cargo run -p xtask -- gen-announcers` checks them
  and writes the web's index, and `gen-credits` the credits. The game
  lists them first under Options → Sound → Announcer, Off by default.

### In Japanese

`pack-ja.toml` says the same lines in Japanese: `ガキンさん`, in
`jm_kumo`, the game ディトイン. Kokoro's Japanese voices (VOICES.md grades them on their training
data) are on the page, reading `[kokoro.texts] j`:

- `jf_alpha` (C+, hours of data): the best of them, of a source not
  given, as the English voices.
- `jf_gongitsune`, `jf_tebukuro` (C) and `jf_nezumi`, `jm_kumo` (C-, a
  few minutes; `jm_kumo` the one man): each learned from one TV
  Nishinippon announcer reading a children's story, recordings under
  CC BY 3.0 (the Koniwa corpus's `tnc`), so a pack with them credits
  that (`--bundle` writes it), and its voice is that announcer's.
- Any other voice with `lang = "j"`, or a blend: `george-ja` and
  `kumo-george` on the page.

Its phonemes are Kokoro's own way, misaki's cutlet over UniDic, here its
lite edition (the full one is a separate download of hundreds of MB).
Latin letters have no reading, so the game is ディトイン, in kana; numbers
are kanji, which it reads right (misaki's kana for 300 comes out
"san-bi-ya-ku"); a word it reads wrong is written in kana (一回 as
いっかい). Anything Kokoro has no phoneme for stops the run.

## Hearing them in the game

Our packs, bundled, are under Options → Sound → Announcer already. For
the auditions, `Announcers/<voice>--<style>/` holds a StepMania announcer each (the
lines in their folders, each text's takes as variants; Kokoro's styles
are `kokoro` and `kokoro-lead`), so:

```sh
cargo run -- --songs target/scratch/announcer-audition/Announcers --screen options
```

then Options → Sound → Announcer (choosing one speaks its
`select music intro`), and play a song: plays and their results speak the
rest. On the web, zip one of the folders and import it.

## Provenance

`state.json` keeps what each request asked for and what came back: for
a design round, its description, model, parameters and previews' ids; for
a kept voice, the preview it came from; for a clip, the voice, model,
settings, seed, text, request id and time; for Kokoro's (`samples`,
`pack` and its clips), the model's revision, the voice, the text, its
phonemes and the versions it was made with. A pack bundled later carries
the same in its `PROVENANCE.toml`.
