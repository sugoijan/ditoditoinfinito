# The host's Twitch in a room: bans mirrored, chat behind the play

Status: designed (2026-11-20), and built as far as it can be without
Twitch itself: round TW1 (room model revision 18, see "Round TW1" in
`multiplayer-prototype-progress.md`) the model, Kick, the link and the
mirror behind a fake Twitch; round TW2 part C's pieces that need no
Twitch ("T1 and T2" below, and its section at the end); round TW3 joined
the two and walked them on a local server (its section at the end). The
Twitch client
is unverified until step 0's spike, and the chat's transports wait for
it. The maintainer answered
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
image is `static-cdn.jtvnw.net/emoticons/v2/<id>/static/dark/<scale>`, or
`…/animated/…` where the fragment's `format` offers one (C5).

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

Animated emotes move, every provider's, on both shells, in both styles
(the maintainer, 2026-11-20: "We should fully support animated
emotes."; this replaces the first design's stills only). How, with its
bounds: "Animated emotes (round TW4)" at the end. Reduced motion shows
every emote's first frame, still.

### C6. The styles

One model of an item (a row of emotes, or text with emotes inline) and
two motions:

- **Across**: items cross the screen right to left in rows, each at a
  speed that keeps it on screen the same time whatever its length, a row
  taken only when its last item has cleared the edge. Niconico's way.
- **Falling**: items fall in columns. In the side space where there is
  some; behind everything where a room's miniatures fill it.

A stage that shows opponents' small fields uses Falling whatever the
device chose (the maintainer, 2026-11-20: Across reads poorly behind the
miniatures); a solo play and a watched stage keep the choice. A play
whose field leaves no side space (a phone held upright) shows emotes
alone, whatever the room and the device allow ("Animated emotes (round
TW4)").

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
  An animated emote's frames are budgeted apart ("Animated emotes (round
  TW4)").
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

## T1 and T2, read in the renderer (round TW2, 2026-11-20)

**T1: where the layer is drawn.** In a texture the backdrop shows, as a
generated background is, not a pass of the 2D painter at that depth.

- The scene's first pass draws the backdrop (`BackgroundPipeline::draw`,
  the field filter's band darkened in its shader where it can), then the
  scene's segments (a work's back layers and the dancing hand behind the
  field as the skins' quads, the filter's quad where the shaders cannot
  darken, the field, the miniatures), then the HUD and the text. A
  painter's pass between the backdrop and the rest would have to split
  that pass, and the 2D painter's pipelines are made for the surface's
  plain format: on WebGL2, where a play with a skin draws its scene
  offscreen in half floats, it would need a second painter in that
  format, or the scene converted to the plain texture and back over the
  whole screen (two full-screen conversions a frame, as a skin's quads
  over the whole screen cost now).
- Drawn into a texture, the layer costs one pass of the play's own 2D
  painter into a screen-sized texture in the surface's format (cleared,
  then its items: the emotes from one atlas in one batch, the text from
  the painter's pages), and one full-screen textured draw more in the
  backdrop's pass, on every backend alike, with nothing of the scene's
  passes changed. On a tiled GPU (Apple's, phones') a split pass would
  store and load the whole frame once more, which is the same bandwidth
  as the texture's clear and read; elsewhere the texture costs about one
  screen of fill more, and only while the layer has items (an empty
  layer draws nothing and composites nothing).
- The backdrop's own shader composites it (one draw slot of its own,
  over the whole target, not the background's frame or entrance, at the
  layer's opacity), so the field filter's band darkens it with the
  background in the shader, or by the filter's quad drawn over it, and
  the notes keep their contrast either way; under a work's back layers
  and the dancing hand behind the field, which belong to the play's
  picture.

The play's 2D painter draws twice in a frame for it (the layer before
the field, the overlay after), its instances kept for the whole frame.

**T2: color glyphs.** The text atlas keeps coverage alone
(`R8Unorm` pages, a run drawn white and tinted), on both shells:
cosmic-text's color glyphs are kept by their alpha, and the browser's
canvas copy keeps the red channel of its premultiplied colors (or the
alpha, read back). So a Unicode emoji in a message draws in the chat's
one quiet color as its shape (a silhouette, shaded on Chrome's copy),
not in its own colors, and is kept: it carries the message's meaning.
Color emoji would need RGBA pages for the runs that have color glyphs:
later, if the study asks for it.

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
4. **Later**: a streamer's own chat in singleplayer. (Animated emotes,
   once later, were decided for now on 2026-11-20 and built in round
   TW4.)

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

## Round TW2 (2026-11-20): the chat without Twitch, built

Everything of part C that needs no Twitch, behind the scripted chat;
nothing in it reaches Twitch or an emote provider.

- **`crates/chat`** (`ddi-chat`, pure): EventSub's messages read
  (`eventsub.rs`; fixtures in `crates/chat/tests/fixtures/`, written from
  the documentation and marked so, for the spike to replace or confirm);
  the four providers' emotes (`emotes.rs`: Twitch's from the fragments,
  7TV's, BTTV's and FFZ's channel and global sets from their JSON, a
  channel's word over a global one and 7TV over BTTV over FFZ, each
  emote's still image's address and its path under the proxy route
  `GET <api>/emotes/<provider>/<path>`, the sets' addresses the same
  way; BTTV's animated emotes left out, their still frame undocumented);
  a message made into an item after C11's filters (`item.rs`); C9's
  budgets as `chat-overlay-v0` and the two motions (`overlay.rs`); the
  images kept, four fetched at once, the proxy once after a failure, a
  host through it for the session, and their cells in one atlas of
  2048 × 1024 (`cache.rs`); the scripted chat (`script.rs`).
- **The settings** (Options → Display): Twitch chat in rooms (Off by
  default), and under it while it is on: Chat style, Chatter names,
  Emotes from, Chat opacity (60%). The Twitch trip's integration point
  is `ddi_ui::state::twitch::want_chat_scope`, which the option calls
  when it is turned on and which only records the wish today.
- **The layer**: the session holds the chat's source and advances it on
  the host's clock (`ddi_session::chat`, `PlaySession::chat`; a test
  that a play with it on judges, scores and logs as without it); the
  room's ceiling is answered in one place, `ddi_session::chat::room_ceiling`
  (the scripted chat's own line until `RoomState::twitch`); the shells'
  play view places and draws it (`ddi_shell::chat`); the renderer
  composites it over the backdrop (T1 above). Today it is connected to a
  play only by the scripted chat, in any play; a room's own chat is the
  transports' round's and will connect it in a room's stage alone.
- **Placing**, decided while studying: Across's rows never cross the
  receptors' rows or the HUD and are taken from the side away from the
  receptors (from the bottom up in the usual scroll), not from the top as
  niconico's are; Falling's columns are in the side space clear of the
  miniatures where there is room for a column (4.5 lines for messages,
  2.5 for emotes), begin below the score, and a line too wide is
  narrowed to three quarters at most and cut with an ellipsis; behind
  the miniatures only where the side space has no such room, over the
  whole width where there is no side space (a phone held upright).
- **Shedding**: no new items from a late second (the dancing hand's
  reading: under 90% of the refresh, its warm-up and a one-off stall
  counting for nothing) to the next good one; what is on screen goes on.
- **The scripted chat** (TESTING ONLY): the web's `mpfakechat=<demo,
  emotes, flood or a rate>` (`mpfakechatimg=<path>`: its emotes'
  pictures fetched from beside the page), desktop's `--mp-dev-chat
  FILE|NAME`.

Frame costs (2026-11-20, Beatucada's Challenge autoplayed, 1280×720, the
`demo` script, a flood of 50 messages a second; main-thread milliseconds
a frame, means):

| | off | on | flooded |
|---|---|---|---|
| Desktop, offscreen, the frame recorded, alone | 0.09 | 0.15 | 0.17 |
| Desktop, eight CPUs in miniature | 0.29 | 0.37 | 0.38 |
| Chrome (WebGPU), `perf=1` total | 0.56 | 0.61 | 0.68 |
| Firefox (Playwright's build, WebGL2), `perf=1` total | 0.86 | 1.00 | 1.06 |

The chat's own part is 0.06 to 0.2 ms; the scene's passes, instances,
batches and WebGL2's conversions are unchanged (the layer is one pass of
the 2D painter, two draws: its text pages and the emotes' one batch, and
one textured draw in the backdrop's pass). The GPU's own time was not
measured. A flood keeps the layer at 40 items: of 750 messages in 15 s,
88 shown, 475 dropped as full, 180 by the rate.

**The study** (`target/scratch/mp/tw2/sheets/`, not committed): at 40%
the chat is faint over a bright picture; at 60% it reads and the notes
stay clear (the filter darkens it in the lanes); at 80% saturated emotes
in the lanes begin to compete with the notes. Recommended: 60% stays the
default. With eight miniatures, Across's rows pass behind the miniatures
and read poorly; Falling's columns in the margins outside the miniatures
read well and leave the field and the miniatures alone: recommended as
the style where a stage shows miniatures. On a phone held upright there
is no side space and either style crosses the lanes: recommended, emotes
alone there (or none).

## Round TW3 (2026-11-20): the halves joined, then walked for real

- **The room's ceiling** (C1): a stage's order carries the room's chat
  (`StageSetup::chat`, `ddi_ui::state::twitch::RoomChat`: the ceiling
  and the channel's id and login, from `RoomState::twitch`), and
  `ddi_session::chat::room_ceiling` answers from it in a room's stage: a
  device never shows more, and a room that shows none builds no layer.
  The audience has none (C13). Outside a room the scripted chat's own
  line still stands for a ceiling, for tests; CPU practice, which has no
  host's Twitch, takes it under the testing switch.
- **A device's scope**: "Twitch chat in rooms" turned on where the
  person's Twitch access lacks `user:read:chat` starts TW1's trip for the
  closed set `chat` (`Online::connect_twitch_chat`): the web leaves the
  page, back to the room kept (else the multiplayer home), and the
  setting is made by itself once `/me` says the scope is there; desktop
  shows the trip's dialog over Options or Host options (the attempt's
  code to type, which no screen showed before: a Host options trip on
  desktop could not be finished), and a canceled trip changes nothing and
  says so. Its description says Options → Data disconnects it.
- **The testing switch** (`ddi_ui::state::twitch::chat_testing`): the
  scripted chat (`mpfakechat`, `--mp-dev-chat`) turns it on. Until the
  transports exist, Options → Display's chat rows and Host options' "Show
  my Twitch chat" show only under it.

**What a player sees after this round** (nothing of it is deployed: the
server and the game deploy together for revision 18): in an online room,
the Players pop-up's Moderate and Kick for the host and co-hosts; the
host's Host options row "Twitch bans: Off / Bans / Bans and blocks",
whose first step takes them through Twitch and back, the setting made by
itself; the status lines for a kick, for a refused or canceled trip, and,
new here, for the setting ending by itself (the owner's access ended, or
the one who set it left); a person kept out sees "You can't join this
room." on the invite page and nothing else of why; Options → Data's
"Twitch access" row with Disconnect. Nothing of the chat: its rows are
hidden, and no room has a chat to show.

**The walks** (local server on a scratch configuration, the development
provider's fake Twitch; scripts and logs in `target/scratch/mp/tw3/`):
bots (a host bot's kick between stages and during one, a forfeit placed
last with no points, the guest back by its invite), the mirror over raw
sockets (a barred member removed when the mirror is turned on and at the
next pick, the others seeing a leave; the neutral refusal at join; a
timeout ending lets the person back within a second of its end; Twitch
slow: let in, refused at the next ticket; the host's link refused: off,
nobody removed, the host told; a co-host's kick of the host or of another
co-host refused by the room), Chrome and Firefox (Host options' rows, the
trips and a canceled one, Moderate and Kick as host and co-host, the
kicked person's notice and way back, the refused newcomer's page,
Options → Data's Disconnect turning the room's mirror off at once), the
chat in a practice stage of eight CPUs and in an online stage, both
styles, at Emotes and at Emotes and messages, a device at Off drawing
nothing, on both browsers and desktop offscreen; and the singleplayer
smoke check. What they found, fixed: guest bots never loaded a bundled
stage's song (no library); a tap on the players panel's heading did
nothing; the refused newcomer's page said "You can join now" above "You
can't join this room."; nothing told the owner that their access ended
or the room that the setter left (B3, B8); the server had no way to make
its fake Twitch slow, down or refused (`POST /auth/fake/twitch`).

Left as they are, for the maintainer: an answer kept five minutes still
keeps a person out after the host disconnected from another device (the
game that disconnects turns the setting off at once; the server learns
the link is gone only when the answers run out); a bot that is kicked
during a stage plays it on locally and its report is refused (harmless,
a development tool's).

Waiting for the spike: the Helix client, the chat's transports and the
subscribe route, the providers' sets and images fetched for real; then
the chat's rows are shown to players.

## Animated emotes (round TW4, designed 2026-11-20)

The maintainer decided on 2026-11-20 that animated emotes are fully
supported: every provider's animated emotes move in the chat layer, on
both shells, in both styles. The first design (C5) had stills alone.
The same day's other decisions are built beside it: a stage that shows
opponents' small fields uses Falling whatever the device chose (Across
reads poorly behind the miniatures; a solo play and a watched stage keep
the choice); a play with no side space beside the field (a phone held
upright) shows emotes alone; and the chat is drawn over a
Dancing☆Onigiri work's back layers, still under the dancing hand, the
field filter's band and the field (C7 had it under them).

**What each provider serves.** The addresses asked, directly first and
through the proxy after a failure (C4), as for stills:

| Provider | Animated where | Address (2×) | Format |
|---|---|---|---|
| Twitch | the fragment's `emote.format` has `animated` | `static-cdn.jtvnw.net/emoticons/v2/<id>/animated/dark/2.0` | GIF ("An animated GIF is available for this emote", Get Channel Emotes, read 2026-11-20) |
| 7TV | the emote's `data.animated` | `cdn.7tv.app/emote/<id>/2x.webp` (the file `2x.webp` of `host.files`; its `static_name` is the still) | animated WebP |
| BTTV | `animated: true` (`imageType: "gif"`) | `cdn.betterttv.net/emote/<id>/2x`, the same address as a still emote's | GIF |
| FFZ | the emoticon's `animated` object (`{"1","2","4"}`) | `cdn.frankerfacez.com/emote/<id>/animated/2` | animated WebP |

7TV's and BTTV's shapes are their API's answers as the fixtures record
them (neither has a documentation page that loads without a script);
FFZ's public developer page documents `urls` alone, and its `animated`
object is read as the chat clients read it: PROVISIONAL, for the spike
to confirm with the providers' real answers. An emote animated by its
provider is drawn from its animated image; its still is never asked
for, since the first frame stands for it where a still is wanted.

**Read before decoded** (`ddi_chat::anim`, pure, on both shells): the
container is read without decoding a pixel (GIF's blocks walked, WebP's
chunks, PNG's and APNG's chunks, JPEG's frame header): its format, its
canvas, each frame's size and duration. Its limits, `anim-limits-v0`,
PROVISIONAL, are checked there, before memory is spent on pixels:

- at most 2 MiB of bytes (read with that cap, an answer whose length
  says more refused before it is read, a longer one stopped once past
  it; the proxy's cap too);
- each side of the canvas and of every frame at most 1024 pixels;
- an unknown or broken container refused (an animated WebP frame's own
  bitstream must say the frame's size: a decoder may size its buffers
  by it before it compares);

any of these refuses the image (it is never drawn, as a failed fetch);
and

- at most 1000 frames, and at most 64 megapixels of frames in all (each
  frame's own area or the canvas's, the larger, summed: what decoding
  and compositing them costs, so a tiny canvas with huge frames counts
  its frames);

past these the animation is refused and its first frame is shown still.
A frame's duration is its own (GIF's delay, WebP's and APNG's), a
duration of 10 ms or less read as 100 ms as the browsers read it; a
loop count is not read (emotes loop for ever, as chat clients play
them).

**Frames kept.** At most 64 frames of an emote, and at most 128 of the
atlas's 64-pixel units (a 4:1 wide emote keeps 32 frames). An animation
with more is thinned evenly: every *k*-th frame kept, each standing for
the frames up to the next one kept, its duration their sum, so it plays
in the same time at a lower rate. Only the kept frames are decoded to
the atlas's row height (64 pixels); the rest are passed over (a GIF's
are still composited in turn, which the work limit bounds).

**Where decoded.** Never on the frame's thread where the platform gives
another way:

- Desktop: on the fetch's own thread (four at most at once): the
  `image` crate the desktop already links (GIF, WebP, APNG; no new
  dependency), its own limits set to the same sides and a 64 MiB
  allocation, the frames iterated, the kept ones scaled, and a deadline
  of 1.5 s between frames, past which the animation is refused and the
  first frame stands.
- The web: the browser's `ImageDecoder` (WebCodecs; Chrome 94, Firefox
  133, Safari 27.3), asynchronous on the browser's own decoders, asked
  for the kept frames alone, each scaled by `createImageBitmap` (also
  asynchronous), drawn into one small canvas and read back once. The
  probe runs in the page (a walk over at most 2 MiB of headers, well
  under a millisecond). A browser without `ImageDecoder`, or one that
  refuses the type, shows the first frame (`createImageBitmap`), as
  before.

**The atlas.** One atlas, 2048 × 2048 pixels (16 MiB of GPU memory, was
2048 × 1024, 8 MiB), so the emotes stay one batch: stills and frames
share its 1024 units of 64 × 64 pixels, an animated emote's frames each
a cell of its own. All animated emotes' frames take at most 768 units
(12 MiB; twelve emotes of 64 frames, or many more of fewer); 256 images
are kept at most as before. A new animation that does not fit sends the
least recently drawn animated emote away (never one drawn this frame);
where none can go, it is placed as a still, its first frame in one
cell: a still first frame rather than nothing. The CPU's part while
decoding: one canvas frame (4 MiB at the most) and the kept frames
(2 MiB at the most) a fetch.

**How frames advance.** On the layer's own clock: the host's seconds,
never the song's, held while the frames run late (the hand's-load
reading of a late second, which already stops new items, also holds
every emote on the frame it had, until a good second) and standing at
zero under reduced motion (every emote's first frame, still). An
emote's frame is the clock's time within its loop (the sum of its kept
frames' durations): every copy of an emote on screen shows the same
frame. Nothing of it is judged, logged or replayed.

**The proxy** (`GET /emotes/<provider>/<path>` on the private server):
Twitch's `emoticons/v2/<id>/animated/<theme>/<scale>` and FFZ's
`emote/<id>/animated/<1|2|4>` join its closed list; 7TV's `<n>x.webp` and
BTTV's `emote/<id>/<n>x` already passed; its 2 MiB cap and its types
(`image/gif`, `image/webp` among them) are unchanged. Found while
designing: the game asked for the providers' sets under `emotes/7tv/…`,
`emotes/bttv/…` and `emotes/ffz/…`, where the server's routes for them
are `7tv-api`, `bttv-api` and `ffz-api`; the game now asks for those.

**No device option.** The chat's rows are hidden from players until the
transports exist; reduced motion already stills every emote; and an
emote moves as its author made it in every chat client. If the study on
a busy chart finds moving emotes compete with the notes, a "Still
emotes" choice beside Chat opacity is the place for it.

**The scripted chat** (TESTING ONLY) gains animated emotes:
`anim:<kind>:<rrggbb>:<frames>:<ms>[:<aspect>]` draws a shape's frames in
code (turning and pulsing) for both shells, never a file; and real GIF
and animated WebP files made by code (`cargo run -p ddi-desktop
--example chat_images -- <dir>`), for a walk through each shell's
decoder (`mpfakechatimg=<path>`: `<id>.gif` for an animated emote,
`<id>.png` for a still one).

### Built and walked (round TW4, 2026-11-20)

All four of the maintainer's decisions of 2026-11-20 are built as
designed above: animated emotes (every provider's, both shells, both
styles), Falling beside opponents' fields (`ddi_chat::overlay::shown`,
`ddi_shell::chat::shown`), emotes alone where the field leaves no side
space, and the layer over a work's back layers (`scene::Segment::Overlay`).

The rule for "a phone held upright" is the layout's, not the device's:
no side of the field as wide as Falling's least column of messages
(4.5 lines and the margins, `Area::side_space`). The play's layout has
no named upright rule to reuse (the miniatures and lyrics read the
window's shape each their own way); this one also covers a narrow
desktop window and a field made as wide as the screen, where any item
crosses the lanes too.

Found in review and fixed: the work limit counted frames times the
canvas, so a 1 × 1 canvas with a thousand 1024 × 1024 frames passed it
(now each frame's own area or the canvas's, the larger, summed); an
animated WebP frame's own bitstream was not read, so it could claim
16383 × 16383 where a decoder sizes a buffer before it compares (now
refused unless it says the frame's size); the web read an answer whole
before checking its size (now refused by its length, or stopped once a
streamed answer passes 2 MiB). Each with a test that fails without it.

Frame costs, main-thread milliseconds a frame (means; the frame
recorder's whole frame on the web, `perf=1`; desktop's offscreen
recorded frame, the chat's own part 0.05 to 0.08). "Animated" is the
`animated` script, a screen full of animated emotes (40 items);
1280×720 and a phone held upright (390 × 844, 3× on the web):

| | off | on (`demo`) | animated | phone off | phone animated |
|---|---|---|---|---|---|
| Desktop, alone | 0.09 | 0.17 | 0.19 | 0.09 | 0.17 |
| Desktop, eight CPUs | 0.31 | 0.39 | 0.38 | 0.31 | 0.37 |
| Chrome (WebGPU) | 0.46 | 0.55 to 0.62 | 0.68 to 0.91 | 0.64 | 0.68 |
| Firefox (Playwright's build, WebGL2) | 0.80 | 1.00 | 1.02 | 0.82 | 0.96 |

TW2 measured 0.09 / 0.15 / 0.17 (desktop), 0.56 / 0.61 / 0.68 (Chrome)
and 0.86 / 1.00 / 1.06 (Firefox) for off, on and flooded: animated
emotes stay in that range, and the chat's own part on the web is 0.10
to 0.20 (Chrome's animated pair of runs: 0.68 and 0.91, the second with
a p95 of 2.2 ms, frames the chat did not cost: its part 0.19). The
numbers are from runs at a load average of 6 to 14 (other work on the
machine); a first Firefox run at 40 to 110 read two to three times as
much, and its layer shed and held its emotes for whole seconds, as
designed.

Walked with the scripted chat under the testing switch (pictures and
results under `target/scratch/mp/tw4/shots/`, the walk
`target/scratch/mp/tw4/walks/chat.mjs`): in Chrome and Firefox, a
practice stage of eight CPUs with Across chosen fell in columns beside
the miniatures; its animated emotes from GIF files, from animated WebP
files (both through the browser's `ImageDecoder`) and drawn in code each
showed another frame at a read 0.17 to 0.4 s later, every still emote
the same frame, and every copy of an emote on screen one frame (40 to 56
copies a read); reduced motion held every emote on its first frame (the
clock at 0); a solo play kept Across with messages; a phone held upright
showed emotes alone. Desktop
offscreen (`chat_offscreen`'s `animated_emotes_move_and_the_layout_decides_the_style`)
the same, and `works`' `the_chat_is_drawn_over_a_works_back_layers` the
work's case (no bundled work exists: a generated one, compared with and
without the chat). Judging with the chat on and off: TW2's test passes.

Waiting for the spike, beside what TW3 listed: the providers' real
answers for animated emotes (7TV's `animated` and file names, BTTV's
`animated`, FFZ's `animated` object), and real emote files through the
proxy.
