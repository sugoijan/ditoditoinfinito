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

## M8b: attacks launched, CPUs shocked, settlement (implemented, 2026-11-11)

The second round of battle attacks, still behind the development switch
(`--mp-dev-attacks`, the web's `mpattacks=1` beside `mp=1`): with it,
practice stages launch attacks; without it, nothing changes (the
survival battle has no ledger). Not in this round: who-attacks-whom
curves, priority seats, the targeting choice (before the stage and in
play), attacks on by default, announcer lines, the results' battle
detail (round 3). The dancing hand is not fed shocks (D7).

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
- **The attack loop** (real mechanics, development switch only):
  `ddi_room::attacks::StageBattle::checkpoint` in a fixed order: (1)
  eliminations, (2) power by timing and life as reported (the life is
  read from each play's record at the checkpoint's instant, so frames do
  not matter), (3) attacks whose last shock passed clear, and a cleared
  one's recipient recovers after eight of its own judgments past the
  clear and four beats, (4) attacks against someone out expire (no
  refund), (5) full bars launch in an order that turns each checkpoint
  (from `k mod n` in participant order), each at the target its policy
  picks among those who can be attacked (in, not itself, no attack
  against them reserved, active or recovering): Spread, the fewest
  attacks admitted this stage; Retaliate, the latest sender to it (as of
  the previous batch) while it can be, else Spread; Leader, the most
  life, then the most timing points earned (`earned`, standing in for
  score), then seeded; ties seeded by (stage seed, checkpoint, sender,
  candidate); judged on the state before the batch; (6) the first safe
  section starting no sooner than the checkpoint plus the target's lead
  and the next checkpoint (four beats), within 16 beats of that start,
  else `Blocked { NoEligible | NoSafeSection }` (told when it starts
  waiting or its reason changes; it tries again each checkpoint). Every
  attack is all eight shocks (D4: no weakening). The ledger's record
  keeps each checkpoint's power and events.
- **Leads per target** (D5, `LEAD_V0`): the target's reading horizon
  (`ddi_engine::battle::reading_horizon`: the longest a note is shown
  before it is judged under the player's speed, scroll action, Hidden or
  Sudden and visible range, over the chart's tempo; lane covers left
  out) plus 1.5 s of warning and 0.1 s of delay, at most 10 s. A CPU's
  lead is reckoned from the local player's horizon
  (`CPU_HORIZON`, `local-player-horizon-v0`): its miniature is drawn
  under the player's scroll, so its shocks come in from the far end. A
  stage with attacks raises an extreme slow speed until the player's
  lead fits under the cap (`ddi_session::battle::raised_speed`), and its
  prompt says "Speed raised so attacks can warn in time".
- **Delivery**: `Battle::advance` runs the due checkpoints after every
  competitor is judged to the frame's time and delivers each commitment:
  into the player's play (`Player::add_hazards`, kept in its log), into
  a CPU's (`Opponent::receive_hazards`) and onto its field
  (`FieldFeed::add_hazards`). One a frame stalled past the lead would
  refuse is counted (`Battle::refused`, the debug state), never forced.
- **Settlement** (D6, `SETTLEMENT_V0`): decided only once at most one
  competitor is still in and no committed attack against anyone still
  in is outstanding (an attack is outstanding until its recipient's play
  is judged past its clear); no Stage won meanwhile (the winner's fail
  stays on). If the last one in goes out while it waits, they share
  first with those out in the same checkpoint interval as the one before
  them, and the results say "Drawn: final attack"
  (`StageOutcome::drawn_by_final_attack`, `room-results-drawn-final-attack`).
- **Played out** (`Battle::play_out`): once the player is out or has
  quit (a forfeit at its judged time), the CPUs and the attacks between
  them are stepped a checkpoint at a time to the song's end and settled
  as they come, replacing the per-CPU `finish()` in the outcome and in a
  stage left at its prompt (`battle::forfeit`).
- **Policies**: the player and every CPU play Spread (`StageOrder::targeting`,
  `CpuSeat::targeting`): data on the seat, so round 3's choice sets it.
- **Warning and waiting line** (`ddi_shell::battle_cue`): a shock glyph
  drawn in quads (whatever the skin) beside the field's right edge on
  the receptors' row (the bottom under Reverse), one dot a shock (eight)
  under it; it scales in over 0.2 s (at once under reduced motion), dims
  to 60% once the first shock is in sight on the field, becomes a small
  shield once the last shock passed, until the player recovers; the
  miniatures keep clear of its place. A muted 12 px line right-aligned
  under the power bar's end (below life) while the player's full bar
  waits: "Spread · waiting for target" (`room-play-attack-waiting`).
- **Debug**: `__DDI_DEBUG.battle.attacks`: per seat power, launched,
  received, checkpoints waited `[NoEligible, NoSafeSection]`, incoming
  sender, outgoing recipients; the attacks so far, the player's incoming
  attack, the warning and waiting line as drawn, settling, a final
  attack's draw, refused deliveries, the last 16 events. The desktop
  prints a line per commitment (`attack: committed CPU 2→you at 41.80 s,
  8 shocks from 46.23 s`); the web logs it to the console (debug level).
- **How often** (autoplayed player, the CPUs at their levels, two seeds
  each; `launches/min/seat` over the checkpoints' span; NoSafeSection's
  share of the checkpoints a full bar met, many at the song's tail where
  no section starts): Beatucada Medium (217 notes, 1.7 min) 1.4–1.7 and
  20–57%; Beatucada Hard (283) 2.0–2.5 and 0–51%; Sweeteners Hard
  (424, 2.5 min) 2.1–2.8 and 10–32%; Into My Dream Medium (280) 1.5–1.8
  and 13–66%; Some Things Must Hard (595, 2.3 min) 3.1–4.2 and 6–30%;
  its Challenge (831) 4.2–5.2 and 12–18%, NoEligible waits common there
  (everyone under attack). With seven CPUs at level 3 the CPUs stepped
  on 8–42 shocks a stage; nobody went out to attacks alone in these runs
  (the autoplayed player steps on none).
- Screenshots (`target/scratch/mp/m8b/shots/`, `shots.mjs`; Some Things
  Must Hard, the player autoplayed): `watch7-chrome-*` seven CPUs with
  shocks on their miniatures, the warning fresh, dimmed and as a shield,
  CPU hits (`*-cpu-hit-zoom-*`: the skin's mine explosion on the hit
  receptor), the waiting line, a sequence every 8 s and the results;
  `watch3-firefox-*` the same with three CPUs in Firefox; `reverse-chrome-*`
  the warning at the bottom under Reverse, reduced motion; `hit3-chrome-*`
  the player stepping on every shock (the script holds the shock's lane
  across it over the autoplay: a test drive, not a player) and going out
  to attacks at 38 s. Under ITG FA+ an attack stepped on whole costs about
  80% of a full gauge (eight misses), so two such attacks end a stage.
  `__DDI_DEBUG.hazards.mines` gives each shock's second;
  `__DDI_DEBUG.battle.attacks.cpu_hits` each CPU's shocks stepped on.
- Tests: `ddi-room` `attacks/tests.rs` (a full bar spent on eight shocks
  from the target's lead and the next checkpoint; a duel waiting for
  recovery; Leader falling back when its target is taken; Spread to the
  fewest attacked and the turning order; Retaliate's memory from the next
  batch over eight seeds; expiry unrefunded; no safe section in reach
  waits; 128 seats bounded, one attack a recipient; the same reports the
  same record; settlement and a final attack's shared first);
  `ddi-session` (`attacks_are_launched_delivered_and_the_same_at_any_frame_rate_and_from_the_log`,
  `an_early_win_waits_for_the_last_attack_to_clear` over 60 seeded
  battery stages, 4 of which waited, `the_player_gone_the_cpus_and_their_attacks_are_played_out`,
  `an_extreme_slow_speed_is_raised_to_fit_the_lead`); `ddi-shell`
  `battle_cue`; `ddi-desktop`
  `a_practice_stage_with_attacks_launches_them_through_the_window`.
- Tests: `ddi-engine` `cpu::mines` (the table, keyed draws),
  `cpu::opponent` (judged and the same at 60/144 frames a second, the
  control unchanged, a poor avoider steps on more than a good one over
  24 seeds, late shocks refused and nothing changed), `battle`
  (`reading_horizon`); `ddi-session` feed (`shocks_show_from_their_commitment_and_a_hit_takes_one_off`),
  stage (`cpus_receive_shocks_judged_alike_at_any_frame_rate`).

## Feedback round A: the opponents in play, and CPU levels as stars (implemented, 2026-11-12)

The maintainer's feedback on the play screen: "we are showing less
players than we can", the list the opponents collapsed into was not
understood and is not wanted, and eight opponents must fit on the
screen. Both shells (the shared `PlayView`, `ddi_shell::opponents`).

**What the play showed before** (eight CPUs in CPU practice, the web in
headless Chrome; Firefox the same at the first three sizes and at
390×844). "Lane" is a miniature's arrow size in logical pixels, "slot"
its name line and field.

| Size | Before | After |
| --- | --- | --- |
| 1280×720 | 8 miniatures, lane 32, slots 138×246 | 8, lane 42, slots 181×280 |
| 1920×1080 | 8, lane 44, 189×330 | 8, lane 68, 292×432 |
| 1024×768 | 8, lane 34, 146×260 | 8, lane 35, 151×300 |
| 800×600 | 8, lane 26, 112×204 | 8, lane 26, 112×229 |
| 720×900 | 8, lane 21, 90×169 | 8, lane 24, 103×176 |
| 844×390 (a phone held sideways) | 8, lane 16, 69×134 | 8, lane 21, 90×156 |
| 390×844 (a phone held upright) | no miniatures: the list, as numbered life bars without names | 8, lane 11, 47×137, four in each margin |
| 2560×1080 | 8, lane 44, 189×330 | 8, lane 79, 340×501 |

Nowhere in the table did the opponents take turns, before or after. (The
"after" sizes are with the levels' stars above the fields, which have a
line of their own in the narrower slots; without it the last three rows
had lanes of 26, 23 and 11.)

**Why it showed fewer, or the list, before.** Three separate rules:

1. *The list in the miniatures' place*, in any of three cases: a song
   with lyrics (a StepMania song with an `.lrc` file as much as a
   Dancing☆Onigiri work: the miniatures were turned off whenever lyrics
   were shown, so many pack songs showed the list at any size); a side of
   the field narrower than one miniature at lanes of 16 px plus its
   margins (about 100 px: a window narrower than about 0.43 of its
   height plus 200 px, such as a phone held upright, or 500×800); or the
   development switch
   (`DDI_MINIATURES=0`). The list itself then showed names only where
   its side had 96 px, else numbered bars, and only as many rows as the
   height held, then "+n more".
2. *Fewer miniatures than opponents, taking turns*: when all of them did
   not fit at lanes of 16 px (a phone held sideways at 568×320 showed
   four of eight; the development-only practice beyond the cap).
3. *Small miniatures in mostly empty sides*: a miniature's lane was
   capped at 45% of the play's own lane and at 44 px, and its field was
   always seven lanes tall, so one to four opponents at 1280×720 were as
   small as eight (lane 32), and 1920×1080 or wider never went past 44.
   A side's whole height was also given up above or below anything on
   it (the score, the watermark, the quit button's place).

**What changed.**

- **The list is gone** (`opponents::draw`, its constants and tests, the
  message `room-play-more`, the development switch
  `opponents::set_miniatures` / `DDI_MINIATURES` /
  `ddi.dev.miniatures`, the web's debug `opponents_column`). A stage
  always shows miniatures, with lyrics over the play too (tested
  offscreen: `lyrics_over_a_stage_keep_the_miniatures`).
- **Every opponent of a room has its own miniature at every size**, no
  turns: `opponents::arrange` goes down to lanes of 8 px
  (`MINI_LANE_MIN`), its margins and gaps shrinking with a side's width,
  so a phone held upright seats four in each margin beside the field
  (never over it). Only a window too small for them at that size shows
  fewer, taking turns, or none where a margin cannot hold one (under
  about 290 logical pixels wide).
- **As large as the room allows**: up to 75% of the play's own lane
  (`MINI_LANE_SHARE`) and 96 px; a field between six and ten lanes tall
  (`MINI_ROWS_MIN`, `MINI_ROWS_MAX`: a short room gives larger lanes on
  shorter fields, a tall one fields as tall as the play's own); what is
  on a side is avoided by height or by width, whichever gives the larger
  miniatures (the attack warning beside the receptors no longer pushes
  the whole right side down). One opponent at 1280×720 has lanes of 54
  (232×562), four have 51, eight 42.
- The name line's size follows the slot (9 to 14 px), a cut name ends in
  an ellipsis without a space before it.
- The status line ("Stage won", a battle's moment) stays under the right
  side's slots; where that side is too narrow for words (under 110 px)
  it goes beside the watermark, on its row.
- Beyond the cap (the development-only practice) nothing changed in
  kind: lanes of at least 16 px (`MINI_LANE_CROWD`), as many slots as
  fit, the rest taking turns (`ROTATION_V0`).
- The arrangement is computed once per field, screen and keep-outs
  (`PlayView`'s `fields_for`), not every frame.

Online rooms still show nobody else during a play: there is no stream
of the others' play yet (M5d). Out of this round's scope.

Tests: `ddi-shell` `opponents` (eight distinct slots, never over the
field's band, the keep-outs or each other, on the screen, at each size
of the table and at 1280×720@2 and 360×640, with and without Reverse and
the attack warning; one to eight at 1280×720; an upright phone; names
cut to their slots); `ddi-desktop` `opponent_fields_offscreen` (eight at
1280×720 and at 390×844, lyrics over a stage). Screenshots:
`target/scratch/mp/fb-a/shots/after-*`.

### Ten CPU levels, shown as five stars

The maintainer: more CPU levels, "especially more to the weaker side",
no "of 5", "some silver/gold star ratings or something that's more
visual and less text".

- **Ten levels** (`ddi_engine::cpu::CPU_LEVELS_V0`, `cpu-levels-v0`,
  PROVISIONAL; the model, the measurement and its tables are in
  [the CPU player's note](cpu-player.md), "An opponent's ten levels").
  Levels 6 to 10 are the five strengths that were levels 1 to 5,
  unchanged: the same chart, seed and skills give the same presses (a
  fixture of press hashes taken before the change). Levels 1 to 5 are
  new and weaker: a lower strength and a lapse, the chance a note is left
  out, growing with how demanding the chart is at the note. The solo CPU
  (`--cpu-strength`, `strength=`) keeps its five names and plays.
- **Chosen by measurement** over twelve arcade mixes of the maintainer's
  local library (2,974 dance-single charts, sixteen seeds; aggregates
  only in the repository). Mean percent under ITG FA+ by level: 39.9,
  49.5, 58.9, 67.8, 76.5, then the old five's 86.2, 91.9, 94.2, 95.8,
  97.3; out of the stage on 68.9%, 60.1%, 44.0%, 23.5%, 3.7% of the
  charts, then never. Level 1 scores about half a Beginner chart's points
  and is out of a quarter of them, and out of nearly every Hard chart.
- **`MINE_RESPONSE_V0` extended** (still uncalibrated): four responses
  along the avoidance now (worst at 0: 30% of shocks stepped on, ±65 ms,
  8% left out; then the three there were, at 1/3, 2/3 and 1). Levels 6
  to 10 respond exactly as levels 1 to 5 did; levels 1 to 5 are a fifth
  of the way each from the worst towards poorly.
- **What was stored or sent on the old scale.** `OpponentSpec.level`
  (the engine's index, `ddi_room`, on the wire in a room's rules and CPU
  members) is opaque to the room and the server: nothing validates it
  there, and `../ddi-server` builds unchanged. Its meaning widened in
  place from 0–4 to 0–9, so a value written before means a weaker level
  now (old "level 3", index 2, is the new level 3: 58.9% rather than
  94.2%). Nothing keeps one: a practice room lives for the session,
  hosted rooms seat no CPUs (their rules carry index 0, the weakest
  before and after), and no setting stores a level. A stage clamps an
  index past the last to the last. The practice default moved with its
  strength (`DEFAULT_CPU_LEVEL` 2 to 7: still the engine's `normal`),
  and the roster's 24 CPUs were spread over the ten levels (two or three
  each), keeping their order from weakest to strongest. Bots:
  `--mp-bots --level 1-10` (a named bot off the roster: level 8).
- **Stars, never a number** (`ddi_ui::stars`, the look's `Stars`
  component; colors `Star.Gold`, `Star.Silver`, `Star.Empty` in
  `infinito.toml`): five stars, levels 1 to 5 silver from the left over
  empty outlines, 6 to 10 gold from the left over silver. Vector shapes
  through the 2D painter (a filled star is five rhombi and a disc, an
  empty one ten strokes), so no face has to carry a glyph. Where a level
  shows: the room's players panel and Players pop-up (stars, then the
  personality), the Rules pop-up's roster (personality, then stars),
  Add CPUs (the level row is "Level" with the stars between its two
  buttons, the button says "Add 2 CPUs", the CPUs by name show theirs),
  the stage prompt's players (stars at the row's end, where people say
  Ready), and the play's miniatures (`ddi_shell::opponents::draw_stars`,
  the same geometry and colors: at the name line's end where a slot is
  at least 170 px wide, on a short line of their own under the name down
  to 44 px, left out below that). The messages `room-cpu-level`,
  `room-rules-level-value`, `room-play-level` and `room-cpu-label` are
  gone, `room-rules-level` is "Level", `room-add-cpus-at-level` became
  `room-add-cpus-add`. The number stays in the debug state
  (`ui.theme.room.members[].level`, `__DDI_DEBUG.opponents[].level`) and
  in the tests.
- Add CPUs' rows by name had their plates drawn half a row's width to
  the right (a list row's origin is its top left): fixed in passing.

Tests: `ddi-engine` `cpu::levels`, `cpu::tests`
(`levels_six_to_ten_press_exactly_as_the_five_strengths_did`,
`levels_one_to_five_lapse_more_the_weaker_they_are`), `cpu::opponent`
(all ten deterministic at 60 and 144 frames a second, a weaker level
scores no better on average), `cpu::mines`; `ddi-ui` `stars` (each
level's fills, the shape inside its box), `cpu_roster` (every level is
somebody's), `room_tests` (the roster's and Add CPUs' stars, no level
as text); `ddi-theme` (the component in its colors); `ddi-shell`
`opponents` (stars beside the name, under it, or left out; a person has
none). Screenshots: `target/scratch/mp/fb-a/shots/stars-*`.

## Feedback round B: the players on the room's song select, the standings on a stage's results (implemented, 2026-11-12)

The maintainer's feedback: "On the multiplayer song select view, I want
to be able to see the players without having to click on Players, maybe
we can add a column to the left and/or simplify the difficulty picker
and banner", and (next section) the standings and the match's final
result on a stage's results.

### The players, always on the room's song select

Everyone in a room sees the players on its song select without pressing
anything, in practice and online, the host and the followers alike
(`crates/ui/src/screens/select_room.rs`, `room.rs`).

- **The wide layout** (`Layout::for_room`): the song's side is short: a
  phone's banner (300×94) with the title and one line (artist, BPM,
  length) under it, and beside it the style button (only where the room
  can play more than one style), one row of chart buttons with their
  levels and the chart chosen in words (its name and level, its notes
  and step artist). The ladder, the tabs and the best's readouts are not
  shown in a room. The **players panel** (`room::players_panel`,
  `RoomPlayers`) takes the 426 px under it down to where the ladder
  ended: a heading with the count ("9 / 9"), then a line a player at 40
  px (30 at least): the place in the match once a stage is confirmed,
  the name (You on a quiet plate; a person's role after it: Host,
  Co-host, Admin; a bot's name labeled Bot; a CPU's five stars and its
  personality), what they are doing in words at the right (Ready in the
  accent; Not ready, Loading with its percent, Playing, Sitting out,
  Watching, Next match, At start for a CPU before the match; why a song
  did not load: Download failed, Song unreadable, Chart changed, in the
  danger color; Joining, Reconnecting and Away with the line dimmed) and
  the match points under a "Points" heading. Nine lines fit at 1280×720
  without scrolling; once the match has places the lines are in its
  order. Past what fits (a development practice's field: 12 lines of 30
  px) a last line counts the rest ("▼ 9 more") and opens the full list.
- **The room's styles**: a practice room's song select offers only the
  styles its CPUs can play (`RoomClient::style_playable`: one player,
  four lanes), so a library kept on Double opens on Single and a song
  without such a chart is grayed, where before it could be picked and
  was then refused in a pop-up (the refusal stays for a pick made any
  other way). An online room plays any style: its style button shows the
  style and its charts ("Single  5 charts  ▾") and Tab changes it. The
  footer's Tab hint is left out where there is one style.
- **The followers** have the same song's side and the same panel (one
  component, so both views agree); the list's place holds the status and
  the match's picks alone (`room::pick_panel_rows`), which the panel
  there no longer repeats the players under.
- **The Players button stays** (and More's Players on a phone): its
  pop-up is the only place a host sees the match's picks, and it lists
  everyone with two lines each. Nothing a keyboard, a controller or a
  touch could do is lost. A tap on the panel (a line, its "more" line)
  opens the same pop-up; the panel itself takes no focus (the pop-up has
  no per-player actions yet: roles and removing someone are not built).
- **A phone** (844×390 and 390×844) has no room for the panel beside
  the song and the list. The host's players are one line over the list
  (`room::players_strip`, `Players.Strip`): everyone's initials (You
  whole), each in the color of what they are doing as the panel's words
  are, those that do not fit counted ("+3"), then how many they are
  ("9 / 9 ›"); a tap opens the full list. A follower's list's place
  holds the panel as before (the picks, then the players, two lines
  each). Chosen for being one line: the phone's song side is short
  already and its list has five rows.
- **Considered and not chosen**: a column of its own at the far left
  (`docs/plans/multiplayer-ui.md`'s first sketch). At 1280 wide the
  song's side has 560 px and the list 596; a third column of the 240 px
  nine names with their states need would take them from the list's
  titles or leave the song's art a thumbnail. Under the song the panel
  is as wide as the song's side, so a line holds the place, the name
  with its stars, the state and the points without cutting any.
- **In passing**: the Rules pop-up's scrolled roster gave its "▲/▼ n
  more" marks 12 px each, so the lower one overlapped the Add buttons
  (and the upper one the first row); each has a row of its own now
  (`room_dialogs.rs`, `MARK`).
- **Singleplayer song select is unchanged**: the same pixels at 1280×720
  and 844×390 before and after (`target/scratch/mp/fb-b/shots/solo-*`,
  compared byte for byte, reduced motion so the live art stands still).

Tests (`ddi-ui` `room_tests`): `the_players_are_always_beside_the_song`
(nine lines without scrolling inside the song's column, You marked, each
CPU's stars and "At start", no ladder or tabs, a tap opening the list,
Ready while a stage gathers, then places and points in the match's
order), `the_players_panel_counts_those_that_do_not_fit`,
`a_phone_shows_the_players_as_a_strip_that_opens_the_list` (both phone
sizes, clear of the list), `the_panels_lines_say_what_each_player_is_doing`
(an online room over `FakeServer`: the host's role, watching, not ready,
ready, loading, a failed download, reconnecting, playing, sitting out;
the host's and the follower's panels the same, the follower's list's
place the picks), and `the_hosts_start_picks_the_song_and_starts_the_match`
rewritten for the room's styles. Screenshots
(`target/scratch/mp/fb-b/shots/`): `select-*` (practice with eight CPUs:
Chrome at 1280×720 in English, Japanese and `qps-ploc`, 844×390 in
English and Japanese, 390×844; Firefox at 1280×720; `select-dev-*` with
twenty CPUs), `online-*-00-*` (a local server of our own with the
development provider: the host, a follower at 1280×720 and one on a
phone in Japanese), `roster-*` (the Rules roster's marks).

### The standings on a stage's results

The maintainer's feedback: "On the multiplayer results view, I want to
see the standings and final result of a match, I think it could replace
the area with the 4 graphs, maybe we can have 2 tabs, one with the
standings (but with more details regarding how everyone performed after
that stage), and another with the graphs."

A stage's results (`ResultsScreen::with_room`,
`crates/ui/src/screens/results_stage.rs`; both shells hand the screen
the stage's room) have two tabs where a play's four graphs are.

- **Standings**, shown first: a table of every starter of the stage, in
  the stage's order once the room confirmed it: the place; the name (You
  on a quiet plate, a bot labeled, a CPU's five stars); what the stage
  awarded ("+12"); the match total after it and the place in the match,
  with a quiet ▲ or ▼ where that place moved; then how they performed:
  the ruleset's main number (the percent under ITG FA+, which counts no
  score; a DDI score or its percent as the player's own settings show
  it), the grade (in the lists' short form), life at the end or when it
  ran out ("Out 1:02"; "Left" for a forfeit), the longest combo, and the
  judgments as one thin stacked bar in the ruleset's tiers' colors
  (`screens::play::tiers`: the same tiers, names and colors as the
  results' own list, FA+'s two Fantastics and DDI's best word included).
  The exact counts are a second line under each starter's where the
  lines are tall enough (up to about six starters), else one line under
  the table for the line the pointer is over (the local player's
  otherwise). With attacks (the development switch) the counts are
  followed by "Attacks 2 · Shocks 1": those launched and the shocks
  stepped on. The panel's heading says "Stage 2 of 4" and, beside it,
  "points experimental" or, while the room has not confirmed, "Waiting
  for everyone's results…".
- **Lines fill in as reports come** (an online stage): before the room
  confirms, the lines are as seated, with no place, award or total; one
  whose result has come shows how they performed, one still playing a
  quiet "Still playing…". The order, places and awards come with the
  room's confirmation. The results screen keeps the room's messages
  coming while it shows (it ticks the client; nothing else did on that
  screen).
- **The match's final result**: after the last stage the tab opens with
  it on top, whatever tab was chosen before: "Match result", the winner
  ("Winner: You") or the winners of a tie in gold, then everyone in the
  match with their final place and total, three across (two on a phone);
  the stage's table under it. Nine starters and the result fit at
  1280×720 without scrolling (lines of 27 px; the pointed line's counts
  give way there). A stopped match has no result block.
- **Graphs**: the four graphs exactly as a play's results have them.
- **Changing the tab**: Left and Right on a keyboard, the shoulders on a
  controller (`MenuAction::Previous`/`Next`), a tap or click on a tab,
  or the focus walk to a tab and Confirm (the focus goes back to Back to
  room after, so Enter leaves). The footer says "← → Change tab" (the
  shoulders' art for a controller). The tab chosen is remembered for the
  session in memory, never saved. No animation: the content is swapped,
  so reduced motion changes nothing.
- **Past what fits** (a development practice's field) the lines scroll
  with Up and Down or the wheel, the rest counted ("▼ 8 more").
- **A phone**: the tabs are in the scrolling column (beside the score
  column sideways, under it upright), the table as tall as its lines
  (30 px). Columns are dropped as the width runs out, in this order: the
  judgments, the combo, life, the place in the match, the grade; the
  text is never smaller. At 844×390 and 390×844 the table shows the
  place, the name, the award, the total, the score and the grade.
- **Solo results are unchanged**: the same pixels at 1280×720 and
  844×390 before and after (`shots/solo-*-results.png`).
- **The pop-up on the room's song select**: once the Standings tab
  showed a confirmed stage (or the match's result), the room counts it
  as seen (`RoomClient::results_shown`), so `StandingsDialog` does not
  pop the same thing up again back on song select. It still does for
  someone who sat the stage out or watched, who left the results before
  the room confirmed, or who only looked at the Graphs tab. A CHOICE
  the brief left open ("may stay as it is"); one line to revert.

**The data** (`ddi_room::RunDetail`, `crates/room/src/battle.rs`): the
score where the ruleset counts one and its percent in hundredths (the
viewer's settings choose), the grade as the ruleset words it, life at
the end in millionths, when the gauge emptied, the longest combo, the
taps counted by the ruleset's tiers (best first, the misses last) and,
where the battle has attacks, those launched and the shocks stepped on.
SELF-REPORTED, shown only, never ranked by; the room cuts a grade to 24
characters and the tiers to 12.

- **Practice**: everything is local. `ddi_session::battle::StageOutcome`
  carries `details`, one a competitor, from the player's and each CPU's
  engine-judged play (`ddi_ui::state::room::run_detail`); the client
  keeps them (`RoomClient::set_stage_details`), since a practice's
  authority ranks the stage itself and takes no reports. A stage left at
  its prompt has the CPUs' alone.
- **Online**: a member's report (`Command::ReportOutcome`) may carry
  `detail`, the room keeps it beside the report
  (`StagePlay::details`, `Event::OutcomeReported::detail`) and every
  mirror has it. ADDITIVE to protocol 2, no new version: the field is
  optional with serde defaults everywhere, a report without one is byte
  for byte what it was, an authority that predates it ignores the field
  and a client that predates it ignores it in events and snapshots (a
  new version would turn away every client or server one step behind:
  noted at `PROTOCOL_VERSION`). A starter whose detail did not come (an
  older build, a server that dropped it) shows what the run itself says:
  the accuracy as a percent, life, or when they went out. The bots
  report theirs too (`crates/desktop/src/bots.rs`).
- **The deployed server needs its next deploy to pass the detail on**:
  it runs this crate's reducer as it was built, so until then it drops
  the field and online standings show the runs' own numbers (percent,
  life, out time), no grade, combo or judgments. `../ddi-server` builds
  against the crate unchanged; its tests construct the command, so they
  gained `detail` (its commit 16c9502: a report's detail is kept by the
  room; 54 and 55 tests pass, clippy native and wasm clean; its
  `xtask` config test fails on the maintainer's local `wrangler.toml`
  edit, untouched).

`StageTable` (`RoomClient::stage_table`) is what the screen reads: the
stage found in the running match or a finished one the room keeps (a
CPU that left with its match is named from the record), each starter's
confirmed placement, reported run and detail, the match's totals and
places after the stage and before it, and the match's final standings
where the stage was its last and the match completed.

Tests: `ddi-room` `tests/online.rs`
(`a_reports_detail_is_kept_alike_by_both_authorities_and_every_mirror`:
the contract's script through `LocalAuthority` and through
`OnlineAuthority` + `FakeServer`, the same rooms, the detail bounded,
the ranking the runs' alone), `tests/walkthroughs.rs`
(`a_reports_detail_is_additive_on_the_wire`: the old shapes read, a
report without a detail unchanged on the wire, an unknown field of a
later detail ignored, a stage without details stored as before);
`ddi-ui` `state::online::tests` (the table before any report, after
one, and confirmed, the same for everyone), `screens::play`
(`the_tiers_are_the_results_rows_with_their_counts`: ITG, ITG FA+,
DDI), `screens::results_stage_tests` (nine starters after a middle
stage: every column, the pointed line's counts, the tabs by keys,
shoulders and taps, remembered, the place's move marks, the last
stage's result and winner, song select's pop-up not repeated; two and
four starters, a tie, one out, one who left, a match tied at the top;
attacks counted; a long table scrolling; an online stage's lines
filling in, an older build's report; both phone sizes' columns), the
`qps-ploc` walk (both tabs after a middle and a last stage);
`ddi-desktop` `tests/room_stage_flow.rs` (the Standings tab after a
stage) and `tests/online.rs` (the detail through a local server).
Screenshots (`target/scratch/mp/fb-b/shots/`): `results-*` (practice
with eight CPUs, a middle stage and the last), `results-final-*` (a
one-stage match: Japanese, `qps-ploc`, Firefox, 844×390, 390×844),
`results-attacks-*`, `results-two-*`, `online-*` (three people against
a local server built from this checkout: one who left early waiting for
the others, the confirmed table, the graphs, the match's result).

### A room opens straight on its song select

The maintainer, 2026-11-12: "I think we don't have to show the rules
when opening, maybe we should make that a hidden option that's off by
default."

- Entering a room (CPU practice and an online room alike) no longer
  opens the Rules pop-up by itself: the room opens on its song select
  with the default rules and, in practice, the default roster (three
  CPUs drawn from the catalog), which the players panel shows.
- **Rules stays behind its button** (the header's, More's on a phone),
  in its one form: Cancel and Apply for an operator, Close alone for
  the others. The first-time form (Reset and OK) is not shown from the
  button: opened by hand before the first pick it had no way to back out
  of a change but Reset, and Cancel is that. It stays in the code for
  the hidden option alone. What is lost by default: Reset (back to the
  default rules in one press), which the button's form never had.
- **The old behavior is a hidden option, off by default**:
  `Settings::multiplayer_rules_first` (no row in Options), set by the
  web's `#/?mp=1&mprules=1` (`mprules=0` turns it off; kept in the
  settings as `mp` is) and desktop's `--mp-rules-first on|off`. With it
  the Rules open in their first form on entering and again each time the
  player comes back to the room, as M7b had it.
- In passing, on the players panel: nobody is "Not ready" before a pick
  gathers readiness (the lobby, between stages); the word shows only
  while a stage's readiness window is open.

Tests: `ddi-ui` `room_tests`
(`practice_opens_straight_on_song_select_with_the_rules_behind_their_button`,
`the_hidden_option_opens_the_rules_first_on_entering`, the online
room's test opening on song select with a member's Rules read only from
the button; the roster test's Reset under the option), the `qps-ploc`
walk (the first form under the option, the button's form without);
`ddi-desktop` `tests/room_offscreen.rs` (song select at once, the
players panel inside the window). Runtime: `room.mjs` and `online.mjs`
open the Rules from the button; `room.mjs … mprules=1` finds them open.

## Feedback round C: the miniatures' proportion and size, the hand in full (implemented, 2026-11-12)

The maintainer, on feedback round A's play screen
(`target/scratch/mp/fb-a/shots/after-chrome-1280x720.png`: eight
miniatures at lanes of 42 px on fields six lanes tall): "I do think the
miniatures might be a bit short, I like the proportion that I see on my
screen, maybe we can even make it have the same proportion as the
players field, also I think that when there are too few players the
miniatures get bigger than they should, we should probably have a cap to
their size. Also, I didn't like the "fade" of the dancing hand, I think
we can show it in full, it's in the background and doesn't get in they
way I think." Phone layouts are low priority: kept working, not
polished. Both shells (`ddi_shell::opponents`, the shared `PlayView`).

### The miniatures in the play's field's proportion, with a size cap

- **The proportion.** A miniature's field is as many lanes tall as the
  play's own field (`opponents::rows_for`: the screen's height over the
  play's lane, ten on a screen wider than it is tall), with its
  receptors a lane and a half from its end as the play's, so it reads as
  a small copy of the player's field and shows as far ahead. The
  six-to-ten range of round A went. Clamped only where a window's shape
  gives the play's field another proportion: eight to twelve lanes
  (`MINI_ROWS_MIN`, `MINI_ROWS_MAX`: a phone held upright, whose field
  has thirteen to sixteen lanes of height, gets twelve; a window too
  short for the play's least lane, eight).
- **The cap.** A miniature's lane is at most half the play's lane
  (`MINI_LANE_SHARE` 0.5, PROVISIONAL, a value to tune; it was 0.75 and
  96 px). `MINI_LANE_MAX` went: the play's own lane is bounded, so the
  share is the cap. One to four opponents are at the cap; more are as
  large as the sides allow under it, in the grid that gives the largest
  lane (six: three in a row a side; eight: two rows of two a side, four
  in a row where the screen is very wide).
- **The lanes** (logical pixels; the slot is its name line and field;
  CPU practice, the attack warning's place not kept clear; with it,
  eight are 25 at 1280×720 and 40 at 1920×1080):

  | Opponents | 1280×720 (play lane 72) | 1920×1080 (play lane 108) |
  | --- | --- | --- |
  | 1 | 36, slot 155×396 | 54, slot 232×562 |
  | 2 | 36 | 54 |
  | 4 | 36, two in a row a side | 54, two in a row a side |
  | 6 | 33, three in a row a side | 52, three in a row a side |
  | 8 | 28, two rows of two a side, slot 120×313 | 46, two rows of two a side, slot 198×482 |

  Eight at the other sizes of round A's table, all showing, none taking
  turns: 1024×768 lane 26; 800×600 lane 20; 720×900 lane 16, four in
  each margin; 844×390 lane 16, four in a row a side; 390×844 lane 13
  at three device pixels a point (11 at two, where the play's own lane
  is wider), four in each margin, twelve lanes tall; 2560×1080 lane 54,
  four in a row a side. (At 360×640, narrower than the table, the lanes are at
  their least, 8 px, and a side short of height for four gives one to
  the other: five and three. Not polished, as asked.)
- At lanes of 28 px a slot is 120 px wide, under the 170 px where the
  level's stars fit beside the name: eight at 1280×720 have their stars
  on a line of their own under the name, as the narrower slots already
  had. The name line, the life gauge, the power bar and the combo sit as
  before.

Tests: `ddi-shell` `opponents`
(`the_miniatures_have_the_plays_proportion_under_a_size_cap`: the
table's lanes exactly, the proportion and the receptors' share, the
grids; `eight_miniatures_of_their_own_at_every_size` and the others of
round A, their `sound` check now also the proportion and the cap);
`ddi-desktop` `tests/opponent_fields_offscreen.rs` (eight at 1280×720:
lanes of 28). Runtime, headless Chrome and Firefox:
`target/scratch/mp/fb-c/shots/minis-*`.

### The dancing hand in a stage, in full

In a stage the hand goes behind the field (M7c). It was drawn as large
as the band allowed below the judgment (`BEHIND_FIELD_ZOOM` 1.9), so its
arm was cut at the frame's top, softly (a fade over a quarter of the
frame). The maintainer did not like the fade and wants the hand in full.

- **Whole, as a solo play shows it.** The hand's picture behind the
  field now holds the whole model, the arm's end included, with nothing
  cut and no fade: its frame is the field's band from past the
  receptors' row to the far end (`play_view::behind_field`), and the
  camera stands back until that frame is as tall, in the hand's scale,
  as a solo play's frame (`dance::behind_field_distance`:
  `BEHIND_FIELD_TALL` 1.13, the solo frame's proportion on a 16:9
  screen, which has the mat's near edge at its foot and the arm's end
  with room to rise at its head), and no closer than fills the band
  (`BEHIND_FIELD_ZOOM` 2.0, PROVISIONAL).
- **Where and how large (what was chosen).** Clear of the receptors'
  row: the whole hand still fits at a sensible size there. At 1280×720
  the hand is 0.79 of its size in a solo play (its scale 486 px against
  616; 0.83 of the cut hand's before), its arm's end about at the
  judgment's line and its mat's near edge 15 px above the band's end;
  the same at 1920×1080. It stands behind the judgment, the combo and
  the notes, which the maintainer accepted (it is in the background).
  Under Reverse the frame is the band from its top to the receptors'
  row, the hand as upright as ever. Had it been allowed behind the
  receptors too, it would be exactly a solo play's size (the band's
  width then limits), its arm's end just under the receptors: the one
  alternative worth a look.
- **No fade.** The play passes none; `Renderer::set_field_underlay_fade`
  stays for whatever else may want one (nothing calls it now). Reduced
  motion and the hand's Off behave as before, and a solo play is
  unchanged (the settings' frame, the camera's own distance).

Tests: `ddi-shell` `dance`
(`the_hand_behind_the_field_is_shown_whole`,
`the_hands_place_in_a_stage_is_the_band_past_the_receptors`, and
`a_stage_puts_the_hand_behind_the_field_and_only_a_stage` as before).
Runtime, headless Chrome and Firefox:
`target/scratch/mp/fb-c/shots/hand-*` (a stage early and mid-song at
1280×720 and 1920×1080, before and after; a solo play before and after,
the same picture).

### DDI's score in a room (noted for round 3, nothing built)

Rooms still play `itg-fa+`. Outside multiplayer, DDI's displayed Score
became its proportional part plus its absolute score on 2026-11-12
(`docs/plans/ddi-ruleset.md`, "Score": `DdiScoreView::score` is
`ratio + absolute`, a perfect 1,000-note chart about 6,000,000). What a
room shows of a competitor's score (the standings on a stage's results,
`RunDetail::score` from `ddi_ui::state::room::run_detail`; a
miniature's numbers) keeps reading the ruleset's views, so under DDI it
would be that sum. **Round 3 must decide** what a room compares and
shows under DDI: a CPU plays a tap-only copy of the chart, so its
`most`, and with it the absolute score and the sum, differ from the
player's for the same play; only the proportional part
(`DdiScoreView::ratio`, the percent) compares across competitors.

## The DDI ruleset's round 3: DDI in multiplayer (implemented, 2026-11-12)

The maintainer noticed that DDI was not available in multiplayer and
approved this round (`docs/plans/ddi-ruleset.md`, "Round 3 (built)": the
rules themselves). Everything is still behind the hidden switch. What is
real: the engine's judging of a shock as a mine with the lane lock, the
placement, the rules in the room's model, the ranking, the screens. What
is not: attacks in an online room (no ledger there, see below);
`MINE_RESPONSE_V0` is still invented and uncalibrated; the CPU tables
below are CPUs against CPUs, never people.

### What changed

- **Rooms play DDI alone** (D0). `MatchRules::RULESETS` is `["ddi"]`;
  the Rules pop-up's Ruleset row went (a fixed row read as noise) and a
  note says "Everyone plays the match under DDI with these rules,
  whatever their own options say." Rules that still name one of the
  seven older ids are valid, kept as they came and played as DDI
  (`MatchRules::ruleset_id`); a stage's play, its CPUs and the bots are
  built from `StageRules` by `ddi_session::battle::stage_ruleset`.
- **Difficulty and Boos are match rules**: rows of the Rules pop-up
  (‹ › between Easy, Normal and Hard; Off and On), saved like Stages,
  locked with the match, read only for a member. Practice (DDI's
  option) is never set in a stage, whatever the player's settings say
  (test `a_stage_is_played_under_the_match_s_rules_not_the_player_s`).
  Summaries say only what differs from the usual: the pop-up's locked
  line ("4 stages · 3 CPUs · DDI · Easy · Boos · Attacks") and a
  stage's prompt ("Rules: DDI · Hard · Boos"); a stage's results say
  the play's own mods line as any DDI play's.
- **Attacks are a match rule** (the maintainer: "a multiplayer rule
  that's off by default but fully accessible is enough, multiplayer
  itself is already behind a development flag"): the pop-up's Attacks
  row, Off by default, chooses the stage's battle
  (`gauge-survival-v0` / `gauge-attacks-v0`; `MatchRules::attacks`,
  `set_attacks`). Everything the development switch gated (the power
  bar, launches, the warning, shocks, the settlement) follows the rule.
  The switches are conveniences now: desktop's `--mp-attacks` (its old
  name `--mp-dev-attacks` still taken) and the web's `mpattacks=1`
  start a practice room with the rule already On. `--mp-dev-mines` and
  `--mp-dev-cpus` stay development tools. A note under the rows says
  what the battle is ("The battle is provisional: your stage ends when
  your life runs out. With attacks, good timing fills a power bar, and
  a full bar sends eight shocks into another player's field.").
- **Online rooms do not offer Attacks**: the row is there, fixed, saying
  "Off · not in online rooms yet" (`screens::room::attacks_offered`).
  What online attacks would take: the ledger (`StageBattle`) run by the
  room from its members' per-checkpoint reports (their judged tiers,
  life and going out: `ddi_room::attacks::Report` exists, no command
  carries it yet), the chart's sections agreed (each client builds the
  `Schedule` from the chart with the engine's placement and sends its
  digest: the room reads no charts), every commitment sent to its
  recipient's device in time to be put into its play
  (`Player::add_hazards`: before the shock's window, so the lead has to
  cover the network, or the stages have to start together, D5), the
  settlement run by the room in the place of ranking the reports as
  they come, and the others' state streamed if their power and shocks
  are to be seen. None of it is built.
- **A shock is a mine** under DDI (D1, D3): the tally, −10 points, the
  life by the match's difficulty (−1, −5, −10), the combo, the floors
  and the lamp as a chart's mine, and its lane locked for a second, for
  the player and for a CPU alike (the same player and judge). The
  player's receptor keeps the skin's mine effect for the lock, as for a
  chart's mine. Shocks are still drawn as the skin's mines: shock-arrow
  art is open.
- **Placement** `placement-v1`: margins about each step of a shock's
  lane (none within 0.25 s after one nor within W3 before it, none from
  a hold's or a roll's head to past its end, none from a second before
  a lift). The bundled four-lane charts have room in every section
  under DDI's three difficulties (1,496 of 1,496 slots under Normal;
  `placement-v0`'s clearances under the same table found 1,484).
- **Miniatures**: each opponent's score, small, under its life gauge's
  right end, as the local player's Score display has it (the
  proportional part out of a million, "987654", or "98.76%"); a lane a
  shock locked is darkened to its end with its receptor's place tinted
  violet for the lock's second (ours, whatever the skin: no skin Lua
  runs for a miniature).
- **Ranking**: those still in by life, then the proportional score,
  then (the same notes) the absolute score; `ddi_room::Absolute` on the
  run, optional and additive; the bots report theirs.
- **The stage results' Standings** show DDI's numbers through the
  ruleset's views: the score column is the proportional part (or the
  percent under Score display Percent), the grade in the lists' short
  form (AB, SSS, …), the judgments' bar in DDI's seven tiers.

### What compares, and what does not

A CPU plays a tap-only copy of the chart (holds and rolls as taps), the
player the whole chart, so their `most` differ: DDI's absolute score and
the displayed Score (the proportional part plus the absolute score) do
not compare across them, the proportional part does. So the miniatures,
the Standings and the ranking use the proportional part; the absolute
score only breaks a tie between those whose `most` is the same (the CPUs
among themselves; everyone in an online room). The player's own HUD is
unchanged: under Score display Score it shows the sum, which is larger
than the numbers on the miniatures beside it (an open question).

### Measured: the ten CPU levels under DDI

`DDI_CPU_CORPUS=… DDI_CPU_RULES=ddi-easy,ddi-normal,ddi-hard cargo test
--release -p ddi-engine --test local_cpu -- --nocapture`
(`crates/engine/tests/local_cpu.rs`): the same twelve arcade mixes of the
maintainer's local library as feedback round A's measurement (724
simfiles, 2,974 dance-single charts, read in place, aggregates only),
sixteen seeds a chart and level, each CPU alone on its tap-only copy to
the song's end with the fail off; "out" is its life emptying at any
point, as a stage would end it. The levels were not retuned.

| All 2,974 charts | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Easy: percent | 68.3 | 73.3 | 77.9 | 82.0 | 85.9 | 90.2 | 94.0 | 95.6 | 96.7 | 97.8 |
| Easy: out % | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Easy: life at the end % | 99.5 | 99.5 | 99.6 | 99.7 | 99.9 | 100 | 100 | 100 | 100 | 100 |
| Normal: percent | 61.4 | 67.5 | 72.6 | 77.4 | 81.7 | 86.1 | 91.0 | 93.1 | 94.5 | 95.9 |
| Normal: out % | 20.0 | 9.6 | 3.8 | 0.3 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Normal: life at the end % | 68.3 | 81.1 | 89.9 | 96.1 | 98.5 | 99.9 | 100 | 100 | 100 | 100 |
| Hard: percent | 56.1 | 62.9 | 68.7 | 73.7 | 78.4 | 83.1 | 88.5 | 90.8 | 92.5 | 94.2 |
| Hard: out % | 82.5 | 69.6 | 52.8 | 26.4 | 3.1 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Hard: life at the end % | 9.3 | 20.4 | 35.7 | 61.2 | 89.5 | 99.4 | 99.9 | 99.9 | 100 | 100 |

Out % by difficulty slot (charts), Normal then Hard:

| | 1 | 2 | 3 | 4 | 5 | 6–10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Normal: Beginner (641) | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Normal: Easy (699) | 1.1 | 0.2 | 0.0 | 0.0 | 0.0 | 0.0 |
| Normal: Medium (699) | 12.1 | 2.7 | 0.3 | 0.0 | 0.0 | 0.0 |
| Normal: Hard (699) | 46.3 | 20.4 | 6.5 | 0.3 | 0.0 | 0.0 |
| Normal: Challenge (236) | 75.3 | 52.4 | 27.9 | 2.5 | 0.0 | 0.0 |
| Hard: Beginner (641) | 51.4 | 28.6 | 20.0 | 6.3 | 0.0 | 0.0 |
| Hard: Easy (699) | 74.1 | 48.7 | 29.3 | 8.6 | 0.1 | 0.0 |
| Hard: Medium (699) | 96.6 | 89.9 | 57.2 | 20.4 | 0.7 | 0.0 |
| Hard: Hard (699) | 99.6 | 97.8 | 87.3 | 51.3 | 6.0 | 0.0 |
| Hard: Challenge (236) | 100.0 | 99.4 | 95.7 | 77.1 | 19.4 | 0.0 |

The nineteen bundled four-lane charts agree (Normal: 61.7 to 95.9
percent, out 26.0, 11.8, 4.9 then none; Hard out 74.0, 69.4, 57.2, 30.3,
5.6 then none; Easy never out).

What it says. Under ITG FA+ (rooms until now) the same levels scored
39.9 to 97.3 percent and went out on 68.9, 60.1, 44.0, 23.5 and 3.7
percent of the charts. DDI Normal is far gentler on the weak levels (a
one-star CPU is out of one stage in five, not two in three, and scores
61 percent, not 40: W4 and W5 still give points and a Miss costs a
tenth of a bar from a life that recovers 2.5 times as fast); DDI Hard
is about where FA+ was; under DDI Easy no level ever goes out. The top
levels score a little less than under FA+ (95.9 against 97.3: the
±16.67 ms top class is narrower than FA+'s Fantastic for points).

### Measured: attack rates in CPU practice

`cargo test --release -p ddi-session --lib attack_rates -- --ignored
--nocapture` (`crates/session/src/battle/study.rs`): the M8b study's six
bundled charts (Beatucada Medium and Hard, Sweeteners Hard, Into My
Dream Medium, Some Things Must Hard and Challenge), each a stage of
eight CPUs with the Attacks rule On and the stage's fail on, the local
player gone at its start (the CPUs and their attacks played out, as a
stage left at its prompt), six seeds: 36 stages a row. Counted, a CPU:
the bars its timing earned a minute while it was in (W0 and W1 two, W2
one, a bar 100, the cap aside); the attacks it launched; and, an attack
received: the shocks stepped on (of eight), the notes that passed missed
in a lane a shock had locked, the life those cost (a mine's and a
Miss's by the difficulty), and whether its recipient went out before the
attack's last shock and its lock were over.

Eight CPUs at one level, DDI Normal:

| Level | Bars a minute | Launched a seat a stage | A received attack: shocks stepped on | notes missed in locks | life lost | knocked out by it | Out of the stage |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1.11 | 0.88 | 2.05 | 1.30 | 23.2 | 15.4% | 59.0% |
| 2 | 1.39 | 1.43 | 1.82 | 1.50 | 24.1 | 11.4% | 45.1% |
| 3 | 1.64 | 2.29 | 1.77 | 1.48 | 23.6 | 5.3% | 24.7% |
| 4 | 1.94 | 3.23 | 1.52 | 1.52 | 22.8 | 3.2% | 11.5% |
| 5 | 2.20 | 3.85 | 1.29 | 1.40 | 20.5 | 0.8% | 3.8% |
| 6 | 2.45 | 4.27 | 1.10 | 1.27 | 18.2 | 0.3% | 1.4% |
| 7 | 2.85 | 5.18 | 1.10 | 1.24 | 17.9 | 0.1% | 0.7% |
| 8 | 3.15 | 5.78 | 0.46 | 0.50 | 7.2 | 0.0% | 0.0% |
| 9 | 3.31 | 5.98 | 0.47 | 0.53 | 7.6 | 0.0% | 0.0% |
| 10 | 3.57 | 6.31 | 0.14 | 0.17 | 2.4 | 0.0% | 0.0% |

The other difficulties, the same fields (life lost by a received attack;
knocked out by it; out of the stage):

| Level | Easy: bars a minute | life lost | out | Hard: bars a minute | life lost | knocked out | out |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1.62 | 7.1 | 0% | 0.67 | 28.0 | 80.0% | 100% |
| 2 | 1.88 | 6.2 | 0% | 0.89 | 30.0 | 65.0% | 99.3% |
| 3 | 2.11 | 5.3 | 0% | 1.18 | 27.3 | 41.1% | 92.0% |
| 4 | 2.34 | 4.9 | 0% | 1.47 | 34.0 | 25.4% | 75.0% |
| 5 | 2.58 | 4.0 | 0% | 1.81 | 33.2 | 14.7% | 47.2% |
| 6 | 2.83 | 3.3 | 0% | 2.12 | 33.7 | 6.9% | 23.3% |
| 7 | 3.16 | 3.1 | 0% | 2.50 | 30.7 | 5.4% | 21.5% |
| 8 | 3.44 | 1.6 | 0% | 2.92 | 14.3 | 0.4% | 2.1% |
| 9 | 3.57 | 1.5 | 0% | 3.11 | 13.4 | 0.3% | 1.7% |
| 10 | 3.78 | 0.6 | 0% | 3.38 | 4.3 | 0.1% | 0.7% |

A field of mixed levels (1, 2, 3, 4, 5, 6, 8 and 10, the seats in turn):
26.7 attacks a stage under Normal (36.7 under Easy, 18.2 under Hard,
where the weak are out before they launch); an attack received costs
16.5 life on average under Normal (1.12 shocks stepped on, 1.09 notes
missed in locks), 4.2 under Easy, 19.6 under Hard; 3.4% of the attacks
under Normal knock their recipient out (7.5% under Hard, none under
Easy). By chart (Normal, the mixed field): 16.0 attacks a stage on
Beatucada Medium (217 notes), 19.0 on its Hard (283), 27.2 on Sweeteners
Hard (424), 23.2 on Into My Dream Medium (280), 37.3 on Some Things Must
Hard (595) and 40.3 on its Challenge (838), that is 1.2 to 3.6 launches
a minute a seat still in.

What it says, for tuning cost and damage. A bar fills from 50 notes in
the top class: a strong CPU fills three and a half a minute on these
charts and launches six attacks a stage; a one-star CPU one a minute.
A whole attack stepped on would cost 40 life on Normal (eight mines)
and what the locks miss; what the CPUs actually lose is 2 to 24 life an
attack by their level, about half of it to the locks (a note missed in a
locked lane costs twice a mine). Under M8b's rule (each shock the
ruleset's miss) an attack stepped on whole cost 80% of an ITG FA+ bar;
under DDI Normal it is 40% and the locks. Attacks alone rarely end a
strong CPU's stage; they end a weak one's often under Hard and never
under Easy.

### The stall of a stage's prompt under reduced motion (found and fixed)

The open question of feedback round B: under reduced motion a player's
prompt stopped at "Stage n starts in 3…", the page drawing and ticking
nothing more.

- **Reproduced**, in CPU practice, in both browsers, without a server:
  a practice room's prompt left alone under reduced motion (no Ready, no
  input; the prompt's own countdown turned off so that it idles), its
  status counting the readiness window's seconds
  (`target/scratch/ddi/r3/stall.mjs`). Before the fix: headless Firefox
  7 stalls in four windows of 27 s, headless Chrome 1 in six; each with
  `__DDI_DEBUG.ui.needsRedraw` false, each caught up at the next mouse
  move. (Earlier rounds saw practice never stall because a practice
  prompt idles for a few seconds only; and took "a still headless
  Firefox page catches up at the next input" for the browser's doing:
  it was this.) An online room was not run: the server answering on
  127.0.0.1:8787 signs in with the real Twitch, not the development
  provider, so no test accounts could enter it.
- **The cause** is in the UI core, not the web's `schedule`: under
  reduced motion a still screen with a deadline (the prompt reads its
  room ten times a second, `Screen::wake_at`) is woken by the shell's
  timer, which asks `Ui::idle_wake` when to wake and, once there, tells
  the UI how long it was idle. The shell's clock is far from zero and
  coarse (a browser's `performance.now()`: tenths of a millisecond in
  Chrome, about a millisecond in Firefox), so when the timer fired right
  at the deadline, `now − from` could come out a hair (1e-13 s) under
  the wait the deadline was reckoned from: the UI's clock then stood
  just short of the deadline, `needs_redraw` was false, the wait was
  reported over, and nothing was left to wake the shell: no frame, no
  timer, until an input. A stage's prompt idles exactly from the moment
  everyone is ready (its own countdown draws frames until then), which
  is why it showed "starts in 3…"; the others played and the room
  ranked the stalled player a forfeit. Normal motion never idles there
  (the controls' demonstration draws every frame). Desktop's shell uses
  the same call.
- **The fix** (`Ui::idle_wake`): a wait that is over moves the UI's
  clock on to the deadline at the least and asks for a frame outright.
  The test that would have caught it:
  `crates/ui/tests/layers.rs`
  `a_wait_that_is_over_always_asks_for_a_frame` (a screen that looks ten
  times a second, a shell clock on a browser's grids far from zero, the
  timer firing at the first instant at or past the deadline, 4,000 wakes
  a grid: it failed at the 260th before the fix). After the fix,
  the same watch: no stall in six windows in Firefox and eight in
  Chrome.
- **The other lead** (a direct `#/play?…` prompt under reduced motion
  keeping `waiting: ["noteskin","audio"]`) was a stale debug state, not a
  wait: `__DDI_DEBUG.prompt` is published by frames, and a still prompt
  under reduced motion drew none when only what the play waits for
  changed (nothing drawn changes). Nine seconds into a direct solo
  prompt left alone, a mouse move (a frame) showed `["audio"]` within
  60 ms in both browsers: the skin had loaded behind the prompt long
  before, and Start then began the play in 0.6 s (Chrome) and 1.0 s
  (Firefox), as with normal motion. Nothing a stage the room starts
  would wait on. The page now asks the prompt for a frame when what the
  play waits for changes, so the state reads true without an input
  (`["audio"]` at 1.6 s in Chrome and 2.3 s in Firefox under reduced
  motion, `target/scratch/ddi/r3/direct.mjs`).

### Checked

- Tests. `ddi-room`: `rules` (old ruleset ids read, kept and played as
  DDI; Difficulty and Boos additive on the wire; Attacks choosing the
  battle), `battle` (ranking by life, the proportional score, then the
  absolute score of the same notes, with ties; a report's absolute score
  there and back), `attacks` (the classes cover DDI and every older
  id), `tests/online.rs`
  `the_match_rules_of_ddi_are_kept_alike_by_both_authorities_and_every_mirror`
  (the contract's script through `LocalAuthority` and through
  `OnlineAuthority` + `FakeServer`: the same rooms, an unknown ruleset
  refused, the rules locked with the match, the tie broken).
  `ddi-engine`: `tests/ddi_shocks.rs` (the twin: a shock and a chart's
  mine come to the same results under each difficulty, with and without
  Boos; the tally, points, hold units, lamp, life by difficulty, the
  locks; one shock a Break flag and no Absolute Dito; the nearer of a
  shock and a note; the same at 1/60, 1/144 and 0.7 s updates and from
  the log), `tests/battle_shocks.rs` (the margins on a chart with a
  stream, a hold, a roll and lifts; an exact play steps on nothing the
  placement places; an all-stream chart has no section; the bundled
  charts under both placements), `cpu::opponent`
  (`under_ddi_a_shock_locks_a_cpus_lane_and_what_passes_there_is_missed`:
  the same at 60 and 144 frames a second, no Boo with Boos on).
  `ddi-session`: `a_stage_is_played_under_the_match_s_rules_not_the_player_s`
  (never Practice), `a_shock_stepped_on_in_a_stage_is_a_mine_by_the_match_s_difficulty`,
  the attack runner's traces now under DDI
  (`attacks_are_launched_delivered_and_the_same_at_any_frame_rate_and_from_the_log`
  and the others), the feed's lock. `ddi-shell`:
  `each_miniature_shows_its_score_and_its_locked_lanes`. `ddi-ui`:
  `room_tests` (the rows by taps, by keyboard and by controller, the
  locked line, an online room's unavailable Attacks and its members'
  read-only rules), the `qps-ploc` walk, `tests/layers.rs` (the idle
  wake). `ddi-desktop`: `tests/room_stage_flow.rs` (the Standings'
  proportional score); `tests/online.rs` was not run (it needs a local
  server with the development provider; the one answering here signs in
  with Twitch). `../ddi-server` builds and passes its tests against the
  changed `ddi-room` (its one literal report gained the new field).
- Runtime, headless Chrome and Firefox
  (`target/scratch/ddi/r3/room.mjs`; screenshots in
  `target/scratch/ddi/r3/shots/`): `rules-*` the Rules pop-up with
  Difficulty, Boos and Attacks in English, in Japanese and in Firefox;
  `words-*` a stage against eight CPUs under DDI pressed 25 ms early
  (the early word "Fantastic", each miniature's score, the player's);
  `attacks-*` (Chrome and Firefox) Attacks set On in the pop-up: a
  shock stepped on by the player (the receptor's effect, the lane
  locked, the combo broken, life −5, a note missed in the lock) and by
  CPUs (`*-cpu-lock-zoom-*`: the dark lane and violet receptor), the
  stage's Standings and the match's result; `hard-*` Hard with Boos
  under Score display Percent (the prompt's "Rules: DDI · Hard · Boos ·
  Attacks", percents on the miniatures, life −10, the results' "hard ·
  Boos"); `reduced-*` a stage under reduced motion.

## Round 3b: follow-ups from the maintainer's first test of DDI in rooms (2026-11-12)

The maintainer's notes after playing round 3 (`docs/plans/ddi-ruleset.md`,
"Round 3b (built)": the rules' side). Everything is still behind the
hidden switch.

### The mostly black room after stopping a match (found and fixed)

The maintainer: "I've tested stopping a CPU practice match mid-way and
when I tried starting a new match something went wrong and the rendering
went mostly black and I had to restart (it was on the web)."

- **Reproduced** in headless Firefox (the maintainer's browser), CPU
  practice (`target/scratch/ddi/r3b/repro.mjs`, `trace.mjs`): a stage
  quit part-way, back in the room More → Stop match, the standings
  closed, then a pick. The room's song select then stood with its
  header, list, song side and footer gone (their exit had played) and
  only the room's own parts left (Rules, Players, More, "Match stopped.
  Pick a song to start the next match."): nothing came after, and no key
  brought it back (`shots/item1-before-black-room-firefox.png`). In
  another order of the same race the standings' Continue or Escape left
  the room for the page before it. It is a race, so not every run showed
  it: of ten Firefox runs of the stop flows before the fix, three ended
  in the black room, two left the room and five went well. Every
  other way of stopping (a quit by Escape or the quit button, Back at a
  stage's prompt before and after Ready, Rules applied mid-match, Back to
  the home and in again, Leave room, the browser's Back and Forward over
  the play route), with Attacks off and on, 4 and 8 CPUs and the dancing
  hand on, drew the next stage as it should in both browsers; no wgpu or
  WebGL error and no lost device in any run.
- **The cause** is not in the renderer but in the web shell's history
  entries for dialogs (`crates/web/src/components/ui_dev.rs`,
  `sync_history`). Each modal layer has a history entry, so the browser's
  Back closes it; a layer closed otherwise is gone back over with
  `history.go(-1)`, whose `popstate` the shell ignores, and a navigation
  waits for that pop. Stop match closes the More menu and opens the
  standings right after: the standings' entry was pushed before the
  menu's `go` had been carried out. Measured in both browsers
  (`target/scratch/ddi/r3b/histexp.mjs`): Firefox then drops the `go`
  with no `popstate` at all, so the shell waited for a pop that never
  came and every later navigation of that page (the pick's way to the
  play route, Back) played its exit and then did nothing; Chrome carries
  the `go` out and drops the pushed entry, so the shell thought it had an
  entry it had not and the next dialog closed went back out of the route.
  Any dialog opening as another closes could do it; Stop match always
  does.
- **The fix**: one history operation at a time. The bookkeeping moved
  into the UI core (`ddi_ui::back_stack::Entries`): while a `go` of ours
  is on its way nothing is pushed, gone through or navigated; its pop
  brings the layers' entries up to date, then a waiting navigation. A pop
  that has not come for a second (`POP_WAIT`) is given up with a console
  warning ("ui: a history pop of ours never came…") and the entries are
  read again from the history, so nothing can wait on a pop forever.
  `__DDI_DEBUG.ui.historyDepth` and `historyBusy` say where it stands.
- **The test that would have caught it**:
  `crates/ui/src/back_stack.rs`
  `a_layer_opening_as_another_closes_waits_for_the_pop`, on a model of
  each browser's history as measured (`pushing_before_the_pop_is_what_breaks_the_history`
  shows the old order failing on both), and
  `a_lost_pop_is_given_up_after_a_while`. The brief asked for an
  offscreen pixel test of a stage after a quit; that would have passed
  before the fix, since the play was never drawn wrong.
- **After**: the same flows (Stop match after a quit, after the
  browser's Back, after Back at the next stage's prompt before and after
  Ready, the standings closed by Continue and by Escape; Rules applied,
  the home, Leave room, a plain quit), with Attacks off and on and 4 and
  8 CPUs: 14 runs in Firefox and 11 in Chrome
  (`target/scratch/ddi/r3b/matrix-fix-*.log`), no black room, nobody put
  out of the room, every second match's first stage drawn
  (`shots/item1-after-*.png`).

### The score a room shows and ranks by is the sum

The maintainer: "We should show the sum everywhere instead of the
proportional part, I think the proportional part was just a
misunderstanding, visually we should have the percent and the score
(sum) and that's it." So everywhere a person reads a DDI score in a room
it is the score (`DdiScoreView::score`, the proportional part plus the
absolute score) or, under Score display Percent, the percent: each
miniature's number under its gauge (`ddi_shell::opponents::ScoreShown`,
`Sum` or `Percent`), the stage results' Standings
(`ddi_room::RunDetail::score`), and what a stage ranks by. The
proportional part is shown nowhere (the HUD, the results, the lists, the
ghost and the summaries already showed only the two; `ratio` stays in
the engine for the sum's arithmetic).

- **A CPU's score on the chart's scale** (`CPU_SCORE_V0`, PROVISIONAL,
  `ddi_engine::rules::ddi`; `DdiScoreView::on_scale`,
  `ddi_session::battle::chart_most` and `cpu_score`). A CPU plays a
  tap-only copy, so on a chart with holds the most its notes give is
  less than the chart's and its own sum would read low beside the
  player's. Its points are taken as the same share of the whole chart's
  most as it earned of its own (`equivalent points = round(its points ×
  the chart's most ÷ its own most)`), and its score is `its proportional
  part + 100 × equivalent points + its W0s`: as if it had done on the
  holds as it did on the taps. Its percent is its own play's. The bots
  (`ddi --mp-bots`), which play the CPU's copy in an online room, report
  theirs the same way. People in an online room play the same chart, so
  their sums compare as they are. What it cannot say: a CPU has no way
  to drop a hold, so its hold units are imputed, never played; and its
  W0s are capped by the taps it has (a hold's units earn no W0 for
  anyone, so the cap is the same as a person's).
- **Ranking** (`ddi_room::battle::rank`): those still in by life, then
  the score (`Run::Live`'s `score`, additive and optional on the wire as
  `absolute` is), equal ones tied; a tie between a person and a CPU is
  an ordinary tie now, since their scores are on one scale. Where
  someone still in reported no score (a build before this round), the
  stage is ranked as round 3 ranked it. The deployed server ignores the
  new field until its next deploy and ranks by the proportional part
  meanwhile.
- **A long score on a narrow miniature** (a phone's eight 32 px fields)
  is set smaller so that it stays within its field's four lanes, never
  under 6 px (`SCORE_FLOOR`).
- Tests: `ddi-engine` `a_score_is_put_on_another_chart_s_scale_by_its_share_of_the_most`;
  `ddi-room` `those_still_in_rank_by_life_then_the_score` (and the
  contract's script through both authorities, its tie now broken by the
  sum); `ddi-session`
  `a_cpu_s_score_is_on_the_chart_s_scale_and_the_stage_ranks_by_life_then_the_score`
  (a bundled chart with holds: each CPU's sum, its detail and the
  ranking); `ddi-shell` `each_miniature_shows_its_score_and_its_locked_lanes`;
  `ddi-desktop` `tests/room_stage_flow.rs` (the Standings' sums) and the
  bots' run.

### "Timing difficulty"

The maintainer: "'Difficulty' doesn't convey the setting too well, maybe
something like 'Timing difficulty' or 'Timing strictness' would be
better." The option (Options → Rules) and the match rule (the Rules
pop-up) are labeled **Timing difficulty** (Japanese タイミング難易度, an
unverified draft, added to `locales/ja/GLOSSARY.md`); the values are
Easy, Normal and Hard as before, and identifiers keep `difficulty`.
Where a summary says what differs from the usual it says "Hard timing"
or "Easy timing" (ハードタイミング, イージータイミング): the Rules pop-up's
locked line and a stage's prompt ("DDI · Hard timing · Boos · Attacks"),
and the results' mods line and the plays list ("hard timing", in that
line's lower case). Looked at in headless Chrome at 1280×720 and 844×390
in English, Japanese and `qps-ploc` (`shots/i23-options-*`,
`i23-rules-*`): the row's label has room in each.

### A battle's shocks drawn as shock arrows, a chart's mines as mines

The maintainer: "I want to differentiate between 'attack mines'
(dynamic) and 'chart mines' (native to the chart) by their skin … attack
mines use the shock note skin." Planned in the ruleset ("Mines and
shocks": one object, two looks) and put off in round 3; built now.

- **A skin element of our own, `Tap Shock`**
  (`docs/plans/noteskin-authoring.md`, "`Tap Shock`"): a skin that opts
  in (`[NoteDisplay] UseTapShock=1`) draws a battle's shocks (the notes
  numbered from `HAZARD_INDEX_BASE`) with it, on the play's field and on
  the opponents' miniatures (the play's skin draws those, with no Lua
  run: the element is loaded with the lane's others, so it is ready
  there); a skin without it draws them as its `Tap Mine`, as before. The
  hit effect is the mine's. The element is loaded only for a play that
  may be given shocks (a stage whose match has attacks, or the
  development switch's section: `StageOrder::may_have_shocks`, told to
  the skin as `ddi_noteskin::lua::Env::shocks`), so a solo play loads
  and draws exactly what it did.
- **Infinito**: `Tap Shock` is its lane-shaped shock arrow, always. Its
  `Tap Mine` is what the `ShockArrows` parameter says, except in a play
  that may be given shocks (`NOTESKIN:PlayHasShocks()`, our extension),
  where a chart's own mines are the spiked band whatever the parameter
  says: the two never look the same. The parameter's description says so
  (it is the skin's own text, not a message). Its files are ours; the
  change is recorded in its `PROVENANCE.toml`'s modifications and its
  README, as its other changes are. No new art: both looks were there.
- **Mine row links** (`MineConnectRows`) join a row's mines with mines
  and its shocks with shocks, each with its own link, never a shock with
  a mine.
- **The built-in fallback arrows** (drawn while a skin loads, or where
  one cannot draw a lane) tint a shock violet (`scene::SHOCK_TINT`); a
  chart's mine stays white.
- **The other bundled skins** have no `Tap Shock` and draw shocks as
  their mines: Alpha-Arrows, CF_ALLOY, CF_CHROME, CF_VIBRANT, Dake,
  DanOni, Delta, Peters-DDR, Peters-DDR-Note and Peters-DDR-Vivid.
- Tests: `ddi-noteskin` `a_skin_opts_into_its_own_tap_shock`,
  `a_skin_s_tap_shock_loads_and_its_lua_is_told_of_shocks`; `ddi-render`
  `a_battle_s_shock_is_drawn_by_tap_shock_and_a_chart_s_mine_by_tap_mine`
  (two different elements in one frame, the links, the fallbacks);
  `ddi-desktop` `tests/tap_shock_skin.rs` (the bundled Infinito read as
  a play reads it: the shock lane-shaped and the mine spiked in a stage
  with the parameter off and on, a solo play as before, Delta and
  Peters-DDR without the element).
- Looked at in headless Chrome and Firefox
  (`target/scratch/ddi/r3b/look6.mjs`, `shots/i6-*`): a stage of eight
  CPUs with Attacks on, on Some Things Must's Challenge chart (seven
  mines of its own at beats 368 to 372, a section of shocks put in from
  beat 364 by `mpdevmines`): Infinito with `ShockArrows` off and on (the
  chart's mines spiked in both, the shocks lane-shaped, on the field and
  on the miniatures), and Delta (both its mines).

### Shocks placed where they matter (`placement-v2`)

The maintainer: "the placement is a bit too easy to ignore sometimes,
maybe we should prioritize adding some of them to lanes which already
have a note nearby." With `placement-v1` most shocks landed in lanes with
nothing near them, where a player who simply does not press never meets
them.

- **`placement-v2`** (`ddi_engine::battle::PLACEMENT_V2`, `Near`; what
  attacks use, `PLACEMENT`). Every margin of v1 is kept exactly (no shock
  from W3 before a step of its lane to 0.25 s after it, none during a
  hold or a roll nor within 0.25 s past its end, none from a second
  before a lift); what changed is the choice among the allowed places.
  A place is *next to a note* of its lane when it is from 0.25 s to
  0.5 s after one (its step, a hold's or a roll's end: a key still
  resting on the lane steps on it) or from W3 to twice W3 before a step
  (an early press steps on it; never before a lift, whose key is ready a
  second ahead). Of a section's eight shocks, six are aimed at such
  places where the section has them, on the sixteenth-note grid: one a
  beat, then a second, in the lane with the fewest so far, never two by
  one note, never two in a row, never two of a lane within the lock's
  second. The rest are placed as v1 places them (the eighths, then the
  sixteenths), so an attack still reads as a pattern.
- **Where a section has no room for that** (a chart whose free lanes are
  few: eight shocks in two lanes over four seconds cannot be a second
  apart), the remaining shocks ignore the second between a lane's
  shocks, as v1 always did; and where even that does not fit, the
  section is v1's own. So no section v1 has is lost, and a section with
  too little room for eight safe shocks is skipped as before.
- **Deterministic** as before: the same chart, section and seed give the
  same shocks (pure in them; nothing read from a clock or a frame), so
  the same at any frame rate and from a log. `placement-v1` and
  `placement-v0` stay readable through `schedule_with`.
- **Measured on the bundled four-lane charts** under DDI Normal
  (`cargo test -p ddi-engine --test battle_shocks -- --nocapture`,
  seed 7): all 1,496 sections of 1,496 slots under both; shocks next to
  a note of their lane 6,875 of 11,968 (57.4%) under v2 against 1,986
  (16.6%) under v1; pairs of one lane's shocks within a second 12 under
  v2 against 4,998 under v1. On the test's chart with room beside its
  notes throughout (steps, jumps, eighth runs, a hold, a roll, lifts) v2
  puts six of eight there (75%), never fewer than five in a section.
- **The attack-rate study again** (`cargo test --release -p ddi-session
  --lib attack_rates -- --ignored --nocapture`, six seeds, round 3's
  rules `attacks-v0`; `DDI_ATTACK_PLACEMENT=v1` for the older placement,
  whose run reproduces round 3's table to the digit). An attack
  received, v1 then v2:

  | Eight CPUs at one level, DDI Normal | Shocks stepped on (of 8) | Notes missed in locks | Life lost | Knocked out by it | Out of the stage |
  | --- | ---: | ---: | ---: | ---: | ---: |
  | Level 1 | 2.05 → 2.54 | 1.30 → 2.24 | 23.2 → 35.1 | 15.4% → 24.4% | 59.0% → 63.5% |
  | Level 3 | 1.77 → 2.20 | 1.48 → 2.19 | 23.6 → 32.9 | 5.3% → 8.6% | 24.7% → 30.9% |
  | Level 5 | 1.29 → 1.63 | 1.40 → 2.07 | 20.5 → 28.9 | 0.8% → 1.5% | 3.8% → 6.2% |
  | Level 7 | 1.10 → 1.21 | 1.24 → 1.58 | 17.9 → 21.8 | 0.1% → 0.1% | 0.7% → 0.3% |
  | Level 8 | 0.46 → 0.54 | 0.50 → 0.67 | 7.2 → 9.4 | 0.0% → 0.1% | 0.0% → 0.3% |
  | Level 10 | 0.14 → 0.18 | 0.17 → 0.23 | 2.4 → 3.2 | 0.0% → 0.0% | 0.0% → 0.0% |

  | A field of mixed levels | Attacks a stage | Shocks stepped on | Notes missed in locks | Life lost | Knocked out by it | Out of the stage |
  | --- | ---: | ---: | ---: | ---: | ---: | ---: |
  | Easy | 36.7 → 36.7 | 1.29 → 1.51 | 1.46 → 1.83 | 4.2 → 5.2 | 0% → 0% | 0% → 0% |
  | Normal | 26.7 → 25.8 | 1.12 → 1.31 | 1.09 → 1.53 | 16.5 → 21.8 | 3.4% → 5.9% | 22.6% → 29.9% |
  | Hard | 18.2 → 15.7 | 0.70 → 0.84 | 0.63 → 0.89 | 19.6 → 26.2 | 7.5% → 14.9% | 55.6% → 65.6% |

  What the numbers can tell: with the same CPUs and the same attacks, an
  attack costs about a third more life under v2 (16.5 to 21.8 under
  Normal), mostly through the locks (a shock stepped on beside a note
  locks the lane the next notes come down: 1.09 to 1.53 notes missed an
  attack), and knocks out nearly twice as often. What they cannot: how
  a person fares. A CPU's response to shocks (`MINE_RESPONSE_V0`) is
  invented and uncalibrated: a chance, by its level, of an extra press
  in each shock's window, and its own presses within a span of a shock
  moved a little or left out. Under v2 more of its presses are near a
  shock, so more are disturbed and some then land on the shock or miss
  their note: that is where the rise comes from. It models neither of
  the two things v2 aims at in a person (a key left resting after a
  step, a habit of pressing ahead of the beat). The direction is the
  placement's doing; the sizes are not a balance.
- Tests: `ddi-engine` `tests/battle_shocks.rs`
  `placement_v2_aims_most_shocks_next_to_a_note_of_their_lane` (the
  margins, the share, one a row, two a beat, the second between a lane's
  shocks, no section lost, pure), `what_the_placement_places_an_exact_play_avoids`
  (now on a second chart too: an exact play, its keys down a fifth of a
  second, steps on nothing v2 places), `bundled_charts_have_safe_sections`
  (the table above).
- A person's view (`target/scratch/ddi/r3b/look4.mjs`, `shots/i4-v1-*`
  before and `i4-v2-*` after, the same section of the same chart and
  seed put into the player's field by `mpdevmines`): Some Things Must's
  Challenge chart from beat 96 (dense: v1's shocks sit on the eighths
  between the lanes' steps, v2's a sixteenth off a step of their own
  lane) and Beatucada's Easy chart from beat 32 (sparse: v1's are far
  from any note, v2's follow or precede the few there are).

### No recovery period, and chained attacks (`attacks-v1`)

The maintainer: "About the lock after an attack, I think we can let the
recovery be even tighter, maybe even allowing chaining attacks (which
could be interesting in intense battles)."

- **`attacks-v1`** (`ddi_room::attacks::ATTACKS_V1`, what stages play:
  `ATTACKS`; `attacks-v0` kept by name). The bar, the cost and the
  checkpoints are v0's. **No recovery period**: an attack against
  someone is over once its last shock's window has passed (at the next
  checkpoint the ledger marks it cleared and recovered at once), not
  waiting for eight of their judgments, four beats, or the lane that
  shock locked. **Chaining** (`AttackRules::chain`, one): while an
  attack against someone is still on its way or passing, one more may
  be reserved against them, in the first safe section that starts after
  the one before it has cleared (and no sooner than their lead, as
  ever), so two attacks' shocks never share a section; never more than
  one waiting. A full bar whose every possible target has one passing
  and one waiting is `Blocked` (`NoEligible`) as before and launches at
  a later checkpoint, once someone's first is over; its policy picks
  among those who can be attacked, as before.
- **Settlement and expiry** (`SETTLEMENT_V0`, unchanged): a stage is
  decided only with no attack outstanding against the last one in, and
  a waiting chained attack is outstanding as any committed one is; when
  a target goes out, every attack against them not over yet expires,
  the waiting one too, nothing refunded.
- **What a player sees** (`ddi_shell::battle_cue`,
  `ddi_session::battle::Incoming`): the warning by the receptors is the
  nearer attack's; where one more waits behind it a second, smaller
  bolt stands at its upper right (`Incoming::more`, `Cue::more`); once
  the first's last shock has passed the warning is the next one's, and
  once every one has passed it goes. The shield that stood for the
  recovery is gone: there is none to wait for. No message spoke of
  recovery, so none changed; miniatures never had a recovery mark.
- **Tests**: `ddi-room` `attacks/tests.rs`
  `a_chain_lands_back_to_back_and_a_third_waits_for_the_first_to_pass`
  (three attackers against one target: the second reserved behind the
  first in the next section, the third blocked while one passes and one
  waits, launched once the first is over, behind the second),
  `a_chain_expires_whole_and_keeps_the_stage_from_settling`,
  `the_rules_are_named_and_v1_has_no_recovery_and_a_chain_of_one`,
  `attacks_v0_never_chains`; `ddi-session`
  `chained_attacks_are_delivered_back_to_back_into_plays_and_cpus` (a
  stage on a long chart against three strong CPUs: 2 of its 12 attacks
  chained, one against the player and one against a CPU, each after the
  one before it had cleared, all in their recipients' plays and fields,
  the record the same at 60 and 144 frames a second); `ddi-shell`
  `it_scales_in_dims_in_sight_and_marks_one_more_waiting`.
- Looked at in headless Chrome (`target/scratch/ddi/r3b/look5.mjs`,
  `shots/i5-*`): a stage of eight CPUs with Attacks on, on Some Things
  Must's Challenge chart (68 attacks): the plain warning, and at 82 s
  the warning with its second, smaller bolt while one attack against
  the player was on its way and one more waited behind it.
- **The attack-rate study again** (the same six charts and seeds,
  `placement-v2` under both; `DDI_ATTACK_RULES=v0` for the older
  rules). `attacks-v0` then `attacks-v1`:

  | A field of mixed levels | Attacks a stage | Shocks stepped on | Life lost an attack | Knocked out by it | Out of the stage |
  | --- | ---: | ---: | ---: | ---: | ---: |
  | Easy | 36.7 → 36.5 | 1.51 → 1.48 | 5.2 → 5.1 | 0% → 0% | 0% → 0% |
  | Normal | 25.8 → 25.8 | 1.31 → 1.23 | 21.8 → 21.2 | 5.9% → 5.9% | 29.9% → 30.2% |
  | Hard | 15.7 → 14.8 | 0.84 → 0.88 | 26.2 → 26.2 | 14.9% → 16.4% | 65.6% → 66.7% |

  Eight CPUs at one level under Normal move as little (level 5: 3.73 to
  3.62 launched a seat a stage, knocked out by an attack 1.5% to 3.6%;
  level 10: 6.31 to 6.34). Against round 3's table (`attacks-v0`,
  `placement-v1`: 26.7 attacks a stage, 16.5 life an attack, 3.4%
  knocked out under Normal) the whole difference is the placement's.
  What it says: in a field of eight a full bar nearly always found
  someone not under attack already, so recovery was seldom what held it
  back; the new rules matter where the field is small (a duel, the last
  two of a stage), which this study does not run, and to a person as a
  second attack arriving right behind the first. A duel's rates were
  not measured.

## M8 round 3: personal targeting, who attacks whom, the attack lines and the results (2026-11-12)

The third round of the attack work, for where attacks work: CPU
practice with the match's Attacks rule On (the default since this
round, below; an online room does not offer it). Everything here is real unless it says
otherwise; the room's model carries what online attacks would need of
it, and nothing here asks the server to behave differently.

### Attacks On by default

The maintainer, 2026-11-12, after testing them: "I've been testing
attacks and I think it's working really well, we can switch it to On by
default." This reverses "Off by default" in the DDI ruleset's round 3
above and is D8's first sentence again.

- **Where the default lives**: in the practice room's rules.
  `MatchRules::practice` names the battle with attacks
  (`gauge-attacks-v0`), so a new practice room, the Rules pop-up's Reset
  and every test that starts from practice's rules have attacks, and
  anyone turns the rule Off in Rules as before. A hosted room's rules
  (`MatchRules::hosted`) are untouched: they name no battle with attacks
  (`provisional-standard`), so an online room has none whoever made it,
  an older client and an older server included (the model's serde and
  protocol did not change).
- **What is played follows what is offered, never the rule alone**:
  `RoomClient::attacks_offered` (not an online room) and
  `attacks_played(rules)` (the rule, where offered) decide the stage's
  battle in its order (`StageOrder::battle`: with attacks only where
  they are played, survival otherwise, whatever id the rules carry), the
  policies' rows, the standings' attack columns and the debug state's
  `attacks`. An online room whose rules were sent the practice battle's
  id still plays survival and says nothing of attacks but its Rules
  row's "Off · not in online rooms yet" (test
  `state::online::tests`, the pick's order).
- **Summaries say what differs from the default**, so now "No attacks"
  where a practice match has them turned off and nothing where it has
  them: the Rules pop-up's locked line ("4 stages · 3 CPUs · DDI · Easy
  timing · Boos · No attacks") and the stage's prompt ("Rules: DDI · No
  attacks"); Japanese 「攻撃なし」. Where attacks are not offered (an
  online room) nothing is said either way (`room::rules_words`,
  `StageOrder::attacks_rule`).
- **Switches**: desktop's `--mp-attacks` (and `--mp-dev-attacks`) and
  the web's `mpattacks=1` are still taken and now change nothing; the
  web's `mpattacks=0` starts a practice room with the rule Off, for
  tests and screenshots (`Development::attacks`, an `Option` now).
  Desktop got no opposite flag: its tests set the rule.
- Tests: `ddi-room` `rules`
  `attacks_are_a_match_rule_that_chooses_the_battle`; `ddi-ui`
  `the_rules_summary_says_no_attacks_only_where_they_are_turned_off`,
  the Rules pop-up's rows (On first, Reset back to On, the locked line),
  the client's first order (`gauge-attacks-v0`), the online room's
  (survival, nothing said); `ddi-session`'s survival stages turn the
  rule off in their room (`room_seeded`), its attack stages leave it.
  Singleplayer is untouched (the smoke check).

### Personal targeting (S6)

- **Each competitor's own policy** (D8): Spread (the default),
  Retaliate, Leader, as `ddi_room::attacks::Targeting` has them. In the
  room's model: `Member::targeting` and `Command::SetTargeting` /
  `Event::TargetingChanged` (additive: the field left out of the JSON at
  its default, protocol 2 unchanged; a peer that predates them reads and
  writes the same room so long as nobody sends the command, and nothing
  sends it where attacks are not offered). A member sets their own alone
  (no participant in the command; the authority's and a CPU's are
  refused, a departed member's too), outside a stage's play: the lobby,
  between stages, while a stage gathers readiness and through its
  countdown. During a stage's play and its song's completion the
  command is refused (`WrongPhase`): the play itself carries a change,
  and the room is told afterwards what the play left the policy at.
  **It clears nobody's readiness** (the plan said it would): it is
  nobody else's rule, and since D8 it may change in play anyway, so
  un-readying a player for it on the prompt would only cost a press.
- **A CPU's policy**: `OpponentSpec::targeting`, optional (Spread where
  none). Eight of the roster's twenty-four name one
  (`crates/ui/src/state/cpu_roster.json`: five Retaliate, three Leader;
  PROVISIONAL, for review with the names). Shown as a policy on its
  line, nowhere as a skill.
- **Before the stage**: the stage's prompt, where the match has attacks,
  has one quiet row under the diagram, "Targeting ◀ Spread ▶", and a
  line saying what the policy does ("Your attacks go to whoever has been
  attacked least."). Up from Ready reaches it; Left and Right, a tap on
  its arrows, a tap on the row and Enter change it, in a ring. The
  players panel on the room's song select and the Players pop-up say
  each player's policy as one faint word (the panel before the status,
  the pop-up under it) while the match has attacks; nothing of it
  without.
- **During the play** (D8): one small muted word under the life gauge's
  left end, the policy (the waiting line keeps the right end and now
  says only "Waiting for target"). It never flashes or moves; under
  Reverse it stays with the bars. It cycles Spread, Retaliate, Leader
  on Tab (unless the player's bindings give Tab to a lane: the lane
  keeps it), on a tap or a click on the label or on the left of the two
  bars above it, and on a controller's View/Select. That button is also
  the one that quits a play (held, or pressed twice), so where the
  policy cycles (a stage with attacks, the player still in, the stage
  not won) a short press cycles it on its release, holding it quits as
  ever, and pressing it twice does not quit (two quick presses are two
  steps, never a forfeit); the quit's hint then says "hold View to
  quit" and shows only once the press is held past a quarter second.
  Escape, Backspace and the quit button are unchanged. PROVISIONAL
  (`policy-pad-v0`).
- **From the next checkpoint, and in the record**: a change is kept in
  the play's log at its judged song second (`PlayLog::policies`, never
  before the time the play was last judged at, as an edge's; binary
  version 5, written only for a play that has one, versions 1 to 4 read
  and written as before) and given to the attack runner, which reports
  the policy as it stood just before each checkpoint
  (`attacks::Report::targeting`); the ledger launches by it from that
  checkpoint on and records `AttackEvent::Retargeted`. So the record is
  the same at 60 and 144 frames a second and again from the log. Stage
  plays are still not kept in the history, so no kept log has needed
  version 5 yet; it exists so that a stage's log, where one is kept or
  sent, judges again to the same attacks. The play starts with the
  policy the room has for the player as it starts (the shells read it
  then, after the prompt), and the room keeps what the play left it at
  (`StageOutcome::targeting`, `RoomClient::played_targeting`).
- **Debug**: `__DDI_DEBUG.ui.theme.room.targeting` and each member's
  `targeting`; in a play `__DDI_DEBUG.battle.attacks.policy` (as the
  label says it), `policy_at` (where a press on it lands, CSS pixels),
  `policy_ledger` (what the ledger launches by) and `policy_changes`
  (the log's).
- **Tests**: `ddi-room` `attacks/tests.rs`
  `a_policy_changed_in_play_applies_from_the_next_checkpoint`;
  `tests/walkthroughs.rs`
  `a_member_sets_their_own_attack_policy_outside_a_stage` (the lobby,
  readiness kept, the authority and a CPU refused, a roster CPU's own,
  refused during the play, taken between stages, refused once gone);
  `tests/online.rs`
  `a_members_attack_policy_is_kept_alike_by_both_authorities_and_every_mirror`
  (`LocalAuthority` and `OnlineAuthority` + `FakeServer`, the JSON's
  default left out and read back); `ddi-engine` `history`
  `policy_changes_round_trip_as_version_5`; `ddi-session`
  `a_policy_changed_in_play_applies_from_the_next_checkpoint_at_any_frame_rate_and_from_the_log`
  (Tab, the label, a pad's View twice; the record equal at 60 and 144
  frames a second and from the log; a held View quits) and
  `a_lane_s_key_is_never_the_policy_s`; `ddi-ui` `room_tests`
  `the_prompt_has_the_attack_policy_where_the_match_has_attacks`,
  `the_players_rows_say_each_policy_where_the_match_has_attacks`, the
  roster's policies, the `qps-ploc` walk; `ddi-shell` `battle_cue`
  `the_policy_label_is_under_the_gauge_inside_the_field` (inside the
  field's width, clear of the quit button and the warning at 1280×720,
  1920×1080, 844×390, 390×844 and a doubled 2560×1440, with and without
  Reverse); `ddi-desktop` `room_stage_flow`
  `a_practice_stage_with_attacks_launches_them_through_the_window` (the
  prompt's row by keys, the play started with it, the label pressed,
  the room told afterwards).
- **Found, not changed**: the singleplayer prompt's play options rows
  have the same ◀ ▶ as this row, and a tap on their ◀ steps forward: the
  arrows' boxes are drawn clear, a clear box takes no press, and the
  press lands on the row, which cycles forward
  (`crates/ui/src/screens/prompt.rs`, the `arrow` closure). The new row
  draws its boxes all but clear instead. Left for the maintainer: it is
  singleplayer's behavior.

## Provisional choices

| Choice | Where | User-visible effect | Decision needed |
| --- | --- | --- | --- |
| A report's detail (`RunDetail`) is self-reported and shown as sent, bounded only in size | `crates/room/src/battle.rs`, `reduce.rs` | An online player's grade, combo and judgments on the standings are what their device said | Whether the room should check a detail against the run, or wait for server-judged plays |
| The stage's standings seen on its results count as seen: no pop-up back on song select | `results_stage.rs` `stage_update` | One screen fewer after each stage for those who saw the standings | Keep, or always pop the standings up |
| The results' tab is remembered for the session, the last stage always opening on the standings | `results_stage.rs` `CHOSEN` | A player who prefers the graphs sees them after middle stages | Whether it should be a saved setting, or always the standings |
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
| Practice's default level is level 8 of ten (the engine's `normal`; "level 3 of 5" before feedback round A), seeds from the time the room was made | `state::room::DEFAULT_CPU_LEVEL`, `PracticeRoom::new` | Add CPUs starts at three gold and two silver stars; two rooms differ in their CPUs' seeds | Practice's default profile; whether the seed should be shown or fixed |
| CPU levels `cpu-levels-v0`: ten, the last five the frozen strengths 0.75 to 1.25, the first five strength and lapse 0.40/0.20, 0.47/0.19, 0.54/0.16, 0.61/0.12, 0.68/0.07, chosen by measurement over local arcade packs, never against people | `ddi_engine::cpu::CPU_LEVELS_V0` | How weak a one-star CPU is: about 40% under ITG FA+ over the arcade charts, out of two stages in three; five silver stars about 77%, out of one in twenty-five | The levels' spacing; whether the five strengths should be respaced (they span 86% to 97% and never go out); calibration against people |
| A chart is playable by the CPUs when its style is a built-in one-player layout with four lanes; song-declared layouts are refused | `state::room::eligible` | Solo, Double, Pump, techno and DanOni 5–9 charts, and a song's own four-lane layout, are refused with a reason | Whether to accept song-declared four-lane layouts before M3's load check |
| Chosen local content counts as loaded at once | `PracticeRoom::sync_loading` | Ready alone makes the player eligible | Replaced in M3 by the real load and plan check |
| Attacks `attacks-v0`: a commitment looks 16 beats past its earliest start; recovery after eight of the recipient's judgments past the attack's last shock and four beats | `ddi_room::attacks::ATTACKS_V0` | How long a full bar waits for a place, how soon someone can be attacked again | The numbers, once played |
| Lead `lead-v0`: the target's reading horizon + 1.5 s warning + 0.1 s, at least the next checkpoint (four beats), at most 10 s; slower settings raised on a stage | `ddi_room::attacks::LEAD_V0`, `ddi_session::battle::raised_speed` | An attack's shocks never appear before the warning has shown 1.5 s; a very slow player plays a stage faster (said on the prompt) | Whether online adds a network budget (D5 says synchronized starts instead) |
| A CPU's lead from the local player's horizon (`local-player-horizon-v0`) | `ddi_session::battle::CPU_HORIZON` | Shocks on a miniature come in from its far end | Whether CPUs should have a horizon of their own |
| Targeting ties: seeded by stage, checkpoint, sender and candidate; Leader's tie-break the timing points earned (cap aside) standing in for score | `StageBattle::launch` | Who gets attacked when several are equal | Whether Leader should use the real score at the checkpoint |
| Launch order turns each checkpoint (`k mod n` in participant order) | `StageBattle::launch` | With several full bars at once, who picks first | None unless play shows a bias |
| Settlement `settlement-v0`: decided only without an outstanding attack against the last one in; a knockout while waiting shares first with those out in the same checkpoint interval as the one before | `ddi_room::attacks::SETTLEMENT_V0` | "Stage won" waits for the last attack to pass; "Drawn: final attack" | As decided (D6) |
| A stage with the player gone is played out a checkpoint at a time (CPUs, their attacks, settled as they come) | `ddi_session::battle::Battle::play_out` | The ranking of the CPUs after the player leaves includes their attacks on one another | None until watching exists |
| CPU shock response `mine-response-v0` (uncalibrated; extended in feedback round A): avoidance from level (1: 0, then a fifteenth more each to 5; 6–7: 1/3, 8–9: 2/3, 10: 1); at 0 a CPU steps on 30% of shocks, its presses within 0.3 s of one moved ±65 ms and 8% left out; at 1/3 14%, ±45 ms, 4%; at 2/3 6%, ±25 ms, 1.5%; at 1 2%, ±12 ms, 0.5% | `ddi_engine::cpu::MINE_RESPONSE_V0` | How often a CPU's life drops to an attack, and how much its timing suffers near one | Calibration against people; whether avoidance joins `Skills` |
| Battle `gauge-survival-v0` (`ddi_room::battle::BattlePreset::GaugeSurvivalV0`, practice's `MatchRules::battle`): life is each competitor's gauge under the match's ruleset (M8a; before, the player's own), empty is out of the stage | `crates/room/src/battle.rs`, `crates/session/src/battle.rs` | A stage is survival under the match's ruleset's gauge; Rules says "Survival · provisional" | The standard battle (life, power) and its numbers |
| Ranking: still in by life, then the proportional score (DDI's points over the most the notes played give; another ruleset's percent, money or EX over its most), then among those who played the same notes DDI's absolute score (its W0s); then the out, later first; then forfeits; exact ties share (the DDI ruleset's round 3) | `ddi_room::battle::rank`, `Absolute` | Placements follow life first; a CPU's score is of its tap-only copy, the player's of the whole chart, each a share of its own most; a tie between a person and a CPU is not broken | Ranking survivors; whether CPUs should play the full chart |
| Decided once at most one competitor is still in; the last out together share first; the winner's fail is turned off to the song's end | `battle::decided`, `PlaySession::tick` | "Stage won" in the miniatures' status line; the player finishes the song whatever the gauge does after | Final-contender settlement once attacks exist |
| The player out: the play ends as a failed play does, the remaining CPUs played to their end at once | `Battle::outcome` | No watching after elimination; the results come after the fail's hold | The spectator view and when the eliminated return to the room |
| A quit, an interruption (page hidden, audio stopped) or Back on a stage's prompt is a forfeit; a song unreadable at play time stops the match (`Interruption::ContentLost`) | `PlaySession::tick`, shells | Losing focus mid-stage costs the stage | Local interruption versus forfeit; reconnection |
| A stage plays with no turns, note removals or rate, the play options row not offered, no ghost, no singleplayer countdown | `SessionConfig::player_spec`, prompt | The player's other settings (speed, skin) apply; the rules are the match's (M8a) | Which personal options a stage allows |
| Stage plays are not kept in the play history or bests | both shells' record paths | A practice match leaves no history | Whether a stage counts as a personal play |
| Battle voice `battle-voice-v0`: priorities outcomes/out 40, (attacks 30, none yet), life critical 20, players left 10, openings and the finished song 5; one battle line at a time, a lesser or equal one passed over, never queued | `ddi_session::announcer::battle::BATTLE_VOICE_V0` | Which battle line speaks when two meet | The battle voice's priorities and whether a line may cut another (needs a voice bus that can stop one sound) |
| A stage leaves out a play's routine lines (combos, comments, life lost, last life, full combos, cleared, failed, grades, records, cheer) | `battle::stage_speaks`, `Announcer::tick` in a stage | A pack without battle lines says only ready, here we go, failed (out) and cleared (finished after a win) in a stage | Whether some routine lines (combo milestones) belong in a battle voice preset |
| A moment shows only where its own line was not said; for 2.5 s with a 0.25 s fade; in the status line under the right side's miniatures (M7c), beside the watermark where the sides are too narrow for words (feedback round A) | `VoicePolicy::cue_seconds`, `cue_fade`, `ddi_shell::opponents` | A phone held upright shows the cue on the watermark's row, over the field's far end | Whether cues show always, and where on a phone |
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
| Miniatures (feedback rounds A and C): lane at most half the play's (`MINI_LANE_SHARE` 0.5; 75% and 96 px before round C), at least 8 px for a room's eight (16 px beyond the cap, then turns); as many lanes tall as the play's field, eight to twelve (six to ten before round C), receptors a lane and a half from their end; between slots 4% of the side's width (4 to 10 px), from the field's band 8% (4 to 20 px), from the screen's edge 4% (3 to 10 px); name line 9 to 14 px | `ddi_shell::opponents` constants (`MINI_LANE_MIN`, `MINI_LANE_SHARE`, `MINI_ROWS_MIN`, …) | Eight miniatures at 1280×720 have 28 px lanes on fields ten lanes tall (2×2 each side), 46 px at 1920×1080, 16 px on a phone held sideways, 13 px in the margins of a phone held upright; one to four are at the cap, 36 px at 1280×720 | The miniatures' size, density and placement; the cap's share; whether 13 px lanes are worth showing on a phone |
| First half of the seats on the left, the rest on the right, each side its own height; in a stage the dancing hand is behind the field, whole, in the band from past the receptors' row to the bottom (the top under Reverse), as large as a solo play's frame's proportion fits there (`BEHIND_FIELD_TALL` 1.13) up to `BEHIND_FIELD_ZOOM` 2.0 against the band's width, with no fade (round C; under the judgment at 1.9 with its top quarter fading in before) | `opponents::arrange`, `play_view::behind_field`, `dance::behind_field_distance` | The whole hand at 0.79 of its solo size behind the judgment, the combo and the notes, darkened by the band | The hand's size (clear of the receptors, or a solo play's size behind them too), its dimming |
| Every miniature at the local player's speed and direction | `PlayView::draw` | Opponents' fields read like the player's own, not like theirs | Their own display settings once a remote stream carries them |
| A judgment shows 0.4 s; a skin's explosion components drawn at 0.4 opacity on a miniature; a skin without them gets a soft glow of the judgment's color | `feed::FLASH_SECONDS`, `noteskin::MINIATURE_HIT_ALPHA`, `opponents::draw_fields` | Infinito's W1 burst is a faint flash on a miniature; Peters-DDR's miniatures glow instead of exploding | The miniature's hit effect |
| An actor-drawn receptor is drawn as it rested a second into the play | `NoteskinPlay::rest_receptors` | StepMania skins' miniature receptors do not press | None expected (no Lua per miniature) |
| Life, combo and moments drawn after a presentation delay, 0 for local CPUs | `feed::LOCAL_DELAY`, `FieldFeed::new` | None locally | The online delay and how it is corrected |
| A miniature's combo never below 0.3 of the play's scale | `hud::MINI_COMBO_MIN_SCALE` | Below 22 px lanes the combo is larger than its field's scale | Legibility against faithfulness at small sizes |
| Nine players: `ExperimentalNineV0`, the eight-player table and 0 for ninth | `ddi_room::awards` | A nine-player stage's ninth earns nothing, the first eight as in an eight-player field; marked experimental | The award rule for nine (and whether ninth should earn something) |
| Development only: practice beyond the cap (129), linear points for any field; past what fits, the opponents take turns in the slots, `ROTATION_V0` (8 s a turn, 0.4 s darkening, none under reduced motion), each always in its own slot, priority seats never rotating (none yet) | `RoomPolicy::development`, `AwardScheme::DevelopmentV0`, `opponents::rotate`, `opponents::priority_seats` | Only through `mpcpus=` / `--mp-dev-cpus` | The large-party density policy |
| Lyrics are drawn over a stage's miniatures where a long line reaches past the field | `PlayView::draw` | A song with lyrics keeps its miniatures; a wide line may cross a miniature for its moment | Whether lyrics should narrow to the field in a stage |
| Spotlight rest `spotlight-rest-v0`: 3 s before the host's resting song is told to the room (and, online, uploaded) | `state::room::SPOTLIGHT_REST_MS` | Guests see a song three seconds after the host stops on it; scrolling sends nothing | The rest's length; whether to upload on rest or only show the card |
| Readiness `hosted-deadline-v1`: 30 s from the song's availability (the server's `ROOM_READINESS_WINDOW_MS`, 15 s in its `wrangler.toml`, overrides), early start once every connected candidate is ready and loaded | `rules::ReadinessPolicy` | A stage starts when the last person readies; a disconnected one waits for the deadline | The window's length; whether a reconnecting member should hold the early start |
| The prompt goes back 2.5 s after the room lets go of it (sat out, canceled) | `prompt_room.rs` `GONE_SECONDS` | The line is readable, then song select | None expected |
| Add random draws any unseated catalog CPU (`RosterAim::Any`) | `state::cpu_roster` | A practice field's levels are mixed | The balanced aim (a level near the people's) |
| Practice starts with three CPUs drawn from the room's seed | `state::room::STARTING_CPUS` | Every practice room has a different three | Fixed starters, or the last roster kept |
| A pick's readiness and the countdown: the host's Back on the prompt cancels the window (back to picking); a member's withdraws only themself | `RoomClient::leave_prompt` | The host can change their mind before the start | Whether the host's Back should keep the window for the others |
| Desktop's session in the config folder's `online-session`, owner-only on Unix, plain text | `crates/desktop/src/online.rs` | Signed in across runs until the session's thirty days | The system keychain instead |
| The match's ruleset is DDI alone (the DDI ruleset's round 3; `itg-fa+` and six more until then, whose ids are still read and played as DDI), its Difficulty (Normal by default) and Boos (Off) match rules; never Practice; fail immediate | `ddi_room::MatchRules::{ruleset, difficulty, boos}`, `ruleset_id`, `ddi_session::battle::stage_ruleset` | Everyone's stage plays DDI under the match's difficulty and Boos, whatever their own options say | The defaults; whether a match should be able to mix difficulties (a handicap) |
| `attacks-v0`: a bar of 100, an attack 100, checkpoints every four beats from beat 4 to four beats past the last note; overflow lost | `ddi_room::attacks::ATTACKS_V0`, `AttackRunner::new` | The bar moves in steps every four beats and fills from about 50 best-window notes | The bar's size and the checkpoint's beat count |
| `power-classes-v0`: the best window 2, the next 1, the rest 0, for every match ruleset (D2); DDI's W0 and W1 2 (±16.67 ms on Normal), W2 1 | `ddi_room::attacks::POWER_CLASSES_V0` | A bar fills from 50 notes in the top class: a strong CPU fills about three and a half a minute on the bundled charts, a one-star CPU one (the round's tables) | The classes, the bar's size and an attack's cost, from the measured rates |
| Attacks are a match rule, On by default where they are offered (CPU practice: `gauge-attacks-v0`; Off by default until M8 round 3); an online room shows them as not there yet and plays none whatever its rules say | `ddi_room::MatchRules::practice`, `attacks`, `ddi_ui`'s `RoomClient::attacks_played`, `screens::room::attacks_offered`; conveniences `mpattacks=0` (Off), `--mp-attacks` and `mpattacks=1` (redundant) | A practice match has attacks unless someone turns them off in Rules; an online host cannot turn them on | Online attacks |
| A shock is a mine under DDI: −5 life on Normal (−1 Easy, −10 Hard), −10 points, the combo, the floors, the lamp, and its lane locked a second; `injected-mine-v0` (the ruleset's miss to the gauge alone) only under a ruleset without the lock, which no room plays | `ddi_engine::Player::shock_is_mine`, `ddi_engine::battle::INJECTED_MINE_V0` | A shock stepped on costs what a chart's mine costs and a second of its lane | The lock's second and the mine's cost (DDI's *tune* values) |
| `placement-v1`: eight shocks over eight beats from a checkpoint, the eighths then the sixteenths, at most two a beat, one a row; margins about each step of the lane: none within 0.25 s after it nor within W3 before it, none from a hold's or roll's head to 0.25 s past its end, none from 1 s before a lift (`placement-v0`'s clearances by windows kept for comparison) | `ddi_engine::battle::PLACEMENT`, `Clearance::Margins` | Where the shocks fall: a shock may sit a sixteenth before a note of its lane from 150 BPM on, where a press more than half of W3 early steps on it | The 0.25 s; whether "before" should be W3 plus the mine window; whether sixteenths are acceptable |
| Development trigger: the first safe section from a beat in each practice stage, its seed the stage's id | `--mp-dev-mines`, `mpdevmines=`, `ddi_session::battle::dev_shocks` | Only through the switch | Replaced by the attack loop (S4) |
| A miniature's score: small (0.8 of its name line, 8 to 11 px; down to 6 px where the sum is wider than its field) under its life gauge's right end; the score (the sum) or the percent, by the player's Score display | `ddi_shell::opponents::ScoreShown`, `SCORE_SIZE`, `SCORE_FLOOR`, `SCORE_TOP` | At lanes under about 17 px the number's top touches the receptors' row | Its place and size |
| `attacks-v1`: no recovery period; one more attack may be reserved behind the one on its way or passing, in the next section after it clears | `ddi_room::attacks::ATTACKS_V1`, `AttackRules::chain` | Two attacks can arrive back to back; nobody is safe for a while after one | Whether a chain of one is right, and whether a lane still locked by the last shock should hold the next attack off |
| `placement-v2`: six of an attack's eight shocks next to a note of their lane (0.25 s to 0.5 s after one, W3 to twice W3 before a step), a second between a lane's shocks | `ddi_engine::battle::PLACEMENT_V2`, `Near` | An attack is harder to ignore: about a third more life lost by the CPUs an attack, mostly through locks | The window's sizes, the aim (six of eight), whether W3 before a step is too close for early players |
| A CPU's (and a bot's) score put on the chart's own scale: its points as the same share of the chart's most, its W0s as counted | `ddi_engine::rules::ddi::CPU_SCORE_V0`, `DdiScoreView::on_scale`, `ddi_session::battle::cpu_score` | A CPU's number beside the player's reads as if it had held the holds as well as it tapped; a CPU can tie a person | Whether imputing the holds is right, or CPUs should play (and be able to drop) holds |
| A miniature's locked lane: the lane darkened (0.42) to its end and its receptor's place tinted violet (0.5) for the lock's second, from the feed's moment | `ddi_shell::opponents::LOCK_LANE`, `LOCK_TINT`, `FieldFeed::locks` | A CPU that stepped on a shock shows a dead lane for a second | The look; a shock's own art |

## Open questions

- The DDI ruleset's round 3: in a stage the player's own HUD shows its
  Score display's number (under Score: the proportional part plus the
  absolute score, about twice an opponent's number early in a song and
  more later), while the miniatures and the Standings show the
  proportional part alone, the one that compares. Should the HUD show
  the proportional part in a stage too?
- The DDI ruleset's round 3: `placement-v1` keeps a shock W3 before a
  step of its lane, as decided. From 150 BPM a sixteenth is 0.1 s, so a
  shock can sit a sixteenth before a note in the same lane; a press more
  than half of W3 early (46 ms on Normal) is then nearer the shock and
  steps on it (the lane locks and the note is missed). Avoidable, but a
  trap for early players. Keep W3, or W3 plus the mine window (167 ms on
  Normal)?
- The DDI ruleset's round 3: DDI Normal is much gentler on the weak
  CPU levels than ITG FA+ was (a one-star CPU out of one stage in five,
  not two in three; under Easy nobody ever goes out; Hard is about
  where FA+ was). Is Normal the right default for rooms, and should the
  levels be respaced for DDI?
- The DDI ruleset's round 3: an attack costs a CPU 2 to 24 life on
  Normal by its level, about half of it through the locks; a strong CPU
  launches six a stage. Are the bar (100), the cost (100) and the
  classes (2, 1) right, now that a shock is −5 and a lock?
- The DDI ruleset's round 3: a tie in life and proportional score
  between a person and a CPU is not broken (their notes differ). Fine,
  or should CPUs play the whole chart first?
- The DDI ruleset's round 3: online attacks (the room running the
  ledger from its members' reports, shocks delivered to each member's
  play in time) are not built; the Rules say so. When?
- The DDI ruleset's round 3: an attack clears (for the settlement, the
  recovery and the warning's shield) once its last shock's window has
  passed, but a lane that shock locked stays locked a second more and
  may still cost a note. Should an attack clear only after its last
  lock, so that "Stage won" and a final attack's draw wait for it?
- The DDI ruleset's round 3: shocks are still drawn as the skin's
  mines; a shock's own art (and the lock's look on a miniature, a dark
  lane and a violet receptor of ours) wait for the noteskin work.
- Feedback round B, FOUND WHILE VALIDATING (SOLVED in the DDI
  ruleset's round 3: the UI core's idle wake lost its wake-up to
  rounding; see that round's section): in an online
  room under reduced motion (headless Chrome, three people), a player's
  stage prompt twice stopped at "Stage n starts in 3…" as the countdown
  began: the page drew and ticked nothing more (its room stayed in
  `countdown`), the others played the stage, and the room ranked the
  stalled one a forfeit at its deadline. Two of five runs of this
  round's builds (once the host, once a guest), none in one run with
  normal motion and none in one run of the build before this round
  under reduced motion: too few runs to say whether this round brought
  it or it was there (reduced motion draws on demand and depends on the
  screen's 0.1 s wake-up; M7b's notes saw a still headless Firefox page
  "catch up at the next input"). The dumps:
  `target/scratch/mp/fb-b/online-stall.log`, `online-stall2.log`;
  `online.mjs` now nudges a stalled page and says whether it catches
  up. CPU practice never stalled (dozens of runs under reduced motion).
  Who should look: the prompt's host on the play route
  (`crates/web/src/components/ui_dev.rs`'s `schedule`: a frame asked
  for while the host has no surface, or a frame's callback that finds
  the host borrowed, leaves no wake-up behind).
- Feedback round B: the room's wide song select has no ladder, so a
  chart's best, notes and step artist of the charts not chosen are not
  shown in a room (the chosen chart's are, in words). Enough, or should
  the Players pop-up or a second line bring them back?
- Feedback round B: a follower's list's place holds only the status and
  the match's picks now (the players moved beside the song), mostly
  empty at 1280×720. What should it hold: the host's list read-only, the
  last stage's standings, chat later?
- Feedback round B: a phone's host sees the players as initials on one
  line. Readable, or should a phone's host trade list rows for names?
- Feedback round B: the players panel takes no focus and its lines open
  the same pop-up; per-player actions (roles, removing someone) are not
  built. Should a line be focusable once they are, or stay a tap?
- Feedback round B: the stage's standings show the grade in the lists'
  short form and FA+'s star grades smaller. Should a failed or out
  player's grade show at all beside "Out 1:02"?
- Feedback round B: with nine starters and the match's result the lines
  are 27 px and the pointed line's counts give way. Should the last
  stage's result be a third tab or a banner that folds, to keep the
  counts?
- Feedback round B: the standings count as seen once shown on the
  results, so song select's pop-up is skipped. Is that wanted, and
  should the pop-up go altogether now?

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
  be dimmed further? (Round C: the fade went and the hand is whole,
  behind the judgment and the combo too, which the maintainer accepted.)
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
- M8b: NoSafeSection waits gather at a song's tail (no section starts in
  its last eight beats while checkpoints run four beats past its last
  note): should a full bar at the end be told it will not be spent, or
  power stop being earned there?
- M8b: a CPU's lead uses the local player's horizon (so its miniature
  shows its shocks scrolling in); should CPUs have a horizon of their
  own (or none) once watching exists?
- M8b: Leader breaks life ties by timing points earned (the cap aside),
  standing in for score; the real score at the checkpoint would need
  each play's score history. Good enough?
- M8b: an attack against the player stays "incoming" in the ledger
  until the next checkpoint after its clear; the warning turns into the
  shield at the clear itself. Should the shield show the recovery's
  progress (eight judgments and four beats)?
- M8b: the incoming warning sits at the field's right edge, and the
  right side's miniatures keep clear of it (beside it, or under it on a
  phone held upright). Left or right, and should it move to the side
  away from the miniatures?
- Feedback round A: the five strengths (levels 6 to 10) are close
  together (86% to 97% under ITG FA+) and never go out on any chart,
  while level 5 is out of a quarter of the arcade Challenge charts and of
  every stream-pack chart: the largest step is between 5 and 6 on hard
  charts. Respace the upper five (which changes the solo CPU's named
  strengths and existing seeds' plays), or give them a small lapse too?
- Feedback round A: under ITG FA+ a weak level's stage often ends in the
  first measures (a gauge starts at half, a miss costs a tenth). Should
  a stage's gauge start fuller, or weak levels lapse less early on?
- Feedback round A: gold and silver stars differ by hue more than by
  brightness. Enough for every player, or should gold stars also differ
  in shape or size?
- Feedback round A: on a phone held upright the eight miniatures have
  lanes of 11 px and names of six or seven letters. Readable enough, or
  should a phone show fewer, larger ones (who attacks, who leads)?
- Feedback round A: eight miniatures at 1280×720 have fields six lanes
  tall (a short look ahead) for lanes of 42 px. Larger arrows on a
  shorter field, or smaller on a taller one? (Answered in round C: the
  play's field's proportion, ten lanes tall, lanes of 28 px, and a cap
  of half the play's lane.)
- M8b: an attack stepped on whole costs about 80% of an ITG FA+ gauge
  (eight shocks, each the ruleset's miss, D1). Is that the intended
  weight, now that every attack is all eight (D4)?

