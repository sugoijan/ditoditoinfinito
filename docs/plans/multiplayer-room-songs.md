# A room's own songs

Status: stages 1 and 2 built (rounds RL1 and RL2, room model revision
17, 2026-11-19, released together); round DB's answers built (revision
19, 2026-11-20); stages 3 and 4 planned.

The maintainer's request (2026-11-19): "a separation step between the
local song list and the room's song list, they would still look the same
but now the room has a song list that is shared and updated in real time,
that way songs can be pre-added and pre-loaded, and also we can support a
rule where any member can add songs the room, and another rule so that
picking a song can be done through as "by the group" instead of by an
individual". The design round's proposal and the maintainer's answers to
its ten questions (2026-11-19) are summarized here; where they differed,
the answers won.

## The four stages

Each is a room model revision of its own; server first, as always.

1. **The room's list** (built, revision 17): the list in the room's state,
   hosts and co-hosts add and remove, uploads when added, two tabs on the
   room's song select, picks from the list.
2. **Downloading ahead** (built, revision 17 too): everything in the list
   by default (never movies), a per-device Options row (Everything, Likely
   next, Off), the rows filled in and who has what over the live side
   channel, cleanup when the room is left.
3. **Everyone may add**: a room setting "Who can add songs: Hosts /
   Everyone" (Hosts by default), beside "If the host leaves" in Host
   options; `RoomState::may_add` is its one check.
4. **Choosing by the group**: "Song choice: Host / Group" (Host by
   default), a per-stage rule; members mark choices at any time; a host's
   or co-host's own choice starts the countdown; the draw cancelled if the
   choices all go; choices cleared after each draw; everyone sees who
   chose what; only those who will play choose; repeats Allowed by default
   (Not in a row, Not in a match as a rule).

## Stage 1 as built

### The model (`crates/room/src/songs.rs`)

- `RoomState::songs: Arc<RoomSongs>`: entries oldest first, the next id,
  the list's rules and the revision of its last change; behind a shared
  pointer so the copy every command is decided on does not copy it (a
  command that changes it copies it once). Left out of the JSON while a
  room never had one: a room stored by revision 16 reads with an empty
  list and writes back what it read (`tests/room-revision-16.json`).
- An entry (`RoomSong`) is a whole song named by its content: the
  `ddi_share_protocol::song_key` of its original charts' digests (the
  versions the game makes left out), made by the adder's device and again
  by the server from the uploaded record. Its card (title, artist, pack),
  its original charts (digest, style, difficulty, level), who added it and
  when, its status (uploading, there, or not the song added), its share,
  and, in a room stored before revision 19, whether it is a bundled song
  listed with nothing uploaded (round DB: a bundled song is uploaded as
  any other since; such an entry still reads and plays from each device's
  own copy, and a device without it cannot load it).
- Commands: `AddSong` (who may: `RoomState::may_add`, the hosts in this
  stage: `Capability::AddSongs`), `RemoveSong` (the adder or a host; never
  a song a stage not yet over names: `SongInUse`), `SetSongList` (the
  owner's: the list's size, 1 to 100, and When the list is full: Remove
  the oldest, the default, or Refuse new songs), the authority's
  `SetSongContent` (what the server made of an upload), and `PickListed`
  (the picker's pick of a listed chart: the stage's song there at once
  with the entry's share; `StageContent::song` names the entry).
- Bounds: at most 100 songs (`MAX_SONGS`); a person at most half the
  list's size, hosts, co-hosts and administrators exempt: they may fill
  the whole list (`RoomState::song_quota`; round DB, 2026-11-20, the
  maintainer: "Hosts and co-hosts are exempt"; Host options' note says
  it); at most 32 original charts an entry, styles and
  difficulties 32 bytes, keys 64 bytes: a full list at its worst is about
  860 KB, typical lists 720 B an entry.
- An adder who leaves, or is away past the reconnect grace, takes the
  entries whose upload was not done with them; their songs that are there
  stay (the room's).
- The audience's public view carries the list's count only.
- `Command::Pick` without an entry (revision 16's, a pending pick shared
  after it under a stage's grant) is still taken: no screen of revision 17
  makes it in an online room, and CPU practice, which has no list, still
  does.

### The server (`../ddi-server`)

- `POST /rooms/<id>/songs/grant {song}`: the entry's adder's grant
  (`UploadGrant::song`, its content key the song's), one share a grant as
  before; the share's room tie names the entry.
- A finished share of an entry tells the room (`/song-ready`) the
  record's original charts; the room checks they make the song's key:
  the entry then there with the record's charts, else "not this song" and
  the share deleted.
- The list is a row of its own (`songs`), written only when the list
  changed.
- A song removed (or made room for) has its share deleted at once; the
  room's end deletes all its shares as before.
- A room's shares are kept while it lives: each report the room sends the
  registry (a minute while anyone is connected, on any change) moves their
  expiry to a day from then (`ROOM_SONGS_KEPT_SECS`; round DB, 2026-11-20,
  the maintainer: "kept a day"; the room's idle life, a week, before). A
  room gone without a word leaves them at most that long; a room whose sockets
  all close ends two minutes later and deletes them.
- The index finds a room's shares by an indexed `room_id` column.
- `room_shares` (a room's shares at most) 64 → 256.

### The screens

- An online room's song select has two tabs over its list: **Room songs**
  (where it opens, the same for everyone, one flat list) and **My songs**
  (the player's library, its packs, favorites and place). M switches them;
  the tabs are buttons; More has the other tab too (a controller's West, a
  phone). Each tab keeps its own place.
- On Room songs the one who chooses picks (Enter: "Pick for stage N");
  each row's line says the artist, the entry's state for this player (On
  this device, Loading, Not loaded yet, Uploading, Upload failed) and who
  added it; the picker's resting song is said first on its row ("alice is
  on it"). An empty list says "Add songs from My songs" to who may add and
  "The host hasn't added any songs yet" to others.
- On My songs Enter adds the song to the room (the footer says "Add to the
  room", or "In the room" where it is listed already); for others it is
  browsable only.
- Nobody follows the picker's resting song any more; the spotlight's
  upload ahead is gone (the list's uploads replace it), the spotlight is
  the mark on an entry.
- More on a room song: Remove from the room (its adder, a host), Upload
  again (a failed upload of this device's).
- Host options (the owner's): Room songs at most (5, 10, 20, … 100) and
  When the list is full, with what they mean.
- CPU practice keeps its own list as before (no room list).

### Bots

`--add ID,…`: the host bot adds those songs to the list at once, then
picks from the list in its order; with only `--pick` (or neither), each
song it will pick is added to the list first and picked once uploaded.
Every bot says the room's list as it changes.

## Stage 2 as built (round RL2, still revision 17)

Nothing had been deployed at revision 17, so stage 2's additions to the
wire (two live messages) stay in it; peers that both speak 17 agree.

### The plan (`crates/ui/src/state/room/preload.rs`, `room-preload-v1`, PROVISIONAL)

- Each member's device downloads the listed songs its own library does
  not have (a song whose original charts' digests it has, a bundled song
  it has, needs nothing) into a store of the room's own
  (`RoomClient::store`, the room's id; a bot's `<room>-<name>`), as
  `PreloadJob`s its shell runs: first every song's **listing** (the
  share's list and the banner, small: the rows fill in), then the songs'
  chart, music and pictures, **never a movie** (`is_small`), two at once.
- Order: a stage's song not yet played, then the song the picker is on,
  then the newest added, then the rest.
- **Never from a stage's countdown to its end** (`Countdown`, `Stage`,
  `SongCompletion`): a download under way is stopped (`Stop`) and taken
  up again after; while a stage gathers readiness only its song; not
  while this device uploads. A hidden page is not paused: a browser does
  not slow a download under way, and the next song starts at the hidden
  page's next run of the room (a second or a minute apart).
- Retries: `load-retry-v0`'s waits (2 s, 5 s), then the song waits for
  the next connection.
- A song removed is deleted from the store (`PreloadJob::Forget`); the
  whole store when the room is left or ends.
- A stage's own load of a listed song goes into the same store, so it
  counts as downloaded after, and a song downloaded ahead loads from it.

### The option

Options → Data, "Room songs ahead": Everything (the default), Likely next
(the picked song, the picker's, and the newest up to eight songs and
128 MB, `room-preload-v1`; round DB, 2026-11-20, the maintainer; four and
64 MB before, `room-preload-v0`), Off (a song is fetched when picked, as in stage 1). Per device
(`Settings::room_ahead`, none stored: the device's own default): on the
web Likely next where `navigator.connection.saveData` is set or the
screen's short side is under 600 CSS pixels, else Everything; desktop
Everything. The description says the default.

### Where it is kept

- **Web** (`crates/web/src/room_store.rs`): the library's IndexedDB
  `files` store under `room/<store>/<share>/<path>`, never a song of the
  library nor of the history. Every read of a shared song looks there
  first (chart, music, pictures, preview, the share's list). A tab
  touches its room's area each minute; at the next start an area nobody
  touched for 30 minutes is deleted (a crash, a closed tab), the room the
  page opens on kept. Out of storage (the quota, or
  `window.__DDI_ROOM_QUOTA_BYTES` in tests): Likely next from then, said
  once on the status line. `window.__DDI_ROOM_STORE`,
  `__DDI_ROOM_STORED` for tests.
- **Desktop** (`crates/desktop/src/share.rs`): the cache folder's
  `rooms/<store>/<share>/`, deleted when the room is left; the run that
  opens the window deletes `rooms/` at its start (rooms left by a crash).

### The rows

A listed song this device has (its library's, or downloaded) is drawn as
any song: its banner, length, BPM, notes, charts, and its preview from
the copy here; where it is the player's library's, its best's grade or
NEW as that library's row (round DB), none where it is here only from
the room (`SongEntry::room_only`). One not here shows its banner as soon as its listing is
read, its state ("Downloading 41%", "Not loaded yet", …), no NEW and no
note counts (`SongEntry::away`); the song side says what the list's
entry says. While not everyone has it, the row adds "11 of 12 have it".

### Who has what (`ddi_room::live::have`, `room-have-v0`, PROVISIONAL)

`ClientLive::Have { songs }` (ranges of entry ids, at most 64) after hello
and when it changes, at most every five seconds; the server keeps it on
the socket's attachment (never the room's state: nothing is written) and
says the member's set to every live socket (`ServerLive::Have`),
everyone's to a socket that says hello; a socket that closes takes its
set with it. The players counted are the people and bots connected who
play. The picker's Enter on Room songs says "Pick for stage N (2 players
don't have it yet)" before the pick. The readiness window needs nothing
new: `hosted-deadline-v2` already ends its loading wait once everyone
connected has loaded, and a song downloaded ahead loads from this device.

### Stage 1's gaps closed

- The status line on My songs says what Enter does there ("Add songs to
  the room here. Pick the next song on Room songs.").
- More on My songs: Add this pack to the room (N songs), up to the
  person's share and, where the list refuses new songs, its room; the
  status line says how many are uploaded.
- The session log records each add (the adder's seat) and each removal
  (why), never which song.
- The web's fake room (`mpfakeroom`) lists its song when the player is
  in and picks from the list; the audience's fake seat (`mpfakeseat`)
  keeps picking as before (the audience sees only the list's count).
- Bots download ahead as a device does (`--no-ahead` for load tests) and
  say what they have, how many everyone has, and when everyone loaded a
  stage's song.

## Not in stage 1

Bests on songs not in one's library (a downloaded song's row has none,
nor NEW);
"Who can add songs" (stage 3); choosing by the group (stage 4).
