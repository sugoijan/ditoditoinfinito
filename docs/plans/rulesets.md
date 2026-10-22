# More rulesets: DDR EXTREME, DDR X, DDR's gauges and FLARE, LIFE4's lamps

Status: done 2026-10-21 (decision 57); the `ffr` preset done 2026-10-22
(decision 58, "FFR" below); the `ddr-x` preset done 2026-10-22
(decision 59, "DDR X" below). Phase 10's "more rulesets"
(`docs/PLAN.md` section 5): DDR's legacy scoring as the `ddr-extreme`
preset, DDR A's other gauges (LIFE4, RISKY, FLARE I–EX, FLOATING FLARE)
with the FLARE rank and FLARE SKILL a play keeps, and LIFE4's clear types
as lamps; then FlashFlashRevolution's rules as R^3 plays them, and DDR
X's (SuperNOVA2's score, grades by the chart's difficulty). Everything
lives in the shared crates (`ddi-engine`, `ddi-ui`, `ddi-theme`), so both
shells have it with no code of their own.

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
  `DdrVariant::Sn2`, unused until 2026-10-22); grades by money and by the
  chart's difficulty (AAA 990k and AA 950k everywhere; A/B/C 800/700/500k
  for Beginner and Basic, 850/750/600k Difficult, 900/800/700k Expert and
  Challenge); no measured windows, no public life numbers, the fraction of
  SC rounded nobody says how, and whether an O.K. adds to the combo not
  documented. Researched further and built as `ddr-x` ("DDR X" below).

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
end-of-song score and life. Left out on 2026-10-21 (no grades, a Boo
rule from the first note, a miss later than the hits end, no holds or
mines); built on 2026-10-22 after reading rCubed again (`main` at
`e57179c`, the commit A2 read; AGPL-3.0, so read for its rules and
nothing transcribed):

- **Windows** (`Constant.JUDGE_WINDOW`, `judgeScorePosition`): a press's
  accuracy (press − note, ms) takes the last entry it is above: Average
  (−118, −85], Good (−85, −51], Perfect (−51, −18], Amazing (−18, 17],
  Perfect (17, 50], Good (50, 84], Good (84, 117]; above 117 nothing.
  No Average on the late side. The times are whole milliseconds
  (`getTimer`), so the early bounds being open only matters at an exact
  instant.
- **The note judged around:** `NoteBox.spawnArrow` places a note half a
  frame into its frame (`POSITION = (time + 0.5/30) × 1000`), and both
  drawing and judging use that; `GAME_FRAME` runs a frame behind the
  clock's frames (`round(time × 30 − 0.5)`).
- **Miss:** each frame, the notes on screen whose frame is six or more
  behind the game's (`GAME_FRAME − FRAME + offset ≥ 6`) are missed, in
  order: 200 ms after the frame starts, 183.3 ms (5.5 frames) after the
  instant judged around. A press on a note past 117 ms is not a hit, and
  the search goes on to the next note of the lane (`judgeScorePosition`
  breaks only on a hit or a note too early), so a late press on a jack
  can hit the second note while the first waits to be missed.
- **Boo:** a press that hits nothing is a Boo, −5 raw score, −5 life,
  0.2 raw goods, the combo kept; `commitJudge` returns on a Boo whose
  frame is before the chart's first note's. That frame is the game's,
  except that a press within three frames of a note of its lane
  (`booConflict`, only with the default judge) moves it past the note:
  a correction for the frame loop running behind the press's own time
  (a press that close always hits the note when judged at its own
  time).
- **Combo:** every hit adds one, a Miss resets it, a Boo leaves it.
- **Raw score** (`gameScore`): Amazing and Perfect 50, Good 25, Average
  5, Boo −5, Miss −10; it is what the game shows in play, what a level's
  best and rank are kept by (`ScoreHandler`'s `rawscore`, the song
  list's icons, multiplayer's score) and may go negative.
- **Total score** (`GameScoreResult.score_total`, the results' "grand
  total"): (Amazing + Perfect) × 500 + Good × 250 + Average × 50 + max
  combo × 1,000 − Miss × 300 − Boo × 15 + raw score, never below 0 (the
  playlist gives a level's best as its notes × 1,550, which agrees).
  `restartGame`'s running total uses the current combo instead; that is
  a statistic of restarts, not a play's score.
- **Raw goods** (`raw_goods`): Good 1, Average 1.8, Miss 2.4, Boo 0.2.
- **Life** (`updateHealth`): whole numbers, 50 at the start, +5 a hit,
  −5 a Miss or a Boo, never above 100; at 0 or below the game ends at
  once (an unfinished play). The bar shades from red to green with no
  danger state (`LifeBar.as`).
- **What a play comes to:** no grade. The song list shows an icon
  (`GlobalVariables.getSongIconIndex`, each that applies replacing the
  one before): passed; FC (every note hit, the combo the notes); SDG
  (the raw score within 250 of the most, fewer than 10 raw goods; no
  full combo needed); OMNIFLAG (one each of a Good, an Average, a Miss
  and a Boo); MISSFLAG (one Miss, the rest Amazing or Perfect); AVFLAG,
  BLACKFLAG (one Average, one Good, a full combo); BOOFLAG (one Boo);
  AAA (the raw score the most: every note an Amazing or a Perfect, no
  Boo). That is R^3's own order, not a ladder the community wrote.
- **Holds and mines:** FFR's charts have neither. R^3 plays StepMania
  charts too (`ChartStepmania.as`, `ChartSSC.as`): it reads `1`, `2` and
  `4` as notes (a hold's and a roll's head a note, its tail kept but not
  played) and lists mines without playing them; lifts, fakes and
  keysounds are not read at all.
- **Per-chart judge:** R^3 can take another judge window
  (`options.judgeWindow`, from the site's `configJudge`, a score then
  not standard); a level holds none, so the importer has nothing to hand
  on as a ruleset hint (PLAN.md 3.2).
- **Found for the import, changed there (2026-10-22):** R^3 judges and
  draws a note half a frame (16.7 ms) after its frame's start, and
  starts the music `mp3Frame` frames into the game (`songDelay`, the SWF
  frame the sound stream begins at, less 2). Both are the chart's timing
  under any rules (an FFR level under ITG's too), so the import places
  the notes there (`docs/plans/phase-10.md`, "Timing as R^3 has it");
  the rules here are unchanged.

### DDR X

Researched again for the preset (2026-10-22). RemyWiki is read through the Wayback
Machine (its live pages answer scripts with Cloudflare's challenge);
BEMANIWiki's version pages directly. Era: SuperNOVA2 (2008), X (2008),
X2 (2010), X3 VS 2ndMIX (2011), DDR (2013) and DDR (2014) score alike;
their judgements and combo differ, so the preset is one version's, X's.

- **Score** (RemyWiki's "DanceDanceRevolution SuperNOVA2 Scoring
  System"; BEMANIWiki's SN2 to 2014 pages): "SC x (number of Marvelous
  + number of OKs) + (SC - 10) x (number of Perfects) + [(SC ÷ 2) - 10]
  x number of Greats", SC = 1,000,000 ÷ (steps + freezes + 4- or 8-way
  shock sets), a jump one step, "A pair of freeze that starts at the same
  time will be considered as one freeze"; Good, Almost and Boo 0
  (BEMANIWiki's DDR A/基礎知識, "DDR2014(変更前)": "GREAT 基準点の50%-10点 |
  GOOD 0点"). An all-Marvelous play is 1,000,000 (BEMANIWiki X).
- **Rounding**: shown in tens in normal play ("multiples of 10 if
  playing in Standard mode", RemyWiki; "スコアは10点単位で加算(1の位は必ず0)",
  BEMANIWiki SN2). How: BEMANIWiki's X2 page drops the ones digit
  ("1の位を切り捨て"), X3, 2013 and 2014's "10点単位、端数は切り捨て"; its SN2
  page rounds half up, but says its formula was guessed from a location
  test video ("北米ロケテ動画から推測したもの"). StepMania's own SN2 scoring
  (waiei's, `Themes/_fallback/Scripts/04 Scoring.lua`, March to July
  2012, MIT) floors the exact total to ten, then takes 10 for each Perfect
  and Great. Flooring an SC to whole points a step instead cannot make
  1,000,000 of every all-Marvelous play (three steps: 999,999).
- **No EX score** in SuperNOVA2 and X (RemyWiki's "What Is DDR?": EX
  score in EXTREME's Challenge mode, "and X2 and later games, which can
  be enabled through a code"). No dance points: grades are by the score.
- **Grades** (RemyWiki's table, its "SuperNOVA2 to (2014) (Dance
  Level)" columns, the current revision and 2015's alike; BEMANIWiki's
  SN2, X2, X3, 2013 and 2014 pages, "A～Dのランクは難易度によって基準スコアが
  異なっている。AAとAAAの基準スコアは全難易度共通"): AAA from 990,000 and AA
  from 950,000 on every difficulty; A, B, C from 800,000, 700,000,
  500,000 on Beginner and Basic, 850,000, 750,000, 600,000 on Difficult,
  900,000, 800,000, 700,000 on Expert and Challenge; D below; E failed
  ("ゲージ、ライフがなくなる"). AAA asks nothing more ("99万点以上でAAAが取れる
  ようになった(GREAT以下を含んでもOK)", BEMANIWiki SN2). No source gives an
  edit's grades; DDR (2013) shows an edit as a Challenge chart
  (BEMANIWiki's "リンク・EDIT関連": "EDIT譜面の難度をBEGINNERに設定しても、
  表示が紫色（CHALLENGE譜面扱い）になる"). Courses score and grade otherwise
  (BEMANIWiki X2, X3); the game has none.
- **Judgements**: Marvelous in every mode from SuperNOVA2 ("MARVELOUS
  timing is also now on all the time in all modes", RemyWiki's AC DDR
  SuperNOVA2; "通常モードにもMARVELOUS判定が出現するようになった",
  BEMANIWiki). SuperNOVA to X name the old Boo Almost and a miss Boo
  ("BOO→ALMOST、MISS→BOOに変更", BEMANIWiki SuperNOVA); X2 removed
  Almost ("ALMOST judgement has been removed, and N.G. is merged with
  BOO, now renamed to its former name MISS", RemyWiki's AC DDR X2;
  Wikipedia says into Good).
- **Windows**: none measured for any of these versions. The 60 fps
  table, Marvelous ±1 frame (16.7 ms), Perfect ±2 (33.3), Great ±5.5
  (91.7), Good ±8.5 (141.7), "Boo" ±13.5 (225), is a post of 1 March
  2016, during DDR (2014), before A ("Only DDR has frame-based timing",
  LHoT10820, r/rhythmgames 48dfkn, archived); "the Marvelous window
  remains at +/-1 frame on DDR A, as it had been in previous games. All
  other windows have remained intact" (NewbStepper, Zenius-I-vanisher
  thread 9479, second hand); frame-tied timing "continued all the way up
  to DDR 2014" (thread 11131). Cabinets judged differently
  ("新筐体(X生産分)、赤筐体(SN生産分)、黒筐体でも判定の違いが確認されている",
  BEMANIWiki X). EXTREME's measured asymmetric table is an older game's.
- **Combo**: a jump adds one ("Jumps count as a single note for combo
  purposes once again", RemyWiki's AC DDR SuperNOVA). Great is "The
  lowest judgment that would not break the player's combo until
  DanceDanceRevolution (2013)" (RemyWiki's glossary; BEMANIWiki 2013:
  "Good判定 COMBO BREAK仕様廃止"). An N.G. breaks it from X: "Obtaining an
  N.G. from a Freeze Arrow/a set of Shock Arrows now breaks your combo"
  (RemyWiki's AC DDR X; "フリーズアローを切ってN.G.判定を出すと、コンボが切れる",
  BEMANIWiki X; in SuperNOVA2 a freeze's N.G. still kept the full combo
  mark). A shock avoided is an O.K. and adds one ("踏まなければO.K.判定と
  なり、同時にコンボが1増える", BEMANIWiki X). Whether a freeze held adds one:
  no source; current DDR's does not ("holding freeze notes to the end
  does not increment the combo", ddrguide).
- **Shocks** (X on, Challenge charts): a full row only ("全レーンにまたがって
  流れてくるため、全パネルから足を離さないとミス扱いになる", BEMANIWiki X);
  stepped on, "your gauge going down like a misstep would" (RemyWiki's AC
  DDR X), the combo broken and the arrows hidden a moment ("一瞬だけステルス
  状態になり", BEMANIWiki X); one unit of SC (Wikipedia's DDR X).
- **Life**: no numbers for any of these versions. It starts half full
  (RemyWiki's "What Is DDR?"); it drains on "timing judgments lower than
  Good (O.K. does not count)" (same), and before 2013 a Good did not lower
  it ("ゲージが減らないもののCOMBOが切れる", BEMANIWiki 2013); Great and better
  refill it. The amounts varied with the stage, the combo and the
  operator's setting ("筐体設定によりゲージの減り方が異なる", Konami's EXTREME
  Q&A as atwiki iamkenzen quotes it; the same wiki: "シミュレータでも完全再現
  を出来ているものはおそらく存在しない", and X's gauge "シリーズを通して一番減り
  づらい"). SuperNOVA2 to X showed DANGER only on RISKY, LIFE4 and
  Challenge courses ("This message would not return until A", RemyWiki's
  AC DDR SuperNOVA2). LIFE4 and RISKY were not options of X's normal
  play (RISKY from X2, LIFE4 from 2014; RemyWiki's "DDR AC General
  Info").
- **Freezes**: a moment off the panel is forgiven ("フリーズアローは一瞬だけ
  なら足を離しても大丈夫", BEMANIWiki 2013); how long, no source.

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
2. SuperNOVA2 and X were not built then: no measured windows or gauge,
   grades that depend on the chart's difficulty (a grade table that
   cannot see the chart), and an undocumented combo rule. Built since at
   the maintainer's request, the gaps taken from the closest sources
   (decision 59, "DDR X" below).
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
11. `ffr` waited for the FFR import; built since (decision 58, below).

### `ffr` (2026-10-22, decision 58)

12. `ffr` ("FFR (R^3)", not approximate: the rules are in rCubed's
    source) is R^3's default judge around the note's time: Amazing −18/+17
    ms, Perfect −51/+50, Good −85/+117, Average −118 early only
    (`presets::ffr`); a note missed 5.5 frames (183.3 ms) after it
    (`JudgeTable::miss_at`, a new field; the windows still end at 117 ms,
    and a later press goes on to the lane's next note); stray presses
    Boos from the chart's first note on (`EmptyPress::BooFromFirstNote`;
    `booConflict` not modeled, as it never applies to presses judged at
    their own time). The windows are relative to the note's time as the
    importer gives it; R^3's half-frame (above) is the importer's to add.
    A stray press counts in every lane of the layout, those without
    notes too (`Judge::with_lanes`, only under Boo rules, so the other
    presets judge as before).
13. Score: the raw score (`rules::ffr::FfrRaw`) is the score, shown in
    play and kept for bests, as FFR keeps it; shown at 0 when below
    (R^3 shows it negative, which only a play missing nearly every note
    reaches; our scores are never negative). The total score and the raw
    goods are results' stats ("Total score", "Raw goods") from
    `Results::ffr` (`FfrResult`), computed from the counts and the
    longest combo.
14. Life: `rules::ffr::FfrLife`, whole numbers as R^3 keeps them, failing
    at once at 0 (`FailPolicy::Immediate`); no danger state.
15. No grade (the grade is empty). The flag is the lamp: `Lamp` gains
    R^3's icons above FC in its order (single digit goods, omniflag,
    missflag, averageflag, blackflag, booflag, AAA), PASS being a clear
    and FC a full combo; only FFR's rules earn them, and bests are kept by
    ruleset, so they never compare with DDR's. A failed play is FAILED
    (R^3's unfinished). The theme gives each its own color.
16. Holds, rolls, mines and the rest in charts from other games: as R^3
    plays a StepMania chart, holds and rolls are taps at their heads and
    everything else is left out (mines, shocks, lifts, fakes, dummies,
    keysounds; `JudgeTable::taps_only`). Not an assist (the rules, not a
    choice of the player; bests are by ruleset anyway). The hold style is
    ignored (a key says dance holds), as are `#COMBOS` segments, a
    StepMania rule.
17. StepMania's options (fail type, life and judge levels, the battery,
    DDR's gauge) do not apply, as under Dancing☆Onigiri's rules: FFR has
    none of them (`Ruleset::is_stepmania` is false for `ffr`). Options'
    rows say so. R^3's own options (autofail thresholds, another judge
    window, the judge offset in frames) are not offered.
18. Which charts: `ffr` is in the ruleset list (after DDR EXTREME), for
    any chart. With "Rules for each chart" on "The chart's game"
    (`RulesetMode::Original`), an FFR level plays by `ffr` as a
    Dancing☆Onigiri work plays by Dancing☆Onigiri's rules: the precedent
    for a chart made for another game. The level's chart says so
    (`Chart::ffr`, set by the importer, not in the digest), and the song
    list's best key reads it from the entry
    (`ddi_library::ffr::is_level_chart`). Under the default ("The ruleset
    below") levels play by the ruleset chosen, as before.
19. Names: Amazing, Perfect, Good, Average, Miss; Boos are the results'
    "Stray presses", always shown under `ffr`; no hold or mine rows. The
    plays list says "FFR". No `JUDGE_VERSION` change: no kept play's
    results change.

### `ddr-x` (2026-10-22, decision 59)

20. `ddr-x` ("DDR X", approximate) is DDR X's normal play: the 60 fps
    frame table with Almost at ±13.5 frames (W5) and a note let pass a
    Boo (`presets::ddr_x`); judged a row at a time, a jump one step and
    one to the combo; Great keeps the combo, Good, Almost and Boo break
    it, as does an N.G. (a freeze let go, a shock stepped on, a mine of a
    row that is not full); a shock avoided adds one, a freeze held none
    (no source: DDR A's rule, the nearest version with one). Freezes
    forgive 0.25 s off the panel (StepMania's `TW_Hold`, as `ddr-a`);
    shocks are a full row of mines, judged in DDR A's 0.07 s.
21. Score: `DdrMoney` with `DdrVariant::Sn2`, the exact total floored to
    ten after the Perfects' and Greats' tens (BEMANIWiki's later pages
    and StepMania's SN2 script; the order does not matter, the tens being
    whole), shown in the HUD and kept for bests; no EX score (none in X).
22. Grades: `presets::ddr_x_grades` by the chart's difficulty (Beginner
    and Easy as Beginner and Basic, Medium as Difficult, Hard and
    Challenge as Expert and Challenge), an edit as Expert and Challenge;
    E failed. The ruleset is built for a chart (`presets::ddr_x_for`, from
    `presets::stepmania`, which `for_chart` and `RulesetKey::build` go
    through, so a replay and a play judged again grade alike); the grade
    table stays a property of the ruleset, as a chart's holds already
    were. Built for no chart (the options' list), Expert's.
23. Life: StepMania 3.9's bar, as `ddr-extreme`'s (merciful drain, the
    regen lockout to 10, the harsh penalty from full): its defaults follow
    the pattern the sources give for 1st to X ("large gain
    Perfect/Marvelous, small gain Great, 0 Good, small loss Boo, large
    loss Miss", atwiki iamkenzen) with Almost as the small loss; a shock
    stepped on and a freeze let go as a Miss (−0.08), a mine −0.16. No
    danger state (`LifeBar`'s threshold 0, its danger line none), as X
    showed none. StepMania's options apply as to the other StepMania
    presets (fail type, life and judge levels, the battery); DDR's gauge
    does not (X had no LIFE4 or RISKY to choose).
24. Lamps as the other StepMania presets' (a full combo with Greats
    GFC, X's "FULL COMBO"); LIFE4's lamps stay DDR A's. The plays list
    says "DDR X". No `JUDGE_VERSION` change: no kept play's results change
    (`DdrVariant::Sn2` and the bar's danger line were used by no preset).
25. Found, changed: the autoplayer stopped re-tapping a roll 0.2 s
    before the lane's next note, inside X's 0.225 s Almost window, so a
    re-tap hit that note early (the local corpus's rolls under `ddr-x`).
    It stops 0.23 s before (`autoplay::ROLL_TAP_STOP`) and taps once there,
    so the roll's tail stays within DDR A's and X's 0.25 s roll window.
    The script is not kept with a play (its presses are), so no kept play
    changes.
26. Not modeled: X's gauge amounts and their variation by stage and
    combo, per-cabinet judging, courses and their scores, X2 to 2014's
    rules (Almost gone, Good keeping the combo from 2013), SuperNOVA2's
    (no shocks, an N.G. keeping the combo).

## Steps

1. Engine: `rules::ddr` (`DdrGauge`, `Flare`, `flare_points`,
   `DdrClear`), `Max2`, `presets::ddr_extreme`, `ComboRules::jumps_add_each`,
   `LifeBar::hot_penalty`, `ScoreCtx::meter`, `Results::ddr`. Done.
2. UI: the "DDR gauge" row, the ruleset list (DDR EXTREME after DDR A),
   the results' stat, lamp and mods, the history's lamps, the plays
   list's ruleset name; the theme's lamp colors. Done.
3. Browser and desktop checks. Done (below).
4. `ffr` (2026-10-22): engine (`rules::ffr`, `presets::ffr`,
   `JudgeTable::miss_at` and `taps_only`, `EmptyPress::BooFromFirstNote`,
   `Results::ffr`, `Chart::ffr`, `RulesetMode::Original` for levels),
   UI (the ruleset row, the results' stats, the lamps and their colors,
   the best key, the plays list's name, the options' wording), checks.
   Done.
5. `ddr-x` (2026-10-22): engine (`presets::ddr_x`, `ddr_x_grades`,
   `ddr_x_for` through `presets::stepmania`, `DdrMoney` without EX under
   `Sn2`, `LifeBar` without a danger line at 0, the autoplayer's roll
   re-taps), UI (the plays list's name; the ruleset row and results
   follow the preset), checks. Done.

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
- `ffr` (2026-10-22). Engine (`crates/engine/tests/ffr.rs`, each play
  judged again from its key and log): every window half a millisecond
  inside and outside its bounds; a missed note at 1 s + 183.3 ms, not
  at the window's end; a press 150 ms late on a jack taking the second
  note, the first missed after it; Boos only from the first note on, in
  a lane without notes too, keeping the combo (raw 90, life 50%, SDG);
  ten Misses failing at the tenth, eleven Boos after a hit, the life
  capped at 100; autoplay AAA with the raw score 50 a note and the total
  1,550 a note; holds, rolls, mines, lifts and fakes as taps or nothing;
  the key, the StepMania options ignored, the chart's game picking `ffr`
  for a level; a `beatBox` level by its frames (a frame late still AAA,
  two a Blackflag). `rules::ffr` unit tests: the raw score, the life,
  totals and raw goods, every flag in R^3's order. `history.rs` replays
  autoplays and random plays (with holds, rolls, mines, lifts, shocks)
  under `ffr`. `local_autoplay` lists `ffr` (AAA, 50 a note).
- UI: `ffr_shows_its_totals_and_flags_and_no_grade`, the song list's
  best key for a level (`the_song_lists_best_key_is_the_plays`).
- Desktop (`crates/desktop/tests/ffr_levels.rs`): the three levels
  autoplayed under the default settings (ITG, no FFR result) and under
  "The chart's game" (FFR: AAA, 50 a note, total 1,550 a note).
- Local corpus (`local_autoplay`, 1,749 files, 11,898 charts, under ITG,
  StepMania 5, DDR A and FFR): every FFR play AAA and judged again to the
  same results; the six DDR A shortfalls (a hold let go in "The Edge"
  and "A Princess On My Horse") are the same at the commit before.
- Browser (2026-10-22, `target/scratch/pw/ffr.mjs`, headless Chrome on
  WebGPU, `ruleset: "ffr"`, Beatucada's Challenge autoplayed): on time
  276 Amazings, the score 13,800, "Total score" 427,800, "Raw goods"
  0.0, no grade, the history's lamp `aaa` and key `ffr`; 70 ms late 276
  Goods (6,900, raw goods 276.0, a full combo); 150 ms late each press
  past its note and 96 ms early on the next (4 Averages), then 7 Boos
  and 7 Misses to an empty life, failed. Not checked: Firefox and Safari
  by hand, a real FFR level's music against Flash's decode (the half
  frame and the music's start are the import's since 2026-10-22, "Found
  for the import").
- `ddr-x` (2026-10-22). Engine (`crates/engine/tests/rulesets.rs`, each
  play judged again from its key and log): 20 steps of a Challenge chart
  all Marvelous 1,000,000 AAA MFC, a jump one to the combo, no EX score;
  a Perfect and a Great 974,980 (AA); a Good, an Almost and a Boo
  850,000 (B on Challenge), the combo broken by each; failed E; three
  steps, a Great 833,320 (the remainder dropped; A on Basic); every
  threshold of every difficulty and an edit's; the chart's difficulty's
  grades through `presets::stepmania`, `for_chart` and a key's build;
  autoplay all Marvelous with freezes and jumps, a freeze held adding
  nothing; freezes let go and a shock stepped on (three N.G.s, 727,270),
  a shock avoided adding one; the bar with no danger, a Good leaving it.
  `history.rs` replays autoplays and random plays under `ddr-x`;
  `autoplay` the roll re-tap before a note under X's windows. UI: the
  results' tiers (Almost, Boo) and no danger line.
- Local corpus (`local_autoplay`, 1,749 files, 11,898 charts, 59,490
  plays under ITG, StepMania 5, DDR A, DDR X and FFR): every play perfect
  and judged again to the same results. The first run, before the
  autoplayer's change, had 104 plays short (`ddr-x`'s roll re-taps
  judged Almosts of the next note, and DDR A's six); after it none, DDR
  A's six (a hold let go in "The Edge" and "A Princess On My Horse")
  gone too. Not checked:
  the browser and desktop by hand (the preset adds no screen; its row
  and results are the shared ones), Firefox and Safari.
