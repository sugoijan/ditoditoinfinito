# Basic and Advanced, the settings' hierarchy and a hidden settings screen (2026-11-26)

A design for the maintainer to decide; its stage 1 is built (section 9,
2026-11-26) on the plan's own recommendations, then on the maintainer's
answers to open questions 1 and 2 the same day (the switch's words, and
the rows leaned toward Advanced), and its stage 2, the words (section
10, 2026-11-26), on the recommendations for questions 6 to 8, and its
stage 3, the hierarchy (section 11, 2026-11-27), on the recommendation
for question 9; the other open questions of section 8 still open. It covers items 3,
7 and 8 of `docs/PLAN.md`'s "What comes next" together, since they
decide the same screen: two levels of the interface, a better hierarchy
for Options with choices shown side by side where the look matters, and
a hidden screen for experimental and developer switches. The Plugins
page of [Song sources: native plugins](source-plugins.md) waits on the
levels (its open question 2).

Every statement about today's UI was read in the code at `dev`'s tip
of 2026-11-26 (commit `7b4e22ab`); the counts are from there.

## The request

The maintainer's words:

- (2026-11-25) "Split the UI into 2 'levels', a basic level and an
  advanced level, toggled from an option in the settings, basic is the
  default and avoids jargons and technical details, advanced is aimed
  at users already familiar with the genre and other games like
  StepMania/Etterna/ITGMania/etc".
- (2026-11-25) "Provide a hidden 'about:config' like settings UI, so
  some experimental/advanced options are easier to toggle without a UI
  that needlessly pollutes the settings".
- (2026-11-25) "Revamp the settings, I want a better hierarchy and
  better preview/visualization for what each option changes, not just a
  'live preview update' but more like seeing each choice side-by-side
  before picking one (in cases where the visual matters most), many of
  these are highly specific to the option being affected and I predict
  that most of the work will be deciding what interface each option
  will have".
- (2026-11-26, on source plugins) "Maybe we should hide the plugin page
  behind an 'advanced' toggle, and we could also start that design,
  otherwise how can a plugin be added for the first time if it is
  hidden?"

## In short

- **Today**: Options has 126 kinds of row in 9 sections (11 with
  desktop's Songs and the hidden Sharing), built from 93 stored settings;
  about 69 of the rows suit anyone, 53 assume the genre or a technical
  reader, and 4 belong to a hidden switch. Outside Options, 52 places
  show the genre's jargon or technical detail (song select's ratings and
  radar, the results' timing panels, calibration's page, the modifiers
  line, the editor, rooms' rules). The web reads 31 testing parameters
  from an address, and desktop about fifty flags.
- **The rule**: a row is Basic when someone who has never played a
  rhythm game can tell from its name and one plain sentence what it
  changes, would want to change it, and can see, hear or feel the
  change, leaning toward Advanced in doubt (the maintainer, 2026-11-26:
  "lean into erring more towards marking a simple option as advanced
  than marking a complex option as basic"); privacy, consent, resets, volume, language, accessibility,
  controls and calibration are always Basic. A row is Advanced when it
  names a genre or another game's concept, moves judging or scoring away
  from the game's own, tunes a mechanism by a number, or serves content
  brought from other games. A switch for testing, an experiment or a
  few people goes to the hidden screen.
- **The toggle**: an ordinary switch, "Show advanced options: Off / On"
  (Off by default; the maintainer's choice of 2026-11-26, section 9),
  first in a new Interface section, and an item in Options' More. Basic hides,
  never removes: a section at Basic ends with a line counting the
  settings changed at Advanced, with Reset and Show.
- **Words**: one message, two wordings: a Fluent attribute `.advanced`
  on the messages whose words differ, chosen by `ddi_l10n` from the
  level as it chooses the language. Ten pairs are proposed (section
  2.4).
- **The hierarchy**: 12 sections at Advanced (13 with desktop's
  Plugins), the same sections with fewer rows at Basic: Play, Arrows,
  Screen, Backgrounds, Rules, Timing, Sound, Controls, Song list,
  Multiplayer, Interface, Data and privacy. Display's 28 rows are split
  among them.
- **Side by side**: a chooser that opens over Options from a row: tiles,
  one per choice, the focused one live in the existing field preview,
  the rest stills drawn once by that same preview or drawn by the 2D
  pass alone. The first five: Arrows, Note colors, Behind the field,
  Judgment words, Touch controls.
- **The hidden screen**: "Experimental settings", reached by a typed
  address (`#/experimental`) or a flag (`--screen experimental`), then
  found again from the end of Advanced Options once used. Entries
  defined in one data file, English only as data; its frame translated.
- **Plugins**: their page under Advanced Options on desktop; "This
  computer" is Basic; a switched-on plugin's source stays in Find songs
  at Basic.

## 1. What is there today

### 1.1 Options as built

`ddi_ui::screens::options_model::OptionsModel::data` builds the rows
from `Settings` and the shell's `World` every time something changes;
`options.rs` draws them in layout A: the sections down the left (a strip
of tabs on a compact screen), the rows in the middle, the field preview
on the right (344 virtual px wide and up to 500 tall on a wide screen;
up to 320 wide sideways; the full width and 200–320 tall on an upright
phone), the focused row's sentence below. A row (`data::OptionDef`) is
an id, a label, a kind (`Choices`, `Stepper`, `Slider`, or `Actions`:
buttons, or none for a row that only says something), the index chosen,
a note beside it, one description, and an optional heading above it.
A section (`OptionSection`) is an id (its English name, which the shells,
the tests and the saved position use), a name and its rows. That is all
the structure there is: no folds, no pages within a section, no search.

The start prompt's play options (`prompt_options.rs`) are Options' own
rows taken by id from the Scroll, Notes and Arrows sections plus the
touch rows (`speed`, `reverse`, `turn`, `appearance`, `lane_cover`,
`noteskin` or `pump_noteskin`, `touch_controls`, `touch_size`), changed
for this play alone until "Save as default". A room's Rules and Host
options (`room_dialogs.rs`) are separate dialogs with rows of their own,
not the options model.

### 1.2 Every option

Columns: **who** needs it: *anyone*, someone who knows the *genre*, a
*developer* or tester; **effect**: visual, audible, feel (timing,
speed), or invisible; **today**: a row, conditional (shown only when
another row or the world says so), on the prompt too, hidden; **level**:
the level the rule of section 2 gives it (B Basic, A Advanced, H the
hidden screen), applied in section 2.2.

**Scroll** (4)

| Row | What it does | Who | Effect | Today | Level |
|---|---|---|---|---|---|
| `speed` | How fast the notes travel (×0.25 to ×10, the chart's tempo times this). | anyone | feel | row, prompt | B |
| `speed_source` | Charts with a speed of their own (Dancing☆Onigiri works) play at it or at mine. | genre | feel | row | A |
| `reverse` | Notes travel down to targets at the bottom. | anyone | visual | row, prompt | B |
| `scroll_action` | Boost, Brake or Wave: notes speed up, slow down or sway on the way. | genre | visual | row | A |

**Notes** (13)

| Row | What it does | Who | Effect | Today | Level |
|---|---|---|---|---|---|
| `turn` | Mirror, Left, Right, Shuffle: every note moved to another lane. | genre | feel | row, prompt | A |
| `appearance` | Hidden, Sudden, Both, Stealth: notes vanish or appear late. | genre | visual | row, prompt | A |
| `lane_cover` | Sudden+, Hidden+, Both: a panel over one end of the lanes. | genre | visual | row, prompt | A |
| `cover_sudden`, `cover_hidden` | How much of the lane each cover hides. | genre | visual | conditional | A |
| `cut` | Leaves out the faster notes (all, quarters, quarters and eighths); the play marked assisted. | anyone | feel | row | B |
| `simplify_onigiri` | Adds Dancing☆Onigiri charts without the onigiri lane to other styles. | genre | invisible (song list) | row | A |
| `simplify_center` | Adds Pump charts without their center to Single and Double. | genre | invisible (song list) | row | A |
| `at_once` | At most two (or one) notes pressed together. | anyone | feel | row | B |
| `no_stretch` | Leaves out wide combinations on the double layouts. | genre | feel | row | A |
| `no_holds` | Holds and rolls become taps. | anyone | feel | row | B |
| `convert_pump_single` | Adds Pump single charts to 5 keys. | genre | invisible (song list) | row | A |
| `convert_five_keys` | Adds 5-key charts to Pump single. | genre | invisible (song list) | row | A |

**Rules** (18; 11 shown under DDI, 17 under another ruleset)

| Row | What it does | Who | Effect | Today | Level |
|---|---|---|---|---|---|
| `ruleset_mode` | One ruleset for every chart, each chart's own game's, or Dancing☆Onigiri's. | genre | feel, scoring | row | A |
| `ruleset` | DDI, ITG, ITG FA+, StepMania 5, DDR A, DDR X, DDR EXTREME, FFR. | genre | feel, scoring | row | A |
| `ddi.difficulty` | Timing difficulty: Easy, Normal, Hard. | anyone | feel | under DDI | B |
| `ddi.boos` | A press on no note costs life and points. | genre | feel, scoring | under DDI | A |
| `ddi.practice` | An empty life does not end the play. | anyone | feel | under DDI | B |
| `ddi.score` | The score as points or as a percent. | anyone | visual | under DDI | B |
| `ddi.words` | The judgment words in English, Japanese, or the menus' language. | anyone | visual | under DDI | B |
| `holds` | Ticked or dance-style holds. | genre | scoring | not DDI | A |
| `fail` | When an empty life bar fails the play. | genre | feel | not DDI | A |
| `life` | StepMania's life difficulty 1–8. | genre | feel | not DDI | A |
| `judge` | StepMania's judge difficulty 1–8 and Justice. | genre | feel | not DDI | A |
| `life_type` | Bar or battery. | genre | feel | not DDI | A |
| `lives` | The battery's lives. | genre | feel | not DDI | A |
| `ddr_gauge` | DDR A's gauges (LIFE4, RISKY, FLARE…). | genre | feel | not DDI | A |
| `rejudge` | Judges every kept play again under the rules chosen. | genre | invisible | row | A |
| `danoni.gauge` | Dancing☆Onigiri's gauge. | genre | feel | row | A |
| `danoni.range` | Dancing☆Onigiri's judge range. | genre | feel | row | A |
| `danoni.excessive` | Dancing☆Onigiri's Excessive (early stray presses cost life). | genre | feel | row | A |

**Timing** (5 kinds)

| Row | What it does | Who | Effect | Today | Level |
|---|---|---|---|---|---|
| `calibrate` | Opens calibration. | anyone | feel | row (and the footer) | B |
| `audio_offset` | The audio offset for devices without a profile, by hand. | genre, technical | feel | row | A |
| `visual_offset` | The visual offset for displays without a profile, by hand. | genre, technical | feel | row | A |
| `profile.{audio,display}.N` | Each known device's offset ("48 kHz, 20 ms, wired"). | technical | feel | a row a device | A |
| `forget.{audio,display}.N` | Forgets a device. | technical | invisible | a row a device | A |

**Sound** (11 kinds)

| Row | What it does | Who | Effect | Today | Level |
|---|---|---|---|---|---|
| `volume` | Every sound's volume. | anyone | audible | row | B |
| `sample` | Plays a song's preview at this volume. | anyone | audible | where a song is there | B |
| `assist_clap` | A clap on every row of notes. | anyone | audible | row | B |
| `mine_sound` | A sound when a mine or shock is stepped on. | anyone | audible | row | B |
| `announcer` | The voice: off, ours, or a pack. | anyone | audible | row | B |
| `announcer.language` | The voice's language. | anyone | audible | where the pack has several | B |
| announcer tags (`announcer.tag.*`) | Lines a pack lets the player switch off (the pack's words, data). | genre | audible | a row each | A |
| announcer parameters (`announcer.param.*`) | A pack's other choices (data). | genre | audible | a row each | A |
| `announcer_volume` | The voice's share of the volume. | anyone | audible | with an announcer | B |
| `announcer.remove` | Removes an imported or studio-made announcer. | anyone | invisible | conditional | B |
| `announcer.studio` | Opens the announcer studio. | anyone | — | row | B |

**Display** (33: 28 always, 5 conditional)

| Row | What it does | Who | Effect | Today | Level |
|---|---|---|---|---|---|
| `field_place` | Where the lanes sit across the screen. | anyone | visual | row | B |
| `show_deltas` | Each hit's timing error in milliseconds under the judgment. | genre | visual | row | A |
| `opponent_fields` | Opponents' fields: as many as fit, or eight. | anyone | visual | row | B |
| `twitch_chat` | A room host's Twitch chat behind the play. | anyone | visual | row | B |
| `chat_style`, `chat_names`, `chat_emotes_from`, `chat_opacity` | The chat's motion, names, emote providers (privacy: other services see the address), opacity. | anyone | visual | with the chat | B |
| `dancing_hand` | The dancing hand beside the field. | anyone | visual | row | B |
| `dancing_hand_multiplayer` | The hand in a multiplayer stage too. | anyone | visual | with the hand | B |
| `hand_layout` | Where the field and the hand stand together. | anyone (fine tuning) | visual | row | A |
| `hand_frame_width`, `hand_frame_height` | The hand's camera frame. | anyone (fine tuning) | visual | row | A |
| `ghost` | How far ahead of or behind one's best. | anyone | visual | row | B |
| `receptor_snap` | Draws a note exactly on the receptor in its closest frame. | technical | visual (subtle) | row | A |
| `lyrics` | Shows songs' lyrics. | anyone | visual | row | B |
| `bg_brightness` | How bright songs' backgrounds are. | anyone | visual | row | B |
| `video` | Background videos: automatic, on, off. | anyone | visual | row | B |
| `random_movies` | StepMania's RandomMovies folder's videos. | genre | visual | row | A |
| `generated_background` | What a song without a background shows. | anyone | visual | row | B |
| `generated_over_pictures` | The generated background over songs' own pictures too. | anyone (fine tuning) | visual | row | A |
| `generated_palette` | The generated background's colors. | anyone | visual | row | B |
| `background_frame` | The shape backgrounds fill (4:3, 16:9, the screen). | genre | visual | row | A |
| `field_filter` | Darkens the background behind the lanes. | anyone | visual | row | B |
| `language` | The menus' language. | anyone | visual | row | B |
| `reduced_motion` | No slides in the menus, the art held still. | anyone | visual | row | B |
| `cursor_style`, `cursor_size` | The menu pointer and its size. | anyone | visual | row | B |
| `start_countdown` | A song starts by itself after this long. | anyone | feel | row | B |
| `chart_ratings` | The chart's number: the song's, MSD, the DDI estimate. | genre | visual | row | A |
| `graphics` | WebGPU or WebGL2 (web; on desktop the row shows and does nothing, 1.7). | technical | invisible | row | A |
| `debug` | The debug overlay. | developer | visual | row | A |
| `demo` | An autoplayed bundled song, on WebGL2 too. | developer | visual | row | A |

**Arrows** (12 kinds, with Infinito's seven parameters)

| Row | What it does | Who | Effect | Today | Level |
|---|---|---|---|---|---|
| `noteskin` | The arrows' look: Infinito, 8 more bundled, imported ones. | anyone | visual | row, prompt | B |
| Infinito's `Colors` | Vivid, Rainbow, Flat. | anyone | visual | row (skin's data) | B |
| Infinito's `NoteTimedCharge` | Each note's own charge cycle. | anyone (fine tuning) | visual | row | A |
| Infinito's `Onigiri` | The onigiri lane's shape. | genre | visual | row | A |
| Infinito's `ShockArrows` | Lane-shaped shocks in battles. | anyone (fine tuning) | visual | row | A |
| Infinito's `Explosions` | Hit flashes. | anyone | visual | row | B |
| Infinito's `ReceptorFill` | A faint fill in the receptors. | anyone (fine tuning) | visual | row | A |
| Infinito's `HitVariation` | Randomized burst rotation. | anyone (fine tuning) | visual | row | A |
| `noteskin.remove` | Removes an imported noteskin (web). | anyone | invisible | conditional | B |
| `pump_noteskin` | A separate skin for the Pump layouts. | genre | visual | where pump skins exist | A |
| `pump_noteskin.remove` | Removes an imported pump skin (web). | genre | invisible | conditional | A |
| pump skin parameters | That skin's declared parameters (data). | genre | visual | conditional | A |

Plus `preview_layout` (the preview's style, P; not saved): anyone, B.

**Controls** (14 kinds)

| Row | What it does | Who | Effect | Today | Level |
|---|---|---|---|---|---|
| `bind_layout` | Which style's bindings are shown. | anyone | — | row | B |
| `bind.any` | Binds the next device pressed. | anyone | feel | row | B |
| `keys`, `keys.lane.N` | The keyboard's bindings, a row a lane. | anyone | feel | row | B |
| `touch_controls`, `touch_hand`, `touch_size` | How a touch screen plays. | anyone | visual, feel | row, prompt | B |
| `pad.N`, `pad_style.N`, `pad.N.lane.M`, `pad.N.start`, `pad.N.back` | Each controller: bind, clear, defaults, forget; its button hints; its lanes; Start and Back. | anyone | feel | a group a controller | B |
| `fast_pad_poll` | Polls controllers every millisecond in play. | technical | feel | row | A |
| `pads.none` | Says the platform reads no controllers. | anyone | — | conditional | B |

**Data** (9; 8 on desktop) all B: `history.export` (export, import,
reset), `reset.settings`, `reset.songs`, `room_ahead`, `twitch` (Twitch
access), `privacy`, `usage_stats`, `error_reports` (web), and
`reset.everything`.

**Songs** (desktop, 2 kinds) both B: `folders.N` and `folders.add`.

**Sharing** (only with the hidden switch `Settings::sharing`, 4) all H:
`share.token`, `share.shares`, `share.server`, `share.about`.

### 1.3 Settings with no row in Options

Of `Settings`' 93 fields, these are stored without an Options row:

| Field | Where it is set | Level |
|---|---|---|
| `style` | song select's style | (song select) |
| `import_details`, `import_copy`, `import_videos` | the import dialog's toggles | details A; copy and videos B |
| `results_panels` | nothing (no picker since the panel sets' tabs went) | H |
| `song_offsets` | the results' offset editor | A |
| `players_folded`, `practice_roster`, `room_twitch` | a room's screens | (rooms) |
| `sharing`, `share_token` | `--sharing`, the web's `#/?share=1` for the page; the token in Sharing | H |
| `multiplayer_rules_first`, `multiplayer_watch_link` | `--mp-rules-first`, `--mp-watch-link`; the web's `mprules`, `mpwatchlink` for the page | H |
| `twitch_used` | kept by sign-ins | internal |
| `rules_moved`, `defaults_reset_2026_11_17`, `rejudge_from`, `announcer_moved`, `announcer_notice`, `ratings_moved` | migrations' marks | internal |

### 1.4 Jargon and technical detail outside Options

What a person who has never played the genre would not understand, or
a number only a technical reader uses. "Both" in the last column: the
same at both levels (a readout, or allowed by the rule); otherwise what
Basic does with it (section 2.5).

| # | Where | What shows | Kind | At Basic |
|---|---|---|---|---|
| 1 | Song select, chart rows and strip | The chart's number: the DDI estimate by default, else MSD or the song's own, the song's fainter where none | a rating whose source is technical | the number, whole |
| 2 | Song select | Difficulty names Beginner…Challenge, Edit (data, StepMania's) | the charts' own names | both |
| 3 | Song select, style button | Single, Solo, Double, Techno 8-panel, Pump single/half-double/double, N keys | layouts by their games' names (allowed) | both |
| 4 | Song select, radar | Its middle label: MSD, DDI or Song | jargon | plain word (pair 8) |
| 5 | Song select, radar | MSD's seven skillsets: Stream, Jumps, Hands, Stamina, Jacks, Chordjacks, Tech (and their long names) | Etterna's jargon | plain words (pair 7) |
| 6 | Song select, radar | The estimate's eight, with Evasion, and "Skills, compared with charts this hard" | jargon | plain words |
| 7 | Song select, radar | Groove's Stream, Voltage, Air, Freeze, Chaos (From the song) | DDR's jargon | plain words |
| 8 | Song select, radar | A number on each spoke | detail | none |
| 9 | Song select, info line | "{artist} · {bpm} BPM · {length}" | musical, common | both |
| 10 | Song select | Version marks: Without onigiri, Without center, From Pump single, From 5 keys | genre (on by default: section 2.8) | both |
| 11 | Song select | ASSIST, AUTOPLAY marks | plain | both |
| 12 | Song select, More | Chart history, Watch best, Edit chart | plain | both |
| 13 | Start prompt | The modifiers line: ×2.5, C400, mirror, sudden+ 30%, battery 3 lives, life 5, judge 6… (`mods.ftl`, 32 messages) | genre | plain words where pairs exist (pair 9) |
| 14 | Start prompt | "Hits run 12 ms late", "Timing not calibrated" | technical, plain enough | both |
| 15 | Start prompt | The solo CPU's "CPU · Tap-only · No fail · Strength 1.5× · Seed 7" (address-only) | developer | both (unreached at Basic) |
| 16 | Play HUD | Judgment words (DDI's twelve; other rulesets' own) | readouts (data) | both |
| 17 | Play HUD | Early and late | readout | both |
| 18 | Play HUD | Each hit's error in ms (option) | technical | where set |
| 19 | Play HUD | Debug overlay (option) | developer | where set |
| 20 | Play HUD | Score (about 7,060,000 for 1,000 notes) or percent | readout | both |
| 21 | Results | Grades (SSS…D, F, QUIT, Absolute Dito, Absolute Bot), lamps | readouts | both |
| 22 | Results | The tiers' counts | readouts | both |
| 23 | Results | Early / late (Fast / slow under StepMania's) | readout | both |
| 24 | Results | Max combo, Accuracy, Combo score, the next grade's line | readouts | both |
| 25 | Results | Ticks hit / missed, Mines hit, Stray presses | readouts where they apply | both |
| 26 | Results | Flare skill, Total score, Raw goods (DDR A, FFR) | a ruleset's own | both (Advanced rulesets) |
| 27 | Results, Timing panel | "mean 4 ms · σ 9" | statistics | histogram without σ |
| 28 | Results | Life panel, "largest drop 23% at 1:02" | plain | both |
| 29 | Results | Offsets over the song, ± ms | technical | not shown |
| 30 | Results | By beat, "16th 96 −11.2 σ 9.9" | technical | not shown |
| 31 | Results | By lane (the Song set, no picker today) | technical | not shown |
| 32 | Results | The song's offset editor: Offset −1 ms, +1 ms, Use mean offset, Reset | technical | conditional (open question 7) |
| 33 | Results | Mean offset line | technical | plain words (pair 6) |
| 34 | Results, notes | fail delayed, fail at end, fail off; "judged again here by X rules" | genre | both |
| 35 | Results, notes | Video turned off for dropped frames, decoder behind; scripts stopped (string grew too long…); sound dropped | technical | both (they explain what happened) |
| 36 | Results, replays | "judged differently… inputs judged at once differ", judge versions | technical | both (replays are rare) |
| 37 | Calibration page | "100 BPM, three clicks and a rest… 95% confidence", "±250 ms", "audio path" | technical | the plain lines only |
| 38 | Calibration page | Each device's offset in ms, defaults, Reset offsets to 0 | technical | the current device in a sentence |
| 39 | Calibration runs | "the test adjusts itself and stops once it converges" | technical word | plain word |
| 40 | Options' preview caption | "Infinito · Single · ×2.5 · Reverse" | genre (×) | both |
| 41 | Options' headings | DANCING☆ONIGIRI RULES, KNOWN DEVICES, DIAGNOSTICS | genre, technical | their rows' level |
| 42 | Options, devices | "48 kHz, 20 ms, wired", "1920×1080 @2×, 120 Hz" | technical | not shown (A rows) |
| 43 | Options, controllers | The controller's id as it reports it, numbered buttons and axes | technical, unavoidable | both |
| 44 | Options, Infinito's parameters | English data from its metrics: "Original SVG flashes", "inset charge regions", "broad-burst rotation" | technical, untranslated | proposal in 1.7 |
| 45 | Import dialog | ".sm, .ssc, .dwi", FFR's ".swf", Dancing☆Onigiri | formats by name (allowed) | both |
| 46 | Import dialog | "Details for pack authors" and the report's details | technical | A |
| 47 | Import dialog (desktop) | The DanOni URL tab, "Allow files from this address?" | genre, network | both |
| 48 | Chart editor | BPM, stops, delays, warps, speeds, scrolls, fakes, ticks, combos, labels, snap, waveform, spectrogram (236 messages) | the format's own | both (section 2.6) |
| 49 | Room Rules | Boos, Shocks per attack, Attacks at once, Charts: Adaptive, Points Strong/Medium/Mild, When life runs out | partly jargon | words only (pair 10) |
| 50 | Room | A CPU's stars and level, "not a skill rating" | plain | both |
| 51 | Room Rules | "Everyone plays the match under DDI" | a ruleset by name | both |
| 52 | Announcer studio | Tags, StepMania's layout export, Tidy (under its Advanced fold already) | technical | both (the page has its own fold) |

52 places. Besides these, about 20 messages and the privacy note name
a place in Options ("Options → Data → Send usage statistics"), which
the hierarchy of section 3 moves (stage 3).

### 1.5 Testing and developer switches

- **The web's address** (the verify skill's "What an address can
  carry", round HY): 31 parameters read on any route at load: `gfx`,
  `auto`, `bias`, `slow`, `perf`, `load`, `msdslow`, `touchlook`,
  `lang`, `share`, `mp`, `mprules`, `mpwatchlink`, `mpattacks`,
  `mpcpus`, `mpdevmines`, `mpfakeseat`, `mpfakeend`, `mpfakematch`,
  `mpfakeroom`, `mpfakeready`, `mpfakelive`, `mpfakefail`,
  `mpfakeslow`, `mpfakeelsewhere`, `mpfakechat`, `mpfakechatimg`,
  `mpfakechaturl`, `mpfakechatlook`, `mpfakechatrules`, `signin`; and
  the routes' own (`#/play`'s `song`, `chart`, `cpu`, `level`…). Since
  round HY none is stored: each lasts the page or the tab.
- **Script switches**: `window.__DDI_*` set by an init script
  (`__DDI_FORCE_WASM_DECODE`, `__DDI_FORCE_WASM_ENCODE`,
  `__DDI_SKIN_DELAY_MS`, `__DDI_FAKE_PAD`…), never in an address.
- **Desktop's flags** (`ddi_desktop::args`): about fifty, in groups: what
  to open (`--screen`, `--play`, `--edit`, `--replay`, `--calibrate`,
  `--shared`, `--join`), three saved switches (`--sharing`,
  `--mp-rules-first`, `--mp-watch-link`) and a dead one
  (`--multiplayer`), development (`--mp-dev-cpus`, `--mp-dev-mines`,
  `--mp-dev-chat` and its socket, providers and recording, `--dev`,
  `--widgets`, `--load-ms`), measurement (`--pacing`, `--in-flight`,
  `--pacing-report`, `--seconds`, `--video-check`, `--decoder`),
  utilities (`--share`, `--shares`, `--delete-share`, `--mp-bots` with
  its own twenty, the DanOni checks), the CPU (`--cpu*`) and the test
  rig's (`--silent`, `--volume`, `--fake-pad`, `--lang`,
  `--lose-device-after`).
- **Environment**: `DDI_TOUCH_LOOK`, `DDI_VIDEO_DECODER`,
  `DDI_VIDEO_PLANES`, `DDI_SKIN_DELAY_MS`, `DDI_SYSTEM_LANGUAGES`,
  `DDI_CPU_SCALE`, `DDI_CPU_PLAY`, and the tests' own.
- **The developer screen** (`crates/ui/src/dev.rs`, `#/ui-dev`,
  `--dev`): the UI core's primitives, not settings.

### 1.6 Counts

| What | Count |
|---|---|
| Stored settings (`Settings`' fields) | 93 |
| Options' sections | 9, plus Songs (desktop) and Sharing (hidden switch) |
| Kinds of row (a family of rows, as a device's or a lane's, counted once; Infinito's seven parameters counted) | 126 |
| Of them, by the rule of section 2: Basic / Advanced / hidden | 69 / 53 / 4 |
| Settings stored with no Options row (beside migration marks and room state) | 11 |
| Places outside Options with jargon or technical detail | 52 |
| Messages naming a place in Options | about 20, and the privacy note |
| The web's address parameters (general) | 31 |
| Desktop's flags | about 50 |
| Prompt rows | 8 (6 shown without touch) |

### 1.7 Found on the way

- **Graphics on desktop.** Display's `graphics` row ("WebGL2… Takes
  effect when the page is loaded again") and the demo's "On WebGL2"
  button are built on both shells; the desktop shell never reads
  `Settings::graphics`. They belong to the web alone.
- **Unused messages.** `options-skill-radar` and its description stayed
  after round SLB took the option away; no code calls them.
- **Infinito's parameter texts are English data.** Skins' parameters
  show their metrics' own words as data (right for third-party skins);
  Infinito is ours, and its seven labels and descriptions are never
  translated and say "SVG", "inset charge regions". Proposal: Infinito's
  parameters, and any skin's that name a message id in an
  `L10nLabel=`/`L10nDescription=` key of ours, show messages.
- **A default and a choice cannot be told apart** (`Settings::ruleset`'s
  comment). The levels lean on "different from the default" (Basic's
  count of changes, the hidden screen's marks); a person who chose the
  default counts as unchanged, which is what they would expect.

## 2. The two levels

### 2.1 The rule

An option, a word or a detail is **Basic** when all three hold:

1. Someone who has never played a rhythm game can tell from its name
   and one plain sentence what it changes.
2. They could want to change it, and they can see, hear or feel the
   change in the game.
3. A wrong value cannot quietly make the game worse in a way they
   cannot trace back (a hand-set offset, a poll rate).

Whatever the three say, these are **always Basic**: privacy and
consent (usage statistics, error reports, the privacy note, Twitch
access, the emote providers), resets and removing what one added, the
volume, the language, accessibility (reduced motion, the cursor's size),
controls and bindings, calibration itself.

It is **Advanced** when any of these holds:

- it names a concept of the genre or of another game (X-mod, Sudden+,
  Boos, jacks, judge 4, MSD, a gauge's name);
- it moves judging or scoring away from the game's own (another
  ruleset, a fail type, a life or judge level);
- it tunes a mechanism by a number whose effect needs explaining (an
  offset by hand, a frame or a poll rate, a camera frame's width);
- it serves content brought from other games (a pump skin, StepMania's
  RandomMovies, the versions made from other games' charts);
- it is a finer control under a Basic one (a cover's height, the
  hand's frame).

It goes to the **hidden screen** when it is for developing or testing
the game, is an experiment not yet decided, or is a switch kept for a
few people (the watch-only link's Copy).

Tie-breakers: in doubt, an option goes to Advanced and is promoted later
(a Basic row is a promise to every player); but anything a person needs
to fix a problem stays Basic. The maintainer leaned this further on
2026-11-26: "I think we should lean into erring more towards marking a
simple option as advanced than marking a complex option as basic." So a
row stays Basic only where one is confident that a person new to rhythm
games understands its name and its one-line description without knowing
the genre, would plausibly want it, and cannot quietly make their game
worse or confusing with it; anything one hesitates on is Advanced. The
always-Basic list above stands. A readout (a judgment, a grade, a score)
is the same at both levels: it is what happened, not a word to choose.
A ruleset or a layout named by its own game's name is allowed at both
levels (as `AGENTS.md` allows it now); what Basic leaves out is the
genre's community vocabulary and the technical numbers.

### 2.2 Applied

(Before the maintainer's lean of 2026-11-26; section 9 has the rows as
leaned, 18 of them moved from Basic to Advanced.) The level column of
the tables in 1.2 applies the rule: 69 Basic, 53
Advanced, 4 hidden. By section (Basic of the whole): Scroll 2 of 4,
Notes 3 of 13, Rules 4 of 18, Timing 1 of 5, Sound 9 of 11, Display
21 of 33, Arrows 4 of 12 and the preview's selector, Controls 13 of 14,
Data 9 of 9, Songs 2 of 2, Sharing 0 of 4 (hidden).

The rows whose placement is a judgment call, for the maintainer to
check first: `turn` and `appearance` (classic modifiers on the prompt,
Advanced here), `cut`, `at_once` and `no_holds` (assists a beginner
wants, Basic), `assist_clap` (a learning aid under StepMania's name,
Basic with a plain name), `ghost` (Basic with a plain name),
`chart_ratings` (Advanced: Basic keeps the default's number),
`chat_emotes_from` (Basic, for privacy), `hand_layout` (Advanced).

A skin's parameter takes a level from its metrics (`Level=Basic`, our
extension, read by `ddi_noteskin::params`); without one it is Advanced,
so imported skins show their parameters at Advanced. An announcer's
tags and parameters are Advanced.

### 2.3 The toggle

(The maintainer chose a switch on 2026-11-26, "Show advanced options:
Off / On": section 9. The proposal as it was:) An ordinary row, first in the Interface section of section 3 (until
that exists, under Display's MENUS heading beside Language), and an item
in Options' More ("Show advanced options" / "Show basic options"), so a
site's instructions can say either way.

- **English draft**
  - Label: "Detail"
  - Choices: "Basic", "Advanced"
  - Description: "Basic shows the options most players need, in plain
    words. Advanced shows every option and more numbers, with the terms
    players of StepMania, ITGmania and Etterna know. Nothing about how
    you play changes."
- **Japanese draft** (unverified, by `locales/ja/GLOSSARY.md`)
  - Label: 「表示の詳しさ」
  - Choices: 「基本」「上級」
  - Description: 「基本では、多くのプレイヤーに必要なオプションだけを、
    わかりやすい言葉で表示します。上級では、すべてのオプションと細かい
    数値を、StepMania・ITGmania・Etternaのプレイヤーになじみのある用語で
    表示します。プレイの内容は変わりません。」

Other names weighed: "Options shown" (says too little: the words
outside Options change too), "Advanced options: On/Off" (an ordinary
switch, but "Advanced: Off" reads oddly when the words change too),
"Interface: Basic/Advanced" (clear, but the section is named Interface
already). "Detail" with its two choices reads right in a row and in
instructions ("set Detail to Advanced").

Flipping it rebuilds every screen in place, as a language change does
(`Effect::Language`, `Cx::rebuild_screens`), and keeps Options' place:
the focused row where it is still shown, else the toggle.

### 2.4 What changes when it flips

**Which options appear.** At Basic, Options and the prompt show the
Basic rows. Basic hides, never removes: every setting keeps its effect
whatever the level. Two ways to show that something is set:

- *Options* (recommended): each section at Basic ends with one row
  where it has Advanced rows changed from their defaults: "Changed in
  Advanced: 3", with **Reset them** (asked once, naming the count) and
  **Show** (sets Detail to Advanced and focuses the first of them). No
  jargon at Basic: the row counts, it does not name. Other way: the
  changed Advanced rows stay shown at Basic, marked; simpler, but
  brings the jargon back exactly for those who had it.
- *The start prompt* (recommended): an Advanced prompt row (Turn,
  Appearance, Lane cover) changed from its default stays on the prompt
  at Basic, since the prompt is about this play and a modifier in
  effect must be visible and undoable there.

"Changed" is "different from `Settings::default()`" row by row: the
model builds the default's rows once (`OptionsModel::new(Settings::default(),
world).data()`) and compares indices by id; device rows, skin and
announcer parameters compare with their declared defaults.

**Which words change: the mechanism.** Four ways, weighed:

| Way | How | Translators | The `qps-ploc` walk | Code |
|---|---|---|---|---|
| A. Two messages | `options-reverse-description` and `options-reverse-description-advanced`; the code picks | two entries apart; easy to change one and forget the other | both reached only if the walk runs at both levels | every call site picks by level |
| B. A `$level` selector | `{ $level -> [advanced] … *[basic] … }`, passed by the caller as `$platform` is | both wordings in one message | as A | every call site passes the level; every signature that varies changes |
| C. An attribute `.advanced` (recommended) | the message's value is Basic's; `.advanced` the genre's; the generated function returns the attribute where the level is Advanced and the message has one, as the language is chosen today, implicitly | both wordings in one entry, the attribute only where they differ | runs at both levels | no call site changes; `crates/l10n/build.rs` (which refuses attributes today, "the generated functions do not take yet"), `xtask l10n check` (a language gives `.advanced` exactly where English does), `ddi_l10n::set_level` beside `set_language` |
| D. An implicit `$level` | `ddi_l10n::format` adds `level` to every message's arguments | a selector inside any message, so half a sentence can vary | runs at both levels | breaks the build's rule that a message's variables are declared; invites building sentences from pieces |

C keeps whole sentences (the localization rule), costs translators
only the messages that differ, and needs no call site to know the
level. A review's hash already covers attributes (`xtask/src/l10n.rs`
hashes the serialized message), so a message given an `.advanced`
becomes unverified by itself. Japanese may make the two equal where its
words already say both.

```ftl
# Options → Play, Reverse's description.
options-reverse-description = Arrows move down to targets at the bottom.
    .advanced = Arrows scroll down to receptors at the bottom.
```

**Ten pairs** (Basic, then Advanced; the Advanced wording is today's
unless marked new):

| # | Message | Where | Basic | Advanced |
|---|---|---|---|---|
| 1 | `options-reverse-description` | Options, Play | Arrows move down to targets at the bottom. | Arrows scroll down to receptors at the bottom. |
| 2 | `options-speed-description` | Options, Play; the prompt | How fast the arrows move. Faster spreads them further apart, which can make busy songs easier to read. | (new) Scroll speed as a multiple of the chart's tempo (an X-mod): ×2.5 on a 150 BPM chart scrolls as 375. |
| 3 | `options-cut`, `-quarters`, `-eighths` | Options, Play | Fewer notes: All / On the beat / On and between beats | Notes: All / Quarters / Quarters and eighths |
| 4 | `options-ghost` | Options, Screen | Compare with your best | Ghost of your best |
| 5 | `options-field-filter` | Options, Backgrounds | Darken behind the arrows | Field filter |
| 6 | `results-mean-offset` | Results | Average timing | Mean offset |
| 7 | `select-radar-jackspeed`, `-chordjack` | Song select's radar | Repeats / Chord repeats | Jacks / Chordjacks |
| 8 | `select-radar-overall`, `-estimate-overall` | Song select's radar | Overall | MSD / DDI |
| 9 | `mods-cover-sudden`, `-hidden`, `-both` | The prompt's modifiers line | cover (incoming) 30% | sudden+ 30% |
| 10 | `room-rules-boos` | A room's Rules | Stray presses | Boos |

More of the same kind, for the stage that writes them: the
results' "Offsets over the song" (Basic "Timing through the song"),
calibration's "stops once it converges" ("stops once it is sure"), the
visual and audio offsets' names ("Picture delay", "Sound delay"), the
radar's other skill names, Groove's five, the Turn and Appearance
choices on the prompt, "Assist clap" ("Clap on every note").

**Which details show outside Options.**

| Place | Basic | Advanced |
|---|---|---|
| Song select's number | the rating chosen, whole | MSD to a tenth where shown (today's `meter_text_fine`) |
| Song select's radar | the shape and plain names, no numbers | today's: names and numbers |
| The prompt | Basic rows, and changed Advanced ones | every prompt row |
| The HUD | identical (its options decide) | identical |
| Results | grade, score or percent, tiers, max combo, early and late, accuracy, combo score, the next grade's line, the Life panel, the Timing histogram without σ, the notes | today's: also Offsets over the song, By beat, σ, the mean offset, the offset editor |
| The song's offset editor | shown only where the song has an offset already or the suggestion is large (open question 7) | always, as today |
| Calibration's page | the three tests in plain words and "Last test"; the current devices' result in a sentence | today's: the about paragraph, every device's offset in ms, the defaults, Reset offsets |
| Rulesets by name | DDI's own rows only; a ruleset's name where a readout gives it (the rejudged note, the room's note) | every ruleset |
| Judgment windows in ms | shown nowhere | as today (descriptions; the debug overlay) |
| The import dialog | Details for pack authors hidden | shown |

**What is the same at both levels.** Everything a play does: judging,
scoring, what is kept, the bests, the replays; the screens and their
order, the routes, keys, controller and touch; the HUD; the readouts;
rooms' rules and what they decide.

### 2.5 Per device, the first run, and those who play already

- **Per device.** The settings are kept per browser profile and per
  desktop data folder, with no sync, so the level is per device as
  everything else is. Nothing to decide.
- **The first run.** Three ways:
  - start Basic, no question (recommended): the start menu stays as it
    is, the first screens are readied before the loader fades
    (`AGENTS.md`, "Deployment") and a dialog would come before the game;
  - one skippable question at the first start ("Have you played games
    like StepMania, ITGmania or Etterna?"): quick, but one more thing
    before the first song, and on the web the loader's fade would
    reveal a dialog;
  - offer it where the person showed they know the genre (recommended
    with the first): desktop's first start already offers other games'
    song folders (`folders_offer.rs`, StepMania 5, ITGmania, OutFox);
    after "Add them", one line in song select: "You play other dance
    games: Options → Interface → Show advanced options uses their
    terms." The
    web's Add songs of a StepMania pack could do the same once.
- **Those who play already.** Their settings were stored before the
  level existed. Recommended: a settings migration with its own mark,
  as the moves before it (`ratings_moved`): settings with any Advanced
  row changed from its default start at Advanced; the rest at Basic.
  The maintainer and testers keep everything; a casual player meets
  Basic once. Other way: everyone at Basic, told once.

### 2.6 Places: Basic forms or Advanced only

| Place | Recommended | Why |
|---|---|---|
| Rooms | the same at both levels, words only (pair 10) | a room's rules are everyone's; a member must see what the host set |
| The chart editor | the same at both levels; its entry ("Edit chart", "New song from a music file") at both | an authoring tool whose words are its file format's; hiding it hides a feature, and a Basic form would be a second editor |
| The announcer studio | the same at both levels | already plain, with its own Advanced fold |
| Calibration | a Basic form (2.4) | everyone may need it; its numbers are for the few |
| The import dialog | the same, Details for pack authors at Advanced | |
| Plugins (desktop) | Advanced only (section 6) | |
| The hidden screen | neither (section 5) | |

### 2.7 Today's defaults that assume knowledge

Not changed by this plan; listed for the maintainer:

- Speed ×2.5, an X-mod whose description speaks of "the chart's tempo".
- The chart's number is the DDI estimate, and the radar always shows
  Etterna's skillset names.
- DDI's judgment words in English, twelve of them (Absolute, Awesome,
  Amazing, Fantastic, Glorious…), early and late told apart by word.
- The score display is the sum (about 7,060,000 for 1,000 notes).
- Without center versions are listed where missing (pump charts turned
  into Single for everyone, marked "Without center"), and both
  conversions are on.
- Practice is off: an empty life ends the play, for a first-time player
  too.
- Controllers polled every millisecond; receptor snap on (invisible,
  right).
- The announcer, the dancing hand, the generated background and the
  start countdown are on (plain, fine).

### 2.8 The convention in `AGENTS.md`, reworded (a proposal)

Today: "UI text avoids jargon from specific games except where naming a
ruleset or a layout by StepMania's own name for its game". Proposed,
once the levels exist:

> UI text at the Basic level uses no terms of the genre's community or
> of other games (X-mod, Sudden+, MSD, jacks, Boos, judge 4) and no
> technical numbers, except a ruleset, a layout or a file format named
> by its own game's name. Such terms are the Advanced level's: on a row
> the levels show at Advanced only, or as a message's `.advanced`
> wording (`locales/README.md`), using the names players of StepMania,
> ITGmania and Etterna know.

## 3. The hierarchy

### 3.1 Today

| Section | Rows | What is in it |
|---|---|---|
| Scroll | 4 | speed and how notes move |
| Notes | 11, 13 with covers | per-play modifiers, assists, and what the song list offers (versions, conversions) |
| Rules | 11 under DDI, 17 otherwise | rulesets, DDI's, StepMania's, Dancing☆Onigiri's, judgment words, score display, kept plays |
| Timing | 3, and 2 a device | calibration, offsets, devices |
| Sound | 6 to 11 | volume, sounds, the announcer |
| Display | 28, 33 with the chat and the hand | the field's place, HUD parts, opponents, Twitch chat, the hand, backgrounds, the menus' language and motion, the cursor, the countdown, chart ratings, graphics, debug |
| Arrows | 8 with Infinito | the skins and their parameters |
| Controls | 7, a row a lane, 5 a controller | bindings, touch, controllers |
| Data | 9 | the history, resets, room songs ahead, Twitch access, privacy |
| Songs | desktop | song folders |
| Sharing | hidden switch | the uploader |

What goes wrong: Display is eight topics in one list; the song list's
settings sit in Notes (versions) and Display (chart ratings, countdown);
judgment words and score display, which only change what is drawn, sit
in Rules; Room songs ahead sits in Data; the language is under a heading
halfway down Display; the hand's five rows are among the HUD's.

### 3.2 Principles

- **Where a person looks.** By what they see change: the play (speed,
  assists), the arrows, the screen around them, the background, the
  sounds, the menus. Rules, timing and data are their own places.
- **Scope.** Per play (the prompt's rows: chosen per song, saved as
  default), per device (offsets, graphics, touch size, poll rate, room
  songs ahead's device default), per skin (parameters), per song (its
  offset, on the results), per room (the room's rules, never Options).
  The section says where it applies in its first line where that is
  not obvious ("This device only").
- **The prompt.** The prompt keeps the per-play rows. Proposed
  addition at both levels: "Fewer notes" (`cut`), an assist chosen song
  by song.
- **The same sections at both levels.** A path in instructions stays
  true at either level; a section with no Basic row is Advanced only.
- **Two levels deep.** Sections and headings, as now; the choosers
  (section 4) are the only thing that opens over a row.

### 3.3 The proposed tree

Advanced (13 on desktop, 12 on the web); Basic shows the rows marked B.

1. **Play** (per play; the prompt's rows come from here)
   - B Speed, Reverse; A Charts with their own speed, Scroll action,
     Turn, Appearance, Lane cover (and its heights)
   - EASIER: B Fewer notes, At once, No holds; A No stretch
2. **Arrows** (how the notes look; the preview's home)
   - B Arrows (the chooser, 4.4), Note colors, Explosions; A
     Infinito's other parameters, Pump arrows and theirs; B Remove
3. **Screen** (what play draws around the notes)
   - B Field position, Judgment words, Score display, Compare with your
     best, Lyrics; A Timing error of each hit, Receptor snap
   - DANCING HAND: B Dancing hand; A Field + hand layout, hand frame
     width and height
4. **Backgrounds**
   - B Background brightness, Darken behind the arrows, Background
     videos, Generated background, Generated colors; A Random movies,
     Over song backgrounds, Background frame
5. **Rules**
   - B Timing difficulty, Practice; A Boos, Rules for each chart,
     Ruleset, StepMania's rows, Dancing☆Onigiri's rows, Kept plays
6. **Timing**
   - B Calibrate, and this device's result in a sentence (new); A
     Default audio and visual offsets, known devices
7. **Sound**
   - B Volume, Sample, Clap on every note, Mine sound, Announcer, its
     language and volume, Your own announcer; A the pack's tags and
     parameters
8. **Controls** (as today)
   - B everything; A Poll every millisecond
9. **Song list**
   - B Start countdown, Song folders (desktop); A Chart ratings,
     Without onigiri, Without center, Pump on 5 keys, 5 keys on Pump
10. **Multiplayer**
    - B Opponents' fields, Dancing hand in multiplayer, Room songs
      ahead, Twitch chat in rooms and its four rows
11. **Interface**
    - B Show advanced options (the toggle), Language, Reduced motion, Cursor, Cursor
      size
    - TROUBLESHOOTING: A Graphics (web), Debug overlay, Autoplay demo,
      Experimental settings (section 5, once used)
12. **Data and privacy**
    - B Play history, Twitch access, Privacy note, Send usage
      statistics, Send error reports, Settings, Imported songs (or Song
      folders), Everything
13. **Plugins** (desktop, Advanced only; section 6)

Sharing, today a section behind its switch, becomes the hidden
screen's switch; while it is on, its four rows show at the end of Data
and privacy at Advanced.

Basic, as a person sees it on the web: Play (5 rows), Arrows (3 with
Infinito), Screen (6), Backgrounds (5), Rules (2), Timing (2), Sound
(8), Controls (7 and a row a lane, more with controllers), Song list
(1), Multiplayer (4, 8 with the chat), Interface (5), Data and privacy
(9): about 60 rows against today's 90 or so shown at once.

### 3.4 What moves

| Row | From | To |
|---|---|---|
| `speed`, `speed_source`, `reverse`, `scroll_action` | Scroll | Play |
| `turn`, `appearance`, `lane_cover`, heights, `cut`, `at_once`, `no_holds`, `no_stretch` | Notes | Play |
| `simplify_onigiri`, `simplify_center`, `convert_*` | Notes | Song list |
| `ddi.words`, `ddi.score` | Rules | Screen |
| `field_place`, `show_deltas`, `ghost`, `receptor_snap`, `lyrics`, the hand's five | Display | Screen |
| `bg_brightness`, `video`, `random_movies`, `generated_*`, `background_frame`, `field_filter` | Display | Backgrounds |
| `opponent_fields`, `twitch_chat` and the chat's four, `dancing_hand_multiplayer` | Display | Multiplayer |
| `room_ahead` | Data | Multiplayer |
| `language`, `reduced_motion`, `cursor_style`, `cursor_size` | Display | Interface |
| `graphics`, `debug`, `demo` | Display | Interface, Troubleshooting |
| `start_countdown`, `chart_ratings` | Display | Song list |
| `folders.*` | Songs (desktop) | Song list |
| `share.*` | Sharing | Data and privacy, behind the switch |

Sections Scroll, Notes, Display and Songs go; their ids are kept as
aliases for a saved position (`OptionsScreen::with_position` by section
id) and for tests.

### 3.5 What the options model lacks

1. **A level per row** (`OptionDef::level`), per skin parameter (the
   metrics key) and per announcer parameter; `OptionsModel::data` and
   `prompt_rows` filter by it.
2. **A default per row**, to count and mark what is changed:
   `OptionDef::default: Option<usize>`, from `Settings::default()` or a
   declared default.
3. **A presentation per row**: `OptionDef::chooser: Option<Chooser>`
   (which tiles, section 4); Left and Right still step the row, Enter or
   a tap opens the chooser.
4. **Prompt rows by id**, not by section: `prompt_rows` takes the
   Scroll, Notes and Arrows sections today.
5. **Section ids that can change**: they are the English names, used by
   the saved position and the tests; aliases for the old ones.
6. **Room for explaining**: one sentence a row, the panel sized for the
   longest (`desc_height`); rules that need a paragraph and an example
   ("explained", 4.1) need a second line of text or a fold in the
   description panel.
7. **Search**: none; not proposed now (Advanced's 120-odd rows in 13
   sections are findable; revisit with plugins' growth).
8. **Messages that name places**: about 20 and the privacy note
   ("Options → Data → …") must follow the tree (stage 3).
9. **Rooms' dialogs** keep their own rows; only their words take the
   level.

## 4. Choices side by side

### 4.1 Sorting the options

| How a choice is best shown | Options |
|---|---|
| **The live field preview is enough** (one value at a time, the change visible at once) | Speed, Reverse, Turn, Fewer notes, At once, No holds, No stretch, the covers' heights, Field position, Timing error, Score display, Compare with your best, Receptor snap, Explosions and Infinito's other toggles, Background brightness alone |
| **Side by side, before picking** (the look matters most, and a moving preview of one choice at a time hides the comparison) | Arrows (noteskin), Note colors, Pump arrows, Appearance, Lane cover mode, Darken behind the arrows with Background brightness, Background frame, Generated background and its colors, Judgment words, Cursor, Touch controls (style, extra button, size), Opponents' fields, Chat style, Field + hand layout |
| **Heard** | Volume (the sample), Clap on every note, Mine sound, Announcer, its language and tags (a line said per choice) |
| **Felt: a try-it strip** (playing a few seconds yourself) | Speed between two values, Timing difficulty, Scroll action, offsets (calibration is the existing form) |
| **Explained rather than shown** (a sentence and an example) | Rulesets and Rules for each chart, Fail, Life, Judge, Battery, DDR's gauges, Dancing☆Onigiri's rows, Boos, Practice, Holds, Chart ratings, the versions and conversions (what the song list gains), Kept plays, Room songs ahead, Usage statistics, Error reports, Video's Automatic |
| **Nothing to show** | Language, Start countdown, bindings, Graphics, Debug overlay, the resets |

### 4.2 What it costs to draw

What exists:

- **The field preview** (`ddi_render::field_preview::FieldPreview`)
  owns a whole `Renderer` (its pipelines, its HUD text atlas, a plate
  for the background), plays the demo chart with the options and draws
  it, HUD and all, into a texture the 2D pass shows; it can seek to a
  time and draw that frame (its snapshots); one is made for the app and,
  on the web, outlives the pages. Its skin is readied as a play's,
  through the shared skin cache, one load of a skin at a time.
- **Miniatures** (`scene::Miniature`): the renderer already draws up to
  72 small fields in two runs, with the play's one noteskin and note
  colors and no Lua per field.
- **The skin preview** (`RenderOptions::preview`, `#/skin-preview`):
  a skin's elements alone at a given geometry.
- **Generated stills** (`ddi_shell::generative::Stills`): the generated
  background's frame at a song second, one source at a time; it needs
  the music's analysis.
- **The 2D pass** draws images, text and quads cheaply: the touch
  glyphs' atlas, the cursors' SVG, the HUD's faces.

Four ways to draw N tiles:

| Way | Cost | Fits |
|---|---|---|
| N live previews | N renderers, N skin loads, N targets each frame; not measured, too heavy for a phone or WebGL2 | none |
| One preview, stills (recommended) | each choice drawn once by the existing preview (ready the choice, seek to a fixed demo time, copy the frame into a painter texture), cached for the session; the focused tile live in the preview itself; memory a small texture a tile (9 tiles of 160×240 at 4 bytes: about 1.4 MB) | anything the field preview draws: skins, their parameters, appearance, covers |
| Miniatures | N live small fields in one renderer, one skin, one set of note colors | choices that change where notes are drawn (Appearance, Lane cover), to verify |
| The 2D pass alone | quads, images and text | judgment words, the cursor, touch controls, darkening composed over one shared still |

On WebGL2 and a phone, stills cost a render each when the chooser
opens (then nothing), and the skin loads, which are the slow part:
Infinito's SVG drawings go through the SVG worker on the web. Tiles
fill in as they are drawn, each showing its name meanwhile.

### 4.3 The chooser

One generic dialog in `ddi_ui`, opened by Enter, Start or a tap on a
row that has one (Left and Right still step the row in place):

- A grid of tiles, the chosen one marked, focus on it at first; the
  arrows, a pad's directions and the shoulders move; Enter, Start or a
  tap picks and closes; Back or a tap outside closes without a change.
- The focused tile is shown live in the field preview's panel (the
  preview follows the focused choice while the chooser is open, and
  returns to the setting when it closes without a pick).
- Each tile's name under it (a message, or data for a skin's or a
  pack's name); a one-line note where a choice needs one.
- Wide screen: the grid over the rows' area, the preview panel beside
  it as now. Sideways phone: the grid over the rows, the preview to its
  right. Upright phone: two columns, scrolled, the preview at the
  bottom as now.
- Reduced motion: stills only, the preview as it is today.
- It joins the `qps-ploc` walk and the controller reachability rules
  (`docs/plans/controller-hints.md`).

### 4.4 The first five

Chosen for the visual mattering most, for being at Basic, and for
building on what exists, one at a time.

**1. Arrows** (the `noteskin` row; later the prompt's).

- Tiles: one a skin offered for the style shown: Infinito first, the
  eight other bundled ones (Alpha-Arrows, CF_ALLOY, CF_CHROME,
  CF_VIBRANT, Dake, DanOni, Delta, Peters-DDR), then imported or song
  folders' ones; today nine on a fresh web profile.
- Each tile: the style's receptors (Single: Left, Down, Up, Right) and
  one tap below each, a quarter of a beat before the receptors, on the
  preview's plate at the background brightness chosen, drawn by the
  field preview with that skin and the player's parameters for it,
  frozen at a fixed demo time; the skin's name under it, its credit
  line small (from the bundled index's `credit`, "shakesoda · CC BY
  3.0") at Advanced; "Imported" for one the player added.
- Tile size: 180×200 virtual px, four columns on a wide screen (three
  rows show nine).
- The focused tile live in the preview panel, with the demo's holds,
  mines and hit effects.
- Cost: one skin load and one render a tile, sequential, through the
  shared skin cache; stills kept for the session by (skin key, style,
  parameters, Reverse).
- New: the chooser, `OptionDef::chooser`, a stills service in
  `ddi_shell` over the field preview for both shells.

**2. Note colors** (Infinito's `Colors`; any skin's Choice parameter
that declares `Preview=Tiles` in its metrics, our extension: Peters-DDR's
Colors next).

- Tiles: Vivid, Rainbow, Flat.
- Each tile: four lanes with a run of notes at different timings
  (a quarter, an eighth, a sixteenth, a triplet), so the color by
  timing reads, no receptors, frozen mid-cycle; the choice's label (the
  skin's word, data).
- Size: three tiles side by side, 220×260 each.
- Focused live in the preview.
- Cost: Infinito readied three times with each value (the parameter is
  part of what a skin is readied with); three stills.

**3. Behind the field** (one chooser for `field_filter` and
`bg_brightness`, opened from either row).

- Two strips: Background brightness (Off, 20, 40, 60, 100%) above,
  Darken behind the arrows (Off, 40, 60, 80%) below; the pair chosen
  marked in each.
- Each tile: the same picture (the song last chosen in song select's
  background if known, else a bundled song's), at that brightness,
  with the filter's band over the lanes and one shared still of the
  field over it: one field still, composed nine times by the 2D pass.
- The large preview shows the pair focused, live.
- Cost: one field still and one picture; the rest is the painter's.

**4. Judgment words** (`ddi.words`).

- Tiles: English, each other language (日本語 today), Same as game
  language.
- Each tile: DDI's words stacked in their tier colors (the DDI palette,
  `rules::colors::DDI`) as the HUD draws them, in its judgment face:
  Absolute, then Awesome and Amazing side by side (early, late), down to
  Too early and Too late and Miss; the language's name above.
- No field: the 2D pass and the HUD's faces; the Japanese bold face is
  fetched on the web where the menus speak another language (as
  `RunRasterizer::words_language` does for play).
- Size: one column a choice, the full height of the rows' area.

**5. Touch controls** (`touch_controls`, with `touch_hand` and
`touch_size` inside it).

- Tiles: Buttons, Pad, Columns (old).
- Each tile: a frame of this device's screen shape, small, with the
  style's receptors where play puts them and the controls as play draws
  them (`touch_controls::place`, the glyph atlas `ddi:touch-glyphs`);
  Double shows two pads.
- Under the tiles: Extra button (left, right; where the style has an
  odd number of lanes) and Size (Smaller, Standard, Larger), changing
  the tiles as they change.
- No field and no renderer: pure placement and the 2D pass.
- Shown from Controls at both levels; on a device that is not touched
  it still opens, framed as a phone held upright.

### 4.5 The rest, a line each

- **Generated background**: five tiles (Off and the four looks) as
  stills at one moment of a bundled song, the focused one live; needs
  `Stills` to hold several sources and a music analysis in Options (the
  volume sample decodes a song's preview already; to measure).
- **Generated colors**: five tiles of the chosen look in each palette;
  as above.
- **Cursor**: three tiles of the cursors' art at the size chosen, the
  Infinito cursor charging; System drawn as a plain outline with "Your
  system's pointer".
- **Appearance and Lane cover**: live miniatures in one renderer (to
  verify), or stills at three moments.
- **Background frame**: four tiles of one movie still fitted each way.
- **Opponents' fields**: two tiles drawing eight and twenty miniatures
  beside a field, as stills.
- **Chat style**: two short loops of the scripted chat's emotes.
- **Field + hand layout**: seven small frames of the field and the
  hand's place, drawn as boxes.
- **Pump arrows**: as Arrows, on Pump single.

## 5. The hidden settings screen

**What it is for**: switches for developing and testing the game,
experiments not yet decided, and the few kept for a few people (today's
hidden settings), and those address parameters that would be saner as
switches the person sets on the screen. **What it is not**: a second
Options. Nothing a player needs is only there; an entry that proves
itself becomes an Options row and leaves.

**Its name**: "Experimental settings" (「実験的な設定」).

**How it is reached**, three ways weighed:

| Way | For | Against |
|---|---|---|
| A typed address and a flag: `#/experimental` on the web, `--screen experimental` on desktop | as `about:config`; never met by accident; easy to give a tester | a person who used it once must remember the address |
| A gesture (the header's wordmark pressed five times) | needs nothing typed, works on a phone | undiscoverable for testers too; met by accident on touch |
| A row at the very end of Advanced Options | findable | not hidden; it invites everyone in |

Recommended: the address and the flag, and once the screen has been
opened on this device or any entry differs from its default, a row at
the end of Advanced Options' Troubleshooting, "Experimental settings…",
so a person finds their way back and sees that something is set. The
row goes when every entry is back at its default and the person resets
it ("Forget this screen").

**An entry** is a key (`rooms.rules_first`), a type (on/off, a choice,
a number in a range, text), a default, a scope (stored, or this session
only), the platforms it exists on, and one line of what it does. The
entries are written in one data file (`crates/ui/src/state/experimental.toml`),
read at build time into a table, each bound in code to a `Settings`
field or to a new `Settings::experiments` map (values at their default
not kept, as skins' parameters are).

**Language**: the entries' keys and lines are English data from that
file; the screen's frame is messages in every language (its title, the
warning, the search field, "Changed", "This session only", "Reset",
"Reset all", "Not on this platform"). That fits the rules as they
stand: what is not ours to translate is data and comes from a file, as
the announcer examples' text does (`announcer/examples.toml`), so the
check's lint (no literal handed to `UiText::data`) holds, and the walk
reads it as data. It is a decision to record (open question 11): these
lines are for developers and testers, and translating them would cost
every entry a message in every language.

**The page**: a page (`PageScreen`) with the warning first ("These
switches are for testing the game. They can break things, and they
change or go away without notice."), a search field filtering keys and
lines (free text needs a keyboard, as elsewhere; a controller scrolls),
the entries grouped (Rooms, Video, Frames, Testing), each with its key,
its control, its line, a mark where it differs from its default and a
Reset; "Reset all" at the end. A session entry says so and shows a
value an address gave ("from the address"). Changes take effect as
Options' do, saved at once where stored. Reset settings in Options
resets stored entries too (they are settings).

**The addresses stay as they are.** Round HY made every address
parameter last the page or the tab and never be stored, because a link
from a stranger could change a player's settings. The screen does not
undo that: an address still never writes a stored setting; a person on
the screen can set a stored switch themselves.

**Day one**:

| Key | Type | Default | Today | Scope | Platforms |
|---|---|---|---|---|---|
| `share.uploader` | on/off | off | `Settings::sharing`; `--sharing`, `#/?share=1` | stored | both |
| `rooms.rules_first` | on/off | off | `multiplayer_rules_first`; `--mp-rules-first`, `mprules` | stored | both |
| `rooms.watch_link` | on/off | off | `multiplayer_watch_link`; `--mp-watch-link`, `mpwatchlink` | stored | both |
| `results.panels` | Overview, Timing, Song | Overview | `results_panels`, no picker | stored | both |
| `touch.look` | glass, mat, glyphs | the default | `touchlook=`, `DDI_TOUCH_LOOK` | stored | both |
| `language.pseudo` | on/off | off | `lang=qps-ploc`, `--lang` | stored (offers the pseudo-languages in Language) | both |
| `rooms.practice_cpus` | number 0–128 | 0 | `mpcpus`, `--mp-dev-cpus` | session | both |
| `rooms.dev_mines` | number (a beat) | none | `mpdevmines`, `--mp-dev-mines` | session | both |
| `chat.script` | text | none | `mpfakechat`, `--mp-dev-chat` | session | both |
| `perf.record` | on/off | off | `perf=1` | session | web |
| `ratings.slow_ms` | number | 0 | `msdslow` | session | web |
| `play.load_ms` | number 0–50 | 0 | `load=`, `--load-ms` | session | both |
| `pacing.mode` | block, late | as now | `--pacing` | stored | desktop |
| `pacing.in_flight` | 1, 2 | as now | `--in-flight` | stored | desktop |
| `video.decoder` | auto, software, the OS's | auto | `DDI_VIDEO_DECODER` | stored | desktop |
| `video.planes` | copy, metal | copy | `DDI_VIDEO_PLANES` (a build with `video-metal`) | stored | macOS |

Not on it: the fake rooms (`mpfakeroom` and the rest), `auto`, `bias`,
`slow`, `gfx` (Graphics is an Advanced row), and the script switches:
they are test drivers, kept in addresses and scripts.

## 6. Plugins' place

- **The Plugins page** ([source-plugins.md](source-plugins.md),
  section 6): a section of Options on desktop, after Data and privacy,
  shown at Advanced only: the sentence of its section 3, the list with
  switches, details with the digest, Remove, "Add plugin…".
- **What a Basic player sees of sources**: Find songs with "This
  computer", which is built in and Basic. A plugin switched on at
  Advanced keeps its source in Find songs at Basic (hiding a source
  someone chose would make their songs vanish from a search); turning
  plugins on or off is Advanced.
- **The path from a site's instructions**: "Open Options. In Interface,
  turn Show advanced options On. Open Plugins and choose Add plugin…". Or
  Options' More: "Show advanced options".
- **Until the levels exist**, the source plugins plan's interim stands
  (the page shows once the plugins folder holds a plugin); the hidden
  screen is not needed for plugins, since Advanced is an ordinary
  choice.

## 7. Stages

Each useful alone and small; the first is what plugins wait for.

| Stage | What | Needs decided | Size |
|---|---|---|---|
| 1 | The level: `Settings::detail` (Basic by default) and its migration mark; the Detail row (under Display's MENUS until stage 3) and Options' More item; `OptionDef::level` and `default`; Basic's filter in `OptionsModel::data` and `prompt_rows`; each section's "Changed in Advanced" row with Reset and Show; changed Advanced rows kept on the prompt; skin parameters' `Level=`; the walk at both levels; tests that every row has a level and that a hidden row keeps its effect | the rule and the level table (2.2), the toggle's name and place, those who play already, the first run | small to medium |
| 2 | The words: attributes in `crates/l10n/build.rs`, `ddi_l10n::set_level`, the check's parity rule, `locales/README.md`; the ten pairs in English and Japanese; the details of 2.4 (radar numbers, results' panels, calibration's page, the offset editor) | the mechanism, the pairs' wording, Basic's results | medium |
| 3 | The hierarchy: the new sections and their ids with aliases, rows moved, `prompt_rows` by id, the messages naming places and the privacy note's page, `AGENTS.md` and the verify skill's paths | the tree | medium |
| 4 | The hidden screen: the entries' file and table, the page, `#/experimental` and `--screen experimental`, the row once used, the day-one entries, Sharing behind its switch in Data and privacy | how it is reached, the day-one list, English-only entries | small to medium |
| 5 | The chooser and the stills service, with Arrows | the five | medium |
| 6… | Note colors, Behind the field, Judgment words, Touch controls, one each; then the rest of 4.5 one option at a time | each one's tiles | small each |

Source plugins' stage 4 (the Plugins page) lands at Advanced any time
after stage 1.

## 8. Open questions for the maintainer

In the order they block work.

1. **The rule, and the level of each row** (blocks stage 1). The rule
   of 2.1 and the table of 1.2; first the judgment calls of 2.2.
   - **Answered 2026-11-26**: "I think we should lean into erring more
     towards marking a simple option as advanced than marking a complex
     option as basic." Applied in section 9; the rows' table for the
     maintainer to mark up is generated by `options_levels`'
     `rows_table`.
   - **Recommended**: as written; Turn and Appearance Advanced,
     Fewer notes, At once and No holds Basic.
2. **The toggle's name and place** (blocks stage 1).
   - **Answered 2026-11-26**: "I want the toggle to read something to
     the tone of 'Enable advanced options: On / Off'." Built as "Show
     advanced options: Off / On" (section 9).
   - **Recommended**: "Detail: Basic / Advanced" (「表示の詳しさ：基本／
     上級」), first in Interface, and Options' More. Others: "Options
     shown", "Advanced options: On/Off".
3. **Those who play already** (blocks stage 1).
   - **Recommended**: Advanced where any Advanced row differs from its
     default, else Basic; once, with a mark. Other: everyone Basic.
4. **The first run** (blocks stage 1).
   - **Recommended**: start Basic with no question; after desktop's
     folders offer is accepted, one line suggesting Advanced. Other: a
     skippable first question.
5. **What Basic shows of Advanced settings in effect** (blocks stage 1).
   - **Recommended**: a counting row a section with Reset and Show;
     changed Advanced rows stay on the prompt. Other: changed rows stay
     shown at Basic, marked.
6. **The wording mechanism** (blocks stage 2).
   - **Recommended**: the `.advanced` attribute (C). Others: two
     messages (A), a `$level` selector (B).
7. **Basic's results and the offset editor** (blocks stage 2).
   - **Recommended**: no σ, By beat or Offsets over the song at Basic;
     the offset editor only where the song has an offset or the
     suggestion passes 15 ms, worded plainly ("This song's notes seemed
     18 ms late. Fix it for this song"). Other: the editor at Advanced
     only.
8. **Rooms, the editor, the studio and calibration** (blocks stage 2).
   - **Recommended**: rooms, the editor and the studio the same at both
     levels (rooms' words only); calibration with a Basic form.
9. **The tree** (blocks stage 3).
   - **Recommended**: section 3.3; with "Fewer notes" added to the
     prompt.
   - **Built on the recommendation, 2026-11-27** (section 11), the
     maintainer's answer still to come.
10. **How the hidden screen is reached** (blocks stage 4).
    - **Recommended**: `#/experimental` and `--screen experimental`, and
      a row in Advanced's Troubleshooting once used.
11. **English-only entries** (blocks stage 4).
    - **Recommended**: the entries' lines as English data from a file,
      the frame translated; recorded in `locales/README.md`.
12. **The day-one entries, and which become stored** (blocks stage 4).
    - **Recommended**: the table of section 5; the three hidden switches
      and the desktop's pacing and video switches stored, the testing
      ones for the session.
13. **The first five choosers** (blocks stage 5).
    - **Recommended**: Arrows, Note colors, Behind the field, Judgment
      words, Touch controls; then Generated background and Cursor.
14. **Plugins at Basic** (blocks source plugins' stage 4 only in its
    wording).
    - **Recommended**: the Plugins page at Advanced; a switched-on
      plugin's source stays in Find songs at Basic; "This computer"
      Basic.
15. **`AGENTS.md`'s convention** (with stage 2).
    - **Recommended**: the wording of 2.8.
16. **Small things found** (any time): the Graphics row and "On
    WebGL2" on desktop; the unused skill radar messages; Infinito's
    parameters as messages (1.7).

## 9. Stage 1 as built (2026-11-26)

The maintainer approved starting stage 1 before going through section
8. It rests on the plan's recommendations for the questions that block
it, each kept in one place so that an answer is applied there; the
answers to questions 1 and 2 came the same day and are applied (below,
"The switch and the lean"):

| Question | What was built | Where to change it |
|---|---|---|
| 1, the rule and each row's level | the levels of section 1.2's tables leaned toward Advanced by the maintainer's answer: Turn, Appearance and Lane cover Advanced, and since the answer Notes, At once and No holds too, with 15 other rows (below) | `crates/ui/src/screens/options_levels.rs`, `LEVELS`: one entry a row or a family of rows (an id ending in `.`); a row not in it is Advanced, and a test holds every row the model makes to an entry |
| 2, the toggle's name and place | by the maintainer's answer, a switch: "Show advanced options: Off / On" (「上級オプションを表示：オフ／オン」, a draft), Off by default, under Display's MENUS heading before Language until stage 3 makes Interface; and Options' More, "Show advanced options" / "Hide advanced options" | `SHOW_ADVANCED_PLACE` (the section and the row it comes before; since stage 3 the tree's table, `options_tree::TREE`, first in Interface); the words are `options-show-advanced*`, `options-more-show-advanced`, `options-more-hide-advanced`, `options-advanced-changed*` |
| 3, those who play already | settings stored before the level start at Advanced where an Advanced row differs from its default, else Basic; decided once at load, marked `level_decided` | `options_levels::advanced_in_effect`, called by `Settings::load` |
| 4, the first run | Basic, nothing asked | `Settings::default().level` |
| 5, Basic's view of Advanced settings in effect | each section at Basic ends with "Advanced changes: N", Reset them (asked once, naming the count and the section) and Show (Show advanced options On, focused on the first of them; lit first, so a press of Start shows rather than resets); a changed Advanced row stays on the start prompt | `options_model::basic_view`; `prompt_options::PlayOptions::rows` |

**What a person sees.** A new player (no stored settings) is at Basic.
A player whose settings were stored before is at Advanced if any of
these differ from the default (and sees exactly what they saw), else
at Basic: an Advanced row of Options (by its value against the default
settings', the rows shown only under another value too: StepMania's
rules under another ruleset, the covers' heights, the chat's rows), a
pump skin chosen, a skin's parameter (none is Basic since the lean), an
announcer's tag or parameter (its Language is Basic). Flipping the
switch, by its row or More's item, rebuilds Options in place: the
focused row where it still shows, else the nearest shown above it, else
below it; where its section is not shown at Basic (Notes), the first row
of the section in its place.

**Rows at each level** (a fresh profile, a controller-reading
platform, nothing connected; the switch counted in Display; `cargo test
-p ddi-ui --lib rows_per_section -- --ignored --nocapture`):

| Section | Basic as first built | Basic since the lean | Advanced |
|---|---|---|---|
| Scroll | 2 | 2 | 4 |
| Notes | 3 | 0 (left out) | 11 (13 with both covers) |
| Rules | 4 | 4 | 11 under DDI (17 under another ruleset) |
| Timing | 1 | 1 | 3, and 2 a known device |
| Sound | 7 | 5 | 7 (more with a pack's tags and parameters) |
| Display | 18 | 10 (11 with the chat) | 30 (35 with the chat and the hand's multiplayer row) |
| Arrows | 3 | 1 | 8 |
| Controls | 10 | 10 | 11 (with each controller's rows at both) |
| Data | 9 (8 on desktop) | 9 (8 on desktop) | 9 (8 on desktop) |
| Songs (desktop) | 2 | 2 | 2 |
| Total | 57 (58 on desktop) | 42 (43 on desktop) | 94 (95 on desktop) |

**Choices made beyond the plan, for the maintainer to check:**

- A known device's offset rows (Timing) have no default to count: their
  values are calibration's results, calibration is Basic, and every
  device gets a profile by playing; counting them would put every
  player who calibrated at Advanced and leave Timing's count lit for
  good. The default offsets for devices without a profile, set only by
  hand, count.
- Sharing's four rows (behind their hidden switch) show at both levels
  until the hidden screen (stage 4) takes them: hiding a section a
  person turned on by a switch would lose it.
- A section with no Basic row and nothing changed is left out at Basic
  (none today); with something changed it shows its count alone. The
  place for desktop's Plugins page is `World::sections`, sections a
  shell adds with their rows' levels; a test proves it with a
  test-only Advanced section.
- A skin's parameter is Basic where its metrics say `Level=Basic` (our
  extension, read by `ddi_noteskin::params`; `skin-parameters.md`):
  Infinito's Note colors and Explosions until the lean, then none, and
  Note colors again since 2026-11-27 (the maintainer: "Bring back the
  Note colors"; a look a newcomer enjoys choosing, as the radar stayed).
  A pump skin's are Advanced whatever they say, as its row is.
- "Changed" is row by row against `OptionDef::default`: the row's value
  under the default settings, or its declared default for a skin's and
  an announcer's parameters. Reset sets each changed Advanced row of the
  section to its default as its row would (so the field's place stays
  where Hand layout put it, as a change of that row leaves it).
- The start prompt's skin row on a pump layout is the pump skins' at
  Basic only where a pump skin is chosen (stored or for this play), else
  the arrows' row, which then draws the pump layouts.
- Settings reset ("Settings" in Data) puts the level back to Basic with
  everything else.

**Tests.** The level's own (`options_levels::tests`): every row has a
level; Basic hides and never removes (the setting still in effect,
stored, in the preview's settings; its count; Show; Reset asked and
done); headings carried to the next row shown; an Advanced-only
section; skins' parameters' levels and defaults; the decision for those
who play already, from stored blobs. The screen's: `options_tests::
show_advanced_flips_the_level_in_place` (the row, More's item, the count and
Show by Start). The prompt's: `prompt_options::tests::
basic_shows_its_rows_and_a_changed_advanced_one`. The `qps-ploc` walk
reads Options at both levels and the new confirmation. Tests that were
about rows rather than the level (the options model's and screen's
tests, the prompt's navigation, the developer sample `sample::options`,
which the theme's tests and the developer routes draw) keep today's
full view by setting Advanced in their helpers, so no drawn pin moved;
Basic has tests and pictures of its own.

**The switch and the lean (2026-11-26).** The maintainer's answers to
questions 1 and 2, applied before stage 1 reached any player (so no
settings needed moving again):

- *The words.* "I want the toggle to read something to the tone of
  'Enable advanced options: On / Off'." Options' On/Off rows are named
  either by a thing (Reverse, Lyrics, Practice) or by what the game does
  when On (Send usage statistics, Poll every millisecond), and no row
  says "Enable". A thing's name would read "Advanced options: Off",
  which says they are off, while they still apply; so the row says what
  On does: **"Show advanced options: Off / On"**, Off by default; its
  sentence "On, every option shows, including those for players who already know rhythm
  games. Off, only
  the options most players need; hidden ones still apply, and each
  section counts those changed."
  More's item says "Show advanced options" or "Hide advanced options";
  a section's count "Advanced changes: N" (Show, Reset them; short enough
  that a phone's row keeps the count beside its two buttons).
  Japanese drafts: 「上級オプションを表示」 (オフ／オン; as
  「タイミング誤差の表示」, short enough for a phone's row), More's
  「上級オプションを表示」／「上級オプションを非表示」, 「上級の変更：N件」.
  The setting is still the level (`Settings::level`, Basic and
  Advanced) and the code's row id
  `show_advanced`.
- *The lean.* Every row was gone through again (the table is printed by
  `options_levels`' `rows_table` test). 18 rows moved from Basic to
  Advanced, none the other way: Notes (`cut`, today's "Quarters"
  wording, and it marks plays assisted), At once, No holds (the genre's
  holds, rolls and jumps); Assist clap (StepMania's name), Mine sound (a
  newcomer knows no mine, and Off takes a warning away quietly); Field
  position, Opponents' fields (a performance knob), Chat style, Chatter
  names, Chat opacity, Dancing hand in multiplayer (finer controls under
  a Basic row), Ghost of your best ("ghost"), Background videos ("costs
  frames", "the decoder"; Automatic already handles a costly video),
  Generated background and Generated colors (looks a newcomer need not
  choose; Background brightness keeps the way to have none), Field
  filter (our jargon); Infinito's Note colors and Explosions (their
  words are a skin's English data: "timing-color families", "SVG
  flashes", so its metrics declare no `Level=Basic` now). Kept Basic
  with a reason each beyond the always-Basic list: Speed, Reverse,
  Timing difficulty, Practice, Score display, Judgment words, Announcer
  and its language and volume, Your own announcer, Twitch chat in rooms
  (the privacy note and Host options send players to it), Dancing hand,
  Lyrics, Background brightness, Start countdown, Arrows, Room songs
  ahead (data use).
- *What it makes awkward.* Notes has no Basic row and is left out at
  Basic unless something in it is changed: a newcomer's assists are
  Timing difficulty's Easy and Practice until stage 2's plain words
  ("Fewer notes") can bring Notes back. Arrows at Basic is one row.
  Calibration's page still tells a player to set an offset "by hand in
  the options" and to turn on "Timing error of each hit", Advanced rows
  since stage 1 (stage 2's calibration page). More players carry a
  hidden setting: the counts, the prompt's rule and the one-time
  placement read the same table, and the stored-settings tests have a
  case for each moved row.

**Not done in stage 1** (as the stages say): no wording changes (stage
2), no section moved (stage 3), nothing outside Options and the start
prompt (song select, the results, calibration: stage 2), no hidden
settings screen (stage 4), no choosers (stage 5). The import dialog's
"Details for pack authors" and a song's offset (the results) are
Advanced settings outside Options and are not counted.

## 10. Stage 2 as built (2026-11-26)

The words, on the plan's recommendations for its open questions 6
(the mechanism), 7 (Basic's results) and 8 (calibration's Basic form,
rooms' words only), with the maintainer's lean still applied.

**The mechanism (way C of 2.4).** A message whose words differ between
the levels gives its Advanced wording, the genre's terms, as its
`.advanced` attribute; its value is the plain wording of the Basic
level. `ddi_l10n::set_advanced` chooses, beside the language
(`Settings::choose_language` sets both; Options sets it again and
rebuilds every screen in place when the level flips, by its row,
More's item, a section's Show or a reset of the settings:
`options_shell::follow_level`), and every function of `ddi_l10n::msg`
returns the wording chosen; no call site knows. Until it is set the
Advanced wording (what every message said before), so tests and tools
that never set a level read the game's words as they were. Options'
rows and the start prompt's are made at the level they show at
(`ddi_l10n::at_level` in `OptionsModel::data_at` and
`PlayOptions::rows`). `crates/l10n/build.rs` takes the attribute (no
other) and the variables of both wordings; `xtask l10n check` holds
every language to giving `.advanced` exactly where English does (Japanese
may make the two equal) and none in `boot.ftl`; the faces' cut reads
both wordings. **Advanced is today's text, byte for byte**: `xtask`'s
ignored `advanced_wordings_keep_the_text_before`
(`DDI_L10N_BEFORE=<a copy of locales/ from before>`) compares every
message of English and Japanese with its text before: an `.advanced`
must be it exactly, any other message unchanged (160 wordings checked).
How to write one: `locales/README.md`, "Two wordings".

**The words.** 79 messages have two wordings (the maintainer's table to
mark up: the round's `words.md`): the plan's ten pairs (pair 2 with
today's text as its Advanced wording rather than a new one, which would
have changed Advanced), the rows brought back, the Basic rows'
sentences that used the genre's terms (Speed, Reverse, Timing
difficulty, Calibration, Background brightness, Arrows, whose sentence
pointed at "its options below", which Basic hides), the modifiers line,
the results' timing words, song select's radar names and captions,
calibration's page and its runs' lines and hints, and a room's Boos
(its row, a change's words, the summary). Two new messages are Basic's
alone: `results-average-timing` ("2.1 ms early") and
`pages-calibration-this-device`.

**Rows brought back to Basic** (`options_levels::LEVELS`), each for its
words alone, each passing the lean once plainly named: Fewer notes
(`cut`), Limit jumps (`at_once`; "Notes together" until 2026-11-27) and No long notes (`no_holds`),
whose sentences say the play is marked ASSIST on the results and its
best kept apart; Clap on each note (`assist_clap`); Compare with your
best (`ghost`); Darken behind the arrows (`field_filter`). Notes shows
at Basic again (its three assists under SIMPLIFY).

| Section | Basic, stage 1 as leaned | Basic, stage 2 | Advanced |
|---|---|---|---|
| Scroll | 2 | 2 | 4 |
| Notes | 0 (left out) | 3 | 11 |
| Rules | 4 | 4 | 11 |
| Timing | 1 | 1 | 3 |
| Sound | 5 | 6 | 7 |
| Display | 10 | 12 | 30 |
| Arrows | 1 | 1 | 8 |
| Controls | 10 | 10 | 11 |
| Data | 9 (8 on desktop) | 9 (8) | 9 (8) |
| Songs (desktop) | 2 | 2 | 2 |
| Total | 42 (43 on desktop) | 48 (49) | 94 (95) |

The one-time placement of those who play already reads the same
table, so a changed row brought back no longer counts as an Advanced
setting changed; everyone placed since stage 1 keeps the level the mark
holds (nobody moves), and settings not opened since then with only such
a row changed are placed at Basic, where the row now shows
(`options_levels::tests::those_who_play_already_keep_what_they_see`).

**Outside Options** (the level the messages are spoken at,
`ddi_l10n::advanced`; Advanced as before):

- *Song select*: a chart's number whole at Basic (`meter_text_fine`; MSD
  whole, the DDI estimate's tenth kept, round AD); the radar's shape
  with plain names and no spoke's number, its caption "Overall" with
  the chart's number as its row shows it.
- *The results*: the Timing and Life panels alone (`panel_sets_shown`),
  "Average timing" in words without σ, the Timing panel's note without
  σ, and the song's timing editor only where the song has an offset or
  this play suggests one 15 ms or more away (`offset_shown`; open
  question 7 as recommended), in plain words.
- *Calibration's page*: plain words; this device's delays in one
  sentence (the sound output and display seen last) in place of the
  defaults and every device's list; where it sent the player to rows
  Basic hides ("by hand in the options", "Timing error of each hit") it
  says how to show them (Show advanced options, Options → Display, then
  Options → Timing); Reset stays, a way out of a bad calibration.
- *The start prompt*: its rows' words and the modifiers line.
- *A room*: "Stray presses" for Boos (pair 10); nothing else of rooms.

**Tests.** `ddi_l10n`'s `the_level_chooses_a_message_s_wording` and
every message formatted at both wordings in every language; the walk in
`qps-ploc` again at Basic (`every_text_shown_at_basic_is_a_message_or_data`);
`play::tests::basic_results_show_less_in_plain_words`;
`ratings_tests::the_radar_at_basic_has_plain_names_and_no_numbers`;
calibration's `the_menu_lists_the_offsets_and_resets_them` at both
levels; the levels' tests for the rows brought back. No drawn pin moved:
the pins draw at the Advanced wording, unset.

**Not done in stage 2**: Turn and Appearance's words and their
modifiers (Advanced rows, shown at Basic only where changed), the
results' notes and the editor (both levels alike, as 2.6 says), the
messages naming places in Options (stage 3), the hidden screen, the
choosers.

## 11. Stage 3 as built (2026-11-27)

The hierarchy, on the plan's recommendation for open question 9
(section 3.3, with Fewer notes on the start prompt); the maintainer
said on 2026-11-27 to start it, and the question itself is still
theirs. Nothing is lost or renamed: every row is there as before, with
the same id, stored key, words, values, level and effect; only its
section, its place in it and the heading over it changed.

**One table.** `crates/ui/src/screens/options_tree.rs`'s `TREE` lists
the sections in their order, each with its id (its English name, which
the saved position, a section's count row and the tests know it by),
its name's message, the old sections whose rows it took (`was`) and its
entries: a heading, a row by id, or a family of rows by their ids'
prefixes (a device's offset and its Forget, a controller's rows). The
model still makes its rows in parts by topic (the sections before);
`options_tree::arrange` takes each row out and puts it where the table
says, with the table's heading (a controller keeps its name as its
own). A row the table does not place would go to the end of the
section that took its part, never lost; a test holds every row the
model makes to exactly one entry. Changing where a row shows is one
line of that table.

**The tree as built.** The same twelve sections at both levels (a path
in instructions stays true at either), then any a shell adds
(`World::sections`, desktop's Plugins page, section 6):

| # | Section (id) | Took from | Basic (web, desktop) | Advanced (web, desktop) |
|---|---|---|---|---|
| 1 | Play | Scroll; Notes' modifiers and assists | 5 | 11 |
| 2 | Arrows | Arrows | 1 | 8 |
| 3 | Screen | Display's play rows and the hand; Rules' Judgment words and Score display | 5 | 11 |
| 4 | Backgrounds | Display's backgrounds | 2 | 8 |
| 5 | Rules | Rules | 2 | 9 under DDI |
| 6 | Timing | Timing | 1 | 3, and 2 a known device |
| 7 | Sound | Sound | 6 | 7 |
| 8 | Controls | Controls | 10 | 11, with each controller's rows |
| 9 | Song list | Display's countdown and ratings; Notes' versions; desktop's Songs | 1, 3 | 6, 8 |
| 10 | Multiplayer | Display's opponents, chat and hand in multiplayer; Data's Room songs ahead | 2 | 4 (8 with the chat on) |
| 11 | Interface | Display's menus and diagnostics, the switch | 5 | 8 |
| 12 | Data and privacy (Data) | Data; Sharing's four at its end | 8, 7 | 8, 7 |
| | Total | | 48, 49 | 94, 95 |

A fresh profile, as stage 2 counted it: the same totals as before (48
and 94 on the web, 49 and 95 on desktop); `cargo test -p ddi-ui --lib
tree_map -- --ignored --nocapture` prints the tree at both levels for
six worlds.

**Where each choice rests.**

| Choice | Rests on |
|---|---|
| The twelve sections, their names and order | open question 9: section 3.3 |
| Show advanced options first in Interface, with no heading | open question 2's place: "first in Interface" (its words the maintainer's answer) |
| Play's order: Speed, Reverse, then the Advanced modifiers; EASIER over Fewer notes, Limit jumps, No long notes, No stretch | 3.3's Play |
| Fewer notes on the start prompt (after Lane cover, before the arrows) | open question 9: "with Fewer notes added to the prompt"; 3.2 |
| Screen: Field position, Judgment words, Score display, Compare with your best, Lyrics, Timing error, Receptor snap; DANCING HAND over the hand's four | 3.3's Screen; 3.4 (`ddi.words`, `ddi.score` from Rules) |
| Backgrounds: Background brightness, Darken behind the arrows, Background videos, Random movies, Generated background, Over song backgrounds, Generated colors, Background frame | 3.3's Backgrounds; a finer row kept right under the row it refines (as the model already puts the covers' heights and the chat's rows) |
| Rules: Timing difficulty, Practice, Boos, then Rules for each chart and Ruleset, StepMania's rows, Kept plays, DANCING☆ONIGIRI RULES | 3.3's Rules (Kept plays kept before Dancing☆Onigiri's heading, so it does not read as theirs) |
| Timing, Sound and Controls as before | 3.3 ("as today"); Timing's sentence for this device (new, 3.3) not built: it would add a row |
| Song list: Start countdown, Chart ratings, SIMPLIFY (Without onigiri, Without center), CONVERT, SONG FOLDERS (desktop) | 3.3's Song list; 3.4 |
| Multiplayer: Opponents' fields, Dancing hand in multiplayer, Room songs ahead, Twitch chat in rooms and its four | 3.3's Multiplayer; 3.4 |
| Interface: the switch, Language, Reduced motion, Cursor, Cursor size; TROUBLESHOOTING over Graphics, Debug overlay, Autoplay demo | 3.3's Interface; Experimental settings waits for stage 4 |
| Data and privacy: Play history, Twitch access, Privacy note, Send usage statistics, Send error reports, Settings, Imported songs (Song folders), Everything; SHARING over Sharing's four at its end | 3.3's Data and privacy; Sharing's rows still at both levels behind their switch, as stage 1 left them, until stage 4 |
| Section ids: the English names; Data kept as Data (named "Data and privacy"); Scroll, Notes, Display, Songs and Sharing gone, kept as `was` | 3.4 and 3.5, point 5 |
| The prompt's rows taken by id | 3.5, point 4 |
| The levels unchanged | section 9's lean (open question 1's answer): the tree's B and A marks in 3.3 predate it |

**Under an existing player's hands.** A row is where 3.4 moves it, and
the game helps a player find it:

- *A place Options remembered* (the web's history entry, `ddiOptions`:
  a section and a row, written as the player moves): the row is looked
  for where it is now, so a place saved in Display on Language opens
  Interface on Language; a row the level hides opens the first row of
  the section that took its old section's rows (`options_tree::
  section_for`: Scroll and Notes to Play, Display to Screen, Songs to
  Song list, Sharing to Data and privacy); the scroll is kept only in
  the same section, else the row is scrolled into view
  (`OptionsScreen::with_position`). Desktop keeps Options on its stack
  for the run and saves no place.
- *Keys, pads and taps*: Tab and the shoulders go through the twelve in
  their order and wrap; a tap's targets are `Section0` to `Section11`
  (Timing `Section5`, Controls `Section7`); the web's rectangles list
  sixteen, room for a shell's own.
- *Every pointer in the game's messages*: both wordings, both
  languages, "Options → Interface" for Show advanced options,
  "Options → Multiplayer → Twitch chat in rooms", "Options → Data and
  privacy" for Twitch access and the upload token. The privacy note's
  sentences (`pages-privacy-*` and `site/privacy.html`) wait for the
  maintainer: the proposed change is a patch outside the repository.
- *A section's count at Basic* counts the same settings as before, in
  the sections that now hold them (the record's sum, below).

**Tests.** `options_rows_tests.rs` holds the rows to
`options_rows_before.tsv`, written by the code before the regrouping:
six worlds (both shells, under DDI and StepMania's rules, Options as
full as the model makes them, and a new player's), at Advanced each
row's id, level, label, values, value, default and the settings stored
after a change to its next value (a digest), at Basic each row's id and
the sum of the sections' counts; the sets are equal, only the section
differs. `options_tree::tests`: every row placed once, the twelve in
order, the saved places' successors. `options_tests::
a_place_saved_before_the_regrouping_finds_its_row`; the levels', the
prompt's and the screens' tests on the new places; the desktop's
`options_levels_offscreen` draws every section at both levels in
English and Japanese.

**Pins.** `room_tests::the_rooms_screens_draw_as_pinned`'s "host":
Host options' line under Show my Twitch chat names Options →
Multiplayer. No other drawn pin moved (the theme's Options pictures
are checked by layout, not by hash).

**Not done in stage 3**: Timing's sentence for this device (a new row);
the Experimental settings row (stage 4); the privacy note's sentences
(the maintainer's); the Plugins page (source plugins' stage 4). A
thirteenth section (desktop's Plugins) will not fit the 1280×720 rail
as it is (twelve reach its foot): it will need the rail to scroll or
its items closer.

## 12. After stage 3: the maintainer's answers (2026-11-27)

- Note colors is Basic again (Infinito's `Level=Basic`); Generated
  background, Generated colors, Explosions and Field position stay
  Advanced.
- Background videos is Basic again, with a plain description beside
  today's (its `.advanced`): "in practice it's a very useful performance
  toggle". Basic shows 50 rows on the web, 51 on desktop.
- **A simplified performance selector, to design later** (the
  maintainer, 2026-11-27): a Performance section, a setting allowed to
  appear in more than one section, and a simplified option ("low quality
  graphics") that prioritizes performance and changes other options
  under the hood, behind which Background videos might then live. "Let's
  design it first, with a bit more care. I have some ideas to how we can
  abstract this nicely, but it's best to go through it as part of the
  design. Which we don't have to do now": after the two fronts under way
  (these levels and the source plugins) are done. Until then Background
  videos shows at Basic so this front keeps going. Notes for that
  design: an override held at play time (stored settings untouched, the
  rows it holds off saying so) against a preset that rewrites them; what
  each effect costs, measured (the web's frame recorder), before
  choosing what it holds off; the rail, which twelve sections fill at
  1280×720.
- `at_once` at Basic reads "Limit jumps: Off / Only allow pairs / No
  jumps" (the maintainer: "'Limit jumps' is simple and descriptive, even
  if 'jump' is a genre term, but it's a potentially important setting
  for users who aren't advanced and the visual for that setting (which
  is something I want to touch on the settings revamp) should
  communicate the behavior much better than the terms themselves"). Its
  description says what a jump is. The chooser's picture for it is the
  revamp's (stage 5).
