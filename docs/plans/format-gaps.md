# Smaller chart and import format gaps (2026-10-21)

`docs/PLAN.md` section 5, "Known gaps … Charts and import", listed a
handful of things simfiles carry that the game ignored: `.lrc` lyrics,
jackets and CD titles, an `.ssc` chart's own music, a separate preview
file, background-change files named with `,` or `=`, Korean and Chinese
legacy encodings, and zips with other compression methods. This plan
closes all but the last, following StepMania 5.1 (`5_1-new`, MIT-style;
read for behavior, the LRC and background-change splitting ported).

## Research (StepMania 5.1)

**Lyrics.** `Song::TidyUpData` takes `#LYRICSPATH` (`SMLoader` and
`SSCLoader` both read it), else the first `.lrc` file in the song folder.
`LyricsLoader::LoadFromLRCFile` reads it line by line: a byte-order mark
is dropped from each line, `//` lines are comments, a line must match
`^ *\[([^]]+)\] *(.*)$`; `[COLOUR]`/`[COLOR]` (any case) with
`0x%2x%2x%2x` sets the color of what follows (each part over 256); a
name matching `^([0-9]+:){0,2}[0-9]+(.[0-9]*)?$` (its `.` unescaped) is
a time, `HHMMSSToSeconds` (hours and minutes as `atoi`, seconds as
`strtof`), the text with `|` as line breaks; everything else, `[offset]`
included ("todo: handle [offset:xxxx] tag?"), is ignored; lyrics are
sorted by time. `LyricDisplay::Update` starts a lyric at its time (the
music's seconds) less `InLength`, shows it until the next one's start or
the song's last second less `InLength + OutLength` (both 0 in
`_fallback`'s metrics), and plays `Changed` on its two texts; the
default color is `LyricDisplayDefaultColor`, green (`_fallback` and
`default`). `_fallback`'s `Actor:LyricCommand` sets the text, colors the
back copy at half the color and the front at 0.9, fades in over 0.2 s to
0.75, crops the front from the right and the back from the left over
three quarters of the duration (the bright copy wipes in from the left),
waits a quarter, fades out over 0.2 s; a new lyric stops the tweens. The
default theme places it at `SCREEN_CENTER_Y+260` of 720 lines, `−260`
under Reverse (`LyricDisplaySet*ReverseCommand`), squeezing a line wider
than the screen (`zoomx`), with a 5-line shadow and a black stroke.

**Jacket and CD title.** `Song::TidyUpData`: by tag, else by name (a
jacket's starts with `jk_` or contains `jacket` or `albumart`; a CD
title's contains `cdtitle`), else in the loop that sizes the remaining
images: after the background and banner rules, an image of at most
100×48 is the CD title, a square one the jacket; the loop stops once
banner, background and CD title are known. `ScreenSelectMusic` shows the
CD title (`g_sCDTitlePath`); the default theme shows no jacket.

**Per-chart music.** `.ssc` `#MUSIC` inside `#NOTEDATA`
(`SetStepsMusic`); `Steps::GetMusicPath` is that file, else the song's;
`AutoKeysounds::LoadAutoplaySoundsInto` loads the master player's steps'
music.

**Preview.** `.ssc` `#PREVIEW` (`SetPreview`). `Song::GetPreviewMusicPath`
is that file when given; `GetPreviewStartSeconds` is then 0; with
`#SAMPLELENGTH` 0 or less, `TidyUpData` takes the preview file's whole
length. `#PREVIEWVID` (`SetPreviewVid`) is only reachable from Lua
(`Song:GetPreviewVidPath`); StepMania's own themes do not show it.

**Background-change names.** `SMLoader::ParseBGChangesString` removes
line breaks, then splits on `,` (changes) and `=` (fields), except that
in the 2nd and 8th fields a name from `GetSongDirFiles` (every file and
folder under the song folder, relative to it, recursively), compared
case-insensitively and followed by the end, `=` or `,`, is taken whole.
It takes the first such name of the directory listing.

**Encodings.** `RageFile` text is used as UTF-8; `Song` loaders call
`utf8_remove_bom`; titles in other code pages show as mojibake on a
system whose code page differs. Nothing in StepMania detects them.

## Decided here (the maintainer may change any)

1. `.lrc` lyrics use the existing **Lyrics** option (on by default),
   whose text now says "songs with a lyrics file". A chart with
   Dancing☆Onigiri lyrics of its own shows those, never both.
2. Lyrics are drawn as StepMania's default theme draws them, through the
   play's lyrics pass (`crates/render/src/lyrics.rs`): centered on the
   screen at 620/720 of its height (100/720 under Reverse), about 1/30
   of the screen high, StepMania's default green where the file sets no
   color. Ours: a line wider than the screen shrinks (to the 10 px
   floor) and then wraps instead of being squeezed; no black stroke (the
   shadow stays); it moves off the HUD's gauge and score as
   Dancing☆Onigiri's lines do.
3. StepMania's `[offset:]` is not applied (StepMania does not), nor are
   repeated time tags (`[00:01][00:05]text` shows `[00:05]text` at 1 s, as
   in StepMania).
4. Jackets and CD titles are found as StepMania finds them, the size
   rules included (so a song without either may gain one from its
   images). The song panel shows a jacket as a square at the right of
   the banner's place, the banner centre-cropped beside it; a CD title
   as a small mark (64×48, whole, its shape kept) in the banner's lower
   left corner. The wheel's rows show banners only. The web copies
   both with the song (like the banner: the list shows them before any
   folder is read).
5. A chart's own music plays in that chart's play on both shells; a
   generated version of a chart plays its original's. A `#MUSIC` naming
   a missing file plays the song's music, with an import note.
6. A `#PREVIEW` file plays from its start, for `#SAMPLELENGTH` when it is
   above 0, else the whole file, at most 30 s (the preview's existing
   cap). The beat pulse still follows the song's timing.
7. `#PREVIEWVID` is read (`Song::preview_video`) but not shown: song
   select has no movie surface, and StepMania's themes show none either.
8. Background-change splitting is StepMania's, but where two names fit
   at a field the longest wins (StepMania's first-listed depends on the
   file system's order). StepMania also adds an empty rate field after a
   name that ends the value (freezing a movie there); not copied: the
   older StepMania versions such files were written for play the movie.
9. The song's files with `,` or `=` in their names are what re-splits
   the changes: at import from the folder's listing, at play from the
   names the entry kept (`ManifestEntry::change_names`).
10. Korean and Chinese text is detected by a heuristic (below) after
    UTF-8 and Shift-JIS; a short title may still be misread.
11. Zips with other compression methods (Deflate64, bzip2, LZMA, zstd),
    encryption or several disks stay refused: song packs are stored or
    deflated in practice (every one met so far). Revisit if a pack
    turns up.

## Legacy encodings

`ddi_library::legacy` decides, for text that is not UTF-8, which code
page reads it; each shell supplies a strict decoder (the browser's
`TextDecoder` with `fatal`, desktop's `encoding_rs`), so no table ships
in the wasm. The order:

1. Shift-JIS read as Japanese (`pack::reads_as_japanese`) with full-width
   kana is taken at once: Korean and Chinese bytes (pairs from 0xA1 up)
   decode in Shift-JIS as half-width katakana and kanji, never as kana.
2. EUC-KR (Windows-949), GBK and Big5 are tried where at least 80% of
   the two-byte characters fall in the rows their text uses (KS X 1001
   and Windows-949's Hangul, GB 2312, Big5's symbols and frequent
   characters). A Korean reading must be Hangul with at most half as
   many hanja, and symbols; a Chinese one ideographs, kana and symbols,
   with no lone ideograph between ASCII letters (accented Latin read as
   CJK). Each is scored by the share of its letters among the most
   common (some 300 Hangul syllables, 600 Chinese characters, written for
   this check); the best at 0.35 or more wins.
3. Shift-JIS read as Japanese without kana (kanji or half-width katakana
   only) stands unless a reading scores 0.5 or more.
4. Windows-1252, byte for byte.

Zip entry names go through the same choice for the whole archive.
Checked on hand-written texts in each code page (Korean, simplified and
traditional Chinese, Japanese with and without kana) and on Western
Windows-1252 titles that must stay as they are; the 7 non-UTF-8 simfiles
among the local packs and OutFox's songs are all Windows-1252 and stay
so (`DDI_ENCODING_CORPUS=<dir> cargo test -p ddi-library --lib
local_corpus -- --ignored --nocapture` lists what each reads as).

## Steps

1. Background-change names (`ddi_chart::formats::sm::split_bg_changes`,
   `Song::resolve_change_names`; the raw value kept as a `bgchanges` or
   `fgchanges` effect before its changes; `scan::song_dir_names`): done.
2. Legacy encodings (`ddi_library::legacy`, both shells' decoders): done.
3. `.lrc` lyrics (`ddi_chart::formats::lrc`, `lrc:lyric` effects;
   `ddi_engine::lyrics` `LyricLook::StepMania`; `ddi_render::lyrics`'s
   StepMania look): done.
4. Jackets, CD titles, lyrics files, preview files and charts' music
   found by the import's core (`pack::resolve_assets`,
   `pack::classify_images`, `scan::Reading::finish`), kept in the entry
   (`ManifestEntry::{jacket, cd_title, lyrics, preview}`,
   `ChartInfo::music`, `ManifestEntry::music_for`), stored on the web as
   `jacket.*`, `cdtitle.*`, `lyrics.lrc`, `preview.*`, `music-<n>.*`,
   read in place on desktop (library cache version 7): done.
5. Song select shows them (`Select.Jacket.Gap`, `Select.CdTitle.*` in
   `infinito.toml`; the sprites' `contain`, drawn whole by the painter):
   done. Plays load their chart's music and the lyrics (web
   `songs::load_song(id, chart)`, desktop `load_with(id, chart, …)`);
   previews play `#PREVIEW` (`ddi_session::preview::file_length`): done.

Songs imported on the web before this need importing again to gain any
of it; desktop reads its library again (cache version 7).

## Checks

- Unit tests: the splitter against StepMania's cases (`sm.rs`), the LRC
  reader (`lrc.rs`), lyric timing and wipe (`ddi_engine::lyrics`), the
  StepMania look's placement and colors (`ddi_render::lyrics`), the
  contained rectangle (`painter.rs`), each code page and Western text
  (`legacy.rs`), the import core's finding and naming (`scan.rs`,
  `pack.rs`), the theme's parts and the song panel with and without a
  jacket (`crates/theme/src/tests.rs`).
- Desktop: a folder scanned in place (EUC-KR title, jacket, CD title,
  lyrics, preview, chart music, a background named with a comma) and a
  play's load of each chart's music with the lyrics
  (`crates/desktop/src/library/tests.rs`).
- Headless Chrome (WebGPU): a generated pack (Opus music, a 9 s chart
  music, a 5 s preview, a jacket, a CD title, an `.lrc` with a color) as
  a zip: the import stores `jacket.png`, `cdtitle.png`, `lyrics.lrc`,
  `preview.ogg`, `music-1.ogg`; song select plays the preview from 0 for
  its 5 s and shows the jacket and CD title; Easy plays its 9 s music,
  Medium the 20 s song music; the lyrics show and change at their times;
  both autoplays all top tier.

## Left

- `#PREVIEWVID` not shown (decision 7).
- Zips with other compression methods (decision 11).
- The wheel's rows show no jackets; the start prompt and results show
  the banner only.
- No black stroke on `.lrc` lyrics (our text runs have none).
