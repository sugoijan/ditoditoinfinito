# The DDI ruleset

Status: planned 2026-11-12, from the maintainer's draft and answers of
2026-11-11/12; every open question answered. Round 1 (the engine),
round 2 (the screens: DDI selectable, the default, shown everywhere) and
round 3 (multiplayer: rooms play DDI alone, a shock is a mine) built
2026-11-12 ("Round 1 (built)", "Round 2 (built)", "Round 3 (built)" and
"Round 3b (built)" at the end).
Values marked *tune* are starting
points the maintainer will play and adjust; everything else is decided.

## Why our own

Until now the game played other games' rules (ITG, ITG FA+, SM5, DDR A
and older, FFR, Dancing☆Onigiri), each reproduced as exactly as we
could. Multiplayer (`docs/plans/multiplayer-ui.md`, decision D0 of
2026-11-11) needs one ruleset everyone plays, and every attack decision
(D1–D3: what a shock costs, what earns power, whether a shock breaks the
combo) had to be answered once per borrowed ruleset. A ruleset of our
own answers them once and lets us take what we like from each family:
DDR's windows and its full-combo lamps, ITG FA+'s top tier inside the
best one, FFR's life, raw score, Boos and flags, Dancing☆Onigiri's
frame-multiple windows.

DDI is:

- the **only** ruleset in multiplayer (the Rules dialog's ruleset row
  becomes fixed; DDI's own options become match rules, below);
- the **default** for normal play;
- one more ruleset beside the others in Options, which all stay.

## Timing tiers

Seven outcomes for a tap, W0 to W5 and Miss. Windows are whole
multiples of a *grid*: 1/120 s on Normal (the size of a 120 Hz frame),
1/144 s on Hard, 1/90 s on Easy. A press within a bound (inclusive) is
in that tier; `delta = press − note`, positive late.

| Tier | Grid units | Normal (ms) | Hard (ms) | Easy (ms) | Early word | Late word |
| --- | --- | --- | --- | --- | --- | --- |
| W0 | 1 | ±8.33 | ±6.94 | ±11.11 | Absolute | Absolute |
| W1 | 2 | ±16.67 | ±13.89 | ±22.22 | Awesome | Amazing |
| W2 | 5 | ±41.67 | ±34.72 | ±55.56 | Fantastic | Glorious |
| W3 | 11 | ±91.67 | ±76.39 | ±122.22 | Nice | Great |
| W4 | 17 | ±141.67 | ±118.06 | ±188.89 | OK | Good |
| W5 | 25 | ±208.33 | ±173.61 | ±277.78 | Too early | Too late |
| Miss | — | the note passed unmatched | | | Miss | Miss |

- **The hint:** every late word has a G (Amazing, Glorious, Great,
  Good); no early word does. Nothing tells the player this; it is there
  for those who notice. W0 is not split. W5 says it plainly.
- **Familiar numbers:** on Normal, W1, W3 and W4 are exactly DDR A's
  Marvelous, Great and Good (whole and half 60 Hz frames); W2 sits
  between DDR's Perfect (33.3 ms) and ITG's Excellent (43 ms + 1.5).
  No `TimingWindowAdd`: the grid is the window.
- **W0 is a distinction, not points.** ±8.33 ms is at the edge of what
  devices measure: a 125 Hz controller or keyboard reports a press up
  to 8 ms late, and desktop reads pads every 5 ms
  (`crates/desktop/src/pads.rs`). W0 therefore scores exactly as W1 in
  the proportional score (below) and only shows in the absolute score's
  tie-break, the grade "Absolute Dito" and the lamps. ITG FA+ does the
  same with its two Fantastics.
- **Engine shape:** `JudgeTable::w0` (the FA+ sub-window inside W1) is
  W0; `Judgement::W1..W5` are W1–W5. The early/late word is the sign of
  the judged delta, already recorded per judgment.

### Early and late counts

Kept for W1–W5 (W0 is neither), named Early and Late, never Fast and
Slow. The results show them beside the tiers.

### Hit effects

W0 and W1 a white shine, W0 a little brighter; W2 a yellowish shine;
W3 greenish; W4 blueish and dimmer (round 4, below; the first draft had
W3 blueish and W4 greenish, as the skins then did). W5 and Miss show no
explosion. A skin keeps its own art; this is Infinito's and the
built-in arrows'.

**Noted for later (maintainer, 2026-11-12):** the shine's colors and the
colors the tiers' words have do not agree (W3's shine is blueish and
its word green, W4's shine greenish and its word purple). Make them one
set, and revisit the set itself: DDR's colors make more sense to the
maintainer (white for the best, yellow, green, blue, then red for a
miss).

**One set, built in round 4 (2026-11-13); awaiting the maintainer's
eye.** DDI's tiers have their own colors, in one table
(`ddi_engine::rules::colors::DDI`) that the play's words, the built-in
arrows' receptor flash, Infinito's hit effects, the results' rows and
timing histogram, a stage's Standings bar and its miniatures' flashes
and Options' preview all read (the theme's `Judgement.Ddi.*` colors
are made from it):

| | Word | Hit effect |
| --- | --- | --- |
| W0 Absolute | `#ffffff` white | white shine, full |
| W1 Awesome / Amazing | `#e2e4ee` white, a touch cooler | white shine, a little dimmer (`SkinEvent::dim`) |
| W2 Fantastic / Glorious | `#ffe14a` yellow | yellow echo with a white core |
| W3 Nice / Great | `#45eb7f` green | green echo |
| W4 OK / Good | `#4f9dff` blue | blue echo, dimmer (0.55 of the others' peak; the built-in flash 0.6) |
| W5 Too early / Too late | `#ff9633` orange | none |
| Miss | `#ff3d3d` red | none |
| Boo | `#ff5c9e` pink, as before | none |

- *W5 orange, not violet.* Both were drawn side by side (the sheet,
  below). Orange reads as "nearly a miss" next to Miss's red and keeps
  away from the Boo's pink, which shows in the same place; a violet W5
  is all but the same color as W4's blue for a red-green deficient eye
  (CIELAB ΔE 3 under a simulated deuteranopia, 10 under protanopia,
  Machado et al. 2009 at full severity), where orange stays 115 or more
  from blue.
- *What was checked:* every color is at least 5.9:1 against the
  field's ground (#05020d; a test holds 4.5:1); between neighbors under
  simulated protanopia, deuteranopia and tritanopia the closest pairs
  are W5–Miss (ΔE 19 deutan), W2–W5 (17 deutan) and W3–Miss (25
  deutan): told apart by lightness rather than hue, as DDR's own
  yellow–green–orange order is for those eyes. The word itself and its
  early/late form carry the tier too, so hue is never the only cue.
- *Only DDI changes.* The set travels with the ruleset's names
  (`JudgeNames::palette`); ITG, FA+, SM5, DDR A (and X, EXTREME), FFR
  and Dancing☆Onigiri keep StepMania's set exactly as the renderer and
  the theme had it (a value test over every ruleset in
  `ddi_render::scene`, and the offscreen pixel test
  `hud_colors_offscreen` for ITG and DDI).
- *Skins.* A skin keeps its own art; its Lua may ask
  `NOTESKIN:GetJudgmentColor("W4")` (our extension: `#rrggbb` under a
  ruleset with its own set, nil otherwise). Infinito's echoes take it
  under DDI and keep their own yellow, green and mint under every other
  ruleset. A room's stage tells its skin DDI's set whatever the
  player's own ruleset is.
- *Not changed:* DDI's lamps keep their colors (the Amazing Full Combo's
  pale cyan was W1's old color: an open question whether it should turn
  white, beside the near-white Absolute Dito); the Lua prelude's
  `JudgmentLineToColor`, StepMania's for its skins, stays StepMania's.

### Names in other languages (later phase)

Built in round 4 (below): Options → Rules → Judgment words, "In your
language" (the default) or English. Choosing a specific language for
them stays for later. The notes it was built from:

- Japanese can keep the hint: katakana writes a G sound with a voicing
  mark (゛). アメージング, グロリアス, グレート and グッド have one;
  オーサム, ファンタスティック, ナイス and オーケー do not. "Late words have
  ゛" is the same hint.
- That is why W3's early word is Nice, not Neat: ニート reads as NEET.
- Dancing☆Onigiri's own words (イイ, シャキン, マターリ, ショボーン,
  ウワァン; キター, イクナイ) are a reference for tone where a
  transliteration reads badly.

## Difficulty

Shown as **Timing difficulty** since round 3b (the maintainer:
"'Difficulty' doesn't convey the setting too well"), so that it is not
read as a chart's difficulty; summaries say "Hard timing" and "Easy
timing".

One choice, Easy / Normal (default) / Hard, which sets:

- the grid (1/90, 1/120, 1/144 s): every tap window, and the hold and
  roll grace below;
- the life losses' factor: Easy ×0.2, Normal ×1, Hard ×2 (gains never
  scale).

The mine window does not scale (a wider one on Easy would make mines
harder): 75 ms everywhere, nine units of the Normal grid, StepMania's
`TimingWindowSecondsMine`.

### Practice

An option, Off by default: reaching 0 life does not end the play. Life
stays at 0 for the rest of the song (it never recovers), the play goes
on to the end and is a failure (grade F, lamp Failed). This is
StepMania's "fail immediately, play on" (`FailPolicy::ImmediateContinue`,
whose gauge takes nothing more once failed). Single player only.

## Combo

- W0–W3 add one to the combo. W4, W5, Miss and a mine or shock stepped
  on break it (`ComboRules::continue_min = W3`, `mine_breaks`). The
  maintainer is testing breaking on W4 for a while.
- A dropped hold or roll does not break the combo and does not add to
  it; its head counts as a tap.
- One combo per note, not per row.

## Stray presses (Boos)

An option, **Off by default** (the maintainer plays with it On to judge
the default). On, a press matched to no note is a **Boo**. Never a Boo:

- a press before the first note's earliest window (warm-up) or after
  the last note's end;
- a press in a lane within 1 s before a lift there (the press that
  readies it);
- a roll's taps and a hold's re-press within its grace;
- a press while any mine, in any lane, is within 0.5 s (*tune*): so any
  free panel can be used to get off a mine;
- a press in a lane locked by a mine (below);
- releases, ever;
- a press with no note of any lane within 3 s of it (long breaks; a
  note's span is its head's earliest window to its end).

**A Boo shows its word (2026-11-12).** The maintainer: "when I enable
Boos they don't show up any text, I can notice the effect, but no text
showing it." A Boo is now a judgment of its own in play: the word "Boo"
(`ddi::BOO_WORD`, English data as the other judgment words,
`JudgeNames::boo`) where the tap judgments show, in its own color (the
theme's `Judgement.Boo`, `ddi_render::scene::BOO_COLOR`: beside Miss's
red, toward pink), with the same pop (none under reduced motion) and
fade, no early or late word, no error under it, and no explosion or
flash on the receptor. The exempt presses above show nothing, as before.
Why nothing showed: the judge made the `Boo` event and the player
counted it (the tally, the points, the life), but the play's judgment
flash was only ever set from a tap's event, so no frame carried a Boo to
the HUD. The flash is now set where the event is scored
(`Player::score_event`, `JudgementFlash::boo`), so a replay shows the
word at the same presses and the results' Boos count is the number of
words shown. Options' field preview is an autoplay and never strays, so
it shows none. Other rulesets are unchanged: FFR's and
Dancing☆Onigiri's stray presses (R^3's Boo, Excessive) are counted and
show no word here because their `JudgeNames::boo` is `None`; StepMania's
presets count no stray press at all, as StepMania.

## Holds, rolls and lifts

- A hold's head is judged as a tap (W0–Miss). A missed head drops the
  hold, as StepMania does.
- Kept as StepMania keeps it (`HoldRules::Decay`): released, the hold
  survives for the grace (38 grid units: 316.7 ms on Normal; rolls 42
  units, 350 ms, between taps) and is dropped past it. Re-pressing
  within the grace keeps it as if never released.
- Outcomes: **Held** and **Dropped** ("Off" until 2026-11-14, the
  maintainer's rename; ja ドロップ).
- **Partial credit by whole seconds.** A hold of length L earns
  `ceil(L)` *units*, L measured in the chart's seconds at 1× rate (the
  chart's timing, so a slower or faster music rate never changes it; a
  hold through a stop counts the stop's time). Its checkpoints are at
  1 s, 2 s, … after the head and the last at its tail; a unit is earned
  for each checkpoint the hold is still alive at. A release or regrab
  within the grace changes nothing; a drop keeps the units already
  earned.
- Rolls are the same, sustained by taps.
- Lifts are judged like taps on the release, with the tap windows.

## Mines and shocks

- One object, two looks. A chart's own mines show as the skin's mines;
  the shocks an attack puts into a multiplayer stage show as shock
  arrows. Judged identically.
- Stepped on (pressed, or held, within the 75 ms window, as now): the
  combo breaks, life drops (Easy −1, Normal −5, Hard −10), and the
  **lane locks** for 1 s (*tune*).
- **A locked lane** ignores its input: presses are neither hits nor
  Boos, a key held through the lock counts as released (a hold in it is
  then dropped past its grace), notes that pass are Missed, mines that
  pass are not stepped on. A key still down when the lock ends needs a
  new press. The receptor shows the hit's shock effect for the whole
  lock.
- **Placement:** an attack's shocks are always avoidable: never in a
  lane within the mine window of a note in that lane, never during a
  hold or roll in that lane, never between a lift's ready press and its
  release. The rest is skill. A chart's own mines are the chart's.

## Life

0 to 100, starting at 50; at 0 the play ends (Practice: see above).

| Event | Normal | Easy | Hard |
| --- | --- | --- | --- |
| W0, W1, W2 | +2 | +2 | +2 |
| W3 | +1 | +1 | +1 |
| W4 | 0 | 0 | 0 |
| W5 | −5 | −1 | −10 |
| Miss | −10 | −2 | −20 |
| Boo | −2 | −0.4 | −4 |
| Mine or shock stepped on | −5 | −1 | −10 |
| Hold or roll dropped | −5 | −1 | −10 |
| Hold or roll held (*tune*) | +2 | +2 | +2 |

From full life, ten Misses in a row end the play, as in ITG; it
recovers 2.5× as fast. The losses' factor is *tune*.

## Score

One point count, shown two ways.

| Event | Points |
| --- | --- |
| W0, W1 | 50 |
| W2 | 40 |
| W3 | 25 |
| W4 | 10 |
| W5 | 0 |
| Miss | −10 |
| Boo | −5 |
| Mine or shock stepped on | −10 |
| A hold's or roll's unit earned | 50 |

The most a chart gives is 50 a tap-like note (heads included) and 50 a
hold unit.

- **Score** (the default display): the proportional part plus the
  absolute score, `floor(1,000,000 × points ÷ most) + (100 × points +
  W0s)` (see the note below). It counts up from 0 during the play; a
  1,000-note chart played perfectly with every tap a W0 is 1,000,000 +
  5,000,000 + 1,000 = 6,001,000, and a longer chart gives more.
- **Percent** (the other display): the proportional part's ratio, two
  decimals, floored. Grades are read from it.
- **The two parts** stay readable (`DdiScoreView::ratio`, `absolute`):
  the proportional part, `floor(1,000,000 × points ÷ most)`, is the same
  for every chart played as well; the absolute score, `100 × points +
  W0s`, is not divided by the chart's size (FFR's raw score's role), and
  its W0s are the score's tie-break. Neither has a line of its own on
  the results.
- All of them may be negative.

**The displayed Score is the sum (2026-11-12).** The first build showed
the proportional part alone as the Score and the absolute score on a
results line of its own. The maintainer: "on the DDI ruleset when
displaying the 'score' it should be the absolute score (or at least
we're not adding the absolute score), currently it's just the percent
times 1,000,000"; asked which, the maintainer chose the sum. So the
Score everywhere it shows (the play's HUD and Options' preview, the
results, song select's best, the plays list, the ghost's lead under the
Score display) is `DdiScoreView::score`, the sum; the results show Score
and Percent and the "Absolute score" line went. Bests go by the score
(`best_cmp`: the sum, then the absolute score). A kept play's view is
worked out again from its points, the chart's most and its absolute
score when it is read (`ddi::Stored`), so a play kept before the change
shows the sum at once, and `JUDGE_VERSION` rose to 4 so kept plays are
judged again by themselves. For multiplayer (round 3 decides): a CPU
plays a tap-only copy of the chart, so its `most` differs from the
player's and the summed scores do not compare across competitors; the
proportional part (`ratio`, the percent) does.

- In a multiplayer stage the opponents' scores are shown on their
  miniatures: the score (the sum) or the percent, as the player's Score
  display has their own; a CPU's sum on the chart's own scale (round 3b,
  which replaced round 3's proportional part there).

## Grades

| Grade | Percent |
| --- | --- |
| Absolute Dito | every tap and lift W0, no mine or shock stepped on, every hold and roll held |
| SSS | 99 |
| SS | 96 |
| S | 89 |
| A | 80 |
| B | 68 |
| C | 55 |
| D | below 55, cleared |
| F | failed |

The thresholds were 99.5, 98, 95, 90, 80 and 70 until 2026-11-12. Over
the maintainer's 447 kept plays judged under both rules the percent
barely moved (a median 1.4 points lower under DDI) while the letters
fell a whole grade (S or better 24% to none, A 47% to 7%), because
ITG's letters ask less of the same percent (S- from 89, A- from 80, B-
from 68). The softer thresholds follow ITG's letter families and put
that history at S 10%, A 60%, B 26%; the points and the 1,000,000 score
are unchanged. `JUDGE_VERSION` rose to 3 with it, so kept plays (both
their judgments) are judged again by themselves and show the new
letters.

**Floors:** no single note costs more grades than tiers it fell. A
note's distance from W0 is its tier (W1 1 … W5 5; Miss, Dropped and a mine
stepped on 6); the play's grade is at least Absolute Dito minus the
sum of distances (never below D, never above SSS). One W1 is SSS
whatever the chart's size; one W2 at least SS; one Miss at least C. On
a long chart the percent already gives as much; the floors matter on
short charts.

An autoplay or a CPU's play that would be Absolute Dito is
**Absolute Bot**.

## Lamps

DDR's full-combo ladder named by the worst judgment (the late word),
with three FFR-style flags, one below each full combo. From best:

1. Absolute Dito (the grade; no separate lamp)
2. Amazing Full Combo: every tap W1 or better, no break
3. Glorious flag: one W2, the rest W1 or better, no break
4. Glorious Full Combo: worst W2, no break
5. Great flag: one W3, the rest W2 or better, no break
6. Full Combo: worst W3, no break
7. Break flag: exactly one break, the rest a full combo
8. Clear
9. Failed

For the lamps a **break** is anything that breaks the combo in play (a
W4, W5, Miss, or a mine or shock stepped on) and also a dropped hold
or roll and a Boo, which leave the combo alone in play but not the
lamp. So one Boo on an otherwise Amazing play is a Break flag.

## Options

DDI's own, beside the StepMania and Dancing☆Onigiri ones
(`StepManiaOptions`, `DanoniOptions`):

| Option | Values | Changes judging | Part of the bests' key |
| --- | --- | --- | --- |
| Timing difficulty | Easy, Normal, Hard | yes | yes |
| Boos | Off, On | yes | yes |
| Practice | Off, On | fail only | no (a failure either way) |
| Score display | Score, Percent | no | no |
| Judgment words | in your language (default), English; a language later | no | no |

In multiplayer, Timing difficulty and Boos are match rules (one value for
everyone, in the Rules dialog); Practice is never offered; the displays
stay personal.

## What it means for multiplayer (M8)

The attack decisions of 2026-11-11 stand, read for DDI:

- **D0:** DDI only; the ruleset row shows it fixed. The room's
  `RULESETS` becomes `["ddi"]` (with protocol 2's old values still read).
- **D1 (damage):** a shock is a mine: −5 life on Normal, a combo break
  and the lane locked. This replaces "a shock costs the ruleset's
  miss". It also answers M8b's question (one attack costing about 80%
  of an ITG FA+ bar): eight shocks cost 40 life plus what the locks
  cost.
- **D2 (power):** W0 and W1 earn 2, W2 earns 1 (`POWER_CLASSES_V0`'s
  row for `ddi`, as FA+'s and DDR A's).
- **D3 (combo):** shocks break it (DDI's `mine_breaks`).
- **Placement:** `PLACEMENT_V0` checks the avoidability rules above
  (a new version if it changes: `placement-v1`'s margins in round 3,
  `placement-v2`'s aim beside the lane's notes in round 3b).
- **CPUs:** a CPU's shocks lock its lanes too (`MINE_RESPONSE_V0`'s
  play then misses what passes in them). Mine avoidance is the 8th skill
  axis since round 4 (Evasion, `MINE_RESPONSE_V1`).
- **Ranking:** those still in by life, then score (the sum, round 3b);
  unchanged otherwise.

## Normal play

- DDI is first in Options' ruleset list and the default for new
  settings. Settings that name ITG (the old default) move to DDI once,
  by a settings migration; other rulesets were chosen and stay.
- Later, not now: settings could keep whether a value is the default
  or was chosen ("X (default)" against "X (explicit)"), so a change of
  default moves only those who never chose. The settings store every
  value today, so the two cannot be told apart, which is why ITG moves
  wholesale.
- A play's record keeps `RulesetKey { id: "ddi", ddi: Some(DdiOptions) }`;
  bests are kept per difficulty and Boos.
- The announcer's grade comments map AD → the best tier's lines, SSS
  the next, and so on down.

## Rounds

1. **Engine:** `rules/ddi.rs` (the judge table on the grid, the names
   with their early/late words, combo, life, the point count and its
   three views, grades with floors, lamps and flags, the options),
   `RulesetKey`, a play log version if the lock or the Boo exemptions
   need one; the lane lock in the judge; hold units. Trace tests in the
   engine's style.
2. **Screens:** Options' rows; the HUD's score or percent; the early or
   late word in play; the results (both words per tier, Early/Late,
   absolute score, grade, lamp); song select's lamps; history and bests.
3. **Multiplayer:** the M8 changes above, then M8 round 3.
4. **Later:** localized judgment words; the CPU's mine-avoidance skill;
   one set of tier colors for the words and the hit effects ("Hit
   effects", the note of 2026-11-12). Built: "Round 4 (built)".

## Decided after the first draft (2026-11-12)

Hold units 50 points; the three single flags only (Glorious, Great,
Break); dropped holds and Boos are breaks for the lamps; no Boos with
no note within 3 s; ITG settings move to DDI once.

## Decided after the re-review (2026-11-12)

- **Boos count against perfection:** Absolute Dito needs no Boo, and a
  Boo's floor distance is 6, as a Miss.
- **A missed hold head is the Miss alone** (StepMania): the hold earns
  no unit and is not also charged as dropped (no extra life, no extra
  floor distance, one break for the lamps).
- **Time:** tap windows, graces, the mine window, the lock and the Boo
  rules are in real (heard) seconds; hold units alone in the chart's.
- **Every chart:** DDI plays every chart by default, pump's and
  Dancing☆Onigiri's included (the ruleset mode's default, the chosen
  preset for every chart, stays). Holds are DDI's own on all of them:
  no ticked pump holds under DDI.
- **Bests:** the highest score, ties by the absolute score; the best
  lamp kept apart. (Since the Score became the sum, "Score": the
  highest sum, which carries the absolute score's W0s.)
- **Re-judging:** kept plays are judged again under DDI from their
  input logs when the settings move from ITG, so bests are not empty,
  and Options has a button to judge the kept plays again under the
  ruleset chosen (round 2; the history's reworker does the judging).
  A re-judged play is marked as such.
- **Shock placement (round 3):** margins, not the bare mine window: no
  shock within 0.25 s after a note in its lane (*tune*; a held key steps
  on a mine) nor within W3 before one.
- **Round 3 measures attack rates again:** power's top class is
  ±16.67 ms now.

## Round 1 (built, 2026-11-12)

The engine alone: no screen offers DDI yet, and the other rulesets judge,
score, grade and save exactly as before.

- **Where:** `crates/engine/src/rules/ddi.rs` (the options, constants
  for every *tune* value, the score, the life, grades, lamps, names);
  the lock and the Boos' exemptions in the judge
  (`crates/engine/src/judge.rs`: `JudgeTable::lock`, `Judge::lock_lane`,
  `Judge::is_locked`, `EmptyPress::Guarded`); hold units, the rate, the
  lamp and Absolute Bot in `crates/engine/src/player.rs`. Trace tests in
  `crates/engine/tests/ddi.rs`, cadence in `tests/cadence.rs`.
- **Key:** `RulesetKey { id: "ddi", ddi: Some(DdiOptions { difficulty,
  boos, practice }) }`, `ddi` left out of other rulesets' keys as
  written; `best_key` drops Practice. "ddi" builds DDI on every chart
  whatever `holds` says (pump's, Dancing☆Onigiri's: never ticked). It
  is not in `presets::all()` or `by_id` yet (Options' list is round 2).
- **Real seconds:** `ddi::at_rate`, which `Player::new` applies from the
  play's `ClockOptions::rate`, multiplies the windows, graces, mine
  window, early/late dead zone, lock and Boo spans by the rate (the
  judge counts song seconds); hold units use the chart's seconds. A
  replay takes the rate from the record's options, so it judges alike.
- **No new log version:** the lock and the exemptions follow from the
  chart and the edges alone; every edge was already logged.
- **API for round 2:** `ruleset.names.word(judgement, delta, w0)` (the
  word shown; `sides(j)` both words of a tier); `Results.fast`/`slow`
  are Early and Late (W0 is inside the dead zone),
  `Results.ddi.sides[tier]` per tier; `ScoreView.ddi`
  (`DdiScoreView`: `points`, `most`, `ratio`, `percent` in hundredths,
  `absolute`, `score` (the two together, as shown), `percent_text()`;
  `ScoreView.percent` is the same percent as a fraction);
  `DdiScoreView::best_cmp` (score, ties by absolute);
  `Results.grade` (`ddi::ABSOLUTE_DITO`, `ABSOLUTE_BOT`, SSS … D, F);
  `Results.ddi.lamp` (`ddi::Lamp`, ordered worst to best, kept apart
  from the score for bests); `Results.tally.hold_units`;
  `Judge::locked_until(lane)` for the receptor's lock effect.
- **Choices the plan left open:**
  - *Units reach the score at the hold's outcome* (its Held or Dropped),
    not at each checkpoint: no new judge event kind (which every
    record's timing format and screen would have had to learn). The
    count is the same; the score jumps at the hold's end.
  - *Absolute Bot* comes from `Player::set_autoplay` (the session's
    autoplay and the CPU opponent set it). A replay does not know who
    played: the screens give a record kept as autoplayed
    `ddi::bot_grade(&grade)`.
  - *A hold's units:* `ceil` of its length to the microsecond, at least
    one (a zero-length hold still has its tail). Alive at a checkpoint
    means not yet dropped strictly before it.
  - *Lock:* a note whose window is still open when the lock ends can
    be hit by a new press; one passing wholly inside the lock is
    missed. A lift whose key the lock lets go of is not judged by that.
  - *Boo spans:* a note's span is its earliest (W5) window to its end;
    "after the last note's end" is after the last tail or tap's own
    second. The 0.5 s mine rule counts every mine of the chart and
    every battle shock put into the play, stepped on or not. The lift rule is
    `BOO_LIFT_READY` (1 s) before a pending lift of the lane.
  - *Life* is kept in tenths (exact for Easy's 0.4); danger below 20
    (`LIFE_DANGER`, StepMania's `DangerThreshold`).
  - *Lamps:* a play with no tap judged is a Clear; a battle's shock
    stepped on is a break.
- **Round 3:** battle shocks still cost the ruleset's miss to the gauge
  (−10 on Normal) and lock nothing; `Judge::lock_lane` is there for
  them.

## Round 2 (built, 2026-11-12)

DDI is selectable, the default and shown on every screen; the other
rulesets look, judge and save as before.

- **Options and settings.** DDI is the first of `presets::all()`
  (`presets::ddi`, `by_id("ddi")`), never ticked (`presets::stepmania`),
  and takes the player's options through `Ruleset::with_ddi`
  (`Settings::ddi`, a `DdiOptions`; `Settings::ddi_score`, Score or
  Percent). `Settings::rules()` is the rules apart from the rest
  (`RuleChoice`), which builds a chart's ruleset. Under DDI the Rules
  section shows Difficulty, Boos, Practice and Score display in the place
  of StepMania's rows (holds, fail type, life and judge difficulty, life
  meter, battery lives, DDR gauge), which show under the other presets;
  Dancing☆Onigiri's rows stay under either, under their heading. The
  ruleset mode is untouched: its default plays every chart by DDI.
- **The move from ITG.** New settings name DDI. Stored settings naming
  ITG move once (`Settings::rules_moved`, read `false` where the field
  is missing) and ask for their ITG plays to be judged again
  (`Settings::rejudge_from`); any other ruleset stays, as does ITG chosen
  after the move, and a preset this build does not know becomes the
  default. The settings store every value, so a default and a choice
  cannot be told apart; keeping "default" against "explicit" is noted on
  `Settings::ruleset` for later, not built.
- **In play.** The judgment is the tier's early or late word, W0's
  "Absolute" (`names.word`; only where the ruleset has early words, so
  FA+ shows as before); the HUD's score is the score or, with Score
  display Percent, the percent, in Options' preview too
  (`RenderOptions::ddi_percent`). A W1 outside W0 sets off its explosion
  a little dimmer than a W0's (`SkinEvent::dim`, drawn at 0.78 where the
  skin's explosion is a component, Infinito's; a skin's own art and
  commands are kept). A locked lane's receptor keeps the mine's effect
  for the whole lock: the player sets the skin's `HitMine` off again
  every 0.25 s until the lock ends (`LOCK_EFFECT_PERIOD`, at the same
  song times at any frame rate), once only under reduced motion
  (`Player::set_lock_effect`). Autoplay is graded Absolute Bot, and so
  is a replay of a record kept as autoplayed (`Player::set_bot`,
  `Replaying::autoplayed`).
- **Results.** W0 by its word, then each tier by both words with its
  early and late counts ("Awesome / Amazing … 4 / 6"), Miss; Held / Dropped,
  mines, Boos (counted whenever they are on), Early / late in the place
  of Fast / slow, and beside the big number (the score, or the percent)
  the other of the two (the absolute score's line went when the Score
  became the sum, "Score"). The lamp by its name
  (BREAK FLAG, FULL COMBO, GREAT FLAG, GLORIOUS FULL COMBO, GLORIOUS
  FLAG, AMAZING FULL COMBO, ABSOLUTE DITO). A grade of two words is set
  on two lines beside the score. The mods line says DDI's options that
  differ ("hard", "Boos", "practice"); Practice's fail has its note.
  Judgment and grade words are English data; the labels are messages.
- **Song select, history, bests.** DDI's lamps are the history's `Lamp`
  (`BreakFlag`, `FullCombo` and five at the end, in the ladder's order),
  with their colors in the theme; a best is chosen by
  `DdiScoreView::best_cmp` (`better_score`), the best lamp apart, under
  `best_key` (difficulty and Boos). The lists show a DDI score per the
  Score display and write the two long grades by their initials (AD,
  AB), the lamps short (BREAKFLAG, FC, GREATFLAG, GFC, GLORYFLAG, AFC,
  AD). The ghost leads in points of the score, or in percentage points
  under Percent.
- **Re-judging** (`docs/plans/play-history.md`, "Judged again under
  other rules"). Both are kept: a play's record and what it came to as
  played are never replaced; its summary gains the rules it was judged
  again under and what it came to by them, which the bests count too.
  The move from ITG asks for the ITG plays; Options → Rules → Kept plays
  → Judge again for every play under the rules chosen now (rules that
  are a play's own take the second judgment away). The history's
  reworker does the judging, a song at a time; a play waiting is marked
  in its summary, so a pass goes on in the next session. Marked "judged
  again" in the plays list, with a note naming both rules on its results
  and replay.
- **Announcer.** Absolute Dito and Absolute Bot speak the best tier's
  grade line, SSS the next, and so on down, C and D the last
  (`old_grade`). The studio's results scenes are not staged under DDI
  yet: ITG's stand in.
- **Choices the plan left open:**
  - *StepMania's rows are hidden under DDI,* not shown as inapplicable:
    Dancing☆Onigiri's rows are always there because a chart may play by
    them whatever the preset; StepMania's never apply while the preset
    is DDI.
  - *The lock's effect is the hit's effect repeated,* not a new look, so
    every skin has it on both shells with no new art; reduced motion
    shows it once.
  - *W0 brighter than W1* is W1 dimmer, so that a W0 is the skin's
    explosion as drawn; the built-in fallback arrows' receptor flash is
    unchanged.
  - *A re-judged play shows as judged last* everywhere a single play is
    shown (list, results, replay); its ITG result stays in its summary
    and in ITG's bests. One second judgment a play.
  - *Short names:* AD and AB in lists; GLORYFLAG for the Glorious flag
    in a chart's row, where nine letters fit.
  - *The percent in lists* follows the Score display, as the HUD does.
- **Checked:** the engine's, UI's and session's tests (options' rows,
  the move, bests' keys, re-judging and its marks, results' text, the
  ghost, the announcer's grades, the lock's effect at two frame rates);
  headless Chrome and Firefox: Options in English and Japanese, a DDI
  autoplay to Absolute Bot, a play pressed early with keys held over
  mines (the words, the lock's effect, Boos on, Percent), stored ITG
  settings with a kept play moved and judged again, the button, song
  select's lamp and the plays list.
- **Round 3 needs:** `presets::ddi()` and `Ruleset::with_ddi` build the
  match's rules (Practice never); `ddi_room`'s `RULESETS` and the Rules
  dialog still list the old presets; battle shocks still cost the miss
  and lock nothing (`Judge::lock_lane`); a locked lane's effect comes
  from the play's `HitMine` skin events, which a miniature field does
  not read (its feed has `ReceptorState::mine`); the miniatures' score
  wants `ScoreView::ddi`.

## Round 3 (built, 2026-11-12)

DDI in multiplayer (the prototype behind its hidden switch; the tables
and the screens are in
[the prototype's progress note](multiplayer-prototype-progress.md), "The
DDI ruleset's round 3"). Outside multiplayer nothing changed: a solo play
has no shocks, and the other rulesets judge a shock as before.

- **D0: rooms play DDI alone.** `ddi_room::MatchRules::RULESETS` is
  `["ddi"]`; new rooms and practice play it. The rules' `ruleset` field
  keeps whatever id it was stored or received with (`itg-fa+` and the
  other six a room could name before are still valid,
  `LEGACY_RULESETS`) and is read through `MatchRules::ruleset_id`, which
  says `ddi` for all of them: an authority one build behind (the
  deployed server until its next deploy) still takes the rules back when
  a newer client edits them, and a stored room needs no migration.
  Protocol 2 is unchanged.
- **Difficulty and Boos are match rules** (`MatchRules::difficulty`, a
  `MatchDifficulty`, Normal by default; `boos`, Off by default): plain
  values, additive (left out of the JSON at their defaults, read as the
  defaults where missing), set in the Rules pop-up like Stages, locked
  with the match. `ddi_session::battle::stage_ruleset` builds
  `presets::ddi().with_ddi(..)` from them (`StageRules`) for the player,
  the CPUs and the bots; Practice is never set in a stage whatever the
  player's own settings say. The player's Score display still chooses
  what their own HUD shows.
- **A shock is a mine** (D1, D3; `crates/engine/src/battle.rs`,
  `Player::shock_is_mine`). Stepped on (pressed in its 75 ms window, or
  its lane held across it), a battle's shock under DDI goes through the
  score as a chart's mine does: the mines' count, −10 points, the life
  by the difficulty (−1, −5, −10), the combo broken, the grade's floors,
  the lamp's break, and its lane locked for the lock's second
  (`Judge::lock_lane`): presses ignored, a held key released, passing
  notes missed, passing mines and shocks not stepped on, a new press
  needed after it; the receptor keeps the mine's effect for the lock. To
  be exact the chart and the shocks are judged in step
  (`Player::advance_judges`: the chart up to each shock's second, the
  crossing with the lane as the chart's judge has it, the lock from that
  instant), and a press in reach of a shock and of a note steps on the
  nearer, as a chart's mine is chosen (`Judge::press_beside`). A twin
  test proves it: the same chart with its mines written in it, and
  without them but with the same mines put in as shocks, come to the
  same results at any frame rate and again from the log
  (`crates/engine/tests/ddi_shocks.rs`). No new `PlayLog` version: the
  shocks were kept since version 4 and the lock follows from them and
  the edges; stage plays are not kept anyway. Under a ruleset without
  the lock a shock is still the ruleset's miss to the gauge alone
  (`INJECTED_MINE_V0`), which no room plays any more.
- **Placement with margins** (`placement-v1`, `PLACEMENT`,
  `Clearance::Margins`; `placement-v0` kept for comparison through
  `schedule_with`, nothing stored names either): no shock from W3 before
  a step of its lane to 0.25 s after it (`after_note`, a value to
  tune), none from W3 before a hold's or a roll's head to 0.25 s past
  its end, none from a second before a lift (`lift_ready`, DDI's
  `BOO_LIFT_READY`) to 0.25 s past it, none on a chart's own mine.
  Sections without eight such places are skipped, as before. An exact
  play (every step on time, its key down a fifth of a second) steps on
  none of what it places, under every difficulty.
- **CPUs.** A CPU's shocks go through the same player and judge, so its
  lanes lock too and what passes in them is missed. It presses nothing
  stray (its extra press on a shock steps on it, its other presses are
  its notes'), so Boos cost it nothing: over twelve seeds of the
  weakest level with Boos on, none was counted. Each miniature shows a
  locked lane (the lane darkened, its receptor's place tinted, for the
  lock's time) from its feed's moments, with no skin Lua.
- **Power** (D2): `POWER_CLASSES_V0`'s row for `ddi` is 2, 1, 0…: W0 is
  judged inside W1 and reported as it, so the reports of judged tiers
  carry nothing new.
- **Ranking and scores.** A stage ranks those still in by life, then by
  the proportional score (points over the most their notes give, the
  1,000,000 ratio), then, among those who played the same notes (the
  same most), by the absolute score, which is to say by their W0s;
  those out by when they went out. `ddi_room::Absolute` on `Run::Live`
  and `ReportedRun::Live` carries it (additive, optional). Each
  opponent's score shows small under its miniature's life gauge, and
  the stage results' Standings show the same number: the proportional
  part out of a million, or the percent under the player's Score display
  Percent. Never the sum with the absolute score: a CPU plays a tap-only
  copy whose most differs from the whole chart's, so only the
  proportional part compares.
- **Attacks are a match rule** (the maintainer, 2026-11-12: "a
  multiplayer rule that's off by default but fully accessible is
  enough"): the Rules pop-up's Attacks row (Off, On) chooses the battle,
  `gauge-survival-v0` or `gauge-attacks-v0` (`MatchRules::attacks`,
  `set_attacks`), offered where attacks work: CPU practice. An online
  room shows the row as "Off · not in online rooms yet". The switches
  `--mp-attacks` (its old name `--mp-dev-attacks` still taken) and
  `mpattacks=1` are conveniences that start a practice room with the
  rule already On.
  **On by default since later that day** (the maintainer, 2026-11-12,
  after testing them: "it's working really well, we can switch it to On
  by default"): a practice room's rules start with attacks
  (`MatchRules::practice`), Reset goes back to them, and anyone turns
  them Off in Rules; the summaries then say "No attacks". An online
  room's rules name no battle with attacks (`MatchRules::hosted`), and a
  room where attacks are not offered plays none whatever its rules say
  (`RoomClient::attacks_played`). `--mp-attacks` and `mpattacks=1` are
  still taken and change nothing; `mpattacks=0` starts a practice room
  with the rule Off (`docs/plans/multiplayer-prototype-progress.md`,
  "M8 round 3").
- **Measured again** (the tables are in the progress note). The ten CPU
  levels alone on their tap-only copies over the same twelve local
  arcade mixes (2,974 charts, sixteen seeds; aggregates only), not
  retuned: percent 61.4 to 95.9 under Normal (68.3 to 97.8 Easy, 56.1 to
  94.2 Hard); out of the stage on 20.0, 9.6, 3.8 and 0.3 percent of the
  charts for levels 1 to 4 under Normal and never above, never under
  Easy, and 82.5, 69.6, 52.8, 26.4 and 3.1 percent for levels 1 to 5
  under Hard. (Under ITG FA+ the same levels scored 39.9 to 97.3 and
  went out on 68.9, 60.1, 44.0, 23.5 and 3.7 percent: Normal is far
  gentler on the weak, Hard is about where FA+ was.) Attack rates over
  the M8b study's six bundled charts, eight CPUs a stage: a bar fills
  1.1 times a minute for a one-star CPU and 3.6 times for the
  strongest under Normal; an attack received costs 23 life at level 1
  and 2.4 at level 10 (about half of it through the locks: a note
  missed in a locked lane costs twice a mine); a field of mixed levels
  launches 27 attacks a stage, 3.4% of which knock their recipient out
  (7.5% under Hard, none under Easy).
- **A stall found on the way** (feedback round B's open question): a
  room's stage prompt under reduced motion stopped drawing and ticking.
  The UI core's idle wake (`Ui::idle_wake`) lost its wake-up to the
  rounding of two clocks' differences; fixed there, with the test that
  would have caught it (the progress note has the account).
- **Choices the plan left open:**
  - *The Ruleset row went* rather than showing "DDI" fixed: a row with
    nothing to choose read as noise; the note under the rows says
    everyone plays DDI with these rules, and the summaries name it.
  - *Summaries* ("DDI · Hard · Boos · Attacks": the Rules pop-up's
    locked line, a stage's prompt) say only what differs from the usual:
    the difficulty when not Normal, Boos and Attacks when on.
  - *The old id is kept, not rewritten* (above), so a not-yet-redeployed
    server and a new client meet.
  - *A tie across other notes:* where life and the proportional score
    are equal and some played other notes (a person and a CPU, both
    perfect), each set of the same notes is ordered by its absolute
    score and the sets' bests share a place.
  - *The player's own HUD is unchanged* in a stage: under Score display
    Score it shows the sum with the absolute score, larger than the
    opponents' proportional numbers beside it (an open question below).
  - *The lock on a miniature* is ours (a dark lane, a violet receptor),
    whatever the skin; the player's own field keeps the skin's mine
    effect, as a chart's mine has it.
  - *Shocks are still drawn as the skin's mines.* Shock-arrow art is a
    later noteskin job (done in round 3b: `Tap Shock`).
- **Open for the maintainer:** the placement's W3 before a step lets a
  shock sit a sixteenth before a note at 150 BPM and more (0.1 s), where
  a press more than half of W3 early (46 ms on Normal) is nearer the
  shock and steps on it: within "always avoidable", but a trap for
  early players; W3 plus the mine window (167 ms on Normal) would rule
  it out. And whether the player's HUD in a stage should show the
  proportional part too.
- **Left for M8 round 3 and round 4:** the targeting choice and its
  screens, who-attacks-whom curves, announcer lines for attacks, attacks
  On by default, online attacks (the ledger run by the room from its
  members' reports, shocks delivered to each member's play), shock-arrow
  art, localized judgment words, the CPU's mine-avoidance skill axis,
  one set of tier colors.


## Round 3b (built, 2026-11-12)

Follow-ups from the maintainer's first test of DDI in rooms (the
account, the tables and the screens are in
[the prototype's progress note](multiplayer-prototype-progress.md),
"Round 3b"). Outside multiplayer one thing changed: the option's label
(the skin element for a battle's shock is loaded for stages alone).

- **The mostly black room after Stop match** was the web shell's
  history entries for dialogs, not the renderer: an entry pushed for the
  standings before the More menu's `history.go` had been carried out
  left a navigation waiting for a pop that never came. One history
  operation at a time now (`ddi_ui::back_stack::Entries`).
- **The score a room shows and ranks by is the sum** (the maintainer:
  "the proportional part was just a misunderstanding, visually we should
  have the percent and the score (sum) and that's it"): the miniatures,
  the stage's Standings and the ranking (life, then the score). A CPU
  plays a tap-only copy, so its sum is put on the chart's own scale
  first (`CPU_SCORE_V0`: its points as the same share of the chart's
  most as of its own, rounded; its score its proportional part plus 100
  a point of those plus its W0s, as if it had done on the holds as on
  the taps). `ratio` stays for the sum's arithmetic and is shown
  nowhere.
- **"Timing difficulty"** is the option's and the match rule's label
  (values Easy, Normal, Hard unchanged; Japanese タイミング難易度, a
  draft); where a summary says what differs it says "Hard timing" or
  "Easy timing" (the room's locked line, a stage's prompt, the results'
  mods line and the plays list, there in lower case as that line is).
- **A battle's shocks are drawn as shock arrows, a chart's own mines as
  mines** ("Mines and shocks": one object, two looks): a skin element of
  our own, `Tap Shock` (`docs/plans/noteskin-authoring.md`), which
  Infinito draws lane-shaped; a skin without it draws shocks as its
  mines. In a stage that may be given shocks Infinito's own mines are
  the spiked band whatever its `ShockArrows` parameter says. Judging is
  untouched.
- **Shocks placed where they matter** (`placement-v2`; the maintainer:
  "too easy to ignore sometimes"): the margins of `placement-v1`
  exactly, and six of an attack's eight shocks aimed at places next to a
  note of their lane (0.25 s to 0.5 s after one, or W3 to twice W3
  before a step), spread over lanes and beats, a second between two
  shocks of a lane. On the bundled charts 57% of the shocks are next to
  a note (17% under v1); between CPUs an attack costs about a third more
  life (16.5 to 21.8 under Normal), mostly through the locks. The CPUs'
  shock avoidance is invented: the direction is the placement's, the
  sizes are no balance.
- **No recovery period, and chained attacks** (`attacks-v1`; the
  maintainer: "we can let the recovery be even tighter, maybe even
  allowing chaining attacks"): an attack against someone is over at its
  last shock's window, and one more may be reserved behind the one on
  its way or passing, in the first safe section after it clears, never
  more than one waiting. The warning by the receptors shows the nearer
  one with a small second bolt for the one waiting; the recovery's
  shield went. Between eight CPUs the rates barely move (25.8 attacks a
  stage under Normal either way): a full bar nearly always found someone
  free; the rules matter in small fields, which were not measured.

## Round 4 (built, 2026-11-13)

Outside multiplayer only what a DDI play shows changed;
every other ruleset looks, judges and saves as before.

- **One set of tier colors** ("Hit effects" above has the table and
  what was checked). Awaiting the maintainer's eye: a before/after sheet
  was made in headless Chrome and Firefox (the words in play, a hit of
  each tier on Infinito and on the built-in arrows, the results, a
  stage's Standings and miniatures).
- **Judgment words in the player's language.** `Settings::judgment_words`
  (`JudgmentWords::Localized`, the default, or `English`), the row
  `ddi.words` among DDI's in Options → Rules; not a match rule, not in a
  best's key, never judging. The words are messages
  (`locales/<tag>/judgments.ftl`, `ddi_l10n::judgments`: W0, each tier's
  early and late word, Miss, Boo and the results' Boos), set on the
  ruleset's names where they are shown (`JudgeNames::shown`; the
  renderer keeps the shown names while the names, the language and the
  option stay, `Renderer::render`; the screens' `play::shown_names`), so
  the play, Options' preview, the results' rows, a stage's Standings,
  replays and the announcer studio's staged plays all follow it, and a
  kept play (its record has tiers, never words) shows in the language of
  now. English reads as before either way. Grades (Absolute Dito,
  Absolute Bot, SSS … F) and lamps stay Latin; Held and Off follow
  the other words since 2026-11-13 (`judgments-held`, `judgments-dropped`:
  キープ and オフ in Japanese), and with Boos on the results show the Boos as
  a row of their own among the judgments, in the Boo's color. The Japanese set is in `locales/ja/GLOSSARY.md`
  (an unverified draft): every late word has a ゛, no early word does;
  W5 is ハヤトチリ / マニアワズ rather than a transliteration. Tests: each
  language's hint (`ddi_l10n` `every_late_word_has_its_hint_and_no_early_word_does`;
  English's W5 has no G, as the plan has it), the results in Japanese
  and with the option off (`ddi_ui`), and the play's word drawn in
  Japanese (`ddi-desktop` `judgment_words_offscreen`).
- **Drawing them in play.** The HUD's text (glyphon) now also loads the
  languages' bold faces where it has their bytes (`TextPipeline`,
  `ddi_l10n::Face::bytes`): embedded on desktop (`embed-faces`), handed
  over on the web once the language's faces are fetched
  (`web::text::load_language_faces`, `ddi_l10n::add_face_bytes`; the
  browser's cache has the file), and used only as the fallback for what
  the HUD's Latin face lacks, so the pop, the color, the early/late
  placement and reduced motion are the word's as before. The katakana
  were already in the cut (its base holds every katakana). Cost
  measured offscreen on the maintainer's Mac (a new word every frame,
  each shaped once and then cached as in play): 0.11 ms a frame for
  English and 0.11 ms for Japanese, CPU and GPU together. The wasm is
  unchanged in size (no face is embedded in it).
- **Shock avoidance a CPU's skill** (the maintainer's M8 decision D8).
  `Skills` has an eighth axis, Evasion (0.5 to 2; seven-value lists keep
  parsing, the eighth 1; `--cpu-skills`, the web's `skills=` and the
  roster take eight), and a CPU's response to shocks is
  `MINE_RESPONSE_V1`'s: its level's (`mine-response-v0`'s) bent by
  `Evasion^-γ`, `γ = 1 + (busy + beside) / 2`, busy the chart's notes
  around the shock and beside a note of its lane, where `placement-v2`
  aims them. Evasion 1 is the level's response exactly (a fixture from
  the build before). Presets: technical 1.25, chords 0.8, and two new,
  evasive 1.6 and reckless 0.6; four roster CPUs dodge or stomp. Measured
  between CPUs over the bundled charts and over the maintainer's local
  arcade mixes (`docs/plans/cpu-player.md`, "Evasion"): Evasion 0.5
  about doubles the shocks a level steps on and 2 about halves them. A
  model of a CPU, never a person's skill.
