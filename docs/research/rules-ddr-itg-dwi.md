# Judgement, scoring and gauge rules: DDR / StepMania / ITG (Simply Love) / DWI

Research notes for a pluggable judgement/scoring/gauge layer in a DDR-style engine. Every nontrivial number carries a source. Where sources disagree, both values are given. "SM" = StepMania 5.1 (`5_1-new` branch); "SL" = Simply Love for SM5/ITGmania; "ITG" = In The Groove 2 arcade behaviour as reproduced by SL's "ITG" game mode.

---

## 1. DDR arcade judgements and timing windows

### 1.1 Judgement names by version

| Era | Tap judgements (best → worst) | Notes |
|---|---|---|
| 1st–5thMIX, MAX, MAX2, EXTREME (normal play) | Perfect, Great, Good, Boo, Miss | Freeze O.K./N.G. from DDRMAX. Marvelous existed only in EXTREME's Nonstop/Oni (course) modes, worth 3 "dance points" vs 2 for Perfect. [RemyWiki glossary](https://remywiki.com/DDR_Glossary), [RemyWiki scoring](https://remywiki.com/DanceDanceRevolution_Scoring_System), [sullla Oni notes](https://sullla.com/DDR/ddr_oni.html) |
| SuperNOVA, SuperNOVA2, X | Marvelous (SN2+ in all modes), Perfect, Great, Good, **Almost**, **Boo** | Old "Boo" was renamed Almost and old "Miss" renamed Boo; Marvelous became the top judgement in every mode from SN2. [Wikipedia DDR](https://en.wikipedia.org/wiki/Dance_Dance_Revolution), [RemyWiki glossary](https://remywiki.com/DDR_Glossary) |
| X2 → WORLD | Marvelous, Perfect, Great, Good, Miss | "ALMOST judgement has been removed, and N.G. is merged with BOO, now renamed to its former name MISS." [RemyWiki AC DDR X2](https://remywiki.com/AC_DDR_X2) |
| Hold/shock pseudo-judgements | O.K. / N.G. | O.K. = freeze held to the end, or shock arrow avoided; N.G. = freeze dropped or shock stepped on. N.G. drains life and breaks combo from X onward. [RemyWiki glossary](https://remywiki.com/DDR_Glossary), [Wikipedia DDR](https://en.wikipedia.org/wiki/Dance_Dance_Revolution) |

Combo rule changes: Great was the lowest judgement that kept combo until DDR (2013); from 2013 onward Good also keeps combo ([RemyWiki glossary](https://remywiki.com/DDR_Glossary)). Stepping on a shock arrow is treated like a miss and briefly hides the following notes ([ddrguide glossary](https://ddrguide.com/glossary/)). Combo increments: in the MAX era a jump counted as 2 for combo (RemyWiki MAX scoring: "Double steps count as two steps ... for your combo count only"); in current DDR "Jumps and avoided shock arrows both increment the combo by one; holding freeze notes to the end does not increment the combo" ([ddrguide](https://ddrguide.com/glossary/)).

### 1.2 Timing windows

DDR before DDR A judged on a 60 fps frame grid; the community-standard table (originally from Taren's research, reproduced in several places) is:

| Judgement | Frames | ms | SM preference equivalent |
|---|---|---|---|
| Marvelous | ±1 | ±16.67 | `TimingWindowSecondsW1=0.016667` |
| Perfect | ±2 | ±33.33 | `W2=0.033333` |
| Great | ±5.5 | ±91.67 | `W3=0.091667` |
| Good | ±8.5 | ±141.67 | `W4=0.141667` |
| Boo / Almost | ±13.5 | ±225 | `W5=0.225000` |

Sources: [stepmania.com forum "DDR Timing window"](https://www.stepmania.com/forums/general-questions/show/586), [ZIv thread 9728](https://zenius-i-vanisher.com/v5.2/thread?threadid=9728), [iamkenzen wiki comparison page (DDR EXTREME column)](https://w.atwiki.jp/iamkenzen/pages/204.html).

A dissenting, measured dataset for DDR EXTREME (credited to Sesse, quoted in [ZIv thread 11131](https://zenius-i-vanisher.com/v5.2/thread?threadid=11131)) is **asymmetric** and smaller: Marvelous ±13.333 ms, Perfect ±26.667 ms, Great −86.667/+73.333 ms, Good −126.666/+113.333 ms, Boo −153.333/+180.000 ms. Treat the two tables as "sources disagree"; the asymmetry is the important design input (early and late bounds must be separately configurable).

Version notes:
- DDR A and later judge at millisecond resolution (described as 1000 Hz, independent of the screen refresh), with Marvelous reported as "about 17 ms"; the exact A/A20/A3/WORLD tables are not publicly documented ([ZIv 9728 reply](https://zenius-i-vanisher.com/v5.2/thread?threadid=9728), [ZIv 11131](https://zenius-i-vanisher.com/v5.2/thread?threadid=11131), [iamkenzen 判定](https://w.atwiki.jp/iamkenzen/pages/300.html), [yuisin DDR WORLD memo](https://yuisin.com/ddr/ddrac/world_hantei.html)). ddrguide quotes the same ≈16.666/33.333/92/142 ms figures as "believed" values ([ddrguide](https://ddrguide.com/glossary/)).
- Console (PS2) DDR widened Perfect from 2 to 3 frames while keeping Marvelous at 1 frame ([ZIv thread 7644](https://zenius-i-vanisher.com/v5.2/viewthread.php?threadid=7644&page=1)).
- Konami's official how-to pages describe judgements only qualitatively ([DDR A howto](https://p.eagate.573.jp/game/ddr/ddra/p/howto/), [DDR A20 howto](https://p.eagate.573.jp/game/ddr/ddra20/p/howto/index.html)).
- Scrapbox's cross-game list also quotes DDR Marvelous ±16.7 ms / Perfect ±33 ms and ITG FA+ blue ±15 ms / white ±21.5 ms, with the caveat that there is little official information ([scrapbox Timing Window](https://scrapbox.io/0b5vr/Timing_Window)).

---

## 2. DDR scoring systems by version

All from [RemyWiki DDR Scoring System](https://remywiki.com/DanceDanceRevolution_Scoring_System) unless noted. N = step count (a jump = 1 step), S = N(N+1)/2.

| Version | Per-step formula | Max / grade rule |
|---|---|---|
| 1stMIX / 2ndMIX | M = floor(combo/4). Perfect = M·M·300, Great = M·M·100, Good = M·100 (and Good ends the combo). | Pure combo-driven, unbounded. |
| 3rdMIX (+PLUS) | P = int(1,000,000 / S); step number S_i: Perfect = P·10·S_i, Great = P·5·S_i, Good/Boo/Miss = 0. Final-step bonus = 10·[1,000,000 − P·S], full for Perfect, half for Great. | Exactly 10,000,000 for all-Perfect. If failed but song continues, Great=5 and Perfect=10 flat. |
| 4thMIX (+PLUS) | Perfect 777, Great 555, others 0 and break combo; plus combo bonus 333·(current combo) on every step. | Grade by dance points (Perfect 2, Great 1, Good 0, Boo −4, Miss −5): AA = all Perfect, A = all Perfect/Great, B ≥ 64 % DP, C below, D = fail. |
| 5thMIX | B = 500,000·(feet+1) (long versions double the feet). StepScore = p·int(B/S)·n with p = 10 Perfect / 5 Great / 0 otherwise; final-step bonus 10·(B − int(B/S)·N) if Perfect. Combo bonus Q·C per step (Q = 55 Perfect, 33 Great). | Dance-level bonus at results: AAA 10,000,000, AA 1,000,000, A 100,000, B 10,000, C 1,000, D 100. |
| DDRMAX (6th) | Same step formula with fixed B = 5,000,000 (all-Perfect = 50,000,000); Groove-Radar bonuses after the song (Stream up to 20M; Voltage, Air, Chaos, Freeze up to 10M each, scaled by the chart's radar value). | Dance points: Perfect 2, Great 1, Good 0, Boo −4, Miss −8, O.K. +6, N.G. 0. Grades: AAA 100 %, AA ≥ 93 %, A ≥ 80 %, B ≥ 65 %, C ≥ 45 %, D < 45 %, E = 0 DP without failing ([Wikipedia DDRMAX](https://en.wikipedia.org/wiki/DDRMAX_Dance_Dance_Revolution_6thMix)). |
| DDRMAX2 (7th) | Same as MAX with B = 1,000,000 × feet. | Grades as MAX. |
| EXTREME | Regular play identical to MAX2. Oni/Nonstop introduce Marvelous: Marvelous 3, Perfect 2, Great 1, O.K. 3, others 0; Nonstop course total 100,000,000 (10M+20M+30M+40M). | — |
| SuperNOVA2 → DDR (2014) | SC = 1,000,000 / (steps + freezes + 4-/8-way shock arrows). Score = SC·(Marvelous + O.K.) + (SC−10)·Perfect + (SC/2 − 10)·Great; Good = 0. Displayed in multiples of 10 in normal play, 1 in course mode (except A20). [RemyWiki SN2 scoring](https://remywiki.com/DanceDanceRevolution_SuperNOVA2_Scoring_System) | Max 1,000,000. Grades no longer require FC. |
| DDR A → WORLD | Same SC; Great = (SC·3/5 − 10), Good = (SC·1/5 − 10). Equivalent formulation: points Marvelous 5 / Perfect 5 / Great 3 / Good 1 / O.K. 5 / Miss 0, percentage × 1,000,000, then −10 per non-Marvelous judgement, floored to the nearest 10 in standard play ([sil.fyi](https://sil.fyi/games/ddr-scores-explained/), [RemyWiki SN2 page](https://remywiki.com/DanceDanceRevolution_SuperNOVA2_Scoring_System)). | Max 1,000,000 = MFC. |

Note the two formulations differ in rounding: RemyWiki keeps SC fractional, sil.fyi computes a percentage and rounds at the end. For an engine, implement "ideal score = 1,000,000 × weighted%" then apply the −10 penalty and the display rounding; that reproduces the arcade in practice.

### 2.1 Grades (DDR A onward)

From the table on [RemyWiki SN2 scoring](https://remywiki.com/DanceDanceRevolution_SuperNOVA2_Scoring_System) (+/− grades added in DDR A): AAA 990,000–1,000,000; AA+ 950,000–989,990; AA 900,000–949,990; AA− 890,000–899,990; A+ 850,000–889,990; A 800,000–849,990; A− 790,000–799,990; B+ 750,000–789,990; B 700,000–749,990; B− 690,000–699,990; C+ 650,000–689,990; C 600,000–649,990; C− 590,000–599,990; D+ 550,000–589,990; D 500,000–549,990 and 0–499,990 (RemyWiki lists two D rows; functionally D < 550,000); E = failed. ddrguide confirms "AAA: Clear with a score of 990,000 or higher" and E for fails ([ddrguide](https://ddrguide.com/glossary/)). The user's threshold list is therefore verified.

### 2.2 EX Score

Marvelous and O.K. = 3, Perfect = 2, Great = 1, Good and lower = 0. Debuted in EXTREME's Challenge mode, absent SuperNOVA–X, back in X2 via operator code (3750 / 0573), standard since DDR A. [RemyWiki EX Score](https://remywiki.com/EX_Score). Max EX = 3 × (taps + freezes + shocks).

### 2.3 Full-combo tiers

MFC = only Marvelous and no N.G.; PFC = Marvelous/Perfect; GFC = up to Great; GoodFC/FC = up to Good (Good does not break combo since 2013). [RemyWiki glossary](https://remywiki.com/DDR_Glossary), [ddrguide](https://ddrguide.com/glossary/). Lamp colours in-game: FC blue, GFC green, PFC gold, MFC white ([mzhang.io](https://mzhang.io/posts/2024-05-02-ddr/)). Using CUT, FREEZE ARROW or JUMP options produces an "assist clear" lamp ([ddrguide](https://ddrguide.com/glossary/), [RemyWiki general info](https://remywiki.com/DDR_AC_General_Info)).

---

## 3. DDR life ("dance") gauges

### 3.1 Normal gauge

- Starts half full; Perfect/Great fill slowly, Boo/Miss deplete quickly, Good is neutral ([Wikipedia DDR 1998](https://en.wikipedia.org/wiki/Dance_Dance_Revolution_(1998_video_game)), [ddrguide](https://ddrguide.com/glossary/)). Failing happens when it reaches 0 and the stage ends with the score frozen; in Premium "all stages guaranteed" play the song continues after depletion ([RemyWiki general info](https://remywiki.com/DDR_AC_General_Info)).
- Boo vs Miss: both drain, Miss roughly twice as hard (MAX dance points −4 vs −8 are the grading analogue; the Japanese wiki describes Boo as a small decrease and Miss as a large one) ([Wikipedia DDRMAX](https://en.wikipedia.org/wiki/DDRMAX_Dance_Dance_Revolution_6thMix), [iamkenzen ゲージ](https://w.atwiki.jp/iamkenzen/pages/219.html)).

Measured/claimed values for modern DDR (the sources disagree; none is official):

| Source | Perfect+ | Great | Good | Miss | Other |
|---|---|---|---|---|---|
| [iamkenzen ゲージ補正](https://w.atwiki.jp/iamkenzen/pages/179.html) (A20/A20 PLUS) | +0.4 % (+0.5 % in DANGER) | +0.2 % (+0.25 % in DANGER) | 0 | −4.8 % first, −3.6 % each consecutive miss; halved in DANGER | Independent of chart position and max combo |
| [replystudio A20 measurement](https://www.replystudio.net/2019/05/ddr-a20.html) | ≈ +0.92 % | — | — | −10 % | Dan (GRADE) gauge: Perfect ≈ +0.42 %, Miss ≈ −2.86 % |
| [sha10n blog (A20, informal)](https://sha10n.hatenablog.com/entry/ar1768120) | 42 Perfects from start refill 13/26 bars (≈ 1.2 %/Perfect) | — | no visible change | ≈1.7 bars at 100 %, ≈0.7 bars at 50 %; consecutive misses hurt more | 26-bar display, starts 13/26; DANGER at 7/26 (30 %) halves losses; GRADE gauge starts full |

The same wiki notes that in 1st–X (except 4thMIX and SOLO 2000) the pattern was "large gain Perfect/Marvelous, small gain Great, 0 Good, small loss Boo, large loss Miss", and that console versions scale the change by timing deviation and combo ([iamkenzen ゲージ](https://w.atwiki.jp/iamkenzen/pages/219.html)). Whether older arcade versions scale gains with step count is not documented in any source found; modern A20 claims are fixed percentages. An engine should therefore support both fixed-percent and step-count-scaled deltas.

### 3.2 Alternative gauges

| Gauge | Rule | Source |
|---|---|---|
| LIFE4 | 4 lives; a life is lost per combo break: Good-and-below from SuperNOVA to X3 (when Good broke combo), Miss from 2013 onward; N.G. counts from X onward. Battery display (3-segment battery in MAX2/EXTREME Challenge). Forced on EXTRA STAGE since SN2 (replacing the old pressure gauge); selectable in normal play since X2 (as "RISKY ON/OFF"), named LIFE4 since 2014. Lives cannot be regained within a song. | [RemyWiki general info](https://remywiki.com/DDR_AC_General_Info), [ddrguide](https://ddrguide.com/glossary/), [Konami option list](https://p.eagate.573.jp/game/ddr/ddra/p/howto/option_list.html) |
| LIFE8 / course battery | Courses: 4 or 8 lives depending on difficulty; lose one per combo break; clearing a song regenerates 1–2 lives. Oni in EXTREME: "any step that does not result in a Perfect or Great decreases one of them (freeze note OKs count too)". | [RemyWiki general info](https://remywiki.com/DDR_AC_General_Info), [sullla](https://sullla.com/DDR/ddr_oni.html) |
| RISKY | One Good (X2/X3 only), Miss or N.G. fails the song immediately. | [RemyWiki general info](https://remywiki.com/DDR_AC_General_Info) |
| Extra-stage "pressure" gauge (MAX–SN) | Starts full, can only go down (plus forced 1.5x & Reverse in MAX); One More Extra used Sudden Death (any combo break ends the game). | [Wikipedia DDRMAX](https://en.wikipedia.org/wiki/DDRMAX_Dance_Dance_Revolution_6thMix) |
| GRADE / DAN gauge | Like NORMAL but starts full with smaller increments and decrements; LIFE4/RISKY unavailable in Dan courses. | [RemyWiki general info](https://remywiki.com/DDR_AC_General_Info), [yuisin A20 Dan memo](https://yuisin.com/ddr/a20/drill_spec.html) |
| FLARE I–EX (A3+) | Non-recovering; decreases on Great/Good/Miss/N.G.; FLARE EX also on Perfect. FLOATING FLARE (WORLD) starts at EX and drops a level each time it empties. WORLD lowered depletion for I–IV (recommended scores I 800k, II 850k, III 900k, IV 930k). | [RemyWiki general info](https://remywiki.com/DDR_AC_General_Info), [RemyWiki AC DDR WORLD](https://remywiki.com/AC_DDR_WORLD) |
| Battle gauge | Tug-of-war bar between players (BATTLE mode). | [RemyWiki general info](https://remywiki.com/DDR_AC_General_Info) |

---

## 4. DDR gameplay options relevant to the engine

Primary sources: [RemyWiki DDR AC General Info](https://remywiki.com/DDR_AC_General_Info) and Konami's [DDR A option list](https://p.eagate.573.jp/game/ddr/ddra/p/howto/option_list.html).

| Category | Values | Engine impact |
|---|---|---|
| SPEED / SCROLL SPEED | x0.25–x8, 24 values through A3 (x0.25/x0.5 removed X2–2013, back in 2014; 9 values Premium-only); WORLD: HI-SPEED in 0.05 steps plus "Real Speed" (target BPM in 10-BPM steps). Speed can be changed with left/right before the first note (X2+). | Beat-based scroll multiplier; optional constant-BPM mode. |
| SCROLL ACTION (was BOOST / ARROW MOVE) | NORMAL, BOOST (accelerate near step zone), BRAKE (decelerate), WAVE (slow/fast at fixed points). | Non-linear position function of (beat distance). |
| ARROW VISIBILITY / APPEARANCE | NORMAL, CONSTANT (fade-in 100–3000 ms, default 1000), STEALTH, HIDDEN, SUDDEN; HIDDEN+, SUDDEN+, HIDSUD+ lane covers adjustable in play (X2+). Old HIDDEN/SUDDEN removed as of A. | Per-note alpha function; lane cover geometry. |
| ARROW PLACEMENT / TURN | OFF, MIRROR (180°), LEFT, RIGHT (90°; not in Double), SHUFFLE (column permutation). | Column remap before judging. |
| STEP ZONE (was DARK) | ON/OFF receptor visibility. | Render only. |
| SCROLL | NORMAL / REVERSE. | Render direction (receptor at top vs bottom). |
| ARROW COLORING | NOTE (red 1/4, blue 1/8, yellow 1/16, green everything else), VIVID (rainbow cycle, phase offset by note value), FLAT (same cycle, same phase), RAINBOW (progress-based gradients: pink 1/64–1/16, blue 5/64–1/8, purple 9/64–3/16, orange 13/64–1/4 within the beat). Defaults: FLAT (1st/2nd), VIVID (3rd–2013), RAINBOW (2014–A3), NOTE (WORLD). | Skin needs beat fraction and quantization per note. |
| TIMING CUT (was LITTLE/CUT) | OFF, ON1 (4ths only), ON2 (4ths+8ths). | Chart transform; counts as assist. |
| FREEZE ARROW CUT | ON/OFF (OFF keeps only the head tap). | Chart transform; assist. |
| JUMP CUT | ON/OFF (removes simultaneous notes). | Chart transform; assist. |
| DISPLAY TIMING | −100…+100 ms visual offset (A3 added a −5.0…+5.0 step-0.1 variant). | Visual-only offset. |
| JUDGMENT TIMING | −100…+100 ms shift of the judging window relative to audio. | Judge offset. |
| DANCE GAUGE | NORMAL, LIFE4, RISKY, FLARE I–EX, FLOATING FLARE, GRADE. | Gauge plugin selection. |
| ARROW DESIGN | NORMAL, X, CLASSIC, CYBER, MEDIUM, SMALL, DOT. | Skin. |
| SCREEN FILTER | OFF, DARK 40 %, DARKER 60 %, DARKEST 80 % (WORLD: 0–100 % in 10 % steps). | Render. |
| GUIDELINE | CENTER, BORDER, OFF. | Render. |
| FAST/SLOW DISPLAY | ON/OFF; shown for Perfect/Great/Good (anything but Marvelous/Miss since X2). WORLD results also show Fast/Slow counts, timing average and variation. | Judge must expose signed offset. |
| Judgement display priority | Judgement-first or arrow-first layering (Konami e-amusement setting). | Render. |

Hands (three panels at once) and quads (all four) are chart-pattern terms, not options; DDR charts contain jumps ("two different notes on the same horizontal row") and shock arrows; a jump is one scoring step and one combo increment ([ddrguide](https://ddrguide.com/glossary/), [RemyWiki SN2 scoring](https://remywiki.com/DanceDanceRevolution_SuperNOVA2_Scoring_System)). Shock arrows (X+) are 4-/8-way rows that must not be stepped on; stepping = N.G./miss and briefly hides upcoming notes; avoiding = O.K. (+1 combo, full step score, 3 EX).


### 4.1 How StepMania implements these options

The engine follows StepMania 5.1 (`5_1-new` at [`825467b`](https://github.com/stepmania/stepmania/tree/825467bcd81c812b33ad684dc04dd151b2d5dec3)) where it defines the maths. Pixel values assume SM's 480-pixel-high screen and 64-pixel arrow spacing (`ArrowSpacing=64` in [`Themes/_fallback/metrics.ini`](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/Themes/_fallback/metrics.ini) `[ArrowEffects]`).

- **Order of transforms.** `NoteDataUtil::TransformNoteData` applies removals first (Little, NoRolls, NoHolds, NoMines, NoJumps, …) and turns last, "so that they affect inserts" ([`src/NoteDataUtil.cpp`](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/src/NoteDataUtil.cpp) 3028–3085).
- **Turn tables** (`GetTrackMapping`, same file, 1370–1998), written as "new lane `t` takes from old lane `iTakeFromTrack[t]`": Mirror is `NumTracks − t − 1` for every steps type; Left is `[2,0,3,1]` for dance-single (`[2,0,3,1,6,4,7,5]` double, `[5,4,0,3,1,2]` solo), so an up arrow becomes a left arrow; Right is the inverse of Left. Shuffle is `std::shuffle` seeded with the stage seed, re-rolled with seed + 1 while the result is the identity.
- **Little** removes every note on a row that is not a whole beat (`i % ROWS_PER_BEAT != 0`), whatever its type (taps, holds, mines). DDR's TIMING CUT ON2 keeps eighths as well; SM has no equivalent, so the same rule at half-beat rows is the natural extension.
- **NoHolds** (`RemoveHoldNotes`) turns hold heads of subtype Hold into taps; rolls are first turned into holds by **NoRolls** (`ChangeRollsToHolds`). DDR has no rolls, so a "no holds" cut that also covers rolls matches both.
- **NoJumps** is `RemoveSimultaneousNotes(1)`: on each row it counts taps and hold heads plus the lanes held by an earlier hold (`GetTracksHeldAtRow`, which scans upward from the row exclusive, so a hold ending on the row still counts) and blanks taps and heads from the leftmost lane until at most one remains. Lifts count towards the row (`GetNumTracksWithTapOrHoldHead`, [`src/NoteData.cpp`](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/src/NoteData.cpp#L263-L273)) but are never removed; mines and fakes are neither counted nor removed.
- **Boost / Brake / Wave** (`ArrowEffects::GetYOffset`, [`src/ArrowEffects.cpp`](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/src/ArrowEffects.cpp) 464–608). With `y` the distance in pixels **before** the scroll-speed multiplier (after `#SPEEDS`) and `H` the note-field height (`SCREEN_HEIGHT` = 480): Boost adds `clamp(y·1.5 / ((y + H/1.2)/H) − y, ±400)`, Brake adds `clamp(y·(y/H) − y, ±400)`, Wave adds `20·sin(y/38)` (`WaveModMagnitude`, `WaveModHeight`). Nothing is applied once `y < 0` ("don't mess with the arrows after they've crossed 0"), and every adjustment is 0 at `y = 0`, so arrival times are unchanged.
- **Hidden / Sudden / Stealth** (`ArrowGetPercentVisible`, `GetAlpha`, `GetGlow`, same file, 1018–1161). `CENTER_LINE_Y` = 160 px and `FADE_DIST_Y` = 40 px from the receptor, in drawn distance. Hidden fades from 160 to 120 px, Sudden from 200 to 160 px; with both on the lines move 10 px further apart each way. A note is drawn only when more than half visible (alpha is 0 or 1) and glows white (`SCALE(|p − 0.5|, 0, 0.5, 1.3, 0)`) around the switch. Stealth subtracts full visibility. Notes past the receptor are fully visible (`m_bStealthPastReceptors` defaults to off).

Our field is 10 arrows tall in landscape against SM's 7.5, so the Hidden/Sudden lines are scaled by 10/7.5 to keep them at roughly the same place on screen (one SM pixel = 1/48 arrow; SM's receptors sit 96 px, 20 % of the screen, from the top, ours 15 %, so the lines land a few percent higher). Boost and Brake keep SM's units because they act before the speed multiplier, on beats rather than on the screen.

---

## 5. StepMania 5 defaults and ITG / Simply Love

### 5.1 Timing windows

Defaults are in `Player.cpp` (`TimingWindowSecondsInit`), with `GetWindowSeconds(tw) = seconds[tw] * TimingWindowScale + TimingWindowAdd`; defaults `TimingWindowScale=1.0`, `TimingWindowAdd=0`, `TimingWindowJump=0.25` ([Player.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/Player.cpp)).

| Window | SM5 default (s) | SL "ITG" mode (s) | SL "FA+" mode (s) | SL "Casual" (s) |
|---|---|---|---|---|
| W1 | 0.0225 | 0.0215 | 0.0135 | 0.0215 |
| W2 | 0.045 | 0.043 | 0.0215 | 0.043 |
| W3 | 0.090 | 0.102 | 0.043 | 0.102 |
| W4 | 0.135 | 0.135 | 0.102 | 0.102 |
| W5 | 0.180 | 0.180 | 0.135 | 0.102 |
| Mine | 0.090 ("same as great") | 0.070 | 0.070 | 0.070 |
| Hold | 0.250 ("allow enough time to take foot off and put back on") | 0.320 | 0.320 | 0.320 |
| Roll | 0.500 | 0.350 | 0.350 | 0.350 |
| Attack | 0.135 | — | — | — |
| Checkpoint (pump) | 0.1664 | — | — | — |
| TimingWindowAdd | 0 | 0.0015 | 0.0015 | 0.0015 |

SL values: [SL_Init.lua](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Scripts/SL_Init.lua). Because of `TimingWindowAdd=0.0015`, SL/ITG effective windows are 23 / 44.5 / 103.5 / 136.5 / 181.5 ms, while the community quotes the raw 21.5 / 43 / 102 / 135 / 180 ms ([ZIv 9728](https://zenius-i-vanisher.com/v5.2/thread?threadid=9728), [iamkenzen comparison](https://w.atwiki.jp/iamkenzen/pages/204.html)); FA+ blue Fantastic = 13.5 + 1.5 = 15 ms, with a 10 ms variant used by some communities ([scrapbox](https://scrapbox.io/0b5vr/Timing_Window)). In ITG mode SL emulates a "W0" (Fantastic+) by re-testing W1 hits against the FA+ W1 window (`IsW0Judgment`, [SL-Helpers.lua](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Scripts/SL-Helpers.lua)).

The SM options-menu "TimingWindowScale" row maps choices 1–9 to {1.50, 1.33, 1.16, 1.00, 0.84, 0.66, 0.50, 0.33, 0.20} (choice 4 = default) and "LifeDifficulty" 1–7 to {1.60, 1.40, 1.20, 1.00, 0.80, 0.60, 0.40} ([ScreenOptionsMasterPrefs.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/ScreenOptionsMasterPrefs.cpp)).

Labels: `_fallback` W1–W5 = Flawless, Perfect, Great, Good, Boo; Held = OK, LetGo = NG; the `default` theme uses "Bad" for W5 ([fallback en.ini](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/Themes/_fallback/Languages/en.ini), [default en.ini](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/Themes/default/Languages/en.ini)). SL: Fantastic, Excellent, Great, Decent, Way Off, Miss; FA+ mode labels W1 and W2 both "Fantastic" (blue/white) ([SL en.ini](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Languages/en.ini)).

### 5.2 Dance points, percent score, grades

Percent = actual DP / possible DP, where possible DP = taps × weight(W1) + (holds + rolls) × weight(Held) (+ checkpoint ticks for pump) ([ScoreKeeperNormal.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/ScoreKeeperNormal.cpp), [PlayerStageStats.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/PlayerStageStats.cpp)). SM keeps two weight sets: `PercentScoreWeight*` (displayed %) and `GradeWeight*` (used by `GetGrade`).

| Event | SM5 PercentScoreWeight | SM5 GradeWeight | SL ITG (percent = grade) | SL FA+ | SL Casual |
|---|---|---|---|---|---|
| W1 | 3 | 2 | 5 | 5 | 3 |
| W2 | 2 | 2 | 4 | 5 | 2 |
| W3 | 1 | 1 | 2 | 4 | 1 |
| W4 | 0 | 0 | 0 | 2 | 0 |
| W5 | 0 | −4 | −6 | 0 | 0 |
| Miss | 0 | −8 | −12 | −12 | 0 |
| Held | 3 | 6 | 5 | 5 | 3 |
| LetGo | 0 | 0 | 0 | 0 | 0 |
| HitMine | −2 | −8 | −6 | −6 | −1 |
| CheckpointHit / Miss | 3 / 0 | 2 / −8 | 0 | 0 | 0 |

Sources: [_fallback metrics.ini](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/Themes/_fallback/metrics.ini), [SL_Init.lua](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Scripts/SL_Init.lua). Note the SM GradeWeights (2/2/1/0/−4/−8, O.K. +6) are literally DDRMAX's dance points, and the SM grade tiers are DDRMAX's too: Tier01 = 1.00 (all W1), Tier02 = 1.00 with `GradeTier02IsAllW2s=true` (AAA), Tier03 0.93 (AA), Tier04 0.80 (A), Tier05 0.65 (B), Tier06 0.45 (C), Tier07 D; `NumGradeTiersUsed=7`.

SL / ITG grade tiers on the percent score ([SL metrics.ini](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/metrics.ini)): ★★★★ 1.00, ★★★ 0.99, ★★ 0.98, ★ 0.96, S+ 0.94, S 0.92, S− 0.89, A+ 0.86, A 0.83, A− 0.80, B+ 0.76, B 0.72, B− 0.68, C+ 0.64, C 0.60, C− 0.55, D otherwise. (Verified against the list in the brief.)

SL EX score ([SL_Init.lua](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Scripts/SL_Init.lua), [SL-Helpers.lua `CalculateExScore`](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Scripts/SL-Helpers.lua)): W0 3.5, W1 3, W2 2, W3 1, W4 0, W5 0, Miss 0, Held 1, LetGo 0, HitMine −1; EX% = floor(points / (steps × 3.5 + (holds + rolls) × 1) × 10000) / 100, clamped at 0; mines still count when "no mines" is on.

ITG quirk: early Decent/Way Off hits could be rescored by a later closer note; SL exposes `RescoreEarlyHits` and `MinTNSToScoreNotes=W3` to emulate or disable this ([SL en.ini](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Languages/en.ini), [SL_Init.lua](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Scripts/SL_Init.lua)).

### 5.3 Combo rules

`MinScoreToContinueCombo` = W3 for dance (W2 in Oni play mode), `MinScoreToMaintainCombo` = W3; `ComboIsPerRow` false for dance (true for pump/Oni), `MissComboIsPerRow` true; `RollBodyIncrementsCombo=false`; `ComboBreakOnImmediateHoldLetGo=false` ([_fallback metrics.ini](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/Themes/_fallback/metrics.ini), [03 Gameplay.lua](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/Themes/_fallback/Scripts/03%20Gameplay.lua)). So in SM dance, a Good (W4) breaks combo and each note in a jump adds 1 to combo, unlike modern DDR.

### 5.4 Life bar, battery, fail types

| Event | SM5 `[LifeMeterBar]` | SL ITG | SL Casual |
|---|---|---|---|
| W1 / W2 / W3 / W4 | +0.008 / +0.008 / +0.004 / 0 | same | 0 |
| W5 | −0.040 | −0.050 | 0 |
| Miss | −0.080 | −0.100 | 0 |
| Held / LetGo | +0.008 / −0.080 (pump: 0 / 0) | +0.008 / −0.080 | 0 |
| HitMine | −0.160 | −0.050 | 0 |
| CheckpointHit / Miss | +0.008 / −0.080 | — | — |
| InitialValue / DangerThreshold / HotValue | 0.5 / 0.2 / 1.0 | 0.5 / 0.2 | — |

Sources: [_fallback metrics.ini](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/Themes/_fallback/metrics.ini), [SL_Init.lua](https://raw.githubusercontent.com/Simply-Love/Simply-Love-SM5/master/Scripts/SL_Init.lua). The brief's guessed ITG values (W5 −0.050, Miss −0.100, LetGo −0.080, mine −0.050) are SL's ITG mode; the SM engine defaults are the softer 5.1 numbers.

Modifiers applied in `LifeMeterBar::ChangeLife` ([LifeMeterBar.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/LifeMeterBar.cpp)): positive deltas are multiplied by LifeDifficulty and negative deltas divided by it (so "harder" = smaller scale); `MercifulDrain` scales losses by 0.5–1.0 with current life; progressive lifebar multiplies losses by 1 + (ProgressiveLifebar/8) × consecutive-miss count; after a loss, `RegenComboAfterMiss` (default 5, SL 5) positive judgements must occur before life regenerates, capped by `MaxRegenComboAfterMiss` (SM 5, SL 10) ([PrefsManager.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/PrefsManager.cpp)); `HarshHotLifePenalty` forces at least −0.10 when the bar is full. `ForceLifeDifficultyOnExtraStage=true`, `ExtraStageLifeDifficulty=1.0`.

Life types and drains ([PlayerOptions.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/PlayerOptions.cpp), [GameConstantsAndTypes.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/GameConstantsAndTypes.cpp)): LifeType Bar/Battery/Time; DrainType Normal / NoRecover ("power-drop", starts full, never gains) / SuddenDeath ("death", starts full; any score below `MinStayAlive` (W3) = −1.0). FailType: Immediate (arcade), ImmediateContinue, EndOfSong, Off. Preferences `FailOffInBeginner` and `FailOffForFirstStageEasy` default false.

Battery ([LifeMeterBattery.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/LifeMeterBattery.cpp), metrics): default 4 lives (`BatteryLives`), `MinScoreToKeepLife=W3` (W4, W5, Miss cost a life), `SubtractLives=1`, `MinesSubtractLives=1`, `LetGoSubtractLives=1`, `HeldAddLives=0`, `MaxLives=0` (uncapped), course entries may add lives; fail when lives == 0.

### 5.5 Hold / roll / mine / lift semantics in SM

From [Player.cpp](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/Player.cpp) and `[Player]` metrics: a hold has `InitialHoldLife=1` (pump 0.05), `MaxHoldLife=1`; while the head has been hit and the button is down, life is reset to max; while released, life decreases by `dt / TimingWindowSecondsHold` (0.25 s in SM, 0.32 s in SL) and reaching 0 yields `HNS_LetGo` (NG); the tail passing with life > 0 yields `HNS_Held` (OK). Rolls decay over `TimingWindowSecondsRoll` and every tap resets life, so they must be re-tapped. `RequireStepOnHoldHeads=true` for dance (the head is a normal tap judgement). `ImmediateHoldLetGo=true` for dance (NG is awarded as soon as life hits 0), `ComboBreakOnImmediateHoldLetGo=false`. Lifts are judged on release (`(pTN->type == TapNoteType_Lift) == bRelease`); mines are hit if a press occurs within `TimingWindowSecondsMine`; `TapNoteType_Fake` is never scored ([NoteTypes.h](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/NoteTypes.h)). Misses are assigned when a note's time passes beyond the W5 window without a press (`UpdateTapNotesMissedOlderThan`).

---

## 6. Dance With Intensity (DWI)

Sources: DWI 2.x README ([dwi.ddruk.com/readme.php](http://dwi.ddruk.com/readme.php)), FAQ ([dwi.ddruk.com/faq.php](http://dwi.ddruk.com/faq.php)), the ddrpcgamer feature page ([ddrpcgamer](https://www.ddrpcgamer.com/Dance-With-Intensity-en.htm)), and SM's DWI format docs ([stepmania Docs/SimfileFormats/DWI](https://github.com/stepmania/stepmania/tree/master/Docs/SimfileFormats/DWI)).

- Judgements: five levels "miss, boo, good, great!, perfect!!" (no Marvelous); combo is kept at Great or better; grades E, D, C, B, A, AA, AAA. Freeze arrows award "OK".
- Scoring/gauge: DWI 2.0 adopted "MAX2 scoring and grading system", "5th/MAX-style power bars", Groove Radar, and a "MAX-like combo system" (1.65.5); earlier it emulated 3rd/4th-mix-style play. A changelog entry fixing "Rounding of scores to proper multiples of 10000000" confirms the MAX-style 10M-based totals. Judgement timing windows are user-tunable ("Ability to tweak timings for judging Perfect, Great, etc. steps in System Options", v1.65).
- Failure: power gauge with "warning"/"danger" announcer ranges; a "GAME OVER" option can be set to "End of Song" (fail at end) instead of immediate. Nonstop courses use lives: `#LIVES:4;`, lives regained per cleared stage by foot rating (≤6 feet: 1, 7–8: 2, ≥9: 3) or `AWARDx`; "Combo" course mode loses a life on anything below Great; `#COMBO:PERFECT;` (combo only on Perfect) and `#COMBOMODE:1|2;` (jumps add 1 or 2 to combo).
- Modifiers: Speed 0.5x, 0.75x, 1.5x, 2x, 3x, 4x, 5x, 8x; BOOST, RBOOST, WAVE; HIDDEN, SUDDEN, STEALTH; LEFT, MIRROR, RIGHT, SHUFFLE; LITTLE, FLAT, SOLO; DARK (hides receptors); REVERSE; NOFREEZE; POWER-DROP ("gauge starts full and depletes") and DEATH ("must full-combo song"). Assist click (F12), song-rate F3/F4.
- Modes/format: styles SINGLE, DOUBLE, COUPLE (two players, different charts; "Battle" use), SOLO (6-panel with up-left/up-right); difficulties BASIC, ANOTHER, MANIAC, SMANIAC (displayed as Light/Standard/Heavy/Complex in later skins); DWI holds are written `8!8` ("show!hold") and released at the next arrow in that column; sub-beat groupings `(…)`=1/16, `[…]`=1/24, `{…}`=1/64, `` `…' ``=1/192.

---

## 7. "Boo" semantics, misses, key-down-only judging, holds

- DDR Boo (Almost in SN–X) is a *hit* far from the note: it consumes the note, breaks combo, gives 0 score and drains life less than a Miss (MAX dance points −4 vs −8; gauge "small loss" vs "large loss") ([RemyWiki glossary](https://remywiki.com/DDR_Glossary), [Wikipedia DDRMAX](https://en.wikipedia.org/wiki/DDRMAX_Dance_Dance_Revolution_6thMix)). From X2 the Boo window simply became part of Miss.
- FFR Boo is a *press with no note*: "given when a player presses a directional key while an arrow is not at the stationary marker"; holding a key spams Boos; Boos subtract from score and life, Averages are life-neutral. FFR judges on a 30 fps grid with a 6-frame total window (Perfect 3 frames); per-note points Perfect 50, Good 25, Average 5, Miss −10 (Boo −5), with end-of-song multipliers ([FFR FAQ](https://www.flashflashrevolution.com/vbz/faq.php?faq=ffrfaq), [FFR timing-system thread](https://www.flashflashrevolution.com/vbz/home/forum/flash-flash-revolution/ffr-general-talk/48518-ffr-s-timing-system)). DDR, SM and DWI have no such penalty: extra presses are ignored.
- Miss = the note scrolls past the last (widest) window without being consumed ([ddrguide](https://ddrguide.com/glossary/); SM `UpdateTapNotesMissedOlderThan`). Judging is key-down only for taps; key-up matters only for Lifts (SM), for hold maintenance, and for FFR-style anti-mashing. Jumps in SM are judged per note but combo/score per row can be toggled; `TimingWindowJump=0.25` is only a calorie heuristic.
- Holds: DDR freezes have no release judgement ("holds do not have release judgement", [mzhang.io](https://mzhang.io/posts/2024-05-02-ddr/)) and tolerate brief releases ("you have some tolerance where you can let go of the arrow and press it again without dropping the freeze arrow", [kevinstiles](https://kevinstiles.game.blog/2020/04/24/changes-in-rhythm-games/)); the exact DDR grace period is not published. SM implements exactly that model with a 0.25 s (SL 0.32 s) life-decay window; ITG rolls require repeated taps within 0.35–0.5 s; a DDR O.K. is worth a full step score and 3 EX and does not add combo, an N.G. breaks combo and drains life (X+).

---

## 8. Note colour quantization schemes

SM's quantization enum is 4th, 8th, 12th, 16th, 24th, 32nd, 48th, 64th, 192nd ([NoteTypes.h](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/NoteTypes.h)); noteskins are either "flat" or "quantized" sheets with one row per NoteType ([SM Noteskins wiki](https://github.com/stepmania/stepmania/wiki/Noteskins)).

| Quantization | SM5 `dance/default` (sampled from `_arrow 1x8`) | ITG "Cel" convention | DDR NOTE | DDR RAINBOW (by position in beat) |
|---|---|---|---|---|
| 4th | red | red | red | orange (13/64–1/4) |
| 8th | blue | blue | blue | blue (5/64–1/8) |
| 12th | green | purple (triplets) | green ("other") | purple (9/64–3/16) |
| 16th | yellow | green | yellow | pink (1/64–1/16) |
| 24th | purple | purple (triplets) | green | — |
| 32nd | teal/cyan | yellow | green | — |
| 48th | magenta/pink | (triplet colour) | green | — |
| 64th, 192nd | grey (unsaturated) | turquoise (64th) / grey | green | — |

Sources: sampled from [SM default noteskin sprite](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/NoteSkins/dance/default/_arrow%201x8%20(doubleres).png); ITG colours from [HURG ITG-Noteskins README](https://raw.githubusercontent.com/HURG-IIDX/ITG-Noteskins/main/README.md); DDR schemes from [RemyWiki general info](https://remywiki.com/DDR_AC_General_Info). DDR VIVID/FLAT are animated hue cycles (VIVID phase-shifted by note value, FLAT one phase). SM supports both "Denominator" (quantization) and "Progress" (beat-fraction) colouring via `TapNoteNoteColorType` ([ZIv noteskin thread](https://zenius-i-vanisher.com/v5.2/thread?threadid=11227)). A skin API therefore needs: quantization bucket (with 192nd fallback), fractional beat position, song beat clock (for cycling), and a "static" override.


### 8.1 How StepMania animates VIVID, FLAT and RAINBOW

- **Vivid cycling** ([`NoteSkins/dance/midi-vivid/metrics.ini`](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/NoteSkins/dance/midi-vivid/metrics.ini), [`src/NoteDisplay.cpp`](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/src/NoteDisplay.cpp#L667-L690) `SetActiveFrame`): the tap texture is a 4×4 sheet of 16 frames played over `TapNoteAnimationLength=4` beats of the song (`fmod(songBeat, 4) / 4`), so a note changes colour as it scrolls and the cycle repeats every measure. With `TapNoteAnimationIsVivid=1` each note's phase is shifted by its beat fraction rounded down to `1/AnimationLength` (`QuantizeDown(fmod(noteBeat, 1), 0.25)`): an eighth note runs half a cycle (two beats) ahead of a quarter note, a sixteenth a quarter ahead, and anything finer shares its quarter's phase. The frames go from orange backwards round the hue wheel (sampled hues 32°, 26°, 9°, 336°, 319°, 308°, 289°, 223°, 199°, 192°, 166°, 114°, 86°, 73°, 60°, 44°), each beat's four frames also gaining a white highlight. DDR's FLAT is the same cycle without the shift, i.e. every note the same colour at a given moment.
- **Rainbow by position**: `NoteColorType=ProgressAlternate` with four colours ([`midi-rainbow/metrics.ini`](https://github.com/stepmania/stepmania/blob/825467bcd81c812b33ad684dc04dd151b2d5dec3/NoteSkins/dance/midi-rainbow/metrics.ini), `NoteDisplay.cpp` 1353–1371) picks `ceil(beat × 4) mod 4`, adding `count − 1` first when the note is exactly on a boundary. For notes on 4ths, 8ths and 16ths that gives DDR's RAINBOW from §4 (16th on the first quarter-beat pink, eighth blue, third 16th purple, beat orange); a note between those boundaries (a 32nd at 0.125, say) gets the band after DDR's. The game follows DDR's definition: within each beat, pink up to and including the first sixteenth, blue to the eighth, purple to the third sixteenth, orange up to and including the next beat. RemyWiki describes the bands as colour families, not flat colours: "Arrows have different gradient cycles which correspond to note value" ([RemyWiki general info](https://remywiki.com/DDR_AC_General_Info)), and VIVID arrows likewise "emit a rainbow gradient cycle". No source gives the gradient's colours or speed; the game draws each rainbow arrow with a gradient between two shades of its family that flows one cycle per beat, an approximation to be checked against the arcade by eye.

**Measured on DDR footage (VIVID).** A gameplay recording of DDR EXTREME 2 (PS2), *butterfly (UPSWING MIX)* at 170.009 BPM with the default VIVID colouring, examined frame by frame (29.97 fps), confirms StepMania's model and adds what its sprite sheet flattens: a note runs through the whole wheel (green → yellow → orange → pink → purple → blue → cyan) in about 1.4 s, i.e. one cycle per 4 beats (1.41 s); notes on and off the beat sit half a cycle apart; and each arrow is a gradient along its own axis, about a sixth of the wheel from tip to tail (e.g. yellow tip, orange middle, red tail), with the tail further along the cycle, so the colour flows from tail to tip and turns with the arrow.

The game's colour schemes (`render/src/note_colors.rs`) use these hues and timings with the procedural skin's own saturation, moving the hue smoothly between frames; Vivid and Flat arrows carry the tail-to-tip gradient (3/16 of the cycle).

---

## 9. Design implications for a pluggable judgement/scoring/gauge layer

1. **Window table with independent early/late bounds**, in seconds, per judgement tier, plus a global scale and additive term (SM's `scale*x + add` reproduces DDR-frame, ITG and SL/FA+ tables; Sesse's EXTREME data needs asymmetry). Allow a tier to be disabled (SL Casual folds W4/W5 into W3; DDR X2 folded Almost into Miss) and allow an extra "sub-window" (FA+ W0) that re-classifies a W1 hit for display/EX only.
2. **Judge offset and visual offset as separate knobs** (DDR JUDGMENT TIMING vs DISPLAY TIMING; SM `GlobalOffsetSeconds` default −0.008).
3. **Judgement → points mapping as data**, with two coexisting weight sets (SM percent vs grade weights) and a "secondary score" slot for EX (3/2/1, or SL 3.5/3/2/1). Money-score plugins need: step count (jumps = 1, freeze = 1, shock row = 1), per-tier multiplier, flat penalty (−10), rounding mode (to 10 vs 1), and the legacy combo-driven formulas (1st/2nd, 3rd, 4th, 5th/MAX), so the formula must see running combo and the step ordinal.
4. **Combo rules as parameters**: lowest tier that continues combo (Great pre-2013 / SM dance; Good in modern DDR; Perfect in DWI `#COMBO:PERFECT`), per-row vs per-note increments (modern DDR 1 per row; MAX era and SM 1 per note), whether O.K. increments (DDR no, SM holds no, rolls optional), whether N.G./LetGo breaks (DDR X+ yes, SM `ComboBreakOnImmediateHoldLetGo=false`).
5. **Gauge as a strategy object** receiving the same event stream: fixed per-event deltas (SM/ITG tables), optional scaling by note count or by position/timing deviation (CS DDR), state-dependent rules (DANGER zone halving losses, consecutive-miss escalation, "hot" penalty, regen-after-miss lockout, merciful drain, LifeDifficulty multiply/divide asymmetry), and distinct start values (50 % normal, 100 % no-recover/dan/flare). Boo and Miss must be separate events.
6. **Fail-condition hooks**: immediate (arcade), immediate-but-continue (score frozen, song keeps playing — DDR Premium guarantee, SM ImmediateContinue), end-of-song, off; N-lives batteries (LIFE4/LIFE8/RISKY, SM battery with configurable costs for W4/W5/Miss/mine/LetGo and per-song regain); sudden death (any combo break).
7. **Hold/roll/mine/lift callbacks**: head judged as a tap; body maintained by a decaying "hold life" with configurable decay window (DDR grace, SM 0.25 s, SL 0.32 s); rolls reset on tap; OK/NG emitted at tail or at life==0 (`ImmediateHoldLetGo`); checkpoint ticks for pump; mines on press within window; lifts on release; fakes ignored. Expose hold judgement weights separately in every mapping (DDR O.K. = full step, EX 3; SM Held 3/6, SL 5, EX 1).
8. **Grade tables as sorted threshold lists over a chosen score** (DDR money score; SM/SL percent; SL EX), with special predicates ("all W2 or better" for SM AAA; FC-gated grades in 4th/5th; "fail" grade E) and FC-tier lamps (MFC/PFC/GFC/FC, SL quad/quint stars).
9. **Event payload** must carry signed offset (Fast/Slow, timing average and variation on results), note row/column, note type and quantization, so renderers and skins (static / quantized / progress-cycling colours) and stats can be driven from the same stream.
10. **Chart transforms are not judging concerns** but affect counts: Cut/Freeze/Jump removal, Mirror/Left/Right/Shuffle, and "Little" change the step count fed to money-score plugins and flag assist clears.
