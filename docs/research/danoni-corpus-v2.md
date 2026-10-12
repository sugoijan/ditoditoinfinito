# DanOni corpus expansion v2: catalogue and 2026 festival survey

Research date: 2026-10-10. This expands the
[single-song source corpus](danoni-indexes.md) with more **live**
publications, both legacy SWF works and HTML5 works spanning the CW engine
generations encountered in this survey. The output is a local source/capture corpus under
`~/Music/DanOni/single-song-corpus-v2/`. It is not a new export generation and
nothing here is in the export registry yet. No third-party bytes are in Git.

## Inputs and selection

| Input | Use |
| --- | --- |
| [Work catalogue](https://cw7.sakura.ne.jp/lst/) | 263 registered sites (11,081 works, 17,004 charts). The site list sorted by `maxCretime` plus 43 per-author lists (`/lst/dsNNNN/`) were parsed into 5,785 work rows: URL, title, artist, per-chart key mode, maker, level, BPM, length, date and event. |
| [V-Keys Festival '26 works](https://apoi108.sakura.ne.jp/event/nfes26/works.html) | 78 works from August 2026 with maker and key-mode badges. These postdate the catalogue's last update on 2026-05-24. |

Selection policy: one to three works per host, 5-key first, spread across
publication years (2005–2026) and engine builds. Prefer the supported modes
5/7/7i/9A/9B. Do not probe ranking or BBS endpoints. Blank catalogue key cells
repeat the previous chart's mode. Catalogue dates are publication or update
hints, not served-file revisions. Several 2006–2010 catalogue entries now serve
HTML5 ports on the same URL (icekirby 2006, avectristesse 2006, omission0 2007,
ngtkdo 2009, rorokoshi 2010). These ports are a separate kind of work from
their original SWFs.

Liveness was checked with a paced (1.5 s per origin), robots-aware scratch
fetcher using the `DdiDanoniDump` agent, followed by the dump tool's own
`CaptureNetwork` for every retained byte.

| Outcome (64 checked pages) | Count |
| --- | --- |
| Legacy SWF works acquired | 18 (two are collections: 12 + 5 songs) |
| HTML5 works captured | 27 (3 complete; 24 unreviewed-engine probes) |
| HTTP 404 / host closed | 9 (daradaras ×2, nano.yu-nagi, kt-rpg Flash, fia, nikemix, gon9604.github.io, kwskmop 2022 page, land.to shutdown) |
| Host unreachable, Wayback snapshot exists | 7 (DNS gone: `*.sitemix.jp` ×2, autumnleaves.snapmix.jp, atpages.jp, kitsunebeats.starfree.jp; connection refused: nandenande.com ×2) |
| Live, not selected / not a work | 3 (icekirby 2026 and ngtkdo 2024 duplicates of captured hosts; a login-based festival result tracker) |

These seven were first recorded as robots refusals. That was wrong: the
scratch fetcher treated any failed `robots.txt` request, including a DNS
error, as disallow-all. Exact-URL Wayback lookups (CDX, then the availability
API after CDX 503s and timeouts) find a 200 page snapshot for each, from 2015
to 2026-04. Only Kitsune Beats' assets were checked. Its
[彼岸帰航 page](https://web.archive.org/web/20230601110749/http://kitsunebeats.starfree.jp/dan_oni/higankikou/index.html)
(Cocon, 2020-07-26; [catalogue](https://cw7.sakura.ne.jp/dtl/ds0321/0001))
keeps its inline dos: `difData=5,StepUp,1$5,Hard,5`, `musicUrl=higankikou.mp3`,
794 and 1,100 arrows per the catalogue. These are new charts for the same
どぶウサギ song, not レク's Normal (562 taps, 9 holds). A CDX prefix query over
`/dan_oni/` returns only HTML pages, so the engine, music and CSS were never
archived. No browser reference is possible. A static import would need the
native Higan audio, aligned and checked separately, because the catalogue
gives 5:08 for this version. Asset checks for the other six (raw `id_`
replay, exact-URL CDX, 5 s spacing; results in
`discovery/wayback/wayback-assets.json`):

| Work | Outcome |
| --- | --- |
| C-BOMB (ぶるーぴえろ, sitemix) | Recovered: `wayback/sitemix-c-bomb`. Page 2023-06-01, movie 2021-01-09 (3.2 MB, embedded stream, no external chart file) |
| 蒼夜の詩 (レク et al., snapmix, 2010) | Recovered: `wayback/snapmix-bluenight`. Page 2023-05-19, `k.swf` 2022-12-21; レクのソース generation 4 |
| 亡き王女の為のセプテット (sitemix) | CDX lists `sept.swf` (5.4 MB, 2022-12-25), but raw replay returned 404; retry later |
| Phantom Horizon ×2 | Not recoverable. The archived page is a Vue shell, and the real CW pages (`/old/danoni/<name>/<name>.html`, `/danoni-sources/`) have no captures |
| ダン☆ぷち 6th #2 (atpages) | Not recoverable: page archived (2015), `rising.swf` never captured |

Mixed capture dates are recorded per resource, as for RE:RAVE.
`ccsyouta.sakura.ne.jp` festival works were skipped
because the earlier survey recorded discouraging terms and a 403.

## Legacy SWF works

All 18 movies are AVM1 (`FileAttributes` AS3 flag clear, no DoABC), with
60 FPS headers. Sidecar names below come from static movie strings and were
fetched only when named explicitly. A restricted native run must confirm them;
none has a title capture or projection yet.

| Directory | Year / modes (catalogue) | Static family and sidecars |
| --- | --- | --- |
| `tyruster-air`, `tyruster-cold` | 2005; 5 / 5×4 | SWF6, embedded streams (57/64 stream heads in sprites); `dos.txt` with bare `left_data=` arrays, no header fields; `ranking*.cgi` strings not probed |
| `icekirby4-aom-vol1` | 2006 collection; 5/7/11 | `musicselect_vol_1.swf` menu → `numXX.swf` + `numXX.txt`; 12 songs acquired, `num13`/`num28` return 404 |
| `carrotcastle-118-rotteler`, `carrotcastle-584-honey-cookie` | 2006 and 2018; 5 / 5×2 | Shared `aom_swf.swf?name=numNNN` engine (`aom_swf_80.swf` in 2018); page-relative `music.swf` is audio-only. The chart sidecar name is built at runtime (guessed `numNNN.txt` was 404), so native observation must supply it |
| `varenntainn-yamiyo` | 2007; 5 | Collaboration with レク (Higan's chart author); Shift_JIS `dos.txt`, 62 KB, numbered charts up to 11 |
| `shomotsu-kami2`, `blossom130-wgo`, `tseirproodni-nightmare`, `clock-luvin` | 2007–2008; 5×2 / 5,5,11 / 5 / 5 | SWF6 one-stream engines with external `&key=value&` dos (`musicTitle`, `difStep`, `difName`, `speedlock`, `frz*`, `boost*`) |
| `matatabi-kantan3` | 2012 collection; 5 | `EasyDanOni3.txt` lists five child SWFs (makers 音ノコギリ, 蒼宮野あいすかぁびぃ４号, izkdic, 鍵盤狐, MFV2); data is embedded in each child |
| `touhouonigiri-tsukimade`, `prologue-babylon`, `otonokogiri-overthef`, `hashi-wintersky`, `rakurakusuisui-boss3`, `mfv2-tohosoha3rd`, `danoninatu-ophelia` | 2009–2013; 5 | SWF8 `ni_spdbst`-style engines (`_root.CREATOR_NAME`, `_root.diffi`); no external file strings, so data is probably embedded; one stream head each |

`pw25.g2.xrea.com` now redirects to `prologues.works`. Several 2007–2011 hosts
serve a shinobi.jp "inactive site" script next to the original page. Each work
directory follows the v1 layout: `manifest.json`
(`ddi-danoni-single-source/experimental-1`, including the catalogue row and
static scan) and `inputs/blobs/<sha256>` exact bodies with their HTTP records.

## HTML5 works

Captured with `node cli.mjs capture` into `single-song-corpus-v2/html5/<id>/`
(`ddi-danoni-dump/1`). The terms note records the page review.

**Complete; offline verification passes; the importer differential test
reports 19 captured charts with zero differences. Fourteen charts use the
played modes 5/7/7i/9A/9B; the five 11/23-key charts remain evidence only:**

| Directory | Engine | Charts |
| --- | --- | --- |
| `apoi-brilliant-blue` | v50.2.0 (reviewed hash) | 5-key ×1 (2020 beginners' festival) |
| `vh-428-zombie` | v47.3.2 (reviewed) | 5-key ×1 (festival 2026) |
| `vh-433-needs` | v47.3.2 (reviewed) | 17: 5×3, 7×3, 7i×3, 9A×3, plus 11 and 23×4 as unsupported data (festival 2026) |

```sh
DDI_DANONI_CORPUS="$HOME/Music/DanOni/single-song-corpus-v2/html5" \
  cargo test -p ddi-chart --test danoni_corpus -- --nocapture
```

**Unreviewed-engine probes** (24). The runner stopped at the registry gate,
by design. Pages and engine scripts are retained; music and references are
not. The root `manifest.json` records each decoded-source SHA-256. A
syntax-only run of the adapter's `findCapturePoints` found all four capture
points (input, reference, prepared, runtime) in every v29–v51 build. This is
triage, not the compatibility review `--trust-engine` requires.

| Engine | Works (directories) |
| --- | --- |
| 51.2.2 split (dataLoader/mainWindow match reviewed upstream hashes; dosConverter differs) | `keian-waza-samurai` (7×3, 2026) |
| 51.1.1 split, three different host builds | `avectristesse-05` (2006 port, 5×4), `omission0-6-witch` (2007 port), `omission0-295-zundamochi` (5/9A/9A, 2026; also a `cache-vary-unsupported` diagnostic), `hashilog-futoukou` (2026, base64 music) |
| 50.3.1 | `icekirby-104-ggh` (2006 port, 5×2) |
| 50.0.0 (custom router, hls.js) | `mfv2-onigiri-hardcore-respect` (2026; CDN robots unavailable, like the earlier jsDelivr case) |
| 49.5.0 / 49.4.2 / 49.3.1 / 49.3.0 / 49.0.0 | `suzume-wind2`; `tarwil-13-lag-train`, `tarwil-23-ads` (GitHub Pages); `skb-sextet` (5/9A/9B), `tseirproodni-180-thrust-up`; `ngtkdo-001-dance`, `averysteps-035-kmd` (5/7/23); `fuminori-gleam-of-aigis`, `kwskmop-hanakumoma` (7, 2026) |
| 48.5.1 | `terz-lapis`, `terz-luminaria` (2026) |
| 41.0.0 / 37.3.1 / 33.2.0 / 29.3.4 | `rorokoshi-12-harusoko`; `hsvsjq-shoujo-dream` (GitHub Pages, 7, 2026); `aconite-glaciate` (base64 music); `mattari-007-raid-folkmetal` |
| 22.5.2 / 18.9.3 | `ricetask-ds009` (`danoni_mainR.js`, `?scoreId=4`); `manysun-26-shinkiro`. Both use `function` declarations, so the current lexical adapter finds no capture points and needs an older-generation variant. |

The same decoded source sometimes serves several hosts: 49.3.0 at ngtkdo and
averysteps (`38976768…`), 49.3.1 at superkuppabros and tseirproodni
(`1baf97df…`). One review therefore covers several works. Other copies of the
same version label differ by hash and need their own review.

## Higan's engine: レクのソース

The Higan page credits "レクのソース" (レク's engine source) at version 2.15. It
also notes that judgement was tightened relative to "1.63 and earlier", and
that Higan remakes its DOXF02 entry, the third chart of this song. The
rendered title says 2.16. The version is drawn as glyphs, not stored as an
ActionScript string. Generations were therefore assigned statically: AVM1
`ConstantPool`/`Push` identifier sets, plus SHA-256 of each DoAction block
larger than 64 bytes, compared with the Higan SWF (`6ce53b12…`, 392
identifiers, 19 blocks). `discovery/scripts/swffp.py` and `abc.py` hold the
method.

レク's catalogue (`ds0065`) has 115 works. 72 pages are live, all SWF, and 66
new movies plus 32 named sidecars are under `autumnleaves/`, each with its
generation in `manifest.json`. Higan, Destined Marionette, 闇夜の月 and
月まで届け were already acquired.

| Generation (catalogue dates) | Static signature | Works |
| --- | --- | --- |
| gen0 (2006-12..2007-08) | SWF6, numbered `dosN.txt`, no `timeline` (0.36–0.40 of Higan) | 6: sai, HARASSIN' BOSS, DOVSF ×2, Her Selves, U.N.オーエン (#9). 闇夜の月 also belongs here |
| gen1 (2007-07..2008-04) | `timeline`/`ftimeline`/`mkArrow`/`frame_num`, `dos.txt`, no `judgeY`; 8/19 Higan blocks | 25, including all 13 DOXF03 entries and 終焉と始まり (#2, Higan's DOXF02 sibling) |
| **Higan build** (2008-05..2008-08) | Identical identifier set; 18/19 blocks shared (END of AIR 17/19); `iro.txt` colour support | 9: 灼冬赫夜 (5×2), トルコ行進曲, 蠢蠢秋月, 弾・幕・決・壊 (all 7-key, 7Fes'08), 最期に君がいた, 60's wind, ENEMY WITHIN, 恋色マスタースパーク＜SlashSpeed＞, END of AIR (夏'08) |
| gen3 (2008-09..2009-08) | Adds `boost_data`, `judgeY`, `option.txt`; 16/19 blocks shared with Destined Marionette | 11, including the DOXF04 entries; Destined Marionette is this build |
| gen4 (2009-11..2010-12) | `frameNum`, data embedded (no `dos.txt` string); 15/20 blocks shared within the generation | 15, plus the Wayback 蒼夜の詩 |

The four generations from gen1 onward expose identifiers associated with
the structures the existing `scheduled-counter` adapter reads (Destined
Marionette is gen3, FAKE800 uses `frameNum`). This static match suggests
adapter candidates; it does not establish runtime compatibility or timing. Higan's own pinned adapter matches the nine Higan-build works
most closely. Five of those nine return 404 for `iro.txt`. Only Higan's page
announces the colour feature, so the file is probably optional; a native run
must confirm. gen0 needs a separate family adapter.

The [gen1 native admission](danoni-gen1-admission.md) subsequently proves all
25 gen1 works and 31 selectors. These older builds fit the constant-travel
family, not `scheduled-counter`: raw arrays are deleted before `frame_num`
initializes, so the adapter additionally needs an intact zero-counter witness.
The shared identifiers above were only a probe hint.

Other authors built on the same source. Static nearest generation:
`tseirproodni-nightmare` is closest to the Higan build (9/19 blocks, 0.92 of
its identifiers). `clock-luvin`, `blossom130-wgo`, `shomotsu-kami2`,
`rakurakusuisui-boss3` and the 12 `icekirby4-aom-vol1` children are nearest
gen3. The `ni_spdbst` movies, `mfv2-tohosoha3rd` and the Wayback C-BOMB are
`frameNum` relatives. Lower block overlap is expected where other authors
customised the source.

**Same songs on later builds or ports.** レク re-released their own songs:
弾・幕・決・壊 (gen1 5-key, 2007 → Higan-build 7-key, 2008), 恋色マスタースパーク
(DOXF03 gen1 → ＜SlashSpeed＞ Higan build), and U.N.オーエン (gen0 → gen1).
彼岸帰航 itself has the 2008 Higan remake, MFV2's 2007 DOXF03 chart
(`mfv2.sakura.ne.jp/do/__Flash/mfv2_003/danoni/DOXF03/higan/DO.higan.html`,
live, not acquired) and Cocon's 2020 HTML5 charts (Wayback, chart text only).

MFV2's chart list (`ALL_List.php`) has **HTML5 ports of nine レク charts**:
`_rekuAL/?id=Dress`, `main/?id=SSSS`, `_rekuAL/?id=saigetsu`,
`main/?id=akatuki`, `main/?id=HeGa`, `main/?id=septet`, `main/?id=Show`,
`_rekuAL/?id=sakura`, `main/?id=ESS`. saber's charts are at `_rekuAL/?id=HELL`
and `?id=end`. Saigetsu (`reku-103`) and Earthquake Super Shock (`reku-106`)
pair live gen4 SWFs with ports, giving SWF-versus-CW cross-checks like Straight
Soul. 恋詠桜 (7-key, 2011) is a dead link in レク's catalogue and survives
only as this port. None is captured yet: they run MFV2's unreviewed v50.0.0
build, the same build as the `mfv2-onigiri-hardcore-respect` probe. No HTML5
port of Higan itself was found.

## Next steps for extraction

1. Merge `~/Music/DanOni/.cache-v2` into the main cache with
   `merge-cache.mjs` (one writer at a time), or keep using it as the cache for
   these works.
2. Review the probe engines and recapture each into a **new** directory; the
   probe directories are non-empty. Every page and engine is already cached,
   so only music and late assets are fetched.
3. Run restricted native title captures for the 18 SWF works. Watch for the
   runtime-built carrotcastle sidecar and for the two collection menus. For
   the レク corpus, start with the nine Higan-build works and with the
   Saigetsu/ESS pairs, then review and capture the MFV2 ports.
4. Add accepted works to the export registry. The registry entries and the
   v1 corpus lists are deliberately unchanged here.

## Extraction handover review (2026-10-10)

The tooling session checked the local root index and the three complete CW
manifests and successful `verification.json` reports. The current accepted
export generation is still the separate 19-song / 23-chart corpus; none of v2
has been registered or promoted. The [CW normalization and default musical
fitting](danoni-default-tempo.md) now provide a shared SSC/Ogg/banner path for
eligible verified captures.

Recommended batches:

1. Normalize the three already verified CW works: two single 5-key charts,
   plus NEEDS' twelve played-mode charts (5/7/7i/9A, three each). Keep its five
   11/23-key charts in source evidence. Check source clock/settings, movement
   units, media associations and native-parser agreement before adding recipes.
2. Observe titles, default chart selection, prepared arrays, movement and audio
   for the nine Higan-build movies. Review the changed action blocks per build;
   shared static fingerprints alone do not authorize a pinned adapter. For the
   five absent `iro.txt` files, establish natively whether chart preparation and
   playback proceed; do not fabricate an empty response or suppress a required
   resource error. Declare any unrepresented colors separately.
3. Review unique CW engine hashes and recapture into new directories using
   `.cache-v2`. Review the MFV2 build early to unlock the Saigetsu/ESS paired
   comparisons. A later port may change the chart or audio origin, so confirm
   chart identity before treating it as an equality oracle.
4. Expand native families, collection menus and runtime-named sidecars, then
   handle the older declaration-based CW loaders. Keep failed Wayback media
   and chart-only survivors pending until their actual dependencies are proved.

The root index's exclusion hints are not exhaustive: 9i is also unplayed,
regardless of whether it appears explicitly in `excludedModes`. Eligibility
uses only 5/7/7i/9A/9B. Kitsune Beats' charts must not inherit レク's chart identity
or audio offset merely because the song title matches.

Preserve the separate v2 cache while another capture writer may still be active.
Source metadata stays immutable; classification, fitted BPM/phase, preview and
banner choices belong to derived recipes/reports. Promote a new export generation
only after source-oracle, timing, scroll, decoded-audio and browser-import checks.

## Provenance and limits

`discovery/candidates.json` (`ddi-danoni-discovery/experimental-1`) lists all
64 checked pages, with catalogue row, static classification and decision.
`discovery/catalogue/` keeps the 45 catalogue response bodies and their HTTP
records. `discovery/scripts/` holds the scratch fetcher, parsers, SWF scanner,
acquisition and triage scripts, and `discovery/logs/` holds fetch logs and
capture stdout/stderr. Public availability and robots permission do not grant
redistribution rights. Everything stays local, and catalogue credits are
attribution, not licences.

## First extraction admission (2026-10-10)

The [admission handover](danoni-corpus-v2-admission.md) records 12 accepted works:
the three complete CW captures and all nine Higan-build sources, with 26 charts.
Native execution identified a separate constant-travel family; static matching
was not sufficient. The expanded exports have 31 songs / 49 charts. The other
101 v2 sources are explicit pending registry entries; they have not been claimed
as playable references. Original acquisition counts above remain historical.

## ParaFla pair admission (2026-10-11)

[Saigetsu and ESS](danoni-paired-ports.md) now have verified native references
and playable exports. The gen4 static hint incorrectly described their chart
data as embedded: actual runs request `dos.txt` and `no200_.txt` / `no200_33.txt`.
ESS has 55 menu choices; this admission covers only レク[Normal]. Saigetsu's CW
converter replays identically but its chart differs from the SWF. The ESS port
comparison is still pending startup compatibility. Fourteen v2 works are now
admitted and 99 remain pending; acquisition metadata above remains historical.
