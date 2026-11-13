# The audience: watching a room beyond its members

Status (2026-11-13): designed; the contract for the recommended path
(relays) is built and tested in `crates/room` (`ddi_room::audience`,
`audience-v0`, PROVISIONAL) with the fake room running it; the server's
side is built and tested in `../ddi-server` (not deployed; "The server, as
built" below); nothing of it is in the game yet.

Read with [Online play: the shared base](multiplayer-online-play.md) (the
live contract, its frames, the load tests and "Platform facts") and
[the prototype's progress](multiplayer-prototype-progress.md) ("Watching
a stage", "Joining a room full of players", "A watch-only link", "Rooms of
1+72 players").

## Why

The maintainer (2026-11-12): "for 'watch only' I'd like to have no limit
(at least within reason of what the Cloudflare deploy can work with)."

A room's members are bounded (`RoomPolicy::DEFAULT_MAX_MEMBERS`, 384: a
field of 73 and about 300 who watch). Every member is a row of the room's
state, written on every change and sent whole in every snapshot; every
membership change is an event to every member's socket, so a wave of `n`
joins costs `n²` sends; and the room's one object sends every member a
frame ten times a second. The server's load test (73 competitors, 300
watching sockets, `live-v2`) used about half a core of the local runtime
and sent 6,800 messages a second; the deployed platform may be two to
three times slower. Watching without a member limit therefore cannot be
"more members". It is **the audience**: people who only watch, **counted,
not listed**, read only, costing the room as little as possible, its
players' stage never slower for them.

## Who is audience, and how they arrive

**The rule (recommended, `audience-v0`):**

- A room has a third link beside its invite and its watch-only link: the
  **audience link** (`<room id>.<secret>`, its secret a one-way hash of
  the invite's under a tag of its own, as the watch-only link's is; it is
  replaced with the invite). Whoever opens it is audience, always. It is
  the link a host posts where many will see it (a stream's chat, a
  community's channel).
- The **watch-only link stays as built**: a named member held to watching
  whom the owner can let play. That is for people the host knows.
- **At the member bound**, a join by the invite or the watch-only link is
  offered the audience instead of a plain refusal: "The room is full: you
  can watch with its audience." (the invite page's Watch button then joins
  the audience). Nobody is refused who could watch.
- An audience seat has **no name in the room**, no row in the room's
  state, no event when it comes or goes; it **sends nothing** but its
  hello, a resume after a gap and a keep-alive. It cannot be promoted: to
  play, a person opens the room's invite (or asks the host for it) and
  joins as a member, which is the existing path.

What the other choices cost:

| Rule | For | Against |
| --- | --- | --- |
| Watch-only link always gives an audience seat | No third link | Loses what was just built (named watchers the owner can let play); a host could no longer invite a friend to watch and then play |
| Audience only once members are full | No new link | A public room fills its 384 member rows (every row in every snapshot and write) before anyone is audience: the cost the audience exists to avoid |
| Host promotes someone from the audience | Feels natural | The audience is not listed: the host has no one to pick without listing it (names, identities in the room: exactly what is left out) |
| **A separate audience link, and the audience offered at the member bound** | The member model untouched; the room's cost flat in the audience; honest (the link says what it gives) | One more link for the host to copy |

**Sign-in (recommended: signed in, for now).** The prototype signs
everyone in (Twitch). An audience seat signed in is bounded per identity
(two seats, `seats_per_identity`), which bounds one device opening many
seats without any central count of addresses. An anonymous audience is
cheaper to join (no Twitch round trip) and harder to bound: one person
could open thousands of seats. The contract leaves the door open: a seat
is admitted for an *identity string*; an anonymous one could be
`anon:<hash of the connecting address and a day>` given by the Worker,
bounded per address the same way (Cloudflare gives the Worker
`CF-Connecting-IP`). Open question 1.

## What an audience device needs to draw the stage

| Need | Where it comes from |
| --- | --- |
| The room as it is shown: name, host, phase, the match's rules, the players and their kind (person, bot, CPU with its level), each stage's song card, chart digest, share and battle plan digest, starts, results and the starters' reported details, the match's standings | The **public view** (`audience::public::PublicView`), derived from the room's state and its name, sent by the room to its relays when what it shows changes (at most once a second), by a relay to each seat on its hello and on a change. About 3.5 KB as JSON for a room of 73. |
| The stage's song | The room's **share** (its id is in the view once available), read by anyone with the id, as every share is. Its chart is found by the digest. |
| The live frames | **Audience frames**: the room's own member frames coalesced four a second into one frame of the audience's numbering, with the audience's count, built once and encoded once by the room, the same bytes from every relay to every seat (`audience::frames`). An audience device reads them with the members' own `FrameReader` and draws them with the same feed (`ddi_session::battle::feed`). |
| Where the stage stands on arriving mid-stage | A relay keeps the stage as a snapshot folded from the frames and the last 30 s of frames: a seat's hello is answered with the view, the count, the snapshot and the last two seconds of frames, with no request to the room. |
| The room's clock (to start the music) | No pings: each audience frame says when the room built it, so the device estimates the room's clock from the largest `built − arrived` of the last minute (`ArrivalClock`: the offset less the quickest trip down, tens of milliseconds). The device draws everything, music included, the audience's delay behind the room's song time, so that error is never seen. |
| How many watch | In every audience frame during a stage; a `count` message between stages (at most every five seconds). |
| The results' Standings | In the view: each stage's result and the starters' details, and the match's standings. |

**Nothing a member alone knows is in the view** (tested,
`the_public_view_says_nothing_a_member_alone_knows`): no identities
(`twitch:…`, `bot:…`), admin flags, roles, connections, readiness or
loading (nor their failure codes), targeting, intents, members who only
watch (they are not listed to the audience at all), the watch-only mark,
the invite's generation or admission, the picker, the spotlight, the next
match's draft, or the song keys of the operators' libraries. A member's
connection change or someone joining to watch moves the room's revision
and leaves the view as it was (`PublicView::same_as`): nothing is sent.

## How frames reach thousands

### The platform (checked 2026-11-13)

From Cloudflare's documentation (and, where marked, the plan's "Platform
facts"):

- A Durable Object accepts at most **32,768 hibernatable WebSockets**, "the
  CPU and memory usage of a given workload may further limit the practical
  number" ([state API](https://developers.cloudflare.com/durable-objects/api/state/)).
- **A soft limit of 1,000 requests a second** an object
  ([limits](https://developers.cloudflare.com/durable-objects/platform/limits/));
  one object is one thread with 128 MB an isolate
  ([Workers limits](https://developers.cloudflare.com/workers/platform/limits/)).
- **Billing** ([Durable Objects pricing](https://developers.cloudflare.com/durable-objects/platform/pricing/)):
  requests $0.15 a million (HTTP requests, RPC sessions, WebSocket
  messages, alarms); incoming WebSocket messages count 20 to 1; **outgoing
  messages are free**; duration $12.50 a million GB-s, wall time at 128 MB
  while the object is active and not eligible to hibernate.
- **Auto-responses** (`setWebSocketAutoResponse`) answer a fixed message
  "without waking WebSockets in hibernation and incurring billable
  duration charges" ([state API](https://developers.cloudflare.com/durable-objects/api/state/)):
  the seats' keep-alive.
- A Durable Object **can be a WebSocket client** of another; **outgoing
  WebSockets do not hibernate** and keep the object in memory
  ([WebSockets best practices](https://developers.cloudflare.com/durable-objects/best-practices/websockets/)).
  Up to 10 tags a socket (`getWebSockets(tag)`).
- **Workers** ([pricing](https://developers.cloudflare.com/workers/platform/pricing/)):
  $0.30 a million requests after 10 million; "since Cloudflare Workers runs
  before the Cloudflare cache, the caching of a request still incurs
  costs", and cached responses on a Worker's route are billed per request
  ([how the cache works](https://developers.cloudflare.com/workers/reference/how-the-cache-works/):
  "Cloudflare Workers run before the cache"; the Cache API "is local to a
  data center").
- **R2** ([pricing](https://developers.cloudflare.com/r2/pricing/)): a
  write (Class A) $4.50 a million, a read (Class B) $0.36 a million, egress
  free; a bucket's custom domain can be cached by Cloudflare without a
  Worker ([public buckets](https://developers.cloudflare.com/r2/buckets/public-buckets/):
  "Domain access through a custom domain allows you to use Cloudflare
  Cache"; `r2.dev` is rate-limited, for development only).

### The three ways

**(a) Audience sockets on the room's own object, up to a bound.** Simplest:
a socket of its own kind on the room, sent the coalesced audience frame.
The room is already the busiest object: 73 competitors and 300 watching
members are 6,800 sends a second and half a local core. At four audience
frames a second, 1,000 seats add 4,000 sends a second to the same thread
(and their keep-alives to its incoming requests); the room's tick, which
every player's attacks wait on, would slow with the audience. The room's
cost is not flat. **Bound: a few hundred at most, beside a full field.**

**(b) Relay objects (recommended).** A relay is a Durable Object of its
own (`AudienceRelay`, named `<room id>:<k>`) holding up to 500 seats' sockets
(hibernatable, tagged) and **one socket up to the room**, which the relay
opens (a Durable Object as a WebSocket client; the room accepts it as a
hibernatable socket of a kind of its own, not a member's). The room sends
each audience frame **once per relay** (an outgoing message: free); the
relay sends the same bytes to its seats (outgoing: free).

- *Found and filled*: the Worker's audience join hashes the identity to a
  home among the first four relays (`home_relays`), and tries the
  following relays in a fixed order while one answers full
  (`AudienceRules::relay_order`). A small audience starts four relays,
  never the bound's forty; relays past them open as the first fill.
- *What a relay holds*: its seats (their number and the identity's mark,
  also in each socket's attachment), who may be admitted (the audience
  link's key, open or not, its seat count: from the room), the view, the
  count, the stage as a snapshot folded from the frames, the last 30 s of
  frames (`audience::relay::Relay`, sans-IO). Nothing is stored.
- *When one restarts*: its seats' sockets stay connected (hibernation),
  their seats in their attachments (`Relay::restore`); its socket up is
  gone, so on the next message or its alarm it opens another and says
  which frame it has (`RelayUp::Hello { after }`); the room sends the
  frames after it, or the stage's snapshot and its last frames. A seat
  that saw a gap asks its relay to resume, as a member's device does. A
  forced update of a running object closes its seats' sockets; they come
  back by the audience join (a new ticket). Tested in the fake
  (`a_relay_that_restarts_mid_stage_gives_its_seats_the_stage_again`).
- *The count back*: each relay says its seats up when they change, once a
  second at most (`RelayUp::Count`); the room sums them, tells its members
  (`ServerLive::Audience { count, open }`) and the relays
  (`RelayDown::Count`) once a second at most, and every audience frame
  carries the total. The count is never a row of the room's state, never a
  revision, never an event: it cannot make an operator's edit stale.
- *The cost to the room*: one socket and four sends a second per relay
  (forty at the bound: 160 sends a second, a twentieth of the 3,000 that
  300 watching members cost now), at most one incoming message a second per relay, and
  building and encoding one audience frame four times a second (56–86 µs
  natively at 73 competitors, below).
- *The cost of a relay*: 2,000 sends a second at 500 seats; its reading of
  a frame 5–45 µs natively; the platform's own cost of a send is the
  server's to measure (the load test's 475 ms of local CPU a second for
  about 7,700 messages a second in and out suggests at most 60 µs a
  message all included: 0.12 of a local core for a full relay, perhaps
  0.3 deployed). The relay's duration: 0.125 GB × 3,600 s = 450 GB-s an
  hour, $0.0056 an hour (an outgoing socket keeps it active).

**(c) Segments read through the cache.** The room writes a second of
audience frames at a time where a cached route serves them, the devices
poll for the next. Through the Worker's own route every poll is a Worker
request billed per request (the Worker runs before the cache): 20,000
devices polling once a second are 72 million requests an hour, **about
$21.60 an hour**, against about $0.25 for relays. Only from a separate
cached hostname (an R2 bucket's custom domain with "Cache Everything" and
a short-lived index object) do the polls cost nearly nothing: then the
room writes one object a second ($0.016 an hour), each data center reads
each segment once. But: a new hostname and cache rules to keep, a delay
of the segment's length plus the write plus the poll plus the cache's
propagation (two to four seconds), no count (a cache hit reaches nothing:
counting needs a beacon through the Worker), no bound (anyone with the
URL reads, the room cannot close it) and no "audience is full".

**Recommendation: (b), relays**, for the prototype and well past it: the
room's cost is flat in the audience (one more socket and four sends a
second per 500), the delay is the members' (well under a second over
the network), counting and closing are natural (sockets), and nothing on
the members' path changes. **When the next becomes necessary**: relays
scale linearly to the bound (40); past about 50,000 a room (100 relays:
400 sends a second from the room, as many sockets), add a second tier
(relays of relays: the same `Relay`, its upstream another relay) before
the room's own sends matter; segments (c) on a cached hostname only for a
broadcast of hundreds of thousands, where a few seconds of delay and an
unbounded, uncounted audience are acceptable.

## A coarser stream

**Frames coalesced four a second** (`frame_ms` 250): the room's frames
come ten a second; the audience's merge two or three of them (each
competitor's moments in order, the last acknowledgement and judged time,
any end; the attacks and checkpoints in order; the last settling). Sends
drop by 2.5 times, bytes barely (a frame's fixed part is small).

**The delay**: an audience device draws everything, music included, the
audience's delay behind the room's song time, adaptive as a member's
views (`ViewDelay`), at most two seconds (`presentation_delay_ms`).
Measured in the fake (73 competitors, 2,000 seats at 20–120 ms each way,
the members at 20–80): a moment reached a seat 367 ms after its song time
at the median, 542 ms at the 99th percentile, 661 ms at most.

**Full fields for all, not filtered**: every audience frame carries every
competitor's moments. Per-socket filtering (full moments for the grid's
page or the followed player, summaries for the rest) would make the
relay encode per seat, and its sends, not its bytes, are its cost. The
numbers (measured frames, `what_the_audience_costs`; four frames a second;
the bytes are free on the platform):

| Players | Watching | Relays | Audience frame | Per seat | Sends a second (all relays) | Bytes a second out (all relays) |
| --- | --- | --- | --- | --- | --- | --- |
| 9 | 1,000 | 4 | 504 B | 2.0 KB/s | 4,000 | 2.0 MB |
| 9 | 5,000 | 10 | 504 B | 2.0 KB/s | 20,000 | 10 MB |
| 9 | 20,000 | 40 | 504 B | 2.0 KB/s | 80,000 | 40 MB |
| 73 | 1,000 | 4 | 4.0 KB | 16 KB/s | 4,000 | 16 MB |
| 73 | 5,000 | 10 | 4.0 KB | 16 KB/s | 20,000 | 80 MB |
| 73 | 20,000 | 40 | 4.0 KB | 16 KB/s | 80,000 | 320 MB |

16 KB a second (128 kbit/s) is little for a device watching 73 players;
paging would save about half of it (12 fields full and 61 summaries:
about 6 KB/s) at the price of a frame per page per relay, so it waits for
a measured need (a channel per grid page, tagged sockets, is the way in:
`audience-v1`).

## Bounds, abuse and cost

| Bound | Value (`AUDIENCE_V0`, PROVISIONAL) | At it |
| --- | --- | --- |
| A room's audience | 20,000 seats, 40 relays of 500 | "The room's audience is full." (`audience_full`, 409) |
| Seats an identity | 2 | `too_many_seats` (409): checked on the relay that holds its seats (its home, or one past it while that is full) |
| A seat given and never taken | dropped after 60 s | its socket (if any) closed |
| The host closed the audience | every seat closed (4403), joins refused | `audience_closed` (403) |
| The room ended | every seat closed (4410) | `room_closed` (410) |
| A link not the room's (or replaced) | | `audience_link_unknown` (404) |
| A room on a server without an audience | | `no_audience` (404; the policy has no `audience`, so the game offers none) |

What the server must count to keep it bounded: each relay its seats and
its identities' seats (in memory, rebuilt from attachments), the room the
sum of its relays' counts (in memory). Nothing central counts addresses
or seats per room beyond that sum.

What an audience costs the maintainer an hour of a stage (list prices
above, after the included amounts; a relay active the whole hour, the
seats' keep-alives every 30 s counted as requests at 20 to 1 in the worst
case; frames into relays at 20 to 1):

| Watching | Relays | Relays' duration | Keep-alives | Frames in | Joins (Worker + relay + socket) | About |
| --- | --- | --- | --- | --- | --- | --- |
| 1,000 | 4 | $0.0225 | $0.0009 | $0.0004 | $0.0008 | **$0.025 an hour** |
| 5,000 | 10 | $0.056 | $0.0045 | $0.0011 | $0.004 | **$0.07 an hour** |
| 20,000 | 40 | $0.225 | $0.018 | $0.0043 | $0.016 | **$0.26 an hour** |

Against the room itself (about 7 cents an hour for 73 competitors under
`live-v2`): a full audience is about four rooms' worth.

## What the room's people see

The audience's count on the room's screens and nothing else of it: "132
watching" on the room's status line and the stage's status line
(`LiveLink::audience`), from `ServerLive::Audience`. The **owner can
close the audience** (and open it again): one live message,
`ClientLive::Audience { open }` (the owner's, as the room's access is:
`Capability::ChangeAccess`); every seat is closed with 4403 and told why
("The host closed the audience."), joins are refused while closed. It is
kept by the room (`AudienceDurable`, stored beside the live part), not in
the room's state: no revision, no event.

## Older peers and deploys

- **A new game against a server without an audience**: the room's policy
  has no `audience`; the game shows no count, no audience link and no
  close control, and an audience link opened in it (or a member join at the
  bound) is refused as before (`no_audience`, `room_full`).
- **An older game in a room with an audience**: `RoomPolicy::audience` is
  an unknown field (ignored); `ServerLive::Audience` is a live message it
  cannot read (passed over, as any unknown text). It sees no count. An
  audience link opened in it is not a link it knows (its invite page
  refuses it plainly).
- **A server deploy mid-stage**: a relay object restarted keeps its seats'
  sockets and gets the stage again from the room; a room restarted mid-
  stage loses its live part (as today): the relays get no frames for that
  stage, the seats keep the view and see the result when the room settles.
- **The members' protocol is untouched** but for the two live messages
  (additive), so nothing a member's device or the room's reducer reads
  changes; the members' frames are the same bytes at the same times with or
  without an audience (tested, `the_members_stream_is_the_same_with_an_audience`).

## The contract (`crates/room`, `audience-v0`)

- `audience.rs`: `AudienceRules` (`AUDIENCE_V0`: 250 ms frames, 20,000
  seats in relays of 500, four home relays, two seats an identity, the
  count once a second, the view at most once a second, two seconds of
  delay at most, 30 s kept, a 30 s keep-alive, a 60 s ticket),
  `relay_order`; `AudiencePolicy` (`RoomPolicy::audience`, additive;
  `RoomPolicy::with_audience`); the refusals with their codes and statuses
  (`AudienceRefusal`), the ends and close codes (`AudienceEnd`, 4403,
  4410); the JSON messages, `{"v":2,"audience":…}` between a seat and its
  relay (`AudienceUp`: hello, resume; `AudienceDown`: view, count,
  snapshot, ended) and `{"v":2,"relay":…}` between a relay and the room
  (`RelayUp`: hello with the view's revision and the last frame, count;
  `RelayDown`: admission, view, snapshot, count, ended); the keep-alive's
  text (`ping`, answered `pong` by the host).
- `audience/public.rs`: `PublicView::of(state, name)`, deterministic;
  `same_as`.
- `audience/frames.rs`: the audience frame (`AUDIENCE_FRAME_V0`, `0xA0`,
  the count, the frame as `live::wire` writes it: a member's reader
  refuses it), `coalesce`, `Coalescer` (the room's: frames gathered, one
  built and encoded per period, kept), `fold` (a relay's snapshot moved on
  by a frame), `ArrivalClock`.
- `audience/hub.rs`: `AudienceHub`, the room's side, sans-IO: a relay's
  hello and counts, the members' frames, the owner's open and close, the
  tick (the view when changed, the count when changed, the frames, a new
  stage's snapshot first, the room's end); `HubOut` (to one relay, to all,
  to the members); `AudienceDurable`.
- `audience/relay.rs`: `Relay`, sans-IO: `admit`, a seat's hello and
  resume, its going, the socket up opened and closed, the room's text and
  frames, the tick (its count up, the count to seats between stages,
  tickets never taken given up); `restore` after a restart; `RelayOut`
  (to one seat or every seat that said hello, closes, up).
- `audience/client.rs`: `AudienceLink` (the device's, sans-IO) and
  `Audience<T: Transport, C: Clock>` over a shell's socket, its tickets
  from the audience join.
- `live.rs`: `ClientLive::Audience { open }` (the owner's), `ServerLive::Audience { count, open }`
  (to members), `LiveLink::audience`.
- `fake/audience.rs`: `FakeServer` runs the hub beside its live side,
  relays with a hop each way and seats with a latency each
  (`audience_join`, `FakeAudienceSocket`, `restart_relay`,
  `set_audience_rules`, `audience_stats`); a seat's keep-alive answered by
  the "host", the count to members arriving with what was on its way.
- Tests: `tests/audience.rs` (the view and its leaks, the view's
  changes, the bound, the count, the closing and the room's end, 2,000
  seats on a 73-player stage with attacks, a relay's restarts and a seat's
  return, the members' stream unchanged, `what_the_audience_costs`
  ignored), and the modules' own (coalescing read as the members' frames,
  the frame's bytes, the fold, the clock, the rules' spread, the messages,
  the policy's JSON).

## What the server builds (`../ddi-server`)

1. **The relay class**: `AudienceRelay` (SQLite-less is enough: it stores
   nothing; a new migration tag after `v2`, class and binding
   `AUDIENCE_RELAYS`), one object per `<room id>:<k>`. It runs
   `ddi_room::audience::relay::Relay`: seats' sockets accepted with the
   hibernation API, tagged `hello` once their hello is taken (frames go to
   `getWebSockets("hello")` as one `ArrayBuffer`, as the room's frames
   do), each attachment `{seat, mark}` (`restore` on wake);
   `setWebSocketAutoResponse("ping", "pong")`; its socket up to the room
   (a `fetch` with `Upgrade: websocket` to the room's stub, path
   `/relay/ws`, internal: the Worker never routes a public path there),
   opened on its first admission and again whenever found closed (on any
   message or an alarm while seats exist); `RelayOut`'s closes with their
   codes; an alarm at `Relay::next_wake` while something waits.
2. **The room's side** in `RoomDO`: an `AudienceHub` beside `LiveHost`,
   restored with `AudienceDurable` (a row of its own) and the audience
   key; relay sockets accepted on `/relay/ws` (attachment `{relay: k}`,
   tag `relay`); `RelayUp` text through `relay_text(Some(k), …)`; every
   member frame the live side broadcasts handed to `member_frame`;
   `tick` after every change and on the alarm (the alarm at the earliest
   of `next_wake`s); `HubOut` applied (to one relay, to every relay socket
   as one buffer for binary, `ServerLive::Audience` to the members' live
   sockets); `ClientLive::Audience` from the member's socket to
   `set_open`; `set_key` when the invite rotates. New rooms' policy
   `.with_audience()`; a stored room given it when loaded (as live play
   was).
3. **The audience link**: `invite::audience_secret` (a one-way hash of
   the invite's secret under its own tag, like `watch_secret`), its key as
   the room keeps it (the hash of that secret, compared by the relay as
   given); `GET /rooms/<id>/invite` gains `audience_invite` for whoever sees
   the invite; `GET /invites/<link>` answers `kind: audience` with the
   view's name, host, phase and the count; the preview at the member bound
   says the audience is offered.
4. **The join**: `POST /rooms/<id>/audience {invite}` (signed in; open
   question 1 for anonymous): the identity's `relay_order`; each relay's
   `/admit {identity, key}` until one admits or refuses for a reason not
   `Full`; a signed audience ticket `{room, relay, seat, expires}` (a
   minute, once); `GET /sockets/<id>/audience/ws?ticket=` opens the seat's
   socket on that relay. Refusals with `AudienceRefusal::code` and
   `status`. The member join at the bound answers `room_full` with
   `audience: true` so the page offers it.
5. **Bounds and counts**: the relay's admission checks (key, open, seats,
   two an identity); the room's sum; closing (`Ended` to the relays, their
   seats closed 4403); the room's end (4410 to every seat).
6. **A load test**: `ddi-load --audience N`: N seats (the game's own
   `Audience` over tungstenite) on a 73-competitor stage, against a local
   `wrangler dev` (thousands of sockets from one machine: raise the open
   files limit; 2,000, 5,000 and 20,000), measuring the room's alarm time
   and CPU with and without them, each relay's CPU, frame spacing at the
   seats, the count, and the members' frame spacing unchanged.

## The server, as built (2026-11-13)

`../ddi-server` (`worker/src/audience.rs`, `relay_do.rs`, `room_do.rs`;
its README's Rooms section). Its list above, item by item:

1. `AudienceRelay` (`AUDIENCE_RELAYS`, migration `v3`, SQLite, storing
   nothing) runs `Relay` through a host core (`RelayCore`) that adds the
   seats given and not yet taken (a ticket takes its seat once), the seats'
   open sockets, and its socket up (opening, answered, refused, the room
   ended). Seats' attachments are `{room, relay, seat, mark}`, the mark as
   text (JavaScript's numbers cannot hold a `u64`: the first local run
   found it). **Not as the list says:** tags are given when a socket is
   accepted and never changed after, so seats are not tagged `hello`; the
   relay sends to the seats `Relay::hello_seats` names (a frame still one
   buffer for all). **Additions:** the socket up is closed after a minute
   without seats (an outgoing socket keeps the object, and its billing,
   awake; the next admission opens it again), and while frames flow the
   relay wakes each second (`RELAY_STAGE_CHECK_MS`, PROVISIONAL; every
   30 s otherwise while it has seats), so one that lost its memory with
   its seats' sockets kept (their keep-alives never wake it) is back within
   the two seconds of frames the room resends: its seats resume from frames
   rather than a snapshot with a hole.
2. `RoomDO` holds a `RoomAudience` beside the live side (the hub, the
   audience link's key following the invite); relays' sockets on
   `/relay/ws`, tagged `relay`, attachment `{relay, seats}` (a room that
   lost its memory counts them again from it); the members' broadcasts and
   the idle end leave relays out (an audience keeps no room open).
3. The audience link's secret is a one-way hash of the watch-only link's
   (so of the invite's): any of the three links can join the audience (a
   join at the member bound by the invite or watch-only link is offered it)
   and none can be climbed. It has no expiry of its own. `GET
   /invites/<link>`: `kind: audience`, `audience_count`; at the bound
   `audience: true` (preview and `room_full`).
4. The join answers `{ticket, relay, expires_at_ms}`; the socket is `GET
   /sockets/<id>/audience/ws?ticket=` (or `/rooms/…`). A relay that cannot
   be readied in 5 s answers `503 relay_unready`.
5. As the contract says.

Native tests drive the objects' handler sequences (the server's live
harness with relays and the game's `Audience` on each seat): the join and
every refusal (the 20,000-seat bound, two an identity, a closed audience,
an ended room, no audience, a link not the room's), a seat following a
stage with attacks, the count reaching the members, a relay restarted
mid-stage, the audience closed and opened again, the invite replaced, and
the members' frames the same bytes at the same times with 60 seats.

### The local load test

`ddi-load --audience N` on a 73-player stage of 40 s, attacks on, against
a local `wrangler dev` (one `workerd` process on the maintainer's Mac, 10
cores), the first 50 seats the game's own client, the rest light clients
reading the same wire. Two builds, the dev build first:

| Seats | Relays | Room alarm p50 / p99 (ms) | Members' frame gap p99 (ms) | Audience frame gap p50 / p99 (ms) | Frames to every seat | Count seen | workerd CPU (ms/s) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 0 | 4 / 9 | 105 | | | | 38 |
| 2,000 | 6 | 5 / 77 | 168 | 251 / 292 | 166 of 166 | 2,000 | 231 |
| 5,000 | 12 | stalled | stalled | stalled | | 5,000 | |

Release build (dev-auth), a second series:

| Seats | Relays | Room alarm p50 / p99 (ms) | Members' frame gap p99 (ms) | Audience frame gap p50 / p99 (ms) | Count seen | workerd CPU (ms/s) |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | 0 | 4 / 15 | 128 | | | 40 |
| 2,000 | 6 | 5 / 217 | 378 | 253 / 467 | 2,000 | 225 |
| 3,500 | 8 | 8 / 306 | 550 | 254 / 795 | 3,500 | 368 |
| 5,000 | 12 | stalled | stalled | stalled | 5,000 | |

What they show: the whole path works at thousands (joins 20–30 ms at the
median, every seat sees its view, every audience frame and the count, the
members see the count; four frames a second at 250 ms; no refusal, no
unexpected close). A relay's cost all included is about 25 µs a send
locally ((231 − 38) ms/s over 7,600 sends a second at 2,000; 23 µs at
3,500): a full relay (500 seats, 2,000 sends a second) about 0.05 of a
local core, within the plan's estimate, so 500 seats a relay holds.

What they do not show: the local runtime runs every object, the room and
its relays alike, on one thread of one process, beside the driver's own
thousands of sockets on the same machine. The members' spacing and the
room's alarms grow with the audience here because the relays' sends share
that thread with the room (the room's own audience work is one frame built
and encoded four times a second, 56–86 µs natively); on the platform each
relay is its own object. The local alarm spans of a relay last until its
next alarm while its socket up is open (an artefact of local tracing; they
are counted apart). At 5,000 seats (and in an earlier run too) the local
`workerd` stopped serving altogether, idle (no CPU, waiting in its event
loop), with 1.4 GB resident: a limit of the local runtime, not measured
further. 20,000 seats cannot be opened from one machine to one local
address in any case (macOS's ephemeral ports, 49152–65535: 16,384). The
bound, the relay size and the platform's own cost of a send need a
measurement on the platform.

## What the game builds

1. **Arriving as audience** on both shells: the audience link's page (the
   invite page with "You're invited to watch with the audience." and a
   Watch button; signing in as for any join), the member bound's offer
   ("The room is full: you can watch with its audience."); the web's route
   `#/mp/audience/<link>`, desktop's `--join` taking an audience link;
   the audience join, its refusals worded ("The room's audience is
   full.", "The host closed the audience.", "This room has ended.", "You
   are already watching this room on two devices."), `Audience` over the
   shell's socket (the web's `WebSocket`, desktop's tungstenite), a new
   ticket on a lost socket.
2. **The spectator view fed by the audience's frames**: the existing
   watched-stage view (`StageSetup::watching`, the grid, the focus, pages)
   fed from `AudienceLink`'s frames and snapshot through the same feed
   (`ddi_session::battle::feed`), the music and the fields both drawn the
   audience's delay behind the room's song time (`ViewDelay` with
   `presentation_delay_ms` as its ceiling, the room's clock from
   `AudienceLink::room_ms`); the song from the view's share (read by
   share id, as a member reads a stage's); shocks from the plan the device
   computes and checks against the view's `battle` digest; the results'
   Standings from the view. Between stages, a page of the view: the room's
   name, host, rules in words (`room::rules_words`), players, picks,
   standings, and the count.
3. **The count shown to the room**: "132 watching" (a message with a
   plural) on the room's status line and the stage's status line, from
   `LiveLink::audience`; the owner's Host options gain "Close the audience"
   / "Open the audience" and "Copy audience link" in More (where
   `RoomPolicy::audience` is said).
4. **The "audience is full" and "audience closed" pages**, and a seat
   closed mid-stage (4403, 4410) saying why.
5. Messages in `locales/en/room.ftl` and `ja`; the `qps-ploc` walk;
   `__DDI_DEBUG.audience` (`count`, `frames`, `delay`, `relay`); tests
   against `FakeServer` with seats.

## Provisional choices

| Choice | Where | Effect | Decision needed |
| --- | --- | --- | --- |
| A third link for the audience; the watch-only link stays a member's; at the member bound the audience is offered | design | Hosts copy a link for each purpose | The rule (open question 2) |
| Audience seats signed in, two an identity | `AUDIENCE_V0.seats_per_identity` | Watching asks for a Twitch sign-in | Anonymous seats (open question 1) |
| 20,000 seats a room, relays of 500, four homes | `AUDIENCE_V0` | Beyond it "The room's audience is full." | The bound, after the server's load test |
| Four audience frames a second, everything in each | `frame_ms`, `audience-v0` | 16 KB/s a seat at 73 players | Paging for large fields later |
| The audience at most two seconds behind, adaptive | `presentation_delay_ms` | Measured about 0.4–0.7 s in the fake | The ceiling |
| The count once a second to the room, every frame or five seconds to seats | `count_ms`, `idle_count_ms` | The number moves a little after people come | None |
| The owner may close the audience; kept beside the live part, not in the state | `AudienceHub::set_open`, `AudienceDurable` | Closing sends everyone away | Co-hosts too? |
| No pings from the audience: the room's clock from the frames' times | `ArrivalClock` | The music tens of ms off true, never seen behind the delay | None |

## Open questions for the maintainer

1. Should the audience need a sign-in (as now, bounded per identity), or
   be anonymous (cheaper to join, bounded per address)?
2. Three links (the invite, the watch-only link for named watchers the
   owner can let play, the audience link for everyone else), or should the
   watch-only link become the audience link?
3. Is "132 watching" on the players' screens wanted during a stage, or only
   between stages?
4. May co-hosts close the audience, or only the owner?
5. The bound: 20,000 a room is what 40 relays carry at a quarter of a
   dollar an hour; higher (a second tier of relays) or lower?
6. Should an audience device see the attack lines and warnings (it has
   them: the frames carry the ledger), or only the fields?
