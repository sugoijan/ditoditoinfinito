# Announcer packs: StepMania's format, its forks and the packs people share (2026-11-03)

Scope: import StepMania-family announcer packs and play them as those
engines do, the plan being `docs/plans/announcers.md`. Same conventions as
the other notes: sources cited, GPL code described and never transcribed,
third-party packs listed by file name only and never copied into the
repository.

Retrieved 2026-11-03. All source read from anonymous shallow clones:

| Tree | Ref / commit | Licence |
|---|---|---|
| StepMania 5.1 | `stepmania/stepmania` branch `5_1-new`, commit `825467bc` | MIT-style (Chris Danford et al.), quotable |
| StepMania 5.0 | tag `v5.0.12` (two files only) | same |
| StepMania 3.9 | tag `v390release` (`stepmania/src/...`) | same |
| StepMania 3.0 | tag `v300-final` (ScreenGameplay/AnnouncerManager only) | same |
| Etterna | `etternagame/etterna` `master` = tag `v0.75.1`, commit `2d706147` | **MIT** (`LICENSE`: "Copyright (c) 2016-2023 Etterna", standard MIT text), quotable |
| ITGmania | `itgmania/itgmania` `release` = tag `v1.3.0`, commit `8a3bf888` | **GPL-3.0** (`LICENSE`): behavior described only |
| Simply Love (ITGmania's bundled theme, submodule) | `Simply-Love/Simply-Love-SM5` `dd06138` | GPL-3.0 (described only) |
| OpenITG | `openitg/openitg` `f2c129f` | SM 3.95-derived; described only |
| Project OutFox | engine source **not public** (the `TeamRizu/OutFox` repo is an issue tracker); wiki `TeamRizu/OutFox-Wiki` branch `gh-pages`, page `dev/announcers/index.html` | docs only |

Line numbers below refer to those commits. "SM5.1" means the `5_1-new` tree.

---

## 1. StepMania 5.1 `AnnouncerManager`

### 1.1 On-disk layout

- Root: `Announcers/` (`src/AnnouncerManager.cpp:15`, `ANNOUNCERS_DIR = "Announcers/"`).
  Several roots are mounted onto that one virtual path: the program folder and
  the user data folder (`src/arch/ArchHooks/ArchHooks_Unix.cpp:389,416`,
  `ArchHooks_Win32Static.cpp:66,100`, `ArchHooks_MacOSX.mm:288,319,357`; on macOS
  also the app bundle's resources, read-only). Listings from all of them are merged.
- An announcer is a **directory** `Announcers/<Name>/` (`GetAnnouncerNames` lists
  `Announcers/*` with directories only, `:35-46`; CVS/SVN and Mac resource-fork
  entries are dropped).
- A line is a **directory** `Announcers/<Name>/<line folder>/` containing sound files.
  `GetPathTo` returns `AnnouncerPath + sFolderName + "/"` (`:114-146`), and playback
  lists only that directory, non-recursively.
- `Announcers/instructions.txt` (shipped) claims a line "can either be a single sound,
  or a folder containing many sounds". **The code does not support a single file**:
  `GetPathTo` only tests `DirectoryIsEmpty(<dir>/)`, and `DoPlayOnceFromDir` only lists
  `<dir>/*.ext`. A file `gameplay ready.ogg` in the announcer root is never found
  (from reading the code; not tested at runtime). Etterna's copy of the text says the same, and is wrong in the same way.

### 1.2 How a file is chosen

`GameSoundManager::DoPlayOnceFromDir` (`src/GameSoundManager.cpp:292-334`):

```cpp
GetDirListing( sPath + "*.mp3", arraySoundFiles );
GetDirListing( sPath + "*.wav", arraySoundFiles );
GetDirListing( sPath + "*.ogg", arraySoundFiles );
GetDirListing( sPath + "*.oga", arraySoundFiles );
```

- Extensions: **mp3, wav, ogg, oga** (matching is case-insensitive, see 1.4). No flac/opus, no `.redir`.
- 0 files → nothing; 1 file → that file.
- Several files: a **shuffle bag per directory path** (`g_DirSoundOrder`, a
  `std::map<RString, std::vector<int>>`, `:92`, `:316-333`): indices are shuffled, popped
  one at a time, and reshuffled once empty. So no file repeats until all have played (it
  can repeat across a reshuffle boundary). The bag lives for the process and is keyed by
  the path. Changelog: "Make announcers repeat their lines less" (`Docs/Changelog_sm5.txt:71`).
- **SM 5.0.12 and Etterna** pick uniformly at random each time (`RandomInt(size)`;
  `v5.0.12 src/GameSoundManager.cpp:303`; Etterna `GameSoundManager.cpp:345`). SM 3.9 uses `rand() % size`
  (`v390release GameSoundManager.cpp:258`, extensions mp3/wav/ogg only, no oga).
- `DoPlayOnce` (`:282-290`) creates a new `RageSound`, `Play(false)`,
  `DeleteSelfWhenFinishedPlaying()`. There is **no announcer-specific volume, no ducking
  of the music, and no interruption**: lines overlap freely with each other and with
  music. The only global control is the master `SoundVolume` preference
  (`src/RageSoundManager.cpp:186-190`). `DimMusic` exists, but no announcer path calls it.
- All announcer requests are queued (`PlayOnceFromAnnouncer` pushes onto
  `g_SoundsToPlayOnceFromAnnouncer`, `:809-815`) and resolved on the music thread
  (`StartQueuedSounds`, `:352-373`: `ANNOUNCER->GetPathTo(name)`, then `DoPlayOnceFromDir`).
  Some callers resolve synchronously with `SOUND->PlayOnceFromDir(ANNOUNCER->GetPathTo(..))`,
  which goes into the same kind of queue.
- `ScreenSelectMaster`'s "select difficulty challenge" instead uses a `RandomSample`
  (`src/RandomSample.cpp`, `LoadSoundDir`: mp3/oga/ogg/wav, shuffled, `PlayRandom`).

### 1.3 Alias table (complete, `src/AnnouncerManager.cpp:77-108`)

The requested name is on the left. If `<Name>/<requested>/` is empty or missing, each
alias row whose left side equals the request (case-insensitive) is tried, and the
right-hand folder is used if it is non-empty:

| Requested name | Fallback folder on disk |
|---|---|
| ScreenSelectDifficulty comment beginner | select difficulty comment beginner |
| ScreenSelectDifficulty comment easy | select difficulty comment easy |
| ScreenSelectDifficulty comment medium | select difficulty comment medium |
| ScreenSelectDifficulty comment hard | select difficulty comment hard |
| ScreenSelectDifficulty comment oni | select difficulty comment oni |
| ScreenSelectDifficulty comment nonstop | select difficulty comment nonstop |
| ScreenSelectDifficulty comment endless | select difficulty comment endless |
| ScreenSelectDifficulty intro | select difficulty intro |
| ScreenSelectStyle intro | select style intro |
| ScreenSelectStyle comment single | select style comment single |
| ScreenSelectStyle comment double | select style comment double |
| ScreenSelectStyle comment solo | select style comment solo |
| ScreenSelectStyle comment versus | select style comment versus |
| gameplay combo 100 … gameplay combo 1000 (10 rows, step 100) | gameplay 100 combo … gameplay 1000 combo |

Notes:
- The comment above the table says it exists "for compatibility, so we don't force
  announcer changes ... We could use it to support DWI announcers transparently, too."
- The combo rows were added in sm-ssc ("Add support for "gameplay combo #" for announcer",
  `Docs/Changelog_sm5.txt:2817`). The engine itself still **requests `gameplay N combo`**
  (`ScreenGameplay.cpp:2954`), which matches the old disk name directly, so these rows only
  help a theme that calls `SOUND:PlayAnnouncer("gameplay combo 100")`. No pack in the
  corpus (section 4) uses `gameplay combo N`.
- 3.9 had only the first 13 rows (no combo rows; `v390release AnnouncerManager.cpp`).
- Debug builds create `<Name>/<requested>/announcer files go here.txt` when nothing matched
  (`:137-143`).

### 1.4 Emptiness, case handling

- `DirectoryIsEmpty` (`src/RageUtil.cpp:1165-1175`) is true if the path is missing or its
  listing is empty. **Any file counts**: a folder holding only `readme.txt` (or the debug
  `announcer files go here.txt`) counts as non-empty, so aliases are skipped and nothing plays.
  `HasSoundsFor(name)` (`AnnouncerManager.cpp:153-156`) uses the same test, so it also
  returns true for a folder with no audio.
- Case: the RageFile layer is case-insensitive on every OS. `FilenameDB` stores a lowercased
  `lname` per entry and compares on it (`src/RageUtil_FileDB.h:17-23,46-59`;
  `FileSet::GetFilesMatching` lowercases the pattern, `RageUtil_FileDB.cpp:12-23`). Announcer-name
  comparisons use `strcasecmp`/`EqualsNoCase` (`AnnouncerManager.cpp:44,56,128,171`). So
  `ScreenSelectStyle comment Single` on disk matches the request `...comment single`, and
  `*.OGG` matches `*.ogg`.

### 1.5 "Empty" announcer and "off"

- `EMPTY_ANNOUNCER_NAME = "Empty"` (`:14`). A folder named `Empty` (any case) is hidden from
  `GetAnnouncerNames` (`:42-45`). Historically this was a template: 3.9's
  `Announcers/instructions.txt` says "copy the announcer "Empty", rename the copy, and add your
  own sounds".
- The name `""` means disabled: `DoesAnnouncerExist("")` is true (`:50-51`) and `GetPathTo` returns `""`
  (`:116-117`), which `DoPlayOnceFromDir` ignores.
- `SwitchAnnouncer(name)` sets `""` (off) when the folder does not exist (`:66-72`).

### 1.6 Choosing the current announcer

- Preference `Announcer` (`src/PrefsManager.cpp:158`), stored **per game type** in
  `Preferences.ini` sections `[Game-<game>]` (`StoreGamePrefs`/`RestoreGamePrefs` `:368-393`,
  read `:481`, written `:539`). Static.ini can override it.
- At startup (`src/StepMania.cpp:719-725,763`): if the preference is empty, **the announcer
  named after the current game is used** (`sAnnouncer = GAMESTATE->GetCurrentGame()->m_szName`,
  e.g. `dance`, `pump`), then `SwitchAnnouncer`. If no such folder exists, the announcer is off.
- Options: `ConfOption("Announcer")` (`src/ScreenOptionsMasterPrefs.cpp:305-329,782`) lists
  "Off" followed by the folder names. Choosing sets both the current announcer and the preference.
  In `_fallback` it is on `[ScreenAppearanceOptions]` (`metrics.ini:3137`, `Line2="conf,Announcer"`).
- Title-menu codes `NextAnnouncer`/`NextAnnouncer2` cycle through the names, with off after
  the last (`ScreenTitleMenu.cpp:65-71`, `AnnouncerManager::NextAnnouncer :158-181`). Both are empty in `_fallback`
  (`metrics.ini:382-388`).
- A GameCommand `announcer,<name>` switches the announcer (`GameCommand.cpp:218-220,753-754`).
- Lua: `ANNOUNCER:GetAnnouncerNames()`, `DoesAnnouncerExist(s)`, `GetCurrentAnnouncer()` (nil when
  off) and `SetCurrentAnnouncer(s)` (only if it exists) (`AnnouncerManager.cpp:186-228`).
  `SOUND:PlayAnnouncer(name)` (`GameSoundManager.cpp:851-856`) plays any folder name.

---

## 2. Every line StepMania 5.1 plays (engine C++, `_fallback`/`default` themes)

Neither `Themes/_fallback` nor `Themes/default` contains any Lua call to `SOUND:PlayAnnouncer`
or `ANNOUNCER` (searched all `*.lua`/`*.ini`/`*.xml`). Every line below is
**engine C++**, and **theme metrics** tune some of them. `Docs/Themerdocs/announcer_files.txt`
is the upstream list. It agrees with the code except that it omits W3/W4 full combos.

### 2.1 Menus

| Folder name | Trigger | Source |
|---|---|---|
| `<ScreenName> intro` | Every `ScreenWithMenuElements::BeginScreen` (nearly every screen; `<ScreenName>` is the screen's metrics name, e.g. `ScreenSelectMusic intro`, `ScreenGameplay intro`, `ScreenEvaluationNormal intro`). Aliases map `ScreenSelectStyle intro` and `ScreenSelectDifficulty intro`. | `ScreenWithMenuElements.cpp:132` |
| `title menu game name` | `ScreenTitleMenu::Init` | `ScreenTitleMenu.cpp:39` |
| `<ScreenName> IdleComment` | `ScreenSelect` subclasses and `ScreenSelectMusic`: every `IdleCommentSeconds` without input, while not transitioning; the timer resets on any input. Metrics: `[ScreenSelectMaster] IdleCommentSeconds=0` (off), `[ScreenTitleMenu] -1`, `[ScreenTitleJoin] -1`, **`[ScreenSelectMusic] 20`** | `ScreenSelect.cpp:120-149`; `ScreenSelectMusic.cpp:396-402`; `_fallback/metrics.ini:1783,1888,2108,4569` |
| `<ScreenName> comment <ChoiceName>` | Choosing a choice on any `ScreenSelectMaster` screen (title menu, select style, select play mode...). The choice name comes from the GameCommand `name,...`, e.g. `ScreenTitleMenu comment GameStart`, `ScreenSelectStyle comment Single` (via the alias), `ScreenSelectPlayMode comment Normal`. `_fallback` choices: title `GameStart,Options,Edit,Jukebox,GameSelect,Exit`; play mode `Normal,Rave,Nonstop,Oni,Endless`; style `Single,Double,Solo,Versus,Couple` for dance. | `ScreenSelectMaster.cpp:958`; `metrics.ini:1856,2017`; `Scripts/03 Gameplay.lua:94-107` |
| `select difficulty challenge` | `ScreenSelectMaster` moving to page 2. That never happens by default (`NumChoicesOnPage1` defaults to 1024, per the themer docs). | `ScreenSelectMaster.cpp:266,702-710` |
| `select music intro` | `ScreenSelectMusic::BeginScreen`, played **in addition to** `ScreenSelectMusic intro` (both requested; the OutFox wiki's claim that `<Screen Name> intro` "will not work here" does not match 5.1's code). | `ScreenSelectMusic.cpp:289` |
| `select music comment repeat` | On confirming a song (`MenuStart`, SelectingSong state): the song is among the songs played this session (in event mode only the last 5 stages). Forced false when an extra stage locks the selection. | `ScreenSelectMusic.cpp:1278-1289` |
| `select music comment new` | else if `PROFILEMAN->IsSongNew(song)` | `:1266,1291` |
| `select music comment hard` | else if any human's steps meter ≥ `HardCommentMeter` (**10**) | `:51,1268-1272,1293`; `metrics.ini:2109` |
| `select music comment general` | otherwise | `:1295` |
| `select course comment general` | on confirming a course | `:1304` |
| `hurry up` | `MenuTimer` crossing below `HurryUpTransition` (**5.5 s**) | `MenuTimer.cpp:102-103`; `metrics.ini:875` |
| `player options intro` | `ScreenPlayerOptions` init (on top of the `<ScreenName> intro`) | `ScreenPlayerOptions.cpp:41` |
| `select group intro` | `ScreenSelectCharacter` | `ScreenSelectCharacter.cpp:139` |
| `name entry` | `ScreenNameEntry` first update | `ScreenNameEntry.cpp:334-335` |
| `music scroll` | `ScreenEnding` | `ScreenEnding.cpp:108` |

### 2.2 Gameplay

The rate limiter is `ScreenGameplay::PlayAnnouncer(type, fSeconds)` (`ScreenGameplay.cpp:1556-1578`):

```cpp
if( GAMESTATE->m_fOpponentHealthPercent == 0 )
    return; // Shut the announcer up
/* Don't play in demonstration. */
if( GAMESTATE->m_bDemonstrationOrJukebox )
    return;
/* Don't play before the first beat, or after we're finished. */
if( m_DancingState != STATE_DANCING )
    return;
if(GAMESTATE->m_pCurSong == nullptr  ||
   GAMESTATE->m_Position.m_fSongBeat < GAMESTATE->m_pCurSong->GetFirstBeat())
    return;
if( *fDeltaSeconds < fSeconds )
    return;
*fDeltaSeconds = 0;
SOUND->PlayOnceFromAnnouncer( type );
```

There is **one shared timer**, `m_fTimeSinceLastDancingComment`: it is reset to 0 at song start
(`:1352`), incremented only while dancing (`:1960`), and reset whenever any gated line plays.
A gated line is **dropped** (not queued) if fewer than `fSeconds` have passed since the last
gated line. Ungated lines (ready/go/cleared/failed/intro) bypass this and do not reset it.

| Folder name | Trigger | Gate | Source |
|---|---|---|---|
| `gameplay intro` | `BeginScreen`, "crowd cheer" (plus `ScreenGameplay intro` from the base class) | none | `:1598` |
| `gameplay ready` | `SM_DoneFadingIn`, when the Ready transition starts. If the Ready transition has zero length, it is deferred to `SM_PlayGo`. If Go also has zero length, only `ready` plays, not `here we go`. | none | `:2663-2700`; changelog `Changelog_sm5.txt:257-260` |
| `gameplay here we go extra` | `SM_PlayGo`, `GAMESTATE->IsAnExtraStage()` | none | `:2705-2707` |
| `gameplay here we go final` | `SM_PlayGo`, the smallest number of stages left for any human == 0 | none | `:2709-2711` |
| `gameplay here we go normal` | `SM_PlayGo`, otherwise | none | `:2713-2715` |
| `gameplay comment hot` | every frame while dancing, mode Regular/Battle/Rave, `OneIsHot()` (some player's `HealthState_Hot`: life ≥ `HotValue` **1.0**, i.e. a full bar) | `SecondsBetweenComments` (**10.0**) | `:1962-1969`; `GameState.cpp:2438-2444`; `ScreenGameplay.cpp:1685-1706`; `LifeMeterBar.cpp:278-286`; `metrics.ini:3489,639` |
| `gameplay comment danger` | else if `AllAreInDangerOrWorse()` (every player Danger or Dead: life < `DangerThreshold` **0.2** with fail type not Off) | same | `:1970-1971`; `GameState.cpp:2430-2436`; `metrics.ini:635` |
| `gameplay comment good` | otherwise (Regular/Battle/Rave). So **a comment plays roughly every 10 s throughout the song**, unless other gated lines keep resetting the timer. | same | `:1972-1973` |
| `gameplay comment oni` | modes Nonstop/Oni/Endless (courses), regardless of life | same | `:1975-1978` |
| `gameplay N combo`, N = 100,200,…,1000 | `Player` posts `SM_<N>Combo` when the combo crosses N (`old < N && new >= N`) in `HandleTapRowScore`. This is a chain of else-ifs, so one message per row. | 2 s | `Player.cpp:3085-3105`; `ScreenGameplay.cpp:2949-2955` |
| `gameplay combo overflow` | above 1000: every crossing of a new hundred (`old/100 < new/100 && new > 1000`) → `SM_ComboContinuing` | 2 s | `Player.cpp:3106-3107`; `ScreenGameplay.cpp:2960-2963` |
| `gameplay combo stopped` | combo was > `ComboStoppedAt` (**50**) and is now < 50 (`SendComboMessages`) | 2 s | `Player.cpp:232,808-813`; `metrics.ini:1161`; `ScreenGameplay.cpp:2956-2959` |
| `gameplay battle trick level1..3` | `SM_BattleTrickLevel1..3` (battle mode attacks) | 3 s | `:2964-2971` |
| `gameplay battle damage level1..3` | `SM_BattleDamageLevelN` sent by `Inventory` | 3 s | `:2972-2976`; `Inventory.cpp:217-218` |
| `gameplay battle damage level3` | enemy health reaches 0 in battle ("HACK: Load incorrect directory on purpose") | 0 s (but then the opponent-health check silences everything afterwards) | `:1895-1903` |
| `gameplay extra` | song finished, not all failed, `HasEarnedExtraStage()` | none | `:2833-2834` |
| `gameplay cleared` | song finished, not all failed, otherwise | none | `:2835-2836` |
| `gameplay oni failed halfway` | `SM_BeginFailed` in course mode, course song index ≥ half the queue | none | `:3031-3033` |
| `gameplay oni failed` | `SM_BeginFailed` in course mode, before halfway | none | `:3034-3035` |
| `gameplay failed` | `SM_BeginFailed`, not a course | none | `:3037-3039` |

Other gameplay metrics: `[ScreenGameplay] MinSecondsToStep=6.0` (`metrics.ini:3510`) is the lead-in before the first step,
not an announcer setting. The combo used is per row or per note according to `ComboIsPerRow` (`metrics.ini:572`).
The 3.9 default theme had `SecondsBetweenComments=10` for gameplay and `=12` for the title-menu attract loop
(`v390release Themes/default/metrics.ini:1242,88`).

Lines asked about that **do not exist** in 3.0, 3.9, 5.0/5.1, ITGmania or Etterna code: "gameplay comment very good",
"gameplay comment other". I found no requester for them anywhere (possibly DWI/older, unverified).

### 2.3 Evaluation (`ScreenEvaluation::Init`, `ScreenEvaluation.cpp:641-724`)

**Exactly one** line is chosen, in this precedence:

1. If an extra stage was earned: the **theme** sound `ScreenEvaluation try extra1/2` (`THEME->GetPathS`), not the announcer (`:675-678`).
2. `evaluation new record`, if some player has machine or personal high-score index 0 **and**
   `HasSoundsFor("evaluation new record")` (`:679-682`). This is the only line with an existence check, so a pack lacking it falls through to the grade.
3. `evaluation full combo W4`, if some player has a W4 full combo and `[Gameplay] MinScoreToMaintainCombo == TNS_W4` (`:683-686`).
4. `evaluation full combo W1|W2|W3`, the best FC type any player has (`:687-691`). `FullComboOfScore(tns)` needs no taps below `tns`, no let-go holds, no checkpoint misses and the player able to achieve an FC (`PlayerStageStats.cpp:558-583`). Note that **a missing FC folder silences the grade line** (no fallback).
5. Else:
   - summary or course mode: `evaluation final <G>`
   - battle: `evaluation win` / `evaluation lose` (master player's stage result)
   - otherwise: `evaluation <G>`

   `<G>` = `GradeToOldString(best grade over players)` (`Grade.cpp:33-49`): `AAAA` (Tier01), `AAA`, `AA`,
   `A`, `B`, `C`, `D` (Tier07), `E` (Failed), `N` (NoData). Grades are first rescaled from the
   theme's `NumGradeTiersUsed` onto 7 (`GradeToOldGrade`, `:51-59`; `_fallback` uses 7 tiers, `default` 17, `metrics.ini:1124/1123`).

Then `evaluation cheer` is posted after `CheerDelaySeconds` (**2.5**) when the old grade is Tier01–03 (AAAA/AAA/AA)
(`:716-724,779-785`; `metrics.ini:3728`). It plays alongside whichever line played above.

The FC lines were added in sm-ssc/SM5 (`Changelog_sm5.txt:2285-2286` W1/W2; `:1150-1151` W3/W4). 3.9 had no FC lines.

### 2.4 Not played by SM5.1 at all (3.9-era names packs still ship)

`stage <N|final|extra1|extra2|oni|nonstop|endless|event|demo>`, `caution`, `game over`,
`title menu attract`, `credits`, `select group comment all music|general`, `select player intro`,
`select course intro`, `select character intro`, and the `ScreenSelectDifficulty comment *` family. 5.1 has no `ScreenSelectDifficulty` class; the names
are reachable only if a theme names a `ScreenSelectMaster` screen that way. A 5.x theme can still play any
of them with `SOUND:PlayAnnouncer`. The stock themes do not.

---

## 3. Forks

### 3.1 Etterna (MIT): a reduced and partly re-purposed subset, **not an extension**

The `AnnouncerManager` is the SM5 one with string types changed. The differences are in the callers.

Etterna-specific behavior (all quotable; `src/Etterna/...` paths):

- **Alias table** (`Singletons/AnnouncerManager.cpp:76-106`): the SM table **minus** the
  `ScreenSelectDifficulty comment oni|nonstop|endless` rows (10 screen rows instead of 13, plus the same 10 combo rows).
- **File choice**: `GetDirListing(..., ONLY_FILE)` for mp3/wav/ogg/oga, then `RandomInt` uniform with no
  shuffle bag (`Singletons/GameSoundManager.cpp:328-347`). Only files are counted (SM5.1 does not filter dirs).
- **Default announcer**: as in SM (preference, else the game's name, `Globals/StepMania.cpp:887-893`). The option is still offered:
  `[ScreenAppearanceOptions] Line2="conf,Announcer"` (`Themes/_fallback/metrics.ini:2295`), reached from Til Death's "Appearance Options".
- **Themes play nothing extra**: no `PlayAnnouncer`/`ANNOUNCER` use in any Lua of `_fallback`, `Til Death`, `Rebirth`
  or `bare-frames` (searched all `*.lua`/`*.ini`/`*.xml`). Etterna ships **no announcer** (only `Announcers/instructions.txt`).
- **PlayAnnouncer gate** (`Screen/Gameplay/ScreenGameplay.cpp:801-822`): same dancing/first-beat/elapsed checks; the
  opponent-health and demonstration checks are gone. The same shared timer.
- **Gameplay comments** (`:1166-1178`): hot / danger / good every `SecondsBetweenComments` (10.0), with
  no play-mode switch (`comment oni` removed). Health is single-player (`Singletons/GameState.cpp:1159-1172`).
- **Combos** (`Actor/Gameplay/Player.cpp:2836-2858`, `:745-747`; `ScreenGameplay.cpp:1856-1867`): identical to SM5.1
  (100..1000, overflow, stopped below `ComboStoppedAt=50`, 2 s gate). The same messages are posted by
  `PlayerReplay.cpp:537-566` and `PlayerSpectate.cpp:589-609`, so **lines also fire while watching replays
  and spectating** (the code comments "mostly dead code for announcers and other stuff...").
- **Ready/Go** (`:1677-1700`): only `gameplay here we go normal` (no extra/final; Etterna has no stage count).
- **End**: `gameplay cleared` (`:1821`) and `gameplay failed` (`:1912`). No `gameplay extra`, no oni-failed lines.
- **Song select** (`Screen/Others/ScreenSelectMusic.cpp:1282-1307`), new score-based criteria ("these are the
  various new criteria for the ssm comments, before they were based on some things that are more arcadey"):
  - `repeat`: a score this session has the same **chart key**
  - `new`: no scores for the chart (`SCOREMAN->GetScoresForChart(key) == nullptr`)
  - `hard`: `GetMSD(1.F, Skill_Overall) > 30.F` (MSD rating at 1.0× rate). The `HardCommentMeter` metric is
    still loaded (`:53`) but **unused**.
  - else `general`. No `select course comment general`.
  - `select music intro` (`:246`), `ScreenSelectMusic IdleComment` (`:411-414`, 20 s).
- **Evaluation** (`Screen/Others/ScreenEvaluation.cpp:78-147`):
  - `evaluation new record` keeps the same condition, but `m_iMachineHighScoreIndex`/`m_iPersonalHighScoreIndex` are only
    ever initialised to −1 (`Models/Misc/PlayerStageStats.cpp:93-94`; no other assignment in `src/`), so
    **it can never fire** (code reading, not tested at runtime).
  - FC W4 (if `MinScoreToMaintainCombo` is W4, `ComboMaintain()` returns W3 for every game) → FC W1/W2/W3, as in SM.
  - Else the grade comes from the **most recent score's Wife grade** (`GetWifeGrade`, or the SSR-normalised % when
    `SortBySSRNorm` is set; `Models/HighScore/HighScore.cpp:92-105`):
    - Tier01 (≥ 99.9935 %) → **`evaluation aaaaa`** (new, Etterna-only)
    - otherwise `evaluation ` + `GradeToOldString` (`Models/Misc/Grade.cpp:36-70`): Tier01–04 → `AAAA`, 05–07 → `AAA`,
      08–10 → `AA`, 11–13 → `A`, 14 → `B`, 15 → `C`, 16 → `D`, **Failed → `F`** (SM: `E`), Invalid → `N`.
    - Wife thresholds (`Grade.cpp:94-150`): AAAAA ≥ 99.9935, AAAA ≥ 99.955, AAA ≥ 99.7, AA ≥ 93, A ≥ 80, B ≥ 70, C ≥ 60, else D;
      mid-grades (AAAA:, AAA., ... ) are optional sub-tiers mapped to the same strings.
  - No `evaluation final *`, no `win`/`lose`.
  - `evaluation cheer` after `CheerDelaySeconds` 2.5 when the Wife grade ≤ Tier10 (**AA or better, ≥ 93 %**) (`:136-147,293-298`).
- Kept unchanged: `<ScreenName> intro` (`ScreenWithMenuElements.cpp:121`), `title menu game name`
  (`ScreenTitleMenu.cpp:34`), `<ScreenName> IdleComment` (`ScreenSelect.cpp:116`), `<ScreenName> comment <Choice>`
  and `select difficulty challenge` (`ScreenSelectMaster.cpp:269,900`), `player options intro` (`ScreenPlayerOptions.cpp:34`),
  `hurry up` (`Actor/Menus/MenuTimer.cpp:110`), `gameplay intro`, `gameplay ready`.
- Removed (no requester in `src/`): `name entry`, `music scroll`, `select group intro`, `select course comment general`,
  `gameplay here we go extra|final`, `gameplay comment oni`, `gameplay extra`, `gameplay oni failed[ halfway]`,
  `gameplay battle *`, `evaluation final *`, `evaluation win|lose`, `evaluation E`.

#### Etterna vs StepMania 5.1

| Aspect | StepMania 5.1 | Etterna 0.75.1 |
|---|---|---|
| Licence | MIT-style | MIT |
| Layout `Announcers/<Name>/<line>/` | yes | same |
| Extensions | mp3, wav, ogg, oga | same (files only) |
| File choice | per-folder shuffle bag (no repeat until exhausted) | uniform random (can repeat) |
| Aliases | 13 screen rows + 10 combo rows | 10 screen rows (no oni/nonstop/endless) + 10 combo rows |
| Default announcer | pref per game, else the folder named after the game | same |
| Single-file lines | no (despite instructions.txt) | no (same text) |
| `<Screen> intro`, `<Screen> IdleComment`, `<Screen> comment <Choice>`, `title menu game name`, `hurry up`, `player options intro`, `select difficulty challenge` | yes | yes |
| `select music intro` | yes | yes |
| select music repeat/new/hard/general | session repeat / profile-new song / meter ≥ `HardCommentMeter` (10) | chart-key score this session / no scores for chart / MSD overall @1.0× > 30 (metric unused) |
| `select course comment general` | yes | removed |
| `gameplay intro`, `gameplay ready` | yes | yes |
| `gameplay here we go normal|extra|final` | all three | normal only |
| Gameplay comments | hot/danger/good (regular/battle/rave), oni (courses); every 10 s | hot/danger/good; no oni |
| Gate | shared timer; off in demo/after battle win | shared timer; demo/battle checks removed |
| Combo 100–1000, overflow, stopped (<50), 2 s gate | yes | yes, **also in replays/spectate** |
| Battle trick/damage lines | yes | removed |
| `gameplay cleared` / `failed` | yes | yes |
| `gameplay extra`, `oni failed`, `oni failed halfway` | yes | removed |
| `evaluation new record` | when machine/personal rank 0 | present but never fires (index never set) |
| `evaluation full combo W1–W4` | yes | yes |
| Grade line | `evaluation AAAA…D, E, N` from DDR-style tiers rescaled to 7 | **`evaluation aaaaa`** (≥ 99.9935 % Wife) + `AAAA…D`, **`F`** for failed, `N` |
| `evaluation final <G>`, `win`, `lose` | yes | removed |
| `evaluation cheer` | old tier ≤ AA, after 2.5 s | Wife grade ≥ AA (93 %), after 2.5 s |
| `name entry`, `music scroll`, `select group intro` | yes | removed |
| Themes calling `PlayAnnouncer` | none | none |
| Bundled announcer | none | none |

So Etterna adds exactly **one** new folder name (`evaluation aaaaa`) and one new grade spelling (`evaluation F`).
Everything else is a subset of SM5.1, with score-based song-select criteria and the Wife-percent grade mapping.
For an importer, SM5.1's name set is the superset to recognise, plus `evaluation aaaaa` and `evaluation f`.

### 3.2 ITGmania (GPL-3.0; behavior only)

v1.3.0 keeps the system **unchanged in behavior** from SM5.1. The alias table carries the same 23 rows (string literals compared).
It requests the same set of folder names from the same screens (compared extracted call sites: identical except formatting), uses the
same mp3/wav/ogg/oga listing with the per-folder shuffle bag, has the same metric defaults (`ComboStoppedAt=50`,
`HardCommentMeter=10`, `SecondsBetweenComments=10.0`), and ships the same `Announcers/instructions.txt`. Its theme
Simply Love offers the Announcer option on its system options page but plays no announcer lines from Lua. No new lines.

### 3.3 Project OutFox

Engine source not available, so behavior is **unverified**. The official wiki page "Announcers" (OutFox-Wiki
`gh-pages`, `dev/announcers/index.html`, last commit 2024-04-16) documents the same set as SM5.1: title,
`<Screen Name> intro`, `hurry up`, IdleComment, ScreenSelectMaster comment/challenge, select music
(HardCommentMeter 10), all gameplay lines including battle/oni (battle lines described as "leftover code from In The
Groove's alpha inventory system", with "no function today"), evaluation incl. FC W1–W4, `final <Grade>`, win/lose
("only on rave mode"), cheer, `player options intro`, `select group intro`, `name entry`, `music scroll`, and custom lines via Lua.
Root is `Project OutFox/Announcers/<Name>/<line>/`, with any number of files picked at random. No new lines are documented.
It claims `<Screen Name> intro` "will not work" on ScreenSelectMusic/ScreenPlayerOptions/etc.; that is untrue of SM5.1's code, and unverifiable for OutFox.

---

## 4. Real-world corpus

### 4.1 Sources

- **SMArchive** (`https://josevarela.net/SMArchive/Announcers/`, by Jose_Varela; site repo `github.com/JoseVarelaP/StepMania-Archive`)
  has an **Announcers** section listing 33 entries (34 archive links). Downloads are hosted at
  `https://s3.us-east-005.dream.io/smannouncers/<name>.(smzip|zip|rar)` with ogg previews in `AudioPreview/`.
  Names: Akuma, Ashens, Caddicarus, Call Me Kevin, Cheesemix 2, Dancemaniax, DDR 1st Mix, DDR A (DDR SELECTION) voices,
  DDR Extreme2, DDR Hottest Party 2, DDR Kids, DDR Mega Announcer Pack!, DDR Solo 2000 / ExtraMix, DDROHA_1, DDROHA_2,
  DDRX Beta1, DWI, ExtraMix, Funhaus, Gameshawn, Jacksfilms, KatzeExtreme, Microsoft Sam, Neo (Spanish), RoboStepper,
  Sabrina, Schizkitty, Space Channel 5, Techno, Trance Machina, Unreal Tournament (Evil), Unreal Tournament (Merc), Worms, Yoshi, Zig.
- StepMania 3.9's own repository tree once carried two announcers: `Announcers/techno/` (21 folders, Ogg, the same content as
  SMArchive's "Techno") and `Announcers/ez2/title menu game name/ez2dancer.wav` (`v390release` tree). Neither is in 5.x.
- I found **no other public announcer repository** on GitHub and no freely licensed announcer (searches for CC0 / CC BY StepMania announcers found nothing).

### 4.2 Method

I listed all 34 archives **by file name only**. For the 32 zip/smzip files I read just the zip central directory with
HTTP Range requests, so no audio was downloaded. The two RARs were downloaded into their own empty directories, listed with `bsdtar -t`,
and deleted. The listings were kept outside the repository.
I did not open or play any audio.

### 4.3 Findings

- **Containers**: 22 × `.smzip` (StepMania package: a `smzip.ctl` control file at the root, and content under
  `Announcers/<Name>/...`), 10 × `.zip`, 2 × `.rar`. The zip roots vary:
  `Announcers/<Name>/` (smzip), `<Name>/` (DDR 1st Mix, DDROHA, DWI…), the line folders at the archive root
  (Ashens, Caddicarus, Call Me Kevin, Funhaus, jacksfilms), or a differently named top folder
  (`Dance Dance Revolution MEGA Announcer Pack!/`, and `Announcers/DDRX/` in "DDRX Beta1"). **An importer must find the
  announcer root heuristically** (the directory whose children are recognised line names).
- **Sizes**: 0.6–62 MB uncompressed; most are 2–7 MB with 20–90 line folders and a median of **2 files per folder** (max 149).
  The largest are the Mega pack (62 MB, 1 456 files, 116 folders) and Yoshi (62 MB, mostly WAV). Archives range from 0.5 to 55 MB.
- **Formats** (all files): mp3 3 980, ogg 2 340, wav 894. Also `.ctl` (smzip), `.txt`/`.doc` readmes, `.pk` (Audacity peak
  files), `.sfl`, `.DS_Store`. Packs mix formats (Akuma: wav plus mp3; ExtraMix: mp3 plus wav; Yoshi: wav/mp3/ogg).
  No flac/opus/oga. **No `.redir` files anywhere.**
- **Nesting/extras**: the Mega pack nests folders inside line folders (`evaluation final a/evaluation a/wave_072.mp3`) and has
  loose sounds at its root (`New Record.mp3`, `Oh my god!.mp3`). SM ignores both. "DDR A (DDR SELECTION) voices" is
  566 unsorted `00000000.wav…` files with no line folders, so it is unusable without hand sorting.
- **Folder names** across the 33 usable packs (162 distinct lowercased names):
  - Universal (≥ 30 packs): `gameplay comment hot|danger|good`, `evaluation a|b|c|d|aa|aaa`, `gameplay combo stopped`,
    `gameplay failed`, `gameplay ready`. Close behind (≥ 22): `title menu game name`, `evaluation e`, `game over`,
    `select music intro`, `gameplay here we go normal|final|extra`, `select music comment general|hard|new`,
    `gameplay cleared`, `gameplay 100…1000 combo`, `evaluation final a…d`.
  - **SM5.1 names never seen in any pack**: `gameplay intro`, `gameplay oni failed halfway`, all `gameplay battle *`,
    `evaluation full combo W4`, `evaluation n`, `evaluation final n`. W1–W3 FC lines appear in **one** pack;
    `evaluation aaaa` in 7; `evaluation new record` in 18; `select music comment repeat` in 6; `gameplay combo overflow` in 10.
    No `evaluation aaaaa` and no `evaluation f` (Etterna names) appear anywhere.
  - **3.9-era names that SM5.1 never plays**, but most packs ship: `stage 1…6|final|extra1|extra2|oni|nonstop|endless|event`
    (in up to 16 packs), `caution` (21), `game over` (28), `title menu attract` (19), `select group comment *`.
  - **Alias (old) names** are common: `select style intro`/`select style comment single|double|versus|solo` and
    `select difficulty intro|comment easy|medium|hard|oni|endless|nonstop|beginner|challenge` appear in up to 20 packs.
    Only 2 packs use the new `ScreenSelectStyle comment …` spelling. **Nobody uses `gameplay combo N`.**
  - **Screen-name lines** (theme-specific): `ScreenCaution intro`, `ScreenSelectPlayMode intro|IdleComment`,
    `ScreenSelectMusic IdleComment`, `ScreenSelectMaster comment oni|nonstop|…`, `ScreenSelectStyle comment Single`,
    `ScreenSelectStyleRave …`, `ScreenSelectStyleCourse …`, `ScreenStageInformation stage event`.
  - **Unrecognised/custom** (for themes calling `PlayAnnouncer`, or mistakes): `gameplay 1100…2000 combo` (Mega pack; the engine
    plays `combo overflow` instead), `crowd cheer|boo`, `menu hurry up`, `gameplaynonstop ready|here we go normal`,
    `select course intro` (18 packs), `select character intro`, `select style comment couple`, `ranking title`,
    `title screen title`, `title menu home intro`, `SSM comment A…H`/`SSM intro` (KatzeExtreme), `SSStyle …`/`SSCourseMode …`,
    `stage extra`.
  - **Case**: most names are lowercase. 51 distinct names have capitalised spellings (`ScreenCaution intro`,
    `SSM comment A`), so matching must be case-insensitive, as SM's is. No leading or trailing whitespace was found.
  - Some packs (RoboStepper, DWI, Hottest Party 2) contain **empty line folders**. Under SM's rules an empty folder
    falls through to aliases and then silence.

### 4.4 Licensing reality

Essentially every pack is a rip of copyrighted audio: Konami DDR voices (1st Mix, Extreme 2, Solo 2000/ExtraMix,
Hottest Party 2, Kids, DDR X, DDR A, Mega pack, DDROHA), other games (Dancemaniax, TechnoMotion "Techno", EZ2Dancer,
Unreal Tournament, Worms, Space Channel 5, Yoshi, Akuma), YouTubers' video clips (Ashens, Caddicarus, Call Me Kevin,
Funhaus, jacksfilms, Gameshawn), and Microsoft Sam TTS. None declares a free licence in its file names. I did not open
the six readmes (DDR 1st Mix, DDR Kids, Microsoft Sam, Space Channel 5, RoboStepper `about.txt`, Mega pack), so their
contents are unverified. **No CC BY / CC0 StepMania announcer was found.** A bundled default would need to be
commissioned or self-recorded, or built from generic free voice assets (some itch.io voice packs claim attribution-only
terms; unverified and not StepMania-shaped).

---

### 4.5 The maintainer's local examples

Three packs on the maintainer's machine, listed by name only (never
bundled or copied; local tests only, as with simfiles):

- *DDR 1st Mix* (1.4 MB, 69 Ogg files, 23 line folders and a readme): the
  clean case; one old screen name, `ScreenSelectPlayMode intro`.
- *DDR Mega Announcer Pack!* (53 MB zipped, 1,396 MP3 files, 116 folders):
  up to 150 variants a line (`gameplay comment hot`), old screen-class
  names (`ScreenSelectMaster comment heavy`, `SSStyle intro`), a second
  near-complete announcer nested inside `evaluation final a/`, `.pk` files
  alone in `stage 6/` and `stage event/`, `gameplay 1100…2000 combo`, and
  `gameplayNonStop ready`.
- *DDR A (DDR SELECTION) voices* (30 MB, 566 WAV files named by hex
  number, no line folders): not an announcer; an import must say it found
  no lines rather than guess.

## 5. StepMania 3.9 and OpenITG differences

### 5.1 StepMania 3.9 (`v390release`)

- Same lookup (`AnnouncerManager.cpp`, `CString`): same directory layout, same `Empty` handling, 13 screen alias rows,
  **no combo aliases**. The engine requested `gameplay 100 combo`… directly (`ScreenGameplay.cpp:2178-2205`). Files are
  mp3/wav/ogg, chosen by `rand() % n` (`GameSoundManager.cpp:241-260`).
- Combo stopped: hard-coded `iCurCombo > 50` on a Miss/Good/Boo (`Player.cpp:1158-1161`). Milestones as in 5.1 (`:1179-1201`).
- Lines 3.9 played that 5.1 does not:
  - `stage <text>` on ScreenStage (`ScreenStage.cpp:99`), text from `GameState::GetStageText` (`GameState.cpp:736-747`):
    `demo`, `oni`, `nonstop`, `endless`, `event`, `final`, `extra1`, `extra2`, else the stage number `1`…
  - `title menu attract`, repeated every `[ScreenTitleMenu] SecondsBetweenComments` (12) (`ScreenTitleMenu.cpp:346-353`)
  - `caution` (`ScreenCaution.cpp:65`), `game over` (`ScreenGameOver.cpp:39`), `credits` (`ScreenCredits.cpp:263`)
  - `<ScreenName>` with no suffix on attract screens (`ScreenAttract.cpp:47`)
  - `select group intro` / `select group comment all music` / `select group comment general` (`ScreenSelectGroup.cpp:126,228-230`)
  - `select player intro` (`ScreenEz2SelectPlayer.cpp:83`)
  - `ScreenSelectDifficulty comment <choice>` and `select difficulty challenge` (`ScreenSelectDifficulty.cpp:108,376`)
  - `<ScreenName> comment <choice>` (`ScreenSelectStyle.cpp:194`); `<ScreenName> intro` from `ScreenSelect.cpp:102`
  - `music scroll` (`ScreenMusicScroll.cpp:67`, `ScreenEnding.cpp:242`); `name entry` (also `ScreenNameEntryTraditional.cpp:474`)
- No `<Screen> IdleComment`, no `hurry up` via the announcer queue (it used `PlayOnceFromDir(GetPathTo("hurry up"))`, `MenuTimer.cpp:74`),
  no FC lines. Evaluation: new record → (stage: battle win/lose or `evaluation <G>`) / (course or summary: `evaluation final <G>`)
  (`ScreenEvaluation.cpp:805-848`), grades `AAAA`…`E`,`N` (`Grade.cpp:15-31`).
- 3.0 (`v300-final`) preloaded the same gameplay names (`gameplay ready`, `here we go extra|final|normal`, `comment danger|good|hot|oni`, `gameplay 100…1000 combo`; `ScreenGameplay.cpp:397-418`).

### 5.2 OpenITG (behavior only)

OpenITG keeps the 3.9/3.95 announcer system: the same 13-row alias table and mp3/wav/ogg. Differences from 3.9:

- Stage lines use `StageToString(GetCurrentStage())`: `1`–`6`, `Final`, `Extra1`, `Extra2`, `Nonstop`, `Oni`, `Endless`, `Event`, `Demo`
  (capitalised; matching is case-insensitive, so `stage final` still matches). `event` is only reached in event mode.
- Gameplay resolves through `PlayOnceFromDir(GetPathTo(type))`, with a single combo format string.
- It adds `<ScreenName> IdleComment` (ScreenSelect, `IdleCommentSeconds` metric).
- It drops `title menu attract`, `caution`, `game over` and the attract-screen `<ScreenName>` lines.
- ITG's own themes are not part of that repository, so whether ITG themes used announcers is unverified.

---

## 6. Implications for an importer (summary)

1. Recognise `Announcers/<Name>/<line>/` with case-insensitive line names, and audio `mp3|wav|ogg|oga` directly inside each line
   folder. Ignore subfolders, loose root files, `.redir`/readmes/`.pk`/`.ctl`. Locate the root inside zips/smzips
   heuristically, because of the four root shapes above.
2. Canonical set = SM5.1's requested names + Etterna's `evaluation aaaaa`/`evaluation f`. Map old names via SM's alias
   table, and normalise case to lowercase.
3. Keep the 3.9-only names (`stage *`, `caution`, `game over`, `title menu attract`) as "known but unused" or for optional use.
   Most packs have them, and they are the most natural candidates for extra hooks (stage start, game over).
4. Treat a folder that contains no audio as absent. SM would treat it as present and play silence for that line, which is
   probably worse.
5. Timing semantics worth copying: one shared gameplay cooldown that drops lines rather than queueing them (combo/stopped 2 s,
   periodic comment 10 s); combo milestones on crossings of 100…1000 and every 100 above that ("overflow");
   "stopped" when the combo falls from above 50; and one evaluation line by precedence, plus a cheer 2.5 s later.
   There is no ducking, no interruption, and plain mixing at sound-effect volume. A shuffle bag per folder (SM5.1) avoids repeats.
