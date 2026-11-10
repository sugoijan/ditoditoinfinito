# Multiplayer prototype progress

What the prototype implementation has built and tested, as opposed to what
the design studies propose. The design remains in
[the battle design](multiplayer.md), [the screen plan](multiplayer-ui.md) and
[the prototype handoff](multiplayer-prototype-handoff.md); nothing here makes
their proposals decided. Each provisional choice below names its effect and
the decision it waits for.

## M1: the shared room, match and stage model (implemented, 2026-11-10)

`crates/room` (`ddi-room`): pure, serde only, no chart, engine, library or
I/O, compiling natively and for wasm. Content is an opaque song key and
chart digest, so the private server can run the same model later. No UI,
no play, no network in this slice.

Implemented and tested:

- **Stable ids** (`ParticipantId`, `MatchId`, `StageId`, `Revision`), never
  names or list positions. A stage keeps its id from the lobby's playlist
  into its match and the room's record; CPU seats get fresh ids each match.
- **Membership and room control**: owner, named co-hosts, the backend's
  admin flag and ordinary members. `can` decides capabilities from role,
  admin flag and phase, never from participation: a watching co-host or
  admin operates, a playing member does not. Owner-only: co-hosts and
  access. Joins, connections, content status and stage outcomes are the
  authority's alone; a client sending them is refused (`AuthorityOnly`).
- **Room access** (admission open/closed, invite generation) changes future
  admission only: members, rules and readiness are untouched; returning
  members and admins still enter a closed room.
- **Match rules** (stages, seats, CPU mode and profile, return each stage,
  mid-match entry, battle preset id, award scheme), locked when the match
  starts. The next match's rules are an explicit draft: saved during a
  match without touching the running one; revalidated at the boundary
  against who wants to play then (a draft outgrown by new reservations
  goes under review and blocks the next start until an operator saves
  rules that pass); stage-count changes keep the first playlist slots.
- **Revisions**: every accepted change is one room revision. Conflicting
  edits (song choice, rules, match start) carry the revision they were
  made against and are refused as stale if their target changed since; an
  edit/start race therefore never applies an unseen edit to a locked
  match, whichever lands first.
- **Phases** as the State model draws them: Lobby, Preparing (with an
  optional readiness window), Countdown, Stage, SongCompletion,
  Intermission, MatchResults, Interrupted.
- **Loading and readiness** per person per stage: Ready is intent and can
  be set while loading; eligibility is intent plus loaded plus connected;
  never automatic, not even alone with CPUs; nothing about viewing results.
  Each stage has a readiness revision: replacing its content clears its
  readiness and bumps it, so a Ready for the old song is refused. CPUs need
  no readiness.
- **Starts**: Start commits at once when everyone who could still become
  eligible is; otherwise it opens the readiness window (never shortened,
  never stacked), at whose deadline the ready and loaded start and the
  rest sit out with zero and keep earlier points. Cancel restores
  preparation with readiness kept. A committed starter leaving or
  disconnecting cancels the countdown (`RosterChanged`); content becoming
  unavailable cancels it (`ContentUnavailable`); too few at the deadline
  cancels (`BelowMinimum`); no ready person refuses the start
  (`NotEnoughReady`). Each is a distinct outcome.
- **Participation** derived per member (the State model's table): waiting
  for the next match, not ready, ready and loading, ready and loaded, load
  failed, committed, competing, winner finishing, placed, sitting out, out
  for the match, watching, left.
- **Results**: a stage's placements are recorded only from a confirmed
  ranking of its lineup, awarded by the rule recorded with them. A result
  confirmed early moves to SongCompletion; the rest of the song cannot
  change it or add points, and nobody advances before the song's end.
  Match standings sum stage awards (missed or unconfirmed stages count
  zero); equal totals share a rank. An interrupted match keeps confirmed
  stages as partial results, awards nothing for the unconfirmed stage and
  has no overall winner.
- **Reconnects** restore the same row: same id, seat, lineup place and
  points; no new scoring row, no CPU takeover, no respawn. A disconnect is
  not a forfeit by itself; Disconnected clears an unstarted stage's Ready.
- **Late entry** (rules option, off by default) admits waiting people into
  vacant seats, never a CPU's, at a stage boundary, with zero for missed
  stages. **Persistent elimination** (option) takes the eliminated, the
  forfeiting and those who miss a stage out for the match; it cannot be
  combined with late entry.
- **The competitor cap** as one policy check
  (`RoomPolicy::check_competitors`), eight initially, applied to seat
  reservations, rules and CPU seats; a larger policy needs no model change.
- Tests: the handoff's walkthroughs in `crates/room/tests/walkthroughs.rs`,
  each accepted revision's events also applied to a mirror that must stay
  equal, each refusal checked to change nothing.

- **The wire contract** (`protocol.rs`): a versioned JSON envelope. A
  client sends `{v, client, seq, base, cmd}`; the authority answers with
  `{v, rev, msg}` where `msg` is an ack, a typed reject, one revision's
  events or a snapshot. Retries of the same `seq` get the same answer
  without applying twice; a lower `seq` is refused; a stale edit is refused
  and followed by a snapshot. `Mirror` applies events only at the next
  revision, ignores old ones and reports a gap for a snapshot to repair.
- **`RoomAuthority`**, the trait shells will hold, and **`LocalAuthority`**,
  the practice authority: the reducer on an injected clock, no network, no
  sign-in. Deadlines and countdowns happen when it is polled. Its
  `authority` method is how the stage runner (M3) will report confirmed
  outcomes; nothing in it scores or plays.
- Tests: `crates/room/tests/authority.rs` runs a practice match through
  envelopes with a mirror (every message also round-tripped through JSON),
  retries, unknown clients and versions, a stale co-host edit, a missed
  revision and its repair, and a reconnect through a new client id.

Not here: any screen, any play, any network transport. The online
authority, sessions, invites and upload grants belong to the private
server (M5); it is meant to run this reducer, which no code does yet.

## M2: the gated CPU-practice entry and the unified room (implemented, 2026-11-10)

The first screens, on both shells, bound to M1's model. No stage is
played yet: the room goes as far as the committed countdown and the
stage's phase, and says that playing it comes in the next slice (M3).

Implemented and tested:

- **The hidden switch** `Settings::multiplayer`: the web's `#/?mp=1`
  (`mp=0` off) on the start route, desktop's `--multiplayer on|off`; never
  an Options row. Off (the default, and the public site), the start menu is
  exactly as before ("Multiplayer · Coming soon", focusable and inert) and
  the web's prototype routes show the start menu; desktop refuses
  `--screen multiplayer|room`. On, Multiplayer opens a minimal
  **multiplayer home** (a page): CPU practice, and Private room shown
  grayed out (M5).
- **The unified room** (`ddi_ui::screens::room`), one screen for both
  shells: the players at the left (stable order by participant id; before
  a match the CPUs the rules will seat, marked "At start"; during it the
  match's CPUs and everyone's points), the selected stage's song card
  (song select's `SongInfo` and its chart's ladder row) with Choose or
  Change song, the match's stages as rows at the right (focus selects),
  a status line, and Ready and Start as separate buttons. Rules and More
  are pop-ups. Keyboard, controller (More on West, Back) and pointer;
  reduced motion through the shared transitions; a compact layout for
  phones (players in one line, Choose song in the action row).
- **Bound to the authority, not copied**: `ddi_ui::state::room::PracticeRoom`
  is a client of `LocalAuthority`: every change is a command in the
  versioned envelope, the screen reads a `Mirror` kept from the replies,
  and refusals are the room's own (`Rejection`), worded on the status line.
  The only client-side facts are what the device knows: which library
  song and chart a stage's opaque content names, whether the CPUs can play
  a chart, and the clock the authority's countdown runs on.
- **Songs from the real local library** through song select's **pick
  mode** (`SelectData::pick`: the whole library, nothing added, edited,
  managed or shared, Enter "Use for stage N", the singleplayer place left
  alone). A chart the CPUs cannot play (`ddi_engine::cpu` takes one-player
  charts with four lanes) is refused with its reason in a pop-up; songs
  stay visible.
- **CPUs and rules**: the Rules pop-up edits the next match's stages
  (1 to the policy's 16), CPU opponents (from the policy's minimum field
  less the players, 1, to the cap less the players, 7) and CPU level
  (the engine's five strengths as "Level 1 of 5" to "5 of 5", never
  skill names); battle and points rows say provisional and experimental.
  During a match it shows the running match's rules, locked, beside the
  next match's draft, which saving leaves the match and its readiness
  alone. Applying in the lobby clears readiness (the model's rule).
- **Readiness and Start**: Ready is the player's own press, never set by
  choosing a song, a rules change or a new match. Start in the lobby
  locks the match and starts stage 1 (two commands, checked first so a
  start the stage would refuse never locks a match); between stages it
  starts the next. The countdown can be canceled (readiness kept); then
  the stage's phase, where Start becomes Stop match, and Next match
  returns to the lobby with fresh readiness and the match recorded.
- **State across navigation**: desktop keeps the room for the run and
  stacks home, room and song select; the web keeps it in a thread-local
  store for the session (`crate::room`), with routes `#/mp`, `#/room` and
  `#/room/pick?stage=<id>&n=<number>`; Back goes room → home → start menu.
  More → Leave room forgets the practice.
- Tests: `crates/ui/src/state/room.rs` (choosing, refusing a double chart,
  readiness, start into countdown and stage, rules and the cap);
  `crates/ui/src/screens/room_tests.rs` (the gate on and off, the home,
  picking through song select and a refused chart, Ready/Start/Cancel/
  Stop/Next match, the Rules pop-up's limits and locked-versus-draft, a
  phone's layout and More from a controller); the `qps-ploc` walk covers
  the gated start menu, the home, the room in each phase, Rules, More, the
  pick and the refusal; `crates/desktop/tests/room_offscreen.rs` draws the
  room offscreen. At runtime: headless Chrome (wide and phone) and
  Firefox walked the switch, the home, a pick, Ready, Start, the
  countdown, Rules and Back (`target/scratch/mp/inspect.mjs`).

Development-only or mocked in M2 (the first two replaced in M3, below):

- No stage is played: the stage's phase shows a line saying so and the
  match can only be stopped. Nothing is scored; points stay zero.
- Loading is not real: a chosen local-library song is reported loaded at
  once (`PracticeRoom::sync_loading`); the chart's load and the CPU plan's
  check (`Plan::automatic`) come with M3, which will report a failure as
  `Loading::Failed`.
- The player's name in the room's data is "Player"; the screen shows the
  local player as "You".

## M3: stages played against CPU opponents (implemented, 2026-11-10)

CPU practice plays real stages end to end on both shells: the room
prepares each stage's song, its countdown hands the stage to the play,
the CPUs are judged by the engine beside it, its outcome is reported to
the room as the authority, the room shows the intermission, then the
next stage, the match's results, and the lobby with fresh readiness for
another match.

Implemented and tested:

- **Real preparation** replaces M2's faked `Loaded`: a chosen stage is
  reported loading and handed to the shell as a job
  (`state::room::LoadJob`); the shell reads the chart from the library
  (desktop `play::load::read_chart`, the web `songs::load_chart`) and
  checks it (`state::room::check_chart`: the chosen chart's digest, and
  the engine's automatic CPU plan built, checked and scripted), then
  reports it loaded or failed with a typed reason (`LoadProblem`:
  unreadable, changed since chosen, the CPUs cannot plan it), worded on
  the status line. Start waits for this device's own preparation (a
  window would wait its whole 15 s for it). A song replaced meanwhile
  asks again; the old answer is dropped.
- **The CPU opponent** (`ddi_engine::cpu::Opponent`): its own `Player`
  on the chart's tap-only projection, fed the analytical plan's seeded
  presses, judged by the normal judge, advanced to the play's judge time
  each frame as the ghost is. Deterministic per chart, level and seed at
  any frame rate (tests), and playable to its end at once.
- **The stage in the session** (`ddi_session::battle`;
  `SessionConfig::opponents`, separate from the solo CPU demonstration's
  `cpu`, which is unchanged): the CPUs are built from the song as played,
  advanced after the player's update, exposed read-only
  (`PlaySession::opponents`: name, level, life, out, score, combo). In a
  stage: no turns, note removals or rate; the gauge's fail is immediate
  (out of the stage); no quick restart; no ghost, no "prior", nothing
  kept in the history or bests (both shells), no song-offset editor. A
  quit, an interruption or Back on the prompt is a forfeit.
- **The outcome** (`StageOutcome`): when at most one competitor is still
  in before the song's end, the stage is decided and its ranking frozen;
  a local winner plays on to the song's end with its fail turned off
  (`Player::set_fail`), and nothing after changes the result; otherwise
  the ranking is taken at the end, the CPUs played to their end (nothing
  they do depends on the player). The shells report it to the room as
  its authority (`PracticeRoom::report_stage`: `ConfirmStage` then
  `EndSong` when decided early, else `EndSong` with the ranking), before
  the results show.
- **The play's opponents column** (`ddi_shell::opponents`, drawn by
  `PlayView`'s 2D pass on both shells): a quiet panel beside the field on
  its roomier side, one stable slot per CPU in the order the room seated
  them (name, level, life bar, Out), headed "CPUs · taps-only copy", with
  "Stage won" once the player has won early; numbered bars only where the
  side is narrow, nothing where there is no side (never over the field),
  moved below the HUD it would cover. Nothing animates but the bars.
- **The prompt** (`PromptInfo::set_stage`): "Stage 1 of 2 · against 3
  CPUs on a taps-only copy · out when your life runs out" before the
  mods; no singleplayer countdown, no play options row, no timing line.
- **Results** (`ResultsKind::Stage`): "Back to room" in Song list's place,
  no Retry (R and a pad's Select do nothing), not REPLAY-marked, no offset
  editor, a note with the player's stage result ("Stage 1: place 1 of 4 ·
  +4 pts · points experimental").
- **The room between stages**: the players' column with places and points
  ("#1 · 4 pts"), the next stage selected and its song shown, the
  player's last result under it, "Press Ready when you are, then Start
  stage 2." (Ready is asked again for each stage; never automatic). At
  the match's end the song's place shows the match's results: title (or
  "Match stopped · finished stages only"), the winner or winners, each
  player's place, stages' points and total; Start reads Next match, which
  returns to the lobby with fresh readiness and the match in the room's
  history.
- **Shells**: desktop plays a stage over the room through the play host
  (`play::Request::stage`), reporting before its results, which pop back
  to the room; `room::room_screen_with` takes the play call (tests). The
  web navigates to `#/play?song=…&chart=…&stage=<id>` (the stage kept in
  `crate::room` for the route; an address the room did not hand over shows
  the room), the game loop reports the outcome as the play ends, and the
  results go back to `#/room`; `stage=` never touches the session's kept
  testing parameters. `__DDI_DEBUG.opponents`, `opponents_column`,
  `stage_won` on the play route; `ui.theme.room.mark` and `standings` in
  the room.
- Tests: `crates/engine/src/cpu/opponent.rs` (determinism per seed and
  frame rate, played-to-end equals frame by frame, refused charts);
  `crates/room/src/battle.rs` (ranking, ties, decided);
  `crates/ui/src/state/room.rs` (preparation jobs, a failed preparation,
  a reported match to its results and the next lobby);
  `crates/session/src/play/tests/stage.rs` (a whole two-stage match of
  autoplayed plays on the fake clock, each CPU's judged life and score
  equal to it played alone, reported into a practice room through the
  intermission, the match's results and the lobby; a quit is a forfeit
  and R twice does not restart; out of life with one CPU left decides
  early for the CPU; a won stage turns the fail off; a stage refuses the
  CPU demo, a replay or a test, and plays without turns or rate);
  `crates/ui/src/screens/room_tests.rs` (the room through intermission,
  match results and the lobby; a stage's results); the `qps-ploc` walk
  (the intermission, the match's results and a stopped match's, a
  stage's results, the prompt's note, the column's words, every kind of
  stage result); `crates/shell/src/opponents.rs` (beside the field, a
  phone's bars, below the HUD); `crates/desktop/tests/room_stage_flow.rs`
  (offscreen on the GPU: a two-stage match through the real play host,
  prompts, plays, results, back to the room, the match's results, the
  next match, and a stage left from its prompt as a forfeit; nothing
  kept). At runtime: headless Chrome and Firefox walked a whole match
  (`target/scratch/mp/inspect.mjs`), a phone-sized and a Japanese walk in
  Chrome.

What is real and what is not in M3:

- The CPUs' plays are real judged plays of the chart's tap-only copy;
  their life is their own ruleset gauge as the engine judges it. They are
  presented as "CPU n" at "Level n of 5" (strength relative to the chart),
  never as skill-tuned opponents, and the column and the prompt say they
  play a taps-only copy. They respond to nothing (no mines, no attacks).
- The battle is the provisional `gauge-survival-v0` below, not the
  design's standard battle: no attack power, no mines added, no
  weakening, no targeting, no settlement of outstanding attacks (there
  are none).
- Watching after elimination is not built: when the player is out, their
  play ends as a failed single play does (its fade and its results) and
  the remaining CPUs are played to their end at once to settle the
  stage. A spectator view, Watching [name] and miniature fields are later
  work.
- The full evaluation can be seen only as the play's results; the room
  does not keep it to reopen later (the design's "My result" detail).
- The practice room still lives for the session (web) or run (desktop).
- On the web, leaving a stage's play by the browser's own Back (not the
  play's quit) reports nothing: the room stays in the stage, which More →
  Stop match ends.

## M4: the battle's announcer lines and their visual fallbacks (implemented, 2026-11-10)

The battle's moments as event semantics: a new category of announcer
lines that packs, our TTS tooling and the announcer studio can grow
into, spoken in a practice stage where the chosen pack has them, and
shown quietly beside the field where it does not (or the announcer is
off). No line was recorded and no pack changed.

Implemented and tested:

- **The category** (`ddi_library::announcer::Battle`, `Line::Battle`,
  `Spoken::Battle`, ours): folders `battle match begin`, `battle stage
  begin`, `battle final stage`, `battle life critical`, `battle survivors
  4`, `battle survivors 2`, `battle eliminated`, `battle stage won`,
  `battle stage not won`, `battle stage drawn`, `battle song finished`,
  `battle match won`, `battle match not won`, `battle match drawn`. Only
  the design's events ("Announcer events implied by the mechanism") that
  the prototype's stages really produce. Fallbacks where the moment is a
  play's own: an opening falls back on `battle stage begin`, then
  `gameplay ready` (whose role it takes); life critical on `gameplay
  comment danger`; out on `gameplay failed`; the winner's finished song
  on `gameplay cleared`; outcomes and milestones on nothing (a chart's
  clear is not a stage's win). StepMania's old `gameplay battle …` and
  `evaluation win`/`lose` stay read and unused. Import finds them as
  any line (they count toward finding an announcer). Pack authors:
  `docs/announcer-lines.md`, "In battle".
- **When each fires** (`ddi_session::battle::Battle::take_events`, from
  the judged stage each frame): the opening at the play's start (the
  match's on stage 1, a one-stage match's too, never stacked; the final
  stage's on the last of two or more; else a stage's), in `ready`'s place,
  `here we go` unchanged; life critical when the player's gauge enters
  danger while still in (a transition: a gauge in danger from the start,
  as one life, never fires; once a stage; never after the stage is won);
  players left when the count still in becomes four or two, having been
  more (only the count reached; a count passed over in one moment is not
  said; at one the stage is decided); out when the player's gauge
  empties (it silences the same frame's critical life and players left);
  the stage won the moment it is decided with the player alone first;
  drawn when the player shares first as it is decided; at the song's end
  won, drawn or not won for one still in; the winner's finished song on
  the results of an early win (not after a quit); the match won, drawn or
  not won on the last stage's results, once the room has confirmed it
  (`PracticeRoom::match_end`, handed to `PlaySession::stage_reported` by
  each shell after its report). A quit stage says `evaluation quit` as
  before. Nothing in the room speaks (no voice there yet).
- **Precedence and freshness** (`ddi_session::announcer::battle`, the
  provisional `BATTLE_VOICE_V0`): in a stage a play's routine lines are
  left out (combos, comments, life lost, last life, the full combos,
  cleared, failed, the results' grades, records and cheer: the design's
  "working battle voice preset"), so a stage decodes only the battle's
  lines and their fallbacks (`battle::wanted`, both shells' `For::Stage`);
  one battle line at a time: a moment due while a line that matters as
  much or more still speaks is passed over (counted in the report's
  `dropped`), never queued or said late; one that matters more speaks at
  once over the end of the other (the voice bus cannot cut a sound); in
  one frame the moment that matters most is tried first; on the last
  stage the match's line takes the stage end's place where the pack has
  it. Judging, the clock and the music never wait.
- **Visual fallbacks** (`PlaySession::battle_cue`,
  `ddi_shell::opponents`): a moment of the play itself (opening, life
  critical, players left, out, drawn) whose own line was not said (a
  fallback such as `ready` names no stage, so it still shows) takes the
  column's heading's place for 2.5 s, faded in and out over 0.25 s, at
  once under reduced motion; "Stage won" already shows for the rest of
  the song; the results' moments show on the results (the stage note)
  and in the room (the match's results). Words in `locales/*/room.ftl`
  (`room-play-cue-*`).
- **The studio** lists the category as its own section, "In multiplayer
  battles", after the results', with a note that they are heard only in
  multiplayer and have no scene to hear them in yet; no Hear button; the
  line picker marks them "· Multiplayer"; an unrecorded one says the
  moment only shows on screen. `announcer/examples.toml` has two English
  and two Japanese example texts for each (text only, for `kokoro.py`'s
  pack files to copy later).
- **Shells**: both decode a stage's voice through the shared
  `battle::wanted`, report the outcome and pass the room's `MatchEnd` back
  to the session; the web's `__DDI_DEBUG.battle` (`policy`, `told`,
  `cue`, `cue_drawn`), desktop's log (`battle: … (shown) …`).
- Tests: `crates/library/src/announcer/tests.rs` (folders, category,
  fallback chains, a pack of play lines speaking for battle moments, the
  studio's name matching unchanged for `lost`, `danger` and the rest);
  `crates/session/src/announcer/battle.rs` (priorities, what a stage
  decodes, one line at a time and never waiting, no combo line in a
  stage while a moment speaks, the cue's fade and reduced motion, what
  counts as said); `crates/session/src/play/tests/stage.rs` (a two-stage
  match with a full pack: each opening in ready's place, no routine line
  though the combo passes the pack's count, each moment once, the stage's
  end on its results and the match's in the last one's place; out of life
  said, said by `gameplay failed` and shown, or shown with the announcer
  off, and the cue's lifetime; players left told once at two with an early
  win and the finished song); `crates/ui/src/state/room.rs`
  (`match_end`); `crates/ui/src/screens/studio.rs` (the section, its note,
  no scenes, the picker); `crates/shell/src/opponents.rs` (the cue in the
  heading's place, fading); the `qps-ploc` walk (the studio's battle
  section, the cue words); `crates/desktop/tests/room_stage_flow.rs`
  through the real host. At runtime: headless Chrome and Firefox played
  a two-stage practice match with `bundled:mr. clank` chosen
  (`target/scratch/mp/m4.mjs`), each opening shown beside the field.

Not built, noted for later (the design's other events): the readiness
deadline warning, stage preparing and next stage approaching as
intermission cues in the room (the room has no voice: the play's opening
coalesces them), attacks (incoming, launched, weakened), a stage
interrupted and a match interrupted (the room shows "Match stopped"),
the spectator's neutral milestones and outcome, participation ended by a
disconnect. A studio scene for battle lines; recorded battle lines in
our packs (`tools/announcer-design/pack.toml`/`pack-ja.toml` have no
entries: adding them would make the next `kokoro.py pack --bundle`
generate and ship them, and its lead style needs a `battle` place in
`PLACES` and a `[lead] battle`; the texts are ready in `examples.toml`);
the import report's per-place counts do not count battle lines
separately.

## M5c: invite-only online rooms, the client (implemented, 2026-11-10)

The game's side of the private online room, against the server built in
M5a/M5b (`../ddi-server`, its README: sign-in, rooms, `RoomDO` running
`ddi-room`'s reducer). Both shells, behind the same hidden switch. Stage
content is **not** shared yet (upload grants come in M5d): an online
stage's song can be chosen and marked available only on a development
server, and the room stops at a labeled development point after the
countdown, as M2 did. Nothing here is working song sharing. (M5d, below,
replaced both: the development content is gone from the clients, and
stages are shared and played.)

Implemented and tested:

- **One room client for practice and online** (`ddi_ui::state::room::RoomClient`,
  `PracticeRoom` before): its link is the practice's `LocalAuthority` or an
  online link over `ddi_room`'s `OnlineAuthority` (`crates/room/src/online.rs`),
  behind the same `RoomAuthority` trait; the room screen is the same one.
  Roles come from the shared rule (`ddi_room::can`, `RoomClient::may`):
  Choose song and Start (and what Start becomes) only for who may, Ready
  only for a player gathering readiness (never a grayed Ready for a watcher
  or a newcomer waiting for the next match), More's Copy invite link for
  operators and Replace invite link for the owner. Rows say Host, Co-host,
  Admin, Watching or Next match, and a person's connection (Joining,
  Reconnecting, Away).
- **The online adapter** (`OnlineAuthority`): the envelope as JSON on a
  shell's socket (`Transport`), a mirror whose gap (or a lost first
  snapshot) sends `Resync` (`{"v":1,"want":"snapshot"}`, the server's
  existing frame, now a protocol type: no version change) once, again
  only after 2 s; commands kept until answered and sent again with their
  own `(client, seq)` after a reconnect (the server answers a repeat
  without applying it; an `out_of_order` answer to a resend is taken as
  handled); duplicate answers and old revisions passed over; a fresh
  ticket for each socket; link states joining, connected, reconnecting
  (backoff `RECONNECT_BACKOFF_V0`), and the ends no retry mends: 4401 sign
  in again, 4410 room closed, 4000 left. A refusal that comes later (an
  online answer) is said on the status line.
- **`FakeServer`** (`crates/room/src/fake.rs`): the room in process behind
  JSON both ways, sockets with one-use tickets, faults (delay, duplicate,
  drop, reorder, an old revision again, a close code). The adapter
  contract test runs one script (joins, a co-host, two refusals, a stale
  edit, content, readiness, the readiness window with a never-connected
  member sitting out, the countdown, the stage) through `LocalAuthority`
  and through `OnlineAuthority` + `FakeServer` and requires the same
  answers and the same room; again under five fault sets.
- **Sign-in** (`ddi_ui::state::online`, sans-IO over the shell's `Http`):
  the web leaves for the server's `auth/twitch/login?return=<route>` and
  comes back to `#/mp`, `#/mp/join/<invite>` or `#/mp/room/<id>`
  (`?signin=canceled`), what it was for (make a room, join an invite, back
  into a room) kept in `sessionStorage` and resumed after `/me`; desktop
  asks for an attempt, opens the system browser on it (the same way it
  opens links), long-polls it one poll at a time (a lost poll again after
  2 s), Cancel deletes it (a later callback signs nobody in), and keeps the
  session in the config folder's `online-session` (owner-only on Unix),
  replaced by a refreshed one from `x-ddi-session`. `GET /me` shows who is
  signed in; Sign out. CPU practice never asks.
- **Flows**: the home's Private room (signing in first when signed out)
  makes a room and opens it; More → Copy invite link puts the web link
  (`<site>#/mp/join/<invite>?mp=1`: it carries the switch) on the
  clipboard; the invite's page shows the room's name, host, people, and
  whether a newcomer joins now, waits for the next match, or cannot
  (closed, full, expired, replaced, unknown, room closed, server
  unreachable) before signing in; Join (or Sign in with Twitch to join)
  joins, rechecked by the server after signing in; Replace invite link
  (owner) refuses old links; Leave room tells the room. Desktop opens an
  invite from the home's field or `--join LINK|INVITE`.
- **Readiness and the countdown on the server**: Ready and Start reach the
  server's readiness window (its alarm) and countdown; then the stage's
  phase, where the status says "Development stop: online stages aren't
  played yet. Stop the match to go on."
- **Development setup**: `Trunk.toml` passes `/api/` with redirects
  untouched (sign-in is a chain of them) and `/api/sockets/` as WebSockets
  (Trunk takes HTTP or WebSockets under a path, not both, so the server now
  also takes the socket at `sockets/<id>/ws`, the path the app uses). With
  `just dev` in `../ddi-server` (port 8787, its development identity
  provider) and `just dev` here, open `http://localhost:8080/#/?mp=1`,
  then `#/mp?mpdev=1`: Private room goes to the provider's form (type a
  name), back to the room. A second browser profile opens the copied
  link. Desktop: `DDI_API_URL=http://127.0.0.1:8787/ditoditoinfinito/api
  ddi --multiplayer on --screen multiplayer --mp-dev-content` (the
  server's `API_URL` must be the address the browser reaches it at).
- Tests: `crates/room/tests/online.rs` (the contract, faults, a reconnect
  after missed revisions, a resend after a lost answer, stale, a gap,
  4401/4410/4000, the backoff, the resync frame); `crates/ui/src/state/online/tests.rs`
  (desktop's attempt from Private room to a room made, canceled and
  failed attempts, a lost poll, the web's leave-and-return resuming a
  join, the invite's preview in each state and a refused join, an online
  room over the fake room to the window, countdown and development stop,
  a song waiting for sharing without the development switch, signing in
  again after 4401 and a reconnect); `crates/ui/src/screens/room_tests.rs`
  (roles, connections and the link's states on the screen); the
  `qps-ploc` walk covers the home on both shells and signing in, every
  invite state signed in and out, and the online room as owner and member
  through its link's states; `crates/desktop/tests/online.rs` (with
  `DDI_ONLINE_API`) signs in two people through desktop's attempt against
  a local server, makes, previews, joins and leaves through desktop's
  HTTP and socket. At runtime against `just dev` of the server (the
  development provider): two headless Chrome profiles and one Firefox
  (phone size) walked sign-in from Private room, the invite's preview
  signed out, a canceled sign-in and its retry, the join, a third person,
  the host's song, Ready, the readiness window and the countdown, the
  development stop, a dropped socket coming back, Replace invite link with
  the old link refused, and Leave (`target/scratch/mp/m5c/online.mjs`).

Development-only or mocked in M5c:

- **Stage content**: an online stage's song is the host's local pick; the
  others see "Song chosen by the host" and download nothing. With
  `mpdev=1` (web) or `--mp-dev-content` (desktop) the client calls the
  server's `POST /rooms/<id>/dev/content`, which exists only in a
  `worker-dev` build with `DEV_FAKE_AUTH=1`, and counts available content
  loaded with nothing downloaded. Without it a chosen song waits ("waits
  for song sharing, which isn't available yet") and Start is refused.
- **The development stop**: no online stage is played; Stop match returns
  the room to its results.
- The invite link's site on desktop is the API's base without `api`
  (`DDI_SITE_URL` overrides): right for production, not for a local
  server whose API is not beside a page.

## M5d: a stage's song shared with the room, and the stage played (implemented, 2026-11-10)

The online room's stages are real: the host's local song reaches everyone
through the existing song sharing, authorized by the room, and the room's
countdown leads into a stage each member plays on their own device. This
replaces M5c's development-only content and its development stop. The
server side is in `../ddi-server` (its README, "Room shares" and "Rooms").

Implemented and tested:

- **One share contract** (`crates/share-protocol`, `ddi-share-protocol`,
  serde only): ids, limits, expiry words, the share's JSON as made,
  answered and listed, path checks, file paths and routes, and the upload
  grant's claims (`UploadGrant`, `GrantAnswer`, the grant's refusal codes).
  `ddi_library::share` re-exports it (the game's callers unchanged); the
  server depends on it by path and dropped its copy. The share routes'
  wire contract is byte for byte as before (the 52-request contract run
  against `wrangler dev` before and after, ids aside).
- **The commit is the pick**: an operator's `ChooseContent` makes the
  stage's content pending and clears its readiness (the reducer did so
  already). The client then asks the server for the stage's upload grant
  (`POST /rooms/<id>/grant {stage}`: the room checks the asker's role, the
  stage not started and its content waiting for its share) and hands the
  shell an `UploadJob`; the shell uploads with its own sharing code
  (`Authorization: Grant`; the web's `share::upload_room`, an imported or
  a bundled song; desktop's `share::upload_room` on a thread). Progress is
  on the status line ("Sharing stage n's song with the room… p%"); a
  failure says why with More's Retry sharing; an expired grant is renewed
  and the upload made again; the others see "Stage n's song is being
  shared with the room…".
- **The server's word, not the client's**: on `finish` the Worker tells
  the room, which makes the stage available with the share's id when the
  share's song lists the chosen chart's digest, else blocks it
  (`unavailable`, `digest_mismatch`, worded on the status line, Retry
  offered). The same content again in the same room reuses the room's
  share (no upload). The room closing deletes its shares.
- **Loading from the share**: every member prepares an available stage:
  the operator who chose it from their library, the others from the share
  (`LoadJob::share`: the web reads it into memory, several shares at once
  now; desktop into its cache, registered for plays), the chart found by
  its digest and reported Loaded only then (`prepare_chart`,
  `finish_shared_load`; a different chart is `Changed`, a failed download
  `Download`). Readiness is Ready and Loaded, as the model had it.
- **Start as one command** (M5c's open question): `StartStage` sent in the
  lobby for the playlist's first stage starts the match and that stage
  together; refused, it changes nothing (no match locked, a newcomer still
  joins to play). `StartMatch` stays for protocol 1's older clients. Both
  clients send the one command; practice keeps wording its own refusal
  before the press.
- **The stage**: after the room's countdown each member's shell plays the
  stage with M3's stage path on the shared song (`StageOrder::others`: no
  CPUs, other people on their own devices; nothing decided on this
  device, no early win, the stage-end line not said), presses Start on its
  prompt as in practice, and reports its own run (`report_run`:
  `ReportOutcome` with `ReportedRun`, life and accuracy in millionths or
  out at a millisecond, or a forfeit for a quit or Back on the prompt).
  The room (`ddi-room`'s reducer, the same the practice runs) ranks the
  reports with the match's battle (`gauge-survival-v0`), awards the
  points, and moves to the intermission or the match's results as
  practice does; one who never reports is a forfeit at the deadline
  (`OutcomePolicy`, below). Results go back to `#/mp/room/<id>`.
- Tests: `ddi-share-protocol` (5: ids, a share's checks, the wire forms as
  before, URLs and routes, grants); `ddi-library` (the share tests
  unchanged, a grant's refusals); `ddi-room` walkthroughs (the atomic
  start refused whole and accepted, an online stage settled on reports and
  on its deadline after the song's end, the cap, no reports on a CPU
  stage); ddi-server natively (grant signing, expiry and kind; scope
  checks, forced owner and expiry, a renewed grant; reuse of a finished
  share of the same content; the chart digests of a record; the room's
  grant checks, content ready and its digest mismatch, outcome collection
  and the deadline on the alarm) and against `wrangler dev`
  (`scratchpad/m5d/grants.mjs`: 24 checks, and with a short idle life the
  room's close deleting its share); `ddi-ui` over `FakeServer`
  (upload-then-load, a stage played and settled from both reports, a
  member who never loads sitting out, an expired grant renewed, a failed
  upload retried, the server's digest refusal, a member's wrong chart, a
  pending song never starting nor locking a match); the room screen and
  the `qps-ploc` walk through the sharing states; `ddi-session` (an
  online stage never decided on the device, its own run reported);
  `crates/desktop/tests/online.rs` with `DDI_ONLINE_API` (desktop's upload
  under the grant, the guest's download into the cache, the start, both
  runs, the intermission). At runtime against `wrangler dev`: two headless
  Chrome profiles and one Firefox (phone size, Japanese), a whole
  two-stage match (`target/scratch/mp/m5d/online.mjs`).

Trust and access limits of the prototype (say so to anyone trying it):

- **Outcomes are self-reported.** Each player's device judges its own play
  and tells the room its life and accuracy (or when it went out); the
  room cannot check them, and a modified client could claim anything. No
  replay, input log or server-side judging backs a result.
- **Room shares are readable by id**, as any share: the id goes to the
  members over the room's socket, but anyone holding it can read the
  song. This is not access limited to the room (a member read grant on
  the share's URLs is the later step). Nothing in the game's text says
  the songs are private.
- The others are not shown during a stage (no opponents' column, no
  battle traffic); each sees the room's ranking after everyone reported.

Development-only or mocked in M5d:

- The server's `POST /rooms/<id>/dev/content` stays, only in a
  `worker-dev` build with `DEV_FAKE_AUTH=1`, as a test aid; the game no
  longer calls it (`mpdev=1` and `--mp-dev-content` are gone).
- The development identity provider stands in for Twitch in every run.

## Provisional choices


| Choice | Where | User-visible effect | Decision needed |
| --- | --- | --- | --- |
| Under-eight awards: `ExperimentalLinearV0`, place `p` of `n` earns `n + 1 - p` (practice's 4-player field: 4, 3, 2, 1) | `awards::AwardRule` | Practice and small rooms show points on this scale, marked experimental | The award rule for fields under eight |
| Award field = the stage's committed starters; sit-outs are not counted | `awards::AwardScheme::rule_for` | A stage with seven starters out of eight seats uses the small-field rule | The population basis for points with sit-outs |
| A confirmed forfeit earns zero; its place's award goes unused | `awards::StageResult::award` | A disconnect forfeit adds nothing, earlier points stay | Forfeit scoring (the connection-loss study proposes zero) |
| Readiness policy `hosted-deadline-v0`: 15 s window, 3 s countdown, at least one ready human and two competitors | `rules::ReadinessPolicy` | Start opens a 15 s window when someone is unready | Window and countdown values; minimum population |
| The window waits only for those who could still become eligible (not ready, loading, load failed, disconnected); departed and watching competitors sit out without a wait | `SitOutReason::may_still_join` | Start is immediate when only leavers are missing | Whether a load failure or a disconnect should hold a window |
| A committed starter leaving or disconnecting before the start cancels the countdown | `reduce.rs` | Everyone returns to preparation; the host starts again | Roster-invalidating failures; repeated cancellations |
| More than eight starters have no award rule | `AwardScheme::rule_for` | Cannot happen under the cap of eight | A rule before raising the cap |
| CPU seeds per seat derived from the match's profile seed | `member::OpponentSpec::for_seat` | Each CPU of a match plays its own fixed seed | Per-CPU profiles, personalities |
| Present people who want to play hold a seat at the match start whatever their connection | `reduce.rs`, `start_match` | A disconnected person sits out stages rather than losing the seat | Roster lock for absent people |
| Persistent elimination ranks by points like the default | `state::Match::standings` | Under that option the standings are not yet survival order | Its ranking (the design says survival placement) |
| Admins may exceed the member limit by `admin_reserve` (4) | `rules::RoomPolicy` | Admins can always enter to help | Operator-observer capacity |
| Protocol version 1, accepting only 1 (`OLDEST_PROTOCOL_VERSION`); the first incompatible change must keep the previous accepted | `protocol.rs` | None yet | The server's compatibility window |
| The room keeps the last 8 finished matches | `RoomPolicy::history` | Older match records drop from the room | How long a room keeps results |
| Practice CPUs start at level 3 of 5 (the engine's `normal`), seeds from the time the room was made | `state::room::DEFAULT_CPU_LEVEL`, `PracticeRoom::new` | A new practice room shows "Level 3 of 5"; two rooms differ in their CPUs' seeds | Practice's default profile; whether the seed should be shown or fixed |
| A chart is playable by the CPUs when its style is a built-in one-player layout with four lanes; song-declared layouts are refused | `state::room::eligible` | Solo, Double, Pump, techno and DanOni 5–9 charts, and a song's own four-lane layout, are refused with a reason | Whether to accept song-declared four-lane layouts before M3's load check |
| Chosen local content counts as loaded at once | `PracticeRoom::sync_loading` | Ready alone makes the player eligible | Replaced in M3 by the real load and plan check |
| Battle `gauge-survival-v0` (`ddi_room::battle::BattlePreset::GaugeSurvivalV0`, practice's `MatchRules::battle`): life is each competitor's own ruleset gauge, empty is out of the stage | `crates/room/src/battle.rs`, `crates/session/src/battle.rs` | A stage is survival under the player's chosen ruleset's gauge; Rules says "Survival · provisional" | The standard battle (life, power, weakening) and its numbers |
| Ranking: still in by life, then accuracy (the ruleset's percent, money or EX over its most); then the out, later first; then forfeits; exact ties share | `ddi_room::battle::rank` | Placements follow life first; the CPUs' accuracy is on the taps-only copy, the player's on the full chart (not like for like) | Ranking survivors; whether CPUs should play the full chart |
| Decided once at most one competitor is still in; the last out together share first; the winner's fail is turned off to the song's end | `battle::decided`, `PlaySession::tick` | "Stage won" over the column; the player finishes the song whatever the gauge does after | Final-contender settlement once attacks exist |
| The player out: the play ends as a failed play does, the remaining CPUs played to their end at once | `Battle::outcome` | No watching after elimination; the results come after the fail's hold | The spectator view and when the eliminated return to the room |
| A quit, an interruption (page hidden, audio stopped) or Back on a stage's prompt is a forfeit; a song unreadable at play time stops the match (`Interruption::ContentLost`) | `PlaySession::tick`, shells | Losing focus mid-stage costs the stage | Local interruption versus forfeit; reconnection |
| A stage plays with no turns, note removals or rate, the play options row not offered, no ghost, no singleplayer countdown | `SessionConfig::player_spec`, prompt | The player's other settings (speed, skin, rules) apply | Which personal options a stage allows |
| Stage plays are not kept in the play history or bests | both shells' record paths | A practice match leaves no history | Whether a stage counts as a personal play |
| Battle voice `battle-voice-v0`: priorities outcomes/out 40, (attacks 30, none yet), life critical 20, players left 10, openings and the finished song 5; one battle line at a time, a lesser or equal one passed over, never queued | `ddi_session::announcer::battle::BATTLE_VOICE_V0` | Which battle line speaks when two meet | The battle voice's priorities and whether a line may cut another (needs a voice bus that can stop one sound) |
| A stage leaves out a play's routine lines (combos, comments, life lost, last life, full combos, cleared, failed, grades, records, cheer) | `battle::stage_speaks`, `Announcer::tick` in a stage | A pack without battle lines says only ready, here we go, failed (out) and cleared (finished after a win) in a stage | Whether some routine lines (combo milestones) belong in a battle voice preset |
| A moment shows only where its own line was not said; for 2.5 s with a 0.25 s fade; in the column's heading's place, nothing in bars-only columns or with no column | `VoicePolicy::cue_seconds`, `cue_fade`, `ddi_shell::opponents` | A phone's narrow column has no cue | Whether cues show always, and where on a phone |
| Survivors milestones at four and two only; life critical is the gauge's own danger, once a stage, never from the start | `SURVIVOR_MILESTONES`, `Battle::tell` | A one-life gauge never says life critical | The battle preset's danger threshold once battle life exists |
| The match's line is said on the last stage's results, in the stage end's place; nothing is spoken in the room | `PlaySession::stage_reported` | The match's outcome is heard as the last results open | A room voice for intermission and match results |
| Each stage is its own Start after the room's countdown (practice's prompt asks for Start) | shells | Two presses per stage: the room's Start, then the prompt's | A shared start in hosted rooms (the room's countdown starts the play) |
| Practice still needs the player's Ready before Start, even alone with CPUs | `PracticeRoom::start` (the model's `min_ready_humans`) | Start says "Press Ready first." until Ready | The entry study had local Start without Ready; the handoff forbids automatic Ready. Keep both presses, or let Start mean Ready in practice? |
| The next match's playlist starts empty (the model hands the finished match's stages to its record) | `ddi_room` `MatchStarted` | After Next match, every stage asks for a song again | Carry the last match's songs into the next in practice? |
| A chart's content key is its digest, else `chart-<index>` where the library has none yet | `state::room::content_id` | None (practice) | Online rooms need the digest; refuse digest-less charts there |
| The practice room lives for the session (web) or run (desktop), never stored | `crates/web/src/room.rs`, `crates/desktop/src/room.rs` | A reload starts a new practice | Whether practice setups should persist |
| Reconnect backoff `RECONNECT_BACKOFF_V0`: 0.5 s, doubling to 8 s, forever; a resync asked again after 2 s | `ddi_room::online::ReconnectPolicy`, `RESYNC_AGAIN_MS` | A dropped socket is back within a second; a long outage retries every 8 s while the others see Reconnecting, then Away after the server's grace | The reconnect window and when to give up |
| A resent command answered `out_of_order` is taken as handled (its outcome unknown, the room's state shows it) | `OnlineAuthority::received` | A command sent twice across a reconnect never says "refused" | Keeping every outcome per seq on the server instead of the last |
| Online rooms refuse charts with no digest; no CPU check (hosted rules seat no CPUs) | `RoomClient::choose` | A song the library has not read yet is refused with a reason | Hosted CPUs, and how content keys are formed for sharing |
| The client id is fresh per tab (web) or run (desktop) | `crate::online::client_id` on each shell | A reload is a new client (the server's last-seq record is per client) | Whether retries should survive a reload |
| A non-operator still opens Rules (the room refuses what they may not save) | `screens/room.rs` | A member can look at the rules; Apply is refused | Read-only Rules for members |
| Online outcomes `self-reported-v0`: a stage settles when every starter reported, else 60 s after the first report of the song played to its end, else 20 min after its start; who never reported is a forfeit | `ddi_room::OutcomePolicy` (`RoomPolicy::outcomes`) | A player whose device stops reporting costs the others a minute at most once someone finished; a late Start press past the minute forfeits | Checked outcomes (server judging, replays), and a deadline from the song's length |
| Reported runs in whole numbers (life and accuracy in millionths, out at a millisecond) | `ddi_room::ReportedRun` | Ties a little coarser than practice's floating ranking | None expected |
| Online stages are never decided on a device (no early win, the winner does not "finish the song" alone); the stage-end line is not said | `ddi_session::battle::StageSetup::others` | Every player plays to their own end or fail; the result comes in the room | The live battle (the others' state during a stage) |
| A stage's content key is its chart's digest; the same digest again in a room reuses the first share (its music and pictures too) | `ddi_share_protocol::content_key` | A second stage with the same chart uploads nothing | Whether the key should cover the song's files |
| A grant: thirty minutes, one song, a whole share's byte limit (2 GB); room shares expire with the room's idle life (seven days from their making) and are deleted when it closes | `ddi_share_protocol::GRANT_TTL_MS`, `../ddi-server` `RoomConfig::share_life_secs` | A song shared in a room lives at most a week unless shared again | Per-room limits and the life of a long room's shares |
| Any operator (owner, co-host, admin) may ask for a pending stage's grant; only the client that chose it asks by itself | `../ddi-server` `RoomCore::grant_request`, `RoomClient::sync_online` | A co-host can't take over a host's half-done upload from the room screen yet | The "host gone before upload" recovery the study proposes |
| Start in the lobby is one command (`StartStage` on the first stage); `StartMatch` kept for protocol 1's older clients | `reduce.rs` | A refused Start locks nothing | None (M5c's question answered) |
| Desktop's session in the config folder's `online-session`, owner-only on Unix, plain text | `crates/desktop/src/online.rs` | Signed in across runs until the session's thirty days | The system keychain instead |

## Open questions

- Fields under eight: keep a linear rule, scale the eight-player table, or
  something else? Should sit-outs count toward the field?
- Should a forfeit occupy a place (it does: last of its group) or be
  ranked separately below everyone?
- Readiness in the lobby gathers on the first stage before the roster is
  locked; is that what hosts expect, or should readiness begin with the
  match?
- Who may reorder or swap playlist stages (not modeled: content can be
  replaced per stage, stages cannot move)?
- Host departure and ownership transfer are not modeled; an absent owner
  keeps the role.
- Practice: should Start count as Ready when the player is alone with
  CPUs (the entry study's "Local Start match needs no separate Ready
  action"), or stay two presses as built?
- Practice: should the next match keep the finished match's songs?
- The multiplayer home is a page with one working choice; the design's
  Find match, Browse rooms, Create room and Join by link come with the
  online slices (or not at all for this prototype).
- M3: should CPUs play the full chart (holds, mines) once they can, so
  that life and accuracy compare like for like? Today they play a taps-only
  copy and the player the full chart.
- M3: should a stage start on the room's countdown alone (no prompt
  Start), as a hosted room's shared start would?
- M3: what should an eliminated practice player see: the remaining CPUs
  (a spectator view) or their results at once, as now?
- M3: should a local interruption (the page hidden) be a forfeit, as now,
  or interrupt the match?
- M3: is a stage play a personal play (history, bests, calibration
  hints)? Today it is not.
- M3: a CPU keeps its seed for the whole match, so the same chart chosen
  for two stages is played the same both times. Derive a seed per stage?
- M4: should a stage keep some of a play's routine lines (the combo
  milestones), or stay as the design's working preset has it (outcomes
  own the voice)?
- M4: should the battle's moments show always (not only where no line
  said them), and where on a phone, whose column has no room for words?
- M4: should the room speak (next stage, final stage, match results), or
  is the play's opening and the last results enough for practice?
- M4: may a battle line cut another short? The voice bus cannot yet stop
  one sound; today a line that matters more starts over the other's end.
- M5c: a refused Start in a hosted lobby used to lock the match.
  Answered in M5d at the model: Start in the lobby is one command
  (`StartStage` on the first stage), refused whole. A Start that opens a
  readiness window still locks the match (the window belongs to it): is
  that right when the window then ends below the minimum?
- M5d: outcomes are self-reported. Is that acceptable for private rooms
  among friends, or should a stage's result wait for checked outcomes
  (inputs sent and judged by the server, or replays)?
- M5d: room shares are readable by anyone holding the id. Is a member
  read grant needed before rooms leave the prototype?
- M5d: each device starts its play on its own Start press after the
  room's countdown. Should the countdown start the play itself (no
  prompt Start) so everyone starts together?
- M5d: what should the others see during an online stage (nothing now:
  the opponents' column is empty)?
- M5d: who recovers a stage whose host left before the upload finished?
  Today any operator may ask for its grant, but the room screen offers it
  only to the device that chose the song.
- M5c: the sign-in prompt is the home's page itself (a line and the
  buttons), not a separate panel naming the destination; is that enough?
- M5c: who may copy the invite: the server gives it to operators
  (owner, co-hosts, admins); should members be able to share it?
- M5c: an admin who is not a member enters watching from a ticket; the
  client has no admin-specific entry point beyond a link.
- M5c: the room's countdown seconds briefly replay part of the screen's
  entrance on each rebuild (seen in practice too: a short dim flash each
  second in headless screenshots); not changed here.
