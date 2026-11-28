# Song sources: native plugins for the desktop app (2026-11-25)

Status: stages 1 to 3 built (2026-11-26); stage 4's Plugins page built
(round PG, 2026-11-28: "Stage 4 as built", below). Since round RC (2026-11-27)
one ordered level says how far a source's song may go, in place of the
room rule ("How far a song may go", section 4; "Round RC as built").
Since round PH (2026-11-27) every play of a found song is kept, with
where its song came from, and Find songs lists the songs played before
from each source; This computer's Import adds a folder in place ("Round
PH as built").
Stage 3: a found song added
to an online room without importing it ("Stage 3 as built", below).
Stage 1: the protocol, the
host, the store for fetched songs, synthetic test plugins,
`--source-check` and the start of the SDK ("Stage 1 as built", below).
Stage 2, the first a player can try, on desktop: "This computer" built
in behind its own switch (`--sources on`, tentative), Find songs in a
first form from song select's More, a found song played without
importing it, and imported ("Stage 2 as built"). The maintainer's
decisions of 2026-11-25 and 2026-11-26:

- plugins are trusted native programs, as other desktop software's are,
  with no sandbox imposed by the game;
- the game ships none;
- songs not imported stay in memory;
- and the six open questions of section 9, all decided on 2026-11-26.

## The request

The maintainer:

- "I'm leaning towards defining a plugin interface that covers all
  features (searches, some filters, download, upload, etc) that
  maintainers themselves (from FFR/Zenius/Etterna/etc) can use to
  implement the integration into the client. I'm wondering what would be
  a good example implementation for that plugin interface that's still
  useful." (2026-11-25)
- "The plugin idea I had in mind is more like the DanOni SDK, only
  targeting the desktop and compiled natively, not a Lua script. A plugin
  could be used to implement local search within a directory, for
  example. Also, the plugin implementer doesn't need to implement every
  'interface', or at least we should model it that way, so the DanOni SDK
  could be one implementation of it, it would take an 'input' being the
  base URL to read from, the interface could be used to expose
  'importing' songs as well as just playing without importing or adding
  them to an online room, also without importing into the local
  library." (2026-11-25)
- "About songs that shouldn't be imported, we should prioritize keeping
  them in memory only and using known cache dirs when that's not
  possible/feasible." (2026-11-25; decided, section 1)
- "I think we shouldn't ship any plugins by default. Even DanOni SDK I'm
  leaning more towards not including it and maybe even moving to another
  repo (but not now)." (2026-11-25; decided, section 3)
- "I'm not [sure] about the sandbox. In DanOni SDK the sandbox has a very
  specific purpose, which is not really to confine the whole plugin, but
  the execution of untrusted Flash, I think extending that to the whole
  plugin system is too much. This should be an advanced case, and akin to
  plugin systems of other software, like OBS, for example." (2026-11-26;
  decided, section 3)
- FFR levels a player has are not to be blocked from rooms ("if the
  author that uploads to FFR also wants to upload the unconverted level
  here, it should be possible too"). Rooms and shares are private and
  temporary. Terms of use, an upload notice and a takedown contact are
  drafted elsewhere (round TU), not here.

And from the research of round SR (four community sites, read on
2026-11-25): the game scrapes no one, and an integration with a site is
for that site's maintainers to write or to sanction. With plugins anyone
may publish, the game cannot enforce that. It ships none, endorses none,
and its SDK and template set the example (section 4).

## In short

- A **source plugin** is a native program an advanced player chooses to
  install. It is trusted like the plugins of other desktop software:
  - it runs with the player's own access and does its own network and
    file I/O;
  - it confines whatever untrusted thing it runs itself, as the DanOni
    SDK confines its Flash worker.
- It runs as **a separate process** speaking a versioned framed protocol
  over pipes, as the DanOni importer does. Why a process rather than a
  library loaded into the game, now that no sandbox is the reason: in
  section 2.
- It declares **capabilities**, each optional: inputs it resolves,
  search, filters, browse, metadata, fetch; sign-in and upload later.
- A fetched song has **three uses**:
  - import it into the library;
  - play it without importing;
  - add it to an online room without importing.

  The plugin does not know which use a fetch is for. It says how far
  its songs may go: played here only, added to rooms too, or imported as
  well (section 4).
- A song that is not imported **lives in the app's memory**, and goes to
  the known cache folder only where memory will not do.
- **The game ships no plugin.** The DanOni importer stays shipped and
  unchanged for now. Moving it to a plugin later is small, because it is
  already nearly one (section 5).
- **The game's own duties** do not depend on trusting a plugin:
  - what a plugin returns is parsed by the game's existing bounded
    parsers, as any song from the internet is;
  - a plugin that hangs or crashes never takes the game down;
  - the protocol's messages are bounded.
- Plugins are an **advanced** feature:
  - installed by putting a folder in the data folder, or with "Add
    plugin…";
  - listed with an on/off switch;
  - shown in the advanced level of the UI, not the default one.
- Searching a folder on this computer is **built into the app**, behind
  the same interface. The SDK, a test harness, examples and a template
  live in **a repository of their own**.

## What exists today (read in the code, 2026-11-25)

**The DanOni helper and its protocol.**

- `crates/importer-protocol` (`ddi-importer-protocol`, protocol version
  1):
  - each frame is a 4-byte big-endian length, then UTF-8 JSON, at most
    32 MiB;
  - `deny_unknown_fields` everywhere;
  - an exact version match on both sides (`Problem::Incompatible` in the
    app, `Fatal` from the helper).
- Its messages are DanOni-shaped:
  - `Initialize { paths: {cache, staging, library}, origins }`;
  - `Import { id, url, offline }`;
  - origin decisions;
  - the importer's own `Stage` enum;
  - snapshots of its managed catalog: `SnapshotBegin`, `SnapshotSong
    { entry, directory }`, `SnapshotEnd`;
  - a `Terminal` outcome.
- Errors are values: `Failure { code, message }`. The app words the
  `code`, and shows the `message` as data.
- The app side (`crates/desktop/src/danoni.rs`, `danoni/client.rs`):
  - the helper on a thread of its own, with bounded channels;
  - a 32 KiB stderr tail;
  - deadlines: 30 minutes for an import, 5 for opening, 10 s to stop,
    then a kill;
  - a snapshot admitted only when complete and inside the managed
    library.
- `danoni/bundle.rs` verifies the bundle beside the executable against a
  digest compiled into the app, so the helper and the app are always one
  build.
- **The helper process itself is not confined.** It is spawned as an
  ordinary child (`crate::os::spawn`) and makes its own requests (the
  SDK's `reqwest` and `hickory` stack, with public-address checks,
  pacing, limits and receipts).
- Only its Ruffle worker, which runs untrusted Flash, is confined, by the
  SDK itself (`supervisor/process.rs`): `sandbox-exec` on macOS; Landlock,
  seccomp and resource limits set between fork and exec on Linux.
- This is the model the maintainer named: the inner sandbox is the
  plugin's own business.
- The app links only the protocol crate. The app is on Rust 1.98; the
  SDK and the helper need 1.99. The importer exists on macOS and x86_64
  Linux only.

**The library.**

- The managed store's verified rows become `LibSong`s
  (`library/managed.rs`): the SDK's ids are kept, and chart versions are
  derived from a clone of each SSC.
- `Library` (`crates/desktop/src/library/mod.rs`) merges three sets by
  id:
  - folder songs, which win a collision;
  - managed songs;
  - a share's songs.
- `library::song` finds tests' songs first, then online rooms' shared
  songs (`register_room_songs`), then the scan's registry.
- Plays and uploads find a song's files through
  `play::load::Files::of(id)`.
- Imported folder songs get `pack::song_id(pack, dir)`.

**Shared songs and room uploads.**

- `crate::share::load_with` reads a share into the cache folder
  (`shares/<id>/…`, or a room's `rooms/<store>/…`), with ids
  `shared:<id>:<n>`.
- `clear_stores` deletes `rooms/` at every start that opens the window;
  `forget_store` deletes a room's store when the room is left.
- A room's list names a song by `ddi_share_protocol::song_key`: the
  digests of its original charts.
- `RoomClient::add_song` keeps the entry in the client's `library` map,
  which the room's song select resets from its data on each refresh
  (`set_library(&self.data.songs)`).
- The upload (`share::upload_room`) gathers files through `Files::of`.
  Nothing checks where a song came from.
- A share holds at most 2 GB, 512 MB a file and 200 songs. A room's list
  holds at most 100 songs.

**How a play reads a song's files on desktop.**

- `Files` is a folder or a zip read in place; `read` returns bytes.
- **The chart** is parsed from bytes, and **the music** is decoded from
  bytes (`audio::decoder::sound`). So are lyrics, a work's data and
  scripted backgrounds' files.
- **Pictures** are texture sources named by a path or by
  `zip:<zip>!<entry>`.
- **Song select's preview** takes a path; a zip's music is extracted to
  the cache's `previews/`.
- **Movies** are paths. The video helper and the OS decoders' demuxing
  start from a path, and a zip's compressed movie is extracted to the
  cache's `movies/`.
- **The room's upload** sends a folder's file from its path, or a zip's
  entry as bytes (`Source::File`, `Source::Bytes`).
- **The scanner** reads through `ddi_library::scan::Reader`: relative
  paths in, bytes out.

**Song data is already treated as untrusted input.**

- The library reads any pack put in a folder or a zip, the web imports
  whatever is picked, and room members download what another member
  uploaded. A plugin's songs are no new kind of input.
- The chart crate has no `unsafe`; the library has one use (`path.rs`).
- The zip reader is sans-IO, checks every length, bounds decompression by
  the declared size, and has opt-in damage tests
  (`zip_fuzz_other_methods`).
- FFR's SWF is refused over 256 MiB, and its interpreter is bounded in
  actions, stack and values.
- Pictures are bounded: 48 MiB a file, 8,192 pixels a side and 96 MiB an
  allocation.
- Music is decoded by pure-Rust decoders (Symphonia, `rusty-opus`).
- Scripted backgrounds run in the sandboxed Lua.
- Movies are decoded in the sandboxed video helper, or by the OS
  decoders for checked subsets.
- Not claimed: no fuzzing campaign over the chart parsers; a huge but
  well-formed chart is bounded only by memory; third-party decoders have
  their own `unsafe`.

## 1. The model

### Sources, items, capabilities

A **source** is one place to find songs: an installed plugin, or the
source built into the app ("This computer", section 5). Both implement
one interface; the built-in one runs it in process.

A source offers **items**: songs or packs, each named by an opaque id
the source chooses (at most 256 bytes), stable for that source.

Each capability is optional. A plugin declares the ones it has in its
manifest and again in its Hello. The app offers only what both say.

| Capability | What it means |
|---|---|
| `input` | Something the player gives the source, resolved into a **scope** to search, browse or fetch from. `folder`: chosen in the app's folder dialog, passed as a path. `url`: pasted. `link`: a link the app routes to the plugin whose manifest claims its host and path pattern. `text`: free text. A source without inputs has one implicit scope. |
| `search` | A query (at most 256 characters, possibly empty) and filter values, in a scope. Results stream in pages of at most 50; a cursor asks for the next page; Cancel ends it. |
| `filters` | A schema declared once, in Hello, which the app draws: `text`; `choice`, single or multiple; `range` (min, max, step, a unit of meter, BPM, seconds, bytes or count); `toggle`. The app knows no site's fields. |
| `browse` | Nodes (packs, categories, folders) with children and items, paged as search is. |
| `metadata` | An item before it is fetched: title, subtitle and artist, with transliterations; pack; charts (style as our layout id, difficulty as one of our six, meter, name, author); length; BPM; size; a banner; the source's page for it; the licence as the source states it; attribution; last change; an opaque revision; and our `song_key` where the source can compute it. |
| `fetch` | One item delivered to the app, with progress and cancellation ("What a fetched item is", below). |
| `signin` | Not in version 1: a plugin that needs a site's sign-in does it itself, as any program would. |
| `upload` | Not in version 1 (section 5). |

What three sources would implement:

- the built-in folder search: `input.folder`, `search`, `filters`,
  `browse`, `metadata`, `fetch`;
- the DanOni importer: `input.url` and `fetch`;
- a site's catalog: `search`, `filters`, `metadata`, `fetch`.

### The three uses

The plugin fetches. The app decides what the fetched files are for, and
the fetch does not say which.

1. **Import into the library.** The app writes the files into its import
   store (below), unless the plugin keeps a library of its own (the
   `managed` delivery: the DanOni importer's store).
2. **Play without importing.** The app lists the fetched songs alone, as
   song select's shared mode lists a share. They play through the
   ordinary prompt, play and results. Plays are kept, and bests are
   keyed by chart digest.
3. **Add to an online room without importing.** The app hands the song's
   `SongEntry` to `RoomClient::add_song`, where its source's level allows
   it (section 4). The upload reads it through `Files::of`, as for any song.
   Members load it from the room's share. Nothing source-specific crosses
   the wire, so there is **no room model revision**.

### Memory first (decided, 2026-11-25)

The maintainer: "About songs that shouldn't be imported, we should
prioritize keeping them in memory only and using known cache dirs when
that's not possible/feasible."

- For play and rooms, a fetched song lives **in the app's memory only**,
  for as long as it is needed:
  - the play, with its prompt and results;
  - a room's upload, and the time the room lists it.
- **Only where memory will not do** does it go to the app's known cache
  folder (`ddi_desktop::paths::cache_dir()`, beside `shares/`,
  `rooms/<store>/`, `previews/` and `movies/`), under `sources/`. Never
  the data folder or the library. It has a cap, and is cleaned up at exit
  and at the next start, as `rooms/` is.
- When memory will not do:
  - **over the budget** (below);
  - **a movie**: the video helper and the OS decoders open movies by
    path. A fetched song's movies are not fetched with it, only when a
    play loads (as a share's are, `share::ensure_all`), and then into the
    cache. With the setting off, the song's background shows instead.
  - **a plugin that keeps files of its own** (the DanOni SDK prepares in
    its own `cache/danoni/staging`).
- Streaming the music is not needed: the play decodes the whole music
  from bytes today.
- The import path does not change.

### What a fetched item is

The plugin tells the app in its `Fetched` message where the item's bytes
are, as one of these deliveries:

| Delivery | What the plugin hands over | Where it ends up |
|---|---|---|
| `in_place` | A path to a pack or song folder, or a zip, already on the player's disk (the folder search). | Nowhere new: the app reads it where it is. |
| `files` | The item's files, streamed over the pipe (`File` frames with attachments, below). | The app's memory. |
| `archive` | One zip or `.smzip`, streamed the same way. | The app's memory, read by the library's sans-IO zip reader. |
| `cached` | Files the plugin wrote into the cache folder the app gave it for this operation, because the item is over the memory budget. | The cache's `sources/<op>/`, owned by the app from then on. |
| `managed` | Rows of the plugin's own library, as the DanOni importer's snapshots are today. | The plugin's store, for import only. |

How the plugin knows which to use:

- `Fetch` tells it which delivery the app wants: memory, or a cache
  folder for this operation when the item's announced size is over the
  budget.
- A plugin that cannot know a size in advance streams. Past the budget,
  the app answers with a cache folder to continue in.

Why stream bytes rather than take a path the plugin wrote:

- it is what keeps a song off disk, as decided;
- a pipe carries hundreds of megabytes a second, far more than a
  download needs;
- a shared-memory arena (the video helper's) earns its keep for video
  frames sixty times a second, not for this.

**The budget in memory** (proposals):

- 64 MiB a song's small files (what `share::is_small` counts). A typical
  song is 3 to 15 MB, mostly its music. A song over it goes to the cache.
- 256 MiB for everything held at once: about 20 to 30 songs. That is
  twice the room store's "Likely next" (128 MB), beside a play's decoded
  music (about 70 MB for three minutes of 48 kHz stereo floats).
- Past it, items nothing holds are dropped, to be fetched again on a
  press. Held items (a play, a room) move to the cache instead.
- `in_place` items take none of it.

**The app reads every delivery with its own scanner**
(`pack::find_songs`, `scan::scan_song`), over a `MemReader` for memory,
as over folders and zips today. A plugin's metadata is a description for
the screens. The truth (charts, digests, files) is what the app reads.

### Playing from memory: what changes

- `Files` gets a `Memory` kind over the item's map of bytes. The chart,
  the music, lyrics and scripts then need nothing else.
- `LibSong` says where a memory song is, and `Files::lib` dispatches on
  it. About 25 places read `.root`; most are folder-only code.
- Pictures gain a `mem:<item>!<path>` source in `images::load_sized`.
- The preview decodes from bytes instead of a path.
- Movies: from the cache only (above).
- The room's upload: `share::source_of` answers `Source::Bytes` from
  memory.
- `song_key`: the scan over the `MemReader` writes every chart's
  digest, as for any song.

The desktop shell alone changes; the web, the session, the engine and
the renderer do not.

### Identity

**Fetched songs**:

- ids `src:<16 hex digits>:<n>`: the FNV-1a of the plugin, the item and
  its revision, then the song's index;
- they never clash with library, bundled or `shared:` ids;
- they are registered where `library::song` finds them, released when
  dropped.

**Imported songs** (deliveries other than `managed`):

- go to an app-owned import store, the data folder's
  `sources/library/<plugin>/<pack>/<song>/`, renamed into place from a
  staging folder;
- are listed by the ordinary scanner as a root the app owns, merged as a
  fourth set (folder songs win collisions);
- get `pack::song_id(pack, dir)`;
- have a catalog (`sources/catalog.json`) keeping each one's plugin,
  item, revision, page, licence as stated, attribution and import date.

**Recognized later** two ways:

- by `(plugin, item)`: "In your library", or "Changed at the source";
- by `song_key`: "You have this song", from wherever it came.

Plays of a fetched song are found by digest after it is imported.

### Lifetimes

| What | Where | Until |
|---|---|---|
| A fetched item nothing holds | Memory | Dropped past the budget, or the run ends. |
| An item a play holds | Memory; the cache if over budget | The play's results close. |
| An item a room holds, its upload not yet confirmed | Memory; the cache if over budget | Confirmed, removed, or the room left. |
| An item a room lists, its upload confirmed | Memory while the budget allows | Removed or left. Under pressure it is released, and the room's share serves this device as it serves every member. |
| A movie a play loads | The cache's `sources/` | Nothing holds it, the cap, or the run ends. |
| An import | `data/sources/library/`, or the plugin's own store | The player removes it. |
| A plugin's process | | 60 s idle, or the game's exit. |

## 2. The protocol

### A process, not a library loaded into the game

Without a sandbox as the reason, should a plugin still be a separate
process, rather than a dynamic library the game loads, as some software
loads its plugins?

**For a library**:

- no inter-process protocol;
- calls are direct and data is shared;
- it is the familiar shape of such plugin systems.

**For a process**:

- **Crash and hang isolation.**
  - A plugin's crash, panic, memory corruption or deadlock in a library
    happens inside the game: during a play, or a room's stage with others
    waiting.
  - As a process, a hang is a timeout and a kill, and a crash is a
    message on the screen.
  - The game's own duty, never to be taken down by a plugin, holds only
    this way.
- **No coupling to the game's Rust.**
  - Rust has no stable ABI. A library interface means a C ABI with
    `unsafe` on both sides of every call, and care with allocators,
    threads and panics across it.
  - Unloading a library is not safe in general.
  - A process needs only the protocol. The DanOni helper already builds
    on Rust 1.99 while the game is on 1.98.
- **Language freedom**: anything that writes JSON to a pipe. A C ABI also
  allows many languages, but with the risks above.
- **Signing and macOS**: a notarized app with the hardened runtime may
  load only libraries signed by the same team, unless it gives up library
  validation (an entitlement). A separate executable keeps that question
  out of the game, if the game is ever notarized.
- **The precedent**: the DanOni helper, which the maintainer named, is a
  process.

**Against a process**:

- the protocol has to be designed and versioned;
- bytes must cross a pipe (cheap at this scale);
- one process a plugin while in use, ended after 60 s idle.

**Recommended: a separate process** (open question 1).

### A new crate, beside the old one

- **`crates/source-protocol`** (`ddi-source-protocol`): serde only,
  platform-free, like `importer-protocol` and `share-protocol`. It is
  published to crates.io for plugin authors.
- `ddi-importer-protocol` stays untouched while the DanOni helper ships.
  It retires when the importer moves (section 5).

### Framing

Version 1's framing, with an attachment for bytes:

- a 4-byte big-endian total length;
- a 4-byte JSON length;
- the JSON;
- optionally, raw bytes after it (a fetched file's chunk).

Limits:

- 4 MiB a frame, 1 MiB an attachment;
- `deny_unknown_fields`, and unknown message types refused;
- truncation is distinct from a clean EOF.

These limits are the game's own protection, whatever the plugin.

### Versions and capabilities

- Hello carries the protocol versions each side speaks; the highest
  common one is used.
- None in common is a value the app words ("This plugin needs a newer
  version of the game", or the reverse).
- The plugin's Hello declares its capabilities, inputs and filters. They
  must be a subset of its manifest's.
- A new optional message or field takes a minor version both sides
  declare.

### Messages (a sketch)

App to plugin:

- `Hello { protocols, app: { version, language }, dirs: { cache, data } }`.
  `dirs` are the plugin's own folders,
  `cache/plugins/<id>/` and `data/plugins/<id>/`, for its own use.
- `Open { op, input }`
- `Search { op, scope?, query, filters, cursor?, page_size }`
- `Browse { op, scope?, node?, cursor? }`
- `Describe { op, item }`
- `Fetch { op, item, deliver: Memory | CacheDir { path } }`
- `Cancel { op }`
- `Shutdown`

Plugin to app:

- `Hello { protocol, plugin: { id, version }, capabilities, inputs, filters }`
- `Opened { op, scope, title? }`
- `Results { op, items }` (repeated), then `Done { op, cursor?, total? }`
- `Nodes { op, nodes }`, then `Done`
- `Item { op, item }`
- `Progress { op, step, done?, total?, bytes? }`. `step` is a closed set
  the app words: resolving, searching, downloading, reading, converting,
  checking.
- `File { op, path, offset, last }`, with the bytes attached.
- `Fetched { op, delivery }`
- `Snapshot…` (the `managed` rows, as today's)
- `Failed { op, code, detail? }`
- `Bye`

Operations:

- several at once, up to 4, each with an id the app chooses;
- `Cancel` ends one within 5 s.

### Errors are values

`Failed.code` comes from a closed list the app words in the player's
language:

- `cancelled`, `busy`, `invalid_input`, `unsupported_input`;
- `not_found`, `gone`;
- `refused_by_source`, `rate_limited`, `unreachable`;
- `too_large`, `unreadable_content`, `source_changed`, `internal`.

`detail` is at most 2 KiB of the plugin's own words, shown as data only
in a Details fold. No plugin sentence is ever an error's summary.

### Text from a plugin

- A plugin's labels (its name, filters, choices) come as a map of
  language tags to strings, English required.
- The app shows the current language, else English, as `UiText::data`,
  framed by our own messages ("From {source}"), in the slots made for
  them: never on a button or as a sentence of ours.
- At most 128 characters each (512 for an item's text). Control and bidi
  control characters are taken out.
- Plugin text never enters `locales/`.

## 3. Trust and installing

### Decided

The maintainer:

- 2026-11-25: "I think we shouldn't ship any plugins by default. Even
  DanOni SDK I'm leaning more towards not including it and maybe even
  moving to another repo (but not now)."
- 2026-11-26: "I'm not [sure] about the sandbox. In DanOni SDK the
  sandbox has a very specific purpose, which is not really to confine
  the whole plugin, but the execution of untrusted Flash, I think
  extending that to the whole plugin system is too much. This should be
  an advanced case, and akin to plugin systems of other software, like
  OBS, for example."

So:

- A plugin is **trusted native code that an advanced player chooses to
  install**.
- The game does **not** confine it, and does **not** do its network or
  file access for it.
- A plugin that runs something untrusted (the DanOni SDK's Flash) confines
  that itself.
- The game ships none.

### What installing a plugin means, said once

Installing a plugin is running a program on your computer, with your
access to your files and the network. The game cannot check what a
plugin does. Install only plugins from people you trust.

The Plugins page says exactly that, once, above its list and on "Add
plugin…". It is not a consent ceremony repeated at every step.

### What the game still does, whatever the plugin

- **Parses what it returns as untrusted song data**, with the bounded
  parsers every downloaded pack already goes through (section "What
  exists today").
- **Bounds the protocol**: frame sizes, text lengths, one operation's
  number of results, the memory budget for fetched songs.
- **Never lets a plugin take the game down**:
  - a plugin silent for 30 s during an operation, a search past 60 s, or
    a stop past 5 s is killed and reaped, and the operation fails with a
    clear message and the stderr tail in Details;
  - three crashes in a run, and that plugin is off until the next run;
  - plugins start at a lower priority than the game, and nothing new
    starts in one during a play.

### Installing

- **Where plugins live**: the data folder's `plugins/<id>/`.
- **What one is**:
  - `plugin.toml`, the manifest: id, name, the source's page,
    publisher, version, protocol versions, capabilities, inputs, how far
    its songs may go (`reach`), attribution;
  - its executable, one per platform (`bin/<target>/…`); the game takes
    its own platform's;
  - its licence.
- **How it gets there**:
  - the player puts the folder there;
  - or uses "Add plugin…", which copies a picked folder or zip there;
  - no download from a URL, and no install links.
- **Listed** on the Plugins page with its name, version, publisher (as
  it states them), the source's page, and an on/off switch.
  - A plugin found in the folder is **off until switched on**. One
    switch is the whole consent: the player's own act of putting it
    there and turning it on.
- **A digest, shown**: the SHA-256 of its executable, in the plugin's
  details, for comparing with what its publisher lists. It costs one
  hash, and it is the one check worth keeping. Signatures, pinned keys
  and consent pages are dropped: they would not make a trusted program
  safer, and comparable software does without them.
- **Updates**: the player replaces the folder. No automatic updates and
  no update checks.
- **Removal**: Remove deletes its folder. Its songs already imported
  stay. Its own `cache` and `data` folders are deleted with it, after
  one question.

### Where it shows in the UI

- Plugins are an advanced feature. The maintainer plans Basic and
  Advanced levels and a hidden settings screen (PLAN.md section 5, items
  7 and 8).
- **Recommended**: the Plugins page and Find songs' plugin sources show
  at the Advanced level only. Until the levels exist, the page shows
  once the plugins folder holds a plugin; "Add plugin…" is also on the
  hidden settings screen once it exists (open question 2).
- "This computer" (section 5) is not a plugin and shows at every level.

### The systems' own checks on a downloaded program

The game does not work around any of them. Each is to verify on the
system in the stage that installs plugins.

- **macOS.**
  - A file a browser downloads carries the quarantine mark, and the
    mark stays on a copy the game makes, if the copy keeps extended
    attributes.
  - When a marked program is started, Gatekeeper refuses it unless
    Apple has notarized it. Whether and how that applies to a program
    another program starts, as here, is to verify.
  - The honest paths:
    - **The author** signs it with a Developer ID and notarizes it
      (Apple's paid developer program).
    - **The player** approves it once in System Settings → Privacy &
      Security ("Open Anyway"), as they would any unnotarized program.
  - The game never strips the mark.
  - On Apple silicon, arm64 code must carry at least an ad hoc signature;
    the Rust toolchain's linker makes one by default.
  - The game is itself signed ad hoc and not notarized
    (`docs/plans/packaging.md`), so its players meet the same step for
    the game.
  - The SDK's documentation says all of this to authors.
- **Windows.**
  - A download carries the Mark of the Web. SmartScreen asks when the
    player opens such a file from Explorer, as far as I know not when
    another program starts it.
  - Smart App Control on Windows 11, when on, blocks unsigned programs
    however they start.
  - The author signs with Authenticode (a paid certificate), or the
    player unblocks the file (its Properties) where Smart App Control is
    off.
- **Linux.**
  - No system check. The game sets the executable bit on a copy it
    makes.
  - Authors build for a stated baseline: static, or an old enough glibc.

Plugins work on every desktop platform the game runs on: macOS, Linux
and Windows.

## 4. A source's rules, and courtesy to the sites

### The manifest

```toml
id = "example-songs"
name = { en = "Example Songs", ja = "…" }
publisher = "…"
version = "1.2.0"
homepage = "https://…"               # the source's page
protocol = [1]
capabilities = ["search", "filters", "metadata", "fetch"]
inputs = []                          # or ["folder"], ["url"], ["link"], ["text"]
reach = "keep"                       # or "rooms", or "play"
attribution = { required = true, link_back = true }
```

### How far a song may go (decided 2026-11-27)

Until round RC a source said one thing, whether its songs may be added
to rooms (`rooms = "upload"` or `"none"`, allowed by default), and
importing was always allowed. The question came up whether "not for
rooms" should still hold once its song is imported. The maintainer
(2026-11-27): "I think that complicates things more than they need to
be, maybe instead of dealing with 'not for rooms'/'importable' as
independent toggles, maybe it should be just a 'level' of how deep the
song can go (playing locally only -> playing online too -> saving it
locally) each level allowing all what comes before."

So a source states **one ordered level**, `reach`, three steps, each
allowing everything before it:

1. `play`: played on this computer only, without importing it;
2. `rooms`: that, and added to an online room (uploaded to the room's
   share for its members, still without importing it);
3. `keep`: those, and imported into the library.

- **A source that says nothing allows everything** (`keep`): rooms were
  allowed by default (decided 2026-11-26) and importing always was.
- **An item may lower its source's level for itself, never raise it**:
  the effective level is the lower of the two (`Reach::narrowed`). An
  item that names a higher one is held to its source's, never an error.
- **Each limit is checked where the action happens**, not only by
  hiding a button: Add to room needs `rooms` at the press, again once
  the item is fetched (by the level the store keeps for it), and at the
  room's upload; Import needs `keep` at the press, once fetched, and in
  the import itself. Play needs none.
- **The page says why a button is absent**, on the result's line, in the
  game's words: "Play only: its source does not allow importing it or
  adding it to rooms"; "Not for importing: its source allows playing it
  and adding it to rooms only".
- **"This computer" is `keep`**: the player's own files. A level in a
  folder of theirs can be added and imported as before (the
  maintainer's "should be possible too").
- **Nothing is remembered after an import**: a song that may be kept may
  go to rooms anyway, and one that may not go to rooms is never kept, so
  an imported song is an ordinary song of the player's.
- **What is not keeping**: the app's own caches, emptied each run and
  never read as the library: a held item moved to the cache folder's
  `sources/` under memory pressure, a movie a play loads, a preview's
  music written into `previews/` for the preview player. Nor is a play:
  a play record is the player's own presses and score, never the song, so
  every play of a found song is kept at every level, with a small note of
  where its song came from (round PH; until then such a play was kept only
  as the library's song of the same chart, else not at all).
- **What it gives up, knowingly**: a source cannot say "keep it, but do
  not share it". A song that may be kept may go to rooms; a source that
  wants its songs off rooms chooses `play`, or accepts rooms. That is the
  price of one rule instead of two.
- **It is a wish the game honors, not a protection**: a player can always
  copy files by hand. The SDK's README says so to authors.
- The person who adds a song is responsible for it, under the terms of
  use being drafted (round TU). The room's upload notice shows for a
  fetched song as for any other.

### Courtesy to the sites is the plugin's

A plugin makes its own requests. **The game cannot enforce pacing,
robots rules, or that a site sanctioned the plugin.** What remains:

- **The game ships no plugin and names none as endorsed.** A plugin's
  name and publisher are shown as it states them, with no logos and
  nothing implying the game's affiliation with a site (AGENTS.md's
  rules).
- **The SDK and the template set the example**, in their code and
  documentation:
  - requests only when the player asks;
  - pacing per host (one at a time; a second or more apart unless the
    site says otherwise; `429` and `Retry-After` honored);
  - an honest client name with the plugin's and the game's;
  - robots rules honored for anything automated;
  - and **a site's own maintainers as the intended authors**, or their
    sanction obtained.
- The SDK may offer helpers for that discipline (a paced HTTP client,
  public-address checks), as a convenience in the plugin's own process.
  The DanOni SDK's network module is the model, but it is not shared
  code with the game.

**An HTTP service in the app for plugins** (the app making requests on a
plugin's behalf, as a convenience): **not in version 1.** It would bring
back the brokered model the maintainer set aside, for no protection a
trusted plugin needs. The SDK's helpers do the job in the plugin.

## 5. Built in, examples, and the DanOni importer

### "This computer": built in

Searching a folder (a USB stick, a downloads folder, a friend's pack)
stays the recommended first source, **built into the app**, not a
plugin:

- **Basic.** No player should install a program to search their own
  disk.
- **Our code.** The app's scanner on the player's own files, as the
  library already does.
- **It makes the work useful alone.** It runs in process behind the same
  `Source` interface plugins implement, so stage 2 is useful before any
  plugin exists. It exercises:
  - input, search, filters, browse and metadata (with `song_key`, so "In
    your library" shows at once);
  - the `in_place` delivery;
  - all three uses.
- **How it differs from an Options folder**: an Options folder becomes
  the library (scanned at every start, cached, listed with everything).
  This looks into a folder without making it the library's.

### Examples, the SDK and the harness

Decided 2026-11-26: they **start in this repository**
(`tools/source-sdk`, `tools/source-test-plugins`) and move to **a
repository of their own** (for example `sugoijan/ddi-sources`) once the
interface settles; never in the game's packages:

- **`ddi-source-sdk`**:
  - a `Source` trait whose methods each default to "unsupported";
  - `serve()`, which does the framing, Hello, cancellation and progress;
  - streaming a fetched item's files;
  - mappings from common style and difficulty names to ours;
  - the optional courtesy helpers (section 4).
- **A harness**:
  - a fake game host that drives a plugin through every declared
    capability;
  - a conformance check: Hello a subset of the manifest, limits honored,
    cancellation within 5 s, values where values belong.
- **Examples**:
  - a folder search (the built-in source's logic as a plugin, the
    authors' model);
  - a small catalog served from local test files, which shows the
    network shape without any real site.
- **A template** cut from the catalog example.
- Other languages: the protocol, documented with JSON examples.

This repository keeps:

- `ddi-source-protocol`, published from here;
- the host;
- synthetic plugins in its own tests (as the importer client's tests use
  shell-script helpers);
- `--plugin-check <folder>`, a headless run of a plugin against the real
  host, as `--danoni-importer-check` runs the shipped helper.

The openly licensed catalog of the earlier drafts is **dropped for now**,
to be reconsidered as a built-in source if more free songs in the game
are wanted.

The game's own shares (share by link, a room's uploads) stay as they
are. **Upload to a site** is not in version 1. It is reserved: the app
would hand a plugin the song's files as the share gathers them, and the
player's choices for that site, never other files, the history, the
settings or the game's session.

### The DanOni importer: shipped now, a plugin later

**Now**, unchanged:

- the `danoni-importer` default feature;
- the build in `crates/desktop/build.rs`, and the package;
- version 1 of its protocol, and the Add songs URL tab;
- `--danoni-bundle-info` and `--danoni-importer-check`;
- CI's `danoni.yml`.

Find songs may show it through an adapter over its existing client;
that is optional.

**It is already nearly a plugin.** It is a separate native program, with
its own network, its own store, and its own inner sandbox for the Flash
worker. With plugins being ordinary trusted programs, the move takes:

- **The protocol.** The helper speaks `ddi-source-protocol`:
  - Hello with its capabilities (`input.url`, `fetch`);
  - its stages mapped onto the closed `step` set, or a few steps added;
  - its catalog snapshots carried as the `managed` delivery;
  - origin decisions dropped (the desktop never asks them today).

  This is a modest change in `tools/danoni-importer/src/`; the SDK is
  untouched.
- **Finding it**: in `plugins/danoni/` instead of the bundle beside the
  executable, with no compiled digest.
- **Its store and the existing imports' ids.** The importer keeps its
  managed store, at its present place (`data/danoni/library`,
  `cache/danoni/…`): the app hands it those paths, or its `dirs` point
  there. Its rows keep the SDK's ids, so bests, offsets and hidden songs
  carry over with **no migration**. Imports stay the plugin's
  (`managed`); the app's import store is for other plugins.
- **Play and rooms without importing**, if wanted: an optional later
  addition, where the SDK stops at its prepared candidate and streams the
  candidate's SSC, Opus and art as `files`.
- **What this repository loses**:
  - `tools/danoni-dump/` and `tools/danoni-importer/`;
  - the Rust 1.99 workspace members and the `importer` profile;
  - the bundle preparation in `crates/desktop/build.rs`;
  - the `danoni-importer` feature and `DDI_IMPORTER_TEST_ONLY`;
  - `crates/desktop/src/danoni/bundle.rs` and the version 1 client
    (replaced by the generic host); `library/managed.rs` stays as the
    `managed` delivery's adapter;
  - `crates/importer-protocol`;
  - package.py's copy and audit;
  - the two `--danoni-…` checks;
  - `danoni.yml`;
  - the helper's notices in the credits;
  - the URL tab, which becomes the plugin's `input.url` in Find songs.

  It keeps the reading and playing of DanOni works and SSCs.
- **What the other repository takes**:
  - the SDK, the helper, the worker's build inputs, the probe and
    verifier (depending on the game's crates by Git, as SkinCraft does);
  - their CI, notices and release builds for macOS arm64 and x86_64
    Linux;
  - its own answer to macOS's Gatekeeper (section 3).
- **What players do**: download the importer for their system, put it
  in the plugins folder (or use "Add plugin…") and switch it on. Their
  imports are where they were.
- **Before it moves**:
  - the host has run plugins for a release;
  - the `managed` delivery works;
  - the other repository releases builds players can run.

  It is still the maintainer's "not now".

## 6. In the screens

All of this is desktop only.

**Find songs**

- Opened from song select's More, from Add songs, and, in an online
  room, from My songs' More ("Find songs to add").
- The sources on the left: "This computer" first, then the plugins that
  are switched on (Advanced level).
- The search field and the source's filters at the top.
- The results, and the item's side: banner, charts, size, licence and
  attribution as stated, the source's page.
- **Actions**: Play; Import where the song's level is `keep`; in a room,
  Add to room where its level is `rooms` or more and the room allows. A pack offers Import and Add pack to room.
- **Progress** from the `step` values, and Cancel.
- **Errors**: our sentence from the code, and Details.
- **"Fetched this session"**: a pack at the top of Find songs and of the
  room's My songs, so the room's `set_library` sees fetched songs.

**Options**

- **Plugins** (Advanced): the sentence of section 3, the list with
  switches, details (page, publisher, digest), Remove, "Add plugin…".
- **Data**: "Fetched songs" (the memory budget), "Fetch movies when
  playing", "Clear now".

**The web**: nothing.

**Text**: messages in `locales/en/` and `locales/ja/` (`sources.ftl`),
joining the `qps-ploc` walk. Plugin text never enters them.

## 7. What it does not do

- The web.
- Ship, list, host or endorse any plugin. There is no store or registry.
- Confine plugins, or make their requests. Both are the plugin's own
  business (decided).
- Enforce courtesy to the sites. The SDK and the template set the
  example.
- Work around the operating systems' checks on downloaded programs.
- Decide rights. Terms of use, the upload notice and takedown are round
  TU's and the maintainer's.

## 8. Stages

| Stage | What | Size | Tested without network |
|---|---|---|---|
| 1 (built 2026-11-26) | `ddi-source-protocol`; the host: finding plugins in `plugins/`, manifests, spawning, framing, operations, streamed files into memory, timeouts, kills and crash counting; synthetic plugins in tests (well-behaved, hanging, crashing, flooding); `--source-check`; the SDK's start. | Medium. | Yes. |
| 2 (built 2026-11-26) | "This computer" built in, behind the in-process `Source` trait; Find songs in a first form; fetched songs in memory (`MemReader`, `Files::Memory`, `mem:` pictures, the preview from bytes, the budget, the cache for what memory cannot hold); play without importing; the import store. Useful with no plugin at all. | Medium to large. | Yes: folders written by tests, offscreen plays. |
| 3 (built 2026-11-26) | Add to a room without importing; the room rule (replaced by the level, round RC). No room model change. | Small to medium. | Yes: `FakeServer` and the shares faked on a loopback port; no local server run. |
| 4 (the page built 2026-11-28) | Plugins in Options (Advanced), on/off, "Add plugin…", details with the digest, Remove; plugin sources in Find songs; `--plugin-check`; the systems' checks verified on macOS, Linux and Windows. Built: the page, its switches, details, Open folder, Look again; not yet: Add plugin…, Remove, `--plugin-check`, the systems' checks. | Small to medium. | Yes: synthetic plugins. |
| 5 | The SDK's repository: SDK, harness, the folder and catalog examples, the template; the protocol crate published. | Medium. | Yes. |
| 6 | Find songs complete: filters drawn from schemas, browse, the item's side. | Medium. | Yes: offscreen pictures. |
| Later | The DanOni importer as a plugin (section 5); sign-in and upload where a site asks. | Small to medium (DanOni). | |

What changes along the way:

- the desktop library: a fourth set, imports, and a fourth registry,
  fetched songs;
- desktop's play loading: memory sources;
- the room's My songs: a "Fetched this session" pack;
- no room model revision, no change to the web, and nothing in the
  game's packages.

## Stage 1 as built (2026-11-26)

Nothing a player sees: no screen starts a plugin, the switches are a file
the host reads, and a player with no plugins pays a missing folder's
check at the window's start (and the empty `sources/` it deletes).

**What exists**

- `crates/source-protocol` (`ddi-source-protocol`, serde only, builds
  for wasm): version 1's framing (`frame`), messages (`message`), limits
  (`limits`), checks of a plugin's messages and of its Hello against its
  manifest (`check`), its text (`text`), the manifest's fields
  (`manifest`), and `vectors/v1.json`, frames as bytes with what they
  decode to or why they are refused, for other languages (made by
  `tests/vectors.rs`, which fails when the file is stale;
  `DDI_WRITE_VECTORS=1` writes it).
- `crates/source-host` (`ddi-source-host`, empty on wasm): finding the
  plugins (`installed`: `plugins/<id>/plugin.toml`, the executable in
  `bin/<arch>-<os>/`, its SHA-256), the switches (`switches`: the
  configuration folder's `plugins.json`, `{"on": [ids]}`, every plugin
  off until listed), a plugin's process and pipes (`process`), its
  thread (`supervisor`), the game's handle (`host::Host`: `start`,
  `request`, `cancel`, `try_event`, `shutdown`; never waiting), the
  store of fetched songs (`store`) and a fetch's assembly and scan
  (`assemble`).
- `crates/desktop/src/sources.rs`: the folders (the data folder's
  `plugins/` and `plugin-data/<id>/`, the cache folder's
  `plugins/<id>/` and `sources/`, the configuration folder's
  `plugins.json`), the store's folder emptied at the window's start, and
  `--source-check`.
- `tools/source-sdk` (`ddi-source-sdk`): the `Source` trait, every
  method optional (answering `unsupported`), `serve`/`main` doing the
  framing, the Hello, operations on threads of their own (up to four),
  cancellation and progress at most ten a second, the checks before
  anything is sent, streaming a fetch's files, and `difficulty`, sites'
  difficulty names as ours; its README for authors.
- `tools/source-test-plugins`: `ddi-example-source`, on the SDK, every
  capability over generated songs (no network, nothing of anyone's), and
  `ddi-bad-source`, misbehaving as the name it is installed under says;
  the host's tests (`tests/host.rs`) run both as real processes. Neither
  crate is a dependency of the app: `package.py` copies the app's own
  executable, its assets and the video helper, never `tools/`.

**The protocol in a page**

- A frame: a 4-byte big-endian length of the rest (at most 4 MiB), a
  4-byte big-endian length of the JSON (at least 2 bytes), the JSON, then
  the attachment (at most 1 MiB, a `file` message's only; an empty one
  only for a file's last chunk). A clean end only between frames: a cut
  frame is `truncated`, a length past the bounds `oversized` (refused
  before anything is allocated), anything else `malformed`.
- The JSON: one object, its kind in `type` (snake_case), every object
  refusing fields it does not know, closed sets (steps, failure codes,
  capabilities, units, difficulties) as values.
- App to plugin: `hello` (`protocols`, `features`, `app` {version,
  language}, `dirs` {cache, data}), `open`, `search` (scope, query of at
  most 256 characters, filters' values, cursor, page size of at most 50),
  `browse`, `describe`, `fetch` (`deliver`: `memory` with its budget, or
  `cache_dir` with a folder), `cancel`, `shutdown`.
- Plugin to app: `hello` (the version chosen, `features`, `plugin` {id,
  version}, capabilities, inputs, filters), `opened`, `results`,
  `nodes`, `done` (cursor, total), `item`, `progress` (step, done,
  total, bytes), `file` (path, its whole size, offset, last; the bytes
  attached), `fetched` (`files`, `archive`, `cached` or `in_place`),
  `failed` (code, detail of at most 2 KiB), `bye`.
- Limits (`limits.rs`): four operations at once; ids 256 bytes, cursors
  1 KiB; labels 128 characters in at most 16 languages, English
  required; an item's text 512; 32 filters, 256 choices, 64 charts an
  item; a fetched file's path 512 bytes and 16 parts, plain names only;
  4,096 files and 256 songs an item; a manifest 64 KiB.
- Versions: the app lists those it speaks, the plugin answers with the
  one it chose (or, sharing none, the newest it speaks, and exits); a
  minor addition is a `feature` both Hellos name.

**What the host does with each bad plugin** (each a test in
`tests/host.rs`, the timeouts shortened there)

| The plugin | The host |
|---|---|
| Never says Hello | Stopped after 10 s (`TimedOut`). A strike. |
| Silent 30 s with an operation running | Stopped (`TimedOut`). A strike. |
| A search, browse, open or description past 60 s | Stopped (`TimedOut`). A strike. |
| Ignores a cancel 5 s | The operation failed as cancelled at once; the plugin stopped after 5 s. A strike. |
| Crashes, or exits while working | Its operations fail as crashed, with the last 32 KiB of its diagnostics. A strike. |
| Leaves a child holding its pipes and exits | Its exit is polled: found gone within a second. A strike. |
| Floods progress | Told the game at most every 100 ms; stopped at the search's 60 s. |
| Floods results past its page, answers an operation never asked, sends a frame past 4 MiB, not JSON, or a log line on its output | Stopped at once (`Misbehaved`), without reading an oversized frame. A strike. |
| A frame cut short | Crashed. A strike. |
| Speaks no version the app speaks | Off for the run (`Incompatible`, which side is newer). No strike. |
| A Hello that is not its manifest's (id, version, a capability or input not declared) | Off for the run. No strike. |
| Says Bye and does not exit | Killed after 5 s. No strike. |
| Exits by itself, successfully, with nothing running | No strike. |
| A fetch whose counts are not what it sent, a chunk out of order or past its size, a path that leaves the item | Stopped (`Misbehaved`). A strike. |
| Song data that does not read | The fetch fails as unreadable, with why for each folder. No strike. |
| A fetch past the cache's cap | The fetch fails as too large and the plugin is told to cancel it. No strike. |
| Three strikes in a run | Off until the next run. |
| Not reading what it is sent (16 messages queued) | Stopped (`Misbehaved`). Not exercised by a test: the app sends too little to fill a pipe. |

Every process ended is reaped; the threads reading its pipes are never
joined, so a pipe a child keeps open holds nothing up. Plugins run at a
lower priority than the game (`nice` 10; below-normal on Windows). A
plugin's text is cleaned of control and direction characters before the
game sees it.

**The store at its limits** (`store.rs`, `assemble.rs`)

- An item's files stream into memory, counted as they arrive (never as
  announced). Past 64 MiB for the item, its files move to the cache's
  `sources/` and the rest follow there. Movies go there always.
- Past 256 MiB in all, items nothing holds are dropped, least recently
  used first; items something holds (`store::hold`, for stage 2's play
  and stage 3's room) move to the cache folder instead, written outside
  the store's lock.
- Past 2 GiB in the cache for the run, items nothing holds there are
  dropped; a fetch that still does not fit fails as too large.
- An item announced over 64 MiB (its metadata's size) is asked for into
  a folder the host made (`cache_dir`), which the plugin fills.
- A zip is read with the library's sans-IO reader, each entry through
  `zip::extract_to`, which bounds its output by its declared size; a
  name that climbs out of the zip is passed over.
- What read becomes songs through `ddi_library::scan::scan_root`, as a
  folder's songs do: ids `src:<16 hex digits>:<n>` (the item's key, the
  FNV-1a of the plugin, the item and its revision), the room's
  `song_key` where every original chart has a digest.
- Emptied when it opens (a run that ended without its exit) and when it
  is dropped.

**What changed from the plan, and why**

- The host is a crate of its own, `ddi-source-host`, that desktop
  depends on: its tests run real plugin processes, which Cargo builds in
  `tools/source-test-plugins`, without building the desktop for them.
  Desktop keeps the folders and the command.
- The check is `--source-check` (every installed plugin, its manifest and
  digest, each started), as the stage's brief asked, rather than
  `--plugin-check <folder>`.
- A plugin's own data folder is the data folder's `plugin-data/<id>/`,
  not `plugins/<id>/`, which is the folder it is installed in.
- Past the memory budget while streaming, the host moves the files to
  the cache folder itself and the plugin is not told: no "continue in a
  cache folder" answer, no race. `cache_dir` is asked only for an item
  announced over the budget.
- The memory budget is an item's, a pack's included (64 MiB), not each
  song's: the host knows the songs only once it has read them.
- A `file` frame carries its file's whole size, so a lie is caught at the
  chunk that tells it.
- `unsupported` joins the failure codes: what the SDK answers for a
  method left out.
- Not in version 1: the `managed` delivery and its snapshots (for the
  DanOni importer's move, which needs its catalog's rules), and an item's
  banner (the app fetches no URLs; a banner needs bytes before a fetch,
  a feature for stage 6).
- `in_place` takes a folder where it is; a zip named in place is read
  into the store as an `archive` is (desktop's in-place zip reader is the
  library's on desktop, for stage 2 to use).
- The manifest also names its `executable` and, for the link input, the
  `links` it claims; the item's licence field is spelled `license`.

**What stages 2 and 3 need from here**

- Stage 2: a `Host` the game holds (made from `sources::host_config`,
  the discovered plugins, `Switches`, a `Store` on `sources::store_dir`)
  with `wake` the event loop's; `Files::Memory` over `store::ItemFiles`
  (already a `ddi_library::scan::Reader`); `library::song` asking
  `Store::song` for `src:` ids; a play holding its item with
  `store::hold` until its results close; `mem:` pictures and the preview
  from `ItemFiles`; a movie through its `Stored::Disk` path; the built-in
  "This computer" as an in-process `Source` behind its own switch; no
  new operation started during a play (section 3), which the game, not
  the host, knows; the switches written by the Plugins page (stage 4)
  through `Switches::set`.
- Stage 3: the room's upload reading `ItemFiles` (`Source::Bytes`),
  holding the item while the room lists it; the rule from
  `store::Origin::rooms` (the manifest's, narrowed by the item's; since
  round RC `Origin::reach`, the level).

## Stage 2 as built (2026-11-26)

**How to try it**: `cargo run -- --sources on` (saved; `--sources off`
undoes it), then song select's More → Find songs…, Add folder…, and a
search. With the switch off and no plugin switched on, nothing shows and
nothing runs: the window's start reads the switches and makes no host.

**What exists**

- The host runs **sources built into the app** behind the same
  requests and events as a plugin's (`ddi_source_host::builtin`,
  `Host::add_builtin`): in process, a thread an operation, at most four,
  a cancel failing the operation at once and its answer dropped; what
  one fetches (its files where they are, or bytes) read by the store's
  scanner into `src:` songs as a plugin's are.
- **Nothing new starts during a play** (`Host::set_quiet`, set by the
  window's loop while a play runs): a request made meanwhile is kept and
  started once the play is over; a built-in operation waits at its next
  step; a plugin's operation already running goes on in its own process
  at its lower priority, and what it answers waits for the game (tested
  with the example's slow fetch, `tools/source-test-plugins/tests/host.rs`).
- **"This computer"** (`crates/desktop/src/sources/this_computer.rs`):
  its switch and folders are the desktop's own setting
  (`ddi.desktop.sources.v1`, never the settings the web shares; no
  Options row in this stage, the levels round rewording Options); the
  folders are chosen on the page itself (Add folder…, the system's
  picker; Remove). It reads song folders (`.ssc`, `.sm`, `.dwi`) and
  zips of them, by the library's own scanner, without adding them to
  the library; search by text (title, artist, pack, their
  transliterations, folded as song select's search), the filters
  `style` (our layouts) and `level` (a range of meters), browse by
  folder then pack, metadata with `song_key`; a song folder fetched
  `in place` (its files read where they are), a zip's song read into
  memory (its movies left out). Bounds, a folder: 8 folders deep,
  200,000 entries looked at, 20,000 songs, 30 s of work (time waiting
  during a play not counted); a cut is said on the page. Symbolic links
  are never followed, so nothing outside a folder named is read. What
  it found is kept in memory until the page opens again; nothing is
  written to disk. (Dancing☆Onigiri works and FFR levels, left out of
  this stage, are read since round SF: "This computer reads what the
  library reads", below.)
- **Find songs** (`ddi_ui::screens::find_songs`, its model
  `crates/desktop/src/sources/find.rs`): song select's More offers
  "Find songs…" while a source is on. A page built from the page blocks:
  the sources as buttons where more than one is on (This computer
  first, then the plugins switched on, by their own names), This
  computer's folders, a search field searched once it rests (and a
  Search button; an empty search lists everything), the results (title;
  artist, pack and charts as tags; "In your library" where the library
  has the same song by its `song_key`, or the item was imported before)
  with Play and Import each, More results, what runs with Cancel, a
  problem in the game's words with the source's own in Details. Keys,
  controller and mouse as every page. Its text is `locales/*/sources.ftl`,
  an area the web's build leaves out (`crates/l10n/build.rs`'s
  `DESKTOP_ONLY`; the screen's module is not built for the web either),
  and joins the `qps-ploc` walk.
- **Play without importing**: Play fetches the item and lists its songs
  alone in song select ("FOUND SONGS", its footer saying where they are
  from), played through the ordinary prompt, play and results. A
  fetched song is found where the library finds a song
  (`library::song` answers `src:` ids from the store as a `LibSong`
  with `fetched` set), its files read from the store
  (`play::load::Files::Memory` over `ItemFiles`), its pictures as
  `mem:<item>!<path>` sources, its movies by their paths on disk (the
  store keeps movies there), its preview from the file where it is on
  disk, else written once into the cache's `previews/` as a zip's music
  is (the preview player opens a file). The item is held from the
  prompt until the results close (`play::Request::held`), and while its
  songs are listed.
- **What such a play keeps** (decided here; replaced in round PH, where
  every such play is kept, "Round PH as built"): it is
  kept in the play history only where the library has a song with a
  chart of the same content (by digest), and then as a play of that
  library song (`sources::fetched::keeps_as`); otherwise it is not kept
  at all: no history entry, no best, no replay, no prior for the
  announcer. So no kept play ever names a song that may not be found
  again (a `src:` id is gone after the run). The ghost is the chart's
  best play by its digest, as for any song.
- **Import** (`crates/desktop/src/sources/import.rs`): the item's songs
  written into the data folder's `sources/library/<source>/<pack>/<song>/`
  (staged, then renamed into place, an earlier import of the same song
  replaced), recorded in `sources/catalog.json` (source, item, revision,
  folders, when), the library read again with that folder added to the
  folders it reads (`Songs::scanned`; Options does not list it), the
  fetched copy let go. Afterwards each is an ordinary song: its id
  `pack::song_id`, its bests and plays by its charts. A player's own
  folder holding a song of the same pack and name wins, as between any
  two folders.
- Tests: the host's built-in sources (`builtin.rs`), This computer over
  generated folders (`this_computer/tests.rs`: folders, a zip, a link
  never followed, a folder too deep, filters, browse, fetch in place and
  from a zip, bounds), Find songs through the play host with This
  computer (`crates/desktop/tests/find_songs.rs`: searched, typed,
  played to its results without being kept, its item held then let go,
  imported, marked, played again and kept as the library's song) and
  with the example plugin as a real process beside one that exits at
  once (`crates/desktop/tests/find_songs_plugins.rs`).

**What changed from the plan, and why**

- The "This computer" source lives in the desktop crate, not the host:
  it reads with the desktop's zip reader and folds text as song select
  does; the host's `Builtin` trait is what it implements.
- The surface is a page from the page blocks, not stage 6's screen; the
  filters are declared by This computer but not drawn yet.
- A play from memory is not kept unless the library has its chart (the
  plan said "Plays are kept, and bests are keyed by chart digest").
- The preview of a song held in memory goes through the cache's
  `previews/` (as a zip's does), not decoded from bytes.
- Import copies This computer's songs too (an `in_place` item), rather
  than adding their folder to the library: the import store is one
  place, and a player who wants a folder in the library has Add songs.
  (Replaced in round PH: This computer's Import adds the folder in place.)
- `Store::remove` lets an imported item go at once.

**What stage 3 needs from here**

- The room's upload already reads a fetched song's files
  (`share::source_of` answers `Files::Memory` with its bytes); it needs
  the item held while the room lists it (`sources::fetched::hold`), Add
  to room offered on Find songs' results and the found list where
  `store::Origin::rooms` allows (since round RC `Origin::reach`), and a
  "Fetched this session" pack in the room's My songs.

## Stage 3 as built (2026-11-26)

**How to try it with two devices**: on the first, a desktop app with a
source on (`cargo run -- --sources on`, or a plugin switched on), signed
in: Multiplayer → Private room, then the room's More → **Find songs to
add…**, Add folder… (This computer) or a plugin, a search, and **Add to
room** on a result. The second device (the web, or a desktop app with
another `DDI_HOME`) joins by the invite link: the song is in Room songs
at once, downloaded ahead from the room's share as any listed song, and
plays when picked. The first device plays it from its own copy.

**What exists**

- **The way in**: the room's More offers "Find songs to add…" on either
  tab, on desktop, while a song source is on and this player may add
  songs (`RoomClient::may_add`). It opens Find songs over the room
  (`sources::find::room_screen`).
- **The page from a room** (`FindView::room`): each result has **Add to
  room** in Play's place (nothing is played alone over a room) and
  Import as before; a result the room lists says "In the room" (by the
  `song_key` its source states, else by its songs as fetched in this
  run); one its rule keeps out says "Not for rooms: its source does not
  allow it" on its line and has no Add to room (`RoomOffer`; since round
  RC its level's words, "Round RC as built").
- **The same checks and words as My songs**, at the press, before
  anything is fetched: who may add, the person's half of the list, a
  full list that refuses or has nothing to remove
  (`RoomClient::add_blocked`), and charts of no style the room can play
  (`RoomClient::style_playable`, which My songs' styles follow), each
  refusal in the room's own notice (`NoticeDialog::show_refusal`); after
  the fetch the songs go through `RoomClient::add_song` (one) or
  `add_pack` (an item of several), which ask the room's reducer again.
- **What is added**: a result the library has (by `song_key`) is the
  library's song, nothing fetched; else the item is fetched (one fetched
  already in this run is used as it is) and its songs added as
  `SongEntry`s of their `src:` ids. The room is told what it is told of
  any song: the card and the original charts' digests, named by their
  `song_key`. **No room model revision, nothing new for the server**, and
  nothing new for the other devices, which load it from the room's share
  as any listed song (the web included).
- **The room rule** (`Sources::room_rule`, replaced in round RC by the
  level, `Sources::reach`): This computer's songs always
  (the player's own files: section 4); a plugin's manifest's `rooms`
  (allowed when it says nothing, decided), narrowed by the item's own
  `rooms`; after the fetch the store's `Origin::rooms` (the same rule, as
  the host worked it out) is checked again.
- **The upload** is the room's own (the entry's grant, progress, Upload
  again, its failures' words): `share::upload_room` reads the song
  through `play::load::Files`, whose `Memory` kind reads the item's files
  from memory or from the cache folder.
- **What is kept, and for how long** (`sources/rooms.rs`): the item is
  held in the store (`store::hold`: in memory, moved to the cache folder
  under pressure, never dropped) from Add to room until the room lists
  it (at most 30 s, `LISTING_WAIT`: a song the room refused after the
  press), then while the room lists it: its upload, and this device's
  own loads and plays of it, which read the kept copy (the room's client
  maps the entry to the `src:` song, as to a library song) and never
  download its own upload. It is let go when its entry leaves the list,
  when the library has the same song (imported since: the fetched copy
  then removed from the store once its upload is done), and when the
  room is left, closed, or this device removed from it (a sign-in or
  "Use this device" to mend keeps it). Each loop checks it
  (`rooms::sync`, from `Songs::poll`); a listed song whose copy here is a
  fetched one is kept however it was added.
- **My songs' "Fetched this session"**: the room's song select is given
  the songs fetched in this run that a room may have (their rule allows
  it, their charts make a `song_key`, the library has no song of that
  key) in a pack of the screen's own (`ddi_ui::state::place::FETCHED_PACK`,
  shown by its message, after the bundled songs), so they are added from
  the usual place (Enter, Add this pack) and the room's client knows
  them (`RoomClient::set_library`). The select is fed again when the
  store's items or the library change (`Songs::feed_room`,
  `Songs::room_data_at`; `Store::generation`). What the room is told of a
  song added from that pack names no pack (`new_song` leaves a screen's
  own pack out).
- **Added, then imported**: one entry still; once the library has read
  the import, the room's client reads the library's copy and the fetched
  one is let go and removed. **A fetched song of a library song** is the
  library's (the fetched pack leaves it out; Find songs adds the
  library's).
- **The store**: an item fetched again keeps what holds it
  (`Store::insert`; before, a fetch of an item held for a room replaced it
  with a copy nothing held).
- Tests: `crates/desktop/tests/room_find_songs.rs` (its own `main`, the
  example plugin under two names): the desktop's own room screen over the
  room model's fake room with a second member, the server's shares faked
  on a loopback port (`tests/common/shares.rs`): the More item, This
  computer's song added, listed, uploaded (the share's bytes are the
  song's files) and held; the example plugin's song added; its item that
  keeps out of rooms and a plugin whose manifest says `rooms = "none"`
  with no Add to room; a full list and a member who may not add refused
  at the press in the room's words; the second member's device
  downloading the song from the share and playing it, the adder's
  playing it from the copy kept (nothing downloaded of its own); a song
  removed let go, then added again from "Fetched this session" and held
  again; a song imported while listed (one entry, the library's copy, the
  fetched copy gone); the room left, everything let go. Unit tests: the
  page from a room (`find_songs.rs`), the qps-ploc walk, `new_song`, the
  store's `every_song` and holds kept across a fetch.

**What changed from the plan, and why**

- The room's More offers Find songs on both tabs, not My songs' alone: a
  host opens the room on Room songs, and adding is what the item is for.
- Find songs from a room offers no Play: a solo play over a room whose
  stage may start is not offered anywhere else either.
- "Fetched this session" comes after the bundled songs, as song select
  orders packs (the plan said "at the top").
- Not done: the rule **after an import** (section 4: "including after an
  import, through the import catalog"). An imported song is an ordinary
  library song, offered by My songs whatever its source said; the
  catalog does not record the rule. A question below; answered by round
  RC's level, under which nothing needs remembering.
- Not done: no test of a chart the room cannot play from Find songs (a
  room that seats CPUs over the fake room); the check is
  `RoomClient::style_playable`, the one My songs' styles follow.
- Not run: a local server (the fake room and the faked shares cover the
  contract; nothing of the server or the model changed).

**Questions for the maintainer**

1. Should a plugin's "not for rooms" follow its songs into the library
   after an import (the catalog would record it and My songs would refuse
   them), or does an import make them the player's like any song?
   (Answered 2026-11-27: one level instead, section 4; a song that may not
   go to rooms may not be imported.)
2. Find songs from a room: should Play be offered there too (a solo play
   while the room waits), or Import be left out?
3. "Fetched this session" lists everything fetched in this run (for Play
   too), not only what was fetched for the room: is that the wanted
   reach?

## Round RC as built (2026-11-27)

The level of section 4, "How far a song may go", through the protocol,
the host, the desktop app, the page, the SDK and the test plugins. No
server change, no room model revision, nothing for the web (the page's
module and `sources.ftl` are desktop only).

- **The protocol** (`ddi-source-protocol`, version 1 changed in place:
  the interface is not settled and no plugin exists outside this
  repository): `Reach` (`play`, `rooms`, `keep`; ordered, `keep` the
  default; `narrowed`, `rooms()`, `keeps()`) replaces `RoomRule`; the
  manifest's `reach` replaces `rooms`, an item's `reach` replaces its
  `rooms`. The old key is no longer read: a manifest that has it is
  refused as any unknown key is (`--source-check` names the key), an
  item that has it makes its frame malformed. `vectors/v1.json` gains an
  item's `reach` and the old key and an unknown step refused.
- **The host**: a fetch's level is the manifest's narrowed by the item's
  (`supervisor.rs`), a built-in source's its own (`Builtin::reach`,
  `keep` unless it says otherwise) narrowed alike, kept as
  `store::Origin::reach`; `Store::every_song` gives each song's.
- **Desktop**: `Sources::reach` (This computer `keep`, a plugin its
  manifest's, a source not known `play`); `fetched::reach`,
  `fetched::item_reach`; Find songs (`sources/find.rs`) works each
  result's level, shows it to the page, and refuses Add to room below
  `rooms` and Import below `keep` at the press and again once fetched,
  in the line's words; `import::import` refuses any song whose item's
  level is not `keep`, nothing written (`import::may_keep`); the room's
  "Fetched this session" offers only songs of `rooms` or more; the
  room's upload refuses a fetched song below `rooms` (a last check
  whatever added it, worded as a system failure: no path reaches it).
- **The page** (`ddi_ui::screens::find_songs`): `ResultView::reach`;
  Import only at `keep`; Add to room (from a room) only at `rooms` or
  more; the line's tag where a button is absent,
  `sources-result-play-only` and `sources-result-not-for-keeping` (in
  place of `sources-result-not-for-rooms`), the same words as the page's
  note when a press is refused (`Problem::PlayOnly`,
  `Problem::NotForKeeping`). Japanese drafts.
- **The SDK and the test plugins**: the README tells authors the three
  steps, that each includes those before it, that an item may only lower
  it, what it gives up, and that it is a wish the game honors, not a
  protection. The example plugin's `song-22`, `song-23` and `song-24`
  each name one step for themselves; the desktop tests' "closed" plugin
  is a play-only source (`reach = "play"`), whose `song-22` (naming
  `keep`) is held to `play`; the host's tests run the example under
  manifests of `keep`, `rooms` and `play`.
- **Tests**: the protocol's (the level's names, the old key refused, the
  narrowing table), the host's (`an_items_level_lowers_its_manifests_and_never_raises_it`,
  a built-in's), This computer's (`keep`), the import refused by itself
  (`import::tests`), the page (`a_results_level_leaves_its_buttons_out_and_its_line_says_why`,
  the qps-ploc walk), and desktop's `find_songs_plugins.rs` (the three
  cases alone, the refused import, My songs' fetched pack) and
  `room_find_songs.rs` (the three cases from a room, the play-only
  source).

## Round PH as built (2026-11-27)

The maintainer: "We should keep the play even if the song isn't kept.
Without a chart the play can't be re-judged, but that's fine. The primary
intention is that a plugin could be made to allow, for example, purely
playing songs from FFR, but at the same time keeping all your history, so
when you re-open the game and load the same song from the plugin, your
play history is still there. Maybe keeping some plugin-specific metadata
on the play could be useful to allow for the history to re-fetch the
chart, or to load the song through a plugin from the play history list."
Where those plays show while their song is not loaded: on Find songs, per
source ("Yes, let's go with that"); a play history screen of its own is a
later item. And for This computer's Import: "I lean into 'folder in
place' instead of copying".

**What a player sees across two runs**: a song found (on this computer
or through a plugin) and played; its results as any play's, its first
clear and new record told. The game closed and started again (the songs
fetched before are gone from memory and the cache): Find songs shows the
source's "Played before", the song with its artist, its chart, its best
and when it was last played. Play fetches it again (its source asked what
the item is now) and lists it as Play on a result does: its plays under
P, its best on its chart's row, its replay watched, the ghost of its best
where the ghost is on. A plugin switched off since: its songs stay listed
under "Played before from …", with "Its source is not on" and no button;
uninstalled, "Its source is not installed"; a source that no longer has
the item (a folder moved, a song taken down), "No longer at its source"
for the run. Nothing is deleted.

**What is kept** (`play-history.md`, "Plays of found songs"):

- Every play of a found song, at every level of how far its song may go,
  as any play is (the summary, the replay's log, the bests, the ghost, the
  announcer's prior): as the library's song where the library has a chart
  of the same content (as before), else by its `src:` id (the FNV of the
  plugin's id, the item's and its revision, `ItemKey::of`: the same the
  next time it is fetched at that revision). Stage plays in rooms are
  still never kept.
- **The origin**, on the record and its summary, optional, left out of
  the JSON where there is none (`PlayOrigin`): `source` (the plugin's id,
  or `this-computer`), `item`, `revision`, `song` (which of the item's,
  from 0), `title`, `artist`, `chart` (`name`, `style`, `difficulty`,
  `meter`). Nothing else of a plugin's own. Ids over 256 bytes (the
  protocol's own bound) make it none; texts are cut at 256 bytes.
- This computer's item id is a hash of where the song is (its folder's
  path, or its zip's and its folder in it) and its revision the chart
  file's size and time: the folder's path is in effect its item id, kept
  hashed; a moved folder is an item gone, an edited chart file a new
  revision. A work's id is its page's (or dump manifest's) path hashed
  with `work:` before it, its revision its page's and its dos's sizes and
  times; a level's its file's path with `level:`, its revision its file's;
  either in a zip, the zip's path and the file in it, the zip's size and
  time its revision (round SF).

**How plays join**:

- Everything that reads plays by chart reads the digest, never the song
  id: the plays list, the bests, the ghost, the results' comparison,
  "played". So after an import (in place, or into the import store) the
  plays kept under the `src:` id are the library song's chart's at once,
  each once (one record under one id; nothing is copied or re-keyed).
- A new revision is another `src:` id. Within the same plugin and item,
  plays are found by the chart's content: a chart whose notes are
  unchanged carries its plays and best; one whose notes changed starts
  with none, the old chart's plays kept (as an edited library chart's).
  Play from "Played before" says so where the revision changed: "This
  song changed at its source since you last played it…".
- Judged again (an older judge, or other rules): by song id as before;
  where the song is not loaded its chart is looked for by content in the
  library, then among the songs fetched in this run
  (`fetched::song_by_digest`, `history::read_chart`); found nowhere, the
  play keeps what it came to as played, its best shown as pending, and the
  pass leaves it for the session (a lookup, no source asked, nothing
  printed). A fetch makes the pass look again (`Songs::poll`, the store's
  generation).

**Every resolver walked**, with a song not loaded:

| Where | What it does |
|---|---|
| Song select's lamps, bests, "NEW" | By digest: a library copy of the chart shows them; none otherwise. |
| The plays list (P), a kept play's results | Reached from a loaded song only; by digest. |
| Replays (`play::replays::request`, `--replay`) | The record's song, else a song of its chart's content (library, then fetched): else refused, "its song is no longer in the library", as a removed song's. |
| The ghost, the results' first clear and new record | By the loaded song's chart's digest. |
| The reworker (desktop and web) | As above: passed over for the session. |
| The history's export and the web's import of plays | The record's text as it is; the summary made from it, origin included. |
| Desktop's import of the web's plays | The same reader. |
| "Played before" | From the origins alone; no song read, no source asked. |

**Privacy**: the origin stays on the device. The desktop sends no error
reports; the web's carry a kind, a route's kind and a scrubbed message,
never a play; the rooms' usage reports carry the client's kind and its
connection's health. A play history export carries it, in the player's
own file.

**"Played before"** (`ddi_ui::screens::find_songs::PlayedSection`,
`sources::find`): under the search, before the results. A row an item's song (whatever its revision), the most recently
played first: title, then artist · chart ("Hard 4 · Single") · "Best …"
(the score and grade as the lists write them, of the latest chart under
the rules it was last played by) · "Last played 2026-11-27 14:03". Five
shown, the rest in a fold, at most 50 a source (`PLAYED_MAX`); beyond
that a muted count ("… are not listed here. Their plays are kept.").
Play asks the source what the item is now (`Describe`) and fetches that
revision (one without metadata is fetched by the revision last played),
then lists its songs. The sources not on: a section each, "Played before
from …", no buttons.

**From a room** (follow-up, the maintainer approving, 2026-11-27): the
same list, with **Add to room** in Play's place on each row of the source
chosen, under a result's rules (`FindPress::PlayedToRoom`,
`Finder::played_to_room`): only where the source's level reaches `rooms`
(the line says "Play only…" where it does not; the item's own level is
checked again once fetched); the room's own checks and refusals at the
press (who may add, a person's half, a full list, a chart of a style the
room cannot play, by the chart last played; all its charts again once
fetched); a song the library has (a chart of the content last played) is
added as the library's copy, else the item is fetched again as Play does
and its songs added; "In the room" where an entry of the room's list has
that chart. Rows of a source that is off or gone stay without a button.

**Also in this round**:

- **This computer's Import adds in place** (`this_computer::InPlace`):
  nothing copied, nothing recorded in the import catalog; added to the
  library's song folders as Add songs adds one (Options → Song list lists
  it; removing it there removes it). What is added: a zip's song, the zip;
  else its pack's folder, the folder its song folder is in where that is
  inside a folder This computer searches, unless a listed folder is inside
  it; else its own folder: a song directly in a folder searched adds only
  its own folder, never the folder searched (follow-up, the maintainer
  approving, 2026-11-27). A folder
  listed already, or inside one, adds nothing and the page says the song
  is in that song folder. The page says what was added by its name. A
  plugin's songs are written into the import store as before.
- **The page's note clears when a new search starts.**
- The page's lead says what is kept; the found list's footer says the
  plays are kept.

**Tests**: `ddi_ui`'s `origin_tests.rs` (kept, read back, bounded, through
an export and an import untouched, `played_before`, counted once beside a
library copy), the page's (`songs_played_before_have_play_where_their_source_can_fetch_them`,
the qps-ploc walk), This computer's in-place rule
(`an_import_adds_the_pack_or_the_song_or_the_zip_in_place_once`: one song
of a pack, a second of it, a loose song, one directly in a folder searched,
a zip, a folder listed already; end to end in `find_songs.rs`), Played
before from a room (`room_find_songs.rs`: in the room already, no button;
removed, Add to room adds it again from the copy fetched),
and desktop's `tests/found_plays.rs` (its own `main`; the example plugin a
real process): two runs over a scratch home, a song of This computer's
and one of the plugin's played and kept, the store emptied, both found
under "Played before" and fetched again by the same ids, their plays and
best, a replay watched (and refused while the song was not loaded), the
plugin switched off, a judge older than today's with the chart absent then
present, an import in place counted once, a revision with the chart
unchanged and one with its notes changed. `find_songs.rs` and
`find_songs_plugins.rs` keep the play now, and import in place.

### This computer reads what the library reads (round SF, 2026-11-28)

The maintainer: "Let's do a quick round fixing the small things". This
computer found song folders and zips of them, but not the two other kinds
of song the library reads on desktop: Dancing☆Onigiri works and FFR
levels. It finds, plays, adds to a room and imports them now, as it does
the others, through the library's own readers:

- **Found**: the folder's files outside song folders (as it walks them,
  within the same bounds) and each zip's names are read by the library's
  readers of works and levels (`crate::library::works_and_levels`: the
  same `scan_works` and `scan_levels` a library folder's scan runs, named
  as the library names a folder's or a zip's files), stopping at the
  bounds between one work or level and the next. A work is one item (its
  page or dump manifest; a work of several songs one item of the kind
  Pack, named by its folder, found by any of its songs), a level one (its
  file). Ids and revisions: above, under "The origin".
- **Fetched**: a work's files (its page, its dos, each song's music and
  layers' images) or a level's (its file, its music beside a text level)
  read where they are (a zip's into memory), named as the library names
  them once the work's folder (the one holding all its files), the
  level's (its pack's) or the zip is added. The host reads an item's
  songs beyond its simfiles by a reader the shell gives it
  (`HostConfig::more`, desktop's `library::item_songs`: the same readers
  again), so the songs, their packs and entries are those the library
  then lists; a work's song carries where its work is
  (`FetchedSong::work`, `ddi_library::works::WorkRef`, moved there from
  desktop's library), which a play reads it by, as a library work's. A
  plugin's items are read the same way.
- **Played, kept, added to a room** as any found song: its `src:` id,
  its origin, "Played before"; the room's upload is the library's (a
  work's as a library work's).
- **Imported in place**: the work's folder, the level's folder, or the
  zip, as a song's pack folder or zip is; nothing copied.
- **Not done: a movie in a zip found.** A zip's song is still delivered
  without its movies (a song folder's play where they are). Wiring it
  needs (1) a stored file the store reads from a zip entry where it is
  (`ddi_source_host::store::Stored`, a variant naming the zip, the entry
  and its size, read through the library's sans-IO zip and counted as
  disk, not memory), delivered by This computer's zip fetch in place of
  leaving movies out, so the item's scan lists the movie; and (2)
  `play::load::Files::movie_unless`'s `Memory` arm opening such an entry
  as its `Zip` arm opens a library zip's movie (a stored entry read in
  place by its range, a compressed one extracted once into the cache's
  `movies/`). Neither is large, but (1) changes the store every source
  shares and its accounting, so it waits for a round of its own.

**Tests**: This computer's
`it_finds_works_and_levels_and_reads_them_as_the_library_does` (a work
and a level in a folder and in a zip: found, the same ids read again,
fetched through the host into the songs the library lists once their
folder or zip is added, entries equal but for the id, imported in place
or covered), and desktop's `tests/found_works.rs` (offscreen through the
play host, two runs over a scratch home: the work and the level found,
played to their results without importing them and kept with their
origin; the store emptied, both under "Played before" and fetched again by
the same ids; imported in place, the library listing them where they are,
nothing copied, the work's play counted for the library's chart).

**Not done**: a play history screen of its own; the "Played before"
list's filters or its search (the source's own search finds the rest);
removing a song's plays from the page (Options' Kept plays removes them
all, song select's plays list one by one once the song is loaded); a
plugin's metadata language for the title (the origin keeps the simfile's
title); the reworker focusing a found song's plays loaded under another
revision's id (the background pass finds them by content instead, a
second and a half later).

## Stage 4 as built (2026-11-28, round PG)

The Plugins page, as a section of Options on desktop at the Advanced
level ([ui-levels.md](ui-levels.md), section 6). Not in this round:
"Add plugin…" (copying a picked folder or zip), Remove, Find songs'
filters drawn from schemas, `--plugin-check`, and the systems' checks
of a downloaded program verified on each system (section 3, "The
systems' own checks"): left for a round of their own.

**What a player sees.** Options → Interface → Show advanced options On:
a thirteenth section, Plugins, at the end of the rail (after Data and
privacy). Its lead says once what a plugin is: a program that finds
songs for Find songs, running on this computer with the player's access
to their files and the network, which the game cannot check, so to
switch on only plugins from people they trust; how to add one (put its
folder in the plugins folder, come back, Look again, switch it on); and
that the game never downloads, installs or updates plugins. Its rows:

- **Plugins folder**: Open folder (the system's file browser on the
  data folder's `plugins/`, made where missing: `crate::os::open_folder`,
  `open` on macOS, `explorer` on Windows, `xdg-open` elsewhere) and Look
  again (the folder read again); beside them how many plugins it holds,
  "None yet", or "Looking…" while it is read; its sentence names the
  folder's path.
- **BUILT INTO THE GAME**: This computer's switch, the same setting
  `--sources on|off` writes (`ddi.desktop.sources.v1`); its sentence
  lists the folders it searches, chosen on Find songs.
- **INSTALLED PLUGINS**: each plugin a switch (Off and On) named by its
  own name, beside it "Version … · publisher · " and the first twelve
  characters of its program's SHA-256 (the plugin's words, data), or
  what keeps it from running now; its sentence (Options' description
  panel, shown on focus) says what it is now, its version and publisher
  "as the plugin says", what it does (from its manifest's capabilities
  and inputs, in the game's words: searching, search filters, browsing,
  song details, fetching songs, songs from a folder you choose, a web
  address, links to its hosts, text), how far its songs may go (the
  three levels' sentences), its page, and the whole SHA-256. A plugin
  with a page has a row under it, its address and Open its page (the
  system's browser). A folder that is no plugin the game takes is a row
  of its folder's name and why: no `plugin.toml`; a `plugin.toml` that
  does not read (the reader's words in the sentence); a folder named
  otherwise than its manifest's id; no program for this computer (the
  platform and the path looked for). Where the folder holds nothing:
  "No plugin is installed." and how to add one.

**The states' words** (English; Japanese drafts in `locales/ja/sources.ftl`):

| State | Beside the switch | Its sentence starts |
|---|---|---|
| Off | Version · publisher · digest | Switched off: it does not run. |
| On | Version · publisher · digest | Switched on: it runs only when Find songs asks it for songs, and stops after a minute with nothing to do. |
| Three abnormal ends this run | Stopped working: off until the game starts again | It stopped working three times, so it is off until you start the game again. |
| A Hello not its manifest's | Not as its plugin.toml says: off until the game starts again | It did not start as its plugin.toml says, … |
| No protocol version in common (its manifest's, or its Hello's) | Needs a newer version of the game / Made for an older version of the game (no switch) | It needs a newer version of the game, so it cannot run. / It was made for an older version … |
| Refused folders | Not taken: no plugin.toml / its plugin.toml does not read / its folder has another name / no program for this computer | the same, with why |

**Switching.**

- On asks once (`plugin-switch-on`): "Switch on {name}?", the plugin's
  name and publisher as it says, that it is a program that runs on this
  computer whenever Find songs asks it for songs, with the player's
  access to their files and the network like any program they run, that
  the game cannot check what it does, and to switch it on only if they
  trust where it came from; Keep it off chosen first (Start alone never
  switches one on), Switch on beside it. The row stays Off while it asks.
- Off, and This computer either way, asks nothing.
- The switches are written at once into the configuration folder's
  `plugins.json` (`Switches::set`), the file `--source-check` and the
  window's start read, and the host is told (`Host::switch`: off, its
  process is shut down and what was queued for it fails as switched
  off); the plugins folder read again gives the host what is installed
  now (`Host::set_installed`: a new plugin added off, one as it was kept
  as it was, a changed one stopped and taken anew with what it is this
  run, one gone stopped and forgotten).
- **Find songs follows at once**: song select's More offers Find songs
  while a source is offered (`Songs::finding`, now "a source on" rather
  than "the sources made"), and its sources are the host's switched on;
  the first switched on in a run that held none makes them
  (`Songs::make_sources`, the window's run only, as `with_sources`
  allows).
- **What starts a plugin**: Find songs asking it for something (a
  search, a fetch, a description), as before. Opening the page, Look
  again and switching it on start nothing: what it does is read from its
  manifest, and its executable hashed (on a thread, `sources::plugins::scan`).
  Its live Hello is never asked for the page; where the Hello disagrees
  with the manifest the plugin is off for the run and the page says so.
- **Reset**: at Basic the section shows only "Advanced changes: N" where
  This computer or a plugin is on (each switch's default Off), its Reset
  switching them off through the shell (`OptionsModel::shell_changes`,
  `OptionsPlatform::section_row`); Data and privacy's Everything switches
  This computer and every plugin off too (its question says so on
  desktop); Settings alone leaves them as they are.

**The rail.** Options' full rail runs down to the footer and keeps
today's 48 px pitch while its sections fit (twelve at 1280×720); past
that its sections sit closer, down to 40 px (thirteen at 45 px, fourteen
at 42 px), and past what fits at 40 px it scrolls: the chosen section
kept in view (Tab, the shoulders), a section focused by keys scrolled
into view, the wheel and a drag moving it (`OptionsScreen::rail`). The
compact layout's tab strip scrolled already. Rows may show a note beside
their choices now (a plugin's state), and a section may have a lead
above its rows (`OptionSection::lead`).

**Where it lives.** `ddi_ui::screens::options_plugins` (the section
from a `PluginsView`; desktop only, the module absent from the web's
build, its text in `sources.ftl`, a desktop-only area);
`crates/desktop/src/sources/plugins.rs` (the scan, the view, the
switches); `crates/desktop/src/options.rs` (the section in `World`, its
rows' calls, the question); `ddi_source_host::Host::switch`,
`set_installed`, `off_why`.

**Tests.** `ddi_ui`'s `options_plugins_tests.rs` (the rows of each state,
the levels' count, a row's change and its count's Reset handed to the
shell, the rail with thirteen and with twenty-four sections), the
`qps-ploc` walk (`plugins_walk`: every state, at both levels, drawn);
the host's `plugins_are_switched_and_the_folder_read_again_while_the_host_runs`
and `off_why` in the crash and protocol tests
(`tools/source-test-plugins/tests/host.rs`); desktop's
`tests/plugins_page.rs` (its own `main`, the example and a crashing
plugin run as real processes, a plugin whose manifest does not read and
one with no program for this computer): each row's state, nothing
started by opening the page, the question and Keep it off, Find songs
following a switch both ways, three crashes, This computer's switch,
Basic's count and its Reset, an empty folder, Japanese, pictures. The
rows' record (`options_rows_tests.rs`) is the model's own rows: a
shell's section is not in it, and is held by its own tests.

## 9. The maintainer's decisions

**Decided**:

- "This should be an advanced case, and akin to plugin systems of other
  software, like OBS": trusted native plugins, installed by the player;
  no sandbox and no network imposed by the host (2026-11-26).
- "I think we shouldn't ship any plugins by default": none shipped; the
  DanOni importer stays for now and may move later (2026-11-25).
- "We should prioritize keeping them in memory only and using known
  cache dirs when that's not possible/feasible": memory first for songs
  not imported (2026-11-25).

The six questions this plan asked, decided on 2026-11-26 (the
recommendations as written below, kept for their reasons):

1. **A separate process.**
2. **The Plugins page behind the Advanced toggle** (the levels are
   another round's work; stage 4).
3. **"This computer" built in, tentatively**: "it might either lead into
   replacing the 'Add song' on the desktop app if it's good and complete
   enough or maybe I'll decide to not include it after testing". So
   behind a switch of its own and apart from Add songs (stage 2).
4. **The room rule when a plugin declares none: allowed** by default.
   Since 2026-11-27 one level replaces the rule ("How far a song may
   go", section 4), and a source that says nothing allows everything.
5. **The memory limits as proposed**: 64 MiB a song, 256 MiB in all, the
   cache 2 GiB for the run, movies always in the cache; to adjust after
   use.
6. **The SDK and the examples start in this repository** and split out
   once the interface settles.

As they were asked, in the order they blocked work:

1. **A separate process, or a library loaded into the game?** (Blocks
   stage 1.)
   - **Recommended: a process**: crash and hang isolation, no coupling
     to the game's Rust or a C ABI, any language, the DanOni precedent
     (section 2).
2. **Where plugins show until Basic and Advanced exist.** (Blocks stage
   4.)
   - **Recommended**: the Plugins page appears once the plugins folder
     holds one; with the levels, at Advanced only.
   - The levels' design is [ui-levels.md](ui-levels.md) (2026-11-26):
     the Plugins page a section of Options at Advanced on desktop,
     reached by turning Show advanced options On (or Options' More); "This computer" Basic; a plugin switched on keeps its source
     in Find songs at Basic (its section 6).
3. **"This computer" built in?** (Blocks stage 2.)
   - **Recommended: yes.** It is Basic, it is our code, and it makes the
     work useful before any plugin exists.
4. **The room rule when a plugin declares none.** (Blocks stage 3.)
   - **Recommended: allowed**, as for any song a player has, with the
     upload notice. The other way: "none" unless declared.
5. **Memory's thresholds.** (Blocks stage 2.)
   - **Recommended**: 64 MiB a song, 256 MiB in all; held items spill to
     the cache; movies only in the cache, fetched when a play loads,
     with a switch; the cache 2 GiB for the run.
6. **Where the SDK and the examples live.** (Blocks stage 5.)
   - **Recommended**: a repository of their own; this one keeps the
     protocol, the host and synthetic test plugins.
