# Song sources: native plugins for the desktop app (2026-11-25)

Status: a design, nothing built. Revised the same day after two
decisions of the maintainer (2026-11-25): the game ships no source
plugin (section 3), and songs not imported stay in memory (section 1).
The maintainer decides the open questions at the end; nothing else here
is decided beyond their own words below.

## The request

The maintainer, 2026-11-25:

- "I'm leaning towards defining a plugin interface that covers all
  features (searches, some filters, download, upload, etc) that
  maintainers themselves (from FFR/Zenius/Etterna/etc) can use to
  implement the integration into the client. I'm wondering what would be
  a good example implementation for that plugin interface that's still
  useful."
- "The plugin idea I had in mind is more like the DanOni SDK, only
  targeting the desktop and compiled natively, not a Lua script. A plugin
  could be used to implement local search within a directory, for
  example. Also, the plugin implementer doesn't need to implement every
  'interface', or at least we should model it that way, so the DanOni SDK
  could be one implementation of it, it would take an 'input' being the
  base URL to read from, the interface could be used to expose
  'importing' songs as well as just playing without importing or adding
  them to an online room, also without importing into the local library."
- "About songs that shouldn't be imported, we should prioritize keeping
  them in memory only and using known cache dirs when that's not
  possible/feasible." (A decision, section 1, "Memory first".)
- "I think we shouldn't ship any plugins by default. Even DanOni SDK I'm
  leaning more towards not including it and maybe even moving to another
  repo (but not now)." (A decision, section 3, "No plugin is shipped".)
- FFR levels a player has are not to be blocked from rooms ("if the
  author that uploads to FFR also wants to upload the unconverted level
  here, it should be possible too"). Rooms and shares are private and
  temporary; terms of use, an upload notice and a takedown contact are
  drafted elsewhere (round TU), not here.

And from the research of round SR (four community sites read on
2026-11-25; the coordinator's notes): the game scrapes no one. An
integration with a site is for that site's maintainers to write or to
sanction. With plugins anyone may publish, the game cannot ensure that.
It makes it the natural way (the SDK and the template say so), and it
enforces what it can whatever a plugin says (section 4).

## In short

- A **source plugin** is a native program **a player installs**: a site's
  maintainers (or anyone) publish it themselves, from their own
  repository, on their own schedule. It runs in a process of its own and
  speaks a versioned framed protocol over anonymous pipes, as the DanOni
  importer does today. **The game ships none** (decided by the
  maintainer). It provides the host, the protocol, the confinement and an
  SDK.
- It declares **capabilities**, each optional: inputs it resolves,
  search, filters, browse, metadata, fetch, and later sign-in and upload.
  The app offers only what a plugin declares and its manifest allows.
- A fetched song has **three uses**: import it into the library, play it
  without importing, add it to an online room without importing. The
  plugin does not know which; the app decides and enforces the source's
  rules.
- **A song that is not imported lives in the app's memory** for as long
  as a play or a room needs it, and is never written to disk. Only where
  memory will not do (over the budget, a movie the decoders open by
  path) does it go to the app's known cache folder, with a cap and
  cleanup as a room's store has. Imports keep their store. (Decided by
  the maintainer.)
- What makes a plugin nobody reviewed tolerable is that it can do
  nothing by itself. It is **confined by the app before its first
  instruction runs**, with no network, no files and no other processes,
  and with memory and CPU bounded. **The app makes every network request
  and every file read** the plugin describes, to the hosts its manifest
  declares, at the app's own pace. At worst it returns bad data and
  wastes its bounded time.
- Recommended: **an installed plugin never runs unconfined**. Where
  confinement is not verified (Windows today), plugins are unavailable.
- The player installs a plugin from a file they picked, in Options. The
  game shows what the plugin declares, in its own words, before it is
  enabled. A signature pins the publisher's key, so an update must come
  from the same key. There is no store or registry run by the maintainer.
- **The DanOni importer stays shipped and unchanged for now.** Its move to
  an installed plugin in its own repository is a later step, which
  changes nothing in the interface (section 5).
- Searching a folder on this computer becomes a **built-in feature**,
  behind the same interface the plugins use. The **examples, the SDK and
  the conformance harness** live in a separate repository; they are
  never in the game's packages. The game's own shares stay as they are.

## What exists today (read in the code, 2026-11-25)

**The DanOni helper and its protocol.**

- `crates/importer-protocol` (`ddi-importer-protocol`, protocol version
  1): a 4-byte big-endian length, then UTF-8 JSON, at most 32 MiB a
  frame; `deny_unknown_fields` everywhere, and an exact version match on
  both sides (`Event::Hello { version != VERSION }` is
  `Problem::Incompatible` in the app; the helper answers a wrong version
  with `Fatal`).
- Its messages are DanOni-shaped:
  - `Initialize { paths: {cache, staging, library}, origins }`;
  - `Import { id, url, offline }`;
  - `OriginRequired` and `OriginDecision`;
  - the `Stage` enum of the importer's steps;
  - snapshots of the managed catalog: `SnapshotBegin`, `SnapshotSong
    { entry: ManifestEntry as JSON, directory }`, `SnapshotEnd`;
  - `Terminal` with `Outcome` and `Counts`.
- Errors are values: `Failure { code, message }`, where the app words a
  `code` it knows and shows `message` as data.
- `Capabilities { url_import, offline, origin_approval }`, all three
  required by the app.
- The helper runs one job at a time with strictly increasing ids, and
  keeps decoding stdin while a job runs on its own thread.
- The app side (`crates/desktop/src/danoni.rs`, `danoni/client.rs`) runs
  the helper on a thread of its own:
  - bounded channels both ways;
  - a 32 KiB tail of stderr;
  - 30 minutes for an import, 5 for opening the catalog;
  - 10 s to stop after Cancel or Shutdown, then a kill;
  - a snapshot admitted only when complete, and only with directories
    inside the managed library.
- `danoni/bundle.rs` verifies the bundle before every start:
  - `danoni-importer/<id>/` beside the executable, or in a macOS app's
    resources;
  - every file's size and SHA-256 against a manifest whose digest is
    compiled into the app (`DDI_DANONI_BUNDLE_ID`,
    `DDI_DANONI_BUNDLE_MANIFEST_SHA256`);
  - no extra files, no symlinks.
- The helper and the app are therefore always the same build: the
  protocol's compatibility is a build-time matter, never a field one.
- **The helper process itself is not confined.** It is spawned as an
  ordinary child (`crate::os::spawn`, a plain `Command`) and makes its
  own HTTP requests (the SDK's `reqwest` and `hickory` stack). Only its
  Ruffle worker, which runs the Flash, is confined:
  - Seatbelt through `sandbox-exec` on macOS;
  - Landlock ABI 6 and seccomp on Linux;
  - not on Windows (no importer there).
- The SDK's network discipline lives in the SDK, not in the app:
  - public addresses only, pinned after DNS (`pin_public_addresses`);
  - per-origin pacing, 1 s at least (`Pacer`);
  - request, byte and time limits;
  - receipts that say `user-initiated`.
- The app links only the protocol crate. It is on Rust 1.98; the SDK and
  helper need 1.99.
- The importer exists on macOS and x86_64 Linux. Windows builds and the
  web have none.

**The managed store and the library.**

- The SDK's managed store is transactional; its verified catalog is the
  publication point.
- The app turns each snapshot row into a `LibSong`
  (`library/managed.rs`): the SDK's id is kept, the pack defaults to
  "DanOni imports", and chart versions are derived from a clone of the
  verified SSC.
- `Library` (`crates/desktop/src/library/mod.rs`) merges three sets by
  id:
  - folder songs, which win a collision;
  - managed songs;
  - a share's songs (`set_shared`).
- Lookups outside the list (`library::song`) look in three places, in
  order: tests' songs, then online rooms' shared songs
  (`register_room_songs`, kept for the run), then the scan's registry.
- A play and an upload both find a song's files through
  `play::load::Files::of(id)`, which asks `library::song` and then the
  bundled songs.
- Imported folder songs get `pack::song_id(pack, dir)`: `u-` and 16 hex
  digits, the same on the web, so bests and offsets match.

**Shared songs and room uploads.**

- A share is read into the cache folder by `crate::share::load_with`:
  - its files go to `shares/<id>/<n>/song/`, or to `rooms/<store>/<id>/`
    for a room's store;
  - its songs get the ids `shared:<id>:<n>`;
  - `clear_stores` deletes `rooms/` at the start of every run that opens
    the window, and `forget_store` deletes a room's store when the room
    is left.
- A room's song list (room model revision 17) names a song by
  `ddi_share_protocol::song_key`: the sorted digests of its original
  charts through FNV-1a 64, `s1:` and 16 hex digits.
- `RoomClient::add_song(&SongEntry)` (`crates/ui/src/state/room.rs`)
  checks four things: who may add, that the digests are known, the room's
  own refusal, and its list's rules. It then remembers the entry in the
  client's `library` map by key.
- The room's song select resets that map from its data on each refresh
  (`set_library(&self.data.songs)`, `select_room.rs`).
- The upload (`crate::room::upload` → `share::upload_room`) gathers the
  files through `Files::of(id)` and `ddi_library::share::stored_files`,
  then sends them under the room's grant. Any song `library::song` finds
  can be uploaded; nothing checks where it came from.
- A share holds at most 2 GB, a file at most 512 MB, and 200 songs. A
  room's list holds at most 100 songs.
- Room shares are readable by anyone holding their id
  (`docs/plans/shared-songs.md`, "Room shares").

**How a play reads a song's files on desktop** (`play/load.rs`,
`images.rs`, `preview.rs`, `video.rs`):

- `Files` is a folder (`Dir`) or a zip read in place (`Zip`). Its
  `read`, `read_head` and `read_at_root` return bytes.
- **The chart** is parsed from those bytes.
- **The music** is read whole with `files.read`, then decoded and
  resampled from the bytes (`audio::decoder::sound(bytes, ext, range,
  rate)`, `ddi_platform::decode_all`). No path is needed.
- **Lyrics, a work's data and the files of scripted backgrounds** come
  through `Files` too.
- **Pictures** (banner, background, jacket, background changes' images)
  are texture sources named by a string: an absolute path, or
  `zip:<zip>!<entry>`. `images::load_sized` reads the bytes and decodes
  them in process. Bytes from elsewhere would need one more kind of
  source.
- **Song select's preview** takes a path (`PreviewSong::music`). A zip's
  music, or an FFR level's, is first extracted into the cache folder's
  `previews/`.
- **Movies** are paths (`Files::movie` → `MovieAt`: a path and an
  optional byte range). A zip's compressed movie is extracted into the
  cache folder's `movies/`, kept to `MOVIE_CACHE` bytes. The video
  helper never opens a file: the game opens the path (`OpenFile`) and
  serves reads. The AVI path and the OS decoders' demuxing also start
  from a path.
- **The room's upload** reads `Files` too: a folder's file as a path
  streamed, a zip's entry as bytes (`share.rs`, `source_of`:
  `Source::File` or `Source::Bytes`; `send_share` sends either).
- **The scanner** reads through `ddi_library::scan::Reader`: relative
  paths in, bytes out.

So everything but pictures, the preview and movies already works from
bytes.

**The video helper's sandbox** (`video/helper/src/sandbox/`, decision
54):

- The helper confines itself before it reads anything:
  - Seatbelt `(deny default)` through `sandbox_init` on macOS;
  - `RLIMIT_AS` 2 GiB, Landlock with no rules and a seccomp allowlist on
    Linux;
  - a restricted token, untrusted integrity and a job object on Windows,
    written but never run on Windows.
- It then tries to open a file, list a directory, reach localhost, bind a
  socket and start a process. Any success and it refuses to serve.
- It never opens a file: the game serves every read (`Read` requests of
  at most 1 MiB).

That last pattern is the model for plugins below. One difference
matters now: the video helper confines **itself**, which is fine for our
own code. A plugin nobody here reviewed cannot be trusted to call its own
sandbox. Its confinement must come from the app, before its first
instruction runs.

**Confining a program we did not write, from the outside** (the DanOni
SDK's supervisor, `tools/danoni-dump/rust/src/supervisor/process.rs`):
the SDK already runs the Ruffle worker, untrusted Flash, under a sandbox
its parent sets up before the worker's code runs.

- **macOS**: through `/usr/bin/sandbox-exec -f worker.sb`, with the
  worker's paths given as profile parameters.
- **Linux**: in a `pre_exec` hook, between fork and exec, the parent
  applies the confinement to the child before it becomes the worker:
  - `setsid`;
  - `RLIMIT_CPU`, `RLIMIT_FSIZE`, `RLIMIT_NOFILE` 128 and `RLIMIT_CORE`
    0;
  - Landlock (ABI 6 required) and a seccomp allowlist.
- **Both**: the environment cleared, and stdin closed.

That is the shape an installed plugin needs (section 3).

**Song data is already treated as untrusted input**:

- The library reads any pack a player puts in a folder or a zip, the web
  imports whatever is picked, and a room's members download what another
  member uploaded. A plugin's fetched songs are no new kind of input.
- The chart crate has no `unsafe`; the library has one use, in
  `path.rs`.
- The zip reader is sans-IO and checks every length it is given. Its
  decompression is bounded by the declared size
  (`decompress_to_vec_with_limit`), and its tests damage and cut archives
  (`zip_fuzz_other_methods`, opt-in).
- An FFR SWF is refused over 256 MiB, and its interpreter is bounded in
  actions, stack and values (`swf.rs`).
- Pictures are decoded in process with bounds: 48 MiB a file, 8,192
  pixels a side and 96 MiB an allocation (`images.rs`).
- Music is decoded in process by pure Rust decoders: Symphonia, and
  `rusty-opus` (pure Rust, with SIMD `unsafe` it documents).
- Scripted backgrounds run on lunita, the sandboxed Lua, with fuel, a
  deadline and memory limits.
- Movies are decoded by the sandboxed video helper, or by the OS decoders
  for checked subsets of H.264, HEVC and VP9.

What is not claimed:

- There is no fuzzing campaign over the chart parsers, only the zip
  reader's damage tests.
- A huge but well-formed chart is bounded only by the memory it takes.
- The image and audio crates are third-party code with their own `unsafe`.

The claim holds in the sense that matters here: a plugin's songs pass
through exactly the code every pack from the internet already passes
through.

## 1. The model

### Sources, items, capabilities

A **source** is what the player sees as one place to find songs: an
installed plugin (a site), or a source built into the app ("This
computer", the folder search: section 5). Both implement the same
interface; a built-in source runs it in process.
It offers **items**. An item is a song or a
pack, named by an opaque id the plugin chooses (at most 256 bytes) and
stable for that source.

Each capability is optional. A plugin declares those it implements in its
manifest and again in its Hello; the app offers only what both say:

| Capability | What it means |
|---|---|
| `input` | The plugin resolves something the player gives it into a **scope** it can search, browse or fetch from. Kinds: `folder` (chosen in the app's own folder dialog; the plugin receives a handle, never the path), `url` (pasted; the app checks it against the hosts the manifest declares), `link` (a link the app routes to the plugin whose manifest claims its host and path pattern), `text` (free text such as a pack name or a code). A source without inputs has one implicit scope (a catalog). |
| `search` | A query (at most 256 characters, possibly empty) with filter values, in a scope. Results stream in pages of at most 50; a cursor asks for the next page; Cancel ends it. |
| `filters` | A schema the plugin declares once, in Hello, which the app renders: `text`, `choice` (single or multiple), `range` (min, max, step, a unit from a closed set: meter, BPM, seconds, bytes, count) and `toggle`. Labels are the plugin's text (section 2, "Text"). The app knows no site's fields. |
| `browse` | Nodes (packs, categories, folders), each with children and items, paged as search is; for a source without search, or beside it. |
| `metadata` | An item's description before it is fetched: title, subtitle and artist (with transliterations); pack; charts (style as one of our layout ids, difficulty as one of our six, meter, chart name, chart author); length; BPM; size in bytes; a banner (a URL the app fetches or a path in a folder scope); the source's own page for it (on a declared host); licence as the source states it (an SPDX id and/or text and a URL); attribution text; when it last changed; an opaque revision; and, where the source can compute it, our `song_key`. |
| `fetch` | One item made into files the app can read, with progress, limits and cancellation (section 1, "What a fetched item is"). |
| `signin` | **Not in version 1.** Sketched in section 3. |
| `upload` | **Not in version 1.** A site's own maintainers' upload of a song the player chooses to that site. Sketched in section 5. |

A plugin implements only what it needs:

- a folder source: `input.folder`, `search`, `filters`, `browse`,
  `metadata`, `fetch`;
- the DanOni importer: `input.url` and `fetch`;
- a catalog: `search`, `filters`, `metadata`, `fetch`.

### The three uses

The plugin fetches; the app decides what the fetched files are for. A
fetch request does not say which use it serves: no plugin can treat an
import differently from a play, or count rooms.

1. **Import into the library.** The app copies the fetched files into an
   app-owned import store (below) and lists them with the library from
   then on.
2. **Play without importing.** The app lists the fetched songs alone, as
   song select's shared mode lists a share (`Library::set_shared`,
   `--shared`), and plays them through the ordinary start prompt, play
   session and results. Plays are kept in the history; bests are keyed
   by chart digest, so they show beside the same chart from the library
   later, as shared songs' do.
3. **Add to an online room without importing.** The app hands the
   fetched song's `SongEntry` to `RoomClient::add_song`. The room's
   upload then reads its files through `Files::of`, exactly as for a
   library song. The source's room rule (section 4) is checked first.
   Members load it from the room's share as today. Nothing
   source-specific crosses the wire, so **no room model revision is
   needed**.

### Memory first (decided, 2026-11-25)

The maintainer: "About songs that shouldn't be imported, we should
prioritize keeping them in memory only and using known cache dirs when
that's not possible/feasible."

So, for the two uses that do not import:

- A fetched song lives **in the app's memory only**, for as long as it
  is needed: the play, its prompt and its results; and for a room, its
  upload and the time the room lists it. It is never written to disk.
- **Only where memory will not do** does it go to the app's **known
  cache folder** (`ddi_desktop::paths::cache_dir()`, where `shares/`,
  `rooms/<store>/`, `previews/` and `movies/` already are), under
  `sources/`. Never the data folder, never the library.
- What goes to the cache has a lifetime, a cap, and cleanup at exit and
  at the next start, as a room's store has.
- **The import path does not change**: an import goes to its store (the
  managed store for DanOni; the import store below for others).

When memory will not do, at once or for one file:

- **Over the budget.** An item larger than the in-memory budget, or one
  that would push the held items over it (the budgets are below).
- **A movie.** The video helper, the AVI reader and the OS decoders'
  demuxing all start from a path. A movie is also large: the room's
  preloading leaves movies out for the same reason (`is_small`). A
  fetched song's movies are not fetched with it. They are fetched only
  when a play of it loads, and then written to the cache (as a share's
  movies are fetched at a play's load, `share::ensure_all`). With a
  setting off, they are not fetched at all, and the song's background
  shows, as for a copying import that leaves movies out.
- **A plugin that writes files itself.** Only the DanOni helper, whose
  SDK prepares in its own `cache/danoni/staging` (section 5).
- **A platform limit.** None is known on macOS or Linux: the bytes are
  ordinary heap memory.

Streaming the music is not needed: the play's loader reads the music
whole and decodes it from bytes today, for every song.

### What a fetched item is

**In memory**, the app's `FetchedItem` holds:

- the plugin's id and the item's id, and the revision it was fetched at;
- its files: a map from each file's path in the item (`RelPathBuf`) to
  its bytes (`Arc<[u8]>`), or, for what had to go to the cache, the
  file's path there;
- the songs the app read from those files: `LibSong`s with transient ids,
  below;
- the source's metadata as stated, the attribution and the page link;
- a record of what was requested (section 3, "Receipts");
- its bytes;
- what holds it: a play, a room, an import in progress.

By the kind of delivery the plugin reports:

| Delivery | What it is | Where its bytes are |
|---|---|---|
| `in_place` | A pack or song folder (or a zip) inside a folder scope the player chose. | Where they already are, on the player's disk: nothing is copied, and nothing is written. |
| `files` | A folder tree (packs of song folders, or one song folder). | In the app's memory: the app downloaded them itself (`Download`), or the plugin sent them (`Write`, small files such as a converted chart). |
| `archive` | One zip or `.smzip`. | In the app's memory, read through the library's sans-IO zip reader over the bytes, as it reads a zip in place over a file today. |
| `managed` | The DanOni helper's verified catalog rows, as today. | The helper's own store. Reserved for that one plugin; import only. |

The plugin never hands the app a path it wrote. Since the app makes the
downloads itself (section 3), a fetched song's bytes arrive straight in
the app's memory and never pass through the plugin. Only a file the
plugin makes crosses the pipe, as `Write` attachments of at most 1 MiB a
frame and 64 MiB an item.

A shared-memory arena, as the video helper uses for frames, was weighed
and is not needed. It earns its keep for 12 MiB frames sixty times a
second. Here only small, rare writes cross the pipe; the bulk never does.

**The budget in memory** (the numbers are proposals, open question 7):

- **64 MiB a song** for its small files (chart, music, pictures, lyrics,
  preview: what `ddi_library::share::is_small` counts). A typical song is
  3 to 15 MB, mostly its music (an Opus or OGG of three minutes, an MP3
  at 320 kb/s about 7 MB). 64 MiB covers long songs and large art with
  room to spare. A song over it is held in the cache instead.
- **256 MiB for all the fetched items held at once.** That is about 20 to
  30 typical songs: a room list's worth that this player added, or a
  session of trying songs. It is twice the room store's "Likely next"
  budget (128 MB, `room-preload-v1`), against a play that already holds
  its decoded music as 32-bit samples (about 70 MB for three minutes of
  48 kHz stereo).
- Past 256 MiB, the least recently used item that nothing holds is
  dropped from memory: it can be fetched again from its source, by a
  person's press. An item something holds is never dropped. If holding
  it would pass the budget, it **moves to the cache** instead (below).
- A pack fetched for a play keeps only the song being played in memory
  once the scan is done; the rest go when the budget needs them.
- `in_place` items take no budget: their bytes are read from the
  player's folder when needed, as a library song's are.

**The cache, where memory will not do**:

- The cache folder's `sources/<item-hash>/`: files at their paths in the
  item, written through a `.part` and renamed, as `share::ensure_file`
  does.
- A cap of 2 GiB, least recently used first, never removing what a play
  or a room holds.
- Deleted when nothing holds the item any more, at a clean exit, and at
  the start of the next run that opens the window (beside
  `share::clear_stores`, which deletes `rooms/`).

**The app reads every delivery with its own scanner**
(`ddi_library::pack::find_songs`, `scan::scan_song`). Desktop's
`DirReader` and `ZipReader` serve folders and zips on disk; a
`MemReader` over the item's map of bytes serves memory, the same
`scan::Reader` trait. What the plugin said before the fetch is a
description for the screens. The truth (charts, digests, files) is what
the app reads. The `managed` delivery keeps today's path
(`managed::songs`).

### Playing from memory: what changes

Verified against the loaders (section "What exists today"):

- **`Files` gets a third kind**, `Memory { item, dir }`, over the item's
  shared map of bytes. `read`, `read_head` and `read_at_root` answer from
  the map. The chart, the music (decoded from bytes as today), lyrics, a
  work's data and scripted backgrounds' files then need nothing else.
- **`LibSong`** holds a folder or zip `root` today. A transient song
  needs to say "memory item": an enum in place of `root` and `zip`, or a
  field beside them. `Files::lib` dispatches on it. About 25 places read
  `.root` directly. Most are folder-only code (the scan, the editor,
  shares, bots) that never sees a transient song; the rest gain a memory
  arm or answer "none" (`stepmania_root`, so no shared folders).
- **Pictures**: a third texture source, `mem:<item>!<path>`, that
  `images::load_sized` reads from the registry of items before
  decoding, as it reads `zip:` entries now.
- **The preview**: `PreviewSong::music` becomes a path or a memory
  source; the preview decodes from bytes, as the play does. Nothing is
  extracted to `previews/` for a song in memory.
- **Movies**: unchanged, and from the cache only (above).
- **The room's upload**: `share::source_of` gets a memory arm answering
  `Source::Bytes` (shared, no copy), which `send_share` already sends.
- **`song_key`**: the app's scan over the `MemReader` writes every
  chart's digest into its `ChartInfo`, as for any song. `room::song_key`
  then takes them from the `SongEntry` as it does for library songs.

None of this touches the web, the session, the engine or the
renderer: the desktop shell alone.

### Identity

**Transient songs** get ids of their own:

- `src:<16 hex digits>:<n>`: the FNV-1a 64 of the plugin's id and the
  item's id and revision, then the song's index in the item;
- they never clash with the library's `u-…`, the bundled songs' ids or
  `shared:…`;
- a history record says where the song was from, as `shared:` does.

They are registered where `library::song` finds them: a fourth registry
beside `register_room_songs`, released when the item is evicted. That
makes `Files::of` (plays, previews, uploads) work for them unchanged.

**The key a room needs** is `song_key`. The app computes it from the
digests its own scan writes into each `ChartInfo`, so a transient song
has one as soon as it is fetched. A folder or catalog source can also
give it before the fetch (its metadata's `song_key`), which lets the
results say "In your library" before anything is downloaded.

**Imported songs** go to an app-owned **import store**: the data
folder's `sources/library/<plugin>/<pack>/<song>/`.

- Each import is written from the fetched item's bytes (or copied from a
  folder scope) into the store's own `staging/`, then renamed into place.
  A song played first and imported after needs no second download.
- It is listed by the ordinary scanner as a root the app owns: not one of
  Options' song folders, and never shown as one.
- It is merged as a fourth set in `Library::merge_songs`; folder songs
  win an id collision, as they do over managed songs.
- Its songs get the ordinary `pack::song_id(pack, dir)`, so the same pack
  imported through Add songs on the web has the same ids.
- A small catalog beside it (`sources/catalog.json`) keeps, for each
  imported song: the plugin, the item, the revision, the source's page,
  the licence as stated, the attribution, the import's date and its
  receipt.

The shipped DanOni importer keeps its managed store and its own ids.

**The same item imported later** is recognized two ways:

- by the catalog's `(plugin, item)`: the results say "In your library",
  or "Changed at the source" where the revision differs;
- by `song_key`: any library song with the same original charts, from
  wherever, shows as "You have this song", and playing it plays the
  library's copy.

A transient song imported after it was played keeps its plays: they are
found by digest.

### Lifetimes, at a glance

| What | Where | Until |
|---|---|---|
| A search's results | The app's memory | The Find songs screen closes. |
| A fetched item nothing holds | The app's memory | Dropped past the 256 MiB budget (least recently used first), or the run ends. |
| An item a play holds | The app's memory; the cache if over budget | The play's results close. |
| An item a room holds, upload not yet confirmed (pending, or failed and offered again) | The app's memory; the cache if over budget | The upload is confirmed, the entry removed, or the room left. |
| An item a room lists, upload confirmed | The app's memory while the budget allows | The entry removed or the room left. Under memory pressure it is released: the room's share then serves this device as it serves every member, through the room's existing store (`rooms/<store>/`, unchanged). |
| A fetched song's movie, when a play loads it | The cache's `sources/` | Nothing holds the item, the cap, or the run ends. |
| An imported song | `data/sources/library/` (DanOni: its managed store) | The player removes it (hidden as other imports are, files kept) or deletes it. |
| A plugin's process | | 60 s idle after the last operation, or the app's exit. |

Memory leaves nothing behind after a crash. The cache's `sources/` is
deleted at the next start, as `rooms/` is.

## 2. The protocol

### A new crate, beside the old one

The options:

- **(A) Extend `ddi-importer-protocol` to a version 2.** One crate. But
  version 1 is DanOni's shape throughout (its `Paths`, its origin
  decisions, its stage enum, its snapshot of `ManifestEntry` JSON with
  absolute directories, its exact version match). Every message would
  change, and so would the shipped helper with it.
- **(B) A new crate, `crates/source-protocol` (`ddi-source-protocol`,
  serde only, platform-free like `importer-protocol` and
  `share-protocol`).** Its own version 1. `ddi-importer-protocol` is not
  touched. The shipped DanOni helper keeps speaking it, as it does now.
  When the importer becomes an installed plugin (section 5), it speaks
  the new protocol, and version 1 retires with the shipped helper in the
  same release.

**Recommended: (B).** Nothing shipped breaks, at any stage.

### Framing

The importer protocol's framing, with one addition for bytes:

- a 4-byte big-endian total length;
- a 4-byte JSON length;
- the JSON;
- optionally, raw bytes after it: the attachment of a file read or of an
  HTTP body. This spares base64 on the reads a folder source makes by the
  thousand.

Bounds:

- 4 MiB a frame (against 32 MiB in version 1: no snapshots cross this
  pipe);
- 1 MiB an attachment;
- `deny_unknown_fields`, and unknown message types refused, as in version
  1;
- truncation distinct from a clean EOF, as in version 1.

### Versions and capabilities

- Hello carries the protocol versions each side speaks; the highest
  common one is used, and none in common ends the session cleanly with a
  value the app words ("This source needs a newer version of the game").
- The plugin's Hello declares its capabilities, filter schema and input
  kinds. They must be a subset of its manifest's, which the app read from
  the installed package, and the player accepted, before starting it.
  More than the manifest is a protocol error, never a widening.
- A new optional message or field needs a minor version both sides
  declare. A message neither side declared is never sent.

### Operations

- Several operations at once, each with an `op` id the app chooses
  (nonzero, increasing). At most 4 outstanding to a plugin: a fetch and a
  few searches. A plugin may answer `busy`.
- `Cancel { op }` at any time. A plugin must end the operation with
  `Failed { code: cancelled }`, or with its result if that was already
  sent, within 5 s.

### Host calls: the plugin's only I/O

The plugin never opens a file or a socket. It asks the app, which serves
the request under the plugin's manifest and the player's grants, or
refuses it with a value:

| Call | What it does | Bounds |
|---|---|---|
| `List { scope, path, cursor }` | The entries of a folder inside a folder scope. | 1,000 entries a page; paths are `RelPath`, never leaving the scope. |
| `Stat { scope, path }` | Size and modification time. | |
| `Read { scope, path, offset, len }` | Bytes of a file in a folder scope, or of an item being fetched. | 1 MiB a call, as the video helper's reads. |
| `Http { method, url, headers, body }` | One request, the response's status, selected headers and body for the plugin to parse (search pages, JSON, an index). | GET, HEAD, or POST with a body of at most 64 KiB; headers from a short list (Accept, Accept-Language, Content-Type, Range); the body back at most 8 MiB; https only; the host in the manifest's list. |
| `Download { op, url, to, bytes?, sha256? }` | A file of a fetch, downloaded by the app into the item's memory at path `to` (into the cache only where section 1 says memory will not do); the plugin gets the outcome, the size and the hash, never the bytes. | The manifest's per-file and per-item limits; the share's (512 MB a file, 2 GB an item) at most, so a fetched item can always go to a room. |
| `Write { op, to, bytes }` | A file the plugin made (a converted chart), into the item's memory at path `to`. A plugin writes nowhere else. | 64 MiB an item in all. |
| `CacheGet` and `CachePut { key, bytes }` | A small store of the plugin's own in the cache folder (a folder source's index of a large folder, a catalog's index), written by the app. Never a song's files. | 64 MiB a plugin; a key at most 128 bytes. |

At most 8 host calls are outstanding at once, and each operation has a
request budget from the manifest (a search, say 10 requests).

### Messages (a sketch, enough to build from)

App to plugin:

- `Hello { protocols: [u32], app: { version, language } }`
- `Open { op, input: Folder { scope } | Url { url } | Link { url } | Text { text } }`
- `Search { op, scope?, query, filters: {key: value}, cursor?, page_size }`
- `Browse { op, scope?, node?, cursor? }`
- `Describe { op, item }`
- `Fetch { op, item }`
- `Cancel { op }`
- `HostReply { call, result: Ok {…} | Refused { code } }`
- `Shutdown`

Plugin to app:

- `Hello { protocol, plugin: { id, version }, capabilities, inputs, filters: [Filter] }`
- `Opened { op, scope, title? }`
- `Results { op, items: [Summary] }`, repeated as results come, then
  `Done { op, cursor?, total? }`
- `Nodes { op, nodes: [Node] }`, then `Done`
- `Item { op, item: Detail }`
- `Progress { op, step, done?, total?, bytes? }`, where `step` is a
  closed set the app words: resolving, searching, downloading, reading,
  converting, checking
- `Fetched { op, delivery: InPlace { scope, path } | Files | Archive { file } }`
- `Failed { op, code, detail? }`
- `HostCall { call, op?, request }`
- `Bye`

The DanOni adapter's `managed` delivery is version 1's snapshot, in the
app; it never crosses this protocol.

### Errors are values

`Failed.code` is one of a closed list the app words in the player's
language:

- `cancelled`, `busy`, `invalid_input`, `unsupported_input`;
- `not_found`, `gone`;
- `refused_by_source`, `rate_limited` (with a retry time),
  `unreachable`;
- `too_large`, `unreadable_content`, `source_changed`;
- `limit` (a budget of the manifest's), `internal`.

`detail` is at most 2 KiB of the plugin's own words. It is shown only in
a Details fold, as data, as the importer's diagnostics are today
(`Problem::Diagnostics`). No plugin sentence is ever an error's summary.

### Text from a plugin

A plugin's labels are data. The rules:

- Its source name, filters' labels and choices' labels come as a map of
  language tags to strings, English required. The app shows the
  language it speaks now, else English, through `UiText::data`, never
  as a message of ours.
- They appear only in the slots made for them: the source's name, a
  filter's label, a choice, an item's metadata, a detail in the Details
  fold. Each is framed by our own messages ("From {source}", "Filters").
- They never appear on a button, as a heading of ours, or as a sentence
  of ours.
- Each is at most 128 characters (an item's text 512). Control and bidi
  control characters are taken out. Empty text is refused.
- No plugin's text enters `locales/`. A built-in source's labels are
  ordinary messages of ours.

## 3. Trust, confinement and installing

### No plugin is shipped (decided, 2026-11-25)

The maintainer: "I think we shouldn't ship any plugins by default. Even
DanOni SDK I'm leaning more towards not including it and maybe even
moving to another repo (but not now)."

So:

- A plugin is something **a player installs**: a native program that a
  site's maintainers, or anyone, publish themselves, from their own
  repository, on their own schedule.
- The game provides the host, the protocol, the confinement, and an SDK
  published for plugin authors.
- Reference implementations exist as examples and as tests of the host,
  **never in the game's packages** (section 5).
- The DanOni importer stays shipped, unchanged, for now. Its move to an
  installed plugin is a later step (section 5).

This makes the plugin nobody here reviewed **the main case**. The rest of
this section is about that case.

### What an installed plugin is, and what makes it tolerable

A native program the player downloaded is, by default, as powerful as
the player:

- it can read their files: the game's config folder (where the online
  session is kept, `online-session`), their browser profiles, their keys;
- it can reach any host on the network;
- it can start other programs, and stay behind after the game closes.

A digest says a file was not changed. A signature says it came from the
holder of a key. **Neither says the program is harmless**, and no warning
makes it so.

What can make it tolerable is that it can do **nothing by itself**:

1. **Confined by the app, before its first instruction runs.** The app
   starts the plugin through a launcher that confines the process and
   then executes the plugin's file, as the DanOni SDK starts its Ruffle
   worker (section "What exists today"). The confinement:
   - no network;
   - no file access at all, beyond loading its own executable;
   - no other processes;
   - only its inherited pipes, memory and threads;
   - memory and CPU bounded;
   - environment cleared.

   It is not the plugin's own call, as the video helper's is: a plugin
   cannot skip it.
2. **The app makes all of the plugin's I/O** (section 2, "Host calls"):
   - every request, to the hosts its manifest declares, at the app's own
     pace and within the app's own limits;
   - every file read, inside folders the player chose for it, through
     handles; it never learns a path;
   - every write, into the app's memory.
3. **It speaks only the protocol.** Every message is bounded and checked.
   Its text is data in slots made for it. Its songs are read by the
   app's own scanner, from bytes the app downloaded.

Within that, a plugin's worst is to:

- return bad or hostile data;
- waste its bounded memory and time;
- send its declared hosts what it was given;
- make the requests the app allows, to those hosts, at the app's pace.

That is the case this design accepts.

### What remains (the residual risk)

- **A sandbox escape.** A kernel or sandbox bug lets a confined process
  out. Where it happens:
  - on macOS, Seatbelt is a deprecated interface (`sandbox_init`, and
    `sandbox-exec`, its command), still used by Chromium's helpers and by
    the DanOni SDK;
  - on Linux, Landlock and seccomp are kernel features, as strong as the
    kernel the player runs;
  - nothing here removes this risk. It is the risk of running a
    sandboxed browser tab, with less code exposed: the protocol, not a
    web platform.
- **A platform where confinement is not verified: Windows.** The video
  helper's restricted token, job object and untrusted integrity were
  written but never run on Windows. **Recommended: an installed plugin
  never runs unconfined. Where the launcher's self-test cannot pass
  (Windows today, a Linux kernel without the needed Landlock or
  seccomp), plugins are unavailable**, and the screens say so.
- **What it learns from what it is passed**:
  - the queries and filter values the player types;
  - the player's language and the game's version;
  - the names and contents of files inside folders the player gave it;
  - the answers of its declared hosts.

  It is never given:
  - paths;
  - the environment;
  - the game's session, the Twitch identity or other plugins' data;
  - the library, the history or the settings.

  It can send what it learned to its declared hosts, inside the requests
  it describes. A plugin that both reads folders and reaches the network
  could therefore send a folder's names and contents to its hosts.
  **Recommended: in version 1, a manifest may not declare both** (open
  question 10). The consent page says plainly whatever a plugin may do.
- **Hostile data**:
  - Song files go through the same parsers every pack from the internet
    already does (section "What exists today": song data is already
    treated as untrusted).
  - Its text is bounded and cleaned (section 2).
  - Its banners are decoded with the images' bounds.
  - A link it gives ("the source's page") must be https, on a declared
    host. It opens in the player's browser, with the host shown first.
- **Wasted resources**: bounded by the limits below. A plugin that hangs
  or floods is killed. One that crashes three times in a run is off for
  the run.
- **Abuse of a site through the app.** A plugin could declare a victim's
  host and use the game as a request engine. The app's own floors
  prevent it (section 4, "What the app enforces whatever a plugin
  says"):
  - requests only during an operation the player started;
  - one request a second a host at most, one download at a time;
  - the app's budgets.

  So at most one person's browsing speed.
- **The install itself.** Players will install what a stranger links,
  however the game words its warning. The consent page cannot prevent
  that. It can only make what the plugin may do plain, and make "the
  game's maintainer has not checked this" impossible to miss.

### Confinement, applied by the app

The sandbox code moves from `video/helper/src/sandbox/` into a small
crate. The video helper keeps calling it on itself, unchanged. A new
launcher calls it on the plugin's behalf.

- **macOS.** Options:
  - **(a)** `sandbox-exec -f <profile>`, as the SDK does;
  - **(b)** a small launcher of ours that calls `sandbox_init` and then
    executes the plugin. Seatbelt's confinement is inherited across exec.

  **Recommended: (b)**: one code path, and the self-test runs inside the
  launcher, confined, before the exec. The profile is the video helper's
  `(deny default)`, plus what loading a program needs: running and
  reading the plugin's one executable, and reading the system's
  libraries. Exactly what dyld needs is the first thing to check in
  stage 1.
- **Linux.** In `pre_exec`, as the SDK does:
  - `setsid` and `PR_SET_NO_NEW_PRIVS`;
  - `RLIMIT_AS` 2 GiB, `RLIMIT_CPU`, `RLIMIT_NOFILE`, `RLIMIT_FSIZE` 0
    and `RLIMIT_CORE` 0;
  - Landlock allowing only to execute and read the plugin's file and the
    system's library folders;
  - seccomp refusing `socket` for every family (no network whatever the
    kernel's Landlock version) and every call a confined Rust program
    does not need.

  Missing kernel support means the plugin does not run.
- **Windows.** The parent-side token and job object exist
  (`crates/desktop/src/video/helper/windows_spawn.rs`), never run. **Unavailable** until
  checked on Windows.
- **The self-test** is the video helper's escape list (open a file, list
  a folder, reach localhost, bind a socket, start a process), run by the
  launcher after confining and before executing the plugin. Any success:
  no exec, and the plugin is reported unavailable.

### Network, made by the app

The options, for an installed plugin:

- **(A) The app makes every request a plugin describes.** The only way to
  enforce the hosts a manifest declares: no sandbox on any of the three
  systems can limit a program to named hosts (my understanding, to verify
  in stage 1):
  - macOS Seatbelt filters outbound connections by port, by IP literal or
    `localhost`, not by host name;
  - Landlock filters TCP by port only (ABI 4 and later), and seccomp
    cannot read the address a `connect` is given;
  - Windows needs the Filtering Platform, which needs an administrator.

  It also makes plugins sans-IO, testable over recorded answers, as
  `ddi_library::share::upload` already is. Against it:
  - unusual transports do not fit (WebSockets, BitTorrent, request
    signatures with a secret, challenge pages meant for browsers);
  - the app grows an HTTP feature set: redirects checked hop by hop,
    ranges, ETags;
  - pinning resolved addresses with `ureq` 3.5 needs its `Resolver`
    trait, which is in its `unversioned` module, outside its semver
    promise. Either pin and test that, or resolve first and connect to
    the address with the host name for TLS.
- **(B) Each plugin makes its own requests.** For an installed plugin,
  this is a program with the network: it could send anything it was given
  anywhere on port 443. Not acceptable.
- **(C) A broker process.** As (A), with a third process and no gain.

**Recommended: (A), with no exception for any installed plugin.** The
shipped DanOni helper keeps its own network while it is shipped (decision
51); as an installed plugin it would not (section 5).

### Receipts

Each fetched or imported item keeps a record:

- the plugin, its version and its key's fingerprint;
- that a person asked for it (`user-initiated`, as the DanOni receipts
  say, claiming no terms review);
- each request's URL, status, bytes and time;
- the hashes of what was read.

It lives with the item and in the import catalog. It is never sent
anywhere.

### Resource limits, crashes and hangs

- **Memory**: `RLIMIT_AS` 2 GiB on Linux. macOS has no enforced
  equivalent: the app watches the resident size and kills the plugin
  past 2 GiB.
- **CPU**: `RLIMIT_CPU` on Linux, and a CPU budget the app watches on
  macOS. The plugin runs at a lower priority than the game, so a busy
  plugin never takes a play's frames.
- **Time**:
  - an operation with no message, and no host call pending, for 30 s is
    a hang;
  - a search ends within 60 s;
  - a fetch's time is bounded by its budgets;
  - past any of them, the plugin is killed and reaped, and its
    operations fail with `internal`. Three crashes in a run and it is off
    until the next run.
- **No plugin runs during a play**, the play's own fetches excepted: its
  operations wait. A room's stage starts nothing new in a plugin.
- **Pipes**: bounded channels; stderr's last 32 KiB kept for Details.
- **Exit**: Shutdown, 5 s, then kill, never blocking the UI.

### Installing

**Where plugins live**: the data folder's `plugins/<id>/`. The app
copies an installed package there and runs only that copy. The player's
downloaded file is never run.

**A plugin package** is a zip (`.ddisource`) or a folder:

- `source.toml`, the manifest (section 4): its id, name, publisher,
  version, the source's page, the protocol versions, capabilities,
  inputs, declared hosts, room rule, attribution;
- `bin/<target>/<executable>`, one per platform it supports
  (`aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, …): the app takes
  its own platform's, and refuses the package where there is none;
- its licences and notices;
- `source.sig`: an ed25519 signature (the SDK's `sign` command makes one)
  over an inventory of every file's path, size and SHA-256, in the shape
  of the DanOni bundle's manifest.

**How the player adds one**: Options → Sources → "Add a source…", which
opens the system's file dialog on a package file or folder they already
have.

- **Version 1: never a URL** the game downloads from, and no "install"
  links. A one-click install would be easier, but:
  - it makes a link in a chat, a room or a web page a way to put a
    program on someone's computer in two presses;
  - the game becomes a downloader of executables;
  - it skips the browser's own warnings and the system's marks on
    downloaded files.

  A file the player saved and then picks takes a deliberate step.
  Recommended: file only (open question 11).

**Before it is enabled**, the consent page, in the game's own words, with
the plugin's own text only where it is the plugin's (its name and
publisher, as data):

- "{name}, version {version}, from {publisher} (as the plugin says). The
  game's maintainer has not checked this program."
- "It runs in a sandbox: it cannot read your files, reach the internet
  or start other programs by itself."
- "Through the game, it can reach: {hosts}, one request a second each at
  most, and only while you use it."
- Or: "It can read the folders you choose for it. It cannot reach the
  internet."
- "Songs it finds can be imported, played, {added to online rooms | not
  added to online rooms}."
- Its key's fingerprint, and the package's digest, for comparing with
  what its publisher states.
- Enable, or Cancel. A plugin is off until enabled here.

**Integrity**:

- The digest shows the package was not changed since it was signed.
- The signature, required, ties every later version to the first one's
  key: the key is **pinned at first install**, per plugin id.
- An update signed by another key is not an update. The page says the
  key changed, and the player must remove the old plugin first.
- What a signature does not say: who the person is, or that the program
  is harmless.

**No registry or store run by the maintainer**, which is out of scope:

- It would make the maintainer the one vouching for programs, or seem to.
- It would mean moderating them, and being asked to delist and to judge
  which sites a plugin may read.
- A listing of integrations with sites of commercial content is a
  liability the research (round SR) already flagged for rooms.
- It costs hosting, and it contradicts "no plugins by default".

Publishers distribute their own, from their own repositories.

**Updates**: the player adds the new package the same way.

- Same id, the pinned key, a higher version: the consent page shows what
  changed in its declarations (new hosts, new capabilities, a changed
  room rule), and an update that widens any of them must be accepted
  again.
- No automatic updates, and no checking for updates in version 1: that
  would be the game reaching the publisher on its own.

**Removal**: Options → Sources → the plugin → Remove. Its folder, its
cache store and its pinned key are deleted. Songs already imported
stay: they are files in the import store, with their provenance.

**Enable switch**: each plugin can be turned off without removing it.
Off, it is never started.

**The systems' own gates** (my understanding; each to verify in stage 5
on the system itself):

- **macOS.**
  - A file a browser downloads carries the quarantine mark. Gatekeeper
    checks a marked program when it is started, and refuses one that
    Apple has not notarized; System Settings then offers Open Anyway.
    Whether this applies to a program another program starts, as here,
    must be checked.
  - Apple silicon also refuses arm64 code with no signature at all. The
    Rust toolchain's linker signs ad hoc by default, which is enough for
    that rule.
  - The game itself is signed ad hoc and not notarized
    (`docs/plans/packaging.md`).
  - Two ways:
    - **(a)** Keep the mark on the installed copy. Every plugin author
      must then notarize, which takes Apple's paid developer program, a
      yearly fee: a heavy price for a hobby site's maintainers.
    - **(b)** Install the copy without the mark: the app writes the
      file's bytes, not its attributes. Gatekeeper then never checks it.
      The protection is the app's confinement, which for this purpose is
      stronger than Gatekeeper's check of the publisher. The consent page
      says the game has not checked it.

    **Recommended: (b)**, saying so plainly, since the game steps around
    a system check on purpose (open question 13).
- **Windows.**
  - A downloaded file carries the Mark of the Web. SmartScreen asks when
    the player opens such a file from Explorer, as far as I know not when
    another program starts it.
  - Smart App Control on Windows 11, when on, blocks unsigned or unknown
    programs however they are started. Authors would then need
    Authenticode signing, which takes a paid certificate.
  - Moot until confinement is verified there: plugins are unavailable.
- **Linux.**
  - No system gate. The app sets the executable bit on its copy.
  - Authors build for a stated baseline (static, or an old enough
    glibc). Landlock must allow reading the system's library folders
    for a dynamically linked plugin.
  - A kernel without the needed Landlock or seccomp support: unavailable.

### Sign-in to a source (not in version 1)

The plugin never sees a site's credentials:

- the manifest declares a scheme (an OAuth flow in the system browser,
  opened by the app as desktop's own sign-in is, `crate::os::open_url`,
  or a device code);
- the app keeps the token, per plugin, apart from the game's own session;
- the app attaches it to that plugin's requests to its declared hosts;
- the plugin learns only "signed in as <name>".

It waits for a site that needs it.

### What this forecloses

- Transports other than HTTPS request and response.
- A plugin keeping a secret of its own: a key a site issues is visible to
  anyone holding the package, as with any desktop client.
- Plugins on Windows, until confinement is verified there.
- Installing from a link.

## 4. Per-source rules, and who decides them now

### The manifest

A plugin's manifest declares its source's rules:

```toml
id = "example-songs"                 # stable; [a-z0-9-], at most 32
name = { en = "Example Songs", ja = "…" }
publisher = "…"                      # as the plugin states it
version = "1.2.0"
homepage = "https://…"               # the source's page
protocol = [1]
capabilities = ["search", "filters", "metadata", "fetch"]
inputs = []                          # or ["folder"], ["url"], ["link"], ["text"]

[network]                            # absent: none
hosts = ["example.org", "files.example.org"]  # exact names; https only
min_interval_ms = 2000               # per host; the app's floor still applies
max_requests_per_operation = 10
max_item_bytes = 2_000_000_000
max_file_bytes = 512_000_000

[files]                              # absent: none (version 1: not with [network])
read = "input-folder"

[uses]
play = true
import = true
rooms = "upload"                     # "upload" (through our server) or "none"; absent: "none"

[attribution]
required = true
link_back = true
```

### Who decides now

- With no review by the game's maintainer, **a plugin declares its own
  rules**. The app enforces exactly what is declared, and its own floors
  on top.
- **Nothing vouches that a plugin is sanctioned by the site it reads.**
  A plugin named for a site may have been written by anyone. The game
  cannot know, and says so: "from {publisher}, as the plugin says", and
  "not checked by the game's maintainer".
- The research's principle (an integration is for a site's maintainers to
  write or sanction) becomes a principle for **plugin authors**, said in
  the SDK's documentation and the template. The game enforces what it
  can.
- The terms of use being drafted (round TU) put the responsibility for
  what a person uploads on that person. The room's upload notice shows
  for a fetched song as for any other.

### What the app enforces whatever a plugin says

- **Requests only during an operation the player started.** No
  background requests, no preloading through a plugin. A room's
  preloading reads our server's share, never the source.
- **Per-host pacing**:
  - one request a second at most, whatever the manifest asks (a
    manifest may ask for slower);
  - one download at a time a host;
  - `429` and `Retry-After` honored.
- **Budgets**: requests an operation, bytes an item and a file (at most
  the share's: 512 MB a file, 2 GB an item), time.
- **Public addresses only**, each redirect hop checked again. No IP
  literals, no `localhost`, https only.
- **robots.txt for search and browse requests.**
  - The app reads a host's robots.txt (cached for the run) before the
    first `Http` call of a search or browse to it, under the token
    `DitoDitoInfinito`, and refuses what it disallows.
  - A `Download` of an item the player chose is not checked, as a single
    work the player asked for is not in the DanOni importer (decision
    51).
  - This gives a site's operators a lever that needs neither the plugin
    author nor the game's maintainer: a site that disallows automated
    reading refuses every plugin's search.
- **A deny list in the game**, data updated with releases, of hosts no
  plugin may declare:
  - the game's own server (its API and share hosts);
  - any host whose operators ask the game's maintainer to be listed.

  Recommended: the list starts with the game's own hosts and grows only
  on request (open question 9).
- **A room's rule as declared**; absent, `none`. It never applies to
  the player's own files from elsewhere: a level in a folder of theirs
  can be added as today (the maintainer's "should be possible too").
  An item's metadata may narrow the rule, never widen it.
- **The User-Agent**, `DitoDitoInfinito/<version> (source <plugin id>)`,
  so a site sees and can limit the traffic.

### Naming a source in the screens

- A plugin's name and publisher are its own, shown as data in our frame
  ("From {source}").
- No logos.
- The built-in source's name is ours ("This computer").
- UI text follows AGENTS.md: a site is named only as the source a plugin
  says it is, and nothing implies the game's affiliation with it.

## 5. Built-in, examples, and the DanOni importer

### Searching a folder on this computer: built in

Is a folder search worth being a plugin for players, now that none
ships? Weighed:

- **As a plugin only**: every player who wants to search a USB stick or
  a downloads folder must first find, download and install a program, to
  do what the game can do with its own scanner. A basic feature behind a
  trust decision.
- **Built in**: the app implements the same `Source` interface in
  process ("This computer"). It is listed first in Find songs, with no
  install and no sandbox needed: it is the game's own code reading the
  player's own files, as the library already does.
  - Search and filters: title, artist, pack, style, difficulty, level
    range, BPM range and length range.
  - Browse by pack; `song_key` before any fetch; delivery `in_place`;
    all three uses.
- **Recommended: built in**, and **also an example plugin** in the SDK's
  repository: the same search over host-call file handles, which is the
  host's test of folder scopes and the authors' model for a source that
  reads folders.

How it differs from adding a folder in Options: a song folder becomes the
library's (listed, scanned at every start, cached). This looks into a
folder without making it the library's: find one song in a downloads
folder, try a pack once, add a friend's song to a room, keep only what is
imported.

### The openly licensed catalog

The earlier draft's catalog (our own index of CC BY and CC BY-SA songs
not bundled, the files in a release of our own repository) has three
fates now:

- **(a) built into the app** as a second built-in source, our own
  content like the bundled songs, reading only our own hosts;
- **(b) an installable plugin** the maintainer publishes separately;
- **(c) dropped.**

(a) is not a plugin, so "no plugins by default" allows it, but it is a
feature of its own: an index, a hosting decision, provenance work. (b)
asks players to trust a program for what the game could do itself.

**Recommended: (c) for now**, revisited as (a) if more free songs in the
game are wanted. The network example authors need is a small fixture
catalog in the SDK's repository (a local index served by the harness),
which exercises search, filters, metadata, `Http`, `Download` and hashes
without any real site.

### The game's own shares

Unchanged (share by link, a room's uploads): one contract on both shells,
the web included. So **upload is not in version 1**, and is reserved in
the protocol:

- `Upload { op, item_files, metadata }`: the app gives the song's files
  as the share gathers them (`stored_files`), its entry, and what the
  player typed for that site; never other files, the history, the
  settings, any session or token;
- the app performs the requests;
- it needs sign-in, which waits for a site that asks.

### Where the examples, the SDK and the harness live

- **(a)** This repository's `examples/` or `tools/`, built in CI as tests
  of the host, never packaged.
- **(b)** A separate repository (for example `sugoijan/ddi-sources`):
  the SDK, the harness, the examples and the template, MIT, consuming
  the protocol crate as SkinCraft consumes the game's crates.

**Recommended: (b)**:

- authors clone a small repository rather than the game's;
- its release cadence is its own;
- it is the shape every third-party plugin will have;
- the game's repository stays free of plugin examples that could be
  mistaken for shipped ones.

This repository keeps:

- `ddi-source-protocol`, published to crates.io from here, since the
  host depends on it;
- the host and the launcher;
- fake plugins in its own tests: synthetic programs, as the importer
  client's tests already use shell-script helpers;
- `--source-check <package>`, a headless check of a package against the
  real host, as `--danoni-importer-check` checks the shipped helper.

**The SDK** (`ddi-source-sdk`):

- a `Source` trait whose methods each default to "unsupported", so a
  plugin implements only its capabilities;
- `serve()`: framing, Hello, host calls, cancellation, progress;
- mappings from common style and difficulty names to our layout ids and
  difficulties;
- `sign` and `package` commands.

The built-in folder source implements the same trait in process.

**The harness**: a fake host (folder fixtures, recorded HTTP, a fake
site) and a conformance check:

- the manifest well formed, and Hello a subset of it;
- each declared capability answering;
- limits honored, and cancellation within 5 s;
- values where values belong.

**Other languages**: the protocol alone, documented with JSON examples
from the SDK's tests.

### The DanOni importer: shipped now, an installed plugin later

**Now**: unchanged:

- the `danoni-importer` default feature;
- the helper and worker built by `crates/desktop/build.rs` and packaged
  by `tools/package/package.py`;
- version 1 of `ddi-importer-protocol`, and the Add songs URL tab;
- `--danoni-bundle-info` and `--danoni-importer-check`;
- CI's `danoni.yml`.

Find songs may show it as a source through an adapter in the app over
its existing client (`danoni::Imports`); that is optional, and changes
nothing of it.

**The move, when it comes**: the importer becomes an installed plugin in
its own repository, speaking the source protocol. The interface does not
change for it; the host must first offer what the importer needs (below).

What this repository would lose:

- `tools/danoni-dump/`: the SDK, the Ruffle worker's build inputs, the
  desktop probe and the pack verifier;
- `tools/danoni-importer/`: the helper, `build.py`, `notices.py` and the
  licences;
- the root workspace members on Rust 1.99, and the `importer` profile;
- the shared lock's part of those dependencies;
- `crates/desktop/build.rs`'s preparation of the bundle, and the
  `DDI_DANONI_BUNDLE_*` variables;
- `cfg(danoni_importer)` and the `danoni-importer` feature, and with
  them the default feature list and `DDI_IMPORTER_TEST_ONLY`;
- the app side: `crates/desktop/src/danoni.rs`, `danoni/bundle.rs`,
  `client.rs` and `report.rs`, and the managed adapter
  `library/managed.rs`;
- `crates/importer-protocol`;
- package.py's `danoni-importer/` copy and audit;
- `--danoni-bundle-info` and `--danoni-importer-check`;
- CI's `danoni.yml`;
- the helper's notices in the game's credits;
- the Add songs URL tab;
- the docs: `danoni-importer.md` and `danoni-import.md`, kept as
  history, plus the research notes.

What it would keep:

- the library's reading of DanOni works and SSCs (`ddi_library::works`,
  `danoni`);
- the danoni layouts and skins;
- the play of imported DanOni songs.

The other repository would take:

- the probe and verifier, depending on the game's crates by Git as
  SkinCraft does;
- its own CI and its own notices;
- its own release builds (macOS arm64, x86_64 Linux; the worker's Vulkan
  and Mesa needs on Linux);
- its signing key.

What the host must offer first, so that the move is not forced early:

- **Network through host calls.** The SDK fetches with its own `reqwest`
  and `hickory` stack, its cache, receipts and pacing. As an installed
  plugin it gets no network: its transport (`network/transport.rs`,
  `HttpTransport`) needs a variant over `Http` and `Download` host
  calls. Its pacing and receipts can stay in the SDK; the app's floors
  apply on top.
- **A private folder.** The SDK's managed store, staging, evidence and
  caches are files. The host would grant each plugin one private folder
  (in the cache folder for the SDK's caches; a data folder only if
  imports stayed in its store), enforced by Seatbelt and Landlock as
  read and write there only. Or the SDK keeps its state in host calls.
  Recommended: the private folder, a generic grant any plugin may
  declare, shown on the consent page.
- **Children from its own package.** The Ruffle worker is a second
  program, confined by the SDK's supervisor. Under the launcher's
  confinement the plugin may not start processes. The host would allow
  a plugin to start only executables inside its own package, confined
  at least as tightly (Seatbelt nests; Landlock and seccomp are
  inherited). The SDK's own `sandbox-exec` and `pre_exec` steps then add
  their own confinement on top.
- **Imports through the app.** As an installed plugin, the importer
  hands prepared songs (its SSC, Opus and art) to the app:
  - for play and rooms, into memory, which is the "prepared" delivery
    in place of today's managed store;
  - for imports, into the app's import store.

  Its managed store then retires.
- **The players' existing imports**, in `data/danoni/library`:
  - the plugin's first run hands the app its verified catalog;
  - the app copies those songs into the import store, keeping the SDK's
    ids so that bests, offsets and hidden songs carry over;
  - a release in between must keep reading the old store, or the songs
    vanish until the plugin is installed.

  This is the move's hardest part, to design then.

What players who use it would do: download the importer's package for
their system from its repository's releases, add it in Options →
Sources, and enable it. Their imported songs would be migrated on its
first run. Players on Windows have no importer today, and still would
not.

Before moving, all of this should be true:

- the host has run installed plugins on macOS and Linux for a release or
  more;
- the private-folder and own-children grants are built and checked by
  the self-test;
- the migration has been tested on real stores;
- the other repository builds, signs and releases.

## 6. In the screens

All of this is desktop only.

**Find songs**

- Opened from song select's More, from Add songs, and, in an online
  room, from My songs' More ("Find songs to add").
- One of our theme's screens, with song select's bars.
- The sources on the left: "This computer" first, then the enabled
  installed plugins, each marked as installed ("from {publisher}, as it
  says").
- The search field and the source's filters at the top (a fold on a
  narrow window), the results, and the item's side: banner, charts,
  size, licence and attribution as the source states them, "Open the
  source's page", and what it allows.
- **Actions**: Play, Import, and, in a room, Add to room (where the
  source's rule and the room's own rules allow). A pack offers Import and
  Add pack to room within `add_pack`'s bounds.
- **Progress**: worded from the step values. Cancel in the item's
  actions.
- **Errors**: our sentence from the code; a Details fold with the
  plugin's detail and stderr tail.
- **Fetched this session**: a pack at the top of Find songs and of the
  room's My songs, so the room's `set_library` sees fetched songs.
  Rows are marked as not in the library.
- **In a room**: a fetched song added to the room reads from memory for
  its upload, and is held while its upload is pending or failed. Members
  see an ordinary room song.

**Options → Sources** (a page of its own; the settings revamp places it):

- the installed plugins, each with its enable switch, version,
  publisher (as stated) and fingerprint;
- Details (the consent page again), Remove;
- "Add a source…";
- on Windows, or where the self-test fails: "Sources from other
  programs aren't available on this computer: the game can't run them
  in a sandbox here." This Computer still works.

**Options → Data**: "Fetched songs" (the memory budget), "Fetch movies
when playing", "Clear now".

**Basic and Advanced** (PLAN.md section 5, item 7): Find songs and
This Computer are Basic. Options → Sources may be Advanced.

**The web**: nothing in version 1.

**Text**: messages in `locales/en/` and `locales/ja/` (`sources.ftl`),
joining the `qps-ploc` walk. Plugin text never enters them.

## 7. What it does not do

- **The web.** No plugins, no sources, no proxy through our server.
- **Ship any plugin.** The game's packages contain none. The DanOni
  importer stays for now, as it is, until it moves.
- **Run a plugin unconfined**, on any system (recommended).
- **Vouch for a plugin.** The game enforces what a plugin declares and
  its own floors; it cannot know who wrote it or whether a site
  sanctioned it, and says so.
- **Scrape through the app.** Search and browse requests honor robots.txt;
  every request is paced and budgeted; nothing is fetched without a
  person's press.
- **Run a store or registry of plugins.**
- **Make native code safe.** Confinement lowers what a plugin can do; a
  sandbox escape remains possible.
- **Rights decisions.** Terms of use, the upload notice and takedown are
  round TU's and the maintainer's.

## 8. Stages

Each is useful and testable alone.

| Stage | What | Needs decided | Size | Tested without network |
|---|---|---|---|---|
| 1 | `ddi-source-protocol`; the sandbox in a shared crate (video helper unchanged); the launcher confining a plugin from outside (macOS `sandbox_init` then exec; Linux `pre_exec`), its self-test; the host: spawn, framing, folder handles (`List`, `Stat`, `Read`), `Write` into memory, `CacheGet` and `CachePut`, limits, kill and restart; synthetic plugins in tests, one of them trying every escape. | Questions 1, 2, 3. | Medium. | Yes; the self-test on macOS and Linux in CI. |
| 2 | "This computer" built in, behind the in-process `Source` trait; Find songs in a first form; fetched songs in memory (`MemReader`, `Files::Memory`, `mem:` pictures, the preview from bytes, the budget, the cache for what memory cannot hold); play without importing; the import store and its catalog. No plugin involved. | Questions 4, 5, 6, 7. | Medium to large. | Yes: folders written by tests, offscreen plays. |
| 3 | Add to a room without importing; the room rule. No room model change. | Question 8. | Small to medium. | Yes: `FakeServer`, the desktop room tests, a local server and bots. |
| 4 | The app's HTTP for plugins: public addresses, the allowlist, redirects, pacing floors, robots.txt for search and browse, the deny list, budgets, receipts, User-Agent. | Question 9. | Medium. | Yes: a local fixture, loopback allowed only under a test switch. |
| 5 | Installing: the package, signatures and the pinned key, Options → Sources, the consent page, enable, update, remove; `--source-check`; the systems' gates checked on macOS and Linux. | Questions 10 to 13. | Medium. | Yes: packages made by tests. |
| 6 | The SDK's repository: SDK, harness, the folder and fixture-catalog examples, the template; the protocol crate published. | Question 14. | Medium. | Yes. |
| 7 | Find songs complete: filters from schemas, browse, the item's side, the room's Find songs to add. | | Medium. | Yes: offscreen pictures. |
| Later | The DanOni importer as an installed plugin (its preconditions, section 5); Windows once confinement is verified there; sign-in; upload; the catalog if wanted. | Questions 15 to 17. | Large (DanOni). | |

What changes along the way:

- **Desktop's library**: a fourth set (imports) and a fourth registry
  (fetched songs). Folder songs keep winning collisions.
  `library-v1.json` is untouched.
- **Desktop's play loading**: `Files::Memory`, `mem:` pictures, the
  preview from bytes.
- **The history**: plays of `src:` songs are kept. Their replays work
  while the item is held.
- **The room's My songs**: a "Fetched this session" pack. No room model
  revision.
- **The video helper**: only where its sandbox code lives.
- **The build and packaging**: the launcher is the one new executable
  shipped (beside the video helper). No plugin is built into a package.

## 9. Open questions for the maintainer

**Decided** (2026-11-25):

- "I think we shouldn't ship any plugins by default. Even DanOni SDK I'm
  leaning more towards not including it and maybe even moving to another
  repo (but not now)." The game ships no plugin. DanOni stays shipped
  for now; its move is a later step (section 5).
- "About songs that shouldn't be imported, we should prioritize keeping
  them in memory only and using known cache dirs when that's not
  possible/feasible." Fetched songs not imported live in memory, with
  the cache only where memory will not do (section 1).

**Open**, in the order they block work:

1. **May an installed plugin ever run unconfined?** (Blocks stage 1.)
   - **Recommended: never.** Where the launcher's self-test cannot pass
     (Windows today, a kernel without the needed support), plugins are
     unavailable and the screens say so.
2. **Does the app make all of an installed plugin's I/O?** (Blocks stages
   1 and 4.)
   - **Recommended: yes, with no exception.** It is the only way to
     enforce declared hosts; the price is HTTPS request and response
     only.
3. **The protocol.** (Blocks stage 1.)
   - **Recommended**: a new crate, `ddi-source-protocol`, published to
     crates.io; the importer protocol untouched while the DanOni helper
     is shipped.
4. **Folder search.** (Blocks stage 2.)
   - **Recommended**: built in ("This computer"), and also an example
     plugin in the SDK's repository.
5. **Where Find songs lives.** (Blocks stage 2.)
   - **Recommended**: a screen of its own, reached from song select, Add
     songs and a room's My songs.
6. **The import store** for sources other than DanOni. (Blocks stage 2.)
   - **Recommended**: an app-owned folder store with a provenance catalog.
     DanOni's managed store is unchanged until it moves.
7. **Memory's thresholds, and what forces the cache.** (Blocks stage 2.)
   - **Recommended**:
     - 64 MiB a song;
     - 256 MiB for everything held;
     - items nothing holds are dropped, held items spill to the cache;
     - movies only in the cache, fetched when a play loads, with a
       switch;
     - the cache 2 GiB for the run;
     - a room's song released to the room's share under pressure once
       uploaded.
   - Alternatives are in section 1.
8. **Room rules.** (Blocks stage 3.)
   - **Recommended**: as the manifest declares, `none` when absent; the
     upload notice of round TU for every fetched song; never applied to
     the player's own files; "This computer" allows rooms.
9. **The app's floors.** (Blocks stage 4.)
   - **Recommended**:
     - one request a second a host at most, one download at a time;
     - robots.txt honored for search and browse, not for a chosen item's
       download;
     - a deny list that starts with the game's own hosts and grows only
       when a site's operators ask.
10. **Folder reading and network in one plugin.** (Blocks stage 5.)
    - **Recommended**: refused in version 1. Together they would let a
      plugin send a folder's contents to its hosts.
11. **How a plugin is added.** (Blocks stage 5.)
    - **Recommended**: a file the player picked in Options only. No
      install links or URLs in version 1.
12. **Integrity.** (Blocks stage 5.)
    - **Recommended**: signatures required, the key pinned at first
      install, an update widening declarations accepted again.
    - No registry or store run by you (out of scope, section 3).
13. **macOS's quarantine mark.** (Blocks stage 5.)
    - **Recommended**: install the copy without it, saying the game has
      not checked the program. The alternative requires every author to
      notarize, through Apple's paid program.
14. **Where the SDK, harness and examples live.** (Blocks stage 6.)
    - **Recommended**: a separate repository; this one keeps the
      protocol, the host, the launcher and synthetic test plugins.
15. **The DanOni importer's move.** Not blocking.
    - **Recommended**: not before the preconditions of section 5 hold:
      installed plugins proven for a release or more, the private-folder
      and own-children grants, and a tested migration of existing imports.
16. **The openly licensed catalog.** Not blocking.
    - **Recommended**: dropped for now; revisited as a built-in source if
      more free songs in the game are wanted.
17. **Later**: sign-in and upload (for the first site that asks);
    Windows (once its confinement is verified); the web (nothing); a
    source's name carried into a room's list (a room model revision).
