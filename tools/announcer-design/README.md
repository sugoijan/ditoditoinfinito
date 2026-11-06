# Announcer audition

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
- **The sound** is trimmed, faded at its ends and leveled (voiced RMS
  −18 dBFS, peaks under −1 dBFS), at 44.1 kHz in 128 kb/s MP3. The same
  configuration makes the same bytes, so a pack can be made again
  exactly: what each was made from is in `state.json`, beside the
  versions of misaki, spaCy and onnxruntime. A clip is made again only
  when that changed.

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
  CC BY column). Then `cargo run -p xtask -- gen-announcers` checks them
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
