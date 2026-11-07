# The UI's languages

What the menus say, in each language the game speaks
(`docs/plans/localization.md`). Each folder is a language, named by its
BCP 47 tag and listed in `languages.toml`; each file an area of the UI
(`start.ftl`, `options.ftl`), written in [Fluent](https://projectfluent.org/fluent/guide/).
English (`en/`) is the source: every message is written there first, with
a comment saying where it shows, how much room it has and what its
variables hold. The game reads each English message as a function
(`select-songs-match` is `ddi_l10n::msg::select::songs_match`), so a
message the code uses must exist in English.

## Adding or changing a message

1. Write it in `en/<area>.ftl`, its id starting with the area
   (`start-quit`), with a comment for the translator. Declare each
   variable in the comment, typed:

   ```ftl
   # The search's result line.
   # Variables:
   #   $count (Number) - how many songs match
   select-songs-match = { $count ->
       [one] One song matches.
      *[other] { $count } songs match.
   }
   ```

2. Write it in every other language's `<area>.ftl` in the same change,
   even as a draft. It is unverified until reviewed (below), and
   `just l10n-check` fails a language that lacks it, or that writes a
   character its faces lack (a kanji no message used before): cut them
   again with `cargo run -p xtask -- l10n fonts` (uv and curl; the
   upstream fonts are fetched once into `target/fonts/`) and commit the
   cuts in `assets/fonts/` with the message.
3. Use it in the code: `ddi_l10n::msg::<area>::<name>(…)`. What is not
   ours to translate (a song's title, a path, what the player typed) is
   `ddi_l10n::UiText::data(…)`, never a message.

Write whole sentences: never build one in code from pieces, and never put
one translated word inside another message (languages decline, gender and
attach suffixes to words). Put data where nothing has to agree with it
(after a colon, on its own line, in quotes the message writes). Capitals
are written in the message, never made in code. Numbers in a message
take the language's marks; a year or another number that must not be
grouped (2,026) is passed as a String.

## Writing a screen's text

Every text the screens show is a message or data (the screens moved over
in `docs/plans/localization.md`'s step 3). A new screen, or new text on
one, follows the same rules; `crates/ui/src/screens/l10n_tests.rs` walks
the screens in `qps-ploc` and fails on text written in code (a new
screen or dialog joins its walk). The areas are the files here: `common`
(shared words, styles, lanes), `start`, `select`, `prompt`, `results`,
`mods` (the mods line), `options`, `pages` (calibration, credits, the
folders offer), `notices`, `editor`, `studio`, and what the screens show
from elsewhere: `import` (an import's report), `danoni` (desktop's URL
importer), `errors` (failures: the system's kinds, loading, video,
shares, history), `devices` (devices' labels, controls' names); and
beyond the screens: `boot` (the web page's loader, before the game runs:
formatted when the page is built, so no variables; few and short, as
the page fetches them before the game; `site/boot.js` names each by its
id without `boot-`) and `shell` (the shells' text outside the screens: the
web's plain pages, desktop's app menu and file dialogs, and the Linux
desktop entry's lines, which the packages take for each shipped
language). The check's lint fails a literal handed to a page's DOM, a
window's title or a file dialog's kind of file, as it does one handed to
`UiText::data`.

What code outside the screens reports (the library, the importers, the
session, the shells) is a value carrying its data, worded by the UI with
these messages, never an English sentence handed on; a value keeps its
English in `Display` for logs. What the system says (an I/O error, a
browser's exception) goes through `ddi_ui::screens::errors::error_text`:
its kind in the player's language, its words after it.

- **Ids** say what the text is for, not its words: `select-remove-pack`,
  `select-remove-pack-confirm`, `options-speed-description`. One message a
  text, even where two read alike in English ("Remove" a pack, "Remove"
  a key): another language may need two words. The words every screen
  uses alone on a button or after a key hint are `common-*` (Back,
  Cancel, Close, More, On, Off, Save, Reset, Open, Copy, Calibrate).
- **The comment** says where it shows, what it does or means, and how
  much room it has ("a button 120 px wide: about 12 letters"; a row's
  label; a sentence under a heading, wrapped), and declares each
  variable with what it holds. A translator sees nothing else.
- **A run is one message.** Never `format!` two messages into one text
  (`«Off» · «On»`: the walk fails it as JOINED), and never a message
  around a translated word. What would be built in code is one message
  with variables: `{ $count } songs match`, `Remove “{ $pack }”?`. Data
  (a title, a pack, a path, a key's name) goes in as a String variable,
  where nothing has to agree with it; counts as Numbers, through a plural
  selector (`{ $count -> [one] … *[other] … }`; Japanese has `other`
  alone).
- **The shells' wording** is one message with a `$platform` selector
  (`Platform::l10n`: `web` or `desktop`), never two code paths: `{
  $platform -> [web] … *[desktop] … }`.
- **Capitals** are written in the message: a heading shown in capitals
  ("OPTIONS") is written so, its Japanese as Japanese is written. Code
  never changes case.
- **Not ours to translate** is `UiText::data(…)`: names read from files
  (songs, packs, skins, announcers), what the player typed, paths, key
  and button legends as their caps print them, and the game's readouts
  (judgments and grades as the Judge names them, scores, percentages,
  combos, offsets in the results and the HUD). Units in the menus are
  messages, a language may write them otherwise (`{ $ms } ms`, Russian
  мс).
- **Identifiers stay in code**: actor names (`"Button.Back"`), setting
  values, rows' ids, what tests and `__DDI_DEBUG` read.
- **Japanese**: read `ja/GLOSSARY.md` first. Buttons and labels short;
  a Japanese character is as wide as two Latin letters in the theme's
  face, so a label of the same letters' count needs twice the room:
  where a button was sized to its English, check the Japanese fits
  (`cargo run -- --lang ja`, `?lang=ja`).
- **New characters**: a kanji no message used before fails `just
  l10n-check` until the faces are cut again (below).

## Checking and reviewing

- `just l10n-check` (`cargo run -p xtask -- l10n check`): every
  language has every English message and no other, each parses, uses
  only English's variables and its own terms, and gives its plural
  selectors the categories its language has; then how many messages are
  reviewed by a person, by an agent, or unverified.
- `cargo run -p xtask -- l10n report ja --unverified [--area start]`:
  each message with its English, translation and comment, for a review.
- `cargo run -p xtask -- l10n review ja --by agent <ids…>` (or
  `--area start`): records that they were reviewed as they stand, in
  `ja/reviewed.toml`, against hashes of the English and the
  translation. A message changed on either side is unverified again by
  itself. Agents record `agent`; `human` is the maintainer's.
- The pseudo-languages show what does not pass through here:
  `--lang qps-ploc` on desktop or `?lang=qps-ploc` on the web writes
  every message accented, longer and in « », data in ‹ ›; `qps-wide` in
  full-width letters. `crates/ui/src/screens/l10n_tests.rs` walks the
  screens so and fails on any text written in code.

## Adding a language

List it in `languages.toml` as `preview` (offered in Options' Language
row and by `--lang`/`?lang=`, never chosen for the system), copy `en/`'s files into its folder and
translate them, write its `GLOSSARY.md`, and give it a face where its
script needs one beyond Iosevka Term's Latin (Korean: Noto Sans CJK KR):
a `[[face]]` in `languages.toml` (its upstream files pinned by SHA-256,
a base for the script), named in the language's `faces`, cut with
`xtask l10n fonts`, credited in `assets/fonts/PROVENANCE.toml` and
`REUSE.toml`, and copied by `site/index.html` (`l10n check` says what is
missing). Both shells then draw it in that face. It becomes `shipped` (offered to everyone, and chosen for
systems set to it) at the maintainer's word.
