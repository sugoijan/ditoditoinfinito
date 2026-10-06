# Freely redistributable StepMania simfiles for a public MIT browser rhythm game

Researched 2026-10-06. Primary sources: the packs' own repos, `credits.txt`/`license.txt` files and licence pages. "OK" below means: bundling chart + audio + art in a public GitHub repo and serving it non-commercially is permitted, with attribution.

## Summary table

| Pack | Songs | Music licence | Chart licence | Audio | Where | Verdict |
|---|---|---|---|---|---|---|
| OutFox Serenity Vol 1–3 (incl. 1.5/2.5 Winter Updates) | 30 | **Per song**: 4 CC-BY, 14 CC-BY-SA, 1 BY-NC-SA, 1 BY-NC-ND, 7 All Rights Reserved, 1 undeclared (see table) | CC-BY 3.0/4.0 (guidelines) | `.opus` (~190–280 kbps) | github.com/TeamRizu/OutFox-Serenity (+ release zips) | **OK for the 18 CC-BY / CC-BY-SA songs**; skip ARR/NC/ND |
| Sockpuppet: Song and Dance (fluffy) | 8 | CC BY-SA 4.0 (itch "Asset license") | same (one grant covers songs, steps, art) | `.mp3` 140–470 kbps | fluffy.itch.io/sockmania; git.beesbuzz.biz/fluffy/sockmania | **OK**, BY-SA; third-party art on 2 songs is unclear |
| StepMania 5 / ITGmania bundled (Goin' Under, Mecha-Tribe Assault, Springtime) | 3 | "CC-NC" (variant unspecified; RPM Fusion records it as CC-BY-NC-4.0) | implied same | ogg ×2, mp3 ×1 | github.com/stepmania/stepmania `Songs/StepMania 5` | **Risky**: NC only, variant unspecified; avoid |
| Club Fantastic | ~40+ | CC BY-NC 4.0 (art); music © individual artists, non-commercial sharing allowed | n/a | – | clubfantastic.dance | **No** (NC + "not … included in any commercial product without consent") |
| Diesel:U:Music 2006 step files (Dancing Monkeys) | 18 | CC BY-NC | auto-generated | – | 2007 links, likely dead | No (NC, autogen charts) |
| Etterna / Project OutFox engine repos | 0 | – | – | – | – | No bundled songs in repo |
| Dancing Onigiri (danoniplus) | 0 | engine MIT; ships only `nosound.mp3` | – | – | github.com/cwtickle/danoniplus | No bundled CC music |
| "Creative Commons pack", "The Free Pack", Kevin MacLeod/ccMixter/FMA packs | – | – | – | – | – | **Not found** after several searches |

## 1. Project OutFox Serenity

**Sources.** Repo: https://github.com/TeamRizu/OutFox-Serenity (branch `main`; no repo-level LICENSE). Guidelines: https://projectoutfox.com/serenity-guidelines. Download page: https://projectoutfox.com/outfox-serenity (links the all-in-one zip). Releases: https://github.com/TeamRizu/OutFox-Serenity/releases — v1.0 (2021-08), v1.1 (adds "legacy" zip for SM5.x), v1.5 Winter Update (156 MB), v2.0 (AIO 388 MB), v2.5 (2023-02, AIO 481 MB / Vol 2.5 only 78 MB), v3.0s1–s3 (2023-06 to 2023-12, "temporary" test releases, AIO 517–585 MB). No Vol 3 final release exists as of today; the repo was last updated 2026-03.

**Licensing rules (guidelines, verbatim).** Music: "Must be licensed under a free content license that allows for unrestricted distribution, modification, and commercial use" (recommends CC-BY or CC-BY-SA). Charts: "You agree to license your charts under the CC-BY 3.0 or 4.0 license, attributed to the chart's authors." Graphics: CC-BY 3.0/4.0, "If your graphics incorporate CC-BY-SA-licensed material, your graphics must instead be licensed under CC-BY-SA." Third-party content attribution goes in `credits.txt`. README: "Folders might include a credits.txt file which defines license for defined content, if not defined then the content falls back to OutFox Serenity Guidelines."

**Reality differs from the rules.** The per-song `credits.txt`/`license.txt` files show 7 songs are "Ⓒ All original rights reserved" (provided to OutFox only), one is BY-NC-SA, one BY-NC-ND. Release notes confirm: "'Source Code' downloads won't include music files for tracks that aren't licensed under a free content license" — the repo `.gitignore` excludes those 7 `.opus` files; they exist only in the release zips. Several *graphics* are also NC even where the music is free. Chart authors are in per-chart `#CREDIT` tags; all charts are `.ssc` (SM5 loads them, but some styles/gimmicks need OutFox).

**Audio format:** `.opus` everywhere in the repo (checked with ffprobe: Opus, ~200 kbps, 2.5–5.6 MB each). The v1.1 "legacy" zip targets SM 5.x (older codec, unverified). Each song ships `-bn` banner (512×160), `-bg` background (1280×720), `-jk` jacket, sometimes `.mp4` BGA.

**Per-song list** (dance-single meters from the `.ssc`; music licence from the song's credits file; graphics licence noted where it differs):

| Vol | Song – Artist | Music | Graphics | dance-single meters |
|---|---|---|---|---|
| 1 | Abandoned Doll – Aspid Cat | **ARR** | BY 4.0 | 3/6/9/14/16 |
| 1 | Conflicting Revenge – Aspid Cat | **ARR** | BY 4.0 | 3/6/12/15/19 (+Edit 18) |
| 1 | Nexen II (Phase Two) – Jack5 | CC-BY (album "Only The Good Ones", version unspecified) | consented album art | 9/11/13/15/16 |
| 1 | Broken – Matduke | **ARR** | BY 4.0 | 2/4/8/12/13 |
| 1 | Heartbeat – Matduke | **ARR** | BY 4.0 | 3/5/9/14/17 (+alt set) |
| 1 | Let Me See You – Matduke | **ARR** | BY 4.0 | 1/4/7/12/14 |
| 1 | Tagebuch der vergangenen Erinnerungen – PizeroFox | BY-SA 4.0 | BY-SA 4.0 | 2/5/9/12/15 |
| 1 | brokenHeart resurrection ~estelle~ – Seo | BY-SA 4.0 | BY 2.0 / BY-SA 2.0 photos | 4/9/14/18/20 |
| 1 | Some Things Must (OutFox Edit) – Sevish | **BY 4.0** | BY 3.0 | 2/9/10/13/16 |
| 1 | After The Ending – mmry | **undeclared** (no credits.txt; only a `Credits.ini` naming "Credits Music: mmry") | – | 2/5/8/13/18 |
| 1 | synthborn lovebirds – spirai project "drazil" | BY-SA 2.0 (incl. graphics) | BY-SA 2.0 | 3/5/11/13 |
| 1.5 | Plasma – DJ Megas | **BY-NC-SA 3.0** | BY-SA 4.0 | 2/5(7)/10/14 |
| 1.5 | Low End Theory – Matduke | **ARR** | BY-SA 4.0 | 3/7/12/16/18 |
| 1.5 | Umi's Secret (Chiptune Mix) – ペコネコ | BY-SA 3.0 | BY-SA 4.0 | 1/2/4/8 |
| 2 | B-Happy – Ace of Beat | BY-SA 4.0 | **BY-NC-SA 3.0 ES** | 2/6/10/13 (+Edit 14) |
| 2 | Technological ≠ Emotional – Ace of Beat | BY-SA 4.0 | BY-SA 4.0 | 3/18 only |
| 2 | Sweeteners – Jack5 | **BY 3.0** | BY 4.0 | 3/10/13 |
| 2 | Beatucada – Kurio Prokos | BY-SA 4.0 | BY 4.0 | 1/3/7/11/11 |
| 2 | Into My Dream – Lagoona (Andreas Viklund) | **BY 4.0** | BY 4.0 (Kevin Turner & Step Revolution) | 1/2/3/8/12/15 |
| 2 | chop chop – Rilliam | BY-SA 3.0 | artwork "taken from the game Fech The Ferret" (unlicensed) | 3/5/7/9/9/14/17 |
| 2 | Neutralize (PTB10 Mix) – SiLiS | BY-SA 4.0 | BY 4.0 | 3/5/10/12/15 |
| 2 | Bounded Quietude – SiLiS vs Finite Limit | BY-SA 4.0 | **BY-NC-SA 4.0** | 6/10/13/16/19 |
| 2 | CRUSH THE DEVIL (IN MY BRAIN) – td | **ARR** | BY 4.0 | 2/14 |
| 2 | Phycietiia – rN & SiLiS | BY-SA 4.0 | BY-SA 4.0 | 2/4/7/11/15/16 (+Edit 18) |
| 2.5 | Halcyon – Akako Hinami | BY-SA 4.0 | BY-SA 4.0 | 3/18 only |
| 2.5 | Summer Overload! – Akako Hinami | BY-SA 4.0 | BY-SA 4.0 | 2/7/8/9/13/15 |
| 2.5 | Relaxation Piece of Conclusion – Zenth | BY-SA 3.0 | text says "CC BY-NC 4.0" but links BY-SA 4.0 (contradictory) | 1/3/6/9/11 |
| 3 | Run 4 Cover – DeBisco | **BY-NC-ND 3.0** | BY-NC-ND 3.0 | 16 only |
| 3 | What Year Is This – Sevish | **BY 3.0** | "BY-NC 4.0" (same contradiction) | 1/5/8/14/17 |
| 3 | PLANETES – SiLiS | BY-SA 4.0 | BY 4.0 | 17 only |

Redistribution in a third-party open-source game: permitted for the CC-BY and CC-BY-SA songs (both allow commercial use and derivatives). ND is present (Run 4 Cover) and NC is present (Plasma, Run 4 Cover, plus several graphics) — exclude those. Note the Serenity website volume pages (projectoutfox.com/outfox-serenity/volume-i, /volume-ii) list songs but *no* licences; the per-folder credits files are the only primary record.

## 2. Sockpuppet: Song and Dance (fluffy)

- itch page https://fluffy.itch.io/sockmania: 8 songs, "Asset license: Creative Commons Attribution_ShareAlike v4.0 International", 47 MB zip (v18), name-your-own-price. Source: https://git.beesbuzz.biz/fluffy/sockmania (clone is 91 MB because Delicious Candy ships a 14.8 MB `.mp4` BGA).
- README: "All songs, steps, and artwork (c) j. 'fluffy' shagam unless otherwise specified." There is **no LICENSE file in the repo**; the CC BY-SA grant exists only on the itch page. Third-party art: cloverfirefly & Robin Kaplan (President Katya), Inspector Caracal (New Dawn) — their licence is not stated, so avoid those two songs' graphics or ask fluffy.
- Format: `.sm` only, **dance-single only**, mp3 audio, banner/bg/jacket PNG/JPG. Charts (meters, duration, mp3 size):
  - Don't Let The Door Hit You — 2/3/5/7/10 — 58 s — 1.9 MB
  - Sliced by a Mandolin — 3/4/6/7/9 — 216 s — 7.4 MB
  - Wiener Dog on a Motorcycle — 2/4/6/8 (+Edit 9) — 89 s — 5.2 MB
  - Get Go — 2/4/6/9 — 82 s — 2.7 MB
  - Delicious Candy — 3/7/9/10 (+Edit 16 by Curus Keel) — 134 s — 2.3 MB
  - Spooky — 4/6/8 — 171 s — 6.1 MB
  - President Katya — 4/6/8 — 76 s — 1.5 MB
  - New Dawn — 2/6/8 — 74 s — 2.3 MB
- Chart author is fluffy (Curus Keel for one Edit). Both `.sm` and audio fall under the single BY-SA 4.0 asset grant. Confidence: high for music/steps, medium for art.

## 3. Other packs

- **StepMania 5 / ITGmania bundled songs.** README (https://github.com/stepmania/stepmania/blob/5_1-new/README.md): "Any songs that are included within this repository are under the CC-NC license" and you may not "Sell the game *with the included songs*". No variant (BY-NC? BY-NC-SA?) or version is given; RPM Fusion ships StepMania in *nonfree* for this reason with `License: MIT AND CC-BY-NC-4.0` (https://github.com/rpmfusion/stepmania/blob/master/stepmania.spec). ITGmania (`Songs/StepMania 5` on `beta`) bundles the same three. Charts: Goin' Under (NegaRen, Fraxtil charts) 1/3/6/8/10 ogg 2.3 MB; Mecha-Tribe Assault (Kommisar, Wyde charts) 2/6/8/10/11 ogg 2.1 MB; Springtime (Kommisar) 1/4/6/9/12 mp3 5.2 MB. Non-commercial hosting would technically fit NC, but the variant is undocumented and SM issue #296 (open since 2014) describes licence records as "scattered, out of date, and incomplete". Low confidence; not recommended. The old SM 3.9 songs were removed precisely for "questionable origin".
- **Etterna** (`Songs/` has only `instructions.txt`), **Project OutFox** engine repo (no Songs dir) — nothing bundled in-repo.
- **Dancing Onigiri (danoniplus)** is MIT code and ships only `music/nosound.mp3`; community works on wikiwiki.jp/danoniplus are not CC-verified. Nothing reusable.
- **Diesel:U:Music 2006 step files** (Boing Boing, Jan 2007): 18 CC **BY-NC** songs with Dancing Monkeys auto-generated steps; hosted on the Dancing Monkeys site, likely dead. Not usable.
- Searches for a "Creative Commons pack", "The Free Pack", Kevin MacLeod/Incompetech, ccMixter or FMA-based StepMania packs with CC charts returned nothing verifiable. If wanted, the viable route is to chart CC music yourself.

## 4. Practical bundling guidance

**Best MVP candidates (CC-BY music and CC-BY graphics, 5 dance-single difficulties, moderate meters):**
1. **Sevish – Some Things Must (OutFox Edit)** (Serenity V1): music BY 4.0, art BY 3.0, 2/9/10/13/16, 145 s, 3.9 MB opus.
2. **Lagoona – Into My Dream** (V2): music BY 4.0, art BY 4.0, 1/2/3/8/12/15, 147 s, 3.6 MB opus (+ optional mp4 BGA/banner).
3. **Kurio Prokos – Beatucada** (V2): music BY-SA 4.0, art BY 4.0, 1/3/7/11/11, 108 s, 2.7 MB.
   Alternates: Jack5 – Sweeteners (BY 3.0, 3/10/13, 157 s, 4.1 MB); SiLiS – Neutralize (BY-SA 4.0, 3/5/10/12/15, 117 s, 3.1 MB); Pekoneko – Umi's Secret (BY-SA 3.0, easy 1/2/4/8, 84 s, 2.5 MB).
   From Sockmania (BY-SA 4.0, mp3): **Don't Let The Door Hit You** (2/3/5/7/10, 58 s, 1.9 MB) and **Sliced by a Mandolin** (3/4/6/7/9, 216 s, 7.4 MB) are the broadest ladders; Wiener Dog on a Motorcycle is a good low-BPM (90) beginner pick.

**Format notes.** Serenity audio is Opus-in-Ogg; fine in Chrome/Firefox/Edge, test Safari or transcode (a transcode is a reproduction, not an adaptation, so the licence is unchanged). Sockmania mp3s are high-bitrate; re-encoding to ~128 kbps would roughly halve size. Charts: Serenity `.ssc` (per-chart `#CREDIT`), Sockmania `.sm`.

**ShareAlike impact.** Bundling BY-SA audio in a game is a "collection", not an adaptation (CC BY-SA 4.0 §1), so your MIT code is unaffected; only modified versions of the song/art must stay BY-SA. Keep the repo LICENSE clear that `assets/songs/**` are under their own CC terms, not MIT.

**Attribution text that must ship** (CC-BY/BY-SA 4.0 §3(a); 3.0 is similar): for every song and graphic — title, creator, source URL, licence name with link, and a note of any modifications (e.g. transcoded/trimmed). Serenity songs also need the **chart author** credited (charts are CC-BY to their authors) and the OutFox Serenity project named as source. Example line: `"Into My Dream" by Lagoona (Andreas Viklund) — CC BY 4.0 — https://github.com/TeamRizu/OutFox-Serenity — charts by <#CREDIT names> (CC BY 4.0) — artwork by Kevin Turner & Step Revolution (CC BY 4.0) — audio transcoded to ogg/vorbis.` Ship it as a CREDITS/LICENSES-ASSETS file and show it in-game.

## 5. Looks CC but isn't / traps

- **Club Fantastic** (https://clubfantastic.dance/): "Graphical art license: Creative Commons BY-NC 4.0"; "Music copyrights owned by individual artists"; "Club Fantastic music and art may not be sold or included in any commercial product without consent of the creators." NC plus artist-owned music: do not bundle.
- **StepMania 5's three songs**: "CC-NC" with no variant/version; treated as CC-BY-NC-4.0 by packagers. Not MIT, not clearly redistributable.
- **Serenity ARR songs** (all Matduke, Aspid Cat, td tracks) are in the release zips but *not* CC; "provided to Project OutFox" only. **Plasma** (BY-NC-SA), **Run 4 Cover** (BY-NC-ND), and NC/undeclared graphics on B-Happy, Bounded Quietude, chop chop (art lifted from a commercial game), Zenth/Sevish-V3 (text says BY-NC, link says BY-SA). **After The Ending** has no licence declaration at all; the README fallback to guidelines is a weak basis — avoid or ask.
- **Sockmania**: repo README says "(c) fluffy" with no LICENSE file; the CC BY-SA grant lives only on the itch page. Screenshot/archive that page and cite it in your credits.
