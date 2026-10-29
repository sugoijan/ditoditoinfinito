# Quiet calibration (2026-10-29)

The maintainer's request (2026-10-29): the device alerts and the
calibration notices are too technical and too intrusive. They go, on both
shells. What they did is taken over by two things, one of them later:

1. A solid onboarding (not now; a plan of its own when it comes).
2. A much subtler suggestion to calibrate, on the start prompt, that never
   forces a choice: a device without a stored calibration plays on the
   defaults without asking, and the suggestion is kept or dropped by what
   the play history shows of the timing on the current devices. A
   consistent shift is evidence that a calibration is needed; it is never
   applied by itself. Timing that is nothing but noise around zero is
   evidence that the defaults are good enough, and dismisses the
   suggestion. A poor play on its own is inconclusive.

## What goes

Today (`ddi_ui::screens::notices`, `crates/web/src/notices.rs`,
`crates/desktop/src/devices.rs`, `ddi_ui::screens::prompt::new_device_step`,
the web's `calibration::Outcome` card on the prompt's screen):

- The first-run banner over every screen ("First time here? Calibrate the
  timing…", calibrate / later), shown while no profile exists.
- The new-device banner naming the audio path and the display ("New audio
  path: …. No calibration for it yet.", calibrate / use defaults / use +N ms
  from … / later).
- The devices line when the devices change to known ones ("devices: …
  (+12 ms) · … (+0 ms)").
- The prompt's new-device step, which replaces the card and holds Start
  until a choice is made (calibrate, defaults, a copied offset, play anyway).
- The calibration outcome card after a calibration run (what it measured
  for the audio path and the display, applied or saved): too technical,
  and it displaces the prompt's layout.

All of it is removed, not hidden behind a setting. The F2 hint and the
notice bar's chrome layer go with them unless another notice still needs
the bar (the app's own notices: a crash recovered, storage refused; keep
the bar for those, without the device texts). The calibration page keeps
offering a known device's offset where one exists, since that is where a
player chooses.

## What stays

- Offsets per audio path and per display, their profiles and fingerprints
  (`Settings::audio_profiles`, `display_profiles`; PLAN.md's timing model).
  A device seen for the first time is registered with the default offset
  at once, on both shells (`register_devices` with no banner before it; the
  desktop's `Devices::for_play` already does this when a play starts).
- Calibration itself, its page and its modes; the results' offset editor.
- The play record's devices and the offset in force
  (`ddi_ui::state::history::Play::{audio, display, audio_offset}`), and the
  worked timing of each play (`Worked::{hits, mean, stddev}`): the evidence
  below needs nothing new recorded.

## The evidence

A verdict per device pair, worked from the kept plays on the current audio
path and display (`ddi_ui::state::history`, a pure function with tests):

- Plays counted: not autoplay, not assisted, ended by playing (not quit, not
  failed), on the same audio and display ids as now, with at least
  `MIN_HITS_PER_PLAY` (50) judged hits; the last `WINDOW` (10) such plays.
- Each play's mean delta is measured against the audio offset in force
  then; a play made under another offset is shifted by the difference, so
  the pool reflects the current setting (a play older than the last
  calibration of either device is dropped instead).
- Pooled mean: hit-weighted; pooled standard error from each play's
  stddev and hits. The spread of the per-play means is also looked at: the
  sign must agree across the window for a shift to count.
- `Verdict::Shift { ms }` when `|mean| ≥ SHIFT_MS` (15) with
  `|mean| ≥ 3 × standard error` and every play in the window on the same
  side: the suggestion shows and names the direction ("your hits run
  about 20 ms late").
- `Verdict::Fine` when `|mean| < FINE_MS` (8) over at least `FINE_HITS`
  (300) pooled hits: the suggestion is dropped for these devices.
- `Verdict::Inconclusive` otherwise (too few hits, a wide spread, mixed
  signs): the suggestion shows only while the devices have no calibrated
  profile, as a neutral line.

The thresholds live in one place (`history::timing_verdict`) with a doc
comment, since they are a judgment call to tune by playing. A calibrated
device with a later `Shift` verdict shows the suggestion again (the
setup drifted, or the calibration was poor), which is the case the
banners never caught.

## The prompt's line

One muted line on the card, below the modifiers, in the small face, laid
out so the card is the same height with or without it (its row is
reserved whenever a line could appear for these devices; never under the
loading screen). Three texts:

- Inconclusive, no calibrated profile: "Timing not calibrated for this
  setup · Calibrate".
- Shift: "Your hits run about 20 ms late on this setup · Calibrate".
- Fine, or calibrated and inconclusive: no line (the row stays empty).

"Calibrate" is a link to the calibration page (the prompt's `Calibrate`
action already exists for the new-device step); the song and chart are
kept to come back to, as the outcome card's "back" did. No dismissal
control: the evidence dismisses it. Under a controller the line is
reachable as the prompt's other links are (`controller-hints.md`); a
touch screen taps it.

Both shells feed the prompt the verdict with the devices seen at the
prompt (`PromptInfo::timing` or a field of its own), the history read once
per prompt (it is already open for the ghost and the bests).

## Tests and debug

- `history::timing_verdict` on made-up play sets: a clean shift, noise
  around zero, mixed signs, too few hits, an offset changed mid-window, a
  play older than a calibration.
- The prompt's screen tests: the row's height constant with each text and
  with none; the link's focus order; the compact layout.
- `__DDI_DEBUG.prompt.timing` (`{verdict, ms, hits, plays}`) and desktop's
  `--seconds` print; the verify skill's recipe (seeding profiles to avoid
  the first-run notice becomes unnecessary and is removed from it).
- A `#/play?…&timing=shift|fine|none` testing parameter is not needed:
  the history can be seeded in IndexedDB as the replay tests seed it.

## Steps

1. Remove the banners, the devices line, the prompt's new-device step and
   the outcome card on both shells; register new devices silently; keep
   the notice bar for the app's own notices only. Update AGENTS.md, the
   verify skill, `overlays.md` and `start-prompt.md`, PLAN.md's status.
2. The verdict in `ddi_ui::state::history`, with its tests.
3. The prompt's line, fed by both shells, with its tests and debug.
4. The calibration page's copy, which the banners used to introduce, read
   once more for jargon (a separate small pass).

Open: the thresholds after playing with them; whether the results screen
should show the same line (no, for now: the prompt is where a play is
about to start, and the results already have the offset editor).

## Status (2026-10-29)

Step 1 done (decision 76): the banners, the devices line, the prompt's
new-device step and the outcome card are gone on both shells, with the
notice bar, the web's device watcher and desktop's banner code; a device
first seen is kept at the default offsets as the play starts
(`Settings::register_devices`). A calibration run settles by itself
(`ddi_session::calibration::settle`): a meaningful correction applied
and saved, a plain line for the menu (`screens::calibration::last_runs`,
a "Last test" heading at the menu's top, cleared by its next Start), then
the guided flow's next test's prompt, or the menu after the last. Left
out of the plan's first wording: copying a known device's offset to a
new one (the banner's "use +N ms from …"), which no screen offers now;
the calibration page could, later. Steps 2 and 3 next.
