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
  stage. A spectator view and Watching [name] are later work (M6 adds
  the miniature fields).
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

## M6: the opponents' miniature fields, a first look (implemented, 2026-11-10)

A practice stage's CPUs are shown as small fields of their own play in
the side space, on both shells, for the maintainer to iterate on the look
from screenshots. It replaces M3's column where the sides have room; the
column stays as the fallback. (M6b, below, changed the look after the
maintainer's review: read it for what is current.)

Implemented and tested:

- **A stream of judged moments per player** (`ddi_session::battle::feed`):
  `Moment { at, tick, lane, judgment, life, out }`, the time it was
  judged, which note of the copy (its tick and lane), the judgment, the
  life after it and whether it put the player out. A `FieldFeed` holds a
  player's moments and draws them a presentation delay after they
  happened (`LOCAL_DELAY` = 0 for CPUs judged on this device; a network
  feed would set its own): a hit note leaves the field once its moment
  shows, a missed one scrolls on past the receptors, each lane flashes
  its last judgment for `FLASH_SECONDS` (0.4 s), life and Out follow the
  last moment shown, and out of the stage the field goes blank as a
  play's does. Nothing is inferred from a note reaching the receptors.
  The local CPUs feed it from their own judged play
  (`Opponent::take_judged`, the engine's judge events of each frame,
  turned into moments by `Battle::advance`); the view never reads the
  CPU's player, so an online room can later feed the same view from the
  network.
- **The copy they play, never the chart**: `TapTrack` is
  `ddi_engine::cpu::project` (holds and rolls as taps; mines, lifts and
  fakes left out), the same copy the CPUs are judged on, one for all of
  a stage's CPUs.
- **The same song clock**: every miniature scrolls on the local play's
  drawn song second, at the player's own speed and direction (Reverse
  puts their receptors at the bottom too); the copy's notes fade in over
  the last lane of a miniature's height and out half a lane past its
  receptors (a miniature has no edge to hide them behind).
- **One noteskin for everything**: the miniatures are drawn by the play's
  own `NoteskinPlay`, the instance its field is drawn with, in the same
  frame and the same passes, after the field
  (`ddi_render::scene::Miniature`, `Renderer::set_miniatures`,
  `NoteLayer::miniature`). No Lua runs for them and nothing is loaded
  twice: the skin's components draw their receptors and taps at the
  miniature's geometry with the frame's clocks, the receptors' `hit`
  clock and the explosion components' instances from the miniature's own
  judgments, while the play's own presses, judgments, held notes, drawn
  positions and camera are put aside and back. A receptor an actor draws
  (StepMania skins: its state is what the play's own presses' commands
  made it) is drawn as it rested a second into the play with nothing
  pressed for half a second, so a miniature's receptors do not press with
  the player's. An actor's explosion cannot be played without the Lua, so
  where the skin's explosions are not components (Peters-DDR, the other
  StepMania skins) the 2D pass draws a soft glow in the judgment's color
  on the receptor instead. Where the skin has no part, the built-in
  arrows draw it, as on the play's own field (the skin falls back the
  same way there).
- **Two runs whatever their number**: every miniature's panel and
  built-in parts in one run of sprites, then every miniature's skin quads
  in one run of quads. A run per miniature cost a render pass each
  (skins' quads blend in passes of their own) and doubled the GPU time on
  desktop; batched, the cost below.
- **Stable slots** (`ddi_shell::opponents::arrange`): the first half of
  the seats (rounded up) on the field's left, the rest on its right, in
  the order the room seated them; each side a grid of the same size, its
  columns from the field out, the block centered in the band the HUD
  leaves (below the score and above the watermark where they share a
  side, clear of the quit button's place whether it shows or not). A
  slot never moves: out, it stays, dimmed, with "Out" in its field and in
  place of its level. Above each field its name, its level and its life
  (filled from the middle, as the play's gauge); above the left side the
  heading "CPUs · taps-only copy", a battle's moment in its place for a
  few seconds where no line said it, and "Stage won" above the right
  side. No combo or score.
- **The dancing hand keeps its side**: while the hand plays the chart
  (on by default) its picture's frame takes its side (the right half by
  default) and all the miniatures go to the other side, in more columns.
- **The narrow fallback**: where a miniature's lane would be under 16
  logical pixels (a phone held upright: about 14), or lyrics show over the
  play, M3's column is drawn instead (names and bars, or bars alone).
- Reduced motion: nothing of the miniatures moves but what the play's own
  field moves (the notes scrolling); the flashes fade as the play's do.
- The development switch for measuring: `DDI_MINIATURES=0` on desktop,
  `localStorage` `ddi.dev.miniatures` set to `0` on the web, shows the
  column instead (`ddi_shell::opponents::set_miniatures`).
- Debug state: `__DDI_DEBUG.opponent_fields` (the slots, their sides, the
  miniatures' lane); desktop's `Host::opponent_fields` (tests).
- Tests: `crates/session/src/battle/feed.rs` (the copy taps-only and each
  note found again; a hit leaves and a miss scrolls on, flashes stop,
  nothing past the visible range; a delay holds the judgment, life, out
  and the blank field back; moments out of order take their place);
  `crates/session/src/play/tests/stage.rs` (each CPU's field fed every
  note of the copy once, in order, its life as shown its gauge's);
  `crates/render/src/scene.rs` (miniatures after the field on their
  panels, the built-in parts and a judgment's flash);
  `crates/render/src/noteskin.rs` (a miniature's own judgment's effect
  drawn at its lane, the play's hits, presses and drawn positions as they
  were, actors' explosions not drawn); `crates/shell/src/opponents.rs`
  (seven slots at 1280×720, 1920×1080 and a phone held sideways, never
  over the field or the HUD's areas, the same size, smaller than the
  play's lanes; fewer are larger; Reverse; the hand's side; a phone held
  upright falls back; Out in place; the cue and Stage won);
  `crates/desktop/tests/opponent_fields_offscreen.rs` (seven CPUs get seven
  slots through the real play host; ignored: the look's scenarios'
  pictures and costs).

Measured costs (seven CPUs in miniature against the same stage with the
column, Beatucada's Challenge chart on desktop and its Hard chart on the
web, CPUs at level 3 of 5, the player autoplayed, mean and 95th percentile
in milliseconds; this Mac's GPU; the dancing hand dominates where it is
on):

Desktop offscreen (`crates/desktop/tests/opponent_fields_offscreen.rs`,
`--ignored`; 220 frames at 60 Hz from 22 s, the same play switching from
miniatures to the column; "record" is the frame body's CPU time, "with
GPU" the same frame waited for on the GPU):

| Desktop, 1280×720 unless said | Miniatures: record / with GPU | Column: record / with GPU |
| --- | --- | --- |
| Infinito, hand off | 0.20 / 1.53 (p95 0.36 / 2.07) | 0.13 / 0.97 (p95 0.23 / 1.09) |
| Infinito, hand off, 1920×1080 | 0.19 / 1.51 (p95 0.33 / 2.11) | 0.14 / 1.20 (p95 0.26 / 1.92) |
| Peters-DDR, hand off | 0.40 / 1.43 (p95 0.60 / 1.99) | 0.18 / 0.93 (p95 0.33 / 1.06) |
| Infinito, hand on (the default) | 1.24 / 3.30 (p95 2.61 / 5.58) | 1.11 / 2.78 (p95 2.67 / 4.79) |

Web (`target/scratch/mp/m6a/m6a.mjs`, the frame recorder, `perf=1`, every
frame of a whole stage past its fifth second, about 5,900 frames; a
separate play for each column; "total" is the frame's main-thread work,
"GPU" WebGPU's submit to done):

| Web | Miniatures: total / GPU | Column: total / GPU |
| --- | --- | --- |
| Chrome (WebGPU), 1280×720, hand off | 0.83 / 2.63 (p95 2.1 / 3.6) | 0.69 / 2.22 (p95 1.6 / 3.6) |
| Chrome (WebGPU), 1280×720, hand on | 1.89 / 3.07 (p95 3.7 / 4.0) | 1.83 / 3.35 (p95 3.6 / 3.9) |
| Chrome, phone held sideways (844×390 at 3×), hand on | 2.05 / 3.55 (p95 3.9 / 4.5) | 2.10 / 3.44 (p95 4.1 / 4.4) |
| Chrome, Peters-DDR, 1280×720, hand off | 1.55 / 2.49 (p95 3.8 / 3.0) | not measured |
| Firefox (WebGL2), 1280×720, hand off | 1.70 (p95 4) | 1.39 (p95 3) |
| Firefox (WebGL2), 1280×720, hand on | 2.34 (p95 4) | 1.66 (p95 3) |

Seven miniatures cost about 0.1–0.2 ms of main-thread time a frame with
Infinito (0.2–0.5 ms with Peters-DDR, whose actors are emitted one by one),
and about 0.3–0.6 ms of GPU time; no browser dropped a frame (Chrome's
frames 16.7 ms apart throughout, Firefox's 8.3). A phone held upright
shows the column (no miniatures). No degradation was needed: the
miniatures use the skin's own textures and materials at their own size,
with its effects reduced to the hit effects its components draw. A
first version that gave each miniature its own runs doubled desktop's GPU
time (a render pass each); batched into two runs, the cost above.

What is real and what is not in M6:

- The miniatures show the CPUs' real judged plays of the tap-only copy,
  as they happen; they are not decorations driven by life.
- They are not the faithful miniatures of the design yet: no holds or
  mines (the CPUs play none), no combo or score, no power, no attacks,
  no No feed state, nothing of a remote player's own display settings
  (each miniature takes the local player's speed and direction), and the
  local player's skin rather than theirs. Online stages still show no one
  during the play (M5d); the feed is ready for a network stream, the
  stream is not built.
- Actor-drawn receptors are frozen at their resting state (their beat's
  animation from the effects' clocks still runs); actor-drawn explosions
  are replaced by the glow.

## M6b: the miniatures after review, nine players, far more CPUs (implemented, 2026-11-11)

The maintainer reviewed M6's miniatures (it plays smoothly) and asked for
these changes; each is built on both shells.

- **Combo.** Each miniature shows its player's combo, drawn by the play's
  own HUD shader (`ddi_render::hud`, the miniatures' instances of it: the
  same digits, outline tiers at each hundred and the pulse), placed in the
  middle of its field as the play's is in the play's field, scaled by its
  lane (a 72 px lane is the play's size). The stream's moments now carry
  the combo after them (`Moment::combo`). Legible rather than faithful:
  it is never scaled below 0.3 of the play's (`MINI_COMBO_MIN_SCALE`),
  so at the smallest lanes (16 px, 0.22 of the play's) it is larger
  than its field's scale; at 1280×720 with eight CPUs (32 px lanes,
  0.44) it is to scale. Hidden once out.
- **Life gauge** as the play's: the HUD's own gauge (its centered,
  less-than-linear fill, jade turbulence, damage and recovery trails,
  Vivid at full life, coral in danger), placed on each field as
  `life_rect` places the play's on its field (22 units below its top,
  8 high, four lanes wide, its units the lane over 72). The miniature's
  field is now shaped as the play's: its receptors a lane and a half from
  its end (`MINI_RECEPTOR`), seven lanes tall. Each miniature has its own
  gauge motion. The name and level moved above the field, where nothing
  else is.
- **The power bar to come: decided, room left.** It goes above the life
  bar, on the play's field and on every miniature
  (`docs/plans/multiplayer-ui.md`, "Play and spectator layout",
  decision). The play's gauge sits 22 HUD units below the screen's top,
  so the power bar fits in the 22 units above it; a miniature's gauge
  sits the same 22 units (scaled) below its field's top, inside its band,
  so the same room is there on every miniature. Nothing is drawn there
  yet.
- **The band.** A miniature's background is the play's field's band: the
  same black at the field filter's darkness (`filter_alpha` of the
  player's Options → Display filter), as wide past its lanes as the
  play's (0.3 lane), square, and absent where the play's field has none
  (the filter off, nothing behind the field). M6's darker rounded panel
  is gone.
- **The dancing hand goes behind the field during a stage**
  (`dance::Preview::set_behind_field`, the play study's "Hand behind
  field"): its picture's frame is the field's band, under the band's
  darkening, out of the miniatures' way; its Off still turns it off; a
  singleplayer play's hand is where the settings put it, as before. The
  hand never claims a side any more: the miniatures always use both.
- **Readable text**: names 14 px semibold (12 px where a slot is under
  100 px wide), levels and Out 12.5 px, the heading 13 px, brighter
  muted text; the column's text grew the same way.
- **The white blot** on Infinito's miniature receptors was the skin's own
  W1 burst (additive white, opaque for its first frames; the play's field
  shows the same burst), which at a third of the size covers the whole
  receptor. A miniature's explosion components are now drawn at 0.4 of
  their opacity (`noteskin::MINIATURE_HIT_ALPHA`): the burst stays
  visible as a hit, not a blot.
- **Nine players, eight opponents.** `RoomPolicy::INITIAL_COMPETITOR_CAP`
  is 9: the one policy check (`RoomPolicy::check_competitors`), so
  practice (CPUs 1–8 in Rules) and the private server (it runs the same
  reducer) both follow it; hosted rules default to nine seats. Awards for
  nine are `AwardRule::ExperimentalNineV0` (PROVISIONAL, named and
  versioned): the eight-player table for the first eight places and 0
  for ninth; marked experimental like the small-field rule; the Rules
  note now says points for fields other than eight are experimental.
  The server needs a rebuild and redeploy from this checkout to take the
  new cap (it depends on `ddi-room` by path; no server code changes).
- **Eight miniatures lay out** at 1280×720 and 1920×1080 in two 2×2
  grids (32 and 44 px lanes), on a phone held sideways in two rows of
  four (16 px lanes; slot gaps went from 12 to 10 px for it), and fall back
  to the column on a phone held upright.
- **Far more CPUs, development only.** `RoomClient::development` (policy
  `RoomPolicy::development`, cap 129; points `AwardScheme::DevelopmentV0`,
  linear for any field; one stage) is reached only by a development
  switch: the web's `#/?mp=1&mpcpus=N` (with the hidden multiplayer
  switch), desktop's `--mp-dev-cpus N`; never from a menu. It is for
  trying the layout and the cost. Where all the miniatures do not fit at
  the smallest lane (16 px), as many as fit are shown, the first seats in
  order, and a line under the right side says how many more
  ("+49 more", `room-play-more`); the column likewise holds what the
  screen's height holds, then says how many more. Nothing ever covers
  the field. (The room screen lists all the players; with a hundred it
  runs off its page: development only, not addressed.)
- Tests: `ddi-room` (the cap of nine, the nine-player rule, the
  development scheme), `ddi-ui` (practice's eight opponents, the
  development room), `ddi-shell` (eight slots at three sizes, in order,
  inside their bands with receptors a lane and a half down; 16 to 128
  opponents at two sizes: the first seats, a count, never over the field,
  the HUD or each other; the column's count), `ddi-session` (the combo
  carried and shown, delayed with the rest), `ddi-render` (the bands,
  the miniature's hit effect), the desktop offscreen test with eight CPUs.

Measured costs (Beatucada, the player autoplayed, CPUs at level 3 of 5, ms,
mean (p95); "miniatures" against the same stage drawn with the column):

Desktop offscreen (`opponent_fields_offscreen`, `--ignored`; 220 frames at
60 Hz, the same play switching from miniatures to the column; "record" the
frame body's CPU time, "with GPU" the frame waited for on the GPU):

| Desktop | Miniatures shown | Miniatures: record / with GPU | Column: record / with GPU |
| --- | --- | --- | --- |
| 8 CPUs, 1280×720, hand off | 8 (32 px) | 0.25 / 1.79 (0.43 / 2.49) | 0.14 / 1.00 (0.27 / 1.23) |
| 8 CPUs, 1280×720, hand behind the field | 8 | 1.33 / 3.54 (2.65 / 5.14) | 1.20 / 2.86 (2.71 / 4.65) |
| 8 CPUs, 1920×1080, hand off | 8 (44 px) | 0.28 / 1.79 (0.44 / 2.61) | 0.14 / 1.04 (0.30 / 1.32) |
| 8 CPUs, Peters-DDR, 1280×720 | 8 | 0.55 / 1.71 (0.82 / 2.16) | 0.27 / 1.12 (0.43 / 1.39) |
| 16 CPUs, 1280×720 (development) | 16 | 0.35 / 2.14 (0.57 / 2.73) | 0.22 / 1.16 (0.38 / 1.40) |
| 32 CPUs, 1280×720 (development) | 29, +3 more | 0.42 / 2.48 (0.66 / 3.14) | 0.20 / 1.13 (0.34 / 1.36) |
| 128 CPUs, 1280×720 (development) | 29, +99 more | 0.48 / 2.69 (0.73 / 3.51) | 0.27 / 1.23 (0.45 / 1.51) |
| 64 CPUs, 1920×1080 (development) | 64 (18 px) | 0.69 / 4.15 (0.93 / 5.53) | 0.22 / 1.24 (0.38 / 1.49) |
| 128 CPUs, 1920×1080 (development) | 79, +49 more | 0.91 / 5.15 (1.24 / 6.86) | 0.29 / 1.45 (0.51 / 2.42) |

Web (`target/scratch/mp/m6b/m6b.mjs`, the frame recorder over a whole
stage past its fifth second, about 5,900 frames; a separate play for the
column; "total" the frame's main-thread work, "GPU" WebGPU's submit to
done; both browsers held their frame rate throughout, Chrome 16.7 ms,
Firefox 8.3 ms):

| Web | Miniatures shown | Miniatures: total / GPU | Column: total / GPU |
| --- | --- | --- | --- |
| Chrome (WebGPU), 8 CPUs, 1280×720, hand behind | 8 (32 px) | 2.12 / 2.50 (4.0 / 3.5) | 2.15 / 2.44 (4.0 / 3.5) |
| Chrome, 8 CPUs, 1920×1080, hand off | 8 (44 px) | 0.85 / 2.96 (2.0 / 4.4) | not measured |
| Chrome, 2 CPUs, 1280×720, hand off | 2 | 0.76 / 2.08 (1.7 / 3.6) | not measured |
| Chrome, Peters-DDR, 8 CPUs, hand off | 8 | 1.54 / 1.89 (3.6 / 3.0) | not measured |
| Chrome, phone sideways (844×390 at 3×) | 8 (16 px) | 2.03 / 3.29 (3.6 / 4.0) | not measured |
| Chrome, phone upright (390×844 at 3×) | the column | 2.00 / 3.04 (3.7 / 3.9) | (the same) |
| Chrome, 16 CPUs, 1920×1080 (development) | 16 (38 px) | 1.00 / 2.70 (2.5 / 4.3) | not measured |
| Chrome, 32 CPUs, 1920×1080 (development) | 32 (25 px) | 1.29 / 2.71 (3.0 / 3.9) | not measured |
| Chrome, 64 CPUs, 1920×1080 (development) | 64 (18 px) | 1.75 / 2.75 (3.8 / 3.8) | not measured |
| Chrome, 128 CPUs, 1920×1080 (development) | 79, +49 more | 2.03 / 2.91 (3.9 / 3.8) | 1.18 / 2.44 (2.8 / 4.0) |
| Firefox (WebGL2), 8 CPUs, 1280×720, hand behind | 8 | 2.08 (4) | 1.74 (3) |
| Firefox, 64 CPUs, 1920×1080 (development) | 64 | 3.76 (6) | 1.34 (2) |

Eight miniatures cost about 0.1 ms of main-thread time a frame (0.3 ms
with Peters-DDR's actors) and 0.7 ms of GPU time on desktop; the 128
CPUs' own judging costs little (the column with 128 CPUs: 0.29 ms a
frame on desktop). What grows with the count is drawing: 64 miniatures
add about 3 ms of GPU time on desktop and 2.4 ms of main-thread time on
Firefox (WebGL2's submit), still within a frame here. Past what fits,
the cost stops growing (only the slots shown are drawn).

Proposed policy for large parties (not decided; the design leaves it
open): up to eight opponents, everyone in miniature; past what fits at the
smallest legible lane (about 16 px: some 29 at 1280×720 and 79 at
1920×1080), a fixed number of miniature slots chosen by relevance rather
than seat order (the followed player, the player's current target and
attacker once attacks exist, then the nearest in standing), stable for a
stage, the rest summarized as a count (and later a dense strip of life
ticks), since a stage's 100+ fields cost GPU time without being readable.

## M7a: the multiplayer home and the bars that stay put (implemented, 2026-11-11)

The maintainer's feedback on M6b's prototype, first part; both shells.

- **The home is the start menu's column** (`ddi_ui::screens::online_home`,
  still reached as `screens::online::OnlineScreen` with
  `OnlinePage::Home`): centered wide buttons in the start menu's style
  (`TitleMenu.Choice`, `TitleMenu.Disabled`), over the start menu's outline
  rain (`TitleMenu.Background`, held still under reduced motion), under
  the shared header and over the shared footer. Focus waits for the first
  key or button, which only focuses the first entry; hover moves it; Back
  (Escape, the header's button, a controller's Back) leaves. Rows shrink
  (to 44 CSS pixels at least) where the bars leave less room, as on a
  phone held sideways; a narrow footer drops its key hints.
- **Every entry the design plans** for the menu (`multiplayer-ui.md`,
  "Menu tree" and the entry study): Find match and Browse rooms grayed
  out ("· Coming soon", focusable and inert, as the start menu's
  Multiplayer), CPU practice, Private room (the design's Create room as
  far as it is built: invite-only, no public rooms), Join by link. The
  design's main three come first (Find match, Browse rooms, CPU
  practice), then the secondary two. The design's description beside the
  focused entry is not built (the column alone).
- **Join by link** is a page pushed over the home on both shells (it
  was desktop's field on the home; the web could only open a link): the
  invite field and Open invite; Back returns to the home.
- **Sign-in moved to the footer's right**: Sign in with Twitch signed out
  (or with the server unreachable), the signed-in display name otherwise,
  which opens a small menu with Sign out; nothing while the server is
  being asked. Sign-in is asked for where it is needed
  (`online_home::SignInDialog`): Private room signed out opens a dialog
  naming Twitch and what it is for (Cancel, Sign in with Twitch); the
  footer's Sign in opens it for the account. The dialog follows the
  sign-in through: desktop's wait for the browser (Open the browser again,
  Cancel), completing, canceled and failed (Try again), and closes itself
  once signed in, the private room then made. A sign-in already under way
  when the home is shown (the web back from Twitch, `?signin=canceled`)
  opens it too. Under the column a muted line says when the server could
  not be reached, the room is being made, or making it failed.
- **The bars stay put, everywhere.** The header and the footer (and what
  sits in the header's row: song select's search, mods and icons,
  Options' Saved and More, the results' Watch replay, the room's Rules
  and More) are the bars (`screens::Edge::HeaderBar`, `FooterBar`). From
  a screen with bars to another they no longer leave and come back: the
  leaving screen keeps them as they are while its own parts leave, and
  the next screen's show at once, so only what is in them changes, at the
  swap, without animation (for now). They still slide in and out to and
  from a screen without them (the start menu, the play's prompt). This is
  global: song select and Options both ways, the results to song select,
  the pages, the multiplayer home and the room, the room and song
  select's pick. The `Ui` decides (`Screen::shared_bars` says ahead
  whether a screen may have bars, `ui::has_bars` reads its tree;
  `BarsOn`, `BarsOff`, `BarsCancel` and `BarsStay` are the parts'
  commands). On the web each page has its own `Ui`: the route a
  `Navigate` or `Back` leads to says whether the next page has bars
  (`router::has_bars`, Back's target from the route trail,
  `router::back_target`), the leaving page keeps its bars, and the next
  page's `Ui` shows its own at once (`Ui::bars_kept`, `Ui::carry_bars`).
  The requests still wait for the exit, as before. The chart editor
  fades as a whole and has no bars (from song select its header leaves
  as before). Under the web's loader the bars' entrances are held like
  every part's.
- Tests: `ddi-ui` (the bars at rest every frame from song select to
  Options and back; they still come in from the start menu and leave to
  it; held until the reveal; carried over from another `Ui`; the home's
  entries, its sign-in dialog, Join by link and practice; the `qps-ploc`
  walk over the home, the dialog and Join by link).

## M7c: the play's sides after review (implemented, 2026-11-11)

The maintainer's feedback on M6b's play screen; both shells (the shared
`PlayView`, `ddi_shell::opponents`).

- **The hand is larger behind the field.** Its frame is the field's band
  from just below the tap judgment's place (`ddi_render::scene::judgement_area`)
  to the bottom (under Reverse: from the top to just above the judgment),
  so the receptors and the judgment never stand over it; the combo may.
  Inside it the hand is drawn 1.9 times as large as the band's width
  framed it before (`dance::BEHIND_FIELD_ZOOM`, PROVISIONAL): the mat it
  plays on is about two thirds of the band wide, the hand and wrist
  filling the room under the judgment. The arm, cut at the frame's top,
  fades in over its top quarter (`Renderer::set_field_underlay_fade`,
  `BEHIND_FIELD_FADE`) rather than ending on a line. Pictures at
  1280×720, 1920×1080 and a phone held sideways.
- **No heading.** "CPUs · taps-only copy" is gone from the play screen
  (the miniatures and the column; its message `room-play-heading` too).
  The taps-only note stays on the stage's prompt. **The battle's moment**
  (a line no announcer said) moved to a status line under the right
  side's slots (the left's when there is only one opponent), beside
  "Stage won", which moved there too; on the column (a phone held
  upright, or lyrics) it is at the column's top, where the heading was.
- **The sides' whole height.** Each side now keeps clear only of what is
  on it: the left of the watermark and the quit button's place, the
  right of the score and the status line, rather than one band for both
  cut by all of them. Both sides' blocks stay on one middle where their
  rooms allow (symmetric for eight), and move into their own room
  where they need it. More fit, and larger: 32 CPUs at 1280×720 all in
  miniature (29 before), 128 at 1280×720 in 40 slots (29), at 1920×1080
  in 96 slots (79); eight at 1280×720 and 1920×1080 keep their lanes
  (32 and 44 px, the size bound).
- **Turns where not everyone fits** (`opponents::rotate`,
  `ROTATION_V0`, PROVISIONAL: eight seconds a turn, a change darkening
  the changing slots for 0.4 s, at once under reduced motion). Each free
  slot goes round its own seats (the `k`-th slot the `k`-th, `k + free`-th,
  ... of the seats not pinned), so an opponent always shows in the same
  place and a slot with one seat never changes; the "+N more" line is
  gone from the miniatures (the column keeps its count). The turn follows
  the song's clock. **Priority slots:** `opponents::priority_seats` names
  the seats that never rotate away and take the first slots (none yet;
  later the player's attacker and target); tested with a stub. Where
  everyone fits, nothing moves, as before ("never move" holds there).
  Where a slot is too narrow for a name and its level, the level goes
  (the seat's number in "CPU 117" shows through a turn). The web's
  `__DDI_DEBUG.opponent_fields` says `rotating`, the status line and each
  slot's seat (its `more` count went).
- Tests: `ddi-shell` (eight slots at three sizes; the sides using their
  own height; 16 to 128 opponents taking turns: every slot filled, every
  seat shown within its turns, each always in the same slot; pinned seats
  staying, the change's darkening only on changing slots and never under
  reduced motion; the status line, the column's moment), the desktop
  offscreen scenarios (pictures for review; costs below).

Measured costs, desktop offscreen (`opponent_fields_offscreen`,
`--ignored`; ms, mean (p95); "record" the frame body's CPU time, "with
GPU" the frame waited for on the GPU):

| Desktop | Slots | Miniatures: record / with GPU | Column: record / with GPU |
| --- | --- | --- | --- |
| 8 CPUs, 1280×720, hand behind the field | 8 | 1.36 / 3.60 (2.68 / 5.20) | 1.21 / 2.97 (2.66 / 4.95) |
| 8 CPUs, 1920×1080, hand behind the field | 8 | 1.39 / 3.81 (2.75 / 6.05) | 1.21 / 2.99 (2.66 / 4.85) |
| 8 CPUs, 844×390, hand behind the field | 8 | 1.28 / 3.42 (2.61 / 5.06) | 1.14 / 2.87 (2.55 / 4.66) |
| 32 CPUs, 1280×720 (development) | 32 | 0.43 / 2.77 (0.63 / 3.80) | 0.18 / 1.09 (0.35 / 1.34) |
| 64 CPUs, 1920×1080 (development) | 64 | 0.63 / 4.07 (0.90 / 5.08) | 0.20 / 1.17 (0.36 / 1.49) |
| 128 CPUs, 1280×720 (development) | 40, turns | 0.52 / 3.03 (0.75 / 4.02) | 0.19 / 1.07 (0.30 / 1.30) |
| 128 CPUs, 1920×1080 (development) | 96, turns | 0.84 / 5.27 (1.13 / 7.10) | 0.21 / 1.18 (0.35 / 1.45) |

The larger hand costs nothing measurable over M6b's (the same picture,
framed closer). 96 miniatures cost about 0.1 ms of GPU time more than
M6b's 79.

## M7b: the match flow as the maintainer confirmed it (implemented, 2026-11-11)

The room screen of M2 to M7a is gone: song select is the room. Both
shells, CPU practice and private online rooms.

- **Rules first.** Entering practice or a room opens the Rules pop-up by
  itself (`RulesDialog::first`): Reset (the defaults, the pop-up stays)
  and OK (applies, closes); Back closes it without applying. It does not
  open by itself again until the player leaves (Back to the home) and
  comes back (`RoomClient::rules_seen`). From the header's Rules later:
  Cancel and Apply, as before. A member who does not operate the room
  sees it read only, with OK (or Close) alone.
- **Song select is the room's main view** (`SelectScreen::with_room`,
  `screens/select_room.rs`). The host (an operator) sees their own list;
  Enter ("Pick for stage N") is `Command::Pick`: the song becomes the
  next (or first) stage's, the match starts (from the lobby, or after a
  match's results: the next match, the finished one recorded) or goes on
  (between stages), and the stage's readiness window opens, as one change
  refused whole (`NotEnoughReady` for a field that could never start,
  `WindowOpen` while one gathers, a stale base). No lobby Start match, no
  lobby Ready, no pre-picking; the stage count still comes from Rules.
  Nothing is added, edited, managed or shared from the room's song
  select; its place is not singleplayer's.
- **After a stage**: its results (Back to room), then the standings pop-up
  over song select (`StandingsDialog`: the player's result line, every
  place, each stage's points, the total; Continue), then song select for
  the next pick. After the last stage the match's results the same way,
  then song select picks the next match's first song.
- **What the others see** (`following`): song select's song side shows the
  host's resting song (the spotlight) and its chart; the list's place
  holds the room's panel: the status, the match's picks (each stage's
  song, its chart and the player's result) and the players, scrolling
  with Up and Down, the wheel or its ▲/▼ marks. They cannot pick (Enter
  does nothing, no Enter hint). The host's song is told once it rested
  `SPOTLIGHT_REST_MS` (3 s, PROVISIONAL): a `ContentCard` (title, artist,
  difficulty, meter, style) shows at once; online, the whole playable
  song is uploaded under a grant for "stage 0" (`SPOTLIGHT`) and the
  others read the room's share of it (`LoadJob` on stage 0,
  `RoomClient::spotlight`), which gives its banner and its preview (the
  web's preview finds shared songs, `share::entry`; desktop registers
  them as for stages). Moving through the list sends nothing. The pick
  of that chart then reuses the share in the reducer
  (`RoomState::known_share`, by chart digest, as the server's reuse is):
  available at once, nothing uploaded again; a pick before the upload
  finished waits for it (`SetSpotlightShare` makes the waiting stage
  available too) or uploads it itself. In practice nobody follows: no
  spotlight is sent.
- **Readiness on the stage's prompt** (`PromptScreen::with_room`,
  `screens/prompt_room.rs`): the prompt opens for everyone the moment
  the host picks (for a member once their device has the song). Its
  Start is Ready (again: Cancel ready); its countdown counts down to
  Ready where the shell asks for one (as auto-start: on the web only on a
  context song select handed over running, the host's pick press or the
  follower's song select, never made outside a gesture; on desktop
  always); any other press cancels it. Beside the controls' diagram the
  players and whether each is ready (CPUs say CPU), scrolling past four
  rows. Under the song the status: "Stage n starts once everyone is
  ready · s". The stage starts for everyone as soon as every connected
  player who could still join is ready and loaded, or at the deadline;
  the prompt then presses Start itself (`PromptChoice::Start` outside a
  gesture: the Ready press made the web's audio context,
  `PromptChoice::Ready`, the music decoded on it ahead). Whoever is not
  ready then sits the stage out: the prompt says so for 2.5 s and goes
  back to song select, which says "You're sitting out stage n; the others
  are playing it." Back on the prompt before the start withdraws
  (`RoomClient::leave_prompt`: not ready; the host's Back closes the
  window, back to picking), never a forfeit; once the stage is on it is
  a forfeit, as before.
- **The model** (`ddi_room`, protocol 2, still taking 1):
  `Command::Pick`, `Command::Spotlight`, `Event::SpotlightChanged`,
  `Command::SetSpotlightShare` (the authority's, by chart),
  `StageContent::card`, `RoomState::spotlight`; `hosted-deadline-v1`: the
  window's deadline counts from when its song is available to everyone
  (a window whose song is still being shared waits past it), and it ends
  early once everyone connected who could still join is ready and loaded
  (v0 never shortened it, the study's proposal; the maintainer's flow
  supersedes it). A disconnected member does not hold the early start.
  The server (`../ddi-server`) runs it: grants for stage 0 and its share
  taken by the room (its README, "Room shares").
- **Status line**: one line at the top of the list's column, under the
  header (the host's, above the list; the others', the panel's first
  line). Chosen because it sits beside what it talks about (the pick and
  the room's song), is read where the eye goes after the header, and is
  never covered by the footer's hints or the play's own prompt, where
  M2's bottom-left line was.
- **CPU roster** (`state/cpu_roster.rs`, `cpu_roster.json`): 24 named CPUs
  of our own, each a personality (`ddi_engine::cpu::Skills` preset or
  seven multipliers) and a level; practice starts with three drawn from
  the room's seed (`CpuMode::Roster`, each seat its own `OpponentSpec`
  with `name` and `personality`). In Rules: each CPU removable, Add random
  (one not seated, `RosterAim::Any`, the hook for aiming at a balanced
  level later) and Add custom (`AddCpuDialog`: by level, several at once,
  or one by name). The stage plays each with its personality
  (`Opponent::with_skills`) and shows its name. The 9-player cap holds
  (Add is disabled at eight CPUs); the development switch `mpcpus` keeps
  its fixed count of unnamed CPUs. Online rooms seat no CPUs yet.
- **Where the room's other parts went**: Rules and Players in the header
  (in More on a phone), Players a pop-up of the panel (the picks and
  every player with readiness, connection, role, level and personality,
  scrolling); More keeps invite, stop, retry sharing, sign in again and
  Leave room. The host keeps the list's full width for picking, and the
  others' list place is free for the panel, which is why the players
  live there for them and one press away for the host.
- Tests: `ddi-room` walkthroughs (a pick starting the match and the early
  start, sitting out at the deadline, the next match after results; a
  pick refused whole; the spotlight shared once and reused, a pending
  song holding the window; the roster's seats and limits), `ddi-ui`
  (the client's flow, leaving the prompt, the roster; the online
  spotlight ahead of the pick and the reuse, a pending song; the screens
  in the harness: Rules first, the host's pick and a refused chart, the
  roster pop-ups, the prompt's Ready and the room's start, sitting out,
  standings and match results, phone sizes, the host and the follower
  online; the `qps-ploc` walk), `ddi-session` (stages from picks),
  `ddi-desktop` (the room flow offscreen, `--test online` against a local
  server), the server's native tests (the spotlight's grant and share).
  At runtime: practice in Chrome and Firefox, and three browsers online
  (`target/scratch/mp/m7b/`), screenshots reviewed.

Development-only or mocked in M7b: the development identity provider for
online runs; `mpcpus`; online rooms seat no CPUs (Rules shows no roster
there).

Found on the way: the server's room sent its departure notice to the
socket it had just closed, which throws past `send_with_str`'s result in
workers-rs and aborted the handler, the others' events lost (clients then
stale); it now skips that socket (`../ddi-server`, `broadcast_except`).
The room's screens look at the room ten times a second while nothing
moves (`wake_at`), as M2's room screen did; headless Firefox still
stopped a still page's frames at times in the three-browser run (the page
caught up at its next input), so the run nudges that page's mouse. The
stage prompt's debug state (`__DDI_DEBUG.ui.theme.room`) now follows the
room as its card does; it had stayed as the prompt opened, and a test
reading it pressed Ready again after the automatic Ready, canceling it.

## M7d: bots for testing rooms (implemented, 2026-11-11)

A DEVELOPMENT TOOL: simulated players that fill a private online room
for testing, driven by the game's CPU player. Against the deployed server
or a local one; never part of the game's menus.

- **A bot is its own kind of player**: identity `bot:<slug>`
  (`ddi_room::Identity::bot`, `is_bot`; `Member::is_bot`), its slug its
  name's letters and digits in lower case (`Lumen Kite` is
  `bot:lumen-kite`). Only the server makes one (`../ddi-server`, its
  README, "Bots": `POST /auth/bots {name}` for an **admin** share token,
  a line of `SHARE_TOKENS` without the `upload` role; the local server
  the same with its development tokens, `test-token`, whatever
  `DEV_FAKE_AUTH` says); a Twitch or development identity can never be
  one, and a client cannot claim one. A bot is never an administrator:
  the server's session and admin check drop it, and the room's reducer
  drops the flag on a bot's join too. Its session lasts twelve hours and
  is never re-signed. Limits: 60 sessions an hour per admin token, 8 bots
  in a room (`409 too_many_bots`).
- **Labeled Bot** wherever players are listed (`room-bot-name`, "{ $name }
  (Bot)", ja "{ $name }（ボット）"; `screens::room::person_name`,
  `shown_name`): the room's panel and Players, the stage prompt's rows
  (both `player_rows`), the standings and match results, and an invite's
  host (`Preview::host_bot`, sent by the server). The miniature fields
  show no online players yet (M6), so there is nothing to label there.
  The debug state's members carry `bot`.
- **The tool**: `ddi --mp-bots …`, a mode of the desktop binary
  (`crates/desktop/src/bots.rs`), headless (no window, no sound device),
  before anything of the shell starts. Chosen over a crate of its own
  because a bot is the game's own room client: desktop's HTTP and socket
  (`crate::online`), its sharing under a room's grant and reading of a
  room's share into the cache (`crate::share`, `crate::room`'s threads,
  each bot with an inbox of its own, `room::news_inbox`), its library and
  chart reading, all reused rather than copied; a separate binary would
  have depended on the whole desktop crate anyway. Each bot drives a
  `RoomClient` as the screens drive it, so it follows the protocol (2)
  exactly as the game does: Rules (the host saves them; a guest has read
  them), the host's resting song (the spotlight, shared ahead), the pick,
  the stage's song read from the room's share and checked for the CPU (a
  chart it cannot plan fails to load: it sits that stage out), Ready on
  the stage's prompt after a person's moment (0.7 to 2.5 s), the stage
  played, its run reported (`report_run`: life and accuracy, or out at a
  song second; self-reported as everyone's is), the standings, the next
  stage and match. Ctrl-C forfeits a stage being played and leaves the
  room (each bot's `Leave`, waited for at most 4 s); a second Ctrl-C ends
  at once.
- **The play**: `ddi_engine::cpu::Opponent` (the chart's tap-only copy,
  a level relative to the chart, a personality) under the default
  settings' ruleset for the chart, fail immediate, judged on a real-time
  song clock from the stage's start (a 2 s lead-in), reported once out or
  2 s after its last note. Each bot's name, level and personality come
  from the CPU roster (`cpu_roster.json`) in an order drawn from the
  run's seed, or from `--names`, `--level`, `--personality`. It is not a
  skill-tuned player and plays nothing but taps.
- **The host bot** (`--host`): makes a room, prints its invite link,
  saves its Rules (`--stages N`), waits for players (`--wait-for N`,
  default 2), rests on a song (`--rest S`, default 8 s; the others see
  and hear it from 3 s on) and picks it, waits 15 s after each stage's
  results for the standings, and plays too. Its songs: the bundled
  songs and the settings' folders, or `--songs DIR` alone, or a fixed
  list (`--pick ID[:CHART],…`), each one original chart one player plays
  on four lanes with a digest (closest to meter 7 unless named), in an
  order drawn from the seed. Guest bots (`--guests N`) join the host
  bot's room or `--join LINK|INVITE`'s. `--matches N` leaves after N
  matches. The log: one terse line a moment per bot (signed in, the room,
  connected, Rules, following the host's song, loading, ready, the stage
  started and its run, the standings, leaving).
- **Running it.** Locally: the server's `just dev` (or `cargo xtask
  worker-dev`) and the app's `just dev`, then
  `DDI_HOME=$PWD/target/scratch/bots/home DDI_SHARE_TOKEN=test-token
  cargo run -- --mp-bots --host --guests 3 --api
  http://127.0.0.1:8787/ditoditoinfinito/api --site http://localhost:8080/`
  prints an invite to open in a browser; `--join <link> --guests 3`
  instead fills a room someone made in the browser. Against the deployed
  server: `DDI_SHARE_TOKEN=<an admin token of SHARE_TOKENS> ddi --mp-bots
  --host --guests 3` (the API and the site default to production's). The
  token comes from `DDI_SHARE_TOKEN`, else the settings' share token;
  never put it on the command line.
- Found on the way: the room's Durable Object read a request's body after
  loading the room, and that await let a second request in: two joins at
  once each saved over the other's room, one bot then refused its ticket
  as not a member. It now reads the body first (`../ddi-server`,
  `room_do.rs`). Desktop's share download named its temporary file by
  the process alone, which two bots of one run fetching the same file
  shared; it is now unique to the download too.
- Tests: `ddi-room` (a bot's identity, never an admin); `ddi-ui` (the
  labels in the debug state, `player_rows`, the standings' name and the
  invite's host); `ddi-desktop` (the flags, profiles from the roster or
  the flags, a stage played frame by frame to its run, the host's chart
  choice); the server natively (slugs and names, the admin token check:
  missing, wrong and an uploader's refused; the session never an
  admin's, never refreshed; the hourly cap; a bot host's preview, bots
  never admins in a room, the room's cap) and against `wrangler dev`
  (the refusals and `/me`'s admin flag false for a bot listed in
  `ADMIN_TWITCH_IDS`). At runtime against a local `worker-dev`: a host
  bot and three guest bots playing a two-stage match; headless Chrome as
  a guest in a host bot's room (the invite's host labeled, the bots
  labeled, the host's resting song followed, both stages played, the same
  standings as the bots logged); headless Chrome as the host with three
  guest bots joining by its invite (`target/scratch/mp/m7d/`).

DEVELOPMENT-ONLY: the whole tool. Its bots' runs are self-reported like
everyone's, and they play the tap-only copy at the engine's levels;
nothing about them is balance validation.

## M8a: the match's ruleset, attack power shown, battle shocks in a play (implemented, 2026-11-11)

The first round of battle attacks, under the maintainer's decisions of
2026-11-11 (D0–D8, recorded in `docs/plans/multiplayer-ui.md`, "Decided
(maintainer, 2026-11-11): attacks"). Nothing is launched yet: no
targeting, commitments, CPUs receiving shocks, settlement or announcer
lines (later rounds).

- **The ruleset is a match rule** (real, D0): `MatchRules::ruleset`, one
  of `MatchRules::RULESETS` (ITG, ITG FA+, SM5, DDR A, DDR X, DDR
  Extreme, FFR: the four-lane keyboard presets the CPUs play, the
  engine's ids), `itg-fa+` for new rooms and for rules kept before it
  existed (serde default; protocol unchanged, an older client ignores
  the field). Validated (`RulesProblem::UnknownRuleset`). A Rules row
  (‹ ›, the name as Options writes it; "Everyone plays the match under
  its ruleset, whatever their own options say"), the locked line, the
  stage's prompt ("Ruleset: ITG FA+ (Simply Love)"). The stage's play,
  its CPUs, a forfeit's CPUs and the bots all play
  `ddi_session::battle::stage_ruleset(id, chart)`: the preset as it is
  (no personal judge or life level, fail type or holds), fail immediate;
  the player's other options are unchanged. Singleplayer is untouched.
  Tests: `ddi-room` rules, `room_tests` (the row, the locked line),
  `stage.rs` `a_stage_is_played_under_the_match_s_ruleset_not_the_player_s`;
  the M3 stage tests that need CPUs to go out give the stage a one-life
  ruleset through the tests-only `SessionConfig::stage_rules`.
- **Points as decimals** (real): the standings, marks and rows show
  `screens::room::points_text` ("1.5", "9.33", the language's decimal
  mark; `room-points-value`), the model keeping exact fractions.
- **Attack power** (development only: `--mp-dev-attacks`, the web's
  `mpattacks=1` beside `mp=1`, practice under
  `BattlePreset::GaugeAttacksV0`, "Survival with power · development" in
  Rules): `ddi_room::attacks::StageBattle`, whole numbers keyed by
  participant, credits each competitor at checkpoints every four beats
  (from beat 4 to four beats past the last note) with what its tap
  judgments before the checkpoint earned (`POWER_CLASSES_V0`), up to the
  cap (overflow lost, `Charged` when it fills); nothing spends it yet.
  `ddi_session::battle::attacks::AttackRunner` feeds it every
  competitor's judge events (their exact instants) and runs a
  checkpoint once every competitor still in has been judged past it, so
  the ledger is the same at 60 and 144 frames a second and again from
  the player's log and the CPUs' seeds (test
  `attack_power_is_the_same_at_any_frame_rate_and_from_the_log`).
  Online stages run none (the room would). The bar
  (`ddi_render::BattleBar`): directly above life on the field and on
  every miniature, as wide, half as thick, 5 units of the play's 72-px
  lane apart, filling from the left in electric violet, no number; full,
  a restrained electric edge running along it (a steady glow under
  reduced motion); above life under Reverse too (life does not move);
  hidden once out or after a confirmed win. The column draws a thin
  violet line over each life bar. `__DDI_DEBUG.battle.attacks`: power
  per seat, checkpoints run, the last events.
- **Battle shocks in the player's play** (the engine's mechanics real
  and tested; reached only through a development trigger,
  `--mp-dev-mines BEAT` / `mpdevmines=BEAT`, which puts the first safe
  section from that beat into each practice stage's play):
  `ddi_engine::battle`. A `HazardJudge` beside the chart's judge (never
  inserted into the chart: note indices stay), its events and sprites
  numbered from `HAZARD_INDEX_BASE`: a press in a shock's window steps on
  it, a lane held across it steps on it, one passed is avoided; exact
  instants, the same however time is cut. `Player::add_hazards` (each
  before its window, after those already in). A hit costs exactly the
  ruleset's miss to the gauge (a `Tap(Miss)` given to the gauge alone),
  never the score, the counts, the timing or the timeline; the combo
  breaks only where the ruleset's mines do (`mine_breaks`: DDR A, DDR
  X); a press that steps on one is not also the chart's Boo; FFR's
  window 70 ms (`INJECTED_MINE_V0`). `Results::battle_mines` (hit,
  avoided; absent from every single play's record). `PlayLog` version 4
  keeps them (`LoggedHazard`; versions 1–3 still read, and written as
  before when there are none); a replay puts them in at its start.
  Drawn as the player's skin's mine (`SpriteKind::Mine`; Infinito's
  spiked mine or its ShockArrows, Peters-DDR's layered mine), hidden by
  Hidden and Sudden as notes are, upside down under Reverse; a hit is the
  skin's mine explosion. `ddi_engine::battle::schedule`
  (`PLACEMENT_V0`): one section a checkpoint, eight shocks over eight
  beats on the eighths (sixteenths where the eighths have too few safe
  places), one a beat where possible, at most two, one a row, each clear
  of every step in its lane by the shock's window plus the widest judged
  window plus 50 ms, of hold bodies across its window, of a tail in the
  150 ms before and of a lift in the 0.5 s after; a section without
  eight safe places is none, never filled with unsafe ones; pure in the
  chart, the judge table and the seed. `__DDI_DEBUG.hazards`.
  The bundled four-lane charts' share of sections with room
  (`cargo test -p ddi-engine --test battle_shocks bundled -- --nocapture`;
  under ITG FA+ / DDR A / FFR): 100% on every chart but Into My Dream's
  Challenge 15 (79 of 83, 95%) and Some Things Must's Challenge 16
  (87 of 92 under ITG FA+ and FFR, 92 of 92 under DDR A).
- Tests: `ddi-room` `attacks` (classes, crediting, cap and overflow,
  elimination, rising checkpoints); `ddi-engine` `battle` and
  `tests/battle_shocks.rs` (a shock's cost equal to a miss from the same
  life under every match ruleset, score/counts/timeline untouched, combo
  per D3, no Boo double count, FFR's 70 ms, the same at 1/60, 1/144 and
  0.7 s steps, version 4 written, read and replayed equal, sprites until
  stepped on, late refusal, placement properties and shares over the
  bundled charts); `ddi-render` `the_power_bar_is_directly_above_life`
  (and the HUD shader for WebGL2); `ddi-session` stage tests.
- Screenshots: `target/scratch/mp/m8a/shots/` (`s0-*` the ruleset row and
  prompt, Chrome, Firefox, a phone in Japanese; `s1-*` the bar filling
  and charged at 1280×720 and 1920×1080, a phone held sideways, the
  column on a phone held upright, reduced motion, Firefox; `s2-*` shocks
  with Infinito, its ShockArrows, Peters-DDR, Reverse and Hidden, Chrome
  and Firefox).

## M8b: attacks launched, CPUs shocked, settlement (in progress, 2026-11-11)

The second round of battle attacks, still behind the development switch
(`--mp-dev-attacks`, the web's `mpattacks=1` beside `mp=1`).

- **CPUs receive shocks** (real mechanics; reached in this commit only by
  the development trigger `--mp-dev-mines BEAT` / `mpdevmines=BEAT`, which
  now puts the section into every CPU's play too):
  `ddi_engine::cpu::Opponent::receive_hazards` puts them into the CPU's
  own player (judged as anyone's: a press in a shock's window or a lane
  held across it steps on it, the ruleset's miss to its gauge) and
  rewrites only its presses still to come: a shock it steps on is an
  extra 12 ms press within 60% of the shock's window; its own presses
  within 0.3 s of a shock move by up to its jitter or are left out
  (slip); every draw keyed by the CPU's seed, the shock (tick, lane) and
  the press, never by the frames; presses already fed or due before its
  judged time are never changed; a script that would not validate is
  kept as it was. By level alone (D8): one *avoidance* value a CPU,
  `MINE_RESPONSE_V0.avoidance(level)` (levels 1–2: 0, 3–4: 0.5, 5: 1),
  its response read from it (`MineResponseTable::response`), shaped to
  become an eighth `Skills` axis later. `MineResponse::OFF` is the
  study's control, for tests only.
- **Miniatures show them**: `feed::Moment::kind` (`MomentKind::Tap`, or
  `ShockHit`); `FieldFeed::add_hazards` takes an attack's shocks at its
  commitment, drawn as the skin's mines (`SpriteKind::Mine`, numbered
  from `HAZARD_INDEX_BASE`) from then on; a hit takes the shock off and
  flashes its receptor with the skin's mine explosion
  (`ReceptorState::mine`, `NoteskinPlay::miniature`); avoided ones scroll
  on past the receptors and fade.
- Tests: `ddi-engine` `cpu::mines` (the table, keyed draws),
  `cpu::opponent` (judged and the same at 60/144 frames a second, the
  control unchanged, a poor avoider steps on more than a good one over
  24 seeds, late shocks refused and nothing changed), `battle`
  (`reading_horizon`); `ddi-session` feed (`shocks_show_from_their_commitment_and_a_hit_takes_one_off`),
  stage (`cpus_receive_shocks_judged_alike_at_any_frame_rate`).

## Provisional choices

| Choice | Where | User-visible effect | Decision needed |
| --- | --- | --- | --- |
| Bots: 12 h sessions, never re-signed; 60 sessions an hour per admin token; 8 bots in a room | `../ddi-server` `bots.rs` | A long bot run stops after 12 h (signed out); a ninth bot is refused | The limits for bots on the deployed server |
| Bot tool pacing: Ready after 0.7–2.5 s, host rests 8 s on a song, 15 s after each stage's results, waits for 2 players | `crates/desktop/src/bots.rs` | How quickly a room of bots moves | None (a test tool), unless it should mimic people more closely |
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
| CPU shock response `mine-response-v0` (uncalibrated): avoidance from level (1–2: 0, 3–4: 0.5, 5: 1); at 0 a CPU steps on 14% of shocks, its presses within 0.3 s of one moved ±45 ms and 4% left out; at 0.5 6%, ±25 ms, 1.5%; at 1 2%, ±12 ms, 0.5% | `ddi_engine::cpu::MINE_RESPONSE_V0` | How often a CPU's life drops to an attack, and how much its timing suffers near one | Calibration against people; whether avoidance joins `Skills` |
| Battle `gauge-survival-v0` (`ddi_room::battle::BattlePreset::GaugeSurvivalV0`, practice's `MatchRules::battle`): life is each competitor's gauge under the match's ruleset (M8a; before, the player's own), empty is out of the stage | `crates/room/src/battle.rs`, `crates/session/src/battle.rs` | A stage is survival under the match's ruleset's gauge; Rules says "Survival · provisional" | The standard battle (life, power) and its numbers |
| Ranking: still in by life, then accuracy (the ruleset's percent, money or EX over its most); then the out, later first; then forfeits; exact ties share | `ddi_room::battle::rank` | Placements follow life first; the CPUs' accuracy is on the taps-only copy, the player's on the full chart (not like for like) | Ranking survivors; whether CPUs should play the full chart |
| Decided once at most one competitor is still in; the last out together share first; the winner's fail is turned off to the song's end | `battle::decided`, `PlaySession::tick` | "Stage won" over the column; the player finishes the song whatever the gauge does after | Final-contender settlement once attacks exist |
| The player out: the play ends as a failed play does, the remaining CPUs played to their end at once | `Battle::outcome` | No watching after elimination; the results come after the fail's hold | The spectator view and when the eliminated return to the room |
| A quit, an interruption (page hidden, audio stopped) or Back on a stage's prompt is a forfeit; a song unreadable at play time stops the match (`Interruption::ContentLost`) | `PlaySession::tick`, shells | Losing focus mid-stage costs the stage | Local interruption versus forfeit; reconnection |
| A stage plays with no turns, note removals or rate, the play options row not offered, no ghost, no singleplayer countdown | `SessionConfig::player_spec`, prompt | The player's other settings (speed, skin) apply; the rules are the match's (M8a) | Which personal options a stage allows |
| Stage plays are not kept in the play history or bests | both shells' record paths | A practice match leaves no history | Whether a stage counts as a personal play |
| Battle voice `battle-voice-v0`: priorities outcomes/out 40, (attacks 30, none yet), life critical 20, players left 10, openings and the finished song 5; one battle line at a time, a lesser or equal one passed over, never queued | `ddi_session::announcer::battle::BATTLE_VOICE_V0` | Which battle line speaks when two meet | The battle voice's priorities and whether a line may cut another (needs a voice bus that can stop one sound) |
| A stage leaves out a play's routine lines (combos, comments, life lost, last life, full combos, cleared, failed, grades, records, cheer) | `battle::stage_speaks`, `Announcer::tick` in a stage | A pack without battle lines says only ready, here we go, failed (out) and cleared (finished after a win) in a stage | Whether some routine lines (combo milestones) belong in a battle voice preset |
| A moment shows only where its own line was not said; for 2.5 s with a 0.25 s fade; in the status line under the right side's miniatures (M7c; at the column's top on the column), nothing in bars-only columns or with no column | `VoicePolicy::cue_seconds`, `cue_fade`, `ddi_shell::opponents` | A phone's narrow column has no cue | Whether cues show always, and where on a phone |
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
| Miniatures: lane at most 45% of the play's and 44 px, at least 16 px (else as many as fit at 16 px and a count, else the column); seven lanes tall, receptors a lane and a half from their end; 10 px between slots, 20 px from the field's band | `ddi_shell::opponents` constants | Eight miniatures at 1280×720 have 32 px lanes (2×2 each side), 44 px at 1920×1080, 16 px on a phone held sideways; a phone held upright shows the column | The miniatures' size, density and placement |
| First half of the seats on the left, the rest on the right, each side its own height; in a stage the dancing hand is behind the field, from below the judgment to the bottom (the top under Reverse), `BEHIND_FIELD_ZOOM` 1.9 against the band's width, its top quarter fading in | `opponents::arrange`, `play_view::behind_field`, `dance::BEHIND_FIELD_ZOOM` | The hand is large under the judgment, the combo over its wrist, darkened by the band | The hand's size, its overlap with the combo, its dimming |
| Every miniature at the local player's speed and direction | `PlayView::draw` | Opponents' fields read like the player's own, not like theirs | Their own display settings once a remote stream carries them |
| A judgment shows 0.4 s; a skin's explosion components drawn at 0.4 opacity on a miniature; a skin without them gets a soft glow of the judgment's color | `feed::FLASH_SECONDS`, `noteskin::MINIATURE_HIT_ALPHA`, `opponents::draw_fields` | Infinito's W1 burst is a faint flash on a miniature; Peters-DDR's miniatures glow instead of exploding | The miniature's hit effect |
| An actor-drawn receptor is drawn as it rested a second into the play | `NoteskinPlay::rest_receptors` | StepMania skins' miniature receptors do not press | None expected (no Lua per miniature) |
| Life, combo and moments drawn after a presentation delay, 0 for local CPUs | `feed::LOCAL_DELAY`, `FieldFeed::new` | None locally | The online delay and how it is corrected |
| A miniature's combo never below 0.3 of the play's scale | `hud::MINI_COMBO_MIN_SCALE` | Below 22 px lanes the combo is larger than its field's scale | Legibility against faithfulness at small sizes |
| Nine players: `ExperimentalNineV0`, the eight-player table and 0 for ninth | `ddi_room::awards` | A nine-player stage's ninth earns nothing, the first eight as in an eight-player field; marked experimental | The award rule for nine (and whether ninth should earn something) |
| Development only: practice beyond the cap (129), linear points for any field; past what fits, the opponents take turns in the slots, `ROTATION_V0` (8 s a turn, 0.4 s darkening, none under reduced motion), each always in its own slot, priority seats never rotating (none yet) | `RoomPolicy::development`, `AwardScheme::DevelopmentV0`, `opponents::rotate`, `opponents::priority_seats` | Only through `mpcpus=` / `--mp-dev-cpus` | The large-party density policy |
| With lyrics shown the column is drawn instead | `PlayView::draw` | A work with lyrics shows the column | Where miniatures go around lyrics |
| Spotlight rest `spotlight-rest-v0`: 3 s before the host's resting song is told to the room (and, online, uploaded) | `state::room::SPOTLIGHT_REST_MS` | Guests see a song three seconds after the host stops on it; scrolling sends nothing | The rest's length; whether to upload on rest or only show the card |
| Readiness `hosted-deadline-v1`: 30 s from the song's availability (the server's `ROOM_READINESS_WINDOW_MS`, 15 s in its `wrangler.toml`, overrides), early start once every connected candidate is ready and loaded | `rules::ReadinessPolicy` | A stage starts when the last person readies; a disconnected one waits for the deadline | The window's length; whether a reconnecting member should hold the early start |
| The prompt goes back 2.5 s after the room lets go of it (sat out, canceled) | `prompt_room.rs` `GONE_SECONDS` | The line is readable, then song select | None expected |
| Add random draws any unseated catalog CPU (`RosterAim::Any`) | `state::cpu_roster` | A practice field's levels are mixed | The balanced aim (a level near the people's) |
| Practice starts with three CPUs drawn from the room's seed | `state::room::STARTING_CPUS` | Every practice room has a different three | Fixed starters, or the last roster kept |
| A pick's readiness and the countdown: the host's Back on the prompt cancels the window (back to picking); a member's withdraws only themself | `RoomClient::leave_prompt` | The host can change their mind before the start | Whether the host's Back should keep the window for the others |
| Desktop's session in the config folder's `online-session`, owner-only on Unix, plain text | `crates/desktop/src/online.rs` | Signed in across runs until the session's thirty days | The system keychain instead |
| Ruleset as a match rule, `itg-fa+` by default, from `MatchRules::RULESETS`; a stage plays the preset as it is (default judge and life levels, holds by the chart's game, fail immediate) | `ddi_room::MatchRules::ruleset`, `ddi_session::battle::stage_ruleset` | Everyone's stage plays ITG FA+ unless Rules says otherwise; a player's judge or life level, battery or fail type do not apply in a stage | Whether personal StepMania options (judge difficulty) should be match rules too |
| `attacks-v0`: a bar of 100, an attack 100, checkpoints every four beats from beat 4 to four beats past the last note; overflow lost | `ddi_room::attacks::ATTACKS_V0`, `AttackRunner::new` | The bar moves in steps every four beats and fills from about 50 best-window notes | The bar's size and the checkpoint's beat count |
| `power-classes-v0`: the best window 2, the next 1, the rest 0, for every match ruleset (D2) | `ddi_room::attacks::POWER_CLASSES_V0` | Time to a full bar depends on timing, not on the ruleset's names | None (D2), unless play shows the classes too tight under FA+ or DDR A |
| Attack power development only (`gauge-attacks-v0`) | `--mp-dev-attacks`, `mpattacks=1`, `ddi_ui::state::room::Development` | Not shown in ordinary practice | D8's "On by default" in a later round |
| `injected-mine-v0`: a shock costs the ruleset's miss to the gauge, the ruleset's mine window (70 ms under FFR) | `ddi_engine::battle::INJECTED_MINE_V0` | A shock stepped on takes ITG's 10%, SM5's 8%, DDR A's miss | None (D1) |
| `placement-v0`: eight shocks over eight beats from a checkpoint, the eighths then the sixteenths, at most two a beat, one a row, clear by window + widest judged window + 50 ms, no body across, no tail 150 ms before, no lift 0.5 s after | `ddi_engine::battle::PLACEMENT_V0` | Where the shocks fall; a dense chart has fewer sections | The clearances, and whether sixteenths are acceptable |
| Development trigger: the first safe section from a beat in each practice stage, its seed the stage's id | `--mp-dev-mines`, `mpdevmines=`, `ddi_session::battle::dev_shocks` | Only through the switch | Replaced by the attack loop (S4) |

## Open questions

- M7b: the readiness window now ends as soon as everyone is ready (the
  study proposed that it never shortens). Is a disconnected member right
  to hold only the deadline, not the early start?
- M7b: the spotlight uploads the whole song after a 3 s rest. Is that
  too eager on slow links (a host browsing slowly uploads several songs),
  and should the card alone show until the pick?
- M7b: the follower's view keeps song select's song side and puts the
  panel in the list's place; the host reaches the same panel through
  Players. Should the host see the players permanently too (a narrower
  list)?
- M7b: the 24 CPU names in `crates/ui/src/state/cpu_roster.json` are
  invented, for your review.
- M7b: a room's socket is read on the page's frames; a page whose
  frames stop (a hidden tab, headless Firefox at times) falls behind
  until it draws again, and may sit a stage out. Read the socket off the
  frame loop?
- M7b: the server's `ROOM_READINESS_WINDOW_MS` is 15 s in its
  `wrangler.toml` (your file); the model's default is now 30 s, counted
  from the song's availability. Raise it there?
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
- M7a: the home lists Find match and Browse rooms as Coming soon and
  shows Private room in the design's Create room place (invite-only, no
  public visibility). Should Create room become its own entry once public
  rooms exist, Private room then its invite-only option?
- M7a: the bars change their content at the swap with no animation;
  should the title or the hints cross-fade later?
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
- M6: the miniatures take the local player's speed and Reverse. Should
  they show each opponent's own (the design's "preserve their allowed
  display settings") once remote players send them?
- M6: a skin whose explosions are actors (Peters-DDR) gets a colored glow
  on the miniature's receptor instead; is that acceptable, or should the
  miniatures always use one effect of our own, whatever the skin?
- M6: Out dims the field and says "Out" twice (its line and its field);
  is one enough?
- M6: at 1920×1080 the miniatures stop growing at 44 px lanes; should they
  grow with the screen, or keep a fixed share of it?
- M6b: the hand behind the field was small. M7c made it as large as
  the band allows under the judgment (the combo over its wrist, its arm
  fading in at the top): is the combo's overlap acceptable, and should it
  be dimmed further?
- M6b: at the smallest lanes the combo is larger than its field's scale
  (legible rather than faithful): acceptable, or hide it there?
- M6b: what should nine players' ninth place earn (0 now)?
- M6b: for large parties, which miniatures? M7c: turns of eight
  seconds in the slots, each opponent in its own slot, priority seats
  (none yet) kept. Is eight seconds calm enough, should Out opponents
  give up their turns, and which seats are priority once attacks exist
  (the attacker and the target; the followed player?)
- M7c: the battle's moment and "Stage won" now share a status line under
  the right side's miniatures; is that noticeable enough, or should a
  moment show over the field?
- M5c: the sign-in prompt was the home's page itself. Answered in M7a:
  a dialog naming Twitch and what it is for, opened where a sign-in is
  needed.
- M5c: who may copy the invite: the server gives it to operators
  (owner, co-hosts, admins); should members be able to share it?
- M5c: an admin who is not a member enters watching from a ticket; the
  client has no admin-specific entry point beyond a link.
- M5c: the room's countdown seconds briefly replay part of the screen's
  entrance on each rebuild (seen in practice too: a short dim flash each
  second in headless screenshots); not changed here.
- M7d: a bot's room is named "<bot>'s room" by the server's default;
  should a bot host's room say it is a test room? Should bots be allowed
  on the deployed server at all outside the maintainer's tests (today:
  anyone holding an admin token)?
- M7d: bots played the default ruleset whatever the room's people
  used. Answered in M8a: the ruleset is a match rule (D0) and bots play
  it.
- M8a: a stage plays the match ruleset's preset as it is, so a
  player's judge difficulty, life difficulty, battery or fail type no
  longer apply in a stage. Should any of them become match rules?
- M8a: power is credited at checkpoints every four beats, so the bar
  moves in steps; should it show earned power between checkpoints
  (provisionally, the ledger still deciding at checkpoints)?
- M8a: placement leaves room in almost every section of the bundled
  charts (95–100%); is the clearance (window + widest window + 50 ms)
  generous enough for dense charts, or too strict once play-tested?
- M8a: a hit's explosion is the skin's mine explosion; should battle
  shocks have a look of their own beside authored mines?
