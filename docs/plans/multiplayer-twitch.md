# The host's Twitch in a room: bans mirrored, chat behind the play

Status: designed (2026-11-20), and built as far as it can be without
Twitch itself: round TW1 (room model revision 18, see "Round TW1" in
`multiplayer-prototype-progress.md`) the model, Kick, the link and the
mirror behind a fake Twitch; round TW2 part C's pieces that need no
Twitch ("T1 and T2" below, and its section at the end); round TW3 joined
the two and walked them on a local server (its section at the end).
Round TW6 (2026-11-21) built the chat's transports and its subscribe
route from Twitch's documentation and `barbed` without the spike, the
maintainer's own run being their verification ("Round TW6" at the end).
The maintainer ran it on their own channel the same day; since round TW7
(2026-11-21) the chat is available to players, a multiplayer feature
alone, both options Off by default ("Round TW7" at the end, with what the
run verified and what it did not). The maintainer answered
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

- Sign out deletes the link and revokes the token at Twitch. Since round
  TB the device remembers which permissions it used and asks Twitch for
  those used within 30 days again at its next sign-in, in the same step
  ("Round TB" below).
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
  mirror on. Since room model revision 24 (round R24) the kick is
  remembered on the person's row (`Member::kicked`) until they are let
  back in, and closing admission keeps them out as it keeps out a
  newcomer; the server asks anyone who left for the current invite
  (round RFs), so replacing it keeps them out too.
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
- **Falling**: items fall from the top over the whole width.

Since round TW5 both cover the whole screen, behind the play's field, the
opponents' fields and the HUD, in the device's chosen style everywhere,
messages on a phone held upright too ("Round TW5" at the end: the
maintainer's words; this replaces TW4's Falling beside opponents' fields
and emotes alone without side space).

Both styles, with emotes and with messages, come in one release
(answer 10).

### C7. The layer

After the backdrop (the picture, the movie, the generated background, a
work's back layers) and the dancing hand (since 2026-11-21), before the
field filter, the attack links, the miniatures, the field and the HUD. In the lane band the filter darkens
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

Named `chat-overlay-v1` since round TW5 (`chat-overlay-v0` retired:
its runs drawn once with "×N" are gone; "Round TW5" at the end), every
number provisional. Since round DE `chat-overlay-v2`: the limits below
tripled (24 emotes a message, 240 messages and 480 emotes on screen, 60
new messages a second; "Round DE" at the end):

- At most 80 messages and 160 emotes on screen and 20 new messages a
  second; one a chatter every 2 s.
- A message brings its first 8 emotes, every one drawn by itself; the
  rest are left out.
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
  picture. (Since round TW4 over a work's back layers, and since
  2026-11-21 over the dancing hand: "The layer in front of the dancing
  hand", at the end.)

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
the chat's rows are shown to players. (Round DC: the Helix client's two
list calls are `barbed`'s generated request and response types now, the
rest of it `barbed`'s builders already; none of it run against Twitch.
Where `barbed` reads an answer differently from the hand-written reader
it replaced, `docs/plans/multiplayer-prototype-progress.md`, "Round DC".)

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
field filter's band and the field (C7 had it under them; since
2026-11-21 over the dancing hand too, "The layer in front of the dancing
hand", at the end).

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

## Round TW5 (2026-11-20): the chat as an effect behind the fields

The maintainer, after seeing the scripted chat: "I see the emotes demo
now. I think it's a good start but it's different than what I had in
mind. The 4x <emote> is sort of the opposite of what I want, I don't
want to summarize emotes, I want to let them show individually to make
some visual impression. Also, the across didn't work (it showed as
falling) the falling was avoiding the field/miniatures but I want them
to be behind the fields not in between them, it's not necessarily going
to be easy to read or to notice everything, but that's fine, it should
work like an added effect to convey what's going on in chat while the
play is going." And, the same day: "Allow messages on upright phones
too." So the layer is an ambient effect, not something to read. This
replaces, of what C6, C9, "Placing" (round TW2) and round TW4 said:

- **The summarizing** (C9: "a run of the same one is drawn once with a
  count", five runs a message): every emote is drawn by itself. Four
  of an emote in a message are four; ten chatters sending one emote are
  ten. There never was a filter turning repeated messages into a count
  (a chatter's 2 s is shedding, not summarizing).
- **The routing around the play** (Across's rows clear of the receptors'
  rows and the HUD, from the side away from the receptors; Falling's
  columns in the side space clear of the miniatures, below the score):
  both styles cover the whole screen, behind the play's field, every
  miniature, the HUD and the judgment. The field filter's band still dims
  the layer in the lanes (it is drawn into the backdrop under the
  filter), which is what keeps the notes readable; no keep-out was kept.
- **Falling forced beside opponents' fields** (round TW4, from a
  description the maintainer has now seen built): the device's chosen
  style is the style everywhere.
- **Emotes alone where the field leaves no side space** (round TW4): a
  phone held upright shows messages too; the room's ceiling and the
  device's option alone decide what shows.

What stays: the room's ceiling and the device's option, the testing
switch, C11's filters, moderation taking an item off (a scattered
message's every emote with it), animated emotes and `anim-limits-v0`,
one host clock, the hold under load, the layer over a work's back
layers, judging identical with the chat on and off.

### `chat-overlay-v1` (PROVISIONAL, `ddi_chat::overlay::CHAT_OVERLAY_V1`)

| | v0 | v1 |
|---|---|---|
| Emotes a message brings | 5 runs, a run drawn once with "×N" | the first 8, each drawn; the rest left out (counted in the debug state's `cut`) |
| On screen | 40 items | 80 messages and 160 emotes |
| New a second | 8 items | 20 messages |
| One chatter | every 2 s | every 2 s |
| Where | rows and columns around the field, the HUD and the miniatures | the whole screen |
| Past the budgets or the room | dropped | dropped (never queued, never a number) |

A message is dropped whole when it would pass a budget. The space is
the budget that bites in a burst: an item comes in only where nothing
else is still coming in across its width (and where it cannot catch the
one ahead of it), so a hype of 25 chatters a second fills the entering
edge and the rest finds no room (in the study's `burst` script, of 147
messages about half placed, the rest no room). A message of fifty
emotes is eight emotes on screen: never the screen's.

### The look: three candidates

`ddi_chat::overlay::LOOKS`, shown to the maintainer in pictures and
frame sequences (`target/scratch/mp/tw5/gallery.html`, not committed):

- **as-built**: items a line tall (a 22nd of the screen's height),
  emotes as tall as the text, every item alike: a burst reads as rows
  of a grid.
- **larger**: emotes 1.6 lines in an emote-only item and 1.35 inline in
  a message's line, the text a line, centered with them; sizes vary by
  up to 15% and times on screen by up to 20% from item to item.
- **scattered** (recommended, `chat-overlay-v1`'s own until the
  maintainer chooses): as larger, sizes ±20% and times ±25%, and an
  emote-only message's emotes each coming in by itself, 0.12 s apart, at
  a place of its own: a burst is a cloud of emotes rather than rows. A
  message with words keeps its emotes inline.

Nothing garish: the one quiet text color, the option's opacity, the
filter's dimming; reduced motion still stands everything in fixed places
(anywhere on the screen now) and holds every emote on its first frame.
`mpfakechatlook=<name>` (desktop `--mp-dev-chat-look NAME`) draws
another candidate for a walk.

No "Still emotes" option for now (decided 2026-11-20).

### The global-emotes demo (TESTING ONLY, `ddi_chat::global`)

The maintainer: "We could also test a demo with an assortment of global
animated emotes from all 4 providers." `mpfakechat=global` (desktop
`--mp-dev-chat global`; on the web taken only where the device's
"Twitch chat in rooms" is on since 2026-11-25, round HY, so that a
link cannot make a visitor's browser ask the providers) reads the four providers' global sets by the
real chat's code: 7TV's, BetterTTV's and FrankerFaceZ's asked directly,
through our proxy after a failure; Twitch's, which no page may read
without a token, through a new route of our server,
`GET <api>/emotes/twitch-api/chat/emotes/global`: Helix's Get Global
Emotes asked by the server with the app's own token (client
credentials), under the proxy's grammar, cap, cache and rate. From each
set an assortment (6 animated, 3 still, by a hash of their ids: two runs
alike), played in bursts and singly; the images asked as a real chat's
(directly, then the proxy). "Emotes from: Twitch" asks no other
provider. `__DDI_DEBUG.chat.global` says what each provider answered;
`providers` and `proxied` the images by provider and the hosts that
needed the proxy.

The third-party lists as they answered on 2026-11-20 (recorded once with
the maintainer's leave, kept local, never committed) showed the shapes
the parsers now follow, and what is left out of what is drawn: 7TV's
zero-width emotes (the set's `flags` bit 1: overlays meant for the emote
before), BetterTTV's and FrankerFaceZ's modifiers, FrankerFaceZ's hidden
emotes and its sets outside `default_sets`; 7TV's word is the set's
`name` (an alias where it differs from `data.name`), its file `2x.webp`
exactly (AVIF and GIF files are listed beside it). Read so: 7TV 42
emotes, 18 animated; BetterTTV 56, one animated; FrankerFaceZ 9, none
animated (its `animated` object stays written from the documentation).
So the demo's assortment has animated emotes where a provider has them:
7TV six, BetterTTV one, FrankerFaceZ none. The committed fixtures are
shaped as those answers, their names and ids invented; Twitch's is
written from its documentation. UNVERIFIED until the maintainer runs it.

### Built and walked (round TW5, 2026-11-20)

Tests: no count is ever drawn (`no_count_is_ever_drawn`, a property over
random floods: every drawn item shows its message's emotes, the first
eight, a repeated one as often as it came; `Glyphs` has no count any
more), the per-message and on-screen caps, both styles' places over the
whole screen (`both_styles_go_over_the_whole_screen`: positions behind
the field's band and behind a miniature's slot), moving items never
overlapping in any look, the scattered look, the chosen style holding in
a stage of eight CPUs offscreen on desktop
(`animated_emotes_move_and_the_chosen_style_holds_everywhere`: emotes
drawn inside the player's field and inside miniatures' slots), a phone
held upright showing messages; the demo's reads, choices and report
(`ddi_chat::global`), its fixtures as files played offscreen
(`the_global_emotes_demo_plays_from_its_fixtures`), the server's route.

Walked in Chrome and Firefox (`target/scratch/mp/tw5/walks/chat.mjs`,
every request outside localhost aborted) and on desktop offscreen: a solo
play in each style, a practice stage of eight CPUs in each style (emotes
behind the main field and the miniatures), reduced motion (every emote
still, the clock at 0, fixed places over the whole screen), a phone held
upright with messages, and the global-emotes demo over its fixtures
(every set read, BetterTTV's set and images and Twitch's through the
proxy's paths, `proxied` naming their hosts; "Emotes from: Twitch"
asking no other provider). No count anywhere in any walk.

Frame costs, main-thread milliseconds a frame (means; the web's frame
recorder's whole frame, `perf=1`; desktop's offscreen recorded frame,
medians in brackets). The machine's load average was 15 to 21 through
these runs (other work on it), so they read high and noisy; the chat's
own part is the steadier number.

| | off | on (`demo`) | flood (50/s) | animated burst |
|---|---|---|---|---|
| Desktop, alone, 1280×720 | 0.39 (0.15) | 0.76 (0.29) | 1.01 (0.58) | 0.27 (0.20) |
| Desktop, eight CPUs | 0.67 (0.44) | 1.21 (0.63) | 1.29 (0.70) | 0.56 (0.48) |
| Desktop, phone upright | 0.14 (0.10) | 0.20 (0.15) | 0.35 (0.28) | 0.19 (0.17) |
| Chrome (WebGPU), 1280×720 | 0.53 | 0.75 | | 0.71 |
| Chrome, phone (390×844 at 3×) | 0.57 | 0.78 | | 0.72 |
| Firefox (WebGL2), 1280×720 | 1.03 | 1.20 | | 2.33 (shedding: the layer held) |
| Firefox, phone | 1.42 | 2.48 | | 1.43 |

TW4's were 0.09 / 0.17 / 0.19 (desktop), 0.46 / 0.55–0.62 / 0.68–0.91
(Chrome) and 0.80 / 1.00 / 1.02 (Firefox) at a lower load. The chat's own
part: desktop medians 0.05 to 0.31 ms (0.09 to 0.19 in TW4's flood), the
web's 0.11 to 0.4 ms. More on screen costs more (up to 80 messages and
160 emotes, against 40 items): the text runs are the larger part, the
emotes stay one batch from the atlas, the scene's batches are unchanged
(31 alone, 71 with eight CPUs, with the chat on or off), and the caps
bound it: the most a frame drew was 95 items and 95 emotes.

## The layer in front of the dancing hand (2026-11-21)

The maintainer: "About the twitch chat, I'm slightly leaning into having
the layer in front of the dancing hand (but of course always behind the
notes field)." Built: the chat's layer is drawn over the dancing hand
(the field's underlay, in a solo play over the frame and in a stage in
the band behind the field: both the same picture to the renderer), as a
scene segment of its own right after the hand's quads
(`scene::Segment::Overlay`, `Renderer::render`'s `overlay_at` the end of
the back quads, a work's back layers then the hand), still under the
field filter's band, the attack links, the miniatures, the field, the
HUD and the judgment, and over a work's back layers as round TW4 made it.
Where a play has neither a work's back layers nor the hand, it is drawn
with the backdrop as before.

- The filter's band dims it in the lanes as before: with a hand (or back
  layers) the band is a quad drawn after the back quads
  (`filter_path`), so after the layer too.
- No pass more: the layer's segment joins the scene pass the field's
  sprites are drawn in (`the_chat_over_the_hand_adds_no_pass`); with the
  chat off there is no segment at all. Measured through the field
  preview's renderer (a hand over the frame, 300 frames, the main thread's
  recording): chat off 0.079 to 0.082 ms, on 0.073 to 0.085 ms, 4 passes
  and 2 conversions in every case, before the change and after it alike.
- `chat_order_offscreen`: a red stand-in hand and a green layer over the
  whole frame, in a solo play's place and a stage's: the layer's green
  over the hand's red, dimmed in the lanes, the receptors over it (it
  fails on the order before).

## Round TW6 (2026-11-21): the live connection, built and waiting for the maintainer's run

The maintainer, asked whether to run the spike (step 0) first or build
the live chat now from the documentation and `barbed` and let their own
live test verify it: "Build it now". So nothing here has met Twitch or a
provider; everything was tested against fakes, and what the maintainer's
run reports names each assumption, so a wrong one is found in one paste.
It replaces of step 0: the subscription made by our server for a socket
the game opened, the ten seconds, the costs, the four chat types under
one token, the events' real shapes and the providers' channel answers
are all read and reported by the real code on the maintainer's first run
(`--mp-dev-chat live`); the spike's other questions (a second
authorization's scopes, the chatter list, the lists with an app token,
three devices against a fourth) stay the spike's, or are answered by
using it.

**The server** (`../ddi-server`): `POST /rooms/<id>/chat/subscribe
{session_id, messages}` and `POST /me/chat/subscribe` (one's own channel,
C14): signed in, a session id of a token's alphabet; in a room, a present
member of a room that shows chat (`RoomCore::chat_check`: `403
not_a_member`, `409 chat_off`), the channel the room's (B3/C1) and
messages only where both the room's ceiling and the device show them;
then the person's `TwitchLink` makes the subscriptions with the person's
own token (`/subscribe`, refreshed or validated first as every use is):
`barbed`'s request (`helix::create_eventsub_subscription_request`;
`channel.chat.message` and `…message_delete` by its builders, the clears
as literal requests of its type), its body sent without the `null`s of
another transport's fields (Twitch's reference refuses a WebSocket
subscription with fields of another method "included"), `barbed`'s send
under a deadline of ours (`SUBSCRIBE_DEADLINE_MS`, 2 s; the four at once,
so a slow Twitch costs one deadline inside the socket's ten seconds), its
answer read by `barbed`'s parser. Each type's answer goes back to the
device with our word for it (`enabled`; `bad_session`, a 400 naming the
session; `bad_request`; `refused`, 401, which deletes the link;
`forbidden`, 403; `exists`, 409; `limits`, 429; `unreachable`, slow or
down; `unknown`), Twitch's status, the subscription's status and `cost`,
`total`, `total_cost`, `max_total_cost`, Twitch's error text and the
milliseconds; the logs count them naming nobody (`chat: subscribe
(room|own): message enabled http 202 cost 0 total 1 cost 0/10 in 140
ms; …`). Refusals of the route: `409 scope_missing` (`need: ["chat"]`),
`403 twitch_refused`, `502 twitch_unreachable`, `400 bad_session`, `403
no_twitch` (a bot), `429 too_many_subscribes` (20 a minute a person, a
room's in its object, one's own in the Worker's memory). The fake Twitch
answers it as the reference documents each answer, its mode set by `POST
/auth/fake/twitch {chat: answers|forbidden|limits|bad_session|refused|
exists|slow|messages_only}`. No configuration changed (`wrangler.toml`
untouched).

**The connection** (`ddi_chat::live`, pure, natively tested on a fake
clock; `live-chat-v0`, PROVISIONAL): the welcome's session subscribed
through our server, a failure in passing asked again once a second later
while four of Twitch's ten seconds are left; a watchdog of the stated
`keepalive_timeout_seconds` and 3 s; Twitch's reconnect followed to its
address, the old socket kept (its events taken, a notification sent twice
taken once, by `message_id`) until the new one welcomes, then closed,
nothing subscribed again; a revocation of the messages' subscription
ends it quietly, of another type is reported; every close code read
(4001, "we sent something", gives up: it would repeat); a loss opened
again after 0, 1, 2, 5, 10, 30, 60 s and given up quietly after eight in
a row; a refusal that would repeat (no scope, not a member, the chat off,
Twitch's 403 or 429) gives up at once. Only the messages' subscription
decides: a moderation type refused (review note 2's case) is reported and
the chat comes. The channel's emote sets (`ddi_chat::channel`): 7TV's,
BetterTTV's and FrankerFaceZ's channel and global sets read when the chat
starts as round TW5's demo reads the globals (directly, then the proxy),
a channel's 404 no failure; Twitch's own from the fragments.

**Where it runs**: a play's chat layer drives it in the session's tick
(`ddi_session::chat`, `ChatSource::Live`, over a shell's `LiveIo`: the
web's `WebSocket` and `fetch`, desktop's tungstenite on a thread a socket
as the room's, ureq threads), from the play's start to its end (the
session dropped closes its sockets; replaced in a room by round DE's
connection kept for the room, "Round DE" at the end): "ends with the stage", the brief's
choice over keeping a socket across a room's stages, simpler and with no
socket left over between plays (a stage's first second of chat is spent
on the welcome). It runs only where a play may show chat (the device's
option, the room's ceiling and channel; outside a room only under the
development script) and, until the maintainer verified it, only under the
testing switch (`ddi_ui::state::twitch::chat_rows_shown`): **players see
nothing new**. Once verified, that function's one line becoming `true`
shows the rows and lets a room's live chat run.

**The development script** (TESTING ONLY): desktop's `--mp-dev-chat live`
(the web's `mpfakechat=live`) reads `/me` and shows the signed-in
person's own channel's chat in every play outside a room (a solo play,
autoplay, CPU practice's stages) through exactly a room's code, the own
route; with it the rows show, and the device's "Twitch chat in rooms"
turned on takes the trip for the chat's scope as before. In a room the
host's "Show my Twitch chat" and each device's option carry the room's
channel to every member who turned it on (each device under the switch).
For tests: `--mp-dev-chat-socket URL` / `mpfakechaturl` a local fake
socket, `--mp-dev-chat-at` / `mpfakechatimg` the providers' addresses
from a local base; `--mp-dev-chat-record DIR` writes every raw socket
frame there (people's chat in them: only where the maintainer asks; it
says so on stderr).

**What the run reports**: desktop's stderr lines led by `[ddi-chat]` (the
web's console the same, and `__DDI_DEBUG.chat.live`): connecting, open in
N ms, welcomed in N ms with the keep-alive Twitch stated, each
subscription with Twitch's status, cost, total, total cost and maximum
and its error, live N ms after the welcome, losses and why, reconnects,
revocations, each emote set by provider (read directly or through the
proxy, or no set, or failed and why, its emotes and how many animated);
at the play's end what was received (messages, deletes, user clears,
clears, other types, keep-alives, duplicates), what was shown and left
out by each filter and dropped by each budget, taken off by moderation,
the images (ready, animated, in flight, failed, hosts through the proxy),
and the redacted shapes of the first five messages the readers did not
understand (every text replaced by its length, the structure's own words
kept: never a token, our session or a person's chat).

**Expected from Twitch** (to be confirmed by the run): `cost` 0 for each
of the four types and `total_cost` 0 of a `max_total_cost` of 10, since
the documentation says a subscription costs nothing where the user it
names (the reader, `user_id`, who authorized the app) has authorized the
app; `total` grows by four a device.

**Tested without Twitch**: the state machine (every state and close
code, the ten seconds, the watchdog, the hand-over, revocations,
refusals, backoff and giving up, the redaction), the channel's sets, the
server's request (its body exactly), its answers' reading (each status)
and the fake, the room's check and the rate limit; the session's live
chat over a fake shell (its route, its report, its sockets closed with
the play) and judging identical with it on and off; desktop's sockets
against a local fake (`the_live_chat_runs_over_desktops_sockets`: a
reconnect's hand-over through real sockets) and offscreen plays
(`live_chat_offscreen`: a solo play and a practice stage). Walked on a
local server with its fake Twitch and a local fake EventSub socket
(`target/scratch/mp/tw6/`, not committed): the route through every answer
and refusal (`walks/route.mjs`), and in Chrome and Firefox a solo play's
live chat (normal, Twitch's reconnect, a silent socket, a refused
subscription, a revocation, messages not understood, the option Off) and
an online room's stage with a guest bot (the room's route), every request
outside localhost aborted.

**Unverified** until the maintainer's run, by name: that a subscription
our server makes binds to the device's socket; the costs; that the three
moderation types take the `user:read:chat` token; that Twitch accepts the
request without the `null`s (`barbed`'s own type would send them); the
events' real shapes (the fixtures are the documentation's); the
providers' channel answers (`/v3/users/twitch/<id>`,
`/3/cached/users/twitch/<id>`, `/v1/room/id/<id>`) and their 404 for a
channel without a set; Twitch's reconnect and keep-alive in practice; the
desktop's TLS handshake to `eventsub.wss.twitch.tv`.

## Round TW7 (2026-11-21): available to players, a multiplayer feature alone

The maintainer, after running the live chat on their own channel: "I
just tested it and it seems to work. I think we should make it available
by default, we could keep a dev option to override which channel's chat
it connects to, so I can test with a busy channel." And the same day:
"we shouldn't show emotes on singlplayer plays, only on multiplayer
plays", then "For testing and under a flag it's fine. But singleplayer
should never require online connectivity, the chat/emotes background
should be purely an optional multiplayer feature."

**What a player has now.** Options → Display's "Twitch chat in rooms"
(and under it, while on, Chat style, Chatter names, Emotes from, Chat
opacity) and Host options' "Show my Twitch chat" show to everyone, both
Off by default; the testing switch (`chat_rows_shown`, `chat_testing`)
is gone, and the scripted chat's line is `scripted_ceiling` (TESTING
ONLY). The chat shows only in an online room's stage, where the host's
"Show my Twitch chat" and the device's option are both on, never more
than the host allows (C1): the room's channel through the room's route.
CPU practice has no host's Twitch and shows none; a singleplayer play
shows none, opens no socket and makes no request for it, whatever the
options say (`ChatConfig::under_room`: a play outside a stage reads only
the development script's channel). Turning the option on where the
device is not signed in (singleplayer, or multiplayer not opened this
session) only saves it: nothing is asked of our server or Twitch; signed
in without the chat's permission it takes the trip through Twitch as
before (C1, round TW3). The words: the option's description says what it
shows, whose chat, that the host chooses, never in singleplayer, the
permission and where it is disconnected; Host options has a line under
its Twitch rows saying what "Show my Twitch chat" does and that each
player turns it on for their own device; Options → Data's Twitch access
says a player connects it to see a host's chat. (C14 and step 4, a
streamer's own chat in singleplayer, are set aside by the maintainer's
rule: singleplayer never needs the network for this.)

**When it cannot run** (`ddi_chat::live::Trouble`, said once and quietly,
never during a play):

- The device's access does not allow reading chat (never allowed,
  disconnected, ended by Twitch): the option's row says "Your Twitch
  access does not allow reading chat, so no chat shows. Turn this Off and
  on again to allow it on Twitch." (where the online state last read the
  access, never read for the asking), and in a room that shows chat the
  status line says once "To see the host's chat, allow it on Twitch."
  (`LinkNotice::TwitchChatNoAccess`) with the step itself beside it
  (round FX: "Allow on Twitch", `screens::room::ALLOW_CHAT`, and More's
  "Allow Twitch chat" while it is so, `RoomClient::chat_allow_offered`,
  `allow_chat`: the same trip through Twitch the option's row starts,
  back to the room, the chat showing from the next stage; canceled,
  nothing changes, the line comes back and the room asks no more that
  session, `CHAT_DECLINED`, More still offering it; never in CPU
  practice or singleplayer).
- After a stage whose chat could not show, one line under the stage's
  standings (`StageOutcome::chat`, `RoomClient::set_chat_trouble`,
  `StageTable::chat`), by why the connection gave up: the access ("…your
  Twitch access does not allow reading chat. In the room, More → Allow
  Twitch chat asks Twitch for it."), Twitch refusing
  the reader (its 403 or its limits, or a revocation other than the two
  below), the channel gone (a revocation `user_removed`, or a named
  channel Twitch does not know), the connection failing (losses in a row,
  or never live), and, TESTING ONLY, a named channel refused to someone
  who is no administrator. A room's own change (its chat turned off, the
  player no member) says nothing.

**The development override of the channel** (TESTING ONLY): desktop's
`--mp-dev-chat live:<login>` and the web's `mpfakechat=live:<login>` show
that channel's live chat wherever the live flag plays (solo plays, CPU
practice's stages) and in an online room's stage in the room's channel's
place on this device alone (`RoomChat::on_this_device`; never what the
room or its other members see, never a way to make a room show another
channel). It is read with the signed-in person's own access through
`POST <api>/me/chat/subscribe {session_id, messages, channel}`, which our
server allows only for `ADMIN_TWITCH_IDS` people (`403
channel_not_allowed` for anyone else, `400 bad_channel`, `404
no_such_channel`), the login found with Get Users (`barbed`'s
`users::GetUsersRequest`, the checks' 2 s deadline); its answer names the
channel (`channel`, `channel_id`), whose emote sets are then read. The
report's first line names the channel and its route; a line names its
Twitch id once our server found it. Plain `live` stays the person's own
channel.

**The run's report kept**: desktop writes the `[ddi-chat]` lines into the
data folder's `chat-report.txt` too (replaced by the first live chat of
each run of the app, every play's lines after it, the same redacted
lines, 1 MB at most), and says where on stderr; so the next run's report
can be sent later.

### Verified by the maintainer's run on 2026-11-21

Desktop, their own channel, `--mp-dev-chat live`, a solo play; the log
they pasted:

```
[ddi-chat] +0 ms live-chat-v0: connecting to eventsub.wss.twitch.tv
[ddi-chat] +599 ms socket 1: open in 599 ms
[ddi-chat] +599 ms socket 1: welcomed in 599 ms (keepalive 10 s as Twitch stated it)
[ddi-chat] +1423 ms subscribed channel.chat.message: enabled (HTTP 202, status enabled, cost 0, total 1, total cost 0 of 10)
[ddi-chat] +1423 ms subscribed channel.chat.message_delete: enabled (HTTP 202, status enabled, cost 0, total 1, total cost 0 of 10)
[ddi-chat] +1423 ms subscribed channel.chat.clear_user_messages: enabled (HTTP 202, status enabled, cost 0, total 1, total cost 0 of 10)
[ddi-chat] +1423 ms subscribed channel.chat.clear: enabled (HTTP 202, status enabled, cost 0, total 1, total cost 0 of 10)
[ddi-chat] +1423 ms socket 1: live, subscribed 824 ms after the welcome
[ddi-chat] emotes 7tv channel: read direct (1 emotes, 1 animated)
[ddi-chat] emotes 7tv global: read direct (42 emotes, 18 animated)
[ddi-chat] emotes bttv channel: read direct (1 emotes, 1 animated)
[ddi-chat] emotes bttv global: read direct (56 emotes, 1 animated)
[ddi-chat] emotes ffz channel: read direct (3 emotes, 2 animated)
[ddi-chat] emotes ffz global: read direct (9 emotes, 0 animated)
[ddi-chat] +108848 ms stopped
[ddi-chat] received 13 frames: 1 chat messages, 0 deletes, 0 user clears, 1 clears, 0 other; 10 keepalives, 0 duplicates, 0 not understood
[ddi-chat] shown: 8 items from 1 messages; left out 0 (commands 0, links 0, cheers 0, shared chat 0, nothing to show 0); dropped: screen full 0, 20 a second 0, a chatter's 2 s 0, frames late 0, no room 0, no image 0; taken off by moderation 0; emotes past eight 2
[ddi-chat] images: 8 ready (8 animated), 0 in flight, 0 failed, 0 hosts through our proxy
```

What it verifies, each taken off round TW6's unverified list:

- A subscription our server makes binds to the socket the game opened (a
  message and a clear arrived on it).
- All four types are accepted with the one `user:read:chat` access, the
  three moderation types included, and Twitch takes the request body as
  we send it, without the `null`s (HTTP 202, `enabled`).
- Each costs 0, the total cost 0 of 10.
- Twitch states a keep-alive of 10 s.
- A real chat message's shape and a clear's are understood (0 not
  understood).
- The three providers' channel and global sets are read directly from
  desktop (`/v3/users/twitch/<id>`, `/3/cached/users/twitch/<id>`,
  `/v1/room/id/<id>` and the globals), FrankerFaceZ's `animated` reading
  included (2 animated in the channel's set).
- Animated images decode (8 of 8 ready, all animated).
- Desktop's TLS handshake to `eventsub.wss.twitch.tv` works.

Also from it: one message brought ten emotes, eight were drawn and two
cut by the per-message cap (`chat-overlay-v1`'s eight, "emotes past
eight 2"), as designed.

**"total 1" on each of the four lines.** Twitch's `total` is the number
of subscriptions the token has made, as the create call answering it
counted them. Our server makes the four calls at once (round TW6), so
each was most likely answered before the others were recorded, and each
says 1: it is each call's own count, not four in all (one device, one
socket). The line prints each answer as Twitch gave it; it would read
more plainly as "total 1 at this call", a one-word change left for the
maintainer so this run's lines stay comparable with the next.

### Still unverified (no report has said them yet)

- A delete and a user's clear in practice (none arrived in the run).
- Twitch's reconnect message and a silent socket, in practice.
- The web shell's sockets and its cross-origin reads of the providers
  (and the proxy's fallback where one fails).
- Twitch's global emotes through our server's route.
- A channel with no third-party set (the providers' 404).
- More than one device of one person (Twitch's three sockets).
- A room's route with several members.
- The named channel's Get Users lookup against Twitch (round TW7; the
  fake Twitch answers it locally).

Built and checked without Twitch: the rows shown to everyone and both
Off by default; a singleplayer play with every option on building no
layer and opening no socket (`a_singleplayer_play_shows_no_live_chat_whatever_the_options`,
and in the browsers every request of a fresh singleplayer session with
both options turned on recorded: none to our server's API or beyond the
page's files, no socket); each trouble from each refusal, answer and
revocation (`each_failure_is_one_trouble_for_the_player`); the results'
line (`with_attacks_the_lines_count_them_and_the_player_s_battle_is_summed_up`);
the room's notice (`a_player_whose_access_cannot_show_the_chat_is_told_once`);
the named channel through the session (`a_named_channel_is_asked_for_by_its_login`)
and the server (`a_channel_is_found_by_its_login`,
`the_fake_link_finds_a_channel_or_says_why_not`); the report file
(`the_live_chat_runs_over_desktops_sockets`,
`the_report_file_is_replaced_and_bounded`). Walked in Chrome and Firefox
on a local server and a local fake EventSub socket (round TW7's
`target/scratch/mp/tw7/walks/tw7.mjs`, `route.mjs`).

## Round TB (2026-11-22): the bans and the chat together, and permissions kept

The maintainer: "Does the 'Twitch bans' work together with 'Show my
Twitch chat'? Because both need an extra permission from the Twitch
connection and maybe enabling both is not working correctly (at least on
desktop)", then, on desktop: "creating a private room, turning on
'Twitch chat' which goes through the flow and works, trying to turn on
'Twitch bans', doesn't even open the pop-up with the code, but opens the
web page to sign-in", and "when I closed the room the Sign in pop up
appeared". It did not work, for three reasons, each fixed:

1. **The second trip gave up the first permission** (the server, both
   shells). A trip asks Twitch for what the link has plus what is wanted
   (review note 1), but the link's `have` was read from a `scopes` field
   its `/info` never said, so every trip asked for its own feature alone
   and Twitch's new token replaced the link's: the room's setting asked
   on the way back (bans and chat) needed the other permission again and
   was never made, each later press started another trip for the other
   one, and the mirror's next check found no `moderation:read` and
   turned the room's whole Twitch setting off. `/info` says the link's
   scopes now (`twitch::info`, `twitch::have`).
2. **Desktop's second trip showed its code under the room.** A trip ends
   as a sign-in does, its room "entered"; the multiplayer home under the
   room took that for a room to show and pushed a second room screen over
   the first's Host options, so the next trip's dialog with its code was
   opened by the first screen's Host options, under the second (the
   browser opened, nothing to type the code from), and closing the room
   showed the home's own Sign in dialog for it. A trip enters no room now
   (`Online::twitch_trip`), the home takes a room to enter only while it
   is shown, and a trip left over when the home shows is stopped, never
   shown as a sign-in.
3. **Host options' second row could undo the first.** Each press asks for
   the whole setting; the other row was read from the room's state, so
   a second row changed before the room had said the first (its answer in
   flight) asked for the first row as it was. The rows show and build on
   what was asked until the room says it (`RoomClient::twitch_shown`).

**Permissions kept** (the maintainer: "we should avoid downgrading the
permissions, at least within a time-frame, so that turning off and on
again is not annoying, maybe if a permission isn't used for 30 or 60
days, it isn't requested on the next sign-in"). Before, a person lost a
permission and went through Twitch again: at every sign-out (the link
deleted), after 30 days unused, when Twitch refused a refresh, by
Disconnect, and (reason 1 above) whenever another feature's trip
replaced the link. Turning a feature off never touched the link, and a
plain sign-in, on any device, leaves it as it is. Now:

- The device remembers which permissions it used and when
  (`Settings::twitch_used`: `bans`, `blocks`, `chat`, the time of their
  last use, written at most once an hour). Used means: a room this
  person hosts with the feature on, or a room whose host's chat this
  device shows (its "Twitch chat in rooms" on and its access allowing
  chat) (`RoomClient::mark_twitch_used`).
- A sign-in asks Twitch for those used within `TWITCH_KEEP_DAYS` (30, the
  server's own horizon for an unused link; 60 is that one number) in the
  sign-in's own step: the web's `login?keep=bans,chat`, the desktop
  attempt's `keep`. The server signs in whoever Twitch signs in and keeps
  their link with those scopes, unless the link they have already grants
  them all (another device's): then it stays and the new token is
  revoked. Signing out still deletes the link on the server.
- Disconnect in Options → Data forgets them on that device: nothing is
  asked at the next sign-in. Resetting the settings does too.
- Nothing new is shown when it works; Options → Data's row says what is
  connected, as before.

## Round DE (2026-11-22): the connection kept for the room, and larger limits

The maintainer's decisions of 2026-11-22: the live chat's connection
stays open for as long as a player is in a room that shows chat, so the
chat is there from a stage's first frame (round TW6 opened it at each
stage's start and closed it at its end: the chat came about a second
into every stage); and "Increase both limits, maybe triple of what they
are."

### The connection kept for the room

- **When**: from the moment a member is in an online room that shows its
  host's chat (the room's ceiling on), this device's "Twitch chat in
  rooms" is on and its Twitch access allows chat (as `/me` last said it),
  until they leave the room or any of those turns off
  (`RoomClient::chat_kept`, a `ddi_ui::state::twitch::KeptChat`: the
  room's channel as this device shows it, at the device's level under the
  ceiling, and where the emotes come from). The room's client tells it as
  it ticks (`keep_room_chat`, every frame of a room's screens, the
  prompt's and the stage's ticks, and the web's kept room's timer under a
  hidden tab or a page over the room, as a room's stage treats them) to
  the keeper `ddi_session::chat` installs with the live chat
  (`keep_for_room`); a room's client dropped says nothing is wanted any
  more. CPU practice tells it nothing; a bot never has one.
- **Between stages** (`chat::Keeper`): the connection driven on the
  room's clock, nothing drawn, and nothing kept for later but the last
  `BUFFER_SECONDS` (2 s) of events, `BUFFER_MOST` (20) at most, which
  the next stage's first frame shows; the emote sets read once and kept
  warm. The keep-alive's watchdog, Twitch's reconnect, the backoff and
  giving up are the connection's own, as before (`live-chat-v0`).
- **A stage** takes it (`ChatLayer::new`: the room's connection where it
  holds one for this chat; the play's host clock drives it from there,
  going on from the room's time) and gives it back at its end
  (`give_back`: kept where the room still wants it, else closed with the
  stage's last lines). A stage that opened its own (the access not known
  yet when it began) gives it to the room at its end where the room
  wants it by then; the room opens none of its own while a stage holds
  one. A change of channel or of what shows closes it and opens another.
- **Outside a room** nothing changes: a singleplayer play opens no
  socket and makes no request whatever the options say
  (`a_singleplayer_play_shows_no_live_chat_whatever_the_options` still
  passes), and the development flags' live chat (`live`, `live:<login>`)
  in solo plays and CPU practice keeps its per-play lifetime.
- **Said**: the report's lines (desktop's `chat-report.txt` and stderr,
  the web's console) say the lifetime: `kept for the room: open until the
  room is left or its chat turned off`, `a stage took the room's
  connection (live, N events from the last 2 s)`, `the stage ended: N
  chat messages during it; kept for the room` with the stage's shown and
  images lines, and `closed for the room (why): kept N s, K stages took
  it` with what was received. `__DDI_DEBUG.chat.live` has `lifetime`
  (`room` or `play`) and `taken_from_room`; every page's
  `__DDI_DEBUG.chatKept` is the room's connection between stages
  (`held`, `lent`, `stages`, `kept_ms`, `buffered`, the connection's
  report).
- **Tests**: `a_rooms_chat_stays_connected_across_its_stages` and
  `a_stages_own_connection_is_given_to_the_room` (`ddi-session`, on a
  fake clock over a fake shell's side: one socket for two stages, the
  chat at each stage's first frame, the buffer's cap, closed on leaving,
  a change of what shows reopening it, a solo play's own connection
  closed with it), `a_room_tells_its_keeper_what_chat_to_keep_connected`
  (`ddi-ui`, over the fake server: alice's access allows chat, bob's
  does not; the option or the room's chat off; leaving; practice silent).
- **Walked** in Chrome and Firefox on a local server (its fake Twitch)
  and a local fake EventSub socket, every request outside localhost
  aborted (`target/scratch/mp/de/walks/room.mjs`): a guest with the
  chat's permission joined a room with no chat (no socket), the host
  turned Show my Twitch chat to Emotes then Emotes and messages (the
  guest's connection opened at once, closed and opened again for the
  messages: two sockets in all), two stages (each took the room's
  connection, live, with the last 2 s's 5 or 6 events: messages there
  at the play's first debug read, before the song's first second; the
  room held it again after each, 6 or 7 events buffered), then the
  guest left (closed: "kept 36 s, 2 stages took it", 116 chat messages
  received over it); a solo play with every option on and a session
  with the chat's permission opened no socket and asked nothing.

### `chat-overlay-v2` (PROVISIONAL, `ddi_chat::overlay::CHAT_OVERLAY_V2`)

| | v1 (round TW5) | v2 (round DE) |
|---|---|---|
| Emotes a message brings | 8 | 24 |
| On screen | 80 messages, 160 emotes | 240 messages, 480 emotes |
| New a second | 20 messages | 60 messages |
| One chatter | every 2 s | every 2 s |

The rate rises with the screen's caps because at 20 a second, with an
item on screen 7 to 8.75 s (Across, ±25%), at most about 175 messages
are ever on screen: the rate, not v2's 240, would have been what limits
a busy chat. A chatter's 2 s stays: it limits one person's flood, never a
busy chat of many chatters. Two things still bite before the caps, as
before: the space (an item comes in only where nothing else is still
coming in across its width; in a wall of 60 messages a second of 30
emotes, most are "no room") and, in a look that keeps a message's
emotes in one row (not v2's scattered one), the screen's width (21 of a
message's 24 at 1280 pixels). The scripts `wall` and `animated-wall`
(60 messages a second of 30 emotes each, all animated in the second)
test the new limits; `mpfakechatrules=v1` (the web) and
`ddi_shell::chat::set_rules` (desktop's study) lay the chat by v1 for
the comparison, TESTING ONLY.

**Frame costs**, v1 against v2 on the same build and the same machine
(load averages 5 to 11 through the runs, other work on it): a solo play
of Beatucada autoplayed, measured from its fourth second. Desktop:
offscreen, the recorded frame's main-thread milliseconds (mean; the
chat's own part in brackets), the test profile
(`DDI_CHAT_COST=cost cargo test -p ddi-desktop --test chat_offscreen study
-- --ignored --nocapture`). The browsers: the release build, the frame
recorder's whole frame (`perf=1`, mean), the chat's own part from the
debug state in brackets (`target/scratch/mp/de/walks/cost.mjs`).

| | off | on (`demo`) v1 → v2 | burst or flood v1 → v2 | `wall` v1 → v2 | `animated-wall` v1 → v2 |
|---|---|---|---|---|---|
| Desktop 1280×720 | 0.10 | 0.19 → 0.20 (0.09 → 0.10) | flood 0.23 → 0.28 (0.13 → 0.17) | 0.23 → 0.33 (0.09 → 0.18) | 0.21 → 0.28 (0.09 → 0.16) |
| Desktop, eight CPUs | 0.27 | 0.37 → 0.40 | flood 0.44 → 0.48 | 0.43 → 0.50 | 0.39 → 0.49 |
| Desktop phone 390×844 | 0.09 | 0.18 → 0.19 | flood 0.26 → 0.24 | 0.22 → 0.27 (0.08 → 0.13) | 0.21 → 0.26 (0.09 → 0.13) |
| Desktop phone, eight CPUs | 0.31 | 0.37 → 0.37 | flood 0.43 → 0.44 | 0.42 → 0.47 | 0.42 → 0.45 |
| Chrome (WebGPU) 1280×720 | 0.50 | 0.93 → 0.99 | burst 0.92 → 0.85 | 1.29 → 1.45 (0.33 → 0.51) | 1.12 → 1.48 (0.30 → 0.54) |
| Chrome phone (390×844 at 3×) | 0.66 | 0.95 → 0.95 | burst 0.90 → 0.88 | 1.24 → 1.25 (0.32 → 0.42) | 1.27 → 1.17 (0.32 → 0.41) |
| Firefox (WebGL2) 1280×720 | 0.90 | 1.10 → 1.10 | burst 1.05 → 1.09 | 1.21 → 1.28 (0.29 → 0.25) | 1.19 → 1.27 (0.20 → 0.36) |
| Firefox phone | 1.03 | 1.12 → 1.09 | burst 1.12 → 1.10 | 1.24 → 1.28 | 1.17 → 1.26 |

The most tripling costs anywhere is 0.36 ms a frame (Chrome, 1280×720,
the animated wall), 0.10 on desktop: under a millisecond everywhere, so
no smaller caps are proposed. What was drawn at once at most: at
1280×720 152 emotes in the wall with v2 (117 with v1), on a phone 104
(89); the 480 counts the emotes a message is let in with, of which many
find no place on their way in. Firefox's clock is a millisecond here,
so its medians read 1. The hold under load (no new items from a late
second) did not engage with v2 in any run, ordinary bursts included,
on Firefox too (once with v1, 12 of 89 reads of the phone's burst, the
machine's load). Round TW5's numbers were taken at load 15 to 21: off,
on and flooded 0.39, 0.76 and 1.01 on desktop, 0.53 and 0.75 in Chrome,
1.03 and 1.20 in Firefox, its animated burst 2.33 while shedding.

**The emote atlas at the new caps** (`ddi_chat::cache`, unchanged): 256
images kept, 768 of its 1024 cells for animated frames. The scripted
chats use eleven distinct emotes (four animated: 62 cells), far inside
it. A real chat's distinct emotes count: where more than 256 distinct
images are drawn in one frame (none of them may go), those past 256 are
not placed, their items go without them after the second's wait (`no
image`) and the image stays failed for the play
(`past_256_distinct_images_drawn_at_once_the_rest_go_without`); since
at most about 150 emotes were drawn at once in a 60-a-second wall, 256
distinct at once is unlikely. Animated frames bind sooner: twelve
emotes of 64 frames fill their 768 cells, and past that the least
recently drawn animation goes, or, where all are drawn this frame, the
new one is still (its first frame), as round TW4 built it; a chat with
more distinct animated emotes on screen at once than fit shows the rest
still. Not changed in this round: a decision for the maintainer if
animated walls of many distinct emotes matter.

## Round SD (2026-11-24): the room's chat setting applies at once

The maintainer: "let's go with applying every change at once." Before,
a stage took the room's chat setting (Host options' "Show my Twitch
chat") at its start and kept it to its end (the chat-touch review's
suspicion), while the room's kept connection already followed the new
setting: a host who turned the chat off because it turned abusive saw it
go on until the stage ended, and the room opened a second connection
beside the stage's.

Now a stage of an online room follows the room's setting while it plays
(`ddi_session::chat::Following`, read each frame from the room's client,
`LiveRoom::room_chat`, the same `RoomClient::stage_chat` a stage's setup
takes at its start):

- **Turned off**: the layer goes in that frame (the view cleared, the
  layer under the field drawn no more); its connection is given back,
  which the room's keeper closes since the room no longer wants it.
- **Lowered to emotes**: the same layer at the new level; the messages on
  screen (and waiting) go, the emote-only items stay, no words come
  (`ddi_chat::overlay::Overlay::set_level`); the connection is the room's
  new one for that level (its subscriptions differ: no moderation events),
  the old given back and closed, the emote sets kept until the new
  connection's are read.
- **Turned on, or raised to messages**: as at a stage's start: the room's
  kept connection taken where it holds one (its last two seconds of events
  at the next frame), else one of the stage's own, which the room then
  does not open a second of; emote sets as they arrive.
- **Another channel** (a new host's, `RoomState::twitch` naming another
  setter): the old layer off, then a new one on; the view is made again
  for it (`ChatLayer::id`, `ChatView::shows`).

Unchanged: the device's own option ("Twitch chat in rooms": never above
it; Off, no layer ever), the permission, the rule that a player has no
connection outside a room, CPU practice (its client follows nothing), the
development flags' solo chat, and the audience (none). Nothing of it is
judged or logged: a stage autoplayed with the setting changed five times
judges, scores and logs exactly as one without the chat, at 60 and 37
frames a second. No room model revision.

Tests: `a_stage_follows_the_rooms_chat_while_it_plays` (`ddi-session`,
the fake clock and the fake shell's side: lowered, raised, off, on, the
channel replaced, with the room's tick before or after the stage's in a
frame; one connection at most at any frame's end, none once off or left),
`following_changes_nothing_the_room_did_not_change`,
`a_stage_follows_the_hosts_chat_and_judges_the_same` (a whole stage over
`PlaySession`), `a_lowered_level_takes_the_words_off_and_off_takes_everything`
(`ddi-chat`), and `a_room_tells_its_keeper_what_chat_to_keep_connected`
(`ddi-ui`: the room's client's `room_chat`, none in practice).

Walked in Chrome and Firefox (the release build) over a local server
(its fake Twitch) and the local fake EventSub socket, every request
outside localhost aborted (`target/scratch/mp/sd/walks/chat-follow.mjs`):
a host and a guest with the chat's permission, the room's chat on
(messages), a stage; during it the host lowered the chat to emotes,
raised it, turned it off and on again by the room's route (as Host
options does). The guest's play showed each 47 to 127 ms after the
host's request returned (Chrome about 120 ms, Firefox about 50 to 75):
lowered, the words gone and an emote alone left on screen; off, no
layer and nothing kept; on, the room's new connection taken, live. Four
sockets opened in all (the room's, then one for each of the lowered,
raised and on levels) and three closed, one live at a time. No frame
around a change was longer than the frames before it (Chrome 16.8 ms
at most around and before; Firefox 12.7 around against 13.0 before).


## Round BE (2026-11-24): the emote sets read by `barbed-emotes`

The maintainer: "Let's finish adopting `barbed-emotes`, if the crate
itself needs more changes let me know and I could make another
`barbed-emotes` release with them." `ddi_chat::emotes` now reads the four
providers' sets with `barbed-emotes` 0.0.7 (the maintainer's own library,
MIT OR Apache-2.0); the game's seven hand-written parsers, its id and path
checks and its address building are gone. What stays the game's: its
`Emote`, `Image` and `List`, the order of sources (`EmoteSets`), the
images' cache and atlas, `anim`'s probing, the overlay, the items, the
EventSub reading and the live connection (`barbed` itself is not in the
web build).

- **One seam**, `emotes::from_barbed`: each emote of a set barbed-emotes
  read made ours. A modifier (BetterTTV's and FrankerFaceZ's effect words,
  7TV's zero-width overlays) and an emote FrankerFaceZ hides are left out,
  as before; the image is barbed-emotes' pick (`Emote::image`: 2×, else
  the nearest larger, else the largest; an animated emote's animation;
  Twitch's dark theme; 7TV's WebP files alone, the others' first listed:
  BetterTTV's file as uploaded, FrankerFaceZ's animated WebP before its
  PNG); the emote is animated where that image is.
- **The addresses** (`set_addresses`, `global_address`, Twitch's
  `twitch_image` and `twitch_animated_image`) are barbed-emotes' own
  (`seventv::user_url`, `bttv::channel_url`, `ffz::room_url`, the global
  URLs, `twitch::image_url`), and every proxy path is `emotes/` and
  barbed-emotes' `proxy::route` for the address.
- **The security checks** (review 2026-11-22, chat-touch 5): an id is
  plain (barbed-emotes' `is_plain_id`, in each parser, in
  `twitch::image_url` for a message's fragment's id, and in the set
  addresses for the channel id); an image's address is on its provider's
  image host with a path that cannot climb out (barbed-emotes'
  `safe::path_on` and each provider's `is_image_path`); and the game's own
  `Image::carried` takes an image only where `proxy::route` (the closed
  list of shapes the proxy carries) names it under the provider's own
  image route, never another provider's nor an `-api` route. The test
  `ids_and_paths_that_leave_the_images_make_none` is unchanged and passes.
- **On real data nothing changed**: the recorded global sets of
  2026-11-20 (`tests/emote_lists.rs`, local) read entry for entry the same
  before and after (7TV 42, BetterTTV 56, FrankerFaceZ 9: every word, id,
  animated flag, address and proxy path), and so do the demo's fixtures,
  the set addresses and Twitch's images.
- **On unusual answers barbed-emotes' reading is taken** where it is as
  good or better (`unusual_answers_are_read_as_barbed_emotes_reads_them`):
  a 7TV still emote's image is the file its answer lists by `name` (the
  game asked `static_name`: the same file on every real one; the one test
  whose expectation changed, `seven_tv_sets_are_read_with_their_animated_and_still_files`);
  no made-up 7TV file where `2x.webp` is not listed (the nearest listed, or
  no emote for AVIF alone); 7TV's emote-level zero-width flag honored;
  FrankerFaceZ's 4× where it has no 2×; addresses of another shape, empty
  words and fields of the wrong type left out; FrankerFaceZ's default sets
  written as strings read; a Twitch emote's own scale list honored.
- **Lost on 0.0.7, asked of the next release** (the list, with a patch,
  `target/scratch/mp/be/barbed-emotes-requests.md`, kept local): a
  BetterTTV GIF its answer calls `animated: false` reads as animated (the
  same `/2x` file is drawn and its frames counted when decoded: only the
  demo's choice of animated emotes and the debug counts see it), and a
  `null` in an optional field (BetterTTV's `animated`, FrankerFaceZ's
  `hidden`, 7TV's `flags`, …) leaves the whole emote out where the game
  read it as the default. Neither is in any recorded answer. With the
  patch applied (a local `[patch.crates-io]`, never committed) the game
  reads both as its own parsers did and every chat test passes.
- **Its cost**: the web app's wasm grows by 124,406 bytes (38,236
  gzipped; 15,391,248 to 15,515,654), the typed readers barbed-emotes
  derives for each provider's entries and serde_json's code for them; no
  feature of the crate would drop it (the game reads all four providers,
  and what it does not use is left out by the linker already). The page's
  first bytes are unchanged (`check-boot`: 12,424 of 12,552 gzipped). One
  crate more in the credits (`barbed-emotes`, MIT OR Apache-2.0).
- **Walked** in Chrome and Firefox (release build, every request outside
  localhost aborted): the smoke check, a solo play with the scripted `wall`
  and `animated` chats over code-made pictures, and the global-emotes demo
  over a local image base (all four sets read, BetterTTV's through the
  proxy as the fixtures arrange; the assortment as round TW5's: Twitch two
  animated, 7TV six, BetterTTV one, FrankerFaceZ none; no image failed).

## Round B8 (2026-11-24): `barbed-emotes` 0.0.8

The maintainer released `barbed-emotes` 0.0.8 with round BE's requests;
the game moves to it. What it changed that the game sees:

- **A provider's `null` in an optional field reads as the field left
  out** (BetterTTV's `imageType`, `animated`, `modifier`; 7TV's `flags`,
  `data.animated`, `host.files`; FrankerFaceZ's `hidden`, `modifier`,
  `modifier_flags`, `urls`; Twitch's `format`, `scale`, `theme_mode`; and
  the lists of a channel's or global answer): such an emote is kept, as
  the game's own parsers kept it, where 0.0.7 left it out.
- **BetterTTV's own `animated` is believed**: a GIF its answer calls
  `animated: false` is still (0.0.7 called it animated); a GIF whose
  answer does not say (or says `null`) still moves, barbed-emotes' rule,
  the one deliberate difference from the game's old parser.
- **`proxy::image_route`**: `Image::carried` asks it for the provider's
  own image route instead of matching the route's name. The same answer
  for every address (each provider has one image route on a host of its
  own, so the first route `proxy::route` finds for an address is the
  only one); `ids_and_paths_that_leave_the_images_make_none` passes
  unchanged.
- **The package's licence texts**: the credits now ship barbed-emotes' own
  MIT notice with its copyright line instead of the game's generic
  Apache-2.0 text (one notice more, 178).

`unusual_answers_are_read_as_barbed_emotes_reads_them` asserts both fixes
on round BE's minimal inputs. The recorded global sets read the same,
entry for entry (7TV 42 / 18 animated, BetterTTV 56 / 1, FrankerFaceZ 9 /
0), and on round BE's comparison against the game's old parsers every
answer agrees as it did under round BE's patch. The web app's wasm:
15,517,403 bytes (6,472,347 gzipped), 1,749 (2,615) more than with 0.0.7;
`check-boot` 12,417 of 12,552 gzipped.

## Round BM (2026-11-25): the maintainer's run against Twitch, and a room's ends

The maintainer ran the ban mirror against real Twitch for the first time,
with a second account, in ten steps. Worked: a join refused while banned;
an unban and the five-minute answer; banned inside, then kicked (kept out
at once); banned inside, then a pick (removed); blocks; the mirror turned
on with a banned person inside; a reconnect; access ended. Did not: **a
timeout, then a kick, let the person back in while the timeout lasted.**

**The timeout (the server, `../ddi-server`).** Read against Twitch's API
reference for Get Banned Users (fetched 2026-11-25): the endpoint lists
people "banned or put in a timeout", a `user_id` filter returns only those
banned or timed out and an empty `pagination` object, `expires_at` is an
RFC 3339 time for a timeout and an empty string for a ban, and the
example answer is `"expires_at": "2022-03-15T02:00:28Z"`. The server read
that shape right: the reader, the "until when" kept in memory, the kept
answer no longer than the timeout and the join's decision treat a timeout
as a ban until its end, and the kick forgets the answer kept, as for a
ban. So the cause was not found by reading, and nothing was guessed:

- One way the server let such a person in was found and closed: an
  `expires_at` it could not read was read by `Date.parse` as `NaN`, taken
  as 0, a timeout over in 1970. It is now read by the server's own RFC
  3339 reader (`twitch::rfc3339_ms`, the same natively and in the
  Worker), and a listed person whose end is unreadable is kept out (with
  no end of its own: asked again after the answer's five minutes). The
  documented shape reads the same as before.
- The fake Twitch now answers Get Banned Users as a body in the
  documented shape, read by the reader Twitch's answer goes through, and
  the documentation's example is a fixture
  (`worker/src/fixtures/twitch-get-banned-users.json`, written from the
  documentation, not recorded), so a fake shaped otherwise cannot hide a
  reading again. Tests through the room's own path: a timeout then a kick
  refused until its end and admitted after it; a timeout longer than the
  kept answer asked again and still refused; a ban unchanged; an
  `expires_at` of `null` still no ban (not read, let in, B8); the blocks
  only where the mirror reads them.
- What real answer could explain the run: Twitch not listing the timeout
  when asked (a lag after `/timeout`, or the person's join coming before
  it was listed); Twitch slow past the 2 s deadline or an answer not read
  (B8 lets in); a time format the old reader could not read (now kept
  out). The server now logs each check without names: the `TwitchLink`'s
  `twitch: check of N (blocks …): get banned users: http …, N rows;
  expires_at: … empty, … times read (ending in … s), … not read, …; time
  shapes: dddd-dd-ddTdd:dd:ddZ; fields: …; nulls: …; pagination: …; kept
  out: …; answered|unreachable or not read: let in`, and the room's
  `room: <id>: the mirror's join|ticket: kept out|let in on an answer
  kept|on the lists' answer (…)`. **To retest:** time the second account
  out (`/timeout <name> 120`), wait a few seconds, kick it, rejoin by the
  invite within the two minutes, and note the time; then read those two
  lines in the Worker's logs at that minute. Rows 0 means Twitch did not
  list the timeout; `not read` or a time shape other than the one above
  means its format; `unreachable` means slow or unread.

**A room's ends (the game).** A removed person's notice had "Multiplayer
home" and "Close", and Close left a room screen where nothing worked.
Every end of a room for its member now has one notice with one button,
OK, which leaves for the multiplayer home, as Escape, Back (the browser's
too) and a press beside it do (`screens::room::EndedDialog`,
`RoomClient::room_gone`):

| End | Before | After |
|---|---|---|
| Removed by a host (4403) | "The host removed you from the room." Multiplayer home, Close | the same words, OK |
| Kept out by the host's Twitch (4406) | "You can't join this room." Multiplayer home, Close | the same words, OK |
| The room closed (4410: its host left with Close, it ended) | the status line alone, the room's screen dead | "This room has closed." "You'll go back to the multiplayer home." OK |
| Closed by the server's operator | the status line alone | "The room was closed." the same, OK |
| No member any more (4000, not this device's own Leave) | the status line alone | "You left this room." the same, OK |
| A room's page, signed in, never a member | Multiplayer home, Close | the same words, OK |
| A room's page, not signed in | Sign in, Multiplayer home, Not now | Sign in, OK |
| Opened on another device (4429) | Use this device, Not now | unchanged: the room is not gone, More's Use this device |
| Signed out, a newer game, the server being updated | the status line, More's action | unchanged: mended from the room's screen |

No stage's prompt is a gone room's player's (`RoomClient::view`): a
prompt showing goes back to the room, saying why, and the room's song
select tells the end. A play still running is left as a quit is
(graded QUIT, judged to then: `LiveRoom::seat_end` now ends a member's
link where they were removed, kept out or the room closed, and the
session leaves the play), what is left to watch of the stage stops and
says why, then the results, then the notice. More's way out reads Leave
room in every case ("Multiplayer home" went from the notices and More,
and its message with it). The web leaves a gone room with the history
returning to the multiplayer home (`router::returning_to`): the browser's
Back does not reach the dead room again.

## Round RT (2026-11-25): a ban or a timeout on Twitch removes the person at once

The maintainer, after round BM: "we can't capture a block in real time,
which is fine, but we can catch a ban/timeout and I think we could do an
immediate kick in those cases", and to the way proposed (below): "Go
ahead with that." (A dedicated Twitch ban event, with its broader
`channel:moderate` permission and a webhook, was declined on 2026-11-24
and is not built.)

**What it uses.** When someone is banned or timed out in a channel,
Twitch's chat clears their messages and sends
`channel.chat.clear_user_messages` to every socket reading that chat.
Each device that shows the room's chat already received it (C11: the
overlay takes the person's items off). Now, in an online room whose
mirror is on, that event is also a hint for the room: the device asks
the server to check that one person again, and the server asks Twitch
itself, exactly as the mirror asks at a join. Nothing is taken on a
device's word: a person is removed only where Twitch lists them.

**On the device** (`ddi_session::chat`, `ddi_ui::state::twitch`,
`ddi_ui::state::room`):

- A user's clear heard on a room's own connection (the room's route, kept
  between the stages or lent to a stage, `LiveWant::rechecks`) is told to
  the room's client (`twitch::note_cleared`, at most 32 waiting). Never
  one heard on the development script's connection (a solo play, CPU
  practice) or on its `live:<login>` channel in a room's place; never in
  singleplayer or CPU practice (no online room, no request).
- The room's client, at its tick, asks `POST /rooms/<id>/recheck
  {user_id}` (`RoomClient::recheck_cleared`, `OnlineLink::recheck`) only
  where the room's mirror is on, the clear was heard in the channel of the
  one who set it, and the person is in the room (not left, not the one
  who set it); once a person in 15 s from this device
  (`link::RECHECK_EVERY_MS`). Its answer is not read: a removal comes as
  the room's own news.
- The room's route now asks for the user's clear with emotes alone too
  (`{clears: true}` in the subscribe body): before, only a room showing
  messages subscribed it. The event has no message text. On screen it
  takes that person's emotes off, as it took their messages off before.

**On the server** (`../ddi-server`, its README's "A ban or a timeout
heard in the room's chat"): the caller a present member (`403
not_a_member`; a bot `403 no_twitch`), the mirror on (`409 mirror_off`),
the person named present and checked by the mirror (else `not_present`,
nothing asked); the answer kept for them forgotten, the setter's lists
asked about them alone; listed, removed as a sweep removes (a leave to
the others, sockets 4406, "You can't join this room.", the single-OK
notice of round BM; a stage they play ended as a removal ends it, graded
QUIT; refused at the door on the answer kept, a timeout's no longer than
it lasts).

| Limit | Value | The most it allows |
|---|---|---|
| A member's asks | 20 a minute (`RECHECKS_A_MEMBER`) | 20 asks; most answered without Twitch |
| One person rechecked | once in 15 s, whoever asks (`RECHECK_SAME_MS`) | 4 rechecks a minute of one person |
| A room's rechecks that ask Twitch | 40 a minute (`RECHECKS_A_ROOM`) | 80 asks of the lists a minute: 80 Get Banned Users calls of the setter's access, 160 with the blocks read too, a tenth to a fifth of Twitch's 800 points a minute for one person's access |
| A device | once a person in 15 s (`RECHECK_EVERY_MS`) | |

Real moderation stays far below these: a moderator clearing twenty
people of one room in a minute. Several devices hear the same event: the
first recheck asks Twitch, the others are answered `already`.

**The retry.** Twitch may list a ban or a timeout a moment after its chat
said so. Where the first answer does not list the person, Twitch is
asked once more 3 s after it (`RECHECK_AGAIN_MS`), never a third time;
still not listed, nothing happens and no "not listed" answer is kept (so
a reconnect's ticket asks Twitch rather than trusting it), and the
mirror's usual checks (a ticket, the next pick) find a ban listed later.
Twitch slow or unreachable: nothing, as B8.

**When it works.** Only while at least one device in the room shows the
room's chat: the host's "Show my Twitch chat" on (Emotes is enough), and
some member's (the host's own included) "Twitch chat in rooms" on with
the chat's permission. With the chat off, or no device showing it,
nothing changes: the mirror's checks at a join, a reconnect and a pick,
and the host's Kick. Blocks have no event: unchanged. An unban or the end
of a timeout needs nothing (the invite lets them back; the answer kept
lapses as before).

**Why an HTTP route, not a socket message.** The removal is the
authority's already (`Command::Remove`); only the hint is new. A route
leaves the room's protocol untouched: no room model revision (24 stays),
a game that predates it never sends it, and a server that predates it
answers `404`, which the game ignores. A socket message would have
needed every server to ignore an unknown live message and every game to
send it only to a server that knows it. Deploy the server first, then
the game; either order is harmless (an old game with the new server:
nothing new happens, its subscriptions as before; a new game with the
old server: its rechecks answered 404, its `clears` field ignored).

**Privacy.** The note gains a moment in its list of when our server asks
Twitch about a person, and two paragraphs in "A host's Twitch chat on
your device" (both languages, the page and the Credits' fold, the date
November 25, 2026), for the maintainer's approval before it is
published.

**Tests.** The server's, through the room's object (`live/tests/mirror.rs`,
the fake Twitch answering in Twitch's documented shape):
`a_ban_then_a_recheck_removes_them_at_once`,
`a_timeout_then_a_recheck_removes_them_until_it_ends`,
`not_listed_is_asked_once_more_then_left`,
`a_recheck_of_someone_not_in_the_room_asks_nothing`,
`two_devices_rechecks_make_one_call`,
`the_rechecks_are_bounded_by_member_and_by_room`,
`a_recheck_is_refused_to_strangers_and_without_the_mirror`,
`a_recheck_unanswered_or_overtaken_removes_nobody`, and the route and the
subscription's types. The game's: `a_user_cleared_in_the_rooms_chat_is_told_to_the_room_alone`
(`ddi-session`, the fake clock and the fake shell's side: between the
stages and in a stage, emotes alone; a solo play's and the named
channel's never), `a_ban_heard_in_the_rooms_chat_asks_the_room_to_check_again`
(`ddi-ui`, over the fake server: the mirror off, another channel,
someone not there, the setter: nothing; one ask a person in 15 s; bob
barred, removed and told; practice nothing).

**Walked** in Chrome and Firefox (the release build) over a local server
(its fake Twitch) and a local fake EventSub socket with a trigger, every
request outside localhost aborted (`target/scratch/mp/rt/walks/rt.mjs`,
not committed): a host (bans and chat) and a guest in a room with the
mirror on and the chat at Emotes; the fake Twitch banned the guest and the
fake socket sent the user's clear to both devices. Both devices asked; the
first was answered `removed` 20 ms later, the second `not_present`; the
guest's notice ("You can't join this room.", OK) showed 47 to 70 ms after
the event, the host's players panel had the guest gone, and a join by the
invite was refused `403 not_admitted`. During a stage, a second guest
timed out (120 s) five seconds into the song: removed 23 to 29 ms after
the event, their play left at once ("Left the stage", what is left to
watch), the results graded QUIT, then the notice; refused at the door.
The singleplayer smoke check passed in both.

## Round RD (2026-11-25): a new room starts with the host's Twitch on, asked for where the room is made

The maintainer: "I'd like to make the default option for creating rooms
to be 'Twitch bans: Bans' and 'Show my Twitch chat: Emotes', but an
important aspect of this is that the first 'Sign-in with Twitch' should
request the extra permissions it needs only when it comes from creating
a private room, if it comes from joining a room it shouldn't request
those extra permissions." Then: "Yes, ask at creation. If the person
declines make the room with whatever was granted. Keep the host's last
choice for new rooms. The consequences are fine, update the privacy
note." This changes A1's table and B1's and C1's "Off by default".

**What the host's side of the chat needs: nothing.** Each device that
shows the chat opens its own socket and is subscribed with its own
person's `user:read:chat` (C2; Twitch's EventSub reference for
`channel.chat.message` over a WebSocket asks for that scope of the
reading user and nothing of the broadcaster, read in the design round;
the broadcaster's `channel:bot` is asked only of an app token, which we
do not use). Our server never used the host's `user:read:chat` for the
room's chat: every subscription names the reader's own link
(`TwitchLink`'s `/subscribe`), the channel only as its broadcaster id,
which the room takes from the host's identity. It was asked (A1's
"Host, Chat shown") only because the design assumed it. Not yet seen
against Twitch: a viewer reading another person's channel with their
own access alone (the real runs these notes record read the runner's
own channel);
the reference says it works, and the room's guests have always relied
on it. So "Show my Twitch chat" now asks nothing of the host: the
server's `POST /rooms/<id>/twitch` needs `moderation:read` for Bans
(and `user:read:blocked_users` with it for Bans and blocks) and nothing
for the chat, and hosting a room with the chat on no longer marks the
chat's permission used (`api::twitch_needs`, `mark_twitch_used`). Each
viewer still needs their own `user:read:chat` for their own device's
"Twitch chat in rooms", Off by default and unchanged.

**A new room's setting** (`crate::state::twitch::host_wanted`): the host's
last choice on this device (`Settings::room_twitch`: what they last left
Host options' two rows at in any room, Off included; written by the
rows' presses, `RoomClient::choose_twitch`, never by Disconnect or by
what the server made of a choice), and for a host who never chose
`HOST_DEFAULT`, Bans and Emotes. It reaches the room in the create
request itself: `POST /rooms {name, twitch: {mirror, chat}}`. The
server makes the room first, then gives it the setting as Host options
would (`RoomDO`'s `/twitch`, the authority's `Command::SetTwitch`), as
far as the maker's link grants it (`twitch::room_defaults`: Bans and
blocks where both are granted, Bans where only the bans are, else Off;
the chat as asked). The room has only its maker, so the mirror's sweep
asks Twitch about nobody. Whatever the link, Twitch or the room
answers, the room is made; the answer says what it got
(`{room_id, invite, twitch: {mirror, chat}}`). Why this way and not the
host's client asking after the room is made: one request, decided on
the server against the link as it is, no window where the room exists
without its setting, no `409` to handle, and nothing new in the room's
protocol (no room model revision: 24 stays). An old game sends no
`twitch` and gets a room with it off, as before; a new game against an
old server gets the same (the field ignored, the answer without
`twitch`).

**The sign-in that comes from making a room** says so: `purpose=host`
on the web's `/auth/twitch/login`, `"purpose": "host"` in desktop's
`POST /auth/attempts` (`host_blocks` for a host whose last choice is
Bans and blocks; no purpose where it is Off). The purpose is a hint: the
server maps it to one of its fixed sets (`twitch::purpose_features`:
`host` the bans' `moderation:read`, `host_blocks` that and
`user:read:blocked_users`, anything else the plain sign-in) and never
takes a scope from the client. It is added to round TB's `keep`. Every
other sign-in (an invite, Join by link, the account's Sign in, a room's
page) asks only the plain sign-in and `keep`, as before. The sign-in
dialog says what Twitch will ask (`room-sign-in-for-private-host`);
desktop's attempt page lists it as for any trip. Twitch's consent is
all or nothing, so a person who declines it at a sign-in is not signed
in and no room is made; the game then remembers for the session that
they said no (`twitch::decline_host`), and their next Private room asks
only the plain sign-in.

**Signed in without it** (`Online::private_room`): where the wanted
mirror needs a feature `/me` does not list, one trip through Twitch for
it first (`connect_twitch_for(Dest::Create, "bans")`, the host's trip,
`Online::host_trip`), then the room. Desktop: the multiplayer home
shows the trip as Host options shows its trips, the code on its plate,
with why (`room-host-trip-why`). The web: the page leaves for Twitch and
comes back to the multiplayer home, which makes the room
(`HOST_TRIP` in `sessionStorage` tells a canceled trip, `?signin=canceled`,
from a canceled sign-in). Granted, declined, canceled in the game's
dialog or failed: the room is made, with what was granted, and nothing
more is asked when making a room this session (`decline_host`); Host
options still asks when a row is turned on. Never sent to Twitch: a
host whose last choice for the bans is Off, a bot, one who said no this
session.

**Told**: a room made asking for the bans that got none says once on
its status line "Twitch bans are off: Twitch did not allow them."
(`LinkNotice::TwitchBansOff`, `Online::take_made`). Host options has a
line under its rows: "The next room you make starts with these two as
you leave them."

**The 30 days** (round TB) are unchanged and for every sign-in: the bans
granted this way count as used when the host hosts a room with them on;
unused for 30 days they are not in the next sign-in's `keep`. A host
whose last choice is Bans but whose permission lapsed is asked again
when they make a room (in the sign-in's purpose, or the trip at
creation); the last choice and the permissions used are kept apart, so
a lapse never turns the choice Off.

**Also changed**: a trip that ended canceled or failed never sets the
room's setting from an older `/me` (`take_twitch_wanted`; before, a
stale `/me` could send the person through Twitch again and again), and
it compares what was granted with what the server asked for (the
`need` of its `409`, `TwitchWanted::need`), so a new game against a
server that still asks the host for the chat is not looped either.

**Deploy**: the server first, then the game; either alone is harmless
(above). The privacy note's section "Twitch access for a room's
features" says it, in both languages, the page and the Credits' fold.

**Tests.** Server: `a_purpose_chooses_among_fixed_sets_and_anything_else_is_plain`,
`a_new_rooms_setting_is_what_the_link_grants` (`twitch.rs`),
`a_new_room_takes_its_makers_setting_and_asks_about_nobody` (`room.rs`).
Game (`ddi-ui`): `making_a_room_signs_in_for_the_hosts_bans_and_joining_asks_nothing_new`,
`signed_in_without_the_bans_a_host_is_asked_at_creation_and_the_room_is_made_regardless`,
`the_webs_host_trip_comes_back_to_a_room_made_either_way`
(`state/online/tests.rs`),
`desktops_private_room_asks_for_the_bans_at_creation_and_the_room_is_made_either_way`
(`screens/room_tests.rs`: the home's dialog with its code and why,
granted and canceled, the rows remembered, Off included, the notice),
and the `qps-ploc` walk over the host's trip and the notice.
