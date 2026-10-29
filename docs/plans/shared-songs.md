# Shared songs by link (2026-10-29)

The maintainer's request (2026-10-29), a proof of concept: a link that
makes the app load a song or a pack to play, without downloading it into
the local library or mixing it with it; the link may expire; once the
shared list is left it can only be reached again from the link. And a way
to upload a song or a pack, authenticated and hidden: a special route or
parameter turns the option on, and from then on (whatever the address the
app was opened at) the upload UI shows. The backend is closed source, a
Cloudflare Worker in a private repository, built like `../heddobureika`'s
and `../goonli`'s (a Rust Worker, `worker-build`, wrangler, routes under
`sugoijan.dev/<name>/…`, Durable Objects). Decisions taken with the
maintainer:

- Auth for uploads: long bearer tokens, a list kept by hand in the
  Worker's secrets; no Cloudflare Access. Later it may become an OAuth
  identity (Twitch and others), as `goonli`'s `barbed` does.
- Plays on shared songs are kept in the history and count for the bests
  (they are keyed by chart digest, so nothing new is needed for that).
- Movies are included. The share's list must load fast: a song's music,
  movies and pictures are fetched when the song is chosen or played, not
  with the list; movies are served with range requests so a decoder can
  read as it goes (a later step on the app's side, below).
- Expiry: one of 1 hour, 1 day, 1 week, 1 month or forever, chosen at
  the upload.
- The link is short, as a link shortener's: `#/s/<id>` with an id of 8
  characters from `[0-9A-Za-z]` (about 48 bits of randomness; the Worker
  never lists shares, so an id is found only by having it).
- An unknown or expired id gets a friendly page: "This link doesn't exist
  or has expired.", with the way to the start menu.

## The contract (the only thing both repositories know)

A share is a pack (one song or many) uploaded by an authenticated client
and read by anyone with its id. The Worker is the game's server
(`ddi-server`, routed at `sugoijan.dev/ditoditoinfinito/api/*`: the
app's own origin and path, so nothing crosses origins) and the app calls
it as `api` beside its page (`just dev` proxies that to a local
`wrangler dev`; `DDI_API_URL` at build or `window.__DDI_API_URL` point
elsewhere for tests); the base is never carried in the link, so no other
origin can feed the app content.

Reading:

- `GET <base>/s/<id>` → `200` with the share:
  `{ "pack": "<name>", "expires_at": <unix seconds or null>,
     "created_at": <unix seconds>,
     "songs": [ { "record": <ImportedSong as the web stores it, with the
     entry's charts and their digests>, "files": [ { "path": "<path in
     the song's folder>", "bytes": <size>, "url": "<relative to the API's
     base, `s/<id>/f/<n>/<path>`, or absolute>" } ] } ],
     "pack_art": <url the same way, or null> }`.
  `404` for an unknown id, `410` once it expired (the body says which;
  the app shows the same page for both). Cached for a minute at most.
- `GET <base>/s/<id>/f/<song index>/<path>` → the file from R2, with
  `Accept-Ranges: bytes` and range requests honored, `Content-Type` from
  the path, `Cache-Control: private, max-age=86400` (the id is the secret),
  `410` once the share expired.

Writing (`Authorization: Bearer <token>`, one of the secrets' list; CORS
for the app's origins only: the Pages site and `localhost`):

- `POST <base>/s` with `{ "pack": "<name>", "expires": "1h" | "1d" | "1w"
  | "1m" | "never", "songs": [ { "record": …, "files": [ { "path", "bytes"
  } ] } ], "pack_art": <bool> }` → `201 { "id": "<id>" }`. Limits refused
  with `413`: 2 GB a share, 512 MB a file, 200 songs.
- `PUT <base>/s/<id>/f/<song index>/<path>` with the file's bytes (the
  same token): streamed into R2 under `<id>/<song index>/<path>`;
  `PUT <base>/s/<id>/art` the pack's art.
- `POST <base>/s/<id>/finish` → `200`: the share is readable from now (a
  share never finished is deleted after a day).
- `DELETE <base>/s/<id>` (the same token) removes it.

The Worker keeps each share's metadata in a SQLite Durable Object
(`ShareIndex`: id, created, expires, pack, the songs' records and files
with sizes, bytes in all, finished) and the files in an R2 bucket; a cron
trigger deletes expired and unfinished shares and their files. The upload
token in a request is compared in constant time against the list in the
`SHARE_TOKENS` secret (one a line); no accounts, no rate limiting beyond
Cloudflare's, for the proof of concept.

## The app

- **A shared origin.** `songs::Origin::Shared` beside `Bundled` and
  `Imported`: the current share, held in memory for the session
  (`thread_local!`, one at a time, never in IndexedDB), its songs' ids
  `shared:<id>:<index>` (so they never clash with the library's and so a
  play's address, `#/play?song=shared:…`, says where the song is from).
  `find_song`, `parse_found`, `song_file_bytes` and `load_song` fetch the
  chart text, the music and the pictures from the share's URLs (the
  texture cache already loads URLs, as bundled songs' banners are);
  `shared_found` is empty for a share (no shared folders). A movie is
  fetched whole as a `Blob` for the decoder's worker at first (what it
  reads today, through `FileReaderSync`); a reader on range requests in
  the worker (synchronous XHR with `Range`) is a step of its own, so a
  long movie starts before it is all in.
- **The route.** `#/s/<id>` (`Route::Shared`): the app fetches the share
  (`__DDI_DEBUG.share` says `loading`, `ready`, `missing` or `expired`),
  then shows song select in shared mode on it: the header's name reads
  the pack's ("Shared · <pack>", with "expires in 3 days" muted where it
  does), the list holds the share's songs alone, no import panel, no
  pack art editing, plays and their results as usual (a play reloaded at
  its own address fetches the share again by the id in the song's id).
  Back goes to the start menu and drops the share; Singleplayer from
  there is the player's own library, as ever. An unknown or expired id
  shows the page above instead.
- **The uploader, hidden.** `#/?share=1` once sets `Settings::sharing`
  (and `#/?share=0` clears it; a developer-ish switch, never in Options'
  rows). With it set, Options gets a "Sharing" section: the token (a
  masked text field; kept in the settings, so it is only as private as
  the browser's profile, which is the proof of concept's bargain), the
  server it uploads to (read-only, the API's base), and a note
  on what sharing publishes. In song select, an imported pack's More
  actions (and a song's) get "Share…": a dialog with the expiry choice
  (1 hour, 1 day, 1 week, 1 month, forever), then the upload with a
  progress line (songs and bytes), then the link to copy
  (`<site>/#/s/<id>`, the clipboard where allowed, selectable text
  always) and "Open" to see it as a player would. Bundled songs cannot
  be shared (they are on the site already) nor a share's own songs.
- **Desktop.** The same origin and route through `--shared <link or
  id>` and the uploader on the library's folders, in a later step: the
  session and the UI core are shared, only the fetches differ.
- **History.** A play's record keeps `shared:<id>:<n>` as its song;
  the plays list finds the chart by digest, so a share's play shows its
  best beside later plays of the same chart from the library, and its
  replay plays only while the share is reachable (a refusal says so,
  "the song of this play is no longer there", as for a removed song).

## Tests and debug

- The contract's types in `crates/web/src/share.rs` with serde tests on
  the JSON above; `Route::Shared` parsing and `to_hash`; the shared
  origin's loaders on a fake server: the verify skill's `serve.mjs`
  gains a `--share <dir>` that serves `<dir>/s/<id>.json` and the files
  with ranges, enough for headless checks of the route, the list, a play
  and the error page (the uploader against it too: it writes the dir).
- `__DDI_DEBUG.share`: `{id, state, pack, songs, expires_at, error}`;
  the uploader's `{state, done, total, link}`.
- The Worker's own tests live with it (the private repository), against
  the contract's JSON kept in this plan.

## Steps

1. The contract types, `Route::Shared`, the shared origin and the error
   page, with the fake server; a share hand-written in a scratch dir.
2. Song select's shared mode (the header, no import, Back drops it).
3. The uploader: the setting and its switch, Options' section, the
   dialog, the upload and the link.
4. The Worker in the private repository, from the contract; its route
   on the app's origin; a real link checked in Firefox and Safari.
5. Range-read movies in the decoder's worker; desktop's flag and uploader.

Open: whether a share should carry a title of its own beyond the pack's
name; link previews (Open Graph from the Worker) for links pasted in
chats; what a token's owner sees of their shares (nothing, for now).

## Status (2026-10-29)

Step 1 done: the contract's types (`crates/web/src/share.rs`; the ids
and the time left in `ddi_library::share`, tested natively), the Worker's
base URL as a build constant with a page override for tests, the shared
origin through every song reader (`songs::Origin::Shared`: a song's
files by URL, the chart, the music, the pictures and the movies through
the same readers as an imported song's), `Route::Shared` (`#/s/<id>`),
the loader page (`components/shared_page.rs`) mounting song select in
its shared mode (`Props::shared`: the share's songs alone) or the page
for a gone link, the song list's pictures by URL, the verify skill's
server standing in for the Worker and a hand-made share. Checked in
Chrome: the list, a play from it to a quit kept under `shared:<id>:<n>`,
the gone pages, the player's own library after. Step 2 done: song
select's shared mode (`SelectData::shared`, a `SharedView` of the
pack's name and its time left) heads the screen "SHARED SONGS", says
"Shared: <pack> · expires in …" in the footer's note, offers no adding,
managing or editing (no `Link.Add`, `AddIcon`, `Link.Manage`, no E or
"Edit chart", no "Add songs" in More) and no storage or library
notices; Back is the start menu, the share held for the session
(`ThemeState::shared_pack`, `__DDI_DEBUG.ui.theme.sharedPack`). Step 3
done: the hidden switch (`#/?share=1` and `share=0` on the start route,
`router::share_switch`, `Settings::sharing`), Options' Sharing section
while it is on (`OptionsModel::sharing`: the upload token set, changed
or cleared through a dialog with one field, `TokenDialog`, kept as
`Settings::share_token` and never shown again; the server; what a share
is), Share… in the More actions of an imported song or pack
(`SelectScreen::with_sharing`, `ShareDialog`: the expiry, then the
upload as the shell reports it through `Feed::share`, then the link to
copy or open), the web's upload (`share::upload`: the records and every
stored or linked file of the songs, the pack's art, `POST`, the `PUT`s,
`finish`; `window.__DDI_SHARE_UPLOAD` for tests) and the test server's
side of it (`serve.mjs --share`, token `test-token`). Checked in
Chrome: a pack imported, shared for a week, its link opened and played.
Not shared: a song's shared background folders (`RandomMovies`), the
library's; a share plays without them. Step 4 done: the Worker, in the
private repository `ddi-server` beside this one (the game's server, shares its first feature) (`worker` 0.8 on
workers-rs; one SQLite Durable Object `ShareIndex` for the records, the
files in the R2 bucket `ddi-server`, a cron purge of expired and
unfinished shares, CORS for the app's origins, the tokens in the secret
`SHARE_TOKENS`; its contract's logic tested natively, the routes and the
app's whole flow checked against `wrangler dev`; Cloudflare's request
body limit caps each `PUT` at 100 MB on most plans, which multipart
uploads would lift). Step 5, the movies: a shared song's movie is read by
ranges (`web::movie_source::MovieSource::Remote`: the decoder worker
reads it with synchronous ranged requests, the browser's decoder and the
probes through `fetch` with `Range`), never fetched whole, so a play
starts at once; the rest of a shared song's files come as `Blob`s the
browser keeps (`songs::fetch_blob`). Afterwards: a shared song's
results (Song list, Back) return to its share, not the library
(`game_canvas::back_route`, the router's fallback for a direct play);
and a share of chosen songs: "Add to share" in an imported song's More
actions builds a list for the session (`SelectScreen::to_share`, the
footer's "N songs to share", `ThemeState::to_share`), More then offers
"Share N songs…" (their pack's name when they share one, else "Shared
songs") and "Clear the share list", and the list clears once the share
is up; "Share this song…" and "Share the pack…" stay for one song or a
whole pack with no list. Desktop (`crates/desktop/src/share.rs`, ureq
on the calling thread with rustls on the system's trust store): a share
read into the cache folder (`shares/<id>/<n>/`, each file at the path
its entry names it by: the web's `bg/`, `bga/` and `layer/` prefixes
map to and from a song folder's paths, `ddi_library::share::stored_files`,
so a share made on either shell plays on the other), its charts, music
and pictures with the list (`--shared <link or id>`, song select's
shared mode, the share's songs beside the library's in its registry)
and what the background changes play when the play loads
(`share::ensure_all`); uploads from song select's Share… with the
switch `--sharing on`, or from the command line with `--share
IDS|pack:NAME [--expires W]`, streaming each file at its length.
Checked against `wrangler dev`: a pack shared from the command line,
listed and autoplayed on desktop, and opened in the web app. Desktop
plays a share's movies from the cache, downloaded whole before the
play, not by ranges.
