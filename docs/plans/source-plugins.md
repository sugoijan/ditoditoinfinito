# Song sources: native plugins for the desktop app (2026-11-25)

Status: stage 1 built (2026-11-26): the protocol, the host, the store
for fetched songs, synthetic test plugins, `--source-check` and the start
of the SDK; nothing a player sees yet ("Stage 1 as built", below). The
maintainer's decisions of 2026-11-25 and 2026-11-26:

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

  The plugin does not know which use a fetch is for.
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
   `SongEntry` to `RoomClient::add_song`, after the plugin's room rule
   (section 4). The upload reads it through `Files::of`, as for any song.
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
    publisher, version, protocol versions, capabilities, inputs, the
    room rule it declares, attribution;
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
rooms = "upload"                     # or "none"
attribution = { required = true, link_back = true }
```

### The room rule is advisory

- A source may say its songs are not for rooms (`rooms = "none"`). The
  game honors it: Add to room is not offered for songs it fetched,
  including after an import (through the import catalog).
- An item's metadata may narrow the rule, never widen it.
- When a plugin says nothing, the default is open question 4.
- It never applies to the player's own files from elsewhere. A level in
  a folder of theirs can be added as today: the maintainer's "should be
  possible too".
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
- **Actions**: Play, Import, and, in a room, Add to room where the rule
  and the room allow. A pack offers Import and Add pack to room.
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
| 2 | "This computer" built in, behind the in-process `Source` trait; Find songs in a first form; fetched songs in memory (`MemReader`, `Files::Memory`, `mem:` pictures, the preview from bytes, the budget, the cache for what memory cannot hold); play without importing; the import store. Useful with no plugin at all. | Medium to large. | Yes: folders written by tests, offscreen plays. |
| 3 | Add to a room without importing; the room rule. No room model change. | Small to medium. | Yes: `FakeServer`, a local server and bots. |
| 4 | Plugins in Options (Advanced), on/off, "Add plugin…", details with the digest, Remove; plugin sources in Find songs; `--plugin-check`; the systems' checks verified on macOS, Linux and Windows. | Small to medium. | Yes: synthetic plugins. |
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
  `store::Origin::rooms` (the manifest's, narrowed by the item's).

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
     reached by setting Interface → Detail to Advanced (or Options'
     More); "This computer" Basic; a plugin switched on keeps its source
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
