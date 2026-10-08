# FlashFlashRevolution (R^3) and Dancing☆Onigiri (danoniplus) as import sources

Research date: 2026-10-06. Primary sources were cloned and read directly:

- rCubed (FFR "R^3" engine), `main` @ `e57179c` (2025-08-21): https://github.com/flashflashrevolution/rCubed
- danoniplus (Dancing☆Onigiri CW Edition) v51.2.0, `develop` @ `500f794` (2026-10-04): https://github.com/cwtickle/danoniplus
- danoniplus-docs wiki @ `387b48f` (2026-10-04): https://github.com/cwtickle/danoniplus-docs/wiki

File citations below are of the form `rCubed:src/...` and `danoniplus:js/...`; prepend `https://github.com/flashflashrevolution/rCubed/blob/main/` or `https://github.com/cwtickle/danoniplus/blob/develop/`.

---

## Part A — FlashFlashRevolution

### A1. Chart format

**Container.** FFR charts are not standalone files. Each level is a Flash SWF ("level_<id>.swf") that contains both the MP3 audio and an ActionScript-2 `DoAction` tag that assigns a `_root.beatBox` array. R^3 downloads this SWF from `game/r3/r3-songLoad.php?id=<play_hash>&session=<session>` (`rCubed:src/URLs.as` L51; `rCubed:src/classes/chart/Song.as` `urlGen`), then (a) walks the SWF tags, interprets the AS2 bytecode just far enough to recover `_root.beatBox` (`rCubed:src/com/flashfla/media/Beatbox.as`), (b) extracts the MP3 frames (`MP3Extraction.extractSound`) and (c) strips audio to reuse the SWF as the background movie (`SwfSilencer.stripSound`) (`Song.as` `musicCompleteHandler`). The only in-engine chart type for FFR content is `NoteChart.FFR_LEGACY = "ChartFFRSWF"` parsed by `ChartFFRLegacy` at **30 fps** (`rCubed:src/classes/chart/NoteChart.as` `parseChart` passes `30`).

**Note representation.** Each `beatBox` entry is an array `[frame, direction, color, (ms)]`:

```actionscript
// rCubed:src/classes/chart/parse/ChartFFRLegacy.as  parseChart()
var validDirections:Array = ['L', 'D', 'U', 'R'];
var beatPos:int = beat[0] + (songInfo.sync || 0);
var beatPosMS:Number = beatPos / framerate;     // framerate == 30
if (beat.length >= 4) beatPosMS = (beat[3] / 1000);   // optional ms timing
Notes.push(new Note(beat[1], beatPosMS, beat[2] || "blue", beatPos));
```

So the canonical unit is **an integer frame at 30 fps** (33.33 ms resolution). `Note` = `{direction:String ('L','D','U','R'), time:Number (seconds), color:String, frame:Number}` (`rCubed:src/classes/chart/Note.as`). A per-song `sync` integer (frames) from the playlist XML (`arc_sync`) or engine config is added to every frame. The forum thread "FFR's timing system" confirms: "FFR is divided up into 30 frames per second, and every note in every song in the game is rounded to the nearest frame" (https://ffr.dance/vbz/home/forum/flash-flash-revolution/ffr-general-talk/48518-ffr-s-timing-system).

**Colors are stored, not derived.** The color string is literally in the chart data (defaulting to `"blue"` if missing). By convention the stepartist's tool colors notes by quantization. The FFR wiki lists: 4th = red, 8th = blue, 12th = purple, 16th = yellow, 24th = pink, 32nd = red-orange, 48th = turquoise, 64th = green, 192nd = white (https://ffr.fandom.com/wiki/Note, via search snippet). The engine's Stepmania importer encodes exactly that mapping, which is the authoritative version of the convention: 4th→`red`, 8th→`blue`, 12th→`purple`, 16th→`yellow`, 24th→`pink`, 32nd→`orange`, 48th→`cyan`, 64th→`green`, 192nd/invalid→`white` (`rCubed:src/classes/chart/parse/ChartStepmania.as` `noteTypeToColor`). The engine's palette is `noteColors = ["red","blue","purple","yellow","pink","orange","cyan","green","white"]` with a user-remappable `noteSwapColors` table (`rCubed:src/game/GameOptions.as` L57-58). Note that the user's premise ("blue=4th, red=8th") is reversed: red is 4th, blue is 8th. Important for import: because color is free-form text, a chart may legitimately carry colors that do not match its rhythm (e.g., "columncolour" and "halftime" mods rewrite colors at runtime: `rCubed:src/classes/chart/NoteMod.as` `COLUMN_COLOR`, `HALF_COLOR`).

**No holds, no mines, 4 lanes only.** `validDirections` is hard-coded to L/D/U/R; the Stepmania importer stores a hold tail but FFR gameplay ignores it. R^3 also ships local-file importers for `.sm`, `.ssc`, `.osu` and Quaver `.qua` (`rCubed:src/classes/chart/parse/ChartStepmania.as`, `ChartSSC.as`, `ChartOSU.as`, `ChartQuaver.as`, `ExternalChartBase.as`); those convert beat positions using 48 ticks per beat (`Math.round(48 * beat)`, `ChartStepmania.as` L617) and then quantize to 30 fps frames.

### A2. Judgement system

Judgements and in-game point values (`rCubed:src/game/controls/Judge.as` L42-47 and `GameplayDisplay.as` `commitJudge`):

| Judgement | Internal score id | Raw score delta | Combo | Health |
|---|---|---|---|---|
| AMAZING!!! | 100 | +50 | +1 | +5 |
| PERFECT! | 50 | +50 | +1 | +5 |
| GOOD | 25 | +25 | +1 | +5 |
| AVERAGE | 5 | +5 | +1 | +5 |
| BOO!! | -5 | −5 | unchanged | −5 |
| MISS! | -10 | −10 | reset to 0 | −5 |

**Timing windows.** The default `Constant.JUDGE_WINDOW` is expressed in **ms relative to the note, early negative**, with the matching 30-fps frame offset (`rCubed:src/Constant.as` L42-49). A hit at accuracy `a = (pressTime + judgeOffset) − notePosition` takes the *last* entry whose `t < a`:

| Entry `t` (ms) | Frame `f` | Result | Effective window |
|---|---|---|---|
| −118 | −3 | Average (5) | −118 < a ≤ −85 |
| −85 | −2 | Good (25) | −85 < a ≤ −51 |
| −51 | −1 | Perfect (50) | −51 < a ≤ −18 |
| −18 | 0 | Amazing (100) | −18 < a ≤ 17 (≈ ±1/2 frame) |
| 17 | +1 | Perfect (50) | 17 < a ≤ 50 |
| 50 | +2 | Good (25) | 50 < a ≤ 84 |
| 84 | +3 | Good (25) | 84 < a ≤ 117 |
| 117 | — | 0 (no hit) | a > 117 → not a hit |

The frame-based fallback `judgeScore()` is the same table in frames: diff −3→5, −2→25, −1→50, 0→100, +1→50, +2/+3→25 (`GameplayDisplay.as` L2020-2060). The window is asymmetric: early side ends at Average, late side ends at Good, and there is **no "Average" on the late side**. Total hittable window ≈ 235 ms (7 frames). The engine allows an alternative window (`options.judgeWindow`, from `ArcGlobals.configJudge`), which flags the score as non-standard.

**Miss.** A note is missed when `GAME_FRAME − note.FRAME + JUDGE_OFFSET_FRAMES >= 6`, i.e. 6 frames (200 ms) after the note (`GameplayDisplay.as` L1230), committing −10.

**Boo.** Any key press in a direction with no note inside the window commits −5 (`judgeScorePosition`/`judgeScore` else-branch). Boos before the first note are ignored (`commitJudge` case -5: `if (frame < gameFirstNoteFrame) return;`). Boo does not break combo. The FFR FAQ: "BOO = Given when a player presses a directional key while an arrow is not at the stationary marker" (https://www.flashflashrevolution.com/vbz/faq.php?faq=ffrfaq).

**Jacks.** Because notes in a lane are judged in order, a second note 2 frames behind the first covers the first note's Amazing window; the community thread documents that "any song that has 2 frame jacks is not AAAable" (forum thread above).

**Raw score and final score.** `gameScore` is the running "raw score" (+50/+50/+25/+5/−5/−10). The end-of-song total is:

```
total = (amazing+perfect)*500 + good*250 + average*50 + maxCombo*1000 − miss*300 − boo*15 + rawScore
```
(`GameplayDisplay.as` L1562). The community-quoted formula ("Perfect*500, Good*250, Average*50, Combo*1000, minus Miss*300, plus in-game score": https://www.flashflashrevolution.com/vbz/node/25839) matches except Boo×15.

**"Raw goods" and flags.** `gameRawGoods` accumulates Good = 1, Average = 1.8, Miss = 2.4, Boo = 0.2 (`commitJudge`). Community terms: AAA = all Perfect/Amazing (no Good/Average/Boo/Miss); FC = no misses; SDG = "single digit goods" (< 10 raw goods); Blackflag = AAA except exactly one Good; Booflag = AAA except one Boo (search results citing https://www.flashflashrevolution.com/vbz/faq.php?faq=ffrfaq and https://ffr.dance/vbz/home/forum/flash-flash-revolution/ffr-general-talk/127961-definition-of-a-ffr-aa). The engine's combo-color slots name them in order: Normal, FC, AAA, SDG, BlackFlag, AvFlag, BooFlag, MissFlag, RawGood (`GameOptions.as` L51). Autofail thresholds exist per judgement and for raw goods / "AAA equivalency" (`options.autofail[0..7]`, `checkAutofail`).

**Life.** `gameLife` starts at 50, clamps at 100, +5 per hit, −5 per Boo/Miss (`GlobalVariables.HEALTH_JUDGE_ADD/REMOVE`, `updateHealth`). Reaching ≤ 0 immediately ends the game (`GAME_STATE = GAME_END`). There is no end-of-song clear threshold; survival is the only rule.

**Offsets.** Two user offsets: *Global offset* shifts chart frames/time (visual + audio alignment; `NoteMod.transformNote` adds `Math.round(offsetGlobal)` frames) and *Judge offset* shifts only the judgement comparison: `JUDGE_OFFSET_FRAMES = round(offsetJudge)`, `JUDGE_OFFSET_MS = offsetJudge*1000/30` (`GameplayDisplay.as` L633-637). Both are therefore specified **in 30-fps frames** (fractional allowed for ms path). An "auto judge offset" option exists (`SettingsTabGeneral.as`). A per-song `sync` is added at parse time.

### A3. Options and supported modes

- 4-key only (`noteDirections = ["D","L","U","R"]`, `validDirections`).
- Scroll direction: `up, down, left, right, split, split_down, plus` (`GlobalVariables.SCROLL_DIRECTIONS`).
- Scroll speed (`scrollSpeed`, default 1.5), receptor spacing, note scale, judge scale, song rate 0.1–200× (`SettingsTabGeneral.as` L425) with pitch-shifted audio (`Song.onRateSound`).
- Game mods: `hidden, sudden, blink, rotating, rotate_cw, rotate_ccw, wave, drunk, tornado, mini_resize, tap_pulse, random, scramble, shuffle, reverse`; visual mods: `mirror, dark, hide, mini, columncolour, halftime, nobackground` (`GlobalVariables.GAME_MODS/VISUAL_MODS`). `mirror` maps dir→3−dir; `shuffle` is a per-play lane permutation; `random`/`scramble` re-roll per chord; `reverse` plays the song backwards (`NoteMod.as`).
- Noteskins: 10 embedded plus external (`src/game/noteskins/`), per-color remap (`noteSwapColors`).
- Display toggles for judge, combo, health, PA window, accuracy bar, screencut, etc. (`GameOptions.as` L31-48).
- Isolation (practice section by note index/length), autoplay, replays (frame-based or ms-based).

### A4. Data availability and terms

Chart SWFs are served only through `r3-songLoad.php` with a `play_hash` and user `session` (`Song.urlGen`, `URLs.as`); the AIR client may cache them encrypted locally (`AirContext.encodeData`). There is no public bulk download, and each SWF embeds the licensed MP3; FFR requires per-artist permission for songs (https://ffr.fandom.com/wiki/Category:Artists_Permissions). The engine is AGPL-3.0 (README), but that covers code only, not chart/music content. Practical conclusion: an FFR importer can target (1) the `beatBox` array format for users who legitimately have SWFs, and (2) FFR-style `.sm` conversions, but chart data must not be redistributed with the game.

---

## Part B — Dancing☆Onigiri (CW Edition / danoniplus)

*B8 (2026-10-08) corrects several points of B1–B5 from the source: note arrays are not expressions, the fraction of `adjustment` has no effect, judging buckets are `[−k, k+1)` frames, `frzAttempt` counts released frames cumulatively, `color_data` keys on arrival frames, Light/Easy recovery is doubled, `sfsf` is unused.*

### B1. Chart format

A "dos" (ダンおにスコア) is plain text of pipe-delimited `|key=value|` pairs, embedded in the HTML (`danoni/danoni1.html` L39-135) or loaded from an external `.txt`/`.js` that defines `g_externalDos` inside `externalDosInit()` (`danoni/dos/0001_ThrustUp.txt`). The spec splits into header, body and effects (https://github.com/cwtickle/danoniplus-docs/wiki/dos_header, `.../dos_score`, `.../dos_effect`). All times are **integer frames at 60 fps** ("60 seconds is 60 seconds x 60 fps = 3600 frames", https://github.com/cwtickle/danoniplus-docs/wiki/dos-h0005-startFrame). The engine simulates Flash-style frames with `setTimeout`; drawing is frame-locked, but input and audio position are not, and sub-frame Adjustment is done by shifting audio position (https://github.com/cwtickle/danoniplus-docs/wiki/AboutFrameProcessing).

**Body: per-lane note arrays.** `|<chara>_data=f1,f2,...|` lists the frame at which each note reaches the step zone; `|frz<Chara>_data=s1,e1,s2,e2,...|` lists hold start/end pairs (https://github.com/cwtickle/danoniplus-docs/wiki/dos_score). The 2nd, 3rd… charts use `left2_data`, `frzLeft2_data`, etc. Lane names come from the key definition's `charaX_Y` array (see B2). Any value may be a mathematical expression, and effect lists may reference another chart's list by name (`|speed2_data=speed_data|`).

**Header keys (selection, with units):**

| Key | Meaning / units | Source |
|---|---|---|
| `musicTitle=title,artist,url[,altTitle,bpm]` (`$`/newline for multiple; `musicTitleEn/Ja`) | metadata; `<br>` for line break, `*comma*` escape | dos-h0001 |
| `difData=key,name[::maker[::level]],initSpeed,border,recovery,damage,init` per chart (`$`/newline) | key mode; chart name; initial speed (default 3.5); gauge border % (0–100, `x` = life-based), recovery %, damage %, initial life % (defaults `x`,6,40,25) | dos-h0002 |
| `musicUrl`, `musicNo`, `musicFolder` | mp3/wav/ogg or base64 `.js`; per-chart music index | dos-h0011/12/13 |
| `startFrame` | first frame to play (frames @60; `m:ss.ff` pseudo-timer allowed) | dos-h0005 |
| `blankFrame` | frames of silence before music starts; default 200 | dos-h0006 |
| `endFrame` | frame to leave play (per chart) | dos-h0007 |
| `fadeFrame=start[,length]` | fade-out start, length default 420 frames | dos-h0008 |
| `adjustment` | per-chart timing correction in frames (pseudo-fractional via audio position) | dos-h0009 |
| `playbackRate` | audio speed; chart frames recomputed | dos-h0010 |
| `setColor`, `frzColor` (`setColorN`/`frzColorN` per chart) | 5 arrow-group colors; freeze = 4 colors (arrow normal, bar normal, arrow hit, bar hit) per group via `$`; `:` gradients | dos-h0003/4 |
| `setShadowColor`, `frzShadowColor`, `defaultColorgrd`, `defaultFrzColorUse`, `frzScopeFromAC` | fill colors / auto-gradient | dos_header |
| `gaugeNormal/Easy/Hard/Original/Light/Heavy/NoRecovery[N]=border,recovery,damage,init` | per-gauge override, same semantics as difData | dos-h0022 |
| `customGauge[N]=Name::F|V[::Label],...` | custom gauge list; F = fixed amounts, V = scaled by note count | dos-h0053 |
| `maxLifeVal` | gauge max, default 1000 | dos-h0045 |
| `dummyId` | chart number whose `*_data` arrays are drawn as always-auto dummy notes | dos-h0042 |
| `frzStartjdgUse`, `frzAttempt` | judge hold starts (changes note count); frames of release tolerance (default 5) | dos-h0037/38 |
| `excessiveJdgUse` | enable "Excessive" (early-press penalty) | dos-h0093 |
| `stepY`, `stepYR`, `jdgY`…, `playingX/Width/Height`, `windowWidth/Height` | layout in px (stepY default 70) | dos-h0014 etc. |
| `minSpeed`, `maxSpeed` | settable speed bounds (default 1–10) | dos-h0015/16 |
| `settingUse` (`motionUse`, `scrollUse`, `shuffleUse`…), `displayUse` | gate options per work | dos-h0035/57 |
| `tuning=name,url` (`tuningEn/Ja`), `hashTag`, `releaseDate`, `makerView` | credits | dos-h0017 |
| `customjs`, `customcss`, `skinType`, `imgType`, `preloadImages` | extension hooks and skins | dos_header |
| `keyExtraList`, `keyCtrlX`, `charaX`, `colorX`, `stepRtnX`, `posX`, `divX`, `shuffleX`, `scrollX`, `assistX`, `keyGroupX`, `transKeyX`… | custom key definitions (see B2) | wiki/keys |

**Effect data (footer).** All share the "frame first" convention and allow comment lines `frame,-,text` (https://github.com/cwtickle/danoniplus-docs/wiki/dos_effect):

- `speed_data` / `speed_change` (synonyms since v8): pairs `frame,multiplier`; applies to the whole screen immediately; 0 stops, negative reverses. `boost_data`: same pairs but applies only to notes *judged after* that frame ("individual acceleration", notes can overtake). Per-frame travel = initialSpeed × speed_data × boost_data + motion term (https://github.com/cwtickle/danoniplus-docs/wiki/SpeedChange, `.../dos-e0001-speedData`). Neither changes timing.
- `color_data` (per-note, applies to notes appearing after frame) / `acolor_data` (whole screen): triples `frame,colorNo,colorCode`. colorNo 0–19 (or 1000+) = individual lane, 20–24 = arrow groups 1–5, 30–39/40–49/50–59 = freeze groups normal/hit arrow/bar, 60/61 = all freezes. Newer `ncolor_data`: `frame,target[:pattern],color[,all]` where target is `0...7`, `1/3/5`, `g0..g9`, `all` and pattern is `Arrow, ArrowShadow, Normal, NormalBar, Hit, HitBar, FrzNormal, FrzHit, FrzShadow, Frz, AF` (`.../dos-e0002-colorData`, `.../dos-e0002-ncolorData`).
- `word_data`: `frame,position,text[,fadeFrames]`; even positions upper row, odd lower; keywords `[fadein] [fadeout] [left] [center] [right] [fontSize=XX]`; default fade 30 frames; variants `wordRev_`, `wordAlt_/Cross_/Flat_`, `wordEn_/Ja_`, `wordA_` (another key mode), `word<9C>`, `word[2]` (`.../dos-e0003-wordData`).
- `back_data` / `mask_data` (plus `backtitle_`, `backresult_`, `backfailedS_`, `backfailedB_`, `Rev/Alt/A/En` variants): `frame,depth,src|text|[c]object|[t]transform,class,x,y,w,h,opacity,animName,animFrames,fillMode`; `frame,depth` alone clears; `[loop]`/`[jump]` for title/result only (`.../dos-e0004-animationData`).
- `arrowMotion_data` / `frzMotion_data`: `frame,colorNo,cssUp,cssDown[,movLock][,initManual]` (`.../dos-e0005-motionData`).
- `keych_data`: `frame,keyGroup[/group2][:opacity]` toggles partial key layouts (`.../dos-e0006-keychData`).
- `scrollch_data`: `frame,colorNo,±1[,layerGroup,transform]` per-lane scroll reversal (`.../dos-e0007-scrollchData`).
- `style_data` (`styletitle_`, `styleresult_`…): `frame,--css-custom-property,value` (`.../dos-e0008-styleData`).

**Example** from the bundled sample (`danoniplus:danoni/dos/0001_ThrustUp.txt`): `|difData=5,Thrust,3.5$7,Hard,3.5,75,1,7$11L,Upper,3.5,75,1,7|`, `|left_data=608,875,...|`, `|frzLeft_data=1728,1781,4093,4135,...|`, `|speed_data=284,1,954,0,1100,1,5475,1.125,6144,1|`, `|color_data=210,20,0x80FFFF,...|`. The second sample uses `|oni_data=...|foni_data=|` for a 14-key onigiri lane, `|speed_change=400,0.25|`, `|boost_data=200,3|`, and per-gauge overrides `|gaugeEasy=$50,3,5,40$...|` (`danoni/danoniA.txt`).

### B2. Key modes

Lane order, data names and default keys (pattern 0) come from `dos_score` and `g_keyObj` (`danoniplus:js/lib/danoni_constants.js` L3346-4010). `stepRtnX_Y` gives the arrow rotation relative to a left arrow (−90 = down, 90 = up, 180 = right, ±45/135 = diagonals, 30/60/120/150 = 12-key fan) or an ASCII-art object: `onigiri`, `giko`, `iyo`, `c`, `morara`, `monar` (https://github.com/cwtickle/danoniplus-docs/wiki/keys). Standard modes:

| Mode | Lanes (data name prefix → `_data`/`frz…_data`) | Default keys | Notes |
|---|---|---|---|
| 5 | left, down, up, right, space | ←↓↑→ Space | space = onigiri (`stepRtn5_0_0: [0,-90,90,180,'onigiri']`) |
| 7 | left, leftdia, down, space, up, rightdia, right | S D F Space J K L | diagonals at −45/135; center onigiri |
| 7i | same names as 7 | Z X C ←↓↑→ | lanes 0–2 are giko/onigiri/iyo AA, 3–6 arrows |
| 8 | 7 + sleft | 7key + Enter | extra lane shown as left arrow |
| 9A (DP) | left,down,up,right,space,sleft,sdown,sup,sright | S D E F Space J K I L | two 4-arrow hands + onigiri |
| 9B | same names, keys A S D F Space J K L + | | fan of 45° steps |
| 9i | sleft…sright (top), left…space (bottom) | ←↓↑→ / A S D F Space | double scroll |
| 9d | 9A names | S D F V B N J K L | |
| 9h | 1x,yx,ux,ix,ax,zx,sx,hx,mx | 1 Y U I A Z S H M | irregular positions |
| 11 / 11L | sleft,sdown,sup,sright (top) + 7key names (bottom) | ←↓↑→ (11) or W E 3 R (11L) + S D F Space J K L | |
| 11W | 11 names | 1 T Y 0 + bottom | |
| 11i | left,down,gor,up,right,space,sleft,sdown,siyo,sup,sright | S X D E F Space J M K I L | `gor`/`iyo` AA lanes |
| 11j | gor + 9A names + siyo | Tab … Enter | |
| 12 | sleft..sright (top), oni, left,leftdia,down,space,up,rightdia,right (bottom) | U I 8 O / Space N J M K < L > | bottom row fan 0..180 step 30 |
| 12i | oni, 7key names, sleft..sright | F1–F12 | |
| 13 (TP) | tleft..tright, left..right, space, sleft..sright | ←↓↑→ S D E F Space J K I L | |
| 14 | sleftdia, sleft..sright, srightdia (top), oni + 7key (bottom) | T U I 8 O @ / Space N J M K < L > | |
| 14i | gor,space,iyo,left..right (top), s-7key (bottom) | Z X C ←↓↑→ S D F Space J K L | |
| 15A / 15B | sleft..sright, tleft..tright (top), 7key (bottom) | W E 3 R ←↓↑→ S D F Space J K L | |
| 16i | gor,space,iyo,left..right (top), sleft..sright, aspace, aleft..aright (bottom) | Z X C ←↓↑→ A S D F Space J K L + | |
| 17 | a/b-left..right interleaved, space, c/d-left..right | A Z S X D C F V Space N J M K < L > + | |
| 23 | a-,b- (top), 7key, oni, s-7key (bottom) | W E 3 R U I 8 O Z S X D C F V Space N J M K < L > | |

(https://github.com/cwtickle/danoniplus-docs/wiki/dos_score, `.../AboutKeyMode`). Each lane also has a **color group** (0–4, indexes `setColor`), a **shuffle group** (lanes permuted only within a group), a **scroll direction** per Scroll option (`scrollDir5_0: {'---':[1,1,1,1,1], Cross:[1,-1,-1,1,1], Split:[1,1,-1,-1,-1], Alternate:[1,-1,1,-1,1]}`), an **assist** flag, a **layer group**, and `div` (split between top/bottom rows). Multiple key *patterns* per mode exist (`keyCtrl11_1`, etc.) and "Another key mode" (`transKeyX`) lets a 9A chart be played with 9B layout. Custom modes are declared in the chart header with the same attributes (`|keyCtrl6=...|chara6=...|color6=...|stepRtn6=...|pos6=...|`), so an importer must treat lane count, names, geometry and colors as **data**, not an enum (https://github.com/cwtickle/danoniplus-docs/wiki/keys; `danoni/danoni1.html` L40/L71 defines a 6-key and 8-key with `arrowA2_data`…).

### B3. Judgement

Windows are symmetric in **60-fps frames** with `|diff| ≤ bound` (`g_judgObj = { arrowJ: [2,4,6,8,16], frzJ: [2,4,8] }`, `danoniplus:js/lib/danoni_constants.js` L1141; https://github.com/cwtickle/danoniplus-docs/wiki/AboutJudgment):

| Judgement (Ja / En) | Frames | ms (@60) | Effect |
|---|---|---|---|
| (・∀・)ｲｼ!! Ii / Perfect | 0–±2 | ±33.3 | combo+1, recovery |
| (`・ω・)ｼﾔｷﾝ Shakin / Great | ±3–4 | ±50–66.7 | combo+1, recovery |
| ( ´∀`)ﾏﾀｰﾘ Matari / Good | ±5–6 | ±83.3–100 | combo display cleared (no recovery, no damage, combo counter *not* reset in code: `judgeMatari` only blanks text) |
| (´・ω・`)ｼｮﾎﾞｰﾝ Shobon / Bad | ±7–8 | ±116.7–133.3 | combo = 0, damage |
| ( `Д´)ｳﾜｧﾝ!! Uwan / Miss | late > +8 (out of frame, +9 or more) | > 133 | combo = 0, damage |
| Excessive | early −9…−16 (only if Excessive ON) | −150…−267 | counted; damage × 0.25 |
| (ﾟ∀ﾟ)ｷﾀ-!! Kita / O.K. (hold) | start within ±4 and held to end | | freeze-combo+1, recovery |
| (・A・)ｲｶﾅｱ Iknai / N.G. (hold) | start ±5–8, or missed, or released > `frzAttempt` (5) frames, or next hold arrives | | freeze-combo = 0, damage |

(`danoniplus:js/lib/mainWindow.js` `judgeArrow`, `judgeRecovery`, `judgeDamage`, `judgeMatari`, `judgeKita`, `judgeIknai`, `displayDiff`, `lifeDamage(_excessive)`.) Selectable judge ranges: Normal `[2,4,6,8,16]`, Narrow `[2,3,4,8,16]`, Hard `[1,3,5,8,16]`, ExHard `[1,2,3,8,16]` (`g_judgRanges`). Hold starts are judged only if `frzStartjdgUse=true`; otherwise only the hold outcome counts. **Fast/Slow**: any hit with `|diff| > justFrames` (default 1 frame; `dosConverter.js` L1789) increments `fast`/`slow` and shows "Fast N Frames"; the result screen also shows an **Estimated Adj** = Adjustment − Bayesian-estimated mean offset when ≥ 20 samples (`result.js` L73). Judgement uses the closest of (next arrow, next hold start) per lane; a late previous note is force-removed when the next enters the recovery window, unless the press is within ±2 of the previous (AboutJudgment).

### B4. Life gauge

Max life `maxLifeVal` (default 1000). Each gauge is `{Border, Recovery, Damage, Init, Variable}` (`g_gaugeDefObj`, `danoni_constants.js` L1330-1341):

| Gauge | Mode | Border | Recovery | Damage | Init | Variable |
|---|---|---|---|---|---|---|
| Original | life | x | 6 | 40 | 25% | F (fixed) |
| Light | life | x | 12 | 40 | 25% | F |
| Heavy | life | x | 2 | 50 | 50% | F |
| NoRecovery | life | x | 0 | 50 | 100% | F |
| SuddenDeath | life | x | 0 | maxLife | 100% | F |
| Practice | life | x | 0 | 0 | 50% | F |
| Normal | border | 70% | 2 | 7 | 25% | V (scaled) |
| Easy | border | 70% | 4 | 7 | 25% | V |
| Hard | border 0 (life) | 0 | 1 | 50 | 100% | V |

Formulas: `init = maxLifeVal × Init/100`; `border = maxLifeVal × Border/100`; for F gauges recovery/damage are absolute life units; for V gauges `real = value × maxLifeVal / allArrows` where `allArrows = arrows + (frzStartjdgUse ? 2 : 1) × freezes` (`settings.js` `gaugeFormat`, `dataLoader.js` `calcLifeVal`). Recovery on Ii/Shakin/Kita; damage on Shobon/Uwan/Iknai; Excessive = 0.25 × damage; Matari neither (`mainWindow.js`). **Clear/fail**: in border mode the game always runs to the end and is FAILED if `life < border` at the final frame; in life mode (`border == 0`) hitting life 0 terminates immediately (`mainWindow.js` L1618-1632: `if (lifeMode === BORDER && lifeVal < lifeBorder) gameOver` at `currentFrame >= fullFrame`; `else if (lifeVal === 0 && lifeBorder === 0)` early stop). So DanOni is predominantly end-evaluated, with mid-song fail only for life-type gauges. `difData` fields 4–7 and `gaugeX` headers override these per chart; `customGauge` adds named gauges.

### B5. Scoring and rank

`score = round( (ii×8 + shakin×4 + matari×2 + kita×8 + sfsf×4 + maxCombo×2 + fmaxCombo×2) / (fullArrows×10) × 1,000,000 )` (`g_pointAllocation`, `result.js` L89-93; `g_maxScore = 1000000`, `danoni_main.js` L204). Ranks by % of max: SS ≥ 97, S ≥ 90, SA ≥ 85, AAA ≥ 80, AA ≥ 75, A ≥ 70, B ≥ 65, C ≥ 60, D otherwise; special: AP (all Ii/Kita → 1,000,000), PF (only Ii/Shakin/Kita), F (failed), X (not all notes played / autoplay / excessive disabled when required) (`g_rankObj`, `result.js` L96-118; https://github.com/cwtickle/danoniplus-docs/wiki/AboutGameSystem#rank). A "Full Combo" state is also tracked (`finishViewing`: no uwan/shobon/iknai).

### B6. Options

(https://github.com/cwtickle/danoniplus-docs/wiki/AboutGameSystem; `g_settings` in `danoni_constants.js`)

- **Speed** 1×–10× (chart may widen via `minSpeed/maxSpeed`), list in 0.05 steps with cursor steps 1/0.25/0.05 (`makeSpeedList`, `speedTerms:[20,5,1]`).
- **Motion** OFF, Boost, Hi-Boost, Brake, Compress, Fountain, Magnet (added per-frame velocity curve; Hi-Boost scales with speed).
- **Reverse** ON/OFF; **Scroll** `---`, Cross, Split, Alternate, Twist, Asymmetry, AA-Split, Flat (availability per key mode).
- **Shuffle** OFF, Mirror, X-Mirror, Turning, Random, Random+, S-Random, S-Random+, Scatter, Scatter+ (group-aware; only OFF/Mirror/X-Mirror keep high scores).
- **AutoPlay** OFF, ALL, plus lane-group assists (Onigiri, Left, Right) depending on mode.
- **Gauge** as above; **JudgRange** Normal/Narrow/Hard/ExHard; **Excessive** OFF/ON.
- **Adjustment** ±90 frames in 0.1-frame steps (`g_limitObj.adjustment=90`, `adjustments` list); **HitPosition** ±50 px-equivalent in 0.1 steps; **Fadein** 0–99 % slider (no high score when > 0); **Volume** 0, 0.5, 1, 2, 5, 10, 25, 50, 75, 100.
- **Appearance** Visible, Hidden (top 50%), Sudden (hidden first 40%), Hidden+, Sudden+, Hid&Sud+ (adjustable with PageUp/PageDown, lockable); **Opacity** 10/25/50/75/100 for judgement text.
- **Display** toggles: StepZone, Judgment, FastSlow, LifeGauge, Score, MusicInfo, FilterLine, Velocity (speed changes), Color (color changes), Lyrics, Background, ArrowEffect, Special.
- **KeyConfig** screen: rebind any lane (multiple keys per lane), key patterns, ColorType (Type0 = chart-defined colors; Types 1–4 = alternative group palettes, user-tintable per group), ImgType (note skins), ShuffleGr./ColorGr./ShapeGr. regrouping, KeySwitch, Another Key Mode.
- No "Ten"/no-judge option exists in the current source; the closest are `Practice` gauge and `AutoPlay`.

### B7. License and reuse of test data

danoniplus is MIT-licensed (`LICENSE`, README "License" section). The wiki text is CC BY-SA 4.0 (wiki footer). The bundled sample charts carry third-party music credits (`musicTitle=Thrust up,Y.W`; `Combative InstinctⅡ,Trial`; `Stormy wing,SHIKI`), and the repo ships only `music/nosound.mp3`; the chart text itself is part of the MIT repo, but the songs are not included and are not MIT. Reusing the `.txt` chart data as parser fixtures (without audio, with attribution) is legally fine under MIT; redistributing the referenced music is not.

### B8. Corrections and additions (2026-10-08)

Checked against danoniplus v51.2.2, `develop` @ `80c3c47` (2026-10-08), by reading the source. Where this conflicts with B1–B6, this section wins; the points marked *browser check* follow the code but have not been observed in a running page.

**Frame to music time.** `scoreConvert` maps a chart frame through `calcFrame = F => Math.round((F − blankFrame) / playbackRate + blankFrame + intAdjustment)` with `intAdjustment = floor((userAdj + headerAdj) / playbackRate + preblankFrame)` (`round` only for local `file://` music) (`js/lib/dataLoader.js` 726–733). The audio starts at counter value `frameNum + blankFrame`, positioned at `frameNum / 60 × rate` (`js/lib/mainWindow.js` 174–176, 1825–1837), and every frame is scheduled against the AudioContext clock from that point (`g_audioClockSync`, `mainWindow.js` 1653–1670). So at rate 1 note frame `F` sounds at `(F − blankFrame + floor(adjustment)) / 60` s into the file. The fractional part of `adjustment` moves the audio start (`play((B − decimalAdjustment + 1)/60)`, `mainWindow.js` 1830) but the frame grid is anchored to that same start, so it cancels (*browser check*). `blankFrame` (default 200) is per chart (`js/lib/dosConverter.js` 1426–1431); `playbackRate` is one header value (1452). `preblankFrame` (`dataLoader.js` 384–410) lengthens the lead-in for charts whose first note comes early and shifts notes and music together, so it never changes the mapping. `startFrame` only skips content.

**Note data.** Note and hold arrays are not expressions: lines are concatenated, split on `,`, `parseFloat`ed, mapped through `calcFrame` and sorted (`dataLoader.js` 740–742); hold values are sorted as one list before pairing, an odd last value is dropped (1556–1558). Chart N > 1 uses the suffix N (`left2_data`, `frzLeft2_data`; `setScoreIdHeader`, `dataLoader.js` 477–484). Hold array names come from replacing parts of the lane name (`g_escapeStr.frzName`, `js/lib/danoni_constants.js` 1023–1027): `sleft` → `sfrzLeft`, `leftdia` → `frzLdia`, `oni` → `foni`, else `frz` + capitalised name. Effect values (speed, boost, colours, gauges) are evaluated as JavaScript (`new Function`, `js/danoni_main.js` 399–405) and may reference another key's value by name (`getRefData`, `dataLoader.js` 988–994); a second field `-` makes the rest of that line a comment. Fields are split on `|` and also on `&` (`dosConverter.js` 521–535). An external dos file is a script defining `externalDosInit() { g_externalDos = `…` }`, loaded with the charset of `<input id="externalDosCharset">` (Shift_JIS in `danoni/danoni2.html`); inline dos is the value of the element with `id="dos"`.

**Judging.** Each arrow counts down `cnt = arrival − current frame`; a keydown is judged at once with `diff = cnt`, positive = early (`mainWindow.js` 937, 1120, 2537), against `|diff| ≤ [2,4,6,8,16]` inclusive (2800–2803). In continuous time that is Ii for presses in `[A − 2, A + 3)` frames, Shakin `[A − 4, A − 2) ∪ [A + 3, A + 5)`, Matari `±6/+7`, Shobon `±8/+9`; Uwan comes automatically at `A + 9` (697–700). Excessive is an early press with 8 < diff ≤ 16, arrows only, ×0.25 damage, the arrow stays (2491–2493, 2565–2566). The press targets whichever of the lane's current arrow and current hold arrives first (a tie judges nothing), not the closest. When the next arrow in a lane is within 4 frames early, an unjudged previous one at least 3 frames late becomes Uwan (`judgeNextFunc.arrowOFF`, 797–816; holds likewise, 826–847). Hard and ExHard judge ranges also tighten the hold windows (`frzJ` `[1,3,8]`, `[1,2,8]`). Estimated Adj needs more than 20 samples (`js/lib/result.js` 73).

**Holds.** A hold start is accepted within ±8 frames, held if within ±4 (`judgeTargetFrzArrow`, `mainWindow.js` 2508–2533); a start at 5–8 frames is Iknai at once only with `frzStartjdgUse`, else at `A + 9`. While held, every frame without the lane's key down adds to a counter that never resets; Iknai when it exceeds `frzAttempt` (default 5; 1319–1324). The hold ends Kita at its end frame whatever the start offset. Counting: `fullArrows = arrows + (frzStartjdgUse ? 2 : 1) × holds` (`dataLoader.js` 446–456).

**Scroll.** Per-frame travel is `speed_data factor × user speed × 2 × baseSpeed` px (`dataLoader.js` 1386–1398); `speed_data` values are absolute (not cumulative) from their frame, and `speedN_change` wins over `speedN_data` with no fallback to chart 1 (1158–1167). `boost_data` applies by each note's arrival frame (1503–1510); notes spawn where their unboosted travel fills the field, so a slowed note appears mid-field (1830–1848).

**Colours.** A lane's colour is `setColor[color group]`, defaults `#6666ff #99ffff #ffffff #ffff99 #ff9966` repeated (`danoni_constants.js` 4345–4349); hold parts are `frzColor[group]` = normal arrow, normal bar, hit arrow, hit bar (defaults `#66ffff #6600ff #cccc33 #999933`). `color_data` (and `ncolor_data` without the all flag) applies to notes *arriving* at or after its frame (the frame is converted to a spawn frame, `dataLoader.js` 1660–1680); `acolor_data` and all-flag colours recolour what is on screen at that frame.

**Key modes.** `g_keyObj` (`danoni_constants.js` 3346–4015) also defines 9d, 9h, 11j and 12i; mode 8's extra lane is an onigiri; 15B uses `U I 8 O` where 15A uses the arrow keys. Lanes with `pos < div` have their step zone at the top and scroll up, the others at the bottom scrolling down (two-row modes); x position `blank × (pos − centre of its row)` (`dataLoader.js` 2273–2289). `keyCtrl` names map to `KeyboardEvent.code` through `g_kCdN` (JIS layout). A header key mode needs `keyCtrlX` at least; missing attributes default to left arrows in one row (`dosConverter.js` 2343–2775). Mirror reverses lanes within each shuffle group, Random shuffles within each group (`applyMirror`, `applyRandom`, `dataLoader.js` 547–611).

**Gauges.** As B4, plus: with `difData` border `x` the gauge list is Original, Heavy, NoRecovery, SuddenDeath, Practice, Light, else Normal, Hard, SuddenDeath, Easy (`dosConverter.js` 2156–2158); `difData` overrides only the first family (Original/Light or Normal/Easy), and Light and Easy take twice its recovery; `gaugeX` headers override after that (2161–2187). Life stops early whenever it reaches 0 with border 0, in any mode (`mainWindow.js` 1617–1634). Initial life is `floor(init × 100) / 100` (`dataLoader.js` 2446).

**Score.** B5's formula holds, but `sfsf` is never incremented (always 0); it is also the name of the 4-frame hold-start bound. Rank X (no rank) when not every note was judged, with autoplay, or with Excessive off on a chart that requires it.

---

## Part C — Implications for an internal chart model

1. **Time base.** FFR = integer frames at 30 fps (plus optional ms per note, plus per-song integer `sync`); DanOni = integer frames at 60 fps with `blankFrame`/`adjustment`/`startFrame` offsets and `playbackRate` rescaling; DDR/SM = beats + BPM/stop segments. A lossless core should store note time as an exact rational or integer in a per-chart **tick base** (e.g., `ticks_per_second` = 30 or 60 for frame charts, beat-based for SM) together with the original frame integer, rather than forcing beats. Conversion to beats is lossy (and FFR charts have no BPM at all).

2. **Lanes as data, not enum.** DanOni lane sets are arbitrary (5…23+, custom), with per-lane metadata: display glyph/rotation (`stepRtn`, including `onigiri`/`giko`/`iyo` AA objects), color group, shuffle group, scroll direction sign, screen position/row (`pos`/`div`), assist flag, layer group, default key bindings and multiple patterns. FFR has exactly 4 lanes with a fixed `[L,D,U,R]` order. The model needs a `LaneLayout` object with N lanes and these attributes; the DDR 4-panel layout becomes one instance.

3. **Notes.** Union of: tap (all); hold with start/end (DanOni `frz`, SM holds/rolls — FFR none); stored per-note color/quantization label (FFR text color; SM derived; DanOni per-lane group color with time-varying overrides); **dummy/visual-only notes** (DanOni `dummyId` chart, always auto, no judgement) ⇒ a `judged: bool` or note kind. Multiple difficulties share one file (DanOni `N`-suffixed arrays; SM `#NOTES` blocks).

4. **Scroll-speed events vs timing.** DanOni `speed_data` (global, immediate), `boost_data` (applies to notes spawned after frame — effectively a per-note speed multiplier), and settings-level Motion curves never alter judgement time; SM `#SCROLLS`/`#SPEEDS` likewise, while `#STOPS`/`#WARPS` do. The model should separate a **timing map** (chart time ↔ audio time) from **scroll events** (global multiplier timeline + per-note multiplier), and also allow per-lane scroll-direction flips over time (`scrollch_data`) and key-layout switches (`keych_data`).

5. **Color timelines.** DanOni needs per-target color changes keyed by frame, where target = lane index, color group g0–g9, or freeze sub-part (arrow/bar, normal/hit, shadow), either "for notes spawned after" or "immediately for all". Represent as `ColorEvent{frame, target, part, color, immediate}` with gradient strings preserved verbatim.

6. **Text and media events.** Lyrics (`word_data`: frame, row slot, text with inline tags, fade in/out, alignment, variants per language/scroll/key mode) and background/mask layers (`back_data`/`mask_data`: image/text/colored-object/CSS-transform with depth, geometry, opacity, CSS animation name/duration/fill; title/result variants with loop/jump) plus CSS variable changes (`style_data`) and note motion CSS (`arrowMotion_data`). These should be kept as an opaque, ordered `EffectEvent` stream per layer with the raw payload preserved, so a round-trip export is possible even if the renderer only supports a subset.

7. **Metadata.** Title/artist/URL (per music index), chart name + maker + level, initial speed, music file(s) and BPM (DanOni optional), credits, hashtag, release date; FFR has level id, play hash, stepartist, note count, min/max NPS from the playlist XML.

8. **Pluggable scoring/judge/gauge.** The three systems disagree on almost every axis:
   - *Windows*: FFR asymmetric 30-fps table (−118…+117 ms, Amazing ≈ ±17 ms, no late Average), DanOni symmetric 60-fps buckets (±33/66/100/133 ms, Excessive −150…−267 ms, four selectable ranges), DDR's Marvelous ±16.7 ms / Perfect ±33 ms (https://scrapbox.io/0b5vr/Timing_Window) with Great/Good/Miss beyond.
   - *Empty presses*: FFR punishes every stray press (Boo: −5 score, −5 life, no combo break); DanOni ignores them unless Excessive is on and the press is 9–16 frames early (quarter damage); DDR ignores them.
   - *Combo*: FFR breaks on Miss only; DanOni breaks on Shobon/Uwan and merely hides on Matari, and keeps a separate freeze combo; DDR breaks on Good/Miss (version dependent).
   - *Life*: FFR fixed ±5 on a 0–100 bar starting at 50, instant fail at 0, no clear threshold; DanOni parameterised `{init, border, recovery, damage}` per chart/gauge, either scaled by note count or fixed, with end-of-song clear line or instant fail; DDR gauge fails mid-song on the normal bar, with life4/risky variants.
   - *Score*: FFR running raw score + end formula with combo×1000 and "raw goods" accuracy metric; DanOni 1,000,000 normalised with per-judgement weights and combo terms, letter ranks by percentage plus AP/PF; DDR 1,000,000 normalised with its own step weights (https://remywiki.com/DanceDanceRevolution_Scoring_System).
   These differences argue for a `ScoringSystem` trait: `judge(note, press_dt) -> Judgement`, `on_empty_press`, `on_miss`, `combo_rule`, `life_rule(initial, per-judgement delta, fail/clear policy)`, `score(aggregate) -> (number, rank)`, with the chart format supplying system-specific parameters (DanOni gauge/difData, FFR judge window) and the time base (30 vs 60 fps frames) supplying the window units.

---

### Source list

- rCubed repository: https://github.com/flashflashrevolution/rCubed (files: `src/Constant.as`, `src/GlobalVariables.as`, `src/URLs.as`, `src/classes/chart/Note.as`, `NoteChart.as`, `Song.as`, `NoteMod.as`, `src/classes/chart/parse/ChartFFRLegacy.as`, `ChartStepmania.as`, `src/com/flashfla/media/Beatbox.as`, `src/game/GameplayDisplay.as`, `src/game/GameOptions.as`, `src/game/controls/Judge.as`, `LifeBar.as`, `src/popups/settings/SettingsTabGeneral.as`)
- FFR timing thread: https://ffr.dance/vbz/home/forum/flash-flash-revolution/ffr-general-talk/48518-ffr-s-timing-system
- FFR FAQ: https://www.flashflashrevolution.com/vbz/faq.php?faq=ffrfaq ; score formula thread: https://www.flashflashrevolution.com/vbz/node/25839 ; AA definition thread: https://ffr.dance/vbz/home/forum/flash-flash-revolution/ffr-general-talk/127961-definition-of-a-ffr-aa
- FFR wiki Note colors: https://ffr.fandom.com/wiki/Note ; artist permissions: https://ffr.fandom.com/wiki/Category:Artists_Permissions
- danoniplus repository: https://github.com/cwtickle/danoniplus (files: `js/lib/danoni_constants.js`, `js/lib/mainWindow.js`, `js/lib/result.js`, `js/lib/settings.js`, `js/lib/dataLoader.js`, `js/lib/dosConverter.js`, `js/danoni_main.js`, `danoni/danoni1.html`, `danoni/dos/0001_ThrustUp.txt`, `danoni/danoniA.txt`, `LICENSE`, `README.md`)
- danoniplus docs wiki: https://github.com/cwtickle/danoniplus-docs/wiki — pages `dos_header`, `dos_score`, `dos_effect`, `dos_setting`, `AboutJudgment`, `AboutGameSystem`, `AboutFrameProcessing`, `SpeedChange`, `keys`, `dos-e0001-speedData`, `dos-e0002-colorData`, `dos-e0002-ncolorData`, `dos-e0003-wordData`, `dos-e0004-animationData`, `dos-e0005-motionData`, `dos-e0006-keychData`, `dos-e0007-scrollchData`, `dos-e0008-styleData`, `dos-h0001`…`dos-h0105`, `dos-s0003-initialGauge`
- Community wiki: https://wikiwiki.jp/danoniplus/
- DDR references: https://remywiki.com/DanceDanceRevolution_Scoring_System ; https://scrapbox.io/0b5vr/Timing_Window
