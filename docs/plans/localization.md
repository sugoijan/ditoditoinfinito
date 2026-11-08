# Localization: the UI in more than one language (2026-11-07)

Status: steps 1 and 2 done (2026-11-07): the groundwork, the start menu
and Options' Language row in English and Japanese (a preview); Japanese
drawn in Noto Sans CJK JP's cuts on both shells, lines broken by
Unicode's rules. Step 3 done (2026-11-07): every screen's text a
message or data, in English and a Japanese draft. Step 4 done
(2026-11-07): what the library, the importers, the session and the
shells report is values the screens word. Step 5 done (2026-11-07): the
web's loader and plain pages, desktop's app menu, window and file
dialogs, the packages' entries and the release's gate (1,607 messages a
language, none reviewed yet).

The maintainer asked for the UI in Japanese as well as English, with the
approach settled first so that more languages are easy to add later, no
text reaches the screen without passing through translation, and a check
says how complete each language is before a release. The languages
expected later: British English, the main European languages (French,
German, Spanish, Italian, Portuguese, Dutch, Polish), Russian, Korean,
Turkish and Tagalog. None of them is written right to left; that case is
not designed for.

## Decisions

1. **Fluent message files** (`.ftl`, Project Fluent), American English the
   source (as the repository's conventions have it). Fluent's selectors
   carry plurals, word order and the wording that differs between the
   shells (`{ $platform -> … }`) inside one message, and translation
   tools (Weblate, Pontoon) read it.
2. **Typed messages.** A build step reads the English files and makes a
   Rust function for each message, its variables its arguments
   (`msg::select::songs_match(count)`): a misspelled message or a missing
   argument does not compile.
3. **Every text shown is a message or marked as data.** The UI's text
   builders take a `UiText`, made from a message or from
   `UiText::data(…)` for what is not ours to translate (a song's title, a
   pack's name, a path, what the player typed). A pseudo-language test
   over every screen catches what slips through.
4. **Judgments and grades vary with the Judge, not the language**: the
   ruleset's names (`JudgeNames`) stay the ruleset's, data to the
   translation. So do the game's readouts (the HUD's and the results'
   score, percentage, combo, offsets), which read the same in every
   language, as the judgments do.
5. **Every message in every maintained language, always.** Whoever adds or
   changes English text (agents included) writes every maintained
   language in the same change. A translation is unverified until a
   review is recorded for exactly that English and that translation
   (below), so drafts always show and are always known to be drafts.
6. **Japanese drawn with Noto Sans CJK JP**, the face the DanOni SDK
   already ships beside Iosevka Term (`tools/danoni-dump/rust/fonts/`,
   2.004, OFL-1.1). Sarasa Term J, compared for the SDK's title art
   (`docs/research/danoni-font-comparison.md`), has the same Source Han
   glyphs; its advantage there, Iosevka's Latin inside one face, the UI
   has anyway, since its Latin is drawn by Iosevka Term. Confirmed by the
   maintainer, with decisions 4 and 7, on 2026-11-07.
7. **A release needs each shipped language complete**; review states are
   reported with the release, not required.

## Message files

```
locales/
  languages.toml          the languages: tag, own name, status, fallback, fonts
  en/start.ftl …          the source, one file per area
  ja/start.ftl …          the same ids
  ja/reviewed.toml        the reviews recorded (below)
  ja/GLOSSARY.md          fixed terms and tone, for translators and reviewers
```

Areas follow the screens: `common` (buttons, units, the header, styles'
and lanes' names), `start`, `select` (with its dialogs), `prompt`,
`results`, `mods` (the mods line three screens show), `options`, `pages`
(calibration's menu, the credits, the folders offer), `editor`, `studio`,
`notices`, `errors` (what the import, the library and the shells report),
`boot` (the page before the game loads).

Ids are kebab case within their area (`select-songs-match`), the
generated function named after them (`msg::select::songs_match`). Each
English message has a comment for the translator: where it shows, how
much room it has, what each variable holds.

The English source is written so that other languages can follow it:

- **Whole sentences.** A sentence is never assembled from pieces in code,
  and a translated word is never put inside another message: Russian and
  Polish decline it, German genders it, Turkish and Korean attach suffixes
  and particles that depend on its sounds.
- **Data where nothing has to agree with it**: after a colon, on its own
  line, or quoted by the message (the quotes are the message's: “ ”, « »,
  „ “, 「」). Counts go through plural selectors.
- **No case changes in code.** Capitals are written in the message
  ("SHARED SONGS"): Turkish uppercases i as İ, Japanese and Korean have no
  case.
- **The player addressed without gender.**
- **The shells' differences as a selector** on `$platform`, not two code
  paths (what `wording_tests.rs` checks: desktop never speaks of the
  browser).

## Showing only translated text

The text actor keeps a plain string (`ddi_actor::Text`). Messages and
data are `UiText` (a message's function returns one; `UiText::data`
makes the other), which the screens hand on as text (`&*msg`, `.into()`).
The plan had everything that builds text in the screens take a `UiText`
and nothing else once the last screen moved; that is not done (step 3):
the walk below, with nothing written in code left in what it reaches, is
the guard, and the type change (every component's `Spec`, the theme's
text builders, the models' strings) waits for the maintainer's word.

- **A lint** (part of `xtask l10n check`) fails on a string literal given
  to `UiText::data`, which is almost always English slipping past;
  symbols (×, …, →, ★) are allowed, tests left alone.
- **Pseudo-languages**, for tests and for looking: `qps-ploc` writes each
  message accented (in Latin-1 and Extended-A, which the theme's subset
  draws), 40% longer and in « » (`«Šíñĝļépļáýéŕ~~~~~»`), data in ‹ ›;
  `qps-wide` in full-width letters in 「」 (CJK widths). `--lang
  qps-ploc` on desktop and `?lang=qps-ploc` on the web show them.
- **The coverage test** (`crates/ui/src/screens/l10n_tests.rs`) walks the
  screens and dialogs in the headless harness on sample data, in both
  layouts (`DESIGN`, `COMPACT`) and both shells' wording, and Options'
  rows and confirmations from its model, in `qps-ploc`: a text drawn with
  a letter in it that is neither « » nor ‹ › is written in code, and
  fails. Through steps 1 to 3 the texts not moved yet were listed in a
  file the test kept to them (508 at the start), the list only shrinking;
  it went with the last of them. The walk covers the start menu, song
  select and its dialogs, the start prompt's steps, the results, Options
  (every row, value and state), the number forms, the calibration and
  credits pages, the folders offer, the chart editor with its menus and
  dialogs, the announcer studio and its scenes' frame, on both shells'
  wording and in both layouts; `moved_screens_show_no_text_written_in_code`
  holds each screen to nothing in code, `every_options_row_is_a_message_or_data`
  Options' states the walk does not reach. A run joining two messages
  fails as JOINED (a sentence built in code), unless only " · " with data
  stands between them (a list of tags: the mods line, a chart's line); a
  message holding another as a variable ("«KeyF moved from «Down» to
  «Left».»") is one. A message wrapped over several runs keeps its marks
  on each. A message cut short where its actor is not meant to cut is
  left to the eyes (screenshots in each language as an area moved).

## Languages and review state

`locales/languages.toml` lists each language: its BCP 47 tag, its name
written in itself, its status (`preview`: in the picker for everyone,
or chosen with `--lang`/`?lang=`, but never for the system, since
2026-11-08; `shipped`: the system's choice too),
its fallback (`en-GB` → `en`), how its numbers are written (decimal
mark, group separator, CLDR's minimum grouping digits) and the faces its
script needs (below, "Text and fonts").

Review state is recorded, never the lack of it. `<tag>/reviewed.toml`
lists the messages reviewed, each with a hash of the English it was
reviewed against, a hash of the translation, who reviewed it (`agent` or
`human`) and when. A message not listed, or listed with other hashes, is
**unverified**: a new message, a changed translation, changed English.
Nobody marks a draft; whatever is written is unverified until reviewed,
and a review lapses by itself when either side changes.

- `xtask l10n report <tag> [--unverified]`: each message with its English,
  its translation, its comment and its state, for a review pass.
- `xtask l10n review <tag> --by agent|human <ids…>` records reviews.
  Agents record `agent`; `human` only at the maintainer's word.
- `just l10n-screens <tag>`: every screen and dialog drawn offscreen in
  that language (the desktop's offscreen shell, as the README's
  screenshots are made), for a reviewer's eyes.

Regional variants (`en-GB` first; later `pt-PT`/`pt-BR`,
`es-ES`/`es-419`) hold only the messages that differ and fall back to
their parent for the rest. Their check flags parent messages with words
the region spells otherwise (color, center, gray, favorite) and no
override.

## The check

`xtask l10n check`, for each language:

- messages missing, or ids English does not have;
- Fluent syntax;
- variables English does not pass; plural categories against CLDR's for
  the language (Japanese and Korean: other; Russian and Polish: one, few,
  many, other; Filipino: one, other);
- characters its fonts lack: each character its messages and its name
  write is in the theme's face or in every weight of its faces (☆ and ★
  are the system's on purpose); each face's file is there, credited in
  `assets/fonts/PROVENANCE.toml` and copied by `site/index.html`;
- how many messages are reviewed by a person, reviewed by an agent,
  unverified.

`just check` runs it: a missing message in any maintained language fails
(agents write every language); review states are only reported. Its
lint fails text written in code where the coverage walk cannot see it:
a literal given to `UiText::data`, to a page's DOM (`set_text_content`,
an `aria-label` or `title`), to a window's title or to a file dialog's
kind of file (the developer pages excepted). The release workflow
(`release.yml`) runs it as `xtask l10n notes` before packaging, so a
language missing a message stops the release, and puts what it prints,
each language's status and review states in a table, at the head of the
draft release's notes.

## Choosing the language

- `Settings::language`: a tag, or none for the system's (the web's
  `navigator.languages`, desktop's through the `sys-locale` crate),
  matched against the shipped languages (the same tag, else the language
  without its region, else any of that language; aliases such as `tl`
  for `fil`), else English.
- Options → Display → Menus, Language lists the system's ("As the system
  (English)") then each language in itself ("English", "日本語"), the
  previews too; pseudo-languages with the debug overlay on, and the one
  chosen whatever it is.
- A change rebuilds every screen in place, without a restart or a
  transition (`Cx::rebuild_screens`, `Ui::rebuild_all`); the web sets
  `<html lang>` with the next frame (step 2, through the painter).
- `--lang TAG` (desktop) and `?lang=TAG` (the web, on any address)
  speak a language for the run whatever the settings say, until one is
  chosen in Options (`ddi_l10n::force`); `?lang=` alone gives the choice
  back.
- `site/boot.js`, which runs before the game, chooses as the game will
  (the address's `?lang=`, the settings' `language` in `localStorage`,
  else `navigator.languages` among the shipped languages, the same
  negotiation), sets `<html lang>` at once and says its few lines in it:
  `locales/<tag>/boot.ftl`, formatted by `ddi_l10n` when the page is
  built (`xtask minify-boot`; each key the script names must be a
  message, each message named; no variables). The page's first bytes
  (`check-boot`'s budget) carry English's lines, each language's first
  ones (the loader's label) and the name of a file with the rest of its
  lines (`boot/<tag>-<hash>.json`, named by its contents), which the
  script fetches at once for a language other than English: in long
  before a box is likely to show, which says English's until it is (or
  for good, should the fetch fail). Japanese's lines all in the page
  took 540 bytes of the budget compressed, leaving it 425; a language's
  first lines and file name take about 60 (the page is 11.7 KB of its
  12.5 with English and Japanese). The pseudo-languages are in dev
  builds only.

## Text and fonts

As built in step 2:

- **Faces per script, cut to what the messages use** plus a base per
  script (Japanese: CJK punctuation, the kana, full-width forms; 56 KB a
  weight with the start menu's messages). A language names its faces in
  `languages.toml` (`faces`); each `[[face]]` there is its family, its
  base, the OpenType features kept (`ccmp,kern,mark`: horizontal text)
  and a file a weight with the upstream file it is cut from, pinned by
  its SHA-256. `cargo run -p xtask -- l10n fonts` makes the cuts into
  `assets/fonts/` (the upstream files fetched once into `target/fonts/`;
  fonttools 4.68.0's pyftsubset through uv, its output the same bytes
  each run): the base and every character the languages using the face
  write (messages, terms, the language's own name) beyond the theme's
  face. The cuts are committed, recorded with the command in
  `assets/fonts/PROVENANCE.toml` as the Latin subsets are, so neither the
  build nor CI needs Python or the network; `l10n check` fails a message
  with a character the cuts lack, naming the command. Latin and Cyrillic
  are Iosevka Term's (the Latin subset grows where a language needs it:
  Romanian's ș ț are in Latin Extended-B; Turkish's and Polish's letters
  are in Extended-A, already in). Japanese: Noto Sans CJK JP 2.004,
  Regular for the theme's 400, Medium for its 600 (its SemiBold; CJK
  looks heavier than Latin at one weight) and Bold for 700; no italic
  (CJK is set upright). Korean later: Noto Sans CJK KR.
- **The face list follows the UI language**: the theme's text stack is
  `ddi_l10n::font_stack` of `fonts.text`, the language's faces after
  Iosevka Term ("Iosevka Term, Noto Sans CJK JP, system-ui, sans-serif"
  in Japanese), rebuilt with the screens on a change. Desktop resolves a
  stack as the browser does (`ddi_render::cosmic`): each grapheme is set
  in the first family named in the stack whose face has it, the rest in
  the first family with cosmic-text's fallback, generic names ending the
  search (before, it took the first family alone). Unified Han
  characters the cuts lack take the UI language's forms from the
  system's faces: the painter tells the rasterizer the language
  (`RunRasterizer::set_language`, on a change it reads as it lays out
  text), desktop matching the system's faces for
  that locale (Hiragino Sans in Japanese rather than cosmic-text's
  default, PingFang SC), the web setting the canvas's and the page's
  `lang` (`<html lang>` from step 5, here since the browser's choice of
  face follows it).
- **A run is drawn whole by one face for its script**: the language's
  faces draw a run only where their cuts have every wide character it
  writes (`ddi_l10n::stack_for`, applied where the painter measures and
  draws runs, `text_atlas::set_in`; the cuts' characters known at build
  time from the same inputs `xtask l10n fonts` cuts from). A message is
  always in its language's face (`l10n check`); a song's title with a
  kanji no message uses is drawn whole in the system's, never half in
  each, with no need to tell data from messages there.
- **Web**: a `FontFace` a weight, `fonts/<file>` (copied by
  `site/index.html`, never preloaded), registered and loaded once the
  language is chosen (`web::text::load_language_faces`): at boot the
  first screen waits for them as for the theme's; later the runs drawn
  meanwhile are measured and drawn again once they load
  (`Ui::forget_metrics`). An English UI fetches none. **Desktop**: the
  cuts in the binary (`ddi-l10n`'s feature `embed-faces`), registered
  under their family at the theme's weight.
- **Widths**: `look::mono_width` already counts wide characters (CJK,
  Hangul) as a full em, which these faces keep.
- **Line breaking**: Unicode's rules (UAX #14, the `unicode-linebreak`
  crate), `ddi_actor::text::wrap`, in place of
  `select_dialogs::wrap_lines`' spaces and the lyrics' own wrap
  (`crates/render/src/lyrics.rs`). Japanese breaks between characters but
  never before 。、」ー, small kana and their kind, nor after 「（; the
  rest at UAX #14's opportunities (after a hyphen too). No-break spaces
  never break. A piece wider than a line (a link, a German compound) is
  cut between graphemes in the menus, kept whole in lyrics; no
  hyphenation. Korean at spaces alone (keep-all) comes with Korean.
- **Cutting with …** per grapheme (`Text::ellipsis`).
- **The header's small caps**: letter spacing (`TextStyle::tracking`)
  only for runs with no letter of a script without capitals
  (`TextStyle::for_text`, applied where runs are measured and drawn);
  Japanese is drawn as written.

## Formatting

- Plurals: Fluent's selectors, CLDR's rules (`intl_pluralrules`).
- Numbers in sentences: Fluent's `NUMBER` through a small formatter per
  language, its separators and percent in `languages.toml` (decimal
  comma for most of Europe, spaces grouping in French and Russian,
  Turkish's percent sign before the number: %98,7), not ICU's data.
- Dates, times and durations: messages taking numbers
  (`{ $year }年{ $month }月{ $day }日`), months' names as messages.
- The game's readouts: as the Judge gives them (decision 4).

## What stays as it is

- Songs, charts, packs and everything else read from files.
- The ruleset's judgment and grade names, and the game's readouts.
- Keycap legends, as keyboards print them. Showing the player's keyboard
  layout (AZERTY's Z Q S D for W A S D) is a matter of its own, not of
  language.
- Licence texts and notices in the credits (their headings translate).
- The developer screens (`--dev`, `#/ui-dev`, `#/video-check`),
  terminal output and logs.
- Announcer packs, which have their own languages. The studio's examples
  follow the UI language (English and Japanese ones exist).
- README, docs and the site's SEO pages (localized pages need their own
  URLs and `hreflang`; later, if wanted).

## Beyond the screens

- **Errors and reports become values** (step 4). What the library's
  scan, the import report, the share client, the DanOni importer and the
  shells reported as English strings became enums with their data,
  worded where shown: an import's findings (`ddi_library::report::Finding`,
  its running step `ImportStep`; worded by `screens::import_report`), the
  URL importer's steps and problems (`ddi_importer_protocol::Stage`, sent
  by the helper as a value; `crates/desktop/src/danoni/report.rs`), why a
  song did not load, what a video, a share, a replay or a history import
  ran into (`screens::failures`: `LoadFailure`,
  `ddi_platform::video::VideoError`, `ddi_library::share::ShareError`,
  `state::history::Refusal`, `ReplayProblem`, `LineProblem`), why a
  song's scripts stopped, devices' labels from their facts
  (`ddi_platform::devices::DeviceFacts`, kept with each saved profile)
  and controls' names (`controller_hints::control_name`: "button 3" a
  kind and a number; a controller's printed legends stay data). Each
  keeps its old English through `Display` for logs, the terminal and
  `__DDI_DEBUG`. What the system itself says (an I/O error, a
  DOMException, an HTTP status) is classified by its names and codes
  (`screens::errors::error_text`, `io_error_text`): its kind in the
  player's language, its own words after it as data. Left as data, as
  the system's or another program's words: parsers' and decoders'
  messages, the helper's `stderr`, the zip reader's details.
- **The web's DOM** (step 5): the loader's lines (`boot`, above): the
  canvas's label, the box for what the browser lacks (its heading now
  only for that, not over a crash or a failed download), the crash's
  and the failed start's lines; the plain pages (`shell`): no GPU, an
  address with no page, the note over song select while files are
  dragged, its list's failure. The import status's live region says
  the report's worded summary (step 4). The developer pages
  (`#/video-check`, `#/ui-test`, `#/skin-preview`) stay English.
- **Desktop** (step 5): the app menu macOS shows, winit's, its items
  named as macOS names them in the UI's language and named again when
  it changes (`crates/desktop/src/app_menu.rs`, found by their actions);
  the file dialogs' kinds of file and the window's title from messages;
  `CFBundleLocalizations` in the macOS app's `Info.plist`, the shipped
  languages alone (`xtask l10n package`): the system's panels follow the
  app's localization, and a preview language declared there would have
  them, and macOS's per-app language setting, speak a language the game
  does not choose from the system; `CFBundleDevelopmentRegion` was
  there already (`en`). Where the player chose another language in
  Options, the panels keep the system's. The Linux desktop entry's
  `GenericName`, `Comment` and `Keywords` from `shell.ftl` in each
  shipped language (`Key[ja]`; its `Name` is the game's, never
  translated).

## Working on it (for AGENTS.md, with step 1)

- A change to UI text edits the English message and every maintained
  language in the same commit. Translations an agent writes are drafts:
  never record a `human` review; `agent` only in a review pass asked for.
- Read the language's `GLOSSARY.md` first. Japanese: prose in です・ます,
  buttons as short verbs or nouns; the judgments and the game's readouts
  are never translated.
- `just check` fails a missing message; `xtask l10n report ja
  --unverified` lists what waits for review.

## Steps

1. **Groundwork** (done 2026-11-07): the `ddi-l10n` crate (message
   files, generated functions, the Fluent runtime, `languages.toml`,
   negotiation, the pseudo-languages, numbers), `UiText`, the language
   setting and its row, `xtask l10n` (check with the lint, report,
   review), the coverage test with its list, AGENTS.md's rules, `just
   l10n-check` in `just check` and CI. Moved as the first: the start
   menu, the key hints it publishes and the Language row, in English and
   a Japanese draft (`preview`). Fluent's runtime measured in a
   size-tuned wasm (`opt-level = "z"`, fat LTO, `wasm-opt -Oz`) against
   the same code with `format!`: 122 KB more, 58 KB gzipped, under 2% of
   the game's release wasm (8.3 MB); kept. Compiling the messages to
   tables (as `infinito.toml` and `examples.toml` are) stays the way out,
   the files and functions unchanged.
2. **Japanese text** (done 2026-11-07): Noto Sans CJK JP's cuts on both
   shells (`xtask l10n fonts`, the check of characters the faces lack),
   the face list by language, desktop's font stacks resolved per
   grapheme, Han's forms by the UI language, UAX #14 line breaking,
   ellipses per grapheme, letter spacing for scripts with capitals.
3. **The screens move over** (done 2026-11-07), one area a commit, each
   with its Japanese draft and passing the pseudo test. First the
   groundwork: the shared words (`common-*`), `$platform` selectors
   (`Platform::l10n`), JOINED runs, a run drawn whole by one face for its
   script (`ddi_l10n::stack_for`), styles' and lanes' names
   (`data::style_name`, `data::lane_name`), ordinal plural selectors in
   the check (`NUMBER($n, type: "ordinal")`, "16th"). Then the areas,
   five of them moved by agents in parallel, each in a worktree of its
   own, merged here: the results and the mods line (`results`, `mods`),
   the start prompt and the play's hints (`prompt`), song select and its
   dialogs (`select`), Options with its dialogs, the binding flow and the
   number forms (`options`), the announcer studio (`studio`), the
   calibration and credits pages and the folders offer (`pages`), song
   select's notices from both shells (`notices`), the chart editor
   (`editor`). Left for later, as values or data meanwhile: what the
   library, the importers and the shells report (step 4: errors passed
   into messages as String variables); the device profiles' labels the
   shells write ("48 kHz · ~20 ms output latency") and the platform's
   names for a controller's controls ("button 3", "South"); a skin's own
   parameters' labels (`metrics.ini`, data: Infinito's rows stay
   English); what a text field shows (typed text, not marked as data in
   the walk). A page builds its document once, and the shells' notices
   are made once: after a change of language they speak the old one
   until made again (a page reopened, a notice raised again).
4. **Errors and reports** as values (done 2026-11-07; "Beyond the
   screens"): the failure classifier first, then four parts moved by
   agents in parallel, merged here: the import's report (`import`), the
   URL importer (`danoni`), play, video, replay, history import and
   share failures (`errors`), devices and controls (`devices`).
5. **The edges** (done 2026-11-07; "Beyond the screens"): the areas
   `boot` (the web page's loader, before the game runs) and `shell`
   (what the shells show outside the screens, and the system's entries
   for the game), the loader's choice of language and its table, the
   web's plain pages, desktop's app menu, window title and file
   dialogs, `Info.plist`'s languages and the Linux entry's
   (`xtask l10n package`), the check's lint on pages, windows and
   dialogs, the release workflow's gate and notes (`xtask l10n notes`).
   After a change of language mid-session, a page's document and the
   shells' notices still speak the old one until made again (step 3).
6. **Review**: agents' passes over the Japanese drafts, then the
   maintainer's; Japanese `shipped` at the maintainer's word.

Each language after that is an entry in `languages.toml`, its messages
(drafted, then reviewed), its glossary, and a face where its script needs
one.

## Notes for the languages to come

- **British English**: a regional variant of `en` (above).
- **Russian, Polish**: four plural categories; cases, so whole sentences;
  Russian needs Iosevka's Cyrillic. Both run long (the pseudo-language's
  40% is for them and German).
- **German**: long compounds need room in buttons and rows.
- **French**: no-break spaces before : ; ! ? and inside « » (U+202F,
  U+00A0), in the messages; the faces keep them and line breaking never
  breaks there.
- **Turkish**: dotted and dotless i (no case changes in code); suffixes
  follow the word they attach to, so data stays in label positions; the
  percent sign comes first.
- **Korean**: Hangul (Noto Sans CJK KR, a large script, cut to what the
  messages use with the system's faces for data); breaks at spaces;
  particles depend on the word before, so data stays in label positions.
- **Tagalog**: CLDR's `fil` (Filipino), `tl` as its alias; Latin script;
  its plural rule (one for numbers not ending in 4, 6 or 9) through
  Fluent's plurals; game terms are commonly kept in English.
- **Spanish, Portuguese**: regional variants if wanted (`es-419`,
  `pt-BR`).
