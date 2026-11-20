# The host's Twitch in a room: bans mirrored, chat behind the play

Status: designed (2026-11-20), nothing built. The maintainer answered
the design round's ten questions and the three that followed from them
the same day; they are listed under "The maintainer's answers", and
where an answer differed from the proposal the answer won.

The maintainer's request (2026-11-19 and 20): a room whose host streams
on Twitch may keep out the people the host banned or blocked there, and
may show the host's chat during play, "an optional nico-nico style
danmaku or alternatively a vertical scroll of emotes or messages (with
emotes) that appear during gameplay behind the fields but in front of
the background", on every player's device, without a second sign-in that
lives only in the browser, without handing Twitch tokens to the game and
without routing chat through our server.

## What was checked, and what was not

Measured from this machine (2026-11-19 and 20, `curl` with an `Origin`
header):

| Host | Answer |
|---|---|
| `static-cdn.jtvnw.net` (Twitch emote images) | `access-control-allow-origin: *` |
| `7tv.io` (7TV API) | CORS allowed (the origin echoed) |
| `cdn.7tv.app` (7TV images, `_static` too) | `*` |
| `api.betterttv.net` (BTTV API) | CORS allowed |
| `cdn.betterttv.net` (BTTV images) | **no CORS header**, png and webp |
| `cdn.frankerfacez.com` (FFZ images) | `*` on the two answers that came; every request with an `Origin` got 522 (their server was down) |
| `api.frankerfacez.com` (FFZ API) | unreachable, unmeasured |
| `api.twitch.tv/helix/eventsub/subscriptions` | preflight allows `POST` with `Authorization`, `Client-Id`, `Content-Type` from any origin |

`site/` sets no Content-Security-Policy, so the page blocks none of
them.

From Twitch's documentation (read 2026-11-20, quoted in the design
round):

- `channel.chat.message` over a WebSocket takes a user token with
  `user:read:chat`, read "as" that user; no scope of the broadcaster's is
  listed. With an app token (webhooks, conduits) it also needs
  `channel:bot` from the broadcaster or moderator status.
- Limits are per user token (client id and user id): 3 WebSocket
  connections with enabled subscriptions, 300 subscriptions each, a total
  cost of 10. A subscription costs nothing where the user it names has
  authorized the app.
- A reader who is no moderator and not authorized by the broadcaster
  counts against 100 chat rooms joined at once and 20 joins in 10 s.
- A reader banned in the channel has the subscription refused with 403.
- Get Banned Users: the broadcaster's own token, `moderation:read`, a
  `user_id` filter of up to 100 ids, timeouts included (`expires_at`
  empty for a permanent ban).
- Get User Block List: the broadcaster's own token,
  `user:read:blocked_users`, the same `user_id` filter.
- `channel.ban` and `channel.unban` need `channel:moderate`. There is no
  event for a block. No call lets a person's own token ask whether they
  are banned somewhere.
- Helix calls cost one point each against a per-user bucket; the docs
  give no number for it.

Not checked, and the spike's to settle (step 0):

- That a subscription made by our server binds to a socket the game
  opened (the socket takes no credentials; the session id is all the call
  names). Expected to work, never run.
- The time the game has between a socket's welcome and its first
  subscription (about 10 s, from memory).
- What `cost` the create call answers for a viewer reading the host's
  channel.
- Whether a reader shows in the channel's list of chatters.
- That `channel.chat.message_delete`, `channel.chat.clear_user_messages`
  and `channel.chat.clear` take the same `user:read:chat` token.
- How long a refresh token of a confidential client lasts unused.
- FFZ, once its server answers.

## The shape in one page

```
            sign-in as today (no scope)           Twitch
 game  ───────────────────────────────►  server ◄──────► id.twitch.tv
        "Connect Twitch" (the scopes a            keeps a person's
         feature needs, asked when it is          refresh token, used
         turned on)                               only when asked

 Bans mirrored (the host's token, the server alone):
   join / ticket / a stage picked ──► server ──► Helix: banned? blocked?
                                         └─► refuse, or remove from the room

 Chat behind the play (each device, its own person's token):
   game opens a socket to Twitch ◄──────────────── chat events, direct
   game ──► server: "subscribe my socket <id>"
            server ──► Helix: create subscription (that person's token)
   game ──► emote images, direct from the emote hosts
            (through the server's proxy only where that fails)
```

Three parts, each shippable alone: the Twitch link on the server (A),
the mirroring (B), the chat (C). B and C both need A and nothing of each
other.

## A. The Twitch link

### A1. What is asked of whom, and when

Sign-in stays as it is: no scope, the token revoked at once
(`worker/src/session.rs`, `TWITCH_SCOPES`). The scopes are asked when a
person turns on the feature that needs them, by a second trip through
Twitch's consent with those scopes:

| Who | Turns on | Scopes |
|---|---|---|
| Host | Bans mirrored | `moderation:read` |
| Host | Blocks mirrored too | `user:read:blocked_users` |
| Host | Chat shown | `user:read:chat` |
| Any member | Chat seen on this device | `user:read:chat` |

Recommended: incremental, as above. The alternative, asking everything
at every sign-in, puts "view your chat messages" on the consent screen
of people who only want to play.

A person who already granted a scope goes through Twitch without a
question, so turning a feature on again is a redirect and back.

### A2. Where the token is kept

It has to be on the server: the ban check runs when someone else joins,
with no request of the host's to carry it.

One small Durable Object a linked person (`TwitchLink`, as
`AuthAttempt` is one an attempt), holding the refresh token, the access
token with its expiry, the scopes granted and when it was last used,
encrypted with a key from a Worker secret. No alarm but its own end.
(Answer 1.)

Rejected:

- Sealed inside the session cookie: stateless, but the host's token is
  then absent whenever the host is not the one asking.
- In the room's own storage: the token would end with the room, but
  every new room would send its host through Twitch again, on desktop
  with a typed code each time.

### A3. Refreshing and validating

Lazy, as the maintainer proposed: the link is touched only when a
request needs it. Before use, an access token past its expiry is
refreshed; one not checked for an hour is validated. Nobody is using it
in between, so nothing wakes for it.

When Twitch refuses (the person disconnected the app, changed their
password), the link is deleted and whatever depended on it turns off:
the room's mirroring and chat if it was the host's, the chat on that
device if it was a viewer's. The person is told once.

### A4. Ending it

- Sign out deletes the link and revokes the token at Twitch.
- Options → Data has "Twitch access: Disconnect" for a person who wants
  to stay signed in.
- A link unused for 30 days is deleted (answer 1; the same horizon as a
  session's longest life).

### A5. The shells

- Web: `GET /auth/twitch/login?return=<route>&scopes=<set>`, back to the
  route. The scope sets are a closed list the server knows, never free
  text.
- Desktop: the typed-code attempt (`POST /auth/attempts`) takes the same
  `scopes`; its page says what is asked.
- The game never holds a Twitch token. `GET /me` gains the scopes
  granted, so the screens know what can be turned on.

## B. Bans and blocks mirrored

### B1. The setting

A room setting of its host's, in Host options beside "If the host
leaves": "Twitch bans: Off / Bans / Bans and blocks", Off by default.
Only the room's owner sets it (never a co-host: the lists are the
owner's own), and only once the server has seen the scopes.

Because a member cannot prove a scope, the game asks the server
(`POST /rooms/<id>/twitch`), which checks the link and gives the room
the change as the authority (`Command::SetTwitch`, authority only, as
`SetContentStatus` is).

### B2. Timeouts

Recommended: a timeout keeps a person out while it lasts. It is the same
call and Twitch treats it as a ban with an end.

### B3. Whose lists, and a change of host

The lists are those of the person who turned the setting on, and they
keep applying while that person is a member of the room, also after the
room passed to someone else (`MakeHost`, the succession) and they stayed
as a co-host (answer 2). The setting turns off when they leave the room;
the status line says so. Being away inside or past the reconnect grace
does not end it: the check needs their link, not their connection.

The room's state names them (`RoomState::twitch`'s `by`), and the Host
options say whose bans apply.

One mirror a room. After the room changed hands (answer 11):

- The new host may turn the earlier host's mirror off, or replace it
  with their own lists (their own scopes checked, as in B1). A
  replacement checks everyone present once, as turning it on does (B4).
- The person who set it may turn it off, host or not.
- Nobody else changes it.

### B4. When it is checked

1. At `POST /rooms/<id>/join`, for a newcomer and for a member who comes
   back.
2. At each ticket (`POST /rooms/<id>/ticket`), from an answer kept five
   minutes for that person.
3. Everyone present at once when a stage is picked: one call per 100
   members for each list, eight calls for a room at its member bound.
4. Everyone present once when the setting is turned on.

No `channel:moderate` and no event subscription: the sweep at each pick
finds a ban made during the room within a stage.

### B5. Someone already inside

The room has no way to remove a member today (`crates/room`: Leave is
the member's own). New: `Command::Remove { participant }`, sent by the
authority for the mirror and by a host for a kick (B9),
`Event::MemberRemoved` saying which, the socket closed with a code of
its own (4403).

A sweep runs at a pick, before readiness, so a removal never interrupts
a stage. A person banned during a stage plays it out unless their socket
drops: a ticket is then refused and they are away as anyone
disconnected.

No list of removed people is stored: the check itself keeps them out,
and an unban lets them back at once.

### B6. What a refused person is told

One neutral sentence for every case: "You can't join this room." The
answer is `403 not_admitted`, the same for a ban, a timeout and a block.
Twitch does not tell a person they were blocked, and the game should not
either. (Answer 4.)

### B7. Who is not checked

- The anonymous audience: no Twitch identity. A banned person can still
  watch a room by its audience link, as they can watch the stream; the
  host can close the audience.
- Bots and CPUs.
- The server's administrators who observe (`RoomState::observes`).

### B8. When Twitch does not answer

- Twitch unreachable or slow (2 s): the person is let in and checked
  again at the next ticket or pick (answer 3). The mirror is a
  convenience over Twitch's own moderation, and a Twitch outage should
  not close a room.
- The host's link refused: the setting turns off (A3), nobody is
  removed, the host is told.

### B9. A host's Kick

In this plan (answer 5), and independent of Twitch: it works in any
online room, with the mirror on or off.

- The Players pop-up has a "Moderate" toggle for the host and the
  co-hosts (answer 12), off each time the pop-up opens. Only while it is
  on does each person's row show Kick, so nobody is kicked by a slip.
- The host may kick anyone but themselves. A co-host may kick ordinary
  members only: never the host, never another co-host.
- Kick removes the person at once (`Command::Remove`, a capability of
  its own, `Capability::Kick`). During a stage they play, it is a
  forfeit, as leaving is.
- Kicking does not ban and no list is kept: the person may come back by
  the invite. A host who wants them kept out replaces the invite link
  (More's Replace invite link, as today) or bans them on Twitch with the
  mirror on.
- The person is told plainly: "The host removed you from the room." A
  kick is no secret, unlike the mirror's refusal (B6).
- Never the server's observing administrators. CPUs leave by the roster
  as today.

### B10. The model

Room model revision 18, server first:

- `RoomState::twitch`: who turned it on (`by`), the mirror's choice, the
  chat's choice (C1), that person's Twitch id and login. Additive, left
  out of the JSON at its default; a room stored by revision 17 reads
  with it off.
- `Command::SetTwitch`, `Command::Remove`, `Capability::Kick`,
  `Event::TwitchChanged`, `Event::MemberRemoved` (kicked, or not
  admitted), `Rejection::NotAdmitted`.
- The audience's public view carries none of it.
- `FakeServer`: a scripted set of barred identities and the removal, so
  the screens and the walkthroughs run without Twitch.

## C. Chat behind the play

### C1. Who decides what

- The host sets the ceiling for the room, in Host options: "Show my
  Twitch chat: Off / Emotes / Emotes and messages", Off by default.
  Messages are the host's moderation risk, so the host allows them.
- The channel is always the own channel of the person who turned it on,
  taken from their identity; no channel name is typed. As the mirror
  (B3), it stays theirs while they are a member, also after the room
  changed hands, and the new host may turn it off or replace it with
  their own channel (answer 13). A replacement makes every device
  subscribe again, to the new channel.
- Each member chooses for their device, Options → Display: "Twitch chat
  in rooms: Off / Emotes / Emotes and messages" (never above the room's
  ceiling), "Chat style: Across / Falling" and "Chatter names: Off / On"
  (C11). Off by default, since it needs the scope; a player who never
  turns it on sees nothing and is asked nothing.

So nobody has moving things put behind their field by someone else's
choice. (Answer 6.)

### C2. The transport

Each device opens its own socket to Twitch (`wss://eventsub.wss.twitch.tv/ws`),
reads its session id from the welcome and posts it to
`POST /rooms/<id>/chat/subscribe`. The server checks the caller is a
member, that the room shows chat, and makes the subscriptions with the
caller's own link. Events then come from Twitch to the device; the
server never sees a message.

- Emotes: `channel.chat.message`.
- Messages: also the delete, the user's clear and the chat's clear, so
  what a moderator removes disappears from the screen.
- A dropped socket is opened again and subscribed again. Twitch's
  reconnect message is followed.
- Connected while the person is in a room that shows chat and their
  device has it on; drawn only during a play.

### C3. Limits that follow

- Three sockets a person: a fourth device of theirs shows no chat (the
  room allows four).
- A viewer banned in the host's chat gets nothing. With the mirror on
  they are not in the room.
- The anonymous audience and bots have no token and see no chat. See
  C13.

### C4. Emote sources

Twitch's own emotes come in the message's fragments with their ids; the
image is `static-cdn.jtvnw.net/emoticons/v2/<id>/static/dark/<scale>`.

Third-party emotes are words in the text, matched against the host's
channel set and the global set, which each device reads from the
provider by the host's Twitch id when the chat starts.

All four providers from the first release: Twitch, 7TV, BTTV and FFZ
(answer 8).

Every emote endpoint, the providers' APIs and their image hosts alike,
is asked directly first. Our server has a proxy ready for each of them,
used only when the direct request fails. So BTTV's images work on the
web today through it, and nothing breaks the day a provider removes a
CORS header or adds one.

- A page cannot tell a CORS refusal from a network failure, so any
  failed direct request is tried once through the proxy. A host that
  needed the proxy is asked through it for the rest of the session, and
  directly again in the next one.
- The proxy (`GET /emotes/<provider>/<path>`) is a closed list: the four
  providers' hosts, the path shapes of an emote image and of a channel's
  or the global set, `GET` only, an image or JSON back, a size cap,
  cached, rate limited, for signed-in people. It never carries chat.
- Desktop has no CORS and asks directly; it uses the proxy only where a
  direct request fails.
- Through the proxy a provider sees our server's address, not the
  player's.

A device option "Emotes from: Twitch / Twitch and others" lets a player
keep their device from asking the other providers at all.

Only emotes of the channel's own sets and the globals are drawn: the
host's curation is the filter.

### C5. Still or animated

Still images in the first version (Twitch's `static`, 7TV's `_static`).
Animated ones later: the browser's `ImageDecoder`, desktop's `image`
crate. Still emotes read better behind moving notes in any case.

### C6. The styles

One model of an item (a row of emotes, or text with emotes inline) and
two motions:

- **Across**: items cross the screen right to left in rows, each at a
  speed that keeps it on screen the same time whatever its length, a row
  taken only when its last item has cleared the edge. Niconico's way.
- **Falling**: items fall in columns. In the side space where there is
  some; behind everything where a room's miniatures fill it.

Both styles, with emotes and with messages, come in one release
(answer 10).

### C7. The layer

After the backdrop (the picture, the movie, the generated background, a
work's back layers), before the field filter, the attack links, the
miniatures, the field and the HUD. In the lane band the filter darkens
it with the background, so notes keep their contrast.

An opacity of its own (Options → Display, about 60% to start; tuned in a
study on a busy chart).

Emotes go into one atlas, so the overlay adds a batch, not one per
emote. Text goes through the platform's runs as lyrics do
(`ddi_render::lyrics`): the browser's fonts on the web, the system's on
desktop, so any script a chat writes can draw.

Two things to read in the renderer before building (step 0):

- T1: lyrics are drawn over a play; this layer is under the field. Either
  the 2D pass gains a pass at that depth, or the layer is drawn into a
  texture the backdrop shows, as a generated background is.
- T2: whether the text atlas keeps color glyphs. If not, Unicode emoji in
  messages draw plain or are left out.

### C8. Time

The layer runs on the host's clock, never the song's. Nothing of it is
judged, logged or replayed. Devices are not in step with each other:
each gets a message when Twitch sends it.

### C9. Floods and budgets

Named `chat-overlay-v0`, every number provisional:

- At most 40 items on screen and 8 new ones a second; one item a chatter
  every 2 s.
- An emote-only item shows up to 5 emotes; a run of the same one is
  drawn once with a count.
- A message is one line, cut at 60 graphemes.
- When full, new items are dropped, never queued: late chat is worse
  than missing chat.
- At most 256 emote images kept, four fetched at once, decoded off the
  frame. An item whose emote is not ready in a second goes without it.
- The overlay is the first thing shed when frames come late: no new
  items while a play's frames are behind.

### C10. Reduced motion

Nothing travels. Items appear in fixed places and fade there.

### C11. What of a message is shown

- Only what Twitch delivered: a message AutoMod holds never arrives.
- A removed message, a cleared user and a cleared chat take their items
  off the screen at once.
- Left out: commands (a leading `!`), messages with a link, cheers,
  messages from another channel's shared chat.
- One quiet color. Chatter names are a device option, "Chatter names",
  off by default, for both styles (answer 9): on, a message is led by
  its chatter's display name.

### C12. Fairness

Opt-in per device and off by default (C1), cosmetic, never judged. A
room's rules do not mention it.

### C13. The audience and people not signed in

No chat (answer 7). Reaching them needs our server to relay something,
which this design avoids. A watcher who wants it signs in and joins as a
watching member.

### C14. A streamer alone

The same link can show a person their own chat in singleplayer. The
subscribe route is written to allow "my own channel" as well as "my
room's host"; the option and its screen are a later step.

### C15. What the players are told

- `site/privacy.html`, the Credits' Privacy fold and the server's README
  gain: the Twitch access kept and for how long; that a device showing
  chat connects to Twitch and the emote providers, who see its address;
  that a reader joins the host's chat as themselves (and whether they
  show among its chatters, once the spike knows); that ban and block
  answers are kept five minutes and never stored.
- Twitch's and the providers' emotes are fetched when shown, never
  bundled. The game names them to say what it reads, and claims no
  affiliation.

### C16. Where the code goes

- A pure crate for the chat (`crates/chat`): Twitch's socket messages
  parsed, fragments made into items, third-party words matched, the
  flood rules, the rows and columns. Natively tested on recorded JSON.
- The shells' transports: the browser's `WebSocket` and `fetch`; desktop's
  tungstenite and ureq, each on a thread, as the room's socket is.
- `ddi-session` advances the layer beside the play; `ddi-shell`'s
  `PlayView` places it; `ddi-render` draws it.
- Settings in `Settings`; every text a message in `locales/en` and
  `locales/ja`.
- For tests, a scripted chat: the web's `mpfakechat=…` beside
  `mpfakeroom`, desktop's `--mp-dev-chat FILE`. Tests never reach
  Twitch.

## Steps

0. **Spike.** A scratch script and a real account: the unknowns listed
   under "What was checked". T1 and T2 read in the renderer. No product
   code.
1. **The link** (A): scopes asked when needed, the per-person store, lazy
   refresh, Disconnect, `GET /me`'s scopes. Server and both shells.
2. **The mirror and the kick** (B): revision 18, the gate at join,
   ticket and pick, `Remove`, the host's setting, Moderate and Kick, the
   fake, the privacy note.
3. **The chat** (C), one release: the chat crate, the transports, the
   subscribe route, the four providers with the proxy behind them, the
   atlas and the layer, both styles, emotes and messages with the
   moderation events and C11's filters, chatter names, reduced motion's
   fixed places, the settings, the scripted chat.
4. **Later**: animated emotes, a streamer's own chat in singleplayer.

Steps 2 and 3 do not depend on each other.

## The maintainer's answers (2026-11-20)

| # | Question | Answer |
|---|---|---|
| 1 | Where a person's Twitch access is kept (A2) | A store a person on the server, deleted after 30 days unused |
| 2 | The mirror when the room changes hands (B3) | Stays while the person who set it is a member (the proposal was: turns off) |
| 3 | Twitch not answering a check (B8) | Let in, checked again later |
| 4 | What a refused person is told (B6) | One neutral line |
| 5 | A host's own removal (B9) | A Kick button behind a Moderate toggle, no ban list, a kick does not ban (the proposal was: none in this plan) |
| 6 | Who controls the chat (C1) | The host sets the ceiling, each device opts in, off by default |
| 7 | The anonymous audience (C13) | No chat |
| 8 | Emote providers (C4) | All four from the start; a proxy ready for every emote endpoint, used only when direct access fails (the proposal was: BTTV last) |
| 9 | Chatter names (C11) | A device option, off by default (the proposal was: none) |
| 10 | Build order (C6) | Everything in one release (the proposal was: Across emotes, messages, Falling) |
| 11 | After the room changed hands (B3) | The new host may turn the earlier host's mirror and chat off or replace them with their own; the person who set them may turn them off |
| 12 | Who may kick (B9) | The host and the co-hosts; a co-host only ordinary members (the proposal was: the host alone) |
| 13 | The chat after the room changed hands (C1) | The same rule as the mirror |

Not asked, standing as proposed: a timeout keeps a person out while it
lasts (B2).

## Review notes (2026-11-20, before building)

Checked again against Twitch's documentation the same day (the pages
named under "What was checked"): Get Banned Users and Get User Block
List both take `user_id` (up to 100) and then ignore paging; a timeout
is told from a ban by `expires_at`; a socket has 10 s from its welcome
(close 4003); WebSocket subscriptions take a user token only; three
connections, 300 subscriptions, a total cost of 10 a user token; a
subscription that names a user who authorized the app costs nothing; the
four chat types share one authorization and one condition. So the
design's reading stands. What the documentation does not say, and step 0
has to show: whether another process may create the subscription for a
socket, and what one authorization gives after an earlier one.

Points for the build:

1. **Scopes are asked as a union.** The documentation does not say that
   a second authorization keeps the scopes of the first; assume it does
   not. Each trip through Twitch asks for what the link already has plus
   what is turned on now, and the link keeps the token of the latest
   trip (the earlier one revoked).
2. **The cost of four subscriptions.** Messages need four subscriptions
   a device, twelve for three devices. They cost nothing only if the
   reader's own authorization counts for all four types; if step 0 finds
   a cost, the three moderation types are made on one device of the
   person at a time, or dropped to the message type alone past the
   budget, and the plan says which before the transports are built.
3. **A mirror's removal is not told to the room.** `Event::MemberRemoved`
   for "not admitted" would tell every member that the host banned or
   blocked that person. To the others it reads as a member leaving; only
   the removed person's socket (4403) and the server's log know why. A
   kick is told to everyone (B9: no secret).
4. **The server's configuration is the maintainer's.** `TwitchLink` is a
   new Durable Object class (a binding and a migration in
   `worker/wrangler.toml`) and its key a new secret; agents do not edit
   that file or any secret. The build delivers the exact lines as a
   patch and runs its local server on a scratch copy.
5. **Step 0 needs a real account**, which no agent has. The spike is a
   script the maintainer runs with a token of their own; the renderer's
   T1 and T2 are read by the build. Until it has run, the transports to
   Twitch (C2) are written last, behind the scripted chat, and the
   mirror's Helix calls behind a fake.
6. **Order.** Kick and removal (B5, B9) need nothing of Twitch and go
   first, with revision 18's whole model (the mirror's and the chat's
   choices too, so the revision is raised once). Then the link and the
   mirror. The chat's pure crate, layer, styles and scripted chat are
   built beside them; its transports and the subscribe route after the
   link and the spike.
