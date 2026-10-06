# Chart format research: .sm / .ssc / .dwi (plus KSF, BMS, osu, SMA)

Primary sources used: StepMania `5_1-new` source (`src/NotesLoaderSM.cpp`, `NotesLoaderSSC.cpp`, `NotesLoaderSSC.h`, `NotesLoaderDWI.cpp`, `NoteDataUtil.cpp`, `NoteTypes.h`, `TimingData.cpp`, `TimingSegments.h`, `GameManager.cpp`), the StepMania GitHub wiki, StepMania's `Docs/SimfileFormats/*` (which contains the original DWI readme), the Project OutFox wiki, and the `simfile` Python library docs. All file references below are to `https://github.com/stepmania/stepmania/blob/5_1-new/...`.

Shared lexical layer (MSD): every format here is "MSD": `#TAG:param1:param2:...;` with `:` separating parameters and `;` terminating; `//` starts a comment ([DWI.txt](https://github.com/stepmania/stepmania/blob/5_1-new/Docs/SimfileFormats/DWI/DWI.txt), [OutFox SM page](https://outfox.wiki/en/dev/mode-support/sm-support)). Internally StepMania quantizes beats to integer rows with `ROWS_PER_BEAT = 48` (`NoteTypes.h`), so a 4/4 measure is 192 rows.

---

## 1. StepMania `.sm`

Sources: [wiki/sm](https://github.com/stepmania/stepmania/wiki/sm), [OutFox SM support](https://outfox.wiki/en/dev/mode-support/sm-support), [NotesLoaderSM.cpp](https://github.com/stepmania/stepmania/blob/5_1-new/src/NotesLoaderSM.cpp), [NoteDataUtil.cpp](https://github.com/stepmania/stepmania/blob/5_1-new/src/NoteDataUtil.cpp).

### 1.1 Header tags (all handled by `SMLoader`)

| Tag | Value / semantics |
|---|---|
| `#TITLE`, `#SUBTITLE`, `#ARTIST` | Strings. Title falls back to directory name. |
| `#TITLETRANSLIT`, `#SUBTITLETRANSLIT`, `#ARTISTTRANSLIT` | Romanized variants (used when `ShowNativeLanguage` is off). |
| `#GENRE`, `#CREDIT` | Strings (credit = chart/pack author). |
| `#BANNER`, `#BACKGROUND`, `#CDTITLE`, `#LYRICSPATH` | Relative file paths (lyrics = `.lrc`). |
| `#MUSIC` | Relative audio path. |
| `#INSTRUMENTTRACK` | `file=Guitar,file=Drum1,...` (rare). |
| `#OFFSET` | Seconds. **Sign:** loader does `m_fBeat0OffsetInSeconds = StringToFloat(param)` and the timing engine starts with `last_time = -m_fBeat0OffsetInSeconds` (`TimingData.cpp` `GetElapsedTimeFromBeatNoOffset`). So **time(beat 0) = −OFFSET** in music time. `#OFFSET:-0.611;` means beat 0 is 0.611 s into the audio; a positive OFFSET puts beat 0 before the audio starts. |
| `#SAMPLESTART`, `#SAMPLELENGTH` | Seconds, preview clip. |
| `#DISPLAYBPM` | `n`, `min:max`, or `*` (random). |
| `#SELECTABLE` | `YES`/`NO` (also legacy `ROULETTE`, `ES`, `OMES`, `1`/`0`). |
| `#BPMS` | `beat=bpm,beat=bpm,...`. Zero BPM entries are ignored; negative BPMs are legal in .sm and are **converted to warps** (see 2.4). |
| `#STOPS` (alias `#FREEZES`) | `beat=seconds,...`. Negative stops also legal, converted to warps. |
| `#DELAYS` | `beat=seconds,...` (SM5 reads it in .sm too). |
| `#TIMESIGNATURES`, `#TICKCOUNTS` | Read by SM5's SM loader (same syntax as SSC). |
| `#BGCHANGES` (+`#BGCHANGES2..` for layers, legacy `#ANIMATIONS`) | `beat=file=rate=crossfade=stretchrewind=stretchnoloop=effect=file2=transition=color1=color2` (up to 11 fields; colors `r^g^b^a` in 0..1 or `#rrggbbaa`). Beat may be negative. |
| `#FGCHANGES` | Same syntax as BGCHANGES, foreground layer. |
| `#KEYSOUNDS` | Comma list of audio files; index referenced by `[n]` suffixes in note data. |
| `#ATTACKS` | `TIME=sec:LEN=sec` or `END=sec` `:MODS=modstring`, entries separated by `:`; **seconds-based**, not beats. |
| `#NOTES` / `#NOTES2` | Chart blocks; `NOTES2` is written when keysounds exist, read identically. |
| Cache-only / deprecated | `#MUSICLENGTH`, `#MUSICBYTES` (ignored), `#FIRSTBEAT`, `#LASTBEAT`, `#LASTBEATHINT`, `#SONGFILENAME`, `#HASMUSIC`, `#HASBANNER`, `#SAMPLEPATH`, `#LEADTRACK`; `#MENUCOLOR` (SM 3.9 Plus, unsupported). |

### 1.2 `#NOTES` block

`#NOTES:stepstype:description:difficulty:meter:radar:notedata;` — the loader requires `iNumParams >= 7` (tag name + 6 fields).

1. **Steps type** — e.g. `dance-single`.
2. **Description** — free text (author). Legacy rule: if description is `smaniac` or `challenge` on a Hard chart it is loaded as Challenge.
3. **Difficulty** — `Beginner|Easy|Medium|Hard|Challenge|Edit` (DWI-compatible aliases also accepted: basic/light→Easy, another/trick/standard/difficult→Medium, ssr/maniac/heavy→Hard, smaniac/expert/oni→Challenge; see `DwiCompatibleStringToDifficulty`).
4. **Meter** — integer.
5. **Radar values** — 5 comma-separated floats, order Stream, Voltage, Air, Freeze, Chaos (OutFox); regenerated on save, safe to ignore on import.
6. **Note data** — measures separated by `,`; rows separated by newlines; one character per column; terminated by `;`.

**Measure/row structure:** each measure is 4 beats (the SM loader assumes 4/4 for note data layout regardless of `#TIMESIGNATURES`). A measure with N rows places row i at beat `4*i/N` → `ROWS_PER_MEASURE/N` internal rows apart (`NoteDataUtil::LoadFromSMNoteDataString`). Supported quantizations (`NoteType` enum): 4th, 8th, 12th, 16th, 24th, 32nd, 48th, 64th, 192nd — i.e. N ∈ {4,8,12,16,24,32,48,64,192}; other N are snapped by `BeatToNoteRow` rounding (position = rational `i/N` of a measure, which an importer should keep exact).

**Note characters** (`NoteDataUtil.cpp` parse switch; `NoteTypes.h`):

| Char | Meaning | Notes |
|---|---|---|
| `0` | empty | |
| `1` | tap | |
| `2` | hold head | duration set when a `3` is found later in the same column |
| `4` | roll head | same pairing as `2` |
| `3` | hold/roll tail | ends the most recent unterminated `2`/`4` in that column; a `3` without a head logs a warning; unmatched heads are deleted on tidy-up |
| `M` | mine | |
| `L` | lift | release-to-hit |
| `F` | fake | tap drawn but never judged; "can only be fake taps, no fake holds" (OutFox) |
| `K` | auto-keysound | plays its keysound, not judged |
| `A` | attack note (writer emits `A{mods:seconds}`; parsing of `{}` is currently disabled in SM5) | |
| `[n]` suffix | keysound index into `#KEYSOUNDS` (0-based), e.g. `1[3]` | |
| `{mods:len}` suffix | per-note attack | |
| `&` | separates per-player note data in composite (routine) charts | |

Hold pairing: a `2`/`4` receives `iDuration = MAX_NOTE_ROW` then `iDuration = tailRow - headRow` when its `3` is encountered. Nothing distinguishes a roll tail from a hold tail; the head defines the kind.

### 1.3 Steps types and column order

From `GameManager.cpp` (`StepsTypeInfo` table and dance `Style` definitions, `m_iInputColumn` arrays indexed by DanceButton order LEFT, RIGHT, UP, DOWN, UPLEFT, UPRIGHT):

| Steps type | Tracks | Column order (index → panel) |
|---|---|---|
| `dance-single` | 4 | 0 L, 1 D, 2 U, 3 R (`{0,3,2,1}` = LEFT→0, RIGHT→3, UP→2, DOWN→1) |
| `dance-double` | 8 | P1 L,D,U,R then P2 L,D,U,R (one player, both pads) |
| `dance-couple` | 8 | same layout; columns 0–3 are P1, 4–7 are P2 |
| `dance-routine` | 8 | same layout; shared field, two players; note data uses `&`-separated composite |
| `dance-solo` | 6 | 0 L, 1 UL, 2 D, 3 U, 4 UR, 5 R (`{0,5,3,2,1,4}`; confirmed by `NotesLoaderDWI.cpp`'s solo column map) |
| `dance-threepanel` | 3 | 0 UL, 1 D, 2 UR (`{0,2,NO_MAPPING,1,0,2}`) |
| Others | | `pump-single`(5) `pump-halfdouble`(6) `pump-double`(10) `pump-couple`(10) `pump-routine`(10), `kb7-single`, `ez2-single/double/real`, `para-single`, `ds3ddx-single`, `bm-single5/versus5/double5/single7/versus7/double7`, `maniax-single/double`, `techno-single4/5/8`, `techno-double4/5/8`, `pnm-five/nine`, `lights-cabinet`, `kickbox-human/quadarm/insect/arachnid` |

---

## 2. StepMania `.ssc`

Sources: [wiki/ssc](https://github.com/stepmania/stepmania/wiki/ssc), [Docs/Changelog_SSCformat.txt](https://github.com/stepmania/stepmania/blob/5_1-new/Docs/Changelog_SSCformat.txt), [Docs/SimfileFormats/ssc_msd5.txt](https://github.com/stepmania/stepmania/blob/5_1-new/Docs/SimfileFormats/ssc_msd5.txt), [NotesLoaderSSC.cpp](https://github.com/stepmania/stepmania/blob/5_1-new/src/NotesLoaderSSC.cpp), [NotesLoaderSSC.h](https://github.com/stepmania/stepmania/blob/5_1-new/src/NotesLoaderSSC.h), [TimingSegments.h](https://github.com/stepmania/stepmania/blob/5_1-new/src/TimingSegments.h), [TimingData.cpp](https://github.com/stepmania/stepmania/blob/5_1-new/src/TimingData.cpp).

### 2.1 Structure and new song-level tags

An `.ssc` is the `.sm` header plus: `#VERSION` (first line, two decimals; current writer emits 0.83), `#ORIGIN`, `#PREVIEWVID`, `#JACKET`, `#CDIMAGE`, `#DISCIMAGE`, `#PREVIEW` (separate preview audio), `#MUSICLENGTH`, `#LASTSECONDHINT` (float seconds; song end if charts are shorter; required when only Edit charts exist), plus the full timing set `#BPMS #STOPS #DELAYS #WARPS #TIMESIGNATURES #TICKCOUNTS #COMBOS #SPEEDS #SCROLLS #FAKES #LABELS`. Cache-only: `#FIRSTSECOND #LASTSECOND #SONGFILENAME #HASMUSIC #HASBANNER #STEPFILENAME`.

Charts are no longer one `#NOTES` tag; each chart is a `#NOTEDATA:;` block followed by its own tags:

| Per-chart tag | Semantics |
|---|---|
| `#CHARTNAME` | Chart title (v0.74+; before that `#DESCRIPTION` is copied into it) |
| `#STEPSTYPE` | e.g. `dance-single` |
| `#DESCRIPTION`, `#CHARTSTYLE`, `#CREDIT` | free text |
| `#DIFFICULTY`, `#METER` | as .sm |
| `#RADARVALUES` | auto-generated (longer list than .sm; includes note counts in v0.83+); ignore on import |
| `#MUSIC` | per-chart audio override |
| `#DISPLAYBPM` | per-chart (v0.75+) |
| `#OFFSET #BPMS #STOPS #DELAYS #WARPS #TIMESIGNATURES #TICKCOUNTS #COMBOS #SPEEDS #SCROLLS #FAKES #LABELS #ATTACKS` | **split timing** (v0.70+): if a chart has *any* timing tag, the chart's timing fully replaces the song's; otherwise the song timing is used |
| `#NOTES` / `#NOTES2` | note data (same character set and measure layout as .sm) |

Version thresholds in `NotesLoaderSSC.h`: `VERSION_RADAR_FAKE 0.53`, `VERSION_WARP_SEGMENT 0.56`, `VERSION_SPLIT_TIMING 0.70`, `VERSION_OFFSET_BEFORE_ATTACK 0.72`, `VERSION_CHART_NAME_TAG 0.74`, `VERSION_CACHE_SWITCH_TAG 0.77`, `VERSION_RADAR_NOTECOUNT 0.83`. Per-chart timing tags are only honored when `version >= 0.70`.

### 2.2 Timing tag syntaxes (all `beat=...` lists, comma separated)

| Tag | Entry | Validation / notes (from `NotesLoaderSM.cpp`/`SSC.cpp` Process* functions) |
|---|---|---|
| `#BPMS` | `beat=bpm` | SSC requires `beat >= 0 && bpm > 0` (no negative-BPM gimmicks in SSC); `.sm` path converts negatives to warps |
| `#STOPS` | `beat=seconds` | `beat >= 0 && seconds > 0` in SSC |
| `#DELAYS` | `beat=seconds` | "pump style stop" |
| `#WARPS` | `beat=length_in_beats` | **relative** length since v0.70; for `version < 0.70` the second value is the absolute destination beat and the loader converts (`WarpSegment(beat, end-beat)`) |
| `#TIMESIGNATURES` | `beat=num=den` | both ≥ 1; default `0=4=4`; affects measure lines/editor only |
| `#TICKCOUNTS` | `beat=ticks_per_beat` | checkpoint-hold tick rate (pump); default 4 per OutFox, writer emits `0=2`... treat as data |
| `#COMBOS` | `beat=combo[=missCombo]` | per-note combo multiplier; miss combo defaults to combo value |
| `#SPEEDS` | `beat=ratio=delay=unit` | `unit` 0 = beats, 1 = seconds; 2-value and 3-value entries are padded with `0` (so `beat=ratio` and `beat=ratio=delay` are accepted); `delay >= 0` |
| `#SCROLLS` | `beat=ratio` | |
| `#FAKES` | `beat=length_in_beats` | `length > 0` |
| `#LABELS` | `beat=string` | default `0=Song Start` |

Defaults added by `TimingData::TidyUpData` if missing: BPM `0=60`, first BPM forced to row 0, `TIMESIGNATURES 0=4=4`, `TICKCOUNTS 0`, `COMBOS 0`, `LABELS 0=Song Start`, `SPEEDS 0=1=0=0`, `SCROLLS 0=1`.

### 2.3 Semantics

**Stop vs. delay.** Both pause the clock for `seconds` at a row; they differ in whether the notes *on that row* are judged before or after the pause. In `TimingData::FindEvent` the search order at an equal row is: warp-destination, BPM change, **delay**, *marker (the queried beat)*, **stop**, warp. Because the marker is checked *after* delays but *before* stops, `GetElapsedTimeFromBeat(b)` for a row carrying a delay returns the time **after** the delay has elapsed, and for a row carrying a stop returns the time **before** the stop begins. In plain terms: STOP = arrows reach the receptor, you hit them, then everything freezes (DDR/ITG style); DELAY = everything freezes first, then the arrows at that beat become hittable (Pump style). The `simfile` Python docs state the same rule: "delays must occur before stops in order to correctly time notes on a beat with both a delay and a stop" ([simfile timing engine](https://docs.simfile.dev/en/main/autoapi/simfile/timing/engine/index.html)). When both exist on one row, the delay is applied first (`FOUND_STOP_DELAY` falls through delay → stop).

**Warps.** `WarpSegment(row, lengthBeats)`: "A warp segment is used to replicate the effects of Negative BPMs without abusing negative BPMs" (`TimingSegments.h`). During a warp zero time passes (`time_to_next_event = 0` while `is_warping`); overlapping warps extend the destination (`warp_destination = max(...)`). Rows inside `[start, start+length)` are not judgable (`IsJudgableAtRow = !IsWarpAtRow && !IsFakeAtRow`) **unless** a stop or delay sits exactly on that row (`IsWarpAtRow` returns false there, "to allow things like stop, warp, stop, warp"). Notes inside a warp are effectively skipped; StepMania marks them fake.

**Negative BPM / negative stop conversion (`SMLoader::ProcessBPMsAndStops`).** BPMs and stops are merged in beat order (BPM wins ties). A BPM `< 0` or `> FAST_BPM_WARP (9,999,999)` starts a warp at that beat; a negative stop also starts a warp with `timeofs = stopSeconds`. While warping, `timeofs += Δbeat * 60/bpm` (skipped for "infinite" BPM) and `timeofs += stop`. When `timeofs` becomes positive at a positive BPM, the warp ends at `warpend = beat - timeofs*bpm/60` and a `WarpSegment(warpstart, warpend-warpstart)` is emitted (plus a `BPMSegment` at warpstart if the BPM changed during the warp). A positive stop that overshoots the deficit ends the warp and emits a `StopSegment` for the excess. Stops before beat 0 are folded into the offset (`m_fBeat0OffsetInSeconds -= stop`); BPMs before beat 0 are dropped. A still-open warp at EOF with infinite BPM gets `warpend = 99999999`.

**Speeds vs. scrolls.** `ScrollSegment(ratio)` rescales *displayed beat distance*: `GetDisplayedBeat(b) = Σ over segments (Δbeat × ratio)` — i.e. it compresses or stretches the note field spacing after that beat without changing timing (it "fakes BPM changes and stops", per the changelog). `SpeedSegment(ratio, delay, unit)` multiplies the player's scroll speed ("Step's BPM × speed mod × ratio") and *interpolates* linearly from the previous ratio to the new one over `delay` **beats** (`unit = 0`) or **seconds** (`unit = 1`), measured from the segment's start time (`GetDisplayedSpeedPercent`; start/end times are taken `- GetDelayAtBeat`). A ratio of 1 resets. Both "were inspired by the Pump It Up series".

**Fakes.** `FakeSegment(row, lengthBeats)`: "the contents inside are neither for nor against the player... Unlike the Warp Segments, these are not magically jumped over: instead, these are drawn normally." Rows in `[start, start+length)` are unjudged.

### 2.4 Beat → time algorithm (what `GetElapsedTimeInternal` does)

```
t = -OFFSET; row = 0; bps = BPM(row 0)/60; warping = false; warp_dest = -inf
loop:
  pick the next event with smallest row among: warp_dest (if warping), next BPM, next DELAY,
  the target beat (MARKER), next STOP, next WARP — with ties resolved in that order
  t += warping ? 0 : (event_row - row)/48 / bps
  match event:
    WARP_DEST  -> warping = false
    BPM        -> bps = new/60
    STOP       -> t += stop.seconds          (STOP_DELAY at same row: handled as stop here, delay separately)
    DELAY      -> t += delay.seconds
    MARKER     -> return t
    WARP       -> warping = true; warp_dest = max(warp_dest, warp.beat + warp.length)
  row = event_row
```
Time→beat (`GetBeatInternal`) mirrors this; while inside a stop or delay's pause, the reported beat is the segment's beat and flags `freeze_out`/`delay_out` are set. Global offset and music rate are applied outside (`GetElapsedTimeFromBeat` subtracts `rate × GlobalOffsetSeconds`). A 16-segment lookup table (`PrepareLookup`) accelerates mid-song queries.

---

## 3. DWI (Dance With Intensity)

Sources: original DWI readme as mirrored in StepMania [Docs/SimfileFormats/DWI/DWI.txt](https://github.com/stepmania/stepmania/blob/5_1-new/Docs/SimfileFormats/DWI/DWI.txt) (from `dwi.ddruk.com/readme.php#4`), and [NotesLoaderDWI.cpp](https://github.com/stepmania/stepmania/blob/5_1-new/src/NotesLoaderDWI.cpp).

### 3.1 Header tags

| Tag | Semantics (readme) | StepMania loader behavior |
|---|---|---|
| `#TITLE` | title | split into main/sub title; encoding unknown, converted "utf-8,english" |
| `#ARTIST`, `#GENRE` | strings (genre may be comma list) | |
| `#GAP` | "number of milliseconds that pass before the program starts counting beats" | `offset = -StringToInt(gap)/1000` — **integer ms; positive GAP = beat 0 is later in the audio**, i.e. `OFFSET = -GAP/1000` |
| `#BPM` | initial BPM | `BPMSegment(0, bpm)`; must be > 0 |
| `#CHANGEBPM` (alias `#BPMCHANGE`) | `BBB=nnn,...` "at 'beat' BBB the speed... will change to a new BPM of nnn" | **beat value is divided by 4**: `BeatToNoteRow(StringToFloat(b)/4)` — DWI "beats" are quarter-beats (16th-note units); BPM must be > 0 |
| `#FREEZE` | `BBB=sss,...` "at 'beat' BBB, the motion of the arrows should stop for sss milliseconds" | `StopSegment(BeatToNoteRow(b/4), ms/1000)` — same /4 and ms→s |
| `#FILE` | music path; fallback: any .wav/.mp3 beside the .dwi | |
| `#MD5` | music checksum | ignored |
| `#DISPLAYTITLE`, `#DISPLAYARTIST` | alternate names with `{image.png}` glyphs | unsupported; image names are blacklisted from banner auto-detection |
| `#DISPLAYBPM` | `*`, `a`, or `a..b` | parsed with `%i..%i` |
| `#CDTITLE` | 64×40 image | |
| `#SAMPLESTART`, `#SAMPLELENGTH` | ms (`5230`), s (`5.23`) or `m:ss.ss`; `+` prefix factors in GAP | `ParseBrokenDWITimestamp`: value with `.` → seconds, else ms; 2–3 colon parts → h:m:s; lengths in (0,1) are ×1000 |
| `#STATUS` (`NEW`/`NORMAL`), `#RANDSEED`, `#RANDSTART`, `#RANDFOLDER`, `#RANDLIST` | song-select/visual options | ignored |
| `#BACKGROUND:` … `#END;` | DWI animation script (layers, MOVIE/VIS/FILE effects, SCRIPT string using step-timing brackets) | ignored by StepMania |

### 3.2 Chart tags

`#SINGLE|DOUBLE|COUPLE|SOLO:DIFFICULTY:METER:steps[:steps_pad2];` — "In doubles, the left pad's steps are given first, then the right pad's, separated by a colon." Difficulty names per the readme: `BASIC`, `ANOTHER`, `MANIAC`, `SMANIAC`; StepMania additionally accepts `BEGINNER`, and all the aliases listed in §1.2 (`DwiCompatibleStringToDifficulty`). Mapping: SINGLE→`dance-single`, DOUBLE→`dance-double`, COUPLE→`dance-couple`, SOLO→`dance-solo`.

### 3.3 Step encoding

Numeric-keypad layout (readme, verified against `DWIcharToNote`):

| Char | Panels | Char | Panels (solo) |
|---|---|---|---|
| `0` | none | `C` | UpLeft |
| `1` | Down+Left | `D` | UpRight |
| `2` | Down | `E` | Left+UpLeft |
| `3` | Down+Right | `F` | UpLeft+Down |
| `4` | Left | `G` | UpLeft+Up |
| `5` | none (StepMania treats as empty) | `H` | UpLeft+Right |
| `6` | Right | `I` | Left+UpRight |
| `7` | Up+Left | `J` | Down+UpRight |
| `8` | Up | `K` | Up+UpRight |
| `9` | Up+Right | `L` | UpRight+Right |
| `A` | **Up+Down** | `M` | UpLeft+UpRight |
| `B` | **Left+Right** | | |

Note the prompt had A/B swapped: the readme says "(U+D = A and L+R = B)" and the loader agrees. Any other char logs "invalid DWI note character". For the second pad (double/couple) the loader offsets to PAD2 notes; columns: single `L,D,U,R` = 0..3; double/couple = 0..3 then 4..7; solo `L,UL,D,U,UR,R` = 0..5.

**Timing:** "Each character defaults to one 1/8 of a beat" — in practice (and in the loader) each character advances `1/8 × 4 = 0.5 beat` (an 8th note). Brackets change the increment until the matching closer resets it to 1/8-of-measure: `(...)` = 1/16 notes (0.25 beat), `[...]` = 1/24 (1/6 beat), `{...}` = 1/64 (1/16 beat), `` `...' `` = 1/192 (1/48 beat). Closers `)` `]` `}` `'` and `>` all reset to the default.

**Jumps/grouping:** `<...>` joins several codes into one beat ("`<MB>` = Left, Right, UpLeft, UpRight"). Historical ambiguity: `<...>` used to mean 1/192 notes; the loader's `Is192()` treats `<` as a 192nd marker if a `0` appears before the next `>`, otherwise as a jump.

**Holds:** `X!Y` = "show!hold": X is displayed, the panels in Y are held; "8!8" starts an up hold, "7!4" shows Up+Left but holds only Left. The hold "will be released the next time the program encounters an 'up' arrow: by itself or combined with another arrow". The loader implements this literally: after placing X's taps, if the next char is `!` it marks Y's columns as hold heads; afterwards, for each hold head, the next non-empty note in that column becomes the tail (that note is *removed*, `iDuration = tailRow - headRow`); unclosed holds are deleted with a warning. Consequently DWI cannot express rolls, mines, lifts, fakes, or a tap immediately following a hold in the same column.

Whitespace (`\n \r \t space`) is stripped from step strings before parsing. `//` comments are allowed anywhere.

---

## 4. Other formats (import difficulty notes)

**KSF (Pump It Up / Kick It Up)** — MSD-like, one difficulty per file, 13-digit step lines in panel order DL, UL, C, UR, DR (+P2 in positions 6–10); `#TICKCOUNT` = lines per beat; `#STARTTIME` in centiseconds with GAP sign (opposite of `#OFFSET`); BPM changes via paired `#BPMx`/`#BUNKIx` (time-based!), holds as repeated `4`s, `2222222222222` terminates ([ksf-format.txt](https://github.com/stepmania/stepmania/blob/5_1-new/Docs/SimfileFormats/KSF/ksf-format.txt); StepMania's `NotesLoaderKSF.cpp` also reads `#BPM2/#BPM3`, `#BUNKI2`, `#STARTTIME2/3`, `#DIFFICULTY`, `#PLAYER`, `#TITLEFILE`, `#DISCFILE`, `#SONGFILE`). Moderate effort; the time-based BPM changes and CP949 text encoding are the pain points; irrelevant to a 4/6-panel dance game unless pump modes are planned.

**BMS/.bme/.bml/.pms** — line-oriented `#mmmcc:data` (measure, channel, base-36 object pairs), `#WAVxx` keysound tables, channel 02 measure length, 03/08 BPM, 09 stops, 11–19/21–29 playable lanes, 51–59 long notes (`#LNTYPE`/`#LNOBJ`) ([hitkey BMS command memo](https://hitkey.nekokan.dyndns.info/cmds.htm)). Fully keysound-driven and implementation-defined in many corners; hard to import faithfully, low value for DDR-style play.

**.osu (osu!mania)** — INI-like sections; `[TimingPoints]` are *millisecond-based* (`time,beatLength,meter,...,uninherited,...`), hit objects are in ms with column `floor(x*columnCount/512)` and holds as type 128 with an end time ([osu! file format wiki](https://osu.ppy.sh/wiki/en/Client/File_formats/osu_%28file_format%29)). Simple to parse, but notes are time-anchored rather than beat-anchored, so mapping onto a beat/row model needs snapping; keysounds/hitsounds are per-note. Easy-to-moderate.

**.sma** — StepMania-3.95-era "sm-ssc precursor" read by `NotesLoaderSMA.cpp`; tags include `#SMAVERSION #BEATSPERMEASURE #ROWSPERBEAT #TICKCOUNT #SPEED #MULTIPLIER #FAKES #DELAYS #PREVIEW #LISTSORT`. Nearly dead in the wild; anything expressible in it maps to the SSC model, so low priority. The other MSD cousins (`.dance` pydance format, documented in `Docs/SimfileFormats/dance-spec.txt`; `.sdf` Pocket DDR) are also trivial text formats but have no living corpus.

**Keysounds / `.ogg` / `.mp3`** — keysound support (`#KEYSOUNDS`, `K` notes, `[n]` suffixes, `#NOTES2`) means playing one sample per note on hit; it is only used by BMS-derived and a handful of SM charts. For an MVP it is enough to parse and *preserve* the keysound index per note and the file list so round-tripping works, and simply never play them. Audio decoding (OGG Vorbis/MP3) is a separate concern from chart parsing; `#MUSIC` just names a file.

---

## 5. Existing parsers and reference implementations

| Project | Formats | License | Status (as of 2026-10) |
|---|---|---|---|
| [pnn64/rssp](https://github.com/pnn64/rssp) "Rust StepMania Simfile Parser" | .sm, .ssc, partial .crs; library + CLI, stats/pattern analysis, ITGmania-style chart hashes | MIT | Active (pushed 2026-10-05, ~1.2k commits) — the most alive Rust option; analysis-oriented rather than a clean chart model |
| [rg_formats](https://docs.rs/rg_formats) (in [zkldi/backbeat](https://github.com/zkldi/backbeat)) | .sm/.ssc/.msd, .dwi, BMS family, bmson, ksh/kson, dtx, tja, .chart | crate metadata says MIT; repo is GPL-3.0 — check before depending | Updated Sept 2026; small (48 commits in monorepo), experimental |
| [rgchart](https://github.com/R2O3/rgchart) (crate `rgchart`, formerly `rgc-chart`) | osu!, StepMania .sm, Quaver, fluXis; converts to a generic mania chart; WASM | MIT | v0.0.15, pushed 2026-06-19 |
| [danceparser](https://codeberg.org/nobbele/danceparser) (crates.io 0.2.1) | .sm only | see repo LICENSE | 5 commits, June 2026; nascent |
| [msd](https://github.com/Anders429/msd) crate 0.4.0 | MSD lexical layer with serde | MIT OR Apache-2.0 | Last release 2023-05 |
| [RGates94/rustmania](https://github.com/RGates94/rustmania) | .sm (game engine) | MIT | Dormant (last push 2021-04) |
| [barrysir/stepmania-parsing-code](https://github.com/barrysir/stepmania-parsing-code) | C++ extraction of SM's loaders (.sm/.ssc/.dwi/.json/.lrc) for 100% identical parsing | MIT | Declared defunct; useful as a reference oracle |
| [simfile (Python)](https://docs.simfile.dev/) | .sm/.ssc with a well-documented `TimingEngine` | MIT | Maintained; its docs are the clearest prose on stop/delay/warp semantics |

StepMania reference files (branch `5_1-new`): `src/NotesLoaderSM.cpp` (+`.h` for `FAST_BPM_WARP`), `src/NotesLoaderSSC.cpp` (+`.h` for version constants), `src/NotesLoaderDWI.cpp`, `src/NotesLoaderKSF.cpp`, `src/NotesLoaderBMS.cpp`, `src/NotesLoaderSMA.cpp`, `src/NoteDataUtil.cpp` (note-data string parse/write), `src/NoteTypes.h`, `src/TimingData.cpp`, `src/TimingSegments.h`, `src/GameManager.cpp` (steps types/columns), `src/MsdFile.cpp` (lexer). Format docs: [wiki/sm](https://github.com/stepmania/stepmania/wiki/sm), [wiki/ssc](https://github.com/stepmania/stepmania/wiki/ssc), `Docs/Changelog_SSCformat.txt`, `Docs/SimfileFormats/`. **Forks:** ITGmania carries the same `Docs/SimfileFormats` and `Changelog_SSCformat.txt` (checked via the GitHub contents API) and no separate format spec; Project OutFox's wiki has a thorough [SM page](https://outfox.wiki/en/dev/mode-support/sm-support) (the only format page under `/en/dev/mode-support/` besides bms-pms, dtx-gda, ksf, oto, tja, txt — there is no SSC page). Note the OutFox page is JavaScript-rendered; fetch with a browser UA.

---

## 6. Freely licensed test simfiles

- **StepMania bundled songs** — `Songs/StepMania 5/{Goin' Under, MechaTribe Assault, Springtime}` in the repo ([listing](https://github.com/stepmania/stepmania/tree/5_1-new/Songs/StepMania%205)); `.ssc` v0.83 with full timing tags (e.g. `#SPEEDS:0=1=0=0; #SCROLLS:0=1; #LABELS:0=Song Start;`). Caveat: `Docs/Licenses.txt` covers only code; I found no explicit license for these audio/chart files, so treat them as "redistributed by the MIT-licensed project" rather than clearly CC. ITGmania ships the same three.
- **StepMania `Docs/SimfileFormats/SDF/test.sm`** — a small in-repo `.sm` fixture.
- **OutFox Serenity** — contributors must license charts and graphics **CC-BY 3.0/4.0** and music under "a free content license that allows for unrestricted distribution, modification, and commercial use" ([guidelines](https://projectoutfox.com/serenity-guidelines)). Best fit for permissive fixtures; packs are distributed via OutFox's channels.
- **Sockpuppet: Song and Dance** — 8-song `.sm` pack, "Creative Commons Attribution-ShareAlike v4.0 International" ([fluffy.itch.io/sockmania](https://fluffy.itch.io/sockmania)).
- **Club Fantastic** (Seasons 1–2, 32 tracks, `.ssc`) — StepMania packs at `download.clubfantastic.dance`; site states "Graphical art is free under the Creative Commons BY-NC 4.0 license" and "music and art may not be sold or included in any commercial product without consent of the creators" ([clubfantastic.dance](https://clubfantastic.dance/)). Fine for private test fixtures, **not** for redistribution in a repo without checking.
- **RustMania** repo ships "Mu" by Solarbear under CC-SA 3.0 as a sample ([repo](https://github.com/RGates94/rustmania)).
- **DWI fixtures:** I found no freely licensed `.dwi` corpus; hand-write small `.dwi` files from the spec above (and/or convert a CC `.sm` by hand) and cross-check against StepMania's or `stepmania-parsing-code`'s output.

---

## 7. Implications for an internal chart model

To import SM + SSC + DWI losslessly the model must represent:

**Positions and quantization**
- Note positions as **exact rationals in beats** (measure index + `i/N` of a 4-beat measure). Keep the rational (or at least a 48-rows-per-beat integer like StepMania) so 12th/24th/48th/192nd notes round-trip; DWI contributes 1/2, 1/4, 1/6, 1/16, 1/48-beat steps and SM contributes anything with N | 192.
- Measure concept is 4 beats for note-data layout regardless of time signature (SM behavior); `#TIMESIGNATURES` is display metadata.

**Timing (all keyed by beat, ordered; multiple segment types may share a beat)**
- Song offset: seconds, stored in SM sign convention (`time(beat0) = -offset`); DWI `GAP` ms → `-GAP/1000`.
- BPM segments (`beat → bpm > 0`).
- Stops (`beat → seconds`, pause **after** notes on that beat are judged).
- Delays (`beat → seconds`, pause **before** notes on that beat are judged); apply delay before stop when both are present.
- Warps (`beat → length in beats`, skipped region, zero time, notes unjudgable unless a stop/delay sits on the row). Importer must run the negative-BPM/negative-stop → warp conversion for `.sm` and the absolute→relative fix for SSC < 0.70.
- Speed segments (`beat → ratio, delay, unit ∈ {beats, seconds}`) with linear interpolation.
- Scroll segments (`beat → ratio`) affecting displayed beat distance only.
- Fake segments (`beat → length in beats`).
- Tick counts (`beat → ticks/beat`), combo segments (`beat → combo, missCombo`), time signatures (`beat → num/den`), labels (`beat → string`).
- Both song-level timing and optional **per-chart override** (SSC split timing: if a chart has any timing tag, the whole set is replaced).
- Display BPM: `single | range(min,max) | random`, song-level and per-chart.

**Charts**
- Steps type (string from the GameManager list) with its track count and panel semantics; column order per style as in §1.3 (dance-single L,D,U,R; dance-solo L,UL,D,U,UR,R; threepanel UL,D,UR; double/couple/routine 2×4).
- Difficulty slot (Beginner/Easy/Medium/Hard/Challenge/Edit) + meter + description + chart name + chart style + credit; DWI/legacy difficulty aliases normalized on import.
- Per-player note data for routine (`&`-separated composite).
- Note kinds: tap, hold head (with duration), roll head (with duration), mine, lift, fake tap, auto-keysound, attack note (`{mods:seconds}`), plus an optional **keysound index** on any note. Tails are derived, not stored. DWI only yields taps and holds.
- Radar values: ignore (regenerated).

**Song metadata**
- Title/subtitle/artist (+translits), genre, credit, origin, music path, preview audio, sample start/length (seconds), banner/background/CD title/jacket/CD image/disc image/preview video/lyrics path, selectable flag, last-second hint, music length, instrument tracks.
- BG/FG change lists (11-field records, beat-keyed, possibly negative beats, multiple layers) and seconds-keyed attack lists — preserve as opaque structured records for round-trip even if unused.
- `#KEYSOUNDS` file list.
- SSC `#VERSION` (so writers can emit 0.83 and readers can branch on <0.70 warps / <0.74 chart names).

**Lossy-on-purpose from DWI:** `#DISPLAYTITLE/#DISPLAYARTIST` glyph images, `#BACKGROUND` animation scripts, `#STATUS`, `#RAND*`, `#MD5` — keep as an "unknown tags" bag if byte-level round-trip of DWI is ever wanted; otherwise drop.
