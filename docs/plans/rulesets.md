# More rulesets: DDR EXTREME, DDR's gauges and FLARE, LIFE4's lamps

Status: done 2026-10-21 (decision 57). Phase 10's "more rulesets"
(`docs/PLAN.md` section 5): DDR's legacy scoring as the `ddr-extreme`
preset, DDR A's other gauges (LIFE4, RISKY, FLARE I–EX, FLOATING FLARE)
with the FLARE rank and FLARE SKILL a play keeps, and LIFE4's clear types
as lamps. Everything lives in the shared crates (`ddi-engine`, `ddi-ui`,
`ddi-theme`), so both shells have it with no code of their own. The `ffr`
preset is not built (below).

## Research

Every number below is from a public source; where the source is a
community estimate it says so, and the preset is marked approximate.
Third-party code read to learn rules (Simply Love, the LIFE4 app) is not
transcribed: the LIFE4 apps are GPL-3.0 (`life4ddr/Life4DDR`) or carry no
licence (`life4ddr/LIFE4DDR-mobile-ng`).

### DDR's legacy scores

RemyWiki's "DanceDanceRevolution Scoring System" (after aaroninjapan.com's
pages) documents every era. Two were candidates:

- **DDRMAX2 and EXTREME** (normal play). Score: of `N` steps and freezes (a
  jump one step), the `n`th scores `p × floor(B / S) × n`, `S = N(N+1)/2`,
  `B` = 1,000,000 a foot of the song's rating, `p` 10 for a Perfect and an
  O.K., 5 for a Great, 0 otherwise; a freeze's O.K. is scored as the step
  after the last one scored; the last step, a Perfect or an O.K., adds
  `10 × (B − floor(B / S) × S)`, so all Perfect is `10 × B`. RemyWiki
  writes `× N` in that bonus, which cannot add up (with its own example,
  B = 8M and S = 97,461, it gives about 79.6M); `× S` does. Dance points:
  Perfect 2, Great 1, Good 0, Boo −4, Miss −8, O.K. 6, N.G. 0; grades
  AAA 100%, AA 93%, A 80%, B 65%, C 45%, D below, E failed (RemyWiki;
  Zenius-I-vanisher thread 7564 post 1, the MAX2 multipliers read from
  memory). Marvelous judges only in courses ("It is only used in courses",
  RemyWiki's AC DDR EXTREME; a hit inside its window flashes white).
  Great keeps the combo, Good, Boo and Miss break it; jumps add two to the
  combo ("Jumps now adding two to your combo (but count as only one step
  for all other purposes)", AC DDRMAX; one again from SuperNOVA); an N.G.
  "will not dock Dance Points or break combo, but will result in a
  significant loss of the Dance Meter" (AC DDRMAX; it breaks the combo
  from X). Windows were measured from EXTREME's `GAME.DAT` (Sesse,
  thread 7564 post 223; the table found at 0x085DB8 by PlasmaVenom and
  Arcorann, post 385): in 1/150 s ticks Marvelous ±2, Perfect ±4, Great
  −13/+11, Good −19/+17, Boo −23/+27, which with each bound half a tick
  out ("correct conversions", post 385) are ±16.667, ±30, −90/+76.667,
  −130/+116.667 and −156.667/+183.333 ms. The symmetric 60 fps table
  (±16.7/33/92/142/225 ms) is a 2016 Reddit post with no version or
  source, and disagrees. StepMania 3.9's `ScoreKeeperMAX2` (MIT) is the
  reference implementation, with deviations (it scores each freeze of a
  jump apart; StepMania 5.1 uses a fixed 100M a song and does not floor
  `B / S`).
- **SuperNOVA2 and X**: SC = 1,000,000 / (steps + freezes + shocks),
  Marvelous and O.K. SC, Perfect SC − 10, Great SC/2 − 10 (the engine's
  `DdrVariant::Sn2`, unused until now); grades by money and by the chart's
  difficulty (AAA 990k and AA 950k everywhere; A/B/C 800/700/500k for
  Beginner and Basic, 850/750/600k Difficult, 900/800/700k Expert and
  Challenge); no measured windows, no public life numbers, the fraction of
  SC rounded nobody says how, and whether an O.K. adds to the combo not
  documented.

Neither era's life gauge has public numbers (start half full, Perfect and
Great fill it slowly, Boo and Miss empty it fast, Good does nothing:
Wikipedia's DDR 1998). StepMania modeled it: `LifeMeterBar`'s defaults,
the same in 3.9 (`PrefsManager.cpp`) and 5.1 (`metrics.ini`): +0.008 W1
and W2, +0.004 W3, 0 W4, −0.040 W5, −0.080 Miss, −0.160 a mine, +0.008
Held, −0.080 LetGo, from 0.5, danger at 0.2; after a loss five gains are
swallowed (`RegenComboAfterMiss`, capped at 10 in 3.9); losses scaled by
`0.5 + 0.5 × life` (`MercifulDrain`, on in 3.9); a loss from a full bar
at least −0.10 (`HarshHotLifePenalty`, always in 3.9, on by default in
5.1's preferences).

### FLARE (DDR A3 and WORLD)

BEMANIWiki's DDR WORLD "基礎知識" (FLARE GAUGE) and its A3 page;
RemyWiki's AC DDR A3 and AC DDR WORLD. The gauge starts full, never
recovers ("a non-recovery LIFE GAUGE with penalties for steps below
Perfects"), takes a fixed share for each judgement whatever the chart's
length, and fails the play when it empties, even on the last note (a full
combo then is FAILED and earns no FLARE SKILL). A Marvelous and an O.K.
take nothing, a Perfect only on EX, an N.G. as a Miss ("MISS･NG"; a shock
stepped on is an N.G.). BEMANIWiki's estimates ("推定値") for WORLD, in
percent:

| Level | Perfect | Great | Good | Miss, N.G. | Recommended |
|---|---|---|---|---|---|
| EX | 1 | 2 | 10 | 30 | 995,000 |
| IX | – | 2 | 10 | 30 | 990,000 |
| VIII | – | 1.64 | 8.2 | 26 | 980,000 |
| VII | – | 1.28 | 6.4 | 22 | 970,000 |
| VI | – | 0.92 | 4.6 | 18 | 960,000 |
| V | – | 0.74 | 3.7 | 16 | 955,000 |
| IV | – | 0.29 | 1.45 | 11 | 930,000 |
| III | – | 0.1 | 0.75 | 4.5 | 900,000 |
| II | – | 0.1 | 0.63 | 3 | 850,000 |
| I | – | 0.1 | 0.5 | 1.5 | 800,000 |

V to EX are A3's; WORLD lowered I to IV (A3's were I 0.2/1/10, II
0.29/1.45/11, III 0.38/1.9/12, IV 0.56/2.8/14). RemyWiki's A3 page has VI's
Good as 4.5 and V's as 3.8, against BEMANIWiki's 4.6 and 3.7, which a
community formula (Good `= 1 + 0.45 × (Miss − 10)`, Great `= 0.2 + 0.09 ×
(Miss − 10)`) reproduces at every A3 level: transcription errors.
Checks the sources give: IX lives through 49 Greats ("グレ49まで耐えられる",
gamerch; note.com/strikeman), WORLD's I through 66 Misses
("FLARE Iでは66MISSまで耐えられる"). DDR draws a low FLARE gauge grey,
never in DANGER.

FLOATING FLARE (WORLD): "内部的に全レベルのフレアゲージ残量を計算しており、
現在のゲージが尽きると「その時点で生存している一番高いフレアゲージ」に移行
する" — every level's gauge is kept from the start and play moves to the
highest still alive, so a level does not start full and several can go at
once (four Misses empty EX, IX and VIII, leaving VII at 12%); the rank is
the level at the end; FLARE I emptying fails the play, never NORMAL.

FLARE SKILL (BEMANIWiki "フレアスキル"; Konami's WORLD teaser info03):
"フレアスキル=基準点*(1+フレアランク*0.06)" floored, the rank 0 under
NORMAL, LIFE4 and RISKY and 1–10 for FLARE I–EX; the base by the chart's
level 1–19 is 145, 155, 170, 185, 205, 230, 255, 290, 335, 400, 465, 510,
545, 575, 600, 620, 635, 650, 665 (agreeing with gamerch's and zk_phi's
tables: level 19 at EX 1064, IX 1024, VIII 984, VII 944; 17 at EX 1016; 18
at IX 1001). Only clears count; an assist option voids it
("アシストオプションを設定してプレーした場合、フレアスキルは獲得できません").
The player's skill sums the best 30 charts in each of three version
groups (CLASSIC, WHITE, GOLD), by single and double: across plays and
DDR's own song versions, not a property of a play.

### LIFE4

LIFE4 (life4ddr.com) is a community ranking: 13 classes (Copper to Ruby)
of five levels each, earned by goals over many plays (clear counts per
level, folder averages, sums of MFC points, trials of four songs played
as a set, calories in a day), with photographed results from the arcade.
None of that is a property of one play, and standings against LIFE4's
requirements need DDR's official songs and their ratings, which the game
does not have: left out.

What is a property of one play is its clear type, the LIFE4 tracker's
`ClearType` in order: fail, clear, LIFE4 clear, good full combo, great
full combo, perfect full combo, single digit perfects, marvelous full
combo. The rules page defines the red lamp as "Earned when you clear a
song with "Life 4" enabled, where 4 or more misses will fail you"; SDP is
derived in the app from a PFC scoring 999,910 or more (each Perfect costs
10), a PFC with 1 to 9 Perfects. The app also promotes a clear on FLARE
VIII or higher to a LIFE4 clear; the site's rules do not say so.

DDR's own LIFE4 gauge (RemyWiki's general info, Wikipedia's DDR (2013),
BEMANIWiki's A20 page): four lives, "4回MissまたはN.G.判定を取った
(=コンボを切った)時点でFAILED"; a Good keeps a life since DDR (2013), as it
keeps the combo; no life comes back. RISKY: one life, lost to a Miss.

### FFR

`docs/research/formats-ffr-danoni.md` A2 gives rCubed's windows, raw and
end-of-song score and life. Not built here: FFR has no grades (its
community flags, AAA, SDG, black flag, are not a ladder the results can
show as one), its Boo rule ignores presses before the first note and its
miss comes 200 ms late while hits end at 117 ms (the engine would need
both), and FFR charts have no holds or mines, so the preset would have
to invent their rules. It belongs with the FFR chart import (another
agent's, phase 10), as a module of its own when that lands.

## Decided here (the maintainer may change any)

1. `ddr-extreme` ("DDR EXTREME", approximate) is DDRMAX2 and EXTREME's
   normal play: the measured `GAME.DAT` windows with Marvelous off (its
   window falls to Perfect, as normal play judged it); MAX2's score
   (`Max2`, shown as the score, in the HUD too) with the feet clamped to
   1–10 as StepMania 3.9 clamps them (DDR rated 1–10); MAX's dance points
   for the grade (AAA at 100%); Great keeps the combo; judged a row at a
   time, a jump adding two to the combo (`ComboRules::jumps_add_each`); an
   N.G. neither breaks the combo nor costs points. Its life bar is
   StepMania 3.9's with `MercifulDrain`, the regen lockout to 10 and the
   harsh penalty from full (`LifeBar::hot_penalty`). Not modeled: the
   white flash of a Perfect in the Marvelous window, the score after a
   delayed fail ("P is changed to 1"; the grade is E either way), the
   long and marathon versions (no source), mines (EXTREME had none;
   StepMania's −8 dance points).
2. SuperNOVA2 and X are not built: no measured windows or gauge, grades
   that depend on the chart's difficulty (a grade table that cannot see
   the chart), and an undocumented combo rule. One legacy preset is
   enough to stand for the era; this research is here for the next.
3. DDR's gauges are an option of `ddr-a` only (A3 and WORLD's DANCE
   GAUGE row): `StepManiaOptions::gauge` (`DdrGauge`: Normal, LIFE4,
   RISKY, FLARE I–IX and EX, FLOATING FLARE), the Rules section's "DDR
   gauge" row. StepMania's "Life: Battery" replaces it (the key then
   keeps Normal); under the other presets it is kept as Normal, as the
   life level is where it does not apply.
4. LIFE4 and RISKY are batteries of four and one lives, a life lost to
   each tap worse than the worst tier that keeps the combo (a Miss under
   DDR A), each mine or shock stepped on and each freeze let go.
5. FLARE is BEMANIWiki's WORLD table, in hundredths of a percent so the
   drains add up exactly (IX fails on the 50th Great, I on the 67th
   Miss). The fail type applies as to any gauge (a FLARE play under fail
   off goes on after the gauge empties, but keeps no rank).
6. A play under `ddr-a` with a bar keeps `Results::ddr` (`DdrClear`): its
   gauge, the FLARE rank kept when its gauge never emptied (0 under
   NORMAL, LIFE4 and RISKY; FLOATING FLARE's level at the end) and its
   FLARE SKILL points, unless an option assisted or the chart's level is
   outside DDR's 1–19 (a 20 is not a 19: StepMania packs rate on other
   scales). The chart's meter reaches the score as `ScoreCtx::meter`.
   The results show "Flare skill: FLARE VII · 944 points" among the
   stats and the gauge in the mods line. The FLARE SKILL total (best 30
   by DDR's version groups) is not made: the game does not know DDR's
   versions.
7. Bests are kept with every gauge's plays (the gauge changes whether a
   play fails, not how it is judged), as for the battery.
8. LIFE4's lamps join the history's: `Lamp::Life4Clear` ("LIFE4", a
   red lamp) between Clear and FC for a clear under LIFE4 or the stricter
   RISKY, and `Lamp::SingleDigitPerfects` ("SDP") between PFC and MFC for
   a PFC with 1 to 9 Perfects, both under DDR A's rules only. The app's
   FLARE VIII promotion is not taken (not in LIFE4's rules). An assisted
   or failed play's lamp is as before.
9. `JUDGE_VERSION` is 2: a DDR A play kept before may now earn SDP and
   gains its FLARE rank and points, so kept plays are judged again.
10. Found, not changed: the `sm5` and `itg` presets' bars take no harsh
    penalty from full, which StepMania 5.1's `HarshHotLifePenalty`
    (`LifeMeterBar::ChangeLife`; the research for this plan reports it on
    by default in `PrefsManager.cpp`) would give; whether Simply Love
    keeps it is not checked. Applying it changes those presets' plays
    (another judge version), so it is left to a decision of its own.
11. `ffr` is left to the FFR import (above).

## Steps

1. Engine: `rules::ddr` (`DdrGauge`, `Flare`, `flare_points`,
   `DdrClear`), `Max2`, `presets::ddr_extreme`, `ComboRules::jumps_add_each`,
   `LifeBar::hot_penalty`, `ScoreCtx::meter`, `Results::ddr`. Done.
2. UI: the "DDR gauge" row, the ruleset list (DDR EXTREME after DDR A),
   the results' stat, lamp and mods, the history's lamps, the plays
   list's ruleset name; the theme's lamp colors. Done.
3. Browser and desktop checks. Done (below).

## Checks

- Engine (`crates/engine/tests/rulesets.rs`): EXTREME's score (90,000,000
  for a 9-foot song all Perfect, a Great at the first step 5 × 115,384
  short), dance-point grades (AAA, AA, E failed), a jump's two combo, the
  tiers by the measured windows; autoplay all Perfect and AAA; MAX2's last
  step bonus where `B / S` is not whole, an N.G. at the end, the clamp;
  the harsh penalty and merciful drain; DDR A autoplayed under every gauge
  (all Marvelous, the rank and points kept: 335 for a 9, 536 at EX); FLARE
  IX failing at the 50th Great, VIII kept through 60 (754 points for a
  12), FLOATING FLARE ending on VII after four misses; fail off keeping
  no rank; LIFE4 kept through Goods, failing at the fourth miss, RISKY at
  the first; the keys (round trip, bests, older keys, an unknown gauge,
  the mods line). Each play judged again from its key and log to the same
  results; `history.rs` also replays autoplays and random plays under
  `ddr-extreme` and DDR A's gauges. `rules::ddr` unit tests: BEMANIWiki's
  checks, FLOATING FLARE's example, the FLARE SKILL tables.
- UI: `ddr_a_shows_its_gauge_flare_and_life4_lamps` (stat, mods, lamps),
  every option row (the new row and preset), the options' fit at every
  size (the row's description).
- Desktop (`crates/desktop/tests/rulesets.rs`, no device, fake clock):
  Beatucada's Beginner autoplayed from the settings under DDR A FLARE EX
  (rank EX, 232 points, "FLARE EX · 232 points" on the results' data, the
  gauge kept in the history, MFC), LIFE4 (rank 0, a LIFE4 clear) and DDR
  EXTREME (all Perfect, 10,000,000, AAA). The HUD is unchanged (the
  gauges draw as the life bar), so no offscreen picture was added.
- Browser (2026-10-21, `target/scratch/pw/rulesets.mjs`, headless
  Chrome on WebGPU and WebGL2, Beatucada Beginner autoplayed): DDR
  EXTREME 38 Perfects, 10000000, AAA, the tiers Perfect to Miss; DDR A
  FLARE EX all Marvelous "FLARE EX · 232 points", mods "FLARE EX"; 400 ms
  late under FLARE EX fails at the fourth miss (E), as does LIFE4;
  FLOATING FLARE with 38 Greats stays on EX, with 38 Goods ends on
  "FLARE IV · 179 points" (EX, IX, VIII, VII, VI and V emptied, as the
  table says); NORMAL with 38 Perfects "145 points", PFC kept; the
  history's record keeps `stepmania.gauge` and the lamp. Headless Firefox
  (Playwright's build, WebGL2): DDR EXTREME and the FLOATING FLARE drop
  alike. Options' Rules section looked at in a screenshot (the "DDR
  gauge" row, FLARE VII). Not checked: the maintainer's Firefox and
  Safari by hand, a LIFE4 clear and SDP lamp in the browser (the
  autoplayer's bias cannot make one miss; unit tests cover them).
