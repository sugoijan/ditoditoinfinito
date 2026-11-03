# Announcer audition

Tries voices for an announcer pack of our own made with ElevenLabs
(`docs/plans/announcers.md`, "A pack of our own"), before any is chosen.
Python 3.11 or later, standard library only.

- **The audition runs on the free plan.** Its audio is non-commercial and
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

## Hearing them in the game

`Announcers/<voice>--<style>/` holds a StepMania announcer each (the
lines in their folders, each text's takes as variants), so:

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
settings, seed, text, request id and time. The pack made later carries
the same in its `PROVENANCE.toml`.
