# A room's own songs

Status: stages 1 and 2 built (rounds RL1 and RL2, room model revision
17, 2026-11-19, released together); round DB's answers built (revision
19, 2026-11-20); stage 3 built (round RL3, revision 21, 2026-11-21);
stage 4 built (round RL4, revision 22, 2026-11-21).

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
3. **Everyone may add** (built, revision 21): a room setting "Who can
   add songs: Hosts / Everyone" (Hosts by default), beside "If the host
   leaves" in Host options; `RoomState::may_add` is its one check.
4. **Choosing by the group** (built, revision 22): "Song choice: Host / Group" (Host by
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

## Stage 3 as built (round RL3, room model revision 21, 2026-11-21)

### The model

- `RoomSongs::who_adds` (`songs::WhoAdds`: Hosts, the default, or
  Everyone), the owner's room setting (`Command::SetWhoAdds`, as
  `SetSongList` and If the host leaves: `Capability::ChangeAccess`;
  `Event::WhoAddsChanged`), kept in the list's row; left out of the JSON
  at Hosts, so a room stored by revision 20 reads as Hosts
  (`tests/room-revision-20.json`).
- `RoomState::may_add` reads it: the hosts (owner, co-hosts and
  administrators) always; under Everyone every person in the room (a
  bot too) who is not held to watching by the watch-only link. **A member
  who watches by their own choice may add** (decided here: adding is not
  choosing a stage's song, which stage 4 keeps to those who will play;
  someone who came by the watch-only link came to look, and stays out as
  for the hosts' own setting). Never a CPU or the audience (no member).
- `RoomState::add_refusal`: the reducer's whole decision on an addition
  (who may, the song's bounds, the same song, the person's half, a full
  list that refuses or has nothing it may remove), which the client asks
  before it sends (`RoomClient::add_song`, `add_blocked`), so a refusal
  is said at the press.
- **Removing**: as built in stage 1: whoever added a song removes it, a
  host or co-host anyone's; never one a stage not yet over names.
- **A full list under Remove the oldest**: the oldest song not in use,
  whoever added it (the simplest rule; the proposal had no fairness rule
  for it). One who is not a host is refused at their half before anything
  is removed, so they push out at most half the list. The alternative,
  removing the adder's own oldest first, is listed for the maintainer.
- **Back to Hosts**: nothing listed goes; an entry still uploading is its
  adder's to finish (its grant, a renewal and a retry are still given:
  the entry is the room's once listed); only new additions are refused
  ("Only the hosts can add songs to this room now."). The alternative,
  dropping the uploads under way, is listed.
- **Kick and leaving**: as before: the adder's songs that are there stay,
  their uploads under way go with them.

### The server

- The songs grant needs nothing new: an entry, and with it a grant,
  exists only where its adder might add it (the reducer's decision), and
  the grant is the entry's adder's alone.
- Upload grants (a stage's or an entry's, renewals included) are bounded
  30 a person a minute for a room (`GRANTS_A_MINUTE`, in the room's
  memory as tickets are; `429 too_many_grants`, shown as an upload that
  failed, with Upload again).
- What a room's songs take at once: the list's size (100 at most) songs
  of a share's size each (`share_max_mb`), the stages' and the
  spotlight's shares within `room_shares` (256); a removed song's share
  is deleted at once. The maintainer's 2026-11-19 answer said 512 MB a
  song; songs are bounded by a share's size (2 GiB unless the operator
  lowers it), as in stage 1 (a question for the maintainer).

### The screens

- Host options (the owner's): "Who can add songs" (Hosts, Everyone) above
  the list's size and When the list is full, the note under the three
  saying what they mean ("Only hosts and co-hosts add songs, and they may
  fill the whole list." under Hosts; "…anyone else may add up to N songs
  and remove their own." under Everyone).
- A member who may add reads My songs as a host does (Enter "Add to the
  room", the row's state while it uploads, Add this pack in More); the
  status line says their count against their half ("You've added 1 of
  the 2 songs you can add."), at their half "You've added all 2 songs
  you can. Remove one to add another.", and a full list that takes no
  more "The room's song list is full. A song must be removed first." (to
  a host too); a press past either is refused in a notice ("You've added
  the 2 songs you can add to this room. Remove one of yours to add
  another.", "The room's song list is full.").
- A row's line says who added it before its state (the state, with how
  many have it, is the long part, cut first on a narrow list). A member who may not add reads My
  songs as before (browsing only).
- Their own rows on Room songs say Added by you and offer Remove from the
  room in More (the host's way too).
- The status line and the players panel say nothing else new.

### Bots

`--everyone` (the host bot sets Who can add songs: Everyone with its
Rules) and `--add ID:N` (the Nth guest bot adds that song, from `--songs`
or the bundled songs, once the room lets it; `--add ID` alone stays the
host bot's). Guest bots alone (`--join`) take `--add ID:N` too.

## Stage 4 as built (round RL4, room model revision 22, 2026-11-21)

The maintainer's decisions of 2026-11-19 (Group a rule, Host by default;
only those who will play choose; no separate Draw: a host's or
co-host's own choice starts the countdown, the draw canceled where no
choice is left; a roulette among the members' choices, one entry each;
choices cleared after each draw; Repeats Allowed by default, Not in a
row and Not in a match as a rule) as built; what was this round's to
design is said with its alternative.

### The model (`crates/room/src/choice.rs`)

- **Where the rule lives**: two per-stage match rules,
  `MatchRules::choice` and `MatchRules::repeats` (and the same in
  `PlayRules`), so a change lands "from stage N" as Timing difficulty's
  does (`ChangeStageRules`, `rules_land_on`), or on the next match
  between matches. Left out of the JSON at Host and Allowed: a room
  stored by revision 21 reads as Host (`tests/room-revision-21.json`).
  Group in a room with no song list (CPU practice) is refused
  (`RulesProblem::GroupNeedsList`): practice has one person, nobody to
  draw among. The alternative, a room setting beside Who can add songs,
  would have meant one more kind of setting for something the stage
  plays by.
- **Who chooses** (`RoomState::choice_refusal`, `chooser_refusal`):
  a person present (a bot too) who plays the stage the choices are for
  (`choosing_for`: the stage a pick would take; while a stage is
  readied or played, the one after it, the next match's first after a
  match's last): seated in the running match, or to be seated in its
  next stage (joining during a match, a place free); before a match,
  anyone who plays. Never a CPU, a watcher (by choice or by the
  watch-only link), the audience, one out of the match or one waiting
  for the next match (`Rejection::CannotChoose { why }`).
- **A choice** (`Choice`: a listed song there to load, one of its
  charts, the revision it was made at; `Command::Choose`,
  `Event::ChoiceChanged`): one a player; another song moves it; none
  clears it. Choices may be made at any time the rule is Group, also
  while another stage is played (for the next).
- **The countdown** (`Draw`, `draw-v0`, `DRAW_V0`, PROVISIONAL): opened
  by the room itself (`settle_choices`, after every change and tick)
  where none runs, the room could pick now (`pick_stage`) and a host's
  or co-host's own choice stands (`Capability::StartDraw`,
  `draw_starter`: the earliest such choice names who started it), made
  while the room could pick. **A choice made while another stage is
  readied or played waits** (round FX, `Choice::waits`; before, an
  operator's choice made during another stage's play opened the
  countdown as soon as that stage was over, while the players were still
  on its results, at a moment nobody chose): it is a choice all the same
  (shown, counted, drawn among), but an operator's opens nothing until
  they make it again once the room can pick (Enter on their chosen song,
  or another song; `RoomState::choice_waits_for`). Clients only mirror
  the choice's flag, which the reducer alone sets, and a choice stored
  or sent without it reads as one that may start the draw: no new
  revision. **Ten seconds**, the proposal's number (long
  enough for a person who has not chosen to choose, short enough that a
  host's choice feels like a start); **early once every player who may
  choose and is there has a choice, never before three seconds** (so it
  is seen); a choice made or changed during it counts until it ends.
  Alternatives: a Pass action counted as chosen for the early end (not
  built: nobody asked for one, and a player who wants to pass simply
  lets the ten seconds run); a longer countdown for large rooms.
- **Canceled** (`Event::DrawCanceled { why }`): every choice gone
  (`Empty`: the operator clears a lone choice); the rule turned Host or
  the room no longer able to pick that stage (`Rules`: the match stopped
  or ended, a host started the stage with its song; since round RL4b
  the choices made for it go too, so that no countdown for a new match
  opens again by itself at the next change of any kind); a draw now
  could not start a stage (`NoStart`: too few players left, the next
  match's rules to review; the choices stand, and the countdown opens
  again as soon as a draw could start). **The operator who started it
  leaving** takes their choice: the countdown goes on while any choice
  stands, canceled when none does; **losing the role** leaves their
  choice and the countdown as they are. A chooser who leaves, is
  kicked, turns to Watch or is away past the reconnect grace takes
  their choice with them. A choice the room takes away when it was not
  its maker's own doing says why (round RL4b, `ChoiceGone` on
  `Event::ChoiceChanged`: Song choice turned to Host, the Repeats rule
  excluding the song now, the song no longer there, the draw called
  off), and their device says so once.
- **The draw** (`Command::Draw { roll }`, the authority's,
  `RoomState::draw_due`): the entries are the choices by participant id,
  one each; the winner is `roll % entries`, so a song two players chose
  has two chances; `Event::Drawn` keeps the record (`RoomState::drawn`,
  `DrawRecord`: entries, winner, roll, time); the choices are cleared;
  then the events of a pick of the drawn song as the picker's pick of a
  listed song (its share there at once, its readiness window open).
  **The randomness**: the roll is eight bytes the server's Worker makes
  at the countdown's end (`getrandom`, the platform's
  `crypto.getRandomValues`), which no device, the host's included, can
  know or steer before the choices lock; every device shows the record's
  result and computes nothing random of its own; a stored room keeps the
  roll with the entries. Tests: `LocalAuthority::set_draw_seed`,
  `FakeServer::set_draw_seed`, the server's `RoomCore::draw_seed`
  (`splitmix64` of the seed), so a test's draws are exact. The
  alternative, a seed known ahead (the room's and the stage's hash, as
  the live plan's), was refused: anyone could compute the winner and
  move their choice at the last second.
- **The Ready window after a draw** (round RL4b): the stage's Ready
  countdown counts from the end of the draw's reveal at the earliest
  (`RoomState::ready_from`, `DRAW_V0.reveal_ms` after the draw), so the
  three seconds every device spends on the reveal never come out of the
  window. Before, a song every device had downloaded ahead ended the
  loading grace at once and the window counted during the reveal: the
  players had 12 of the deployed 15 seconds (2 of the 5 an operator may
  set, `readiness_window_ms`'s least).
- **Under Group the picker does not pick** (`Rejection::GroupChooses`);
  a song somebody's choice names is in use (`song_in_use`: never
  removed, never made room for under Remove the oldest).
- **Repeats** (`RoomState::excluded_songs`): Not in a row, the song of
  the stage before the one chosen for (this match's; for a match's
  first, the last match's last); Not in a match, every song a stage of
  this match picked or played (a pick whose start was canceled, on the
  stage being chosen for again, counts for nothing). Choosing an
  excluded song is refused (`Rejection::Repeated { rule }`) and its row
  and Enter's hint say why. **Where the
  rule would leave no song to choose, it gives way** (every song may be
  chosen; Rules' note says so). Alternative: refuse every choice and
  ask the host to change the rule.
- **The audience** (`audience::PublicView`): the rule, each choice with
  what it names (its song's card: the audience has no list), the
  countdown with its end and the last draw with its entries' songs.
  Nobody of the audience chooses.

### The server

`RoomCore::tick` draws where the room says one is due (`RoomCore::draw`,
`random_u64`: the Worker's random bytes; natively for tools and tests
the standard library's keyed hasher, never in the Worker), and the
room's alarm is set to the countdown's end (`next_deadline`). The
session log says who opened a countdown (their seat), each draw (its
stage and how many choices) and each cancel (why), never a song.
Nothing new is stored but the room's state; nothing in `wrangler.toml`.

### The screens

- **Rules**: Song choice (Host, Group) with a sentence for each, and
  Repeats under Group with a sentence for each value (its values read
  Allowed, Not in a row and Once a match: "Not in a match" did not fit
  between the row's buttons; Once a match's sentence says "Each song
  plays at most once in a match: a song played in this match can't be
  chosen until the next match"), both per-stage rows ("From stage
  N"), in an online room only; the summary words say "Group song
  choice" and the repeats rule where they are not the usual.
- **Room songs** under Group: Enter is this player's choice ("Choose
  this song"; the host's and co-hosts' hint "Choose this song and start
  the draw"; on the song chosen "Take back your choice"; on a song the
  Repeats rule excludes "Can't choose: the last stage's song" or "…:
  already in this match", round RL4b); each row's
  line says who chose it first ("Chosen by you", "Chosen by bob and
  Beep", "Chosen by you and 3 more": two names at most, then a count),
  and a song the Repeats rule excludes "Last stage's song" or "Already
  in this match" (round RL4b: it may be picked and not played yet; and
  none of it while the device reveals a draw, which it would give away).
  A refused choice is said in a notice ("You're watching, so you don't
  choose: only players choose the next song.", "That song is the last
  stage's song: Repeats is Not in a row.").
- **The status line** (`room::group_status`): "Choose stage 2's song.
  Your choice starts the draw." to a host; "… The draw starts when a
  host or co-host chooses." to others; "3 of 4 chose stage 2's song. …" once some
  chose; "The draw in 7 s · 3 of 4 chose · started by alice" while it
  counts (its seconds counted on this device from when it saw the
  countdown open, or the room's clock where known: the same for
  everyone within the network's delay); "The draw waits until a stage
  can start: more players are needed." where a host's choice stands
  and a draw could not start (or "The draw waits: a host must check the
  next match's Rules first." where the next match's rules are what
  stops it); "… players chose …. Only players choose." to a watcher.
  **A host's or co-host's choice that waits** (round FX) reads "2 of 3
  chose stage 2's song. Your choice waits: press Enter or tap your song
  to start the draw." to them, Enter's hint on that song "Start the
  draw" (instead of "Take back your choice"), and "… The draw starts
  when a host or co-host starts it." to the others who may not start it.
  **A host or co-host who watches** (round RL4b) has no choice, so
  their choice cannot start the draw (the maintainer's decisions: only
  those who will play choose; a host's or co-host's own choice starts
  it): they read "You're watching. To start the draw, switch to Play and
  choose a song, or ask a co-host." ("…, or ask a co-host" only where a
  co-host plays). **Where every host and co-host watches or is away**
  nobody can start a draw, and the players read "2 of 3 chose stage 2's
  song. The draw needs a host or co-host who plays." **A choice the room
  took away** is said to its maker on the status line for 12 seconds
  ("Your song choice was cleared: the host picks the songs now.", "…:
  the Repeats rule excludes that song now.", "…: that song can't be
  played any more.", "…: the draw was called off.";
  `LinkNotice::ChoiceGone`); the one whose own change took it is not
  told. **A late joiner** sees the choices and the countdown at
  once from the room's snapshot and may choose at once where they will
  play.
- **A phone**: the same rows and status line; its Play button reads
  what Enter does ("Choose this song"); the reveal fitted to the screen.
- **The audience's page** says the choosing in its line ("2 of 4
  players chose stage 2's song. Only players choose.", the countdown)
  and, once drawn, "Beep's choice: Sweeteners · Hard 8", and shows the
  members' reveal over the page (round RL4b: `AudienceClient::reveal`
  from the public view's draw, `room_reveal::open_audience_reveal`, the
  same reel and three seconds; meanwhile the page's line says "Stage 2 ·
  The draw" and the song's banner waits, so nothing behind the reel gives
  its result away). Beside each player's name the page says the song
  they chose ("Chose Sweeteners"), as the members' rows say who chose
  what.
- **The players panel** marks who has chosen (a tick, `Mark::Chose`;
  "Chose a song" in the pop-over); the picker's note is not shown under
  Group.
- **The reveal** (`screens/room_reveal.rs`, layer `room-draw`): over the
  room's song select for everyone, members watching included, three
  seconds (`DRAW_V0.reveal_ms`) from when the device saw the draw: a
  reel of the entries ("Beep · Sweeteners · Hard 8"), in the draw's
  order, spinning upward and slowing (a cubic ease-out over the first
  three quarters) to stop the drawn one in the middle band, gold; then
  "Beep's choice: Sweeteners · Hard 8". Its motion comes from the record
  alone (`reel_at`), so every device shows the same reel; skippable by
  nothing; under reduced motion the drawn entry stands in the band with
  the line at once. The stage's Ready card follows (`RoomClient::reveal`
  holds the prompt back meanwhile); the readiness window opens at the
  draw on the room's clock (the song may load meanwhile), and its Ready
  countdown counts from the reveal's end (round RL4b; before, the
  reveal cost three seconds of it).
  **Candidates**: A, cards in a row with a highlight hopping along them
  (the proposal's), and B, the reel (built), are drawn side by side in
  `target/scratch/mp/rl4/shots/reveal-candidates.png`. B was built: it
  reads as a roulette at any number of entries (A needs a grid past
  six), every line names the chooser with the song, and its motion is
  one value.

### Bots

`--group` and `--repeats allowed|row|match` (the host bot's Rules), and
`--choose random|first|none` (each guest bot's choice, a moment after
the room can take one, each a little after the one before; random by
default, from `--seed`). Under Group the host bot chooses the song it
would have picked; its choice starts the draw (one that waits, made
during a stage, it makes again; round FX). Every bot says how many
chose, the countdown opening and ending, and each draw.

## Not in stage 1

Bests on songs not in one's library (a downloaded song's row has none,
nor NEW).
