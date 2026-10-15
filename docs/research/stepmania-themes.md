# StepMania themes: what they are, what hosting them takes (2026-10-15)

Scope: whether the game can host StepMania and ITGmania themes, at least
in part, aiming at the themes collected at
https://josevarela.net/SMArchive/Themes/; what a theme's screens are, how
much freedom a theme has, whether themes handle other screen ratios and
whether they bring noteskins. The menus the game draws itself are
planned in `docs/plans/ui-core.md`, informed by `ui-toolkits.md`.

Sources: StepMania 5.1 (`5_1-new`, MIT; `src/` and `Themes/_fallback`),
read and quoted by path; ITGmania and Simply Love (GPL-3.0) read only;
ten themes downloaded into a scratch directory (Cerulean Skies,
Consensual, CyberiaStyle 3, Dance Dance Revolution MAX, ITG Shadow,
Lambda, Simply Love and Simply Love for ITGmania, XX (starlight),
ultralight) and the archive's index counted by script. Nothing from a
theme was copied or executed.

## 1. The archive

355 entries, by the engine they were made for:

| Engine | Entries |
|---|---|
| StepMania 3.9 family | 136 |
| The ITG engine (3.95, OpenITG, ITG2 PC, NotITG) | 84 |
| StepMania 4 CVS builds | 13 |
| StepMania 5 family (5.0, 5.1, OutFox, ITGmania) | 113 |
| Etterna | 9 |

About 112 recreate a commercial game's look. Some downloads carry
executables (`.exe`, `.dll`, `.bat`) beside the theme; an importer must
ignore them.

## 2. What a theme is

- **Screens.** StepMania 5.1 has 74 C++ screen classes; a theme's
  `metrics.ini` names its screens and the class each is (`Class=`), with
  the positions and commands of the class's parts. A theme can also define
  screens wholly in Lua (Consensual's song wheel is 2,455 lines of it).
- **Layers.** Every menu screen (`ScreenWithMenuElements`) loads, from the
  theme's `BGAnimations`, an underlay, an overlay, decorations, and the
  in, out and cancel transitions (`src/ScreenWithMenuElements.cpp:68-109`).
- **Actors.** Everything drawn is an actor (`ActorFrame`, `Sprite`,
  `BitmapText`, `Quad`, `ActorMultiVertex`, `Model`, `ActorScroller`, …),
  animated by its command queue (`src/Actor.cpp`), reacting to broadcast
  messages; the same model as noteskins (`noteskins.md`).
- **Fallback.** A theme names the theme it falls back to; anything it
  does not define comes from there, and ultimately from `_fallback`
  (`src/ThemeManager.cpp`).
- **Gameplay.** The theme draws the play screen's HUD (life, score,
  combo, judgements, the song's progress); the noteskin draws only notes,
  holds, mines, receptors and explosions. Modern themes hide the engine's
  HUD parts and draw their own (Simply Love's gameplay Lua is about 4,990
  lines).
- **Fonts.** Bitmap only: an `.ini` and page images per font, no TrueType.
  Japanese comes from `_fallback`'s `_japanese 24px` pages.

## 3. Generations

- **3.9:** metrics and images, no Lua.
- **ITG engine:** XML actor files with inline Lua (about 500 inline
  functions across the sampled themes). ITG's own fallback theme is part
  of ITG's game data, not redistributable.
- **StepMania 5:** Lua throughout. Simply Love (ITGmania) is 38,408 lines
  of Lua calling 491 distinct engine methods, 14 of them ITGmania-only
  (`NETWORK:HttpRequest` among them); StepMania 5.1 binds about 1,460
  methods in all.

## 4. Screen ratios

- 3.9: a fixed 640×480 screen, stretched.
- OpenITG: widened only when the `DisplayAspectRatio` preference is set.
- StepMania 5: the theme's `ScreenHeight` (480) is kept and the width
  follows the display's aspect (`src/ScreenDimensions.cpp`); themes adapt
  with `WideScale` and `IsUsingWideScreen`. Simply Love supports 4:3 to
  21:9.
- No sampled theme handles a portrait screen.

## 5. Noteskins

No engine loads noteskins from inside a theme. A theme can name a default
(`[Common] DefaultNoteSkinName`, `CommonMetrics.cpp:20`; ITG's
`DefaultModifiers`) or set one from Lua; some downloads ship a `NoteSkins/`
folder beside `Themes/`, which an importer can offer as noteskins. The
game never substitutes a skin the pack did not ask for.

## 6. Licences

Mostly undeclared. Free: ultralight (Unlicense/CC0), Lambda (MIT),
Consensual (BSD). Copyleft: Simply Love (GPL-3.0). Restrictive: XX. None
may be bundled unless its files declare a licence the bundling rules allow
(`AGENTS.md`); players import their own.

## 7. Support tiers

- **(a) 3.9 and OpenITG themes** (metrics, images, XML actors): medium to
  large; ITG themes lean on ITG's own fallback, which we cannot ship.
- **(b) StepMania 5 gameplay and evaluation decorations**: a theme's HUD
  and results drawn over our play and results screens; about 300–500 host
  methods. Best value.
- **(c) Whole StepMania 5 themes** (every screen, their own menus and
  options): rebuilding StepMania's front end; not recommended.

## 8. Conclusions for the design

1. Hosting themes needs the actor model the noteskins already run, grown
   into a scene for whole screens: layers per screen, messages, a virtual
   screen as tall as the theme says and as wide as the display's aspect,
   bitmap fonts.
2. Our own screens follow StepMania's structure (named screens with
   underlay, overlay, decorations and transitions, positions from
   metrics-like data), so a theme's parts can replace ours piece by piece.
3. Order: (b) first, then a theme's decorations around our song list,
   then (a) where cheap. Options screens stay ours.
4. Portrait (phones) is ours to design; themes will not cover it.
