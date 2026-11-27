# Writing a song source plugin

A **song source plugin** lets the DitoDitoInfinito desktop app search a
source of songs and fetch them: songs on a site you run, a catalog of
your own, a folder layout the game does not know. The player can then
import a fetched song, play it without importing it, or add it to an
online room. Your plugin does not know which; it only fetches.

This crate, `ddi-source-sdk`, is the start of the kit for writing one in
Rust. It lives in the game's repository for now and will move to a
repository of its own once the interface settles. The protocol itself is
`ddi-source-protocol` (`crates/source-protocol/`); a plugin in another
language speaks it directly, and `crates/source-protocol/vectors/v1.json`
has frames as bytes for checking an implementation.

The game ships no plugin and endorses none. Plugins are an advanced
feature: a player installs yours by choice.

## What a plugin is

- **A native program**, started by the game when it needs it and spoken
  to over its standard input and output. It is stopped after a minute
  with nothing to do, and when the game exits.
- **It runs with the player's own access**, like plugins of other desktop
  software: it does its own network and file access, and the game does
  not confine it. If your plugin runs something untrusted (a downloaded
  script, a site's player), confine that yourself.
- **Every capability is optional.** Implement what your source does:
  `open` (resolve what the player gives: a folder, a link, text),
  `search` (with filters you declare), `browse`, `describe` (an item's
  metadata) and `fetch`. Declare them in your manifest and in
  [`Info`](src/lib.rs); the game asks only for those.
- **What you fetch is read by the game itself.** Your metadata describes
  an item for the screens; the charts, digests and files the game plays
  are what its own parsers read from the files you deliver, held to the
  same limits as any downloaded pack.
- **The game holds you to the protocol's limits.** A message past one,
  out of order, or a plugin that goes silent for 30 seconds during an
  operation, takes more than 60 seconds over a search, or ignores a cancel
  for 5 seconds, is stopped; three such ends in a run and the plugin is
  off until the next. The SDK checks what it sends before it sends it.

## Installing one

A plugin is a folder in the game's data folder, `plugins/<id>/`:

```text
plugins/my-songs/
  plugin.toml
  bin/aarch64-macos/my-songs
  bin/x86_64-linux/my-songs
  bin/x86_64-windows/my-songs.exe
  LICENSE
```

`plugin.toml`:

```toml
id = "my-songs"                 # the folder's name
name = { en = "My Songs", ja = "マイソング" }
publisher = "You"
version = "1.0.0"
homepage = "https://example.org/songs"
protocol = [1]
executable = "my-songs"
capabilities = ["search", "filters", "metadata", "fetch"]
inputs = []                     # or ["folder"], ["url"], ["link"], ["text"]
reach = "keep"                  # or "rooms", or "play": how far your songs may go
attribution = { required = true, link_back = true }
```

A plugin found there is **off until the player switches it on**.

### How far your songs may go

`reach` says what a player may do with a song your plugin fetched, in
three steps, each allowing everything before it:

1. `play`: play it on this computer, without importing it. Nothing is
   added to the library, and the song is gone when the game exits.
2. `rooms`: play it, and add it to an online room (the game uploads it
   to the room for the other players, for as long as the room lists it),
   still without importing it.
3. `keep`: all of that, and import it into the player's library, where
   it stays as any song of theirs.

A manifest that says nothing allows everything (`keep`). An item may
lower its source's level for itself (`ItemMeta::reach`), never raise
it: an item that names a higher step than your manifest is held to the
manifest's. There is no "keep it but do not share it": a song that may
be kept may go to rooms, so choose `play` for that, or accept rooms.

This is a wish the game honors, not a protection. The game offers only
the buttons your level allows and checks it again when each is pressed,
but a player can always copy files by hand, and a song played is
decoded on their computer like any other. The
game shows your executable's SHA-256 beside it: publish yours so players
can compare. Updates are a new folder in its place; the game never
updates or downloads plugins.

`ddi --source-check` lists the installed plugins with their manifests and
digests, starts each, and prints what each declared, without a window.

The operating systems check downloaded programs, and the game does not
work around them: on macOS sign and notarize your executable, or the
player approves it once in System Settings; on Windows sign it, or the
player unblocks it; on Linux build for an old enough glibc, or
statically.

## Writing one

```rust
use ddi_source_sdk::*;

struct MySongs;

impl Source for MySongs {
    fn info(&self) -> Info {
        Info::new("my-songs", "1.0.0", [Capability::Search, Capability::Fetch])
    }

    fn search(&self, cx: &Context, query: SearchQuery) -> Outcome<Page> {
        cx.progress(Step::Searching, None, None, None);
        Ok(Page::default())
    }

    fn fetch(&self, cx: &Context, item: &str, out: &mut Fetch) -> Outcome<Delivery> {
        cx.check()?; // cancelled?
        out.file("My Song/song.ssc", b"...")?;
        out.file("My Song/song.ogg", b"...")?;
        Ok(out.files())
    }
}

fn main() {
    ddi_source_sdk::main(MySongs);
}
```

- Operations run at once, each on a thread of its own, up to four.
  Check `cx.check()` or `cx.cancelled()` in anything long, and end within
  five seconds of a cancel.
- Errors are values: return a [`Failure`](src/lib.rs) with a `FailCode`
  the game words in the player's language, and your own words as
  `detail`, shown as data in a fold. Never put a sentence where a value
  belongs.
- Your text (your name, filters' and choices' labels) comes as a map of
  language tags to strings, English required, at most 128 characters
  each (512 for an item's text). The game shows it as data, framed by its
  own words.
- Deliver a fetched item as streamed files (`Fetch::file`), a zip
  (`Fetch::archive`), files written into the cache folder the game gives
  when the item is over its memory budget (`Fetch::cache_dir`, then
  `Fetch::cached`), or where it already is on the player's disk
  (`Delivery::InPlace`).
- `cx.dirs()` names a cache folder and a data folder of your own.

`tools/source-test-plugins/src/bin/example.rs` is a complete plugin over
generated songs, using every capability.

## Courtesy to the sites you read

The game cannot enforce any of this; your plugin can, and the players
and the sites are counting on it.

- **Make requests only when the player asks**: a search they typed, an
  item they chose. No crawling, no prefetching a catalog, nothing in the
  background.
- **Pace your requests per host**: one at a time, a second or more apart
  unless the site says otherwise; honor `429` and `Retry-After`.
- **Name yourself honestly** in your requests' `User-Agent`: your plugin,
  its version and the game, never a browser's.
- **Honor robots rules** for anything automated.
- **Be the site's own, or have its sanction.** The best plugin for a site
  is written by the site's maintainers; if you are not one, ask them.
- Keep what the site states of a song: its license and who to credit
  (`ItemMeta::license`, `ItemMeta::attribution`), and its page.
- Say how far its songs may go (`reach`, above) as the site wishes:
  `play` for a site that wants its songs played and nothing more.
