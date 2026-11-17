# The DDI ruleset

Status: planned 2026-11-12, from the maintainer's draft and answers of
2026-11-11/12; every open question answered. Round 1 (the engine),
round 2 (the screens: DDI selectable, the default, shown everywhere) and
round 3 (multiplayer: rooms play DDI alone, a shock is a mine) built
2026-11-12 ("Round 1 (built)", "Round 2 (built)", "Round 3 (built)" and
"Round 3b (built)" at the end). Combos count in the score since
2026-11-16 (`ddi-combo-v0`, "Combos count" at the end).
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
language" (the default) or English. Since 2026-11-17 (the maintainer):
English by default, Japanese, or "Same as game language", each other
language offered by itself; stored settings put back to English once. The notes it was built from:

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

One point count, shown two ways, and a combo part beside it.

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

- **Score** (the default display): three parts together, the
  proportional part, the absolute score and the combo part,
  `floor(1,000,000 × points ÷ most) + (100 × points + W0s) + combo`
  (see the notes below). It counts up from 0 during the play; a
  1,000-note chart played perfectly with every tap a W0 is 1,000,000 +
  5,000,000 + 1,000 + 1,062,400 = 7,063,400, and a longer chart gives
  more.
- **The combo part** (`ddi-combo-v0`, since 2026-11-16, "Combos count"
  below): every note judged in combo adds `6 × min(k, 100) + 28 ×
  isqrt(k − 100)` points, `k` the combo after it (the second term 0 up to
  combo 100): quickly up to combo 100, then slowly and without a cap.
  Whole numbers, in the order the notes are judged; nothing divided by
  the chart's size. It follows the combo exactly (above: W0 to W3
  continue it, a dropped hold neither adds nor breaks), never falls, and
  carries across a step of the ladder as the combo does. A full combo of
  100 notes adds 30,300, of 300 notes 200,672, of 1,000 notes 1,062,400.
- **Percent** (the other display): the proportional part's ratio, two
  decimals, floored. Grades are read from it: pure accuracy, the combo
  part never in it.
- **The parts** stay readable (`DdiScoreView::ratio`, `absolute`,
  `combo`): the proportional part, `floor(1,000,000 × points ÷ most)`, is
  the same for every chart played as well; the absolute score, `100 ×
  points + W0s`, is not divided by the chart's size (FFR's raw score's
  role), and its W0s are the score's tie-break; the combo part is not
  divided either. The results show the combo part on a line of its own
  ("Combo"), beside Score or Percent; the absolute score has none.
- The first two may be negative; the combo part never is.

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
| Judgment words | English (default since 2026-11-17), Japanese, the game's language | no | no |

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
  (`JudgmentWords::Localized`, the default, or `English`; since
  2026-11-17 `English`, the default, `Language(tag)` or `Game`, stored
  as "English", the tag or "Localized"), the row
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

## DDI's score and combos: a study (2026-11-16)

**Measured only: nothing a player sees or a play scores changed** (the
combo part was built after it, from round P1a's extension of this
study: "Combos count" below). The
maintainer (2026-11-16): "our DDI ruleset seems to reward precision too
strongly, making it so some fullcombos lose in score to non fullcombos
that had a few more precision. I think we should incorporate the length
of combos into the score, and also the max combo, carefully balancing it
so a longer combo counts a lot more than if the combo was broken in 2."
Today a miss costs 60 points, exactly six *slips* (a W1 that became a
W2, 10 points), wherever it falls: a full combo loses to any play with
one miss and seven fewer W2s.

The study is `combo_score_study`
(`crates/session/src/battle/combo_study.rs`; `DDI_COMBO_CORPUS=<dir>[:<dir>…]
cargo test --release -p ddi-session --lib combo_score_study -- --ignored
--nocapture`). It reads plays as the engine judged them (each play's
judgments in order, its points, the notes it was shown) and works
candidate scores out beside today's; today's score worked out that way
is asserted equal to the engine's for every play. MEASUREMENTS OF CPUS
(`cpu-levels-v1` playing the chart a person is given, `cpu-play-v1`), not
of people: where a CPU breaks (lapses under load, in the dense passages)
is its model's. The combo is the engine's: W1 to W3 continue it, a W4, a
W5, a Miss and a mine or shock stepped on break it, a dropped hold does
not.

### The candidates

Each is today's score plus a combo part; the percent, and so the grades
and the lamps, stay today's. A full combo's part is a share S of a
scale: `M`, the proportional million (S = 10, 20, 30%, as asked), or
`F`, the chart's own full score, the million and 100 a point (S = 2, 4,
6%: added by the study, see "Two scales"). With N the chart's tap-like
notes:

- **A50, A100, A200 (a ramp to a cap):** each note judged in combo earns
  `min(combo, K) / K`; over a full combo's sum.
- **B (the runs' squares):** the sum of each run's length squared, over
  N². One run 1, two halves 0.5, four quarters 0.25.
- **C (the longest run):** the longest run over N.
- **A100+B, A100+C:** half of each.
- **Miss −50, miss −100:** no combo part, a heavier miss (the percent and
  the grades change with it).

Every part is whole-number arithmetic (sums of `min(combo, K)`, of
squares, a maximum), so devices would agree on it exactly.

### What a break costs, and where

A made-up full combo, every tap a W1, one tap turned into a miss; in
slips (today 6 everywhere). The part of each number beyond today's 6 is
proportional to S: at 20% M it is twice what is shown, at 30% three
times.

| S = 10% M | 100 notes: middle | 300: at 2.5% | 300: at 25% | 300: middle | 600: middle | 1,200: at 2.5% | 1,200: middle |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Today | 6.0 | 6.0 | 6.0 | 6.0 | 6.0 | 6.0 | 6.0 |
| Miss −50 | 10.0 | 10.0 | 10.0 | 10.0 | 10.0 | 10.0 | 10.0 |
| Miss −100 | 15.0 | 15.0 | 15.0 | 15.0 | 15.0 | 15.0 | 15.0 |
| A50 | 17.3 | 7.6 | 11.6 | 11.6 | 9.3 | 7.6 | 7.9 |
| A100 | 22.8 | 7.8 | 17.4 | 18.1 | 12.9 | 8.0 | 9.8 |
| A200 | 22.8 | 8.4 | 24.5 | 32.3 | 21.1 | 8.2 | 13.8 |
| B | 23.0 | 9.1 | 28.8 | 36.2 | 43.6 | 10.3 | 48.9 |
| C | 22.7 | 7.6 | 21.2 | 36.0 | 43.5 | 8.2 | 48.9 |
| A100+B | 22.9 | 8.5 | 23.1 | 27.1 | 28.3 | 9.1 | 29.3 |
| A100+C | 22.7 | 7.7 | 19.3 | 27.0 | 28.2 | 8.1 | 29.3 |

A slip is 0.2 percent points on 100 notes, 0.067 on 300, 0.033 on 600
and 0.017 on 1,200: A100+B's 27 to 29 slips are 1.8 points of percent on
300 notes and 0.5 on 1,200.

More misses, spread evenly (the costliest places under B and C) and, in
brackets, at random places (the mean of 400 draws), S = 10% M:

| | 300 notes: 2 | 300: 5 | 300: 10 | 1,200: 2 | 1,200: 5 | 1,200: 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Today | 12 | 30 | 60 | 12 | 30 | 60 |
| A50 | 23 (21) | 58 (50) | 103 (92) | 16 (16) | 39 (39) | 79 (76) |
| A100 | 36 (29) | 72 (61) | 111 (103) | 20 (19) | 49 (46) | 98 (88) |
| B | 52 (42) | 80 (73) | 115 (110) | 69 (54) | 102 (92) | 138 (132) |
| C | 52 (35) | 80 (66) | 115 (104) | 69 (44) | 101 (81) | 138 (122) |
| A100+B | 44 (35) | 76 (67) | 113 (106) | 44 (37) | 75 (69) | 118 (110) |

The same on the CPUs' real full combos (157,983 of them over the local
charts, their own mix of judgments): one miss put
in at a place of the chart, slips, and one more miss at the middle of
each of the 82,139 clears that already had one to five breaks.

| S = 10% M | 2.5% | 10% | 25% | 50% | 75% | 90% | 97.5% | One more |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Today | 5.9 | 5.9 | 5.9 | 5.9 | 5.9 | 5.9 | 5.9 | 5.8 |
| Miss −50 | 9.9 | 9.9 | 9.9 | 9.9 | 9.9 | 9.9 | 9.9 | 9.8 |
| A50 | 7.5 | 10.6 | 13.6 | 14.8 | 13.6 | 10.6 | 7.6 | 10.5 |
| A100 | 8.0 | 12.3 | 18.4 | 21.8 | 18.3 | 12.2 | 8.0 | 12.8 |
| A200 | 8.4 | 13.8 | 22.0 | 27.5 | 22.0 | 13.8 | 8.4 | 14.2 |
| B | 8.7 | 14.6 | 23.4 | 29.0 | 23.4 | 14.6 | 8.7 | 14.4 |
| C | 7.3 | 10.7 | 17.6 | 28.8 | 17.5 | 10.7 | 7.3 | 14.7 |
| A100+B | 8.3 | 13.4 | 20.9 | 25.4 | 20.8 | 13.4 | 8.3 | 13.6 |
| A100+C | 7.6 | 11.5 | 18.0 | 25.3 | 17.9 | 11.5 | 7.7 | 13.7 |
| A100+B 20% M | 10.8 | 21.0 | 35.9 | 44.9 | 35.8 | 20.9 | 10.7 | 21.4 |
| A100+B 30% M | 13.2 | 28.5 | 50.9 | 64.4 | 50.7 | 28.4 | 13.2 | 29.2 |

Each candidate in a sentence:

- **A (the ramp):** a break costs about K/2 notes' bonus wherever both
  runs beside it are longer than K, so the curve is flat with ramps at
  the two ends; but the part is one fixed prize shared over N notes, so
  on the `M` scale the cost fades with the chart's length (A100: 23
  slips on 100 notes, 18 on 300, 10 on 1,200, where it is hardly more
  than today's 6) and "broken in two" loses only K/2 of N. On the short
  arcade charts A100 and A200 are nearly B.
- **B (the squares):** exactly "a longer combo counts a lot more than
  broken in two" (half the part), and the known unfairness with it: a
  miss in the middle costs four to five times one at an end (36 against 9
  slips on 300 notes, 49 against 10 on 1,200), and after the first break
  the others cost little more than today's (the second about 16 to 20,
  from the fifth on about 7 each).
- **C (the longest run):** B's middle cost with a sharper peak (five to
  six times the end's), and breaks outside the longest run cost nothing: two
  random misses cost 35 slips where two even ones cost 52.
- **A100+B:** the middle costs 23 to 29 slips at every length measured
  (A's share fades with length as B's grows), 8 to 9 at the ends (about
  three times), and each further break keeps costing 7 to 17.
- **Miss −50, −100:** 10 or 15 slips, the same everywhere, no reward for
  the long run, and the percent and grades of every play with a miss
  fall.

### Two scales

On `M` the combo part is the same 100,000 (at 10%) on every chart while
the rest of the score grows with the chart (a point is 100 of score
whatever the chart), so the part is worth 6.7 percent points on 100
notes, 4 on 300 and 1.4 on 1,200. On `F` it is the same share of the
chart's full score, so a full combo is worth S percent points on every
chart and nothing more accurate than that can be overturned; 4% F is 10%
M on a chart of 300 notes. In slips `F` grows with the length instead
(A100+B 4% F, a miss at the middle: 16 slips on 100 notes, 27 on 300, 42
on 600, 71 on 1,200; B alone 126 on 1,200).

### Plays of known skill

The ten absolute levels, eight seeds a level, DDI Normal, the fail off,
on every dance-single chart as written of the local arcade mixes
(sixteen folders, 1,161 simfiles, 4,750 charts; read in place,
aggregates only) and of the bundled songs (19 charts, in line with
these, too few to choose by). Plays whose gauge emptied are left out
(they are no clears). Of the clears:

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Out (%) | 53.5 | 42.0 | 31.3 | 24.4 | 15.7 | 9.7 | 6.6 | 3.4 | 1.6 | 0.2 |
| Full combos (%) | 10.5 | 7.0 | 34.9 | 38.3 | 30.6 | 33.9 | 67.3 | 76.2 | 78.6 | 85.2 |
| Breaks a play | 10.8 | 11.4 | 8.7 | 8.7 | 7.6 | 6.0 | 4.0 | 2.8 | 2.6 | 1.0 |
| Percent | 83.2 | 85.0 | 90.6 | 91.2 | 93.1 | 94.4 | 96.6 | 97.7 | 98.0 | 99.0 |
| A100's part | 0.35 | 0.36 | 0.60 | 0.64 | 0.64 | 0.68 | 0.86 | 0.91 | 0.92 | 0.97 |
| B's part | 0.33 | 0.33 | 0.55 | 0.58 | 0.56 | 0.60 | 0.80 | 0.86 | 0.87 | 0.93 |
| C's part | 0.42 | 0.43 | 0.62 | 0.65 | 0.63 | 0.67 | 0.84 | 0.88 | 0.90 | 0.95 |

**The case the maintainer named.** Over the pairs of clears of one chart
where one is a full combo and the other has exactly one break (1,545,326
pairs; today the full combo loses every one in which the broken play has
more points): how often the full combo still **loses**, by how much more
accurate the broken play is, in percent points.

| Broken play's lead | up to 0.25 | 0.25 to 0.5 | 0.5 to 1 | 1 to 2 | 2 to 4 | over 4 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Pairs | 24,094 | 22,075 | 25,781 | 24,663 | 15,451 | 2,237 |
| Today | 100 | 100 | 100 | 100 | 100 | 100 |
| Miss −50 | 15.6 | 44.7 | 73.1 | 98.8 | 100 | 100 |
| Miss −100 | 3.4 | 12.0 | 29.1 | 59.8 | 89.1 | 100 |
| A50 10% M | 13.2 | 30.4 | 38.4 | 55.5 | 80.6 | 100 |
| A100 10% M | 6.6 | 17.6 | 23.6 | 30.5 | 56.2 | 100 |
| A200 10% M | 5.2 | 10.4 | 16.5 | 18.1 | 54.4 | 100 |
| B 10% M | 3.4 | 7.4 | 11.8 | 16.8 | 51.8 | 100 |
| C 10% M | 7.4 | 13.7 | 20.1 | 33.5 | 73.3 | 100 |
| A100+B 10% M | 4.5 | 9.4 | 15.9 | 22.2 | 54.3 | 100 |
| A100+C 10% M | 6.9 | 13.9 | 21.3 | 31.1 | 66.1 | 100 |
| A100 20% M | 2.7 | 6.7 | 12.0 | 13.7 | 11.7 | 33.8 |
| B 20% M | 1.5 | 3.9 | 5.5 | 8.0 | 7.1 | 26.6 |
| A100+B 20% M | 2.0 | 5.0 | 7.0 | 10.1 | 9.2 | 30.0 |
| A100+B 30% M | 1.3 | 3.3 | 4.6 | 6.6 | 4.8 | 13.9 |
| A100+B 2% F | 9.0 | 20.0 | 51.0 | 99.9 | 100 | 100 |
| A100 4% F | 6.0 | 13.4 | 25.5 | 49.1 | 99.9 | 100 |
| B 4% F | 3.3 | 7.4 | 11.9 | 27.2 | 99.5 | 100 |
| A100+B 4% F | 4.3 | 9.0 | 15.0 | 37.8 | 99.7 | 100 |
| A100+B 6% F | 2.5 | 6.2 | 9.7 | 15.1 | 70.7 | 100 |

Against a play with two breaks (5,309 pairs in which it has more points)
the full combo loses less still: A100+B 10% M 1.4, 2.8, 10.1, 28.8 and
74.2% in the first five columns, B 10% M 1.0, 1.7, 7.1, 16.4 and 64.6%,
miss −50 3.8, 25.0, 56.9, 91.7 and 96.6%. With three and more breaks a
broken play is hardly ever the more accurate one (703 pairs of 853,562
with three to five, 18 with six to ten, none beyond).

At 10% M a full combo beats nine in ten one-break plays up to half a
point more accurate and three in four of those one to two points ahead,
and a play two to four points more accurate still wins about half the
time; at 20% and 30% M one break is all but decisive (a full combo
beats nine in ten plays two to four points more accurate). `M` blurs
the line because the part weighs more on short charts; on `F` the line
is sharp at S.

**Rankings.** Over all 10,644,098 pairs of clears of one chart that
today's score orders: the share a candidate turns around (the rank
correlation with today's, Kendall's tau, is one less twice that); over
the 204,125 pairs where one play has fewer breaks and the other more
points, how often the one with fewer breaks wins, by the other's lead;
and over the 10,287,730 pairs where one play has no more breaks and no
fewer points, how often it loses (a break's place alone deciding;
today's 0.07% are equal points decided by the W0s).

| | Turned (%) | Fewer breaks wins: lead up to 0.25 | 0.5 to 1 | 1 to 2 | 2 to 4 | The better play loses (%) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Today | 0 | 0 | 0 | 0 | 0 | 0.07 |
| Miss −50 | 0.82 | 77.0 | 23.9 | 4.2 | 0.5 | 0.11 |
| Miss −100 | 1.38 | 85.4 | 61.6 | 37.2 | 13.0 | 0.21 |
| A50 10% M | 1.13 | 73.6 | 41.8 | 28.0 | 11.9 | 0.19 |
| A100 10% M | 1.45 | 76.5 | 53.2 | 44.6 | 27.0 | 0.32 |
| A200 10% M | 1.58 | 76.0 | 58.3 | 52.8 | 28.3 | 0.38 |
| B 10% M | 1.61 | 76.0 | 60.9 | 53.7 | 29.9 | 0.39 |
| C 10% M | 1.60 | 73.9 | 56.5 | 43.7 | 16.7 | 0.49 |
| A100+B 10% M | 1.55 | 76.7 | 58.3 | 50.0 | 28.3 | 0.35 |
| A100+B 20% M | 2.03 | 81.3 | 69.5 | 63.7 | 58.2 | 0.61 |
| A100+B 30% M | 2.29 | 83.2 | 73.8 | 69.3 | 63.5 | 0.79 |
| A100+B 4% F | 1.41 | 77.4 | 57.8 | 39.7 | 0.3 | 0.31 |

Every candidate leaves the order of 97 to 99% of the pairs alone (most
pairs are levels apart); what moves is what was asked for, the cleaner
play past the slightly more accurate one. C is the least fair for what
it moves (the most "better play loses" at each S).

### A multiplayer stage

The attack study's stages (`attack_rates`: eight CPUs at levels 1, 2, 3,
4, 5, 6, 8 and 10 on its six bundled charts, 24 seeds, 144 stages a
row), DDI Normal. As picked ranks those still in by life, then the score
(62% of the pairs still in are level on life with attacks, 88%
without); Adaptive ranks by the score first. How often the final order
of those still in and its first place differ from today's, and what an
attack received costs in score (the same play with its shocks not
stepped on and the notes it missed in the lanes they locked hit; under
Adaptive the steps it took are left as they were), against today's
(9,462 of score an attack As picked, 12,283 Adaptive):

| | As picked, attacks: order / first (%) | attack costs | As picked, none | Adaptive, attacks | attack costs | Adaptive, none |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Miss −50 | 3.5 / 0.7 | ×1.56 | 6.2 / 2.1 | 18.8 / 1.4 | ×1.54 | 13.2 / 0.7 |
| Miss −100 | 11.1 / 4.2 | ×2.27 | 16.7 / 4.9 | 38.9 / 2.8 | ×2.22 | 34.7 / 4.2 |
| A50 10% M | 4.9 / 2.1 | ×1.23 | 8.3 / 3.5 | 7.6 / 2.1 | ×1.13 | 4.2 / 0.7 |
| A100 10% M | 6.9 / 2.8 | ×1.30 | 13.9 / 4.9 | 8.3 / 2.8 | ×1.13 | 9.7 / 4.2 |
| B 10% M | 6.2 / 3.5 | ×1.33 | 18.8 / 6.9 | 7.6 / 3.5 | ×1.12 | 13.9 / 5.6 |
| C 10% M | 7.6 / 2.8 | ×1.31 | 12.5 / 4.9 | 8.3 / 2.8 | ×1.14 | 8.3 / 4.2 |
| A100+B 10% M | 6.9 / 3.5 | ×1.31 | 15.3 / 5.6 | 8.3 / 3.5 | ×1.13 | 10.4 / 4.2 |
| A100+B 20% M | 11.8 / 6.2 | ×1.63 | 23.6 / 8.3 | 12.5 / 4.2 | ×1.26 | 16.0 / 6.2 |
| A100+B 30% M | 15.3 / 6.9 | ×1.94 | 27.1 / 10.4 | 16.7 / 5.6 | ×1.38 | 19.4 / 7.6 |
| A100+B 4% F | 6.2 / 3.5 | ×1.36 | 16.0 / 5.6 | 8.3 / 3.5 | ×1.14 | 11.1 / 4.2 |

At 10% M a stage's order changes in about one stage in fourteen with
attacks and its winner in one in thirty; attacks grow a little more
decisive in score (an attack received costs 1.1 to 1.3 times today's),
not in who is out (life is untouched). Stages without attacks change
more often than those with them: with attacks life already separates
more of the field.

**A step between charts** (Adaptive): the engine keeps the combo across
a step. A step up plays the old chart to the boundary and the new one
from it with the count running on; the notes taken away are not judged,
so they neither add nor break (of 496 steps up with attacks, 244 had a
run going across the boundary, 333 of 706 without; the others had a
break of their own near it). A step down comes when life runs out, which
a miss or a shock did, so the run was already broken (none of 900
carried). The notes a part is measured over are those the play was shown
(`ScoreCtx::steps` after `restep`), as the score's most already is; on
`M` a full combo on an easier rung is worth as much as on a harder one,
which the study of playing an easier chart on purpose
(`adaptive_study`'s fourth part) would have to be run again for.

### The pick

**A100+B at 10% of the million** (a full combo adds 100,000: half by the
ramp to 100, half by the runs' squares), for "a full combo should
usually beat a slightly more precise broken play, without one mid-song
miss deciding everything":

- one break in the middle costs 23 to 29 slips against today's 6 at every
  length from 100 to 1,200 notes, where A fades to nothing on long charts
  and B and C grow to 49;
- a full combo beats 95% of one-break plays up to a quarter point more
  accurate, 84% of those half a point to a point ahead and 78% of those
  one to two points ahead, while a play two to four points more accurate
  still wins about half the time and one more than four always: at 20%
  one break is all but decisive;
- broken in two, the part falls by 35% on 300 notes and 27% on 1,200 (B
  alone 50%, A100 alone 20% and 4%): the long run counts a lot more, and
  every further break still costs 7 to 17 slips where B's cost flattens
  after the first;
- its unfairness is B's, halved: a miss in the middle costs about three
  times one at an end (B four to five times, C five to six), and a play no worse
  in breaks or points loses in 0.35% of such pairs.

If the trade should be the same in percent on every chart rather than
in judgments, the same shape at **4% of the chart's full score** is the
alternative: a full combo is worth four percent points on every chart, a
miss in the middle 1.2 to 1.8 points from 300 notes up, and it is
identical to 10% M on a chart of 300 notes. The maintainer's "a few more
precision" reads as judgments, which is why the pick is `M`.

### What adopting it needs (not done)

- **The engine's score:** `DdiScore` already gets the combo after every
  event (`ScoreCtx::combo`) and the notes (`ScoreCtx::steps`, taken again
  at `restep`): it would keep the ramp's sum, the closed runs' squares
  and the running one, in whole numbers. `DdiScoreView` gains the part
  (and what it is made from), `score` becomes `ratio + absolute + combo`;
  `percent`, the grades and the lamps stay. Live in play the part counts
  up with the score and never falls.
- **Kept plays:** `ddi::Stored` works a view out from `points`, `most`
  and `absolute`, which do not say the runs: the new numbers are kept
  with the view, and `JUDGE_VERSION` rises so kept plays are judged again
  from their logs by themselves (as for versions 3 and 4). Bests go by
  `best_cmp` on the score, so they are ranked again as the plays are
  judged again; a best may change hands to a cleaner play.
- **The screens:** nothing must change (the score is one number), but
  the results would want a line saying the combo's part; the ghost's
  lead, song select's best and the plays list follow the view.
- **Multiplayer:** a stage ranks by scores each device reports
  (`Run::Live`'s `score`, a member's own) and the CPUs' scores every
  device computes: peers must agree, so `ROOM_MODEL_REVISION` rises and
  the CPUs' proof's fixture (`the_cpus_play_alike`) is recorded again;
  the private server ranks what it is told and needs the revision alone.
  The miniatures' scores and the Standings follow.
- **Adaptive:** the part over the notes shown, as above, and
  `adaptive_study`'s "easier chart on purpose" run again.
- **The combo's own rules** stay to be decided first (the maintainer is
  testing W4 as a break): the part follows whatever breaks the combo.

### Not measured

- **People.** The maintainer's kept plays were not allowed for this
  study and were not read; where people break (nerves near the end of a
  full combo, the hard passage in the middle) is not the CPUs' model.
- Long charts by play: the arcade charts' clears average 140 notes at
  level 1 to 270 at level 10; 600 and 1,200 notes are the made-up tables
  alone (the stream packs were not run).
- DDI Easy and Hard, Boos on, charts with many holds (a dropped hold
  breaks no combo), and online stages between people.
- What the part does to the choice of an easier rung under Adaptive.

## Combos count (2026-11-16, `ddi-combo-v0`)

**Built (round P1a): DDI's score has a combo part.** The maintainer's
words, after the study above: "our DDI ruleset seems to reward precision
too strongly, making it so some fullcombos lose in score to non
fullcombos that had a few more precision. I think we should incorporate
the length of combos into the score, and also the max combo, carefully
balancing it so a longer combo counts a lot more than if the combo was
broken in 2." Asked one by one: the mix at the heavier weight, 200,000
("a break should matter a lot"); "an absolute amount, not exactly
'scaled' but counted with absolute values like hits are" (every note hit
in combo adds its own combo points, growing with the combo: quickly up
to combo 100, then slowly and without a cap; nothing divided by the
chart's size; a full combo on a 300-note chart worth what the 200,000
mix was); a line of its own on the results; percent, grades and lamps
pure accuracy, unchanged.

**The rule** (`ddi_engine::rules::ddi::COMBO_SCORE`, PROVISIONAL, its
numbers `COMBO_RAMP`, `COMBO_RAMP_POINTS`, `COMBO_GROWTH_POINTS`):

    c(k) = 6 × min(k, 100) + 28 × isqrt(max(k − 100, 0))

added for each note judged in combo, `k` the combo after it, in whole
numbers in the order the notes are judged (`ddi::combo_points`). The
part is the sum; the Score is `ratio + absolute + combo`.

| Notes in one run | 100 | 300 | 600 | 1,200 | 2,400 |
| --- | ---: | ---: | ---: | ---: | ---: |
| A full combo's part | 30,300 | 200,672 | 532,656 | 1,356,812 | 3,438,256 |
| Its share of the score (every tap W1) | 2.0% | 7.4% | 11.8% | 16.2% | 20.9% |
| Broken in two: the part lost | 50.5% | 34.2% | 24.8% | 21.6% | 21.1% |
| One miss in the middle: slips (today 6) | 11.1 | 47.2 | 105.2 | 256.9 | 676.3 |
| … in percent points | 2.2 | 3.2 | 3.5 | 4.3 | 5.6 |
| One miss at 2.5% / at the first or last note: slips | 6.6 / 6.2 | 10.6 / 6.6 | 20.1 / 6.9 | 43.6 / 7.3 | 103.8 / 7.8 |

(A *slip* is a W1 that became a W2, 10 points; a percent point of
accuracy is `10,000 + 50 × notes` of score.)

### How it was chosen (the study, extended)

`combo_score_study` (`crates/session/src/battle/combo_study.rs`) measures
per-note candidates beside the first study's pick at 20% of the million
(the "200,000 mix", the reference), each weighted so a full combo of 300
notes is about 200,000: a ramp to 100 (`R` = `min(k, 100)`) plus
`isqrt(k)` (R+sqrt: 4 and 30), `ilog2(k)` (R+log2: 4 and 53),
`min(k, 1000)` (R+lin≤1000: 3.9 and 2.2), `k` (R+lin, uncapped: the
mix's runs' squares counted per note), and `isqrt(k − 100)` past 100
(R+sqrt>100: 6 and 28, chosen; and twice its growth, 4 and 56). The
engine's combo part is asserted equal to the chosen candidate's on every
CPU play the study makes.

Made-up full combos, every tap a W1: the part lost when broken in two,
and the part's share of the score at 2,400 notes:

| | 100 | 300 | 600 | 1,200 | 2,400 | Share at 2,400 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| The 200,000 mix | 50.7% | 35.2% | 29.7% | 27.2% | 26.1% | 1.5% |
| R+sqrt | 41.4% | 25.4% | 21.1% | 20.7% | 21.8% | 20.0% |
| R+log2 | 33.8% | 18.0% | 11.5% | 8.5% | 6.9% | 14.0% |
| R+lin≤1000 | 50.5% | 35.3% | 35.7% | 38.6% | 22.0% | 28.2% |
| R+lin | 50.5% | 35.3% | 35.7% | 40.0% | 44.0% | 35.8% |
| **R+sqrt>100** | 50.5% | **34.2%** | 24.8% | 21.6% | 21.1% | **20.9%** |
| R+sqrt>100, twice the growth | 50.5% | 48.3% | 35.9% | 30.4% | 28.3% | 27.8% |

- **R+sqrt>100 is the one that meets every condition**: at 300 notes it
  is the mix (200,672; broken in two loses 34.2% against the mix's
  35.2%; a miss in the middle costs 47.2 slips against 48.3, at 2.5% of
  the chart 10.6 against 11.0); the loss in two halves stays over a fifth
  up to a marathon (not vanishing); it grows without a cap, slowly
  (`c(2,400)` is 2,000 against `c(100)`'s 600); and a 2,400-note full
  combo's part is 21% of its score (under a quarter).
- R+sqrt loses only a quarter at 300 notes (no ramp-and-root reaches 35%
  there with a quick ramp); R+log2's halves fade to 7% (vanishing);
  R+lin≤1000 is capped and its part 28% of a marathon's score; R+lin
  (the mix's squares per note) 36% and growing; twice the growth loses
  48% at 300 and is 28% of a marathon's score.
- **Short charts weigh it less than the mix did.** An absolute part is
  small where the notes are few: 30,300 for 100 notes (2% of the score;
  a miss in the middle 11 slips, 2.2 percent points, against the mix's
  40 and 8); the arcade charts' clears average 140 to 275 notes, so most
  of them sit below the 300-note calibration.

The CPUs (`cpu-levels-v1`, `cpu-play-v1`), eight seeds a level, DDI
Normal, the fail off, every dance-single chart as written of the local
arcade mixes (1,066 simfiles, 4,429 charts; read in place, aggregates
only), clears only; MEASUREMENTS OF CPUS, not of people:

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Full combos (%) | 10.6 | 7.0 | 34.8 | 38.1 | 30.4 | 33.7 | 67.0 | 75.8 | 78.2 | 84.6 |
| Breaks a play | 10.9 | 11.5 | 8.8 | 8.7 | 7.6 | 6.1 | 4.1 | 2.9 | 2.7 | 1.1 |
| Notes a play | 139 | 161 | 184 | 198 | 218 | 235 | 245 | 258 | 266 | 274 |
| The part, of a full combo's | 0.34 | 0.34 | 0.57 | 0.60 | 0.60 | 0.64 | 0.83 | 0.89 | 0.90 | 0.95 |

A full combo against a play of one break more accurate than it (the
pairs of clears of one chart), how often the full combo still loses, by
the broken play's lead in percent points:

| Lead | up to 0.25 | 0.25 to 0.5 | 0.5 to 1 | 1 to 2 | 2 to 4 | over 4 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Today | 100 | 100 | 100 | 100 | 100 | 100 |
| The 200,000 mix | 2.0 | 5.0 | 7.0 | 10.0 | 9.4 | 30.8 |
| R+sqrt | 2.9 | 10.6 | 20.3 | 72.9 | 100 | 100 |
| **R+sqrt>100** | 2.7 | 11.1 | 20.3 | 60.6 | 99.2 | 100 |
| R+sqrt>100, twice the growth | 2.8 | 14.0 | 30.8 | 60.3 | 98.4 | 100 |

Over all pairs of clears of one chart: R+sqrt>100 turns 1.54% of them
around against today's score (the mix 2.05%); a play with no more breaks
and no fewer points loses in 0.44% of such pairs (the mix 0.61%, today
0.07%, its W0s); one miss put into the CPUs' full combos costs 8.4,
14.0, 22.1, 26.9 slips at 2.5%, 10%, 25% and the middle of the chart
(the mix 10.8, 21.0, 36.0, 45.0: the middle three times an end for
both), and one more miss in the middle of a play of one to five breaks
19.3 (the mix 21.5). On four local stream packs (255 charts, four seeds)
the clears of levels 9 and 10 average 1,300 to 1,700 notes and keep 51
to 58% of a full combo's part.

**A multiplayer stage** (the attack study's stages: eight CPUs at levels
1, 2, 3, 4, 5, 6, 8 and 10 on its six bundled charts, 24 seeds, 144
stages a row, DDI Normal, `adaptive-v5`): how often the final order of
those still in and its first place differ from today's, and what an
attack received costs in score against today's (5,356 As picked, 6,116
Adaptive):

| | As picked, attacks: order / first (%) | attack costs | As picked, none | Adaptive, attacks | attack costs | Adaptive, none |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| The 200,000 mix | 14.6 / 6.2 | ×1.79 | 23.6 / 8.3 | 11.1 / 5.6 | ×1.38 | 16.7 / 6.2 |
| R+sqrt | 11.1 / 3.5 | ×1.76 | 22.2 / 10.4 | 7.6 / 3.5 | ×1.37 | 13.9 / 6.2 |
| **R+sqrt>100** | 13.2 / 4.2 | ×1.87 | 25.7 / 11.1 | 9.7 / 4.2 | ×1.39 | 16.0 / 6.9 |

An attack's shocks break combos, so an attack received costs about 1.9
times today's in score As picked (1.4 under Adaptive, which ranks by the
score first); who is out is untouched (life is). Of 544 steps up under
Adaptive with attacks 262 had a run going across the boundary (333 of
706 without): the combo and its part carry on there; a step down comes
when life runs out, never with a run going.

**Playing an easier chart on purpose** (Adaptive ranks by the score):
for the 3,336 local charts with one below, a perfect play of the next
easier (every tap a W1, one run; 219 notes against the stage chart's
326 on average) against the stage's chart at 95% and at 85% of its most,
in one run / broken once / broken three times; how often the easier
wins:

| | 95%: one run | broken once | three times | 85%: one run | broken once | three times |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Today | 3.3 | 3.3 | 3.3 | 27.0 | 27.0 | 27.0 |
| The 200,000 mix | 3.3 | 8.9 | 13.8 | 27.0 | 40.8 | 50.0 |
| **R+sqrt>100** | 2.9 | 4.2 | 7.3 | 18.9 | 26.6 | 35.0 |

An absolute part makes the easier chart's fewer notes worth fewer combo
points: against a clean play of the harder chart going down pays less
than today; against a broken one a little more, far less than under the
mix (which gave every chart the same 200,000).

**The maintainer's kept plays** (read in place, read-only; aggregates
only; 97% of them played under other rules and judged again under DDI
before, their taps put on DDI's grid here by their offsets since the
charts were not read: 66% match the second judgment's counts exactly, an
estimate). Of the cleared plays: 13% full combos, 9% one break, 7% two,
22% three to five, 20% six to ten, 28% more; breaks by tenth of the play
from 8% of them in the first tenth rising to 14% in the last (the end
of a song is where this person breaks most); 54% of the notes in runs
of 100 or more; the longest run 54% of a play on average. The combo
part is 5.0% of a clear's score on average and 53% of what a full combo
would have given. Among this person's plays of one chart, R+sqrt>100
turns 7.0% of the pairs around against today's score (the mix 6.5%),
and **the best changes hands on 14% of the charts played twice or more**
(the mix 12.5%): there a cleaner play overtakes a slightly more accurate
one.

### What was built

- **Engine** (`rules/ddi.rs`): `DdiScore` counts the part as it scores
  (every step the player's combo grows by adds `combo_points` of it, so
  the part follows exactly what continues and breaks the combo, an
  avoided shock arrow too); `restep` (a step of the ladder, or a combo
  given to a staged play) counts on from the combo as it stands.
  `DdiScoreView::combo`, `with_combo`; `score` the three parts.
  `JUDGE_VERSION` 5.
- **Kept plays**: the part is kept with the view (`combo`) and read back
  (`Stored`); a view kept before reads with none. JUDGE_VERSION's rise
  makes every kept play stale, and the history's reworker judges each
  again from its log by itself, as for versions 3 and 4: a DDI play
  gains its part, its best is ranked again (`best_cmp` on the score, the
  part in it), an autoplayed record's grade stays the bot's. A play that
  cannot be judged again (its chart gone or changed) keeps its old
  summary, stale, and the best it is part of says pending, as before.
  What a play came to under other rules (`rejudged`) is judged again
  with it and stays beside what it came to as played. An export read
  either way (before or after the part) is judged again on import
  (tests in `ddi_ui::state::history`).
- **Screens**: the results' **Combo** line (the part's points; `results-ddi-combo`,
  ja コンボ, a draft) after Score or Percent; the HUD's score, Options'
  preview, song select's best and the plays list, the ghost's lead, a
  stage's Standings and the miniatures show the score as before, the
  part in it.
- **Multiplayer**: the scores each device reports (`Run::Live::score`,
  `RunDetail`) and computes for the room's CPUs include it, so a stage
  ranks by it; a remote person's percent is worked out from their
  stream's sum less the combo part their moments' combos add
  (`battle::live::ddi_of_sum`), exactly from the start of a stream (from
  part-way, runs that ended before are not told: the percent a little
  high). The room's model revision stays 12 (deployed nowhere yet);
  the server only carries scores.

### Not measured

- How people break: one person's kept plays, judged mostly under other
  rules and put on DDI's grid; nerves near the end of a long run are in
  them (breaks rise toward the end) but not in the CPUs.
- DDI Easy and Hard, Boos on, charts heavy in holds (a dropped hold
  breaks no combo, so its part is untouched).
