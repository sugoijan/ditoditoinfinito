# Song sources: native plugins for the desktop app (2026-11-25)

Status: a design, nothing built. One principle is decided (the
maintainer, 2026-11-25: songs not imported stay in memory, section 1).
The maintainer decides the open questions at the end; until then nothing
else here is decided beyond their own words below.

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
- FFR levels a player has are not to be blocked from rooms ("if the
  author that uploads to FFR also wants to upload the unconverted level
  here, it should be possible too"). Rooms and shares are private and
  temporary; terms of use, an upload notice and a takedown contact are
  drafted elsewhere (round TU), not here.

And from the research of round SR (four community sites read on
2026-11-25; the coordinator's notes): the game scrapes no one. An
integration with a site is for that site's maintainers to write or to
sanction. The design below makes that the natural way to add a source,
and the only one.

## In short

- A **source plugin** is a native program shipped beside the desktop app,
  run in a process of its own, speaking a versioned framed protocol over
  anonymous pipes, as the DanOni importer does today.
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
- Recommended for version 1: plugins **shipped with the game only**,
  **confined** with the video helper's sandbox, and **the app doing every
  network request and every file read** a plugin describes (a plugin with
  no I/O of its own). The DanOni importer is the one reviewed exception
  that keeps its own network, and keeps working as shipped throughout.
- Reference implementations, in order: **a local folder source** (no
  network; every capability but sign-in and upload; all three uses), **the
  DanOni importer as a source** (first through an adapter, the helper
  unchanged), **a catalog of openly licensed songs** (network search and
  fetch over content anyone may redistribute, rooms included), then **a
  template and an SDK** for a site's maintainers. The game's own shares
  stay as they are.

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

That last pattern is the model for plugins below.

## 1. The model

### Sources, items, capabilities

A **source** is one plugin, as the player sees it: "Folder on this
computer", a catalog, a site. It offers **items**. An item is a song or a
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

The DanOni plugin keeps its managed store and its own ids.

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
  touched. The DanOni helper keeps speaking it, through an adapter in the
  app (section 5). Later the helper may speak both, choosing by Hello.
  Version 1 of the importer protocol retires once no shipped helper
  speaks it; since the helper and the app are one build, that is one
  commit.

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
  the verified bundle before starting it. More than the manifest is a
  protocol error, never a widening.
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
- A shipped plugin's English and Japanese labels are reviewed with the
  plugin, but stay the plugin's data: they never enter `locales/`.

## 3. Trust, confinement and distribution

### What a native plugin is

A native plugin is a program running on the player's computer. Without
confinement it can do anything the player can:

- read their files: the game's config folder, where the online session is
  kept (`online-session`), their browser profiles, their keys;
- make network requests to anywhere;
- start other programs, and stay behind after the game closes.

A digest compiled into the app says the bundle is the one this build of
the game was made with. A signature says who made it. **Neither says it
is harmless.** Confinement lowers what a plugin can do, but sandboxes
have holes:

- macOS's `sandbox_init` is deprecated, although Chromium's helpers use
  it;
- a kernel bug is an escape;
- the Windows path has never run on Windows.

Nothing below makes native code a player downloaded safe.

### Who ships plugins

1. **Shipped with the game** (as the DanOni helper is). Built and
   reviewed in this repository, or pinned from another one by commit and
   built from source here, as the Ruffle worker is. Each plugin is a
   bundle `sources/<plugin>/<content id>/` (a manifest, `bin/`,
   notices), with its content id and manifest digest compiled into the
   app, verified before every start by the code of `danoni/bundle.rs`,
   made general. A site's maintainers write the plugin (or sanction ours),
   and the game's maintainer accepts it as any change.
   - Cost: a site's change waits for a release of the game, and every
     plugin's dependencies' licences pass `gen-credits` and the
     importer's notice checks.
   - Gain: everything a player runs has been read.
2. **Installed by the player**: a folder of the data directory holding
   bundles the player put there, pinned by a digest the player approves,
   or signed by a key the game trusts. The game would show a warning that
   says what is true ("This runs a program on your computer"), and none
   of it makes the program safe.
   - Cost: the game becomes a distribution channel for programs it has
     not read. Players will install what a stranger links. Every hole in
     the sandbox becomes the game's problem. Support turns into "which
     plugin did you install".
   - Gain: sites ship on their own schedule.

### Confinement

For a plugin whose only I/O is host calls (section 2), the video
helper's sandbox fits as it is: no files, no network, no other
processes; pipes, memory and threads only, checked by the same self-test
before it serves. A plugin that fails its self-test does not run.

- The sandbox code moves from `video/helper/src/sandbox/` into a small
  crate both helpers use. The video helper's profile and allowlist do not
  change.
- A plugin's seccomp allowlist may need a few calls more than FFmpeg's
  (a Rust program's thread and allocator calls are much the same). To
  check in stage 1.

What a plugin that **makes its own requests** would need, against what
the systems can enforce (my understanding, to verify in stage 1 before
anything relies on it):

- **macOS Seatbelt**: outbound network filtered by port and by IP
  literal, or `localhost`. Not by host name.
- **Linux Landlock**: since ABI 4, TCP connect and bind filtered by port
  only. seccomp cannot read a `connect` call's address.
- **Windows**: a restricted token blocks no network by itself. Filtering
  by destination takes the Windows Filtering Platform, which needs an
  administrator.

So **"outbound HTTPS to declared hosts only" cannot be enforced by any of
the three sandboxes**: only "port 443 anywhere" can. A plugin with its
own network could send what it read anywhere on port 443.

Files are the easier part. A folder the player chose can be allowed
read-only by Seatbelt (a profile string built at run time) and by
Landlock. But the app serving reads, as the game does for the video
helper, needs no rule at all and works the same on every system.

### Who makes the network requests

- **(A) The app makes every request a plugin describes.** The plugin has
  no network and is confined totally.
  - The host allowlist is enforced where it can be: in the app, against
    the manifest the maintainer accepted.
  - The DanOni discipline is applied in one place: public addresses only
    (each hop of a redirect checked again); per-host pacing (the
    manifest's interval, never under 1 s, one download at a time a host);
    `429` and `Retry-After` honored; request, byte and time budgets;
    cancellation; a User-Agent naming the game and the plugin
    (`DitoDitoInfinito/<version> (source <plugin id>)`), so a site can
    see and limit us.
  - The player can be shown what was requested.
  - A future sign-in token can be attached by the app and never seen by
    the plugin.
  - The plugin's logic becomes sans-IO, testable over recorded answers,
    as `ddi_library::share::upload` already is (`contract::Upload`'s
    steps, which both shells drive).
  - Against it: unusual transports do not fit. WebSockets, BitTorrent (EO
    offers magnet links beside its zips), HTTP signatures computed with a
    secret, and challenge pages meant for browsers. The app grows an HTTP
    feature set (redirects, ranges, ETags, resumes).
  - Against it: the app's HTTP is `ureq` 3.5. Pinning resolved addresses
    needs its `Resolver` trait, which sits in ureq's `unversioned` module,
    outside its semver promise. Either accept that (pin the version and
    test it) or resolve first and connect to the address with the host
    name for TLS.
  - Against it: the DanOni SDK's mature network stack (cache, receipts,
    replay) is not reused. The app does not link the SDK, deliberately.
- **(B) Each plugin makes its own requests.** Simple for plugin authors,
  and any transport works. But the allowlist is not enforceable (above):
  confinement shrinks to "files", and the game's promise never to read a
  site that did not sanction it becomes the plugin's promise.
- **(C) A separate network broker process.** As (A), outside the game's
  process. It adds a third process for no gain over (A), since the game
  already makes HTTP requests (shares, rooms, sign-in).

**Recommended: (A) for every new plugin. The DanOni helper keeps its own
network as the one reviewed exception**, as shipped. Its requests already
carry the SDK's discipline and receipts. It takes any DanOni work URL on
any host because a person asked for that one work (decision 51); no new
plugin gets that shape.

### Receipts

Each fetched or imported item keeps a record:

- the plugin and its version;
- that a person asked for it (`user-initiated`, as the DanOni receipts
  say, without claiming any terms review);
- each request's URL, status, bytes and time;
- the hashes of the files written.

It lives with the item and in the import catalog. It is never sent
anywhere.

### Resource limits, crashes and hangs

- **Memory**: `RLIMIT_AS` 2 GiB on Linux, as the video helper; a job
  object's memory limit on Windows. macOS has no enforced equivalent: the
  app watches the plugin's resident size and kills it past 2 GiB.
- **Time**:
  - an operation with no message, and no host call pending, for 30 s is
    a hang;
  - a search ends within 60 s;
  - a fetch's time budget is the manifest's, beside its byte budgets;
  - past any of them, the plugin is killed and reaped. Every operation
    then fails with `internal`, and the next use starts it again.
  - Three crashes in a run, and the source is off for the rest of it
    ("This source stopped working. It will be back next time you start
    the game.").
- **Pipes**: bounded channels both ways, as the importer's client has;
  stderr's last 32 KiB kept for Details.
- **Exit**: Shutdown, 5 s, then kill, never blocking the UI, as
  `danoni::Imports::shutdown` and `drained` do today.

### Updates

- Shipped plugins change with the game: a new content id.
- Player-installed plugins (if ever) would have no automatic update in a
  first version: the player replaces the folder.
- A site's API that changes under a shipped plugin makes its operations
  fail with values (`source_changed`, `unreadable_content`) until the
  next release.

### Version 1, recommended

- Shipped plugins only.
- Confined with the video helper's sandbox and its self-test.
- Every request and every file read made by the app.
- The DanOni helper as it is.
- macOS and Linux first. On Windows, sources wait until the restricted
  token is checked on Windows, as the video helper's is still to be (open
  question 3).

What it forecloses:

- a site shipping or fixing its plugin on its own schedule;
- plugins in languages our CI does not build (a plugin pinned from
  another repository must build from source here);
- transports other than HTTPS request and response (no torrents, no
  sockets);
- a plugin keeping a secret of its own (an API key). A site that wants a
  key per client issues one to the game; the manifest holds the key's
  name; the app attaches it; and a key compiled into an open-source game
  is public, which the site must accept.

None of these is closed forever: section 9 says what would open each.

### Sign-in to a source (not in version 1)

The plugin never sees a site's credentials:

- The manifest declares a scheme, such as an OAuth flow in the system
  browser (the app opens it, as desktop's own sign-in does, through
  `crate::os::open_url`) or a device code.
- The app keeps the token in the config folder, per plugin, apart from
  the game's own session.
- The app attaches it to that plugin's requests to the hosts the
  manifest names. The plugin learns only "signed in as <name>".

This waits for a site that needs it and asks for it.

## 4. Per-source rules

### The manifest

A plugin's manifest is the source's rules. It is data in the bundle,
covered by the compiled digest, and the app enforces it:

```toml
id = "open-catalog"                  # stable; [a-z0-9-], at most 32
name = { en = "Open songs", ja = "…" }
kind = "first-party"                 # or "site": the site's own integration
operator = "…"                       # who runs the source; shown in its details
homepage = "https://…"
protocol = [1]
capabilities = ["search", "filters", "metadata", "fetch"]
inputs = []                          # or ["folder"], ["url"], ["link"], ["text"]

[network]                            # absent: none
hosts = ["example.org", "files.example.org"]  # exact names; https only
min_interval_ms = 1000               # per host, never under the app's 1 s
max_requests_per_operation = 10
max_item_bytes = 2_000_000_000
max_file_bytes = 512_000_000

[files]                              # absent: none
read = "input-folder"                # folder scopes the player chose

[uses]
play = true
import = true
rooms = "upload"                     # "upload" (through our server) or "none"

[attribution]
required = true                      # shown with the song wherever it is shown
link_back = true                     # the item's page shown with the song

[sanction]                           # for kind = "site"
by = "…"                             # the site's maintainers, by name
record = "…"                         # where their yes is recorded (an issue, a mail summarized)
date = "2026-…"
```

### Room rules

- A source's room rule applies to the **songs it fetched**: transient,
  and imported (through the import catalog's provenance).
- An item's metadata may narrow it (a site with mixed content may mark an
  item "not for rooms"), never widen it.
- `rooms = "none"` means Add to room is not offered for them, and the
  room's My songs says why on such a row.
- It **never** applies to files the player has otherwise. A level in a
  folder of the player's own can be added as today: the maintainer's
  "should be possible too". The research's point stands: what is in a
  folder cannot be traced to where it came from. The rule covers what the
  game itself fetched, the case the research said differs ("the game, not
  the user, choosing the source").
- A third value, `reference` (each member's device fetching the item
  through the plugin, nothing through our server), is left out. The
  research found it fails where it would matter: N automated downloads
  for one person's choice, whole packs, no web members. It can come back
  if a site asks for it.

### Who decides

- A site's maintainers decide their source's rules, through the plugin
  they write or sanction: what it may do, its hosts, its pacing, whether
  rooms may carry its songs.
- The game's maintainer decides by accepting the plugin into the game,
  and may set stricter rules than a site asks for, never looser.
- Code infers no rule.

### Naming a source in the screens

- The source's name is its manifest's: the site's name for itself, or our
  own words for a first-party source. It is shown as data in our frame
  ("From {source}").
- No logos in version 1: a mark is a trademark, and showing one invites
  a reading of affiliation.
- A site's source shows "Made by {operator}" in its details, which is
  true and implies nothing of us.
- UI text otherwise follows AGENTS.md: no other game's jargon of our own
  making; a site is named only as the source it is.

## 5. Reference implementations

### The local folder source (first)

**Input**: a folder the player chooses (a USB stick, a downloads folder,
a StepMania install, a friend's pack).

**Search and filters**:

- title, artist and pack text;
- style (the layouts found), difficulty, a meter range, a BPM range and a
  length range;
- browse by pack.

**Metadata**: read the way the library reads it. The plugin links
`ddi-library` (pure, MIT) and reads through host calls: `List`, then
`Read` of simfiles and of image headers. It implements `scan::Reader`
over the protocol, and zips through the library's sans-IO zip reader on
ranged reads. Its index of a large folder is kept with `CachePut`. It
gives `song_key`, so "In your library" shows before anything else
happens.

**Fetch**: `in_place`. The app reads the song where it is.

- Import copies it into the import store, so it stays after the stick is
  gone.
- Play plays it in place.
- Add to room uploads it in place, as a zip's or folder's library song is
  uploaded today. Its small files are read into memory when it is added
  (within the budget), so the upload, an Upload again and the adder's
  own play do not depend on the stick staying in.

**Network**: none. **Files**: the chosen folder, read through the app.

**How it differs from adding a folder in Options**:

- An Options folder becomes part of the library: listed with everything,
  scanned at every start, cached in `library-v1.json`, its bests and
  packs mixed into the list.
- This source looks into a folder without making it the library's: find
  the one song in a downloads folder of a hundred zips, try a pack once,
  add a friend's song to a room, and keep only what is imported.

What it needs from the library: nothing new in the scanner. Desktop's
`files::DirReader` and `ZipReader` stay the app's; the plugin brings its
own `Reader` over host calls.

Why first: it exercises input, search, filters, browse, metadata, fetch
and all three uses, plus confinement and host calls, with no network,
testable offscreen with folders a test writes. And it is useful by
itself.

### The DanOni importer as a source (second, in two steps)

**Step one, an adapter in the app.** The shipped helper and its version 1
protocol are unchanged.

- The app presents it as a source with `input.url` and `fetch`; its
  delivery is `managed`.
- Its client code (`danoni::Imports`, `client::run`) is driven by the
  sources screen's Fetch, and its report and stages are shown there.
- Use: import only, as today.
- The URL tab in Add songs stays until the sources screen is accepted;
  then it opens the sources screen on this source.
- Nothing about the helper, its bundle, its packaging or its checks
  (`--danoni-importer-check`) changes.

**Step two (optional, later): play and rooms without importing.**

- The SDK already separates a verified "Ready candidate" from its
  admission into the store (`bind-import`, `resume-import`,
  `prepared_import.rs`).
- Play without importing would stop at the candidate. The SDK prepares
  on disk by design, in its own `cache/danoni/staging` (a known cache
  folder, the case where memory is not feasible). The helper names the
  candidate's SSC, Opus and art (a new delivery, `prepared`); the app
  reads those few small files into memory as a transient item; and the
  SDK drops the candidate when the app says it is done, or at its next
  start, as its staging is cleaned today.
- Import afterwards binds the same candidate, with nothing fetched again.
- Add to room then works as for any transient song.
- This is SDK work (an API that stops before admission and hands over
  the candidate's files) and helper work (the source protocol beside
  version 1, chosen by Hello). Medium to large.
- The helper's own confinement (it is unconfined) is a separate question,
  unchanged by this.

Its room rule: today a DanOni import is a library song like any other,
and can be added to a room. Recommended: keep `rooms = "upload"`, so
nothing changes for players (open question 4).

### A catalog of openly licensed songs (third)

**What**: songs whose music, charts and art each carry a free licence in
the upstream files, by the same rule as the bundled songs (AGENTS.md,
"Licensing and attribution"), but not bundled, to keep the game small.
`docs/research/cc-songs.md` lists the candidates: about fourteen more
CC BY and CC BY-SA songs of OutFox Serenity beyond the four bundled, and
Sockpuppet's eight under CC BY-SA, minus the art it found unclear.

**The index is ours**:

- `xtask gen-catalog` builds `catalog/index.json` from a `catalog/`
  folder of provenance entries in the bundled songs' `PROVENANCE.toml`
  shape: upstream commit, licences per part, exclusions, modifications.
- Each song carries its metadata, its charts with digests, its
  `song_key`, and every file's URL, size and SHA-256.
- It is published on the game's own Pages site beside the bundled songs'
  manifest.

**The files**, three options:

- **(a)** On our Pages site, as the bundled songs are. Simple, but the
  site grows by hundreds of MB, near Pages' limits.
- **(b)** From upstream's own release assets and repository at pinned
  commits, verified by our hashes. Licensed for it, but the bytes come
  from someone else's account.
- **(c)** A release of our own repository (as the `build-inputs`
  pre-release holds the FFmpeg builds): the upstream files byte-identical,
  with their provenance.

Recommended: **(c)**. Both ends are ours, and nothing reads anyone's
site.

**The plugin**:

- `Http` for the index (cached with `CachePut`, refreshed by ETag);
- search and filters over it locally;
- `Download` for an item's files (the app checks every hash), delivered
  as `files`;
- every use allowed, `rooms = "upload"`, attribution required, shown with
  the song.

It exercises network search, metadata with licences and attribution,
fetch with verified hashes and rooms, with nothing in it anyone could
object to. It is the closest model of a site's plugin, so the template is
cut from it.

### The game's own shares (not a plugin)

Share by link (`#/s/<id>`, `--shared`) and a room's uploads stay as they
are:

- they run on both shells, the web included, from one contract
  (`ddi-share-protocol`, `contract::Upload`);
- they need the game's own session or grant;
- making them plugin operations would split the web's path from
  desktop's for nothing.

A share link pasted into the sources screen can open the shared mode as
`--shared` does, which is a link, not a plugin.

So **upload is not in version 1**:

- no first-party source needs it;
- a site's upload needs that site's sign-in, which is not in version 1
  either.

The capability is reserved in the protocol's design:

- `Upload { op, item_files, metadata }`: the app gives the song's files
  as the share gathers them (`stored_files`), its entry, and what the
  player typed for that site. Never other files, the history, the
  settings, the game's session or another plugin's tokens.
- The app performs the requests the plugin describes, as with (A).

The SDK's harness tests it against a fake site in the test process.
The first real implementer is a site that asks.

### A template and an SDK (fourth)

- **`ddi-source-sdk`** (a crate in the workspace, MIT):
  - the protocol's types;
  - a `Source` trait whose methods each have a default answer of
    "unsupported", so a plugin implements only its capabilities;
  - a `serve()` loop: framing, Hello, host calls as async-free
    request/answer helpers, cancellation tokens, progress;
  - mappings from common style and difficulty names to our layout ids and
    difficulties.
- **A harness**:
  - runs a plugin (in process, or its binary over pipes) against a fake
    host: a folder fixture, recorded HTTP answers, a fake site for
    upload;
  - a conformance check (`xtask source check <bundle>`): the manifest
    well formed, Hello a subset of it, every declared capability
    answering, limits honored, cancellation within 5 s, no text where a
    value belongs.
- **The template** (`tools/sources/template/`): the catalog plugin
  reduced to one search, one filter and one fetch, with its tests.
- **Other languages**: the protocol alone, documented in a reference
  page, with JSON examples taken from the SDK's tests. A shipped plugin
  in another language must build in our CI and pass the same check.

## 6. In the screens

All of this is desktop only.

**Where sources live**

- A **Find songs** screen, opened from song select's More and from Add
  songs, and, in an online room, from My songs' More ("Find songs to
  add").
- It is one of our theme's screens: header and footer bars as song
  select's, so no bars slide.
- On the left, the sources: Folder… first, then the others in the
  manifest order the game chooses.
- At the top, the search field, and the source's filters in a row (a
  fold on a narrow window).
- The results list.
- On the right, the selected item's side: banner, charts, size,
  licence, attribution, "Open the source's page", and what the item
  allows.

**What each result offers**

- **Play**: fetches if needed, then opens the start prompt on the chosen
  chart, as song select does.
- **Import**: fetches and copies into the import store, then says "In
  your library".
- **Add to room**: in a room only, where the room rule and the room's
  own rules allow (`RoomClient::add_blocked`).

A pack offers Import and, in a room, Add pack to room, which reuses
`add_pack`'s bounds. Disabled actions say why on the status line, as
song select's footer does.

**Progress**: one line per running fetch ("Downloading 41%", "Reading
the songs…"), worded from the `step` values. Cancel is in the item's
actions. Reduced motion as elsewhere.

**Errors**: our sentence from the `code`, with a Details fold holding
the plugin's `detail` and the stderr tail.

**Fetched this session**: a pack at the top of the Find songs list, and
of the room's My songs, holding the run's fetched songs. This is how
they reach `set_library`: the room's song select builds its library map
from `data.songs`, and a transient song must be in it, or the next
refresh drops it from the client's map. Rows are marked as not in the
library, as `SongEntry::room_only` marks a room's songs today.

**An online room's Add to the room, for a transient song**: the same as
for a library song.

- The row's line says "From {source}".
- The upload reads the song's bytes from memory, which the room now
  holds: never dropped while its upload is pending or failed (More's
  Upload again needs them). Once the upload is confirmed they stay while
  the budget allows; under pressure they are released, and this device
  then loads the song from the room's share as every member does.
- Leaving the room releases them.
- Members see an ordinary room song.

**Options**: Data gets "Fetched songs" with the memory budget, a
"Fetch movies when playing" switch (on by default), a "Clear now"
action, and the list of shipped sources with an on/off for each. The
settings revamp (PLAN.md section 5, item 3) places them. Under Basic and
Advanced (item 7) the Find songs screen is the same. Its filters are the
source's, and a source decides how technical they are; ours (the folder
source's) are style, difficulty, level, BPM and length, all Basic words.

**The web**: nothing in version 1. Add songs stays as it is. A line such
as "More ways to find songs are in the desktop app" is possible later
(open question 12).

**Text**: every message in `locales/en/` and `locales/ja/`, in an area
of its own (`sources.ftl`), joining the `qps-ploc` walk. Plugin text
never enters it.

## 7. What it does not do

- **The web.** No plugins, no sources, no proxy through our server: a
  proxy would make our server the one redistributing (PLAN.md section 5,
  "What comes next", item 1).
- **Scraping.** No plugin reads a site's pages or private endpoints
  without that site's maintainers writing or sanctioning it. The host
  allowlist in the manifest the game's maintainer accepted is where that
  is enforced. The DanOni importer's user-triggered single-work fetch
  (decision 51) stays as it is and is not extended to anything new.
- **Shipping an integration with a site that has not written or
  sanctioned it.** The reference implementations are a folder, our own
  catalog, and a helper the game already ships.
- **Making native code safe.** A shipped plugin is as trustworthy as the
  review that accepted it. A player-installed one (if ever) is as
  trustworthy as whoever wrote it, sandbox or not.
- **Rights decisions.** Whether rooms may carry content at all, terms of
  use, the upload notice and takedown are round TU's and the
  maintainer's. This design only gives a source a way to say "not in
  rooms" for what it fetched.
- **Automatic downloads from a source.** Nothing is fetched through a
  plugin without a person's press: room preloading of a song added from
  a source reads our server's share, never the source.

## 8. Stages

Each stage is useful and testable alone. "No network" means a test needs
no connection beyond loopback fixtures.

| Stage | What | Needs decided | Size | Tested without network |
|---|---|---|---|---|
| 1 | `ddi-source-protocol`; the sandbox moved to a shared crate (video helper unchanged, its self-test as before); the app's plugin host in `crates/desktop/src/sources/` (bundles of several plugins verified as `danoni/bundle.rs` does, spawn, framing, host calls `List`, `Stat`, `Read`, `CacheGet`, `CachePut`, limits, kill and restart); a fake plugin in the tests. | Questions 1, 2, 9. | Medium. | Yes: synthetic plugins as the importer's client tests use shell-script helpers; self-test on macOS and Linux in CI. |
| 2 | The folder source and the SDK's first cut; fetched items in memory (`MemReader`, `Files::Memory`, `mem:` pictures, the preview from bytes, the budget) with the cache's `sources/` for what memory cannot hold, and `src:` ids in a registry `library::song` reads; play without importing (Find songs in a first, plain form); import into the import store with its catalog, merged into `Library`. | Questions 5, 6, 7. | Medium to large. | Yes: folders written by tests, offscreen plays as `tests/play.rs` does, imports and reopening. |
| 3 | Add to a room without importing: fetched songs in My songs, held while listed, the room rule enforced. No room model change. | Question 4. | Small to medium. | Yes: `FakeServer` and the desktop room tests; a local server and bots for the upload. |
| 4 | Find songs complete: filters rendered from schemas, browse, the item's side, progress, errors, Options rows, `sources.ftl`, the `qps-ploc` walk. | Question 5's details. | Medium. | Yes: offscreen pictures, the harness. |
| 5a | The DanOni adapter: the shipped helper as a source, nothing of it changed. | | Small. | Yes: the importer client's script helpers; `--danoni-importer-check`. |
| 6 | The app's HTTP for plugins: public addresses, allowlist, redirects checked hop by hop, pacing, `429`, budgets, receipts, cancellation, User-Agent. | Question 2's details (the resolver). | Medium. | Yes: a local HTTP fixture, with loopback allowed only under a test switch. |
| 7 | The catalog: `xtask gen-catalog`, the release that holds the files, the plugin, its credits. | Question 8. | Medium. | Yes against the fixture; one real run at the end against our own release. |
| 8 | The template, the SDK's docs, the conformance check. | | Small to medium. | Yes. |
| 5b | (Optional) DanOni play and rooms without importing: the SDK stopping at a Ready candidate, the helper speaking the source protocol. | Question 13. | Medium to large. | Yes: the SDK's synthetic candidates. |
| Later | Sign-in, upload, player-installed plugins, Windows. | Questions 3, 10, 11. | | |

What breaks or changes along the way:

- **The library**: a fourth set in `Library::merge_songs` (imports) and a
  fourth registry in `library::song` (transient songs). Folder songs keep
  winning collisions. `library-v1.json` is untouched: it caches folder
  roots only.
- **Desktop's play loading**: `Files` gets its `Memory` kind, `LibSong`
  says where a memory song is, the texture sources gain `mem:`, and the
  preview takes bytes. Library, folder and zip songs read as before.
- **The history**: plays of `src:` songs are kept. Their replays work
  while the item is there, then say "the song of this play is no longer
  there", as shared songs' do.
- **The room's My songs**: one more pack, "Fetched this session". No room
  model revision. A pin of drawn output may move with the new pack, so it
  is recorded again.
- **The build**: `crates/desktop/build.rs` builds the shipped plugins as
  it builds the helpers, each in its own target directory, with a
  `DDI_SOURCES_TEST_ONLY` debug switch as `DDI_IMPORTER_TEST_ONLY` is.
- **Packaging**: `tools/package/package.py` copies `sources/`, and a
  `--sources-check` runs headless as `--danoni-importer-check` does.
- **The video helper**: only its sandbox's place in the tree.

## 9. Open questions for the maintainer

In the order they block work.

1. **Who ships plugins in version 1?** (Blocks stage 1.)
   - (a) Only plugins built in or pinned by this repository, accepted by
     you.
   - (b) Also plugins players install, with a warning.
   - **Recommended: (a).** Player-installed plugins are not considered
     until the sandbox holds on all three systems and someone asks with
     a real plugin. Even then: brokered network only, digest approval by
     the player, and no promise of safety.
2. **Who makes a plugin's network requests?** (Blocks stages 1 and 6.)
   - (a) The app, for every new plugin; the DanOni helper keeps its own.
   - (b) Each plugin.
   - (c) A broker process.
   - **Recommended: (a).** It is the only one where "declared hosts only"
     holds, and it makes plugins sans-IO and testable. What it
     forecloses is in section 3.
3. **Windows.** (Blocks shipping there, not the work.)
   - (a) Sources wait until the restricted-token sandbox is checked on
     Windows hardware.
   - (b) Shipped plugins run unconfined on Windows.
   - **Recommended: (a)**, the video helper's rule: an open sandbox never
     serves. The folder source's input is the player's own files, which
     the game already parses in its own process, so (b) for that one
     plugin alone would be defensible if Windows players should have it
     sooner.
4. **Rooms for songs a source fetched.** (Blocks stage 3.)
   - Recommended:
     - `rooms = "upload"` for the folder source (the player's own files,
       your "should be possible too");
     - the catalog: its licences allow it;
     - the DanOni importer: as today, where any import can be added;
     - a site's plugin: its maintainers' choice, never looser than you
       accept.
   - Also: should a source's "none" follow its songs into the library
     after an import? Recommended: yes, through the import catalog; and
     never to the player's own copies from elsewhere.
5. **Where sources live in the screens.** (Blocks stages 2 and 4.)
   - (a) A Find songs screen of its own, reached from song select, Add
     songs and a room's My songs.
   - (b) Tabs inside Add songs, as DanOni's URL tab is.
   - **Recommended: (a)**: search and results need a list and a side,
     which a dialog's tab cannot hold well.
6. **The import store for sources other than DanOni.** (Blocks stage
   2.) DanOni's managed store stays as it is in every option.
   - (a) An app-owned folder store scanned as a root, with a provenance
     catalog beside it.
   - (b) A transactional managed store like the SDK's, generalized.
   - **Recommended: (a)**: an import is a copy renamed into place; the
     SDK's transactions exist for its admission and replacement rules.
7. **Memory's thresholds, and what forces the cache.** (Blocks stage
   2.) Memory first is decided; these numbers and cases are not.
   - **A song's small files in memory: 64 MiB at most.** Typical songs
     are 3 to 15 MB. Above that, the song goes to the cache.
   - **All fetched items held in memory: 256 MiB.** Twice the room
     store's "Likely next", and beside a play's own decoded music of
     about 70 MB. Alternatives: a share of the machine's memory (say an
     eighth, bounded 128 MiB to 1 GiB), or a setting only.
   - **What is dropped and what spills**: unheld items are dropped past
     the budget (fetched again on a press); held items (a play, a room)
     spill to the cache instead. Alternative: refuse a fetch that would
     pass the budget, which is simpler but says "no" where a disk has
     room.
   - **Movies**: never in memory; fetched only when a play loads, into
     the cache; a switch to leave them out. Alternative: never fetch a
     transient song's movies (the background shows), so nothing of it is
     ever written. Recommended: fetch to the cache, as shares do, the
     switch on by default.
   - **The cache's cap and lifetime**: 2 GiB, the run (deleted at a clean
     exit and at the next start). Alternative: kept for a day, as room
     shares are on the server, which saves fetching again at the cost of
     a cleanup that must work after crashes.
   - **A room's song after its upload is confirmed**: kept in memory
     while the budget allows, then released to the room's share and
     store (recommended). Alternative: held in memory until the room is
     left, whatever the budget.
8. **The catalog.** (Blocks stage 7.)
   - Which songs: **recommended**, the CC BY and CC BY-SA songs of
     `cc-songs.md` not bundled, each checked again against its upstream
     files.
   - Where the files live: **recommended**, a release of our own
     repository, byte-identical files with their provenance.
   - Also: whether the web may one day read the same index (our origin,
     no CORS question) for a "more songs" list. Not in version 1.
9. **The protocol.** (Blocks stage 1.)
   - **Recommended**: a new crate, `ddi-source-protocol`, with the
     importer protocol left alone until the DanOni helper moves (5b) or
     never.
10. **Player-installed plugins, ever?** Not blocking. Recommended: not
    planned; reconsidered only on a concrete request from a site whose
    plugin cannot be shipped with the game.
11. **Sign-in and upload.** Not blocking. Recommended: out of version 1,
    first built for the first site that asks, through the app (section
    3).
12. **The web.** Not blocking. Recommended: nothing in version 1; later,
    perhaps one line in Add songs pointing to the desktop app.
13. **DanOni without importing (stage 5b).** Not blocking.
    Recommended: after stage 5a, only if you want DanOni works playable
    and addable to rooms without entering the library; it is SDK work.
14. **A source's name in a room's list.** Not blocking. Carrying "From
    {source}" to the other members would put a field in a room's song
    record and its share's record, a room model revision. Recommended:
    later, if attribution in rooms is wanted. The adder sees it now.
