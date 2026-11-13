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
disconnect. A studio scene for battle lines; the import report's
per-place counts do not count battle lines separately. (Recorded battle
lines in our packs: done 2026-11-12, below.)

**Our packs say the battle's lines** (2026-11-12, the announcer round):
`tools/announcer-design/pack.toml` and `pack-ja.toml` have all sixteen
(the fourteen above and the attacks' two), two to four short texts
each, 46 sounds in English and 45 in Japanese, said after a lead of
their own place (`battle` in `kokoro.py`'s `PLACES`; `Mr. Clank`'s
"Players, the battle is on!"; `ガキンさん` has one lead for every
line), made with the packs' other sounds through the game's tidy
(`docs/plans/announcers.md`, "Louder packs") and bundled. So with
`Mr. Clank` or `ガキンさん` chosen a practice stage is told by voice:
the opening in ready's place, here we go, the attacks against the
player and their own, their life running low, who is left, out, the
stage's and the match's ends; the status line then shows only a moment
passed over for a line that mattered as much. Checked through the
window offscreen: `crates/desktop/tests/room_stage_voice.rs`
(`our_announcers_say_the_battle_lines_in_a_practice_stage`: both, a
stage with attacks, every attack against the player said, nothing but
battle lines and here we go spoken).

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
  skill-tuned player and plays nothing but taps. `--level A-B`
  (2026-11-13) spreads the bots over that inclusive range: with as many
  bots as levels every level is used, with fewer each takes a distinct
  one; which bot gets which is drawn from `--seed` (the run's first line
  says each bot's level).
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
- **A consequence for the miniatures**: a practice stage now keeps the
  attack warning's place clear beside the field by default, so eight
  miniatures at 1280×720 have lanes of 25 px, not the 28 of a stage
  without attacks (feedback round C's table). The desktop's miniature
  scenarios (`opponent_fields_offscreen`) turn the rule off to keep
  measuring the miniatures' own sizes; one of them
  (`eight_cpus_get_miniature_fields_beside_the_play`) failed between
  the commit that changed the default and this round's last commit.

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

### Who attacks whom (S5)

Only the local player's own relationships are drawn large; CPU against
CPU stays what it was (shocks in their fields, the lock's look).

- **What the session tells** (`ddi_session::battle::Battle::links`):
  each attack the player sent or was sent, from the frame its checkpoint
  ran until it has cleared: the opponent's seat, the direction, when
  this play learned of it (what the drawing is timed from), its first
  shock and its clear, and whether it waits behind another against the
  same recipient (a chain). None once the player is out or the stage is
  won, none for a recipient who went out.
- **What is drawn** (`ddi_shell::attack_links`, the 2D pass over the
  miniatures, both shells through the shared play view;
  `attack-links-v0`, PROVISIONAL): **outgoing**, a thin violet line from
  the power bar's end at the band's edge to the recipient's miniature at
  its own power bar, a pulse (a bright dot) running along it for 0.6 s,
  the line whole meanwhile and faded out by 1.5 s, the recipient's
  miniature outlined (it comes in over 0.2 s) until the attack has
  passed. **Incoming**, the same the other way: from the sender's
  miniature to the warning by the receptors, the sender's miniature
  outlined. The warning stands at the field's right edge, so a sender on
  the left ends at the field's left edge on the receptors' row (a line
  never crosses the field). **Chained**: the waiting attack's line and
  outline at 0.45 until the one before it has passed. Colors from the
  theme (`Attack.Line`, `Attack.Glow`: the power bar's violet, lighter).
- **Where the line goes**: straight legs only, the shortest way with the
  fewest corners that stays out of the field's band (the HUD's bars and
  the judgment are in it), the score, the watermark, the quit button's
  place, the status line, the warning's place (unless it ends there) and
  every miniature with its name line, each kept 1.5 px away
  (`attack_links::route`: a search over the lines just off those boxes'
  edges). In practice: along the top at the power bar's height, down the
  gap beside the miniature's column, into it at its bar. Under Reverse
  the bars stay at the top and the outgoing line with them; the incoming
  line's end follows the receptors. Where there is no clear way (the
  margins of a phone held upright: 14 of the 960 lines the test looks
  at, all outgoing under Reverse at 360×640) the outline alone
  says it.
- **Reduced motion**: no pulse and no fade; the line stands still for
  its 1.5 s, then goes; the outline at once.
- **Priority seats** (`opponents::priority_seats`, from
  `Battle::kept_seats`; only where the opponents take turns, beyond the
  cap): the senders of the attacks against the player, then the
  recipient of the player's latest attack, stay in view while those
  attacks last and one rotation turn (8 s) after. A link whose
  opponent is not in view is not drawn.
- **Debug**: `__DDI_DEBUG.battle.attacks.links`, each
  `{attack, from, to, seat, slot, phase, chained, line, pulse, outline,
  path}` (`from`/`to`: `"me"` or the opponent's seat; `phase`: `pulse`,
  `fade`, `still`, `outline`), and `kept_seats`. Desktop:
  `Host::attack_links`.
- **Tests**: `ddi-session`
  `the_player_s_attacks_are_links_while_they_last` (from the commitment
  to the clear, the seat and direction, chained exactly while an earlier
  one lasts, none between CPUs, kept seats a turn after); `ddi-shell`
  `attack_links` `every_line_keeps_to_the_side_space` (one to eight
  opponents at the ten sizes of the miniatures' table, with and without
  Reverse: each line's ends where they should be, straight legs, none
  over the band, the HUD's areas, the judgment, the quit button, the
  warning (outgoing) or any miniature) and
  `a_link_pulses_fades_and_keeps_its_outline` (the phases, chained,
  reduced motion, what is drawn, the debug state); `ddi-theme` (the two
  colors); `ddi-desktop` `room_stage_flow` (links drawn through the
  window: lines for their time, outlines after).

### The attacks told, and on the results (S7)

- **The moments** (`ddi_session::battle::AttackMoment`): an attack
  against the local player committed (`Incoming`) and one of their own
  (`Launched`), taken by the session each frame
  (`Battle::take_attack_moments`) and told (`PlaySession::attacks_told`):
  an incoming one at its commitment, which is at least the player's
  reading horizon plus 1.5 s before its first shock, so before its
  shocks can be seen; not again within six seconds of the one before
  (`VoicePolicy::incoming_again`: a chained attack right behind it, which
  the warning's second bolt says). Their place among the battle's
  moments (`battle-voice-v0`): `attack_priority`, 30 for incoming (under
  the outcomes' 40, over critical life's 20: the place M4 kept for it), 8
  for launched (under the milestones' 10).
- **Said, or shown** (the announcer's two attack lines, 2026-11-12): the
  moments are battle lines now, `battle attack incoming` and `battle
  attack launched` (`ddi_library::announcer::Battle::AttackIncoming`,
  `AttackLaunched`, in `Battle::ALL`; `AttackMoment::event`), folders a
  pack may have, falling back on nothing (a pack without them stays
  silent for them). `VoicePolicy::priority` ranks them (30 and 8) with
  the battle's other moments, and a frame's moments are said together,
  the one that matters most first (`PlaySession::tell`): an attack
  against the player at its commitment, with a deadline, its first shock
  coming into view (`Announcer::battle_by`: said before it or passed
  over, never late; the commitment is at least 1.5 s before it); one of
  the player's own when nothing that matters as much is speaking, else
  passed over. A stage decodes up to four sounds of each
  (`ddi_session::announcer::variants`): they speak again and again. The
  status line beside the field says "Attack incoming" and "Attack
  launched" for the cue's 2.5 s exactly where the line was not said
  (`AttackTold::said`; `PlaySession::stage_cue`: of a battle's moment no
  line said and the attack moments no line said, the one that matters
  most, the newest of equals), so a moment is never told by neither.
  Nothing is by sound alone: the warning, the line and the outline say
  an attack too. The announcer studio lists both with the battle's
  lines (no scene, as the rest of them), with text examples in both
  sets. Debug: `__DDI_DEBUG.battle.attacks.told`, each `[moment, host
  second, line said or null]`; desktop's log, `attacks: … (said|shown)`.
  Tests: `ddi-library` (the folders, any case, the studio's names for
  takes, no fallback); `ddi-session`
  `the_attack_lines_speak_by_their_priority_and_deadline` (the voice on
  a fake clock) and
  `the_attack_lines_are_said_in_a_stage_and_shown_only_where_they_were_not`
  (a pack made in a scratch folder, found as an import finds it, in a
  practice stage: said before the shocks can be seen, not repeated
  within the window, the cue exactly where no line was said; a pack
  without the folders silent); `ddi-ui` (the studio's list);
  `ddi-desktop` `room_stage_voice` (a scratch pack chosen in the
  settings, a practice stage through the window offscreen).
- **The standings** (a stage's results, with attacks): three small
  columns at the table's end, **Sent** (attacks launched), **Taken**
  (attacks launched against them) and **Shocks** (shocks stepped on), a
  number a line for every competitor, in the combo's place (the widest
  table has not the room for both beside the names; life and the
  judgments' strip are a little narrower beside them, so that the names
  keep over 180 px at 1280×720, where nine lines fit, or six and "3
  more" under the match's result); a phone's table, already
  without the combo and the judgments, has none of the three. The old
  "Attacks n · Shocks n" after the judgment counts went.
  `RunDetail::received` carries the second number (additive, as
  `attacks` and `shocks`: an online report may carry all three).
- **The player's battle summary**, under the standings' lines on their
  own results (small, muted; one line where it fits, a dot between,
  else more): "Attacks sent: Mossy Accordion ×2, Tinsel Pike", "Attacks
  taken: Basalt Wren" (or "No attacks sent", "No attacks taken"),
  "Shocks stepped on: 3" and, where the last one in was knocked out by
  an attack the stage waited for, "Drawn: final attack"
  (`SETTLEMENT_V0`; the results' footer says it too, as before). From
  the stage's runner (`Battle::attack_summary`, `StageOutcome::summary`)
  kept by the practice's client (`RoomClient::set_stage_summary`,
  `StageTable::summary`); an online stage has none.
- **Tests**: `ddi-session`
  `the_player_s_attack_moments_are_told_in_the_status_line` (told at
  the commitment, the incoming one further off than it can be seen plus
  the warning, none again within six seconds, the cue's choice and
  time, nothing said); `ddi-ui`
  `with_attacks_the_lines_count_them_and_the_player_s_battle_is_summed_up`
  (the columns, nine lines, the summary's lines inside the panel, none
  without attacks), the `qps-ploc` walk; `xtask l10n check`.

### Not done in this round

- The two announcer lines as lines (above): the moments are told on
  screen, their folders are not read. Their recipes and examples wait
  with them.
- Online attacks, retuning, a duel's attack rates, phone polish (out of
  scope, as the brief has it).

## One picker at a time, and the room's song select coming in once (2026-11-12)

The maintainer asked for a way to "take over" picking, so that a
co-host can pick from their own library instead of following and the
host can take it back, and agreed to the design in
`multiplayer-ui.md`, "Role differences" (the 2026-11-12 note). Before,
every operator of a room saw their own list at once, each one's resting
song became the room's spotlight, and the first pick took the stage.
Everything here is real; nothing is mocked. What could not be run is at
the end.

### The room's model

- **The picker is the room's** (`ddi_room::Picking` in
  `RoomState::picking`): who chooses the songs now, and the shares the
  room keeps of songs a picker rested on when the pick changed hands. A
  new room starts with its owner. `RoomState::picker()`, `chooses(id)`,
  `may_take_pick(id)`, `one_picker()`.
- **The picker alone chooses**: `Command::Spotlight`, `Command::Pick`
  and `Command::ChooseContent` are the picker's. Another operator is
  refused with `Rejection::NotPicker { picker }` (who has it, or nobody);
  a member with `Unauthorized { ChooseContent }`, as before. The pick's
  start of the match is still an operator's (`Advance`), which a picker
  is today.
- **Taking it**: `Command::TakePick`, an operator's
  (`Capability::TakePick`: owner, co-host, admin), accepted at once in
  every phase, with one event, `Event::PickerChanged { picker }`. During
  a stage's readiness, countdown or play it is the next choice that
  changes hands: the stage's song stays as it was chosen. Taking what
  one has changes nothing (no revision).
- **It stays** with whoever has it across stages and matches.
- **Its holder gone** (`release_pick` in the reducer): they leave the
  room, lose the role that lets one take it (a co-host made a member; an
  administrator demoted keeps it), or the authority reports them
  `Connection::Disconnected`. The pick goes to the owner while the owner
  is in the room and not Disconnected, else to nobody
  (`picker: None`) until an operator takes it. The trigger for "gone
  too long" is the room's existing one: the server sets a dropped socket
  Reconnecting and, after its reconnect grace (30 s,
  `ROOM_RECONNECT_GRACE_MS`), Disconnected; Reconnecting keeps
  everything, the pick included.
- **The spotlight goes with its picker**: `PickerChanged` clears it (in
  `evolve`, so a mirror does the same) and keeps its share by chart
  (`Picking::shares`, the newest eight), which `known_share` finds: the
  song rested on again, or picked, is shared already. (The server's
  index also reuses a share by content key when a grant is asked for;
  this saves the request.)
- **Leaving room for taking turns**: the model says "who chooses now"
  and checks the picker by id, whatever their role; only `TakePick` is
  an operator's. A rule that passes the pick around would set the picker
  itself and let the picker's `Pick` advance.

### Old and new, both ways (protocol 2 stays)

All of it is additive: `picking` in the room's JSON, the `take_pick`
command, the `picker_changed` event, the `not_picker` refusal, the
`take_pick` capability.

- **A new client and a server that predates it** (the web deploys on
  push, the server separately). That server's rooms have no `picking`
  in their state, and it would close the socket (4400) on a command it
  cannot read, which the client would send again on every reconnect. So
  the missing state is the signal, in the model and on the screens: a
  room without `picking` is a room from before the picker. The reducer
  lets every operator choose there, as it did (`chooses` is the old
  operator check), `may_take_pick` is false, the client never sends
  `TakePick`, the action is not shown, nobody is marked, and the others'
  status line says "The host is choosing…" as before. Nothing appears to
  work and do nothing. The same holds for a room made before the
  server's deploy and still open after it: it stays as it was until it
  closes.
- **An old client and a new server**. As the owner it is the picker and
  works as before; as a member it follows as before. A `picker_changed`
  frame it cannot read is passed over (`OnlineAuthority::received`), the
  missed revision shows as a gap at the next frame and it asks for the
  room again (unknown fields are ignored): in step. An old client that
  is a co-host or an administrator still shows its own list, but the
  room refuses its rests and picks with `not_picker`, which it cannot
  read: its pick does nothing visible, and the unanswered command is
  sent again after a reconnect (refused again). Such a client needs the
  new build to pick.
- **Online rooms need the next server deploy for taking over** (the
  server runs this reducer; `../ddi-server` needed no change and its
  tests pass against the new crate).

### The client and the screens

- `RoomClient::chooses()`, `picker()`, `may_take_pick()` (connected, and
  the room's `may_take_pick`), `take_pick()`. What was "an operator" is
  the picker now: the resting song told the room, the spotlight's and a
  pick's upload, More's Retry sharing, and a stage prompt's Back
  cancelling the pick (another operator's Back only withdraws them).
- **Song select** (`select_room.rs`): the picker has their own list; the
  others the following view. An operator who follows has **Pick songs**:
  a button in the header where the picker's search is
  (`Room.TakePick`), an item of the room's More on every layout (a
  phone has no room for the button; a controller's way to it), and T,
  with its hint in the footer. A member has none of them.
- **Each player's own list is kept** through a change of hands
  (`RoomMode::library`, from 33697787) with where they were in it
  (`Place`: the song and chart shown, the open pack, the style, the
  search): a picker who becomes a follower and takes the pick back has
  them as they left them. A search being typed is ended first.
- **Who chooses, said**: the others' status line names the picker
  ("alice is choosing the first song.", "… stage 2's song.", "Match
  over. … the next match's first song."); with nobody choosing,
  "Nobody is choosing songs right now.", and for one who may take it
  "Nobody is choosing songs. Use Pick songs to choose them."; the
  picker's own lines are as they were ("Pick a song to start the
  match."). The players' rows mark the picker after their role
  ("Host · Picking", "Co-host · Picking"; 選曲中) in an online room;
  practice, with one person, shows nothing. A refused rest or pick of a
  non-picker is not a status of its own: the line says who chooses.
- **After taking it back**, the song shown is told the room again once
  it has rested (`SPOTLIGHT_REST_MS`, 3 s), as any song.
- **Bots** (`bots.rs`): a host bot picks while the pick is its own. A
  person who takes it over chooses until they give it up (the room
  hands it back when they leave, lose the role or are gone too long);
  the bot follows meanwhile, says who chooses, and takes the pick only
  when nobody has it. It never takes it back from anyone.
- The debug state (`ThemeState::room`): `host` is "this player
  chooses", with `picker` (a member's id or none), `can_take_pick`, and
  each member's `picker`.

### The room's song select comes in once

The maintainer, the same day: "some 'animation flicker' when creating a
private room." Confirmed in the harness: an online room's song select
is built before the room's first answer, so it was built as the
following view (no list, the picks' panel), and when the answer said who
the player is the whole tree was built again and shown at once: one
layout, then the other. A member got the same rebuild; so did practice
on its first frame, invisibly.

- **One side at a time** (`select_room::Side`: Joining, Picker,
  Follower): what the screen is built as, its data with it, changed only
  by `swap_side`. The rest of the screen reads the side shown, never the
  room's state of the moment.
- **Before the room is known** (no member for this player in the state:
  an online room before its first snapshot) nothing role-specific is
  built or shown: no list, no song's side, no backdrop, no status line,
  no panel, no players, no room buttons, no preview. The bars, and one
  quiet line in the middle, "Joining the room…" (the link's existing
  words), which shows only if joining takes more than 0.4 s, and says
  instead why the link ended where it ends before the room is known
  (closed, signed out, not a member). The footer has Back alone; keys
  but Back and More do nothing.
- **Once known, the view comes in once**: its parts play the screen's
  own entrance; the bars are the same actors as before (carried into
  the new tree with their tweens, only what is inside them changed), so
  an answer that comes while the screen is still coming in neither
  begins the bars' entrance again nor cuts it short; the joining line
  fades from wherever it is.
- **A change of hands is one swap**: the view's parts leave
  (`Motion.Screen.Out`, 0.15 s; the list is still the player's own
  while it does), then the tree is built as the other side and its parts
  come in; the bars stay. Under reduced motion, and under the web's
  loader, the other view stands at once. The room's name arriving
  changes the header in place (it used to rebuild the screen too).
- **CPU practice** is known from its first frame: built as the picker's
  at once, no line, no swap; its first-frame rebuild is gone.
- Chosen: the two-step swap rather than a crossfade (the two views
  share the list's place, and a crossfade shows both); an instant swap
  under reduced motion rather than fades (as asked).

### Tested

- `crates/room` (98 tests, 6 new): the reducer's walkthroughs (taken by
  a co-host and back by the owner, during a stage, across stages and
  matches; a member's and a non-picker's rest, pick and choice refused;
  the fallback on leaving, demotion and disconnection, the owner gone,
  nobody until taken; the share kept, eight at most; a room from before
  the picker read, written back and run as it was), the adapter
  contract (the same script through `LocalAuthority` and through
  `OnlineAuthority` + `FakeServer`, with the network's faults), and a
  client meeting frames it cannot read (passed over, the room asked for
  again).
- `ddi-ui` (12 new tests; 508 in its library at the time): the client over the fake room (taken
  over and back, the resting song the picker's, its upload the picker's,
  the kept share reused; a room from before the picker); the screens
  with three clients (the owner picks and the co-host follows; the
  co-host takes over and has their own list, the owner follows their
  song; the owner takes it back with T and has their song and search
  again; a member has no action; a phone's More; the pick left behind;
  a room from before the picker; the late first snapshot of 33697787
  still holds); the entrances (nothing of a view before the answer, then
  each part's entrance begun once and never falling back, the bars
  untouched, for the host and for a member; an answer during the
  screen's own entrance; one swap on a change of hands; reduced motion;
  practice; a link that ends before the room is known); the `qps-ploc`
  walk.
- Desktop, offscreen in the game's theme over the fake room
  (`tests/room_picker_offscreen.rs`, `DDI_OFFSCREEN_PPM=<dir>` keeps the
  pictures): joining, the owner's list, the co-host's following view
  with Pick songs, the member's without, both sides after the take, at
  1280×720 and 844×390, English and Japanese.
- Browsers (headless Chrome and Firefox, 1280×720 and 844×390, English
  and Japanese, with and without reduced motion): CPU practice as
  before, the picker's from its first frame, no line, no action, the
  pick starting the match.
- **Not run**: an online room in a browser or on desktop against a
  server. The maintainer's local server signs in with Twitch (no
  development sign-in), and no server of our own was started. The
  online behavior rests on the fake room (the same reducer, the wire's
  JSON both ways) and the pictures above; `tests/online.rs` was not
  run.

## A room's host: succession, promoting someone, asking before leaving (2026-11-12)

The maintainer: "when hosting a room, I think it's too easy to abandon
it, maybe we should add some confirmation prompt when going back or
closing the tab or app", and on what leaving should do: "the policy
should be configurable, by default we could promote a co-host to host,
and if no co-host than a viewer, maybe in order of who joined first.
But also adding controls to promot a viewer to host/co-host which could
be used for a smoother transition." Three parts: the host's succession
(the room's model), the controls to promote someone, the confirmation.

### What leaving did before this round (as found)

- **An online room's owner leaving** (More's Leave room,
  `Command::Leave`): their row marked left, **still the owner**; their
  socket closed with 4000; the pick nobody's. The room stayed open with
  no host in it: an administrator could still operate it (no screen
  could make a co-host), everyone else could only follow, and nobody
  could change its access. It closed only at the server's idle end
  (`ROOM_IDLE_TTL_SECS`, seven days without a change and with nobody
  connected).
- **Its owner dropping** (the tab closed, the app quit, the network
  gone): the server marks them Reconnecting, and after its reconnect
  grace (`ROOM_RECONNECT_GRACE_MS`, 30 s) Disconnected: their readiness
  for a stage not started cleared, a countdown they were in canceled,
  the pick nobody's. Still the owner; the room waited for them.
- **Coming back by the link**: always possible. The server's join needs
  no invite from someone it knows: one who left gets the same row back
  (the owner the room again), one who dropped reconnects with a ticket.
- **Back on the room's song select** did not leave the room: the screen
  went to the multiplayer home and the room's client was kept for the
  session with its socket open and nobody reading it, so the room saw
  its host connected while they were gone. Only More's Leave room told
  the room.
- **A member leaving between stages**: marked left; the stages after
  count them as sitting out (`SitOutReason::Left`), and a readiness
  window does not wait for them. **During a countdown** they are in:
  the start is canceled for everyone (`CancelReason::RosterChanged`).
  **During a stage**: they stay in its lineup, no report comes from
  them, and the stage settles without it at its report deadline (60 s
  after the first report of the song played to its end, 20 minutes at
  most), where they are a forfeit with no points; their earlier points
  stay.
- **CPU practice**: Back kept the practice room for the session; More's
  Leave room forgot it, its match with it.
- **The server** (`../ddi-server`) decides nothing about a room's
  lifetime but its idle end, and treats the creator specially only in
  making them the first owner.

### The host's succession (the room's model)

- **A room setting, "If the host leaves"** (`ddi_room::Hosting` in
  `RoomState::hosting`, `HostLeaves`): Pass it on (the default) or Close
  the room. The owner's to change (`Command::SetHostLeaves`, under
  `Capability::ChangeAccess`, as the room's access is), in every phase:
  not a match rule, never locked by a match.
- **When it applies** (`settle_host` in the reducer, from the room's
  facts alone, after every command): the owner **left** the room
  (`Command::Leave`): at once. The owner **away past the authority's
  patience** (`Connection::Disconnected`): the room's existing notion
  and grace, the server's 30 s, the one a picker is given. Reconnecting
  keeps everything, so a reload costs nothing.
- **Pass it on** (`RoomState::heir`): among the people present, those
  connected first, then those whose connection is coming or coming back,
  then those away; among equals a co-host before a member, then whoever
  joined first (the lower id), playing or watching. The one before
  becomes a co-host and the new one the owner (two `RoleChanged`, which
  every client reads), with everything an owner has; nobody is asked.
- **Nobody to pass it to**: an owner who **leaves** alone, or with only
  bots or nobody else present, ends the room, as Close does. An owner
  who is **away** passes it only to someone connected; with nobody
  connected the room keeps its absent host (nobody is there to run it
  for, and a lost connection should not destroy a room) until the host
  is back or someone else connects (or comes in by the link), who then
  has it. The server's idle end still closes a room nobody returns to.
- **Close the room**: the room ends for everyone when its owner leaves
  or is away past the grace, with others there or not
  (`Event::RoomClosed`, `RoomState::closed`, `Closure::HostLeft`). A room
  that ended decides nothing more (`Rejection::RoomClosed`); its
  authority closes every socket with 4410, the existing close, with its
  wording ("This room has closed.").
- **Bots** are never passed the room and cannot be made co-hosts or
  hosts (`Rejection::NotAHuman`): a bot follows a room, it does not run
  one, so a room left to bots would be a room nobody runs. A bot that
  made the room (`--mp-bots --host`) is its owner as before, and when it
  leaves the room passes to a person.
- **The one before, back**: a co-host (their role changed at the
  handover, so also after leaving and coming back by the link). The new
  owner hands the room back with one command (`Command::MakeHost`).
- **The pick** agrees with it: an owner's pick goes to the new owner
  (the succession runs before `release_pick`); a co-host who holds the
  pick keeps it when the room changes hands around them. And the open
  question of the round before is answered: **a pick nobody holds is the
  owner's whenever the owner is there** (`settle_host`), so an owner who
  comes back while still the owner chooses again without taking it.
  Nobody chooses only while the owner is away with nobody connected to
  pass the room to.
- **A running match goes on** under the new owner: the stage in play
  keeps its lineup (the one who left is a forfeit when it settles), the
  next pick is the new owner's, and what the one before sends as an
  owner is refused whole (`Unauthorized`, `NotPicker`).
- **`Command::MakeHost { participant }`** (`Capability::MakeHost`, the
  owner's): hands the room to someone present and connected, never a
  bot; the giver becomes a co-host and cannot take it back alone. The
  pick stays with whoever has it.

### Old and new, both ways (protocol 2 stays)

All additive: `hosting` and `closed` in the room's JSON, the
`set_host_leaves` and `make_host` commands, the `host_leaves_changed`
and `room_closed` events, the `room_closed` refusal, the `make_host`
capability.

- **A new client and a server that predates it**: that server's rooms
  have no `hosting`, pass nothing on, and would close the socket on a
  command they cannot read. The missing state is the signal
  (`RoomState::has_succession`): the client never sends
  `SetHostLeaves` or `MakeHost` there, offers neither, and says of a
  host's leaving what that server does (the room stays open without a
  host). `SetCoHost` is older than this round and is sent to any room.
- **A room made before the server's deploy and still open after it**
  (no `hosting` in its stored state): this reducer reads it as Pass it
  on, and writes the setting with the first change it makes there
  (`HostLeavesChanged`, one more event of that revision), so such a
  room has the setting and the succession from its first command after
  the deploy.
- **An old client and a new server**: a handover is `RoleChanged`,
  which it reads; `host_leaves_changed` and `room_closed` are frames it
  cannot read, passed over, and mended by the room asked for again (or
  the socket's 4410).
- **Online rooms need the next server deploy** for all of it. The
  server (`../ddi-server`) runs this reducer; one small change there: a
  room the model ended is closed as an idle one is (every socket 4410,
  the one who left 4000, its shares deleted, every request refused
  `room_closed`), from a message or from the alarm that ends a reconnect
  grace.

### Promoting someone (the screens)

`Command::SetCoHost` had no screen. Now:

- **Host options** (`room_dialogs::HostDialog`), an online room's
  owner's alone, opened from the Players pop-up: its button at the
  pop-up's bottom left (keyboard and controller: Left from Close), or a
  tap on another person's row there (the rows the owner can act on sit
  on a faint plate; the pop-up opens on that person). It holds:
  - **"If the host leaves"**, as a row like the Rules pop-up's (its
    value between two arrows: Pass it on, Close the room), with the
    value's sentence under it ("If the host leaves, a co-host becomes
    the host, or else the player who joined first." / "If the host
    leaves, the room closes for everyone."). Applied at once, in every
    phase. Chosen over the Rules pop-up: Rules is the next match's
    draft with Cancel and Apply and locks with a match; this is the
    room's, changes at once and belongs with who runs it.
  - **Other players**, each on a row with **Make co-host** or **Remove
    co-host** and **Make host**. Make host asks once more ("Make carol
    the host?", Cancel first): the giver becomes a co-host and cannot
    take it back alone. Make host is grayed for someone who is not
    connected (the room would refuse); a co-host may be made while
    away. Never a bot's row or a CPU's.
  - It closes by itself when the player stops being the owner (the room
    handed over).
- **Everyone's Players pop-up** says the room's setting in the same
  sentence, under the status, and lists the players before the picks
  now (it listed the picks first; the players' rows are what the host
  acts on, and on a phone they were below the fold).
- **The roles as they change**: every client's rows say Host, Co-host
  and who is picking from the room's state, as before; a co-host gets
  Pick songs at once; a new owner the controls. A change of owner alone
  rebuilds nothing (the rows refill); where the pick moves with it (a
  host who left while choosing), the new picker's view changes hands by
  the existing one swap.
- **A room whose authority does not pass on** (no `hosting`): Host
  options has the co-host buttons alone (that command is older); no
  setting, no Make host, no line in the Players pop-up, and nothing is
  sent that it could not read (`RoomClient::set_host_leaves` and
  `make_host` send nothing there).
- **Bots**: a bot that made the room (`--mp-bots --host`) is its owner
  as before and never opens Host options; a bot's row has no controls
  and the room refuses a bot as co-host or host.
- The debug state (`ThemeState::room`): `manages_hosts`, `host_leaves`
  (`pass_on`, `close`; none where the room does not pass on),
  `host_open`.

### Asking before leaving

- **When the game asks** (`RoomClient::leave_cost`, `LeaveCost`): an
  online room's **host**, always (with others in it or not, a match
  running or not); a **player in the running match**, the one
  **choosing its songs**, or a **co-host**, while a match is running.
  Nobody else (a member between matches, a watcher), nothing before
  the room is known, and nothing once the link has ended (the room
  closed, the sign-in ended, left). (CPU practice with a match running
  asked too until 2026-11-13: never now, "Watching a stage" below.)
- **The question** (`screens::room::ask_leave`, the theme's confirm
  dialog named `room-leave`): "Leave the room?", one or two sentences
  saying what leaving does in the case at hand, **Stay** chosen first
  and **Leave**; Escape and Back in it are Stay. Its sentences, from
  what the room will do:
  - the host, the room passing on: "You are the host. alice will become
    the host." (with a match: "…, and the match goes on without you.")
  - the host, the setting on Close (or only bots to pass it to): "You
    are the host. The room will close for everyone." (with a match:
    "…, and the match in progress will end.")
  - the host alone: "You are the only one here. The room will close."
  - the host of a room whose server does not pass rooms on: "You are
    the host. The room stays open without a host, and you can come back
    by its link."
  - in the match: "You are in the match in progress. It goes on without
    you, and you score nothing for the stages you miss."
  - choosing its songs: "You are choosing the songs. The host will
    choose them instead, and the match goes on without you."
  - a co-host: "You are a co-host. The match in progress goes on
    without you."
- **Every way out goes through it**, once: Back on the room's song
  select (Escape, Backspace, the header's Back, a controller's Back,
  the browser's Back), More's Leave room, and the window's close and
  the app's Quit on desktop. Leave after the question leaves without
  another. Back on a stage's prompt only leaves the prompt, as before
  (it withdraws to the room's song select).
- **Leaving, now**: Back on an online room's song select **leaves the
  room** (the room told, `Command::Leave`, and forgotten), asked or
  not; it used to keep the room's client connected behind the home
  with nobody at it. CPU practice between matches is kept for the
  session on Back, as before; with a match running, Leave forgets the
  practice (the question says the match is lost), where Back used to
  keep the match as it stood.
- **The web**: a screen that asks before it is left
  (`Screen::guards_leaving`) has one history entry more over its route
  (`ddi_ui::back_stack`'s depth plus one), so the browser's Back pops
  that entry and reaches the screen as the menu's Back: the question
  opens and the route stays; the entry is pushed again with the
  question's own, and is gone back over quietly when nothing guards the
  screen any more. Closing or reloading the tab: `beforeunload` is
  registered exactly while the page is a room's (its song select, a
  stage's prompt, play or results) and leaving costs something
  (`crate::room::sync_unload_guard`), never in singleplayer; the browser
  shows its own words, and only once the person has used the page. The
  page cannot tell the room it goes (nothing sent from an unload
  arrives reliably, and a reload must not make its player leave): the
  socket closes with the page and the room gives the player its
  reconnect grace, then counts them away (a host's room then passes on
  or closes, by its setting). `__DDI_DEBUG.ui.leaveGuard` and
  `unloadGuard` say both.
- **Desktop** (`Shell::close_requested`): the window's close button and
  the app's Quit (⌘Q, the Dock's) tell the screens
  (`ddi_ui::CLOSE_REQUESTED`) where the one shown guards its leaving;
  the room's song select and a stage's prompt ask, and the game quits
  only on Leave, once the room has answered its leaving or after
  0.6 s (`QUIT_WAIT`). Asked again while the question is open, the game
  quits without it. A signal or a forced quit is not intercepted.
- **Not covered**: a stage being **played** (the field on screen). No
  question can be shown over a running play in either shell: the
  window's close and Quit end the game at once there, as before (the
  room sees a dropped connection), and the browser's Back leaves the
  play for the room's song select, as before (a forfeit of the stage).
  The web's `beforeunload` does cover the tab's close during a play.
  And a member's **browser** Back off an online room's route where
  nothing is asked still only changes the page (the room keeps them,
  silent, until the tab closes); the game's own Back tells the room.

### Tested (the model and the server)

- `crates/room` (108 tests, 10 new): the walkthroughs (leaving with
  co-hosts, with members only by join order, connected before away,
  never a bot; alone and with only bots, the room ended and refusing
  everything; a drop inside the grace and past it, the owner back as a
  co-host and handed the room back, nobody connected and the first one
  there; Close for a leave and for a drop; mid-match and mid-countdown;
  Make host and its refusals; a room from before the setting), the
  adapter contract (the same script through `LocalAuthority` and through
  `OnlineAuthority` + `FakeServer`, with the network's faults), and the
  fake server's grace (`FakeServer::set_grace`) and close (every socket
  4410, the leaver 4000, no ticket after). The picker's fallback test
  was rewritten for a room that passes on.
- `ddi-ui` (4 new tests; 510 in its library): over the fake room with
  three clients and their screens: the owner makes and removes a
  co-host, sets Close and back, and hands the room over (asked once
  more, Cancel changing nothing), the views of the owner, the one
  promoted and a member after each, the new owner handing it back; the
  same by keyboard and by controller, and a touch on a person's row; a
  bot's row without controls, someone away never handed the room, a
  room that does not pass on with co-hosts alone and nothing sent; the
  owner leaving and the room changing hands on the others' screens. The
  `qps-ploc` walk has the pop-ups, wide and narrow.
- Desktop, offscreen in the game's theme over the fake room
  (`tests/room_host_offscreen.rs`, the harness shared with the picker's
  pictures in `tests/room_fake/`): the owner's Players pop-up, Host
  options with each value, a member's Players pop-up, the question
  before the handover, both sides after it, and "Leave the room?" for
  a host and for a host alone, at 1280×720 and 844×390, English and
  Japanese; looked at.
- Asking before leaving, `ddi-ui` (3 new tests; 513 in its library):
  practice (no question between matches; with a match running the
  question by Escape, Backspace, the header's Back, a controller's
  Back and More, once each; Stay by Escape, Back, its button and Enter
  keeping the room, the match and the list; Leave forgetting the
  practice with no second question); an online room over the fake
  server (each case's sentence; a member between matches leaving at
  once with the room told; a co-host asked and staying; the host
  leaving and the room passing on with the match going on; the last one
  leaving and the room closed; nothing asked once the link has ended);
  the window's close request (asked on the song select and on a
  stage's prompt, Stay keeping the readiness, Leave quitting once the
  room is told). The desktop shell (`shell.rs`, 1 new test, headless):
  a close request quits at once on the start menu and in a room between
  matches, asks with a match running and does not quit, quits on a
  second request while the question is open, and on Leave.
- Browsers (headless Chrome and Firefox, 1280×720 in English and
  844×390 in Japanese; CPU practice): nothing guarded on the start
  menu, song select or a room between matches (Back leaves at once);
  with a match running (the pick, then back out of the stage's prompt)
  the room's song select has its history entry and `beforeunload`
  registered, as has the stage's prompt; Escape, the header's Back and
  the browser's Back (twice in a row, and once more inside the
  question) each open the question or close it, the route staying
  `#/room` and the history's entries in step; Leave goes to the home
  with nothing left registered; then the browser's Back shows the
  room's address with a new practice (nothing to lose), Forward the
  home, and two more Backs each move. The browser's own prompt on
  closing the tab was not seen: headless browsers show none, so only
  its registration is checked (`__DDI_DEBUG.ui.unloadGuard`). The
  multiplayer home's sign-in check (`GET api/me`) answers 404 on the
  static test server, as before this round; the script counts those
  apart and fails on any other error.
- **Not run**: an online room in a browser or on desktop against a
  server (none was started: the online cases rest on the fake room in
  the harness and the offscreen pictures); a real desktop window's
  close button and ⌘Q (the shell's logic is tested headless, the
  window never opened); Safari.
- `../ddi-server` (56 tests, 2 new; 57 with the development provider):
  a host who leaves passes the room on, comes back a co-host with no
  invite, and under Close ends the room for every request and socket;
  a host away past the grace at the alarm. Clippy natively and for
  wasm, with and without `dev-auth`. Its `xtask` test
  `the_deployed_config_keeps_the_scripts_names_and_bindings` fails on
  the maintainer's own `wrangler.toml` (a value where the test expects
  none), before and after this round; left alone.

## Watching a stage: Play or Watch, the spectator view, watching on after going out (2026-11-12, 2026-11-13)

The maintainer: "it could be useful to have a host that doesn't play,
only chooses the songs and watches the plays (which on the view could
either have one user in focus which is switchable, or no one in focus
and only show miniatures)." Built first where the plays are at hand:
CPU practice, each CPU's play judged on this device and fed to its
field as a stream of moments (`ddi_session::battle::feed`). Everything
here is real unless it says otherwise; nothing is mocked. An online
room's plays are not streamed yet: a watcher there keeps the room's
song select during a stage, as before (the next round). Names:
`watch-v0` (PROVISIONAL) for the choices below.

### Play or Watch (the room's model and its screens)

- **The choice is the member's own**: `ddi_room::Intent` (`Play`,
  `Watch`), `Command::SetIntent`, both there since M1. Chosen on the
  room's song select: More's item and the Players pop-up's button at
  its foot (it says what a press does: **Watch only** while they play,
  **Play** while they watch; keyboard, controller, touch); everyone's
  players panel and pop-up say **Watching** on their line. It holds
  across stages and matches until changed (the room keeps it; a
  practice room for the session).
- **A watcher is no competitor** (the model, tested): not in the
  match's roster, so not ranked, not awarded and not in the standings;
  no Ready (`Rejection::NotPlaying`); nothing a readiness window waits
  for (`SitOutReason::Watching` never holds one); not counted against
  the cap of nine (`RoomState::reserved_humans` counts those who want
  to play; `RoomPolicy::check_competitors` is asked with that); no seat
  in the attack ledger, so never a target or a sender.
- **A watching host still hosts** (`ddi_room::can` goes by role, never
  by participation): picks (where they are the picker), edits the
  rules, starts the stage.
- **A stage needs competitors**: where every person watches, a match
  starts only if CPUs compete (`RoomState::start_match`: CPU practice),
  and its stages start with the CPUs alone
  (`RoomState::humans_needed`: the ready people a start needs are never
  more than the people of the match who want to play); the policy's two
  competitors still hold (one CPU alone is refused: "Not enough players
  to start a stage."). A hosted room where everyone watches is refused
  whole, typed (`Rejection::NoPlayingHumans`), said as "Everyone is
  watching. Someone has to play." Nothing else of the reducer changed,
  no command, event or field was added, and the protocol, `online.rs`
  and `fake.rs` are untouched: an older server refuses the same hosted
  rooms it did (its rooms seat no CPUs).
- **A change during a match** is the model's rule as it stood, kept:
  Play to Watch applies from the stage being prepared (the Ready is
  cleared; a stage counting down or in play with them in it is played);
  one of the match who watches sits its next stages out with nothing
  for them and keeps what they won; back to Play, they are in from the
  next stage. One who was watching when the match began is not of it:
  choosing Play then waits for the next match ("Next match"), the
  room's rule for arrivals (practice admits nobody into a running
  match). The control is offered wherever the player is not committed
  to a stage (`RoomClient::may_choose_intent`).
- **The watcher's prompt**: "Watching" where Ready is; a press readies
  nothing (it only gives the shell its sound, as Ready's does); no
  attack policy row. In practice the start is committed with the pick
  (nobody is waited for), so the room's three-second countdown runs and
  the stage starts by itself; where the web has no sound context
  running the prompt asks for a press, as for any automatic start.
  Leaving the prompt before the start cancels the start (no stage runs
  unwatched); leaving once it is on plays the CPUs out at once and
  reports the stage (nobody forfeits).
- Client: `RoomClient::my_intent`, `watching`, `set_intent`,
  `may_choose_intent`, `watches` (a stage the player is not in is
  watched on this device: CPU practice), `in_stage`;
  `StageOrder::watching`. Debug: `ui.theme.room.watching`,
  `can_choose_intent`, each member's `participation` (`watching`).

### The spectator view

A watched stage is a play session without a play of its own
(`StageSetup::watching`): the song's music, background, lyrics and
clock as a play has them; the session's own player is never judged
(nothing is pressed, missed, clapped or logged), there is no hand, and
nothing is kept in the history. The battle runs the CPUs on the song
clock exactly as beside a player (`Battle::advance_watched`), with the
watcher in no roster.

- **Nobody followed: the grid** (`ddi_shell::opponents::arrange_grid`):
  everyone's fields over the whole screen, each a field in the play's
  proportion with what a miniature has (name, a CPU's stars, life,
  power bar, combo, score, locked lanes, shocks, Out), drawn by the
  play's skin as miniatures are (`ddi_render::scene::Miniature`; the
  renderer draws no field and no HUD of its own,
  `RenderOptions::no_field`). One size for all, the layout that gives
  the largest lane, one with more rows preferred where its lane is
  within a tenth (`GRID_FILL`: eight are four by two), the block in
  the screen's middle, a short last row in its own middle, the status
  line under it. **The size rule**: a grid's field is at most a play's
  own field (`GRID_LANE_SHARE` 1.0 of the lane the play's field would
  have); the cap that keeps a player's miniatures at half their own
  lane (`MINI_LANE_SHARE`) is for miniatures beside a field of one's
  own and does not apply where there is none. Each field keeps the
  room of its attack warning on its right (a lane), shown or not.

  | Competitors | 1280×720 (play lane 72) | 1920×1080 (play lane 108) |
  | --- | --- | --- |
  | 1 | 65 | 101 |
  | 2 | 65, side by side | 101 |
  | 3 | 63 | 99 |
  | 4 | 57, four in a row | 86 |
  | 5 | 45 | 68 |
  | 6 | 37, six in a row | 56 |
  | 7 | 29, four and three | 48 |
  | 8 | 29, four by two | 48 |

  (A player's miniatures at the same counts: 36 or less at 1280×720,
  25 for eight with the attack warning's place kept clear. Two rows of
  fields ten lanes tall cannot have lanes over about 30 px at 720 px:
  eight in the play's proportion are only a little larger than a
  player's eight.) Beyond a room's eight (the development practice)
  as many as fit at lanes of 16 px, the rest taking turns.
- **One followed** (the focus): that competitor's field in the play's
  own field's place and size, as a player's own is: its judgment's word
  with its early or late, its combo, its score in the HUD's place, its
  life and power bar, its locked lanes, its Out; the others beside it
  exactly as a player's opponents are (`arrange`, the half-lane cap).
  Tab and Shift+Tab or a controller's shoulders go round the seats, the
  grid again past the last; a click or a tap on a field follows it; a
  click on the large field, or Escape once, goes back to the grid.
  Escape held or pressed twice still leaves the stage, as a play's quit
  (nobody forfeits; the CPUs are played out and the stage reported).
  The focus is the session's alone: remembered for the stage, never
  saved. The status line says "Watching [name]"; for a stage's first
  six seconds in the grid, "Tab or tap a field to follow one player".
- **Attacks**: in the grid a line from the sender's field (at its power
  bar) to the warning beside the recipient's field, the recipient's
  field outlined while the attack is on its way and passing, each
  recipient's warning beside its own receptors
  (`attack_links::shown_between`, in straight legs along the gaps
  between the fields); followed, the competitor's attacks as a player's
  own (`SeenLink::of`, the same `attack_links::shown` and the same
  warning by the receptors). The status line says the stage's moments,
  nobody's "you" (`WatchMoment`): "[name] is out", "[a] attacks [b]",
  "[name] wins the stage" (which stays), "Stage drawn", and the
  players-left milestones. The announcer speaks the stage's opening
  and the players-left lines; the lines addressed to "you" (attack
  incoming and launched, life critical, out, won) are not said to a
  watcher (the battle tells none of those moments).
- **The end**: a while after the stage is decided
  (`WATCH_END_HOLD` 2.5 s, the music fading as after a fail, the
  winner's name in the status line), or at the song's end, the watcher
  goes to the stage's results: the Standings alone (no Graphs tab:
  there is no play of theirs to draw), and on the left, where a
  player's grade and judgments are, the stage's song, **Stage winner**
  with the name (a CPU's stars) and "You watched this stage."
  (`ResultsScreen::watched`).
- Debug: `__DDI_DEBUG.watch` (`view`: `grid`, `focus`, `out`, `left`;
  `focus`, `lane`, `fields`, `label`, `cue`, `warned`, `mark`, `skip`,
  `fields_at`, `skip_at`, `over`), `battle.attacks.links` (between two
  seats in the grid), `ui.theme.stage.watched` and `stage_winners`;
  desktop's `Host::watch`.

### What the feed gained, and what an online stream must carry

The followed field is drawn from the competitor's stream of judged
moments, never by a second judge. A moment
(`ddi_session::battle::feed::Moment`) had: when (judged song seconds),
which note or shock (tick and lane), what (a tap's judgment, or a shock
stepped on), and after it the life, the combo and whether it put them
out. For a field shown large it gained **`offset`** (press minus note,
seconds: the word's early or late), **`w0`** (whether a top hit was
inside the ruleset's inner window: the word's tier under DDI) and
**`score`** (the score after it as the room shows it, a CPU's on the
chart's scale); `FieldFeed::frame_large` makes the frame with the
word, for one second as a play's. **Hold states were not added**: the
CPUs play a tap-only copy, so there are none to show; a person's
stream will need them (a hold's head hit, held, let go and done) and
the notes of the chart they play rather than the copy, with their
turns and removals.

What an online stream must carry for this view, per competitor: the
moments above in order (with a presentation delay the feed already
takes), the attack commitments that reach them (each shock's tick and
lane and the commitment's time: `FieldFeed::add_hazards`), and from
the room's ledger the attacks between competitors (sender, recipient,
commitment, first shock, clear) and each one's power, for the bars,
the lines and the warnings. The scroll each field is drawn under is
the viewer's own.

### Out of a stage: watching on, and leaving a play as a forfeit

- **Out by life**: the play no longer ends for the player at once.
  Their field stays in place, blank and dimmed with **Out** as their
  miniature would be, the music plays on, and the stage goes on in
  real time between the others (the CPUs and their attacks really
  played, as they already were during the fail's hold), the others'
  miniatures beside it as before, who attacks whom drawn between them.
  **Enter**, a controller's Start or South, or a press on the plate
  under the mark ("Enter: skip to results"; "Skip to results" on a
  touch screen) skips to the results (the rest played out at once, as
  going out did before); so does the quit (Escape held or twice).
  Else the results follow a while after the stage is decided
  (`WATCH_END_HOLD`), or at the song's end. Where nothing is left to
  watch (the stage decided by their going out, an online room's) the
  fail's fade and hold are as before.
- **The results' timing data** are those of the moment they went out:
  the play ended there (its fail is final then, as before), and the
  time watched adds nothing to their judgments, score or graphs.
- **Leaving the play is a forfeit, then watching on** (the maintainer,
  2026-11-12, asked whether a question is wanted over a running play:
  "'forfeit' the stage and drop into watching the stage until a
  decision to quit is made"). One path for out by life and out by
  quitting: the play's quit (Escape held or twice, the quit button, a
  controller's hold) forfeits the stage (`Battle::forfeit_now`: out of
  the stage from then, no target and no sender; the play judged to the
  quit, `Player::quit_at`) and the screen becomes the same watching
  view, the mark **Left the stage**. From there: skip to the results,
  keep watching, or quit again (the results). **What the standings
  show for it**, as a forfeit did before: last of the stage, no points,
  life "Left", the grade QUIT, their score and judgments to the quit;
  the room's mark "Stage n: left the stage · place p of f · no
  points". A quit after the stage is decided (the winner finishing the
  song) ends the play as before, and is no forfeit. Restart (R or `\`)
  stays unavailable in a stage (tested as before). A solo play's quit
  is unchanged (its results at once).
- **Online**: nobody else's play is streamed yet, so after a forfeit
  there is nothing to watch: the play ends and its results follow as
  before (the Standings filling in as reports come). The path is the
  same code: where the battle has others playing here
  (`Battle::goes_on`) the session watches on, so a stream of the
  others' plays plugs into it.
- **A close request during a stage's play** (desktop's window close
  and Quit): where leaving the room costs something (an online room),
  the stage is left first (the forfeit above, `PlaySession::leave`)
  and the room's question is asked over its results, saying so first:
  "You left this stage: no points for it." and then what leaving the
  room does. Stay keeps the results (the forfeit is not undone); Leave
  tells the room and quits; asked again meanwhile, the game quits. A
  stage's results reached the ordinary way now ask too
  (`ResultsScreen` guards its leaving where the room's leaving costs).
  The web: closing the tab during a play is covered by `beforeunload`
  as before; **the browser's Back during a play is unchanged** (not
  done, below).

### CPU practice is never asked about (the maintainer, 2026-11-12)

"For practice I think even silently dropping the match is fine. I
actually don't like the behavior of keeping the unfinished match, and
a prompt would be annoying." `RoomClient::leave_cost` is none in
practice, always (`LeaveCost::Practice` and its sentence are gone):
Back on the room's song select, More's Leave room, desktop's close and
Quit, the web's history guard and `beforeunload` all behave with a
match running as they do without one. Leaving with a match running
**forgets the practice and its unfinished match**: the game's Back and
Leave room discard it (`screens::room::leave_now`), and on the web a
practice whose pages were left by any way with a match running is
dropped (`crate::room::forget_left_practice`: the browser's Back, an
address typed), so coming back to CPU practice is a fresh room.
Between matches (the lobby, a finished or stopped match) Back keeps
the practice for the session, as it was. Online rooms ask exactly as
built. The table of cases in "Asking before leaving" above stands
without its practice line.

### Tested

- `ddi-room` (111 tests, 3 new): `walkthroughs`
  `a_watcher_is_no_competitor_and_a_watching_host_still_runs_the_room`
  (the cap, no Ready, the pick, the roster, the window, no placement,
  a change during a match both ways) and
  `a_stage_needs_competitors_and_practice_cpus_play_alone_for_a_watcher`
  (a hosted room refused whole, practice's CPUs alone committed with
  the pick, one CPU refused); `online`
  `a_members_watching_is_kept_alike_by_both_authorities_and_every_mirror`
  (`LocalAuthority` and `OnlineAuthority` + `FakeServer`).
- `ddi-session` (170 and one ignored, 10 new): a watched stage to its settlement, the
  same ranking at 60 and 144 frames a second and as played out at
  once, the watcher in no ranking and no attack; the focus by Tab and
  Escape, the followed frame equal to the feed's; the end a hold after
  the decision; a watcher's quit; going out then watching on to the
  ranking, the draw flag and the player's own results that skipping
  (playing out at once) gives, at 60 and 144; a quit forfeits, watches
  on and is skipped by Enter or a second quit; leaving from outside the
  play (online: at once; practice: watching on); the feed's large
  frame; the watch's keys.
- `ddi-shell` (61, 5 new): the grid for one to eight at the ten sizes
  of the miniatures' table, with and without Reverse (on the screen
  with its warning's room, clear of the quit button's place, the status
  line and each other, the play's proportion, never over a play's own
  lane); the table above; the followed field's place and what is drawn
  over it; the out field's mark and plate; the grid's lines (every pair
  for two to eight at the ten sizes: from the sender's bar to the
  recipient's warning, straight legs, over no field).
- `ddi-ui` (518 and two ignored; 5 new, 2 rewritten): the control (More by a controller, the
  Players pop-up by keys and a tap), the watcher's prompt, leaving it,
  the sentence; a watched match through the client (it ends for nobody
  here: no "match not won" for a watcher); the watched results (wide
  and a phone's); practice never asking (rewritten, with the close
  request's test); the results' question after a close request; the
  `qps-ploc` walk (the watched results joined it); `xtask l10n check`.
- `ddi-desktop` `tests/room_watch_offscreen.rs` (2): a practice stage
  watched through the window offscreen (the prompt, the grid, Tab, a
  press on a field, on the large field, Shift+Tab, Escape, lines and
  warnings drawn, the results, back to the room; then a stage left by
  Escape twice, nobody forfeiting); a player going out and watching on
  (Enter), then leaving a play (Escape twice: the forfeit, watching on,
  a press on the plate). `room_stage_flow` unchanged for a playing
  person.
- Runtime, headless Chrome and Firefox
  (`target/scratch/mp/watch/watch.mjs`, screenshots in
  `target/scratch/mp/watch/shots/`).

### Not done in this round

- **The web's browser Back during a stage's play** is as it was: no
  forfeit-then-question there (the play route's own handling of Back
  during a play was left alone). Desktop's close request has it.
- **The desktop's close request during an online stage's play** is
  wired (`Shell::close_requested`, `ask_after_play`) and its parts are
  tested (the session's `leave`, the results' question), but no test
  plays an online stage through the window to close it.
- A watching member of an **online** room sees nobody's play (no
  stream yet); they stay on the room's song select through a stage.
- The followed field's hit effects are the miniatures' (the skin's own
  explosions run on a play's skin events, which a stream does not
  carry); hold states (above); the dancing hand never follows a
  competitor.
- A player out of a stage cannot follow one competitor large: their
  own field stays where it is. (Built since: "A player out of a stage
  follows a competitor large".)

## Online play's shared base: the contract in `crates/room` (implemented, 2026-11-13)

The base the server's and the game's fronts build on, described in full in
[Online play: the shared base](multiplayer-online-play.md). Built and
tested in `ddi-room` alone; nothing in the game's screens, shells or
server changed, so nothing of it is reachable yet.

- Real and tested (in process, through `FakeServer`): the room's clock
  estimated from pings; a stage's song zero on it where the room's policy
  plays live; the battle plan offered with the pick, checked by digest,
  kept, its digest on the stage's content; each competitor's moments and
  checkpoint reports in batches, one binary frame a tick to everyone,
  resumed after a gap or a reconnect; the attack ledger run by the room at
  deadlines, giving the same record as practice for the same reports;
  the stage decided by the room as practice decides it (final attacks
  included), reports after an early decision kept for the standings, the
  song ended once every stream ended.
- Reducer changes (additive to protocol 2): `RoomPolicy::live`,
  `StartCommitted::song_zero_at`, `SetBattlePlan`/`BattlePlanSet`,
  `StageContent::battle`; in a live room's stage confirmed early, a
  `ReportOutcome` in SongCompletion is taken as detail and the song ends on
  the last one or the report deadline. Nothing changes for a room without
  live play or for practice.
- Development only: the 73-competitor tests use the development award
  scheme (no award rule exists over nine); the cap stays nine.
- Provisional (each in the plan's table, with its effect and the decision
  it waits for): `live-v1`'s numbers (250 ms tick and batches, 1 s view
  delay, 750 ms report deadline, 1.25 s network lead, 5 s late join, 30 s
  of frames kept), song zero 1 s after the countdown, a late attack dropped
  rather than moved, early decisions for online survival rooms, devices
  that predate live play left out of attacks and early decisions,
  Disconnected past the grace as a forfeit.

## Online play, the client front: the game plays a private room's stage live (2026-11-13)

The game's side of `docs/plans/multiplayer-online-play.md` (its "What the
client front builds"), on the contract in `crates/room` and tested over
the in-process `FakeServer` alone: no real server was started for it.

### What is built

- **The contract's bug, fixed first.** A stage the room decided in the
  same tick in which it saw every stream end (everyone a no-show, say)
  sent `EndSong` with a ranking after its own `ConfirmStage`; the reducer
  refused it and the song waited for the report deadline.
  `LiveStage::song_end` now sends no ranking once it decided
  (`tests/live.rs`, `a_stage_everyone_misses_…`). The server's workaround
  can go.
- **Binary sockets.** The web's `WebSocket` reads `ArrayBuffer`s and
  sends bytes, desktop's tungstenite thread carries `Message::Binary`
  both ways (its read waits 10 ms now, so that a ping waits no longer on
  its way out); both say `Transport::binary()`. A room whose policy does
  not play live is never sent a live message (tested from the client);
  close 1012 reconnects and plays live (tested).
- **The room client** (`ddi_ui::state::room`): `plays_live`, `room_now`,
  `set_me` from the mirror, the live news kept only while a stage is
  played live. A stage's preparation in a live room works out what live
  play needs (`LoadJob::live`, `prepare_stage`, `LivePrep`): the reading
  horizon under the player's options (raised as practice raises it) and,
  with attacks, the battle plan, made by `ddi_ui::state::battle_plan`
  (the checkpoints and sections practice's `AttackRunner` uses, moved
  there so both paths make them the same way; the seed `seed_for`). The
  picker's device offers its plan; every device compares the room's
  digest, says `prepared` (its horizon, the digest) and only then reports
  Loaded; a different plan reports `battle_plan_mismatch`, sits the stage
  out and says so ("This device sees the stage's attacks differently from
  the room, so you sit this stage out.", "Attacks differ" in the players
  panel). The stage's order carries it (`StageOrder::live`: the prep and
  the other people).
- **The synchronized start** (`ddi_session::battle::live::live_start`,
  `PlaySession::start`): song time zero heard at `song_zero_at` on the
  room's clock as estimated, through the output's latency; the prompt's
  Ready still makes the web's audio context in the press, and the play
  begins when the room starts the stage (a second before song zero, the
  lead-in fitting in it). A device later than that joins from where the
  room's clock has the song (its notes before judged as misses, a staged
  start); later than `late_join_ms` it forfeits at once and watches.
- **Sending** (`battle::live::Live::send`): the play's judge events as
  moments (taps with tier, offset and W0, hold ends, mines, shocks, Boos,
  with life, combo and DDI's sum after each frame's events), an
  `attacks::Reporter` fed as practice counts (chart taps, going out,
  policy changes), the stream's end (finished, out, forfeit) before the
  `ReportOutcome`. The room client is ticked by the play itself (no
  screen ticks it during a play). Batches are cut every interval also
  while the socket is down (`LiveLink::due`), so a catch-up after a long
  reconnect is many small batches, never one over the room's bounds
  (64 KiB asserted with room to spare).
- **The remote ledger** (`Battle` with `with_live`): no local
  `AttackRunner`; commitments from frames go into the play (refused,
  with a `refused` moment, when its first shock's warning is already
  due) and onto the recipient's field; power bars, the waiting line, the
  warning, who attacks whom (links, the watcher's links), the attack
  moments and lines come from the room's checkpoint records and
  commitments. One code path for the views. Practice is untouched (its
  suites pass unchanged).
- **Receiving**: frames feed the other people's miniatures and the
  spectator view a second behind (`presentation_delay_ms`); people's
  fields are drawn from the chart itself (`TapTrack::full`: taps, the
  heads of holds and rolls and lifts drawn as taps; their bodies are
  not drawn yet); a refused attack's shocks are taken off; a field whose
  stream is more than `stale_ms` behind is dimmed; locks from shock
  moments; ends shown (out). A member who watches gets the live stage
  (`RoomClient::watches` is true in a live room), the grid and the
  focus as in practice. A reopened socket resends what was not
  acknowledged and resumes the frames (tested with frames lost too).
  `unknown_stage` for the stage in play (a server restart) stops the
  stream, keeps the play and says "The room lost the live stage; your
  run counts at the end" on the status line.
- **Settlement shown**: the room's confirmed early decision is the
  stage's decision here (`StageWon`/`StageDrawn`, the winner playing the
  song out with the fail off, "Drawn: final attack" where the room was
  settling), then the results with the Standings as before.
- **Online rules**: `attacks_offered` is true in a live room, so the
  Attacks rule is offered there; in a room that does not play live it
  stays as it was. Rooms made before the deploy are given live play by
  the server when their object loads (its sockets closed with 1012 then
  reconnect and read the policy), so they offer the rule after the
  deploy.
- **Bots** (`ddi --mp-bots`): in a live room a bot's CPU play is
  streamed, its song clock on the room's instant, the room's attacks put
  into its CPU's play (`battle::live::LivePlay`). DEVELOPMENT TOOL.
- **Debug state**: the web's `__DDI_DEBUG.live` during a live stage:
  `on`, `clock` (`offset_ms`, `uncertainty_ms`, `samples`),
  `song_zero_in`, `stream` (`acked`, `unacked`, `ended`), `frames` (the
  last read), `stale`, `refused`, `lost`.

### Tested

`crates/room`: the contract fix (fails without it); batches through a
60 s disconnect. `crates/ui` (`state/online/live_tests.rs`): live off
for an old room and on for a live one with the clock within 5 ms; the
plan offered by the picker, compared by two devices and mismatched by a
third that sits out; close 1012. `crates/session`
(`play/tests/stage/live.rs`, real plays judged by the engine on a shared
fake clock through the fake room with latency and jitter): a two-player
stage started within a frame of the room's instant, streamed, one out,
decided early by the room, the winner finishing, the runs reported and
the song ended; attacks both ways, each into its recipient's play and
onto the other's view, the same counts as the room's ledger; a watcher
following both fields and the attacks; a late join inside the bound and
one past it forfeiting and watching; a socket dropped mid-stage (and
frames lost) ending with every tap seen by everyone; every attack
refused by a device whose frames come three seconds late, and taken off
the others' views; a bot's CPU play beside a person.

### Not done in this round

- No full live stage in the UI harness (the session tests drive the room
  clients and plays directly) and no desktop offscreen live stage.
- No real server, real latency or two real browsers: the backend front's
  server and a deployed or local one are the next check.
- A remote person's percent is not on the stream (it carries DDI's sum):
  their miniature shows the sum where the player chose the percent.
- Holds' bodies on remote fields; a remote Boo, mine or hold end shows
  only in the numbers.
- The clock is mapped to the audio clock once, at the play's start (no
  drift correction over a song).

## A person in a live room went unattacked (found and fixed, 2026-11-13)

The maintainer's first live room against their local server, their browser
among bots with Attacks on: "I haven't received any attack, but other
players seem to have received."

### The cause

A stage's order (`StageOrder`, with what live play needs: `LiveOrder::prep`,
the reading horizon and the battle plan) is taken once, when the stage's
prompt opens (`screens::room::Pump::open_prompt`, the bots' `stage`). For
the room's picker the prompt opens as soon as the readiness window does,
before its own load has finished and it said it plays live (`prepared`);
for anyone it can open before the room's plan is known (a guest whose
song came in before the picker's offer). The play then started from that
copy: no preparation, so it did not stream, had no battle plan and no
reporter. The room had the device in its ledger all the same (its
`prepared` came later, before its Loaded): it ended that stream as a
no-show at 5.75 s, which the ledger takes as out from the start, so
nobody could target it again and it earned nothing; every attack between
the others went on. A host bot had the same fault (its `LivePlay` was
none), and guest bots whose song came in before the plan.

The in-process tests never saw it: their harness took every order after
the loads and the plan were done.

Reproduced in process (`play/tests/stage/live.rs`, `person_among_bots`):
a person's real `PlaySession` among three bots streaming as `--mp-bots`
does, orders taken as the screens take them. Before the fix, the person
as picker: 0 attacks committed against them in each of the six runs
tried, the bots 2 to 4 each.

### The fix

A play reads what live play needs from its room client when it starts
(`battle::live::prep_now`, in `Battle::with_live` and `LivePlay::new`),
the order's copy only as a fallback. By the stage's start the device has
reported Loaded, which in a live room it does only after saying it plays
live, so the preparation is there. Nothing in the contract or on the
wire changed; the server needs nothing.

### The timing budget, measured

The refusal lead was checked at the same time, since a person's device
refuses a commitment that comes after its first shock's warning is due.
Commitments are taken from frames as they arrive (not at the views'
one-second delay). Over 36 stages in process (a person among three bots,
as picker and as guest, x2.5 and x0.5, three seeds; 20, 80 and 200 ms
each way with as much again of jitter), no attack was refused and the
person was attacked as often as the bots (2 to 4 each). The least time
to spare when a commitment came: 1.51 s at 20 ms, 1.27 s at 80 ms,
0.78 s at 200 ms (up to 0.4 s one way, 0.8 s a round trip); a slow speed
adds to it. The plan's worst case (0.15 s) assumed a checkpoint run at
its deadline; with every report in, it runs a tick after the last.

### Diagnostics

- The web's `__DDI_DEBUG.live.me` during a live stage: `streams` (this
  play is streamed), `plan` (it has the battle plan: attacks can go into
  it), `horizon_ms`, `against`, `taken`, `late`, and `incoming`, each
  attack against this play with its sender, checkpoint, first shock,
  when it came, `spare_ms` (negative: that late) and its `fate` (taken,
  late, out, rejected).
- The battle log (the web's console, desktop's terminal, each bot's
  lines): the live part as the play starts ("live: stage N: streamed,
  with the battle plan, reading 2.10 s ahead", or why not), then each
  attack against this play taken with its spare time or refused with
  why and how late.
- Nothing on the screen: a refused attack is one its target never saw
  (the others' views take its shocks off), refusals were none at these
  latencies, and a line about an attack that did nothing would be noise.

### What to read after the next live stage

`__DDI_DEBUG.live.me` in the browser's console: `streams` and `plan`
true, `against` as many as the room's ledger committed (the stage's
standings), `late` 0; where `late` is not 0, `incoming`'s `spare_ms`
says by how much, and `clock.uncertainty_ms` how sure the device was of
the room's time.

### Against the local server

Four bots (`--mp-bots --host --guests 3 --level 5-8`) in a live room
against a local `wrangler dev` of the server, one stage of Beatucada's
Challenge chart: every bot streamed with the battle plan, the host bot
included (before the fix it would not have), the room committed ten
attacks (two to three against each), every one taken, with 1.49 to
2.18 s to spare.

### Tested

`a_person_is_attacked_as_the_bots_are_at_a_slow_links_latency`: at
200 ms each way with 200 ms of jitter, the person as picker and as
guest, two seeds: attacked at least once and no fewer times than the
least-attacked bot less one, every attack taken, none refused for
anyone (it fails before the fix). The study behind it,
`person_among_bots_study` (ignored; `cargo test -p ddi-session --lib
person_among_bots_study -- --ignored --nocapture`). The live tests'
harness takes each device's order as its screens do.

## Joining a room full of players: in to watch (2026-11-13)

The maintainer, on the Play/Watch choice: "by default join as 'Play'
unless the room is full, then join as 'Watch'". Everyone still joins
meaning to play; where every player's place is taken, the join succeeds
as a watcher, never refused for being full of players.

- **As found**: the reducer refused a join asking to play past the
  competitor cap (`Rejection::Cap`), and the server's join route asked
  again to watch on that refusal, so an online room already seated such
  a person watching, silently (the invite's page said "You can join
  now", nothing said why the player watched). The server's `full`
  (admission) was, and is, the member limit, not the players' cap.
  `FakeServer` refused.
- **The rule, one decision in `ddi-room`** (`join-watch-v0`,
  PROVISIONAL): `RoomState::players_full` (one more who wants to play
  would pass `RoomPolicy::check_competitors`) and
  `RoomState::arrival_intent`, which the reducer's `Command::Join` makes:
  asking to play where the room is full of players joins to watch
  (`MemberJoined` carries the intent, so every mirror agrees). The
  server's join route and `FakeServer` follow it by running the reducer
  (the server's retry is gone); the server's invite preview says it
  ahead (`players_full`, additive).
- **What bounds a room's members**: `RoomPolicy::max_members`, 32
  people besides CPUs (the server's `ROOM_MAX_MEMBERS`), administrators
  four more (`admin_reserve`), bots eight of them
  (`MAX_BOTS_PER_ROOM`); watchers count in it. At the member limit the
  join is refused as before (`room_full`; the invite's page "This room
  is full."). So a room has at most nine who play and the rest of its
  32 watching. Kept; an audience beyond the members is later work.
- **Told once**: the invite's page says "The room is full: you'll join
  to watch." in the place of "You can join now…" (or of the running
  match's line), and Join stays offered. After joining, the room's
  status line says "The room is full: you're watching. Choose Play when
  a place frees up." for 12 s from the room's first connection
  (`LinkNotice::JoinedToWatch`, `JOINED_TO_WATCH_MS`; carried from the
  join's answer, `Joined::intent` Watch on a new join, by
  `Online::take_seated_to_watch`), then the room's own line. Their line
  in the players panel and pop-up says "Watching", as any watcher's.
  Nothing new on the main screen.
- **A place later**: nobody is moved to Play when a place frees up
  (PROVISIONAL: they may be away from the screen). The watcher chooses
  Play with the existing control (More, the Players pop-up's foot);
  while the room is still full it is refused in words of its own,
  "Every player's place is taken. Choose Play again when one frees up."
  (it said "Couldn't do that (Cap)." before).
- **Older peers**: a new game against a server that predates this has
  no `players_full` in the preview (the page says "You can join now"),
  but that server seats the person watching all the same (its retry),
  and the join's answer says Watch, so the room's line says it after
  joining. Its `full` (the member limit) still shows today's refusal.
  An older game joining a newer server's full room joins as a watcher
  and sees "Watching" on its line with no explanation (accepted). The
  server needs a deploy for the preview's sentence; nothing else of it
  changed in behavior.
- **Tested**: `ddi-room` (the join at the cap in to watch, the event's
  intent, the member limit refusing, Play refused while full and taken
  once a place frees up with nobody moved; the same through
  `FakeServer`, the members' mirrors and `LocalAuthority`); `ddi-ui`
  over the fake room (the invite's sentence from the preview, the join
  in to watch, the status line and its end, "Watching", the refusal's
  words, Play taken later), the `qps-ploc` walk (the preview, the
  notice, the refusal), `xtask l10n check`; `../ddi-server` (the join
  route and the preview at the cap and at the member limit).

### A watch-only link (2026-11-13)

The maintainer: "additionally we could offer an option for the host to
create a Watch-only link". Built as a second invite beside the room's.

- **The link** (the server): each room's invite has a watch-only twin,
  `<room id>.<secret>` like it, its secret a one-way hash of the
  invite's (`invite::watch_secret`), so the watch link cannot be turned
  into the room's own. Replacing the invite (More's Replace invite link,
  the owner's) replaces both; the old watch link then says it was
  replaced, as the old invite does. Expiry is the invite's. The invite's
  answer to its operators (`GET /rooms/<id>/invite`, the owner, co-hosts
  and administrators, who see the room's invite today) carries
  `watch_invite`; the preview carries `kind` (`play` or `watch`).
- **Who may copy it**: whoever sees the room's invite today (the owner
  and co-hosts; More's "Copy watch-only link" under Copy invite link,
  "Watch-only link copied." on the status line). Only where the server
  answered a `watch_invite`, so never against a server that cannot honor
  it.
- **Its page**: "You're invited to watch." in the place of the
  admission's sentence (the closed and full refusals stay theirs), and
  the button **Watch** (signing in as for any join: "Sign in with Twitch
  to join").
- **The rule** (the model, `ddi-room`): a newcomer by it joins to watch
  and is held to it, `Member::watch_only`, set by the authority right
  after the join (`Command::SetWatchOnly { on: true }`, authority only;
  `Event::WatchOnlyChanged`). Their Play is refused
  (`Rejection::WatchOnly`: "You came by a watch-only link: the host can
  let you play.") and their screens offer no Play (More and the Players
  pop-up have no Play/Watch item for them). **The owner lets them play**:
  Host options' row for such a person says "Watch-only link" under the
  name, and its first button is **Let play** in the place of Make
  co-host (`SetWatchOnly { on: false }`, under
  `Capability::ChangeAccess`, the owner's, as the room's access is); once
  let, the row is anyone's again and the person chooses Play themselves
  (nobody is moved). An owner is never held to it (a watch-only person
  handed the room may play). Someone already a member who opens a link
  of either kind is answered as they are; the "room is full" line is not
  said to someone who came by the watch link.
- **Older peers**: all additive (`watch_only` left out of the JSON when
  false, a new command, event and refusal, `kind` and `watch_invite`
  fields). A game that predates it never sees a watch link (it reads no
  `watch_invite`); opening one in it shows the room's ordinary page and
  joins, held to watching; its Play is refused with a refusal it cannot
  read (passed over, as an unknown frame is), and the owner's
  `watch_only_changed` is mended by the room asked for again. A server
  that predates it gives no `watch_invite`, so nothing is offered.
- **Tested**: `ddi-room` (held by the authority alone, Play refused,
  nobody but the owner lets them, let and then Play, an owner never
  held; through `FakeServer::join_watch_only` and every mirror);
  `ddi-ui` over the fake room (More's item after Copy invite link, the
  page's sentence, the join held, no Play offered and no "full" line,
  the refusal's words, Let play and then Play), the `qps-ploc` walk
  (the page, the copied line, the refusal, Host options with a held
  person and after Let play); `../ddi-server` (the twin's secret, its
  preview, the join held and answered as before on a second join, the
  room's own invite unchanged, both replaced together, expiry).
- **Needs the server's deploy.**

### Attacks On by default in a private room that plays live (2026-11-13)

The maintainer: "I think Attacks should be on by default on private
rooms."

- **Where the default lives**: `ddi_room::MatchRules::hosted_for(policy)`:
  `MatchRules::hosted` with the Attacks rule On where the room's policy
  plays live (`RoomPolicy::live`: its authority runs the attack ledger),
  as practice's (`MatchRules::practice`); Off where it does not. The
  server makes a new room's rules with it (`RoomCore::create`), as
  `FakeServer::hosted` does; Rules' Reset in an online room restores it
  (`room_dialogs::defaults`). The host still turns them Off in Rules.
- **What is played follows what really runs**, unchanged: a room that
  does not play live (a server from before live play, or a room made
  there) plays without attacks and says so ("Off · not in online rooms
  yet") whatever its stored rule says (`RoomClient::attacks_played`,
  `attacks_offered`), and its stage's battle is survival.
- **Rooms made before**: their stored rules stay as they were (a live
  room made before the server's deploy has Attacks Off, its hosted
  default then, until its host turns them On; Reset there now gives On).
  Nothing flips a stored rule.
- **Summaries**: they say only what differs from the default, so a live
  room's rules with Attacks Off read "DDI · No attacks", as practice's.
- **Needs the server's deploy** for new rooms to start with them On.
- Tested: `ddi-room` (`hosted_for` with and without live play, the fake
  server's rooms), `ddi-ui` over the fake room (a live room's Rules On
  by default, Off applied and summarized "No attacks", Reset back to On;
  the non-live room's row as before), `../ddi-server` (a new room plays
  live with Attacks On).

## The Rules pop-up during a match: said at its top, and a restart (2026-11-13)

The maintainer: "rules can't change mid-match but the Rules pop-up
doesn't convey that very well, maybe it can both convey it and offer to
restart the current match, or we can even support mid-match rule changes
(although it doesn't always work)." The first is built; the second is
written down as an open option (below).

- **As found**: a match's rules lock when it starts. The Rules pop-up
  edited the next match's draft (Cancel and Apply) under a small "This
  match · locked" line with the running match's rules in a few words,
  and a muted note at the foot ("Changes apply to the next match; this
  one keeps its rules."). Applying mid-match changed nothing anyone
  could feel until the match ended. More's Stop match interrupts it.
- **Said where it cannot be missed** (`room_dialogs::RulesDialog`):
  while a match runs (`PhaseKind::match_running`: a stage's readiness,
  countdown, play and song end, the intermission) the pop-up's title is
  **"Rules for the next match"**, and its top says "This match is on
  stage 1 of 4 and keeps these rules:", the running match's rules on the
  next line in the existing few words ("4 stages · 3 CPUs · DDI · Hard
  timing"), then in gold "The rows below are for the next match." The
  "Next match" heading and the foot's locked note go (the statement says
  both). A row whose draft differs from the running match's (stages,
  timing difficulty, Boos, the CPU roster, Attacks) has a small gold
  **●** in the margin before its label, and the sentence then adds "●
  marks what differs from this one." A member who cannot edit sees the
  same statement over read-only rows, with Close. Between matches (the
  lobby, a match's results, a stopped match) the pop-up is as it was.
- **Apply and restart**: beside Apply, at the foot's left (on a row of
  its own where the pop-up is too narrow), for whoever may both save the
  rules and stop the match (`Capability::EditNextRules` and
  `StopMatch`: the owner, co-hosts, administrators), offered only while
  a match runs, in StopMatch's own phases (all of the above, a stage's
  play and countdown included: stopping is allowed there, so is this).
  It checks the draft, then asks once: "Restart the match?" / "This
  match stops now, and its points so far won't carry over. The next
  song picked starts a new match under these rules." (Cancel focused
  first; Restart match). Then the draft is saved and the match stopped;
  the stopped match's standings pop up as after Stop match (its
  finished stages, recorded as an interrupted match), and the status
  line says "Match stopped. Pick a song to start the next match under
  the new rules." (said wherever a stopped match's next rules differ
  from the ones it was played under, so also after Apply then Stop
  match). The plain Apply still saves the draft for the next match.
  Keyboard (Left from Cancel), controller and touch as the pop-up's
  other buttons; nothing animates.
- **One change in the room**: `Command::RestartMatch { rules }`
  (`ddi-room`): both capabilities checked, the envelope's base against
  the draft's last save (as `SaveNextRules`), the rules validated, then
  the same events as `SaveNextRules` followed by `StopMatch`'s, in one
  revision; refused whole otherwise. Two operators cannot come between
  the apply and the stop, and a restart made against a draft someone
  saved since is refused as stale.
- **Older peers**: additive to protocol 2. The room says its authority
  reads the command: `RoomPolicy::restart`, true for every room this
  build makes (`RoomPolicy::default`, which the server's rooms are made
  from), left out of the JSON when false. A room whose authority
  predates it (or a room the server stored before its deploy) has no
  such field: there **the action is not offered** (`RoomClient::
  may_restart`), never sent, and the plain Apply and More's Stop match
  work as before. Not the two old commands in turn: that would let
  another operator's command land between them. Old events only, so an
  older client mirrors a restart as a save and a stop. **Online rooms
  need the server's deploy** to offer it (no code change there: the
  server builds its rooms' policy from the default); practice has it at
  once.
- **Tested**: `ddi-room` (`walkthroughs`: refused in the lobby, to a
  member, with rules the room cannot use and against a stale draft;
  a co-host's in the intermission as one revision, saved then stopped;
  refused once stopped; the next match under the new rules; during a
  stage's play; a policy without the field reads false. `online`: the
  local authority, `FakeServer` and every mirror agree). `ddi-ui`
  (`room_tests`: practice before a match as before, the statement, the
  marks, the action, the question and its Cancel, by keyboard and by
  controller, the restart, the status line, the pop-up after it, the
  next pick under the new rules; an online room's member read-only with
  no action, a co-host restarting for everyone, a room from before the
  command never offered or sent it while its Apply still saves), the
  `qps-ploc` walk (the statement with a row marked, the question, the
  status line), `xtask l10n check`. Headless Chrome and Firefox in CPU
  practice at 1280×720 and 844×390, English and Japanese
  (`target/scratch/mp/rules/shots/`).

- **Open**: after a restart (and after Stop match) the stopped match's
  standings pop up even where no stage was finished, a table of zeros.
  Skip them there?

### Changing rules from the next stage on (built, 2026-11-13)

The maintainer's second alternative, queued after the restart: a running
match's rules changed from a later stage on. Built in the room's model,
the Rules pop-up, practice and online (over the fake server; the
server's deploy offers it online).

- **What may change**: the rules each stage is played under on its own
  (`ddi_room::PlayRules`): **Timing difficulty, Boos, Attacks** and
  **At zero life** (Out / Step down: per stage by its nature, a stage's
  ladder and plan are worked out per stage). **Points does not**: a
  stage's award is per stage, but the bias decides what every stage is
  worth against the others, so changing it mid-match would make the
  match's total a sum of two scales; the simpler honest choice is the
  match's Points for its whole length (Next match or Apply and restart
  for it). Open question below. Never: the stage count, the seats and
  the practice roster, the ruleset.
- **When it lands** (`RoomState::rules_land_on`): on the next stage
  nobody has begun: not one whose readiness window is open (a pick opens
  it with the song), whose countdown or play has begun, or that anyone
  readied or loaded for. So a change saved in the intermission before the
  next song is picked lands on that next stage; once it is picked (or
  counting down, or playing) it lands on the stage after. The pop-up says
  which before Apply ("From stage 3"), and follows the room while open.
  With no stage left (the last stage gathering or playing) the pop-up
  is today's form.
- **The model** (additive to protocol 2): `Match::changes` (each
  `RulesChange { from, rules, saved_at }`, a later change from the same
  or an earlier stage replacing those from its stage on; left out of the
  JSON while empty), `Match::rules_for(stage)` (every reader of a stage's
  rules: the stage's order and so the play, its CPUs, the bots, the
  battle preset and the ledger, the live plan, the room's settlement and
  live server, the Standings), `Match::stage_rules_changed`.
  `Command::ChangeStageRules { from, rules }`: `EditNextRules` (the Rules
  pop-up's operator), a running match, the envelope's base against the
  last change (`Match::changes_saved_at`, else `Rejection::Stale`), and
  `from` the stage it lands on now, else `Rejection::RulesLandLater {
  asked, from }` naming where it would land (never moved silently: a
  change racing a pick or a start is ordered by the room's revision);
  Step down only where the authority honors it
  (`RulesProblem::StepDownNotHonored`). `Event::StageRulesChanged`, which
  every mirror applies. `RoomPolicy::stage_rules` says the authority
  reads the command (true for every room this build makes, left out when
  false): a room without it never offers the choice nor is sent the
  command, and the pop-up is today's. `RestartMatch` and the next
  match's draft are unchanged.
- **Two devices never on different rules**: a change never lands on a
  stage once picked, so every device loads a stage with its rules known.
  The battle plan also names a changed stage's rules
  (`BattlePlan::stage_rules`, e.g. `hard/boos/out`, in the digest only
  then: a stage under the match's own rules has the plan it had), so a
  device that would play it under the match's rules (one that predates
  the change) disagrees on the digest and sits it out. Proved over
  `FakeServer` (`a_live_change_of_rules_lands_after_the_stage_loaded_and_both_devices_agree`):
  Alice changes the timing while stage 1 loads (after its plan was
  worked out); it lands on stage 2; both devices play stage 1 under the
  match's rules and stage 2 under Hard, the same plan digest on both, an
  older device's another.
- **The Rules pop-up** (`room_dialogs::RulesDialog`), while a match runs
  in such a room, for whoever may edit rules: a row **Changes apply: ‹
  From stage 2 ›** under the statement, From stage N first; its other
  value **Next match**. From stage N: the title "Rules from stage 2", the
  statement "This match is on stage 1 of 4, under these rules:" (the
  rules of the stage it is on), then in gold "Timing, Boos, zero life and
  attacks below apply from stage 2 on." (● marks the rows changed from
  that stage's rules); the per-stage rows edit that stage's rules, the
  others (Stages, the CPUs, Points) are shown as values with a quiet
  note: "Stages, CPUs and Points can't change during a match: choose
  Next match for them, or Apply and restart." Next match: as before the
  round (the rows the next match's, marked against the running match).
  Apply saves each edit where it was made; **Apply and restart** keeps
  its place and takes what the pop-up shows (from From stage N, the
  per-stage rows as shown become the next match's). Keyboard and
  controller as the other rows (the destination is a ‹ › pair); a
  member sees today's read-only form. Pictures:
  `target/scratch/mp/nextstage/shots/` (`rules-from-stage.png`,
  `standings-marked.png` from the desktop's offscreen test;
  `2-from-stage-*`, `3-marked-*`, `4-status-*`, `5-next-match-*` from
  headless Chrome and Firefox).
- **Telling everyone**: the room's status line, for everyone, from the
  change until its stage begins: "From stage 2: Hard timing · Boos · No
  attacks. Pick stage 2's song." (what differs from the stage before,
  `room::change_words`; then the line it would say). The stage's prompt
  says that stage's own rules (its order's). A refused change says "The
  stage these changes were for has begun since. Check where they apply
  now, then Apply again." The Standings tab beside its heading, where the
  stage was played under changed rules: "● Changed rules: DDI · Hard
  timing"; the match's standings pop-up a gold line "● From stage 2: DDI
  · Hard timing" a change and a gold ● on each such stage's column
  heading. The Players panel is untouched. The audience's public view
  keeps the match's rules and names a changed stage's
  (`PublicStage::rules`, additive); a seat plays and words each stage by
  its own.
- **Online, what a deploy changes**: the server builds against the room
  crate by path and needed no change (its clippy, tests and the wasm
  check pass as they are); once deployed, its rooms say `stage_rules`
  and decide the command. Before the deploy the deployed server's policy
  says nothing, so a new client shows today's form there and sends
  nothing. An older client in a newer room: it reads `changes` as
  nothing, so it would play a changed stage under the match's rules; its
  plan digest differs where the stage has attacks, so it sits such
  stages out; a stage without attacks has no plan, and the older client
  would play it under the match's timing and Boos (open question).

PROVISIONAL choices: where a change lands (above); Points fixed for the
match; Apply and restart taking the per-stage rows as shown.

Tested: `ddi-room` (`walkthroughs`: between stages, a second change
replacing the first, back to the match's own, refused in the lobby, to a
member, without a base, with no stage left, racing a pick, stale, Step
down where not honored, the record keeping each stage's rules, the JSON
additive both ways; `online`: both authorities and every mirror alike;
the plan's digest), `ddi-ui` (`room_tests`: the three destinations, the
status line, the refusal, the Standings' marks, a member and an older
room never offered it, an online co-host's change over the fake reaching
every mirror and status line; the `qps-ploc` walk; `xtask l10n check`),
`ddi-session` (a practice match through its stages: Hard timing from
stage 2 and attacks off from stage 3, each stage judged, its battle and
ranking under its own rules; the live test above), desktop offscreen
(`room_rules_offscreen`), headless Chrome and Firefox at 1280×720 in
English and 844×390 in Japanese.

## Rooms of 1+72 players: the award curve, the caps, the screens for a large field (2026-11-13)

The maintainer (2026-11-12/13): "I want to increase the room limit from
1+8 to 1+72 players, and for 'watch only' I'd like to have no limit (at
least within reason of what the Cloudflare deploy can work with)."

### The award curve (decided by the maintainer)

Fields of nine and more read the eight-player table (12, 9, 7, 5, 4, 3,
2, 1) stretched down the field and biased toward the top
(`ddi_room::TopBiased`, rules `AwardRule::TopBiased{Strong,Medium,Mild}V1`,
ids `top-biased-strong-v1` …). Place `p` of `N` is `u = (p − 1) ÷ (N − 1)`
down the field; the table is read at `7 × u^k`, linearly between its
entries, rounded to a whole number (halves up), with
`k = b + (1 − b) × 7 ÷ (N − 1)`: 1 at eight players (the table itself),
falling toward the bias `b` as the field grows. First place stands alone
in every bias (any other place that would round to 12 gets 11: the
maintainer, "first place alone at 12 seems good for all curves"). Equal
counts for neighboring places are fine (the maintainer).

The awards are fixed tables in the source (for each bias and each field
from 9 to 73, how many places earn 12, 11, … 1), never a platform `powf`
in a room's decision: the room runs natively and as wasm. A test
recomputes them from the formula in double precision and checks that no
value lies within 10⁻⁴ of a rounding boundary (the closest is 2.4·10⁻⁴:
Medium, 13 players, place 6), so any correctly rounded `powf` gives the
same integers.

| Field | Strong (b = 0.4, default) | Medium (b = 0.5) | Mild (b = 0.6) |
| --- | --- | --- | --- |
| 9 | 12 9 7 5 4 3 3 2 1 | 12 9 7 5 4 3×2 2 1 | 12 9 7 5 4×2 3 2 1 |
| 12 | 12 9 7 6 5 4 4 3 3 2 2 1 | 12 9 8 6 5 4×2 3×2 2×2 1 | 12 9 8 6 5 4×2 3×2 2×2 1 |
| 16 | 12 9 7 6 5×2 4×2 3×3 2×3 1×2 | 12 9 8 7 6 5 4×3 3×2 2×3 1×2 | 12 10 8 7 6 5×2 4×2 3×3 2×2 1×2 |
| 33 | 12 9 8 7 6×2 5×3 4×5 3×7 2×7 1×5 | 12 9 8×2 7 6×2 5×4 4×5 3×6 2×7 1×4 | 12 10 9 8×2 7×2 6×2 5×3 4×5 3×6 2×6 1×4 |
| 73 | 12 9 8×2 7×3 6×3 5×6 4×12 3×15 2×19 1×11 | 12 10 9×2 8×2 7×4 6×4 5×7 4×12 3×14 2×16 1×10 | 12 11 10 9×2 8×4 7×4 6×4 5×8 4×12 3×13 2×15 1×8 |

The bias is a match rule, `MatchRules::points` (`PointsBias`: Strong, the
default, Medium, Mild), locked with the match like the others; additive
(left out of the JSON at Strong, read as Strong where missing). The
Rules pop-up's Points row is its choice (‹ Strong ›, a sentence under it
saying what the choice favors, brighter while the row is focused, and a
quiet note that it changes nothing with eight players or fewer); the
running match's changed rows mark it; the rules' summaries name it only
when it is not Strong ("DDI · Medium points"); Reset gives Strong.
`RoomPolicy::points_bias` (additive, true for every room this build
makes) says the authority reads it: a room on an older server shows the
fixed row as before ("By place · experimental") and never offers a
choice it cannot honor.

`ExperimentalNineV0` is retired: never awarded; still read (an authority
one build behind awards nine so, and a new client in its room reads its
results). Nothing stored names it on the deployed server's side that
this build cannot read. `AwardRule::Unrecognized` (`#[serde(other)]`)
reads a newer authority's rule without breaking the result (its
placements and points are explicit). Fields under eight keep
`ExperimentalLinearV0` (not decided: an open question below). The
screens' "points experimental" marks follow `AwardRule::experimental`:
under eight (and development's) only.

### The caps

| Cap | Value | Where |
| --- | --- | --- |
| Competitors (people and CPUs), online and practice alike | 73 | `RoomPolicy::INITIAL_COMPETITOR_CAP` (one policy check, `check_competitors`); `TopBiased::MAX_FIELD` follows it |
| A hosted room's seats | the cap, at most its authority's | `MatchRules::hosted` (73), `hosted_for` (`min` with the policy's, so Reset on an older server asks for nine) |
| Members (not administrators), watchers included | 384, PROVISIONAL (`member-bound-v0`) | `RoomPolicy::DEFAULT_MAX_MEMBERS`; the server's `ROOM_MAX_MEMBERS` var overrides it, raised to at least the competitor cap (`RoomConfig::from_vars`) |
| Administrators beyond the members | 4 (unchanged) | `RoomPolicy::admin_reserve` |
| Practice's CPUs | 72 (the cap beside the player) | `RoomClient::max_cpus`; the roster's 24 named CPUs, then the same again as "Lumen Kite 2", "… 3" (`cpu_roster::pool`; Add custom's list never longer than 24) |
| Bots in one run (desktop's `--mp-bots`) | 73 | `bots::MAX_BOTS`; names past the roster's 24 numbered the same way |
| Bots in one room (server) | 73 | `../ddi-server` `bots::MAX_BOTS_PER_ROOM` |
| Bot sessions an hour per admin token (server) | 200 (was 60) | `../ddi-server` `bots::ISSUES_PER_HOUR` |
| Development practice beyond the cap | 129, kept for stress runs past 73 (`mpcpus=`, `--mp-dev-cpus`); its awards `DevelopmentLinearV0` | `RoomPolicy::DEVELOPMENT_COMPETITOR_CAP` |

Measured for the member bound (`crates/room/tests/scale.rs`,
`what_a_large_room_costs`, release, native, this Mac): the room's state
as JSON (what the server writes as one row on every change and sends in
each snapshot), and a join or a member's connection change (the reducer
and the state serialized, as the server's row write does):

| Players | Watching | State JSON | Join | Connection change |
| --- | --- | --- | --- | --- |
| 73 | 0 | 12.1 KB | 65 µs | 58 µs |
| 73 | 100 | 27.4 KB | 163 µs | 132 µs |
| 73 | 200 | 42.8 KB | 266 µs | 226 µs |
| 73 | 300 | 58.2 KB | 357 µs | 290 µs |
| 73 | 500 | 89.1 KB | 562 µs | 453 µs |
| 73 | 1,000 | 166.5 KB | 1.06 ms | 0.83 ms |

Every membership change also sends one small event to every member's
socket, so a wave of `n` joining costs `n²` sends; and a snapshot of 58
KB to each newcomer. 384 (73 players and about 300 watching) keeps a
change under half a millisecond natively (the Worker's wasm perhaps two
to three times that), the state well under Durable Objects' 2 MB row,
and is what the server's own load test carried (73 and 300 watching
sockets: frames on time, a quarter of a core). Beyond it is the
audience (counted, not listed): not this round
(`multiplayer-online-play.md`, "Watching and an audience").

Older peers: a new client in a room on an older server reads that
server's policy (cap nine, no `points_bias`): the players' count says
"/ 9", the Rules' Points row is fixed, Reset asks for nine seats. An
older client in a room of this build lists everyone it is given (its
lists scroll as before, "n more"), but it cannot read a stage result
awarded by the new tables (an unknown rule name in a `StageConfirmed`
event and in snapshots): from the first confirmed stage of nine or
more starters it falls out of step and asks for snapshots it cannot read.
Nothing before that differs. An older client must be reloaded (the web)
or rebuilt (desktop) for rooms of nine or more; this build reads unknown
rules from now on.

What a deploy changes: the server's competitor cap becomes 73 for every
new room (a stored room keeps its policy: nine), new rooms' member bound
the deployed `ROOM_MAX_MEMBERS` (`worker/wrangler.toml` says "32", which
the server now raises to 73; set it to "384", or remove it, for watchers
beyond a full field: the maintainer's), 73 bots a room and 200 bot
sessions an hour per admin token.

### The screens for a large field

Small rooms (eight opponents or fewer, nine rows or fewer) take exactly
the paths they took (the arrangement, the rotation, the lists' orders);
only the cap they show reads "/ 73" where it read "/ 9".

- **In play** (`ddi_shell::rivals`, `opponents::arrange`): past eight
  opponents the side space has the eight slots a room of nine has (the
  left side a status line's height shorter), and those that matter to
  the player now in them: whoever attacks them or whom they attacked
  (the stage's kept seats, as before), then the nearest rivals by
  standing (still in, life, then score), as many above as below
  (`Rivals`, `RIVALS_V0`, PROVISIONAL: a slot's opponent stays at least
  8 s; one within two places and a twelfth of the field of the nearest
  keeps its slot, so two
  rivals trading places every tick never change a slot; an attacker
  comes in at once in the farthest rival's slot; a new opponent fades in
  over 0.3 s, at once under reduced motion). The old take-turns
  rotation stays for a room of nine in a window too small for eight.
  Under the left side's slots, the ladder (`rivals::Ladder`,
  `opponents::draw_ladder`): "20 of 73 · 73 still in" and a thin strip
  of everyone's life, best first, the player's own mark gold and taller;
  clear of the HUD, the watermark, the quit button and the attack
  warning (the slots' rooms keep its line). `__DDI_DEBUG.large_field`
  (`words`, `place`, `of`, `still_in`, `shown`).
- **Feeds**: every competitor's stream is still taken whole (the large
  field's miniature needs its moments when it comes into a slot); what a
  frame asks of a field not drawn is its summary (`FieldFeed::shown`:
  life, combo, out), now a search and a comparison whatever the stream's
  length (it scanned every moment for "out" before). Only the eight
  slots build a field's frame.
- **The spectator view**: a grid of more than twelve competitors
  (`ddi_session::play::GRID_PAGE`, `watch-pages-v0`, PROVISIONAL) shows
  a page of twelve by standing; Page Up and Page Down turn the pages
  (round), as do the D-pad's Up and Down on a controller (its shoulders
  keep following a competitor) and a tap on the ladder; the hint says so
  for the first seconds ("Tab or tap to follow · Page Up/Down or D-pad
  for more"); the ladder takes the status line's right half ("Page 2 of
  6 · 41 of 73 still in"); following someone keeps the page for coming
  back. The focus view's side fields are chosen as a player's are,
  around the competitor followed. `__DDI_DEBUG.watch.page`.
- **Lists**: past nine rows (a large room) the players panel and the
  Players pop-up list the local player first, then the operators (and
  anyone with a role), then people, then CPUs; the panel orders by place
  after the player's line; Host options lists co-hosts and administrators
  first. A stage's Standings (the results' tab and the room's pop-up)
  keep the local player's line in view however far it is scrolled (the
  window's first line above it, its last below: `room::window_keeping`),
  and the match's result on top names the first few (one line) and the
  player's own place rather than everyone. The stage prompt's list had
  the player first and scrolled already.
- **Readiness with many**: the window (`hosted-deadline-v1`) waits only
  for those who could still join and ends early once every connected
  player who could join is ready and loaded; at its deadline the ready
  start and the others sit out. One person not ready therefore delays 72
  others by the window at most (30 s), never vetoes the start; nothing
  changed. The status line's words do not count people; nothing breaks
  at this size.
- **Attacks at this size**: the targeting policies are the ledger's
  (tested to 128 seats). What a player is shown of attacks is theirs
  (`Battle::links` and the warning: attacks against them and their own),
  with the kept seats bringing those opponents into slots; others'
  attacks show only where both fields are drawn; the announcer's battle
  lines are one at a time (`BATTLE_VOICE_V0`). Nothing floods at 72.

### Measured

- Practice with 72 CPUs, desktop offscreen (optimized `desktop` profile
  without LTO, this Mac, `opponent_fields_offscreen`
  `miniature_scenarios_pictures_and_costs`, `DDI_FIELDS_ONLY=practice`):
  planning the stage (the chart read and 72 CPU plans) 0.56–0.59 s
  (16 CPUs: 1.43 s, the first load); a frame recorded 0.41–0.42 ms mean,
  0.72–0.76 ms p95, 1.75 ms with the GPU's work (16 CPUs: 0.29 ms; 32:
  0.33 ms; 64: 0.36 ms). Practice's bound is the cap: 72 stays smooth.
- In the browser (`target/scratch/mp/scale/scale.mjs`, a release build,
  practice through the development switch `mpcpus=N`, so its awards are
  development's): frame times over 5 s of play, rAF intervals: Chrome
  (WebGPU) with 16, 33 and 72 CPUs at 1280×720 and 72 at 1920×1080,
  16.6–16.7 ms mean, p95 16.7, max 16.8 (the display's 60 Hz held);
  headless Firefox (WebGL2) with 72, 13.4 ms mean, p95 33 (as a player)
  and 9.0 ms mean, p95 11.6 (watching the grid). A room of nine draws no
  ladder (`large_field` null).
- Bots on the maintainer's local server (`ddi --mp-bots --host --guests
  72 --wait-for 73 --stages 1`, the documented local token): 73 bots
  signed in and joined through the real join path (no development
  override; the local `ROOM_MAX_MEMBERS = "32"` raised to 73 by the
  server), a live stage of 73 streamed with attacks (159 attacks taken,
  13 out), settled by the top-biased table (a bot 29th of 73 earned 3
  points, as Strong's row says), and a second match begun; all left on
  Ctrl-C. `--wait-for` takes 2 to 73 now (it took 2 to 9).
- The server's own load driver was not run again: it needs its own
  `wrangler dev` with a `dev-auth` build and a configuration of its own
  on another port, which the earlier backend front set up; the bots'
  run above went through the real join path at 73 on the maintainer's
  server instead. Its 73 and 300 run (frames on time) stands.

Screenshots: `target/scratch/mp/scale/shots/` (`play-player-{16,33,72}-*`,
`grid-*`, `grid-page2-*`, `focus-*`, `results-*`, `select-*`, the
desktop's `practice-72-cpus-1280x720-at-21s.png`).

## Live play, second round: half a second behind (2026-11-13)

The maintainer (2026-11-13): "1-second delay is a bit much, I'd like to
aim for 500ms or even lower, if that proves to be too unstable we can aim
for 1s." Everything here is real and tested; the numbers are in
[Online play](multiplayer-online-play.md), "The views' delay, lateness
and ends".

### The others' fields half a second behind, or less

- **What the delay was made of** (`live-v1`): a batch every 250 ms, a
  frame every 250 ms, both trips and the clocks' error: a view must
  cover `batch + tick + up + down` just before a frame comes, so up to
  500 ms of periods alone; it waited a constant second.
- **`live-v2`** (`ddi_room::live::LIVE_V2`, PROVISIONAL): a frame and a
  batch every 100 ms; everything the room decides as `live-v1`'s (the
  checkpoint deadline, the network's lead, the late join). A room says
  it by an additive `timing` beside its contract's id
  (`LivePolicy::v2`: `{"id":"live-v1","prelude_ms":1000,"timing":"live-v2"}`).
  The server makes new rooms so; a stored room keeps its policy as
  stored (a stage in play across the deploy is not re-timed). The
  client follows its room's rules when its socket opens
  (`LiveLink::opened`, `Outbox::set_rules`); bots and the server's load
  driver use the same client, so they follow too.
- **A delay each device measures** (`ddi_room::live::delay`,
  `ViewDelay`, `VIEW_DELAY_V0` `view-delay-v0`, PROVISIONAL): sampled
  from each competitor's news as it comes (how far its stream had
  fallen behind just before) and from each stall at once; the 99th
  percentile of the last 8 s and 40 ms, within 150 ms and a second, up
  at 500 ms a second, down at 50. Display only: nothing the room
  decides depends on it.
- **The fields wholly behind** (`ddi_session::battle::feed`): a remote
  field's notes now scroll the delay behind the song too, not only its
  moments (`FieldFeed::set_delay`, `view_time`). Found on the way: with
  `live-v1`'s second, notes scrolled on the song's time while their hits
  showed a second later, so a remote person's hit notes passed the
  receptors as if missed and vanished a second after. The attack
  warnings and the lines between fields of a remote recipient are said
  on that field's time (`Battle::incoming_for`, the watcher's links).
- **Old and new**: a client that predates `live-v2` reads the id alone
  and plays live by `live-v1`'s numbers in a new room (batches every
  250 ms, its views a second behind; the room serves it as before); a
  new client in an old room (or on an old server) batches at 250 ms and
  its views still adapt (0.6 to 0.85 s). Nothing on the wire changed.
- **Measured** in process (`crates/room/tests/live.rs`,
  `view_delay_study`, 9 competitors and a watcher, three seeds): at 20,
  50, 80 ms each way (with up to 20, 30, 40 ms of jitter) the delay is
  292, 370, 479 ms on average (p95 310, 410, 500), at 80 ms with 80 of
  jitter 524 ms, at 150 ms 693, at 200 ms 822, at 200 ms with 200 of
  jitter at the ceiling (994); 73 competitors the same within 15 ms.
  Stalls: under 0.1% of field-time and at most about one a field-minute.
  `live-v1`'s constant second: 0.8% of field-time stalled at 200 ms and
  13.7% at 200 ms with as much jitter. The test
  `the_views_follow_half_a_second_behind_at_typical_latencies` holds the
  80 ms row (mean ≤ 500, p95 ≤ 520, stalls under 0.5%).
- **Attacks' budget**: unchanged in the rules, larger in fact: the least
  time to spare when a commitment came, 1.81, 1.51 and 1.11 s at 20, 80
  and 200 ms (was 1.51, 1.27, 0.78; `person_among_bots_study`, 36 stages
  each, none refused).
- **Considered and left out**: the room forwarding a batch at once (in a
  full room it saves nothing without multiplying frames; moving the
  alarm writes storage each time); batches cut on the first moment (a
  stream's time moves every frame: the period is the bound).
- **Debug**: `__DDI_DEBUG.live.delay` (`ms`, `target_ms`, `rules`,
  `live_rules`, `stalls`, `stalled_now`).

### A watched stage runs until the winner finishes

The maintainer: a watched stage should end "when the winner finishes"
the song. `watch-v1` (PROVISIONAL): after the decision the watching goes
on while anyone still plays the song (`Battle::still_playing`: in
practice a CPU neither out nor done with the chart; online a competitor
whose stream has not ended, or whose end its field, drawn the view's
delay behind, has not reached yet); the decision is said on the status
line as it comes and kept ("X wins the stage", "Stage drawn"); the
results follow 2.5 s after the last one still playing ended (the music
fading), or at the song's end; Enter, Start or the plate skip as before.
The same for a player out of the stage or who left it. Online, the room
keeps the stage in SongCompletion after its early decision and keeps
taking and sending the streams until the last report (or the report
deadline): the watcher follows the winner's stream to its end. The
session test `a_watched_stage_ends_when_the_winner_finishes_and_a_watcher_leaves_freely`
(practice, CPUs on a battery of one life): not over at the decision,
the decision on the line throughout, the results the hold after the
last one still playing ended (or at the song's end), never the hold
after the decision. A CPU winner keeps its fail after the decision (the
engine's CPU offers no way to turn it off, and that code was left
alone), so in practice a winner whose gauge empties afterwards ends the
watching there; a person who wins has their fail turned off as before.
(Fixed since: "A winning CPU plays the song out".)

### The gaps the first round left

- **A remote person's percent** (where the player's Score display is
  Percent): worked out, not sent. The stream carries DDI's sum alone
  (nothing was added to the wire); a person's taps carry their W0s, and
  the chart's most under the stage's rules is known here, so the points
  are the one count whose `floor(1,000,000 × points ÷ most) + 100 ×
  points` is the sum less the W0s (strictly increasing in the points),
  and the percent follows exactly as their own play's
  (`ddi_session::battle::live::ddi_of_sum`, tested for every count,
  negative ones too, against `DdiScoreView::of`). Where the person's
  copy of the chart differs (an option that leaves notes out) it is
  their points against this device's most: approximate there. Their
  miniature says it, and their Standings line says their own reported
  percent once their run is reported (as before).
- **The standings pop-up after a stop or a restart with no stage
  finished** is skipped (`RoomClient::unseen_results`: none where the
  match was stopped before any stage was finished, so no table of
  zeros); tested in `ddi-ui` (the client's stop, the room screen's
  Back after a stopped match).
- **The dancing hand rests** where the player went out or left the
  stage (`PlaySession::hand_rests_at`: the gauge's emptying, the
  forfeit's time, else the last judged time); the play view draws it at
  that time from then on, never at the song's (`ddi_shell::play_view`).
- **A remote person's field drawn fully** (`ddi_session::battle::feed`,
  `TapTrack::full`): their holds and rolls to their ends (the head
  pinned to the receptor while held, the receptor holding; gone once
  held to the end; drawn dropped and scrolling on once let go), their
  lifts as lifts, the chart's mines (gone when stepped on, the receptor
  flashing), shocks and the lane's lock as before, a Boo said on the
  followed field ("Boo") and nowhere on a miniature (as a play's own
  field shows it). Tested on the feed (`a_persons_holds_mines_and_boos_show_as_their_stream_says`);
  not looked at in a browser this round (no live stage with a person
  was run there).
- **The `unknown_stage` path**: `FakeServer::restart_live` loses the
  stage in play as the server's object does on a restart (its storage
  kept, every batch and resume of that stage answered `unknown_stage`,
  no frames while the room is on it). The session test
  `a_stage_the_room_lost_in_a_restart_plays_on_and_settles_on_the_reports`:
  both plays hear it, play on unstreamed (the status line's "The room
  lost the live stage"), report at their ends, and the room settles the
  stage on the reports.
- **Clock drift, measured** (`crates/room/tests/live.rs`,
  `clock_drift_study`: every device's clock 0, 50 or 500 millionths fast
  or slow, alternately, a five-minute song, the views over its last
  minute, nine competitors and a watcher at 80 ms each way). A play's
  song clock is mapped from the room's once, at its start, and runs on
  the device's own (its audio clock: the player's judging is never
  re-timed), so two devices drifting apart move each other's streams
  later or earlier by the drift. 50 ppm: nothing to see (the delay 468
  ms on average against 469 without drift, no stall). 500 ppm (a fast
  device watching a slow one: 300 ms apart after five minutes): no
  stall either, the adaptive delay rises with it (608 ms on average,
  780 at most, against 480); `live-v1`'s constant second was untouched
  by it at both. Mapping a stream's times and the views onto the room's
  clock as estimated through the stage instead (the study's
  "resynced") holds 480 ms at 500 ppm. Built since (below, "What the
  second round left").
- **Resends backed off** (found by the load test, below): a stream
  resent every unacknowledged batch each second while the room did not
  answer; at ten batches a second a room that fell behind was sent a
  growing backlog (17,000 messages a second into a room of 73 and 300
  watching, which never recovered). `Outbox` now doubles its wait after
  each unanswered resend, up to eight seconds, and starts over on an
  acknowledgement or a new socket.

## What the second round left (2026-11-13)

### The clocks' drift kept out of the views

Built as the study proposed (`ddi_room::live::slew`, `SongSlew`,
`SLEW_V0` `slew-v0`, PROVISIONAL; [Online play](multiplayer-online-play.md),
"The clocks' drift"): each frame of a live play
(`ddi_session::battle::live::Live::sync`) its song time plus a correction
that follows the room's clock as estimated, averaged over 4 s and moved at
most 2 ms a second, never backwards; its moments and its stream's
judged-to time are sent on it, and its views' delay is measured and drawn
on it. **The player's own judging is untouched**: the presses, the notes'
times, the music, the attacks' fairness (a commitment is still checked
against the play's own judged time) and the checkpoint reports (chart
times) stay on the audio clock. A viewer puts each remote competitor's
moments back on that competitor's own song clock by the shift its taps
say (no wire change), so a hit still meets its note; the attack warnings
and links of a remote field are said on that field's time
(`Live::field_delay_s`). Where the clocks disagree past the estimate's
uncertainty (and 20 ms), the log says so and the debug state shows it
(`__DDI_DEBUG.live.slew`); the room is told nothing more.

Tested: `ddi-room` `the_views_keep_their_delay_when_the_clocks_drift`
(the study's rows as a regression test: 50 and 500 millionths over a
five-minute song, three competitors and a watcher: 459 and 482 ms against
608 mapped once at 500, no stall) and the slew's own two tests;
`ddi-session` `a_live_stage_keeps_the_room_s_clock_while_the_devices_drift_and_judges_as_without`
(two plays 500 millionths fast and slow on the fake room: every note's
judgment byte-identical to the same plays without drift, each correction
the drift's, each shift the other's correction, the delay as without
drift and no stall). The study prints a third row, "mapped once, kept in
step".

### The browser's Back during a stage's play

The web's play of a room's stage pushes a history entry of its own over
the play's (the same address, its trail one longer; `GameCanvas::guard_stage`,
`router::push_guard`). The browser's Back pops it: the stage is left as the
play's quit leaves it (`PlaySession::leave`: a forfeit, "Left the stage"
over the dimmed field, the stage watched on: online the others' streams, in
practice the CPUs judged here), the page staying on the play's address.
Forward comes back to the guard's entry and changes nothing; once the stage
was left (or the play is over, on its results), a Back to the play's entry
goes on back at once (one `history.back()`, to another address), so a
second Back reaches the room's song select in one press and the results'
own way back counts the guard. No `go` of ours waits on another
(`ddi_ui::back_stack`'s rule).

**Where the question is asked**: over the stage's results, not over the
watching view. Neither shell draws a screen's dialog over a running play
(the desktop's close request asks over the results the same way:
`Shell::ask_after_play`), and drawing the UI over the play view is more
than this item. Once the results of a stage left by Back are up, an
online room's question is asked by itself (`ResultsScreen::after_back`:
"You left this stage: no points for it." and what leaving the room does,
Stay chosen first): Stay keeps the results, Leave tells the room and goes
to the multiplayer home (`crate::room::results_shell`). The player can
watch on as long as they like before that, or skip to the results at once.
**Practice** forfeits and watches on with nothing asked (the maintainer:
practice is never asked about); leaving outright was considered and not
chosen: Back then does what the play's own quit does, one rule for both.

Tested: `ddi-ui`
`a_stages_results_after_the_browsers_back_ask_and_leave_only_on_leave`
(online over the fake room: asked by itself with the stage's sentence
first, Stay kept and not asked again, Leave telling the room and leaving
through the shell; practice asks nothing). Headless Chrome and Firefox
(`target/scratch/mp/left/back.mjs`, practice, autoplayed): the Back
forfeits and watches on at the same address, Forward changes nothing,
Enter skips to the results (nothing asked, the line Left and QUIT), a Back
from the results reaches the room in one press, and a Forward from there
shows the room (an address the room did not hand over).

### A player out of a stage follows a competitor large

After going out or leaving a stage's play, Tab and Shift+Tab, a
controller's shoulders, or a press on a miniature put that competitor in
the large center field exactly as a watcher's focus does (their notes,
judgment's word, combo, score, life and power bar, their attack warning
and links; the status line "Watching [name]"). The player's own field is
then a miniature among the others, named "You" and marked "Out" or "Left
the stage" (`opponents::Shown::own`), fed from their own judged play as
the others' are from their streams (`Battle::own_feed`, `own_view`: taps,
holds' ends, mines, shocks, Boos, their going out). Tab goes round the
competitors and back to their own field; Escape once (or a press on their
own miniature, or on the large field) goes back to their own dimmed field
with its mark; Escape held or twice still skips to the results, as do
Enter, Start or South and the skip's plate (at the band's foot while
following, `OwnOut::dimmed`). Practice and online alike (one code path).

Tested: `ddi-session` `watch`'s keys (`a_player_out_of_the_stage_skips_and_follows_one_large`),
`a_quit_in_a_stage_forfeits_and_watches_on_until_skipped` (Tab follows
seat 0, the frame the feed's large frame, their own miniature out, Escape
back), the live stage's test (`a_live_stage_is_started_streamed_attacked_and_decided_by_the_room`:
Bob, out, follows Alice's streamed field and comes back); `ddi-shell` the
plate's place while following; `ddi-desktop` `room_watch_offscreen`
(out, Tab: four slots with their own among them and the followed last,
the label, the plate; Escape once back, the play going on; picture
looked at).

## The audience: the design and its contract (2026-11-13)

The maintainer: "for 'watch only' I'd like to have no limit (at least
within reason of what the Cloudflare deploy can work with)." Designed in
[The audience](multiplayer-audience.md): people who only watch, beyond the
members, counted and not listed, by a link of their own (and offered at
the member bound), served by relay objects that take each audience frame
once from the room and send it to up to 500 seats each (relays
recommended over the room's own sockets and over cached segments, with
the platform's numbers and costs: about $0.03, $0.07 and $0.26 an hour at
1,000, 5,000 and 20,000 watching).

- **Real and tested, in `crates/room` alone** (`ddi_room::audience`,
  `audience-v0`, PROVISIONAL): the public view and what it leaves out, the
  coalesced audience frames (four a second, built and encoded once by the
  room), the room's side (`AudienceHub`), a relay's (`Relay`) and a
  device's (`AudienceLink`, `Audience`), all sans-IO; the count to the
  members (`ServerLive::Audience`) and the owner's closing
  (`ClientLive::Audience`), both live messages an older peer passes over;
  `RoomPolicy::audience` (additive). `FakeServer` runs it: 2,000 seats on
  a 73-player stage with attacks, every seat's fields equal to the room's,
  a moment at a seat 367 ms after its song time at the median and 661 ms
  at most (20–120 ms each way); a relay's restart mid-stage; the members'
  frames byte for byte and time for time the same with 500 seats as with
  none (`tests/audience.rs`).
- **Not built**: the server's relays, room side, audience link and join;
  the game's audience page, spectator view on audience frames and the
  count on the room's screens. Nothing is reachable yet; singleplayer and
  the members' protocol are unchanged.
- **Provisional**: a third link; signed-in seats, two an identity; 20,000
  seats in relays of 500, four homes; four frames a second with every
  field; two seconds of delay at most; the owner closes the audience
  (`multiplayer-audience.md`, "Provisional choices"; its open questions 1
  to 6).

### The live-stage tests the live rounds did not write

- **Three devices through their screens** (`ddi-session`
  `a_live_stage_through_the_screens_of_three_devices`, the UI harness:
  each device's stage prompt and results on its own `Ui`, its clock the
  room's, over `FakeServer` at 40 ms each way with 40 of jitter): Alice
  (host) and Bob play, Carol watches; Ready pressed on the prompts, Start
  on all three when the room starts; Bob's device joins two seconds late
  (part-way, in the stage); Alice's socket dropped at 20 s and
  reconnected; attacks taken both ways; Bob leaves once both were
  attacked (a forfeit), which decides the stage early; Carol is not over
  at the decision and her view runs to Alice's finish; every results
  screen shows the Standings confirmed, Alice first and Bob second (the
  watcher's marked watched).
- **A desktop offscreen live stage** (`ddi-desktop` `tests/live_offscreen.rs`):
  the window's play host plays a live stage (its prompt, the room's
  start, autoplayed) through the fake room at 40 ms each way against
  another device's scripted stream (hits, a hold held to its end, one let
  go, a mine stepped on, a shock stepped on); compared structurally: the
  held hold pinned to the receptor with the receptor holding, the dropped
  one scrolling on, the mine drawn then gone with its receptor flashing,
  the shock's lane locked, one miniature slot, the view's delay adapting
  (about 470 to 550 ms, never a second). The desktop gained a test hook,
  `ddi_desktop::room::set_live_for_tests` (the wall clock a live stage's
  play reads and its room), so that it runs on the fake clock.

### Remote fields looked at

Pictures from the offscreen live stage (`DDI_ROOM_SHOTS`), looked at. Two
things were wrong and are fixed: **a miniature drew no hold or roll at
all** (the skin's `NoteLayer::miniature` drew the notes alone, never the
holds' pass: a person's holds showed as nothing, their heads included),
now `holds` then `notes` as a field; and **a remote hold's body ran past
its field** (above it once let go, below it to the screen's edge), now
drawn within it (`FieldFeed::frame`: the tail no further than the far
end, a let-go head held at the near edge while its body is in sight).
Mines, the locked lane (darkened, its receptor tinted) and the dropped
hold's gray look right. The followed field's Boo word was not pictured
(tested on the feed). Not looked at in a browser: the web draws the same
`PlayView` and skin.

### A winning CPU plays the song out

Found in the second round: a CPU winner whose gauge emptied after the
stage was decided went out there, and the watching ended early (its
field marked Out, `still_playing` false). Now, once a practice stage is
decided, each CPU of the first place still in plays the song out with its
fail off and is never put out afterwards, whatever its gauge does
(`Battle::winners_play_on`, `Seat::out_at`; `ddi_engine::cpu::Opponent::set_fail`,
new, which only sets its player's fail policy: its plan's presses are
untouched, every existing seed's fixtures pass unchanged). The decision
stands whatever happens after, as for a person who wins (their fail
turned off as before). Tested: `ddi-session`
`a_watched_stage_ends_when_the_winner_finishes_and_a_watcher_leaves_freely`
(CPUs on a battery of one life: the winner's field never Out after the
decision, the results the hold after it finished the chart; it fails
without the change, the winner put out a frame after the decision).

## The audience, the client front (2026-11-13)

The game's side of [The audience](multiplayer-audience.md) ("What the game
builds"), on `ddi_room::audience` and tested over `FakeServer`'s audience
alone (relays and seats with latency); no server was started for it. The
server's side was built at the same time in `../ddi-server`; its shapes
(the join's answer `{ticket, relay, expires_at_ms}`, the socket path, the
previews' `kind: audience`, `audience`, `audience_count`, the member
bound's `409 {error: room_full, audience: true}`, `audience_invite`, the
refusals' codes, `503 relay_unready`) are what the game reads.

- **Arriving** (both shells): an audience link (`#/mp/audience/<link>?mp=1`,
  desktop's `--join`, the home's invite field) opens the invite page,
  which says "You're invited to watch with the audience." (no member count),
  with the count where the server gives it, and **Watch** (signing in as
  for any join; the sign-in returns to `/mp/join/<link>`, the only invite
  route the server's allow-list has, and goes on to the audience join,
  `Dest::Audience`). At the member bound a room's own links say "The room
  is full: you can watch with its audience." with **Watch with the
  audience**, from the preview or from a join refused with the offer. The
  join's refusals are worded (`audience_full`, `audience_closed`,
  `too_many_seats`, `audience_link_unknown`, `no_audience`,
  `room_closed`; `relay_unready` as the server unreachable, asked again
  shortly). The seat is `ddi_ui::state::audience::AudienceClient`:
  `ddi_room`'s `Audience` over the shell's socket (the web's `WebSocket`,
  desktop's tungstenite, each with the audience's path), the invite
  page's ticket for its first socket, a new ticket from the join after a
  lost socket (`RECONNECT_BACKOFF_V0`); a relay's restart is survived
  without one (its seats keep their sockets). The web's route for the
  watching is `#/mp/watch/<link>`.
- **The audience's page** (`screens::audience`): the room's name, its
  host, the count, what the room is doing (the host choosing, stage n's
  song and artist, the stage being played, the match over or stopped),
  the rules in words, the last stage's winner, the standings (twelve and
  "n more"; before any, the players), the seat's state and why it ended
  or was refused, Stop watching (and Sign in again after a 4401). Made
  from the public view alone.
- **The stage watched**: each stage's song is read from the room's share
  as the view names it (the shells' share readers, `LoadJob`), its chart
  found by the digest; with attacks this device's battle plan is computed
  (`seed_for`, the view's rules) and kept only where its digest is the
  view's `battle` (else the fields are drawn without shocks and the page
  says so quietly). Once the room starts the stage and a frame has said
  the room's clock, the client hands the shell a watched `StageOrder`
  (`me` nobody, `AUDIENCE_ME`; the lineup with the view's names); the
  session plays it as any watched live stage, fed through a new trait,
  `ddi_ui::state::room::LiveRoom` (what `battle::live` reads: the room's
  clock, the stage's song zero, the frames, the decision, the stream:
  a member's `RoomClient` or the seat; nothing changed for members). The
  music and the fields are drawn `MUSIC_DELAY_V0` (`audience-view-v0`,
  PROVISIONAL: 0.8 s) behind the room's song time as the seat estimates
  it from the frames' times, plus a field delay of its own from nothing,
  as the frames measure, within the audience's two-second ceiling.
  **Desktop** plays it through the play host over the audience's page
  (the prompt counting down, the grid, the focus and pages as any watched
  stage), then back to the page, which shows the result (no results
  screen: there is no member table to show). **The web does not watch
  the stage yet**: the play route's stage path is the room's (its prompt,
  its report, its results); its page says the stage is being played and
  shows the result. That is the next piece of work.
- **The count for members**: "· N watching" after the room's status line
  (song select and the stage's prompt, never over a play), from
  `ServerLive::Audience`, only where the room's policy has an audience;
  Host options' row "Audience · N watching" with **Close the audience** /
  **Open the audience** (the owner's, `Capability::ChangeAccess`;
  "The audience is closed: nobody watches with it now." on the status
  line); More's **Copy audience link** where the server gave
  `audience_invite` ("Audience link copied: anyone with it can watch.").
- **Endings**: the seat closed (4403 "The host closed the audience.",
  4410 "This room has ended.") asks for nothing more and its page says
  why with Stop watching. Mid-stage on desktop the watched view plays on
  without frames (its fields stale) until the song's end or Escape, then
  the page says why; a line over the play itself is not built.
- **Bots** (DEVELOPMENT TOOL): `ddi --mp-bots --audience N --join <link>
  --api <api>`: N seats (two a signed-in bot, at most 5,000 a run) that
  only watch, a line every ten seconds (seats watching, frames received,
  the count, the phase) and each seat's totals at the end.
- **Debug**: `__DDI_DEBUG.ui.theme.audience` (`seat`, `why`, `count`,
  `frames`, `sockets`, `delay_ms`, …), `ui.theme.room.audience`.

### Tested

- `ddi-ui` (`state/audience/tests.rs`, over the fake room's audience): the
  audience link's page, Watch, the seat, the view and the count, the
  count told to the members, two seats an identity and the third
  refused; every refusal worded, a room without an audience offering
  none; the owner's closing (a member's refused), the seat ended and
  asking nothing more, the room's end; four relays restarted with no new
  socket; a socket cut and a new ticket; the pages through the UI
  harness (the invite page, the audience's page, closed). The `qps-ploc`
  walk (the previews, the offer at the bound, the refusals, the
  audience's page in its states, Host options' row and its two states).
- `ddi-session` `the_audience_watches_a_live_stage_behind_the_room_with_the_members_fields`:
  two people playing and a member watching in a live room with attacks,
  a seat 80 ms away (40 of jitter): the song prepared from the share,
  the stage handed over, the music 0.94 s behind the room's song time
  throughout (the 0.8 s delay and the clock's 0.14 s error from the
  frames' times, never ahead), the fields 50 ms more at most, every field
  equal at the end to the member watcher's (combo, out, the shocks), the
  relays restarted at 30 s with no new socket, the members told the
  count.
- `ddi-desktop` `tests/audience_offscreen.rs`: two members' scripted
  streams, a seat 60 ms away, the stage watched through the window's
  play host (the grid of two, both moving, 0.95 s behind, within the
  ceiling); the picture looked at (`audience-grid`).
- Headless Chrome and Firefox (`target/scratch/mp/aud/pages.mjs`, no
  server): the audience link's page and the audience's page with its
  refusal, English and Japanese; the singleplayer smoke check.

### Not done in this round

- **The web's watched stage** (above). **No real server, relays or
  latency** were tried: the server's side landed at the same time.
- **A 73-player stage watched through the session**: the session test has
  two players; the 73-competitor audience is the room crate's own test
  (2,000 seats, fields equal to the room's). Paging and the focus are the
  watched stage's own (unchanged, tested there), not tested again for a
  seat.
- A seat's end said over the watched play itself; the count while one
  plays (decided: between stages only).
- `MUSIC_DELAY_V0` is fixed; raising it after a stage whose fields lagged
  is not built.

### Trying it (once the server's side runs)

1. Run the server locally (`just dev` in `../ddi-server`) and the game's
   `just dev`; sign in and make a private room on the web (`#/?mp=1`),
   open More → **Copy audience link**.
2. Open the link in another browser profile (signed in as someone else):
   the invite page says "You're invited to watch with the audience.";
   press **Watch**: the audience's page names the room and "One person
   watching"; the host's song select ends its status line with
   "· 1 watching".
3. Desktop: `ddi --join "<audience link>"` (with `DDI_API_URL` at the
   local API) opens the same page; pick a song in the room and Ready:
   desktop watches the stage behind the room and comes back to the page
   with the result.
4. Many seats: `DDI_SHARE_TOKEN=test-token ddi --mp-bots --audience 50
   --join "<audience link>" --api http://127.0.0.1:8787/ditoditoinfinito/api`
   (a line every ten seconds); Host options → **Close the audience**: the
   seats end (4403) and the bots say so.

## The audience, what the client front left (2026-11-13)

All real and tested over `FakeServer`'s audience; no server was started.

### The web watches an audience's stage

- **The route**: `#/mp/watch/<link>/stage`, its own (a member's
  `#/play?…&stage=<id>` is untouched). The audience's page hands the
  watched `StageOrder` over (`crate::online::watch_stage`: the order, the
  link and the seat kept beside the page) and navigates there; the play
  route mounts the play with `game_canvas::Props::watch` (the link) and
  `room_stage` the order. The play is the session's watched live stage
  fed by the seat (`LiveHandle { room: seat }`, as desktop has it): the
  grid, its pages, the ladder, Tab and the shoulders, a tap on a field,
  the music and the fields behind the room's song time. No room prompt,
  no report (`crate::room::report` passes a watched stage over), no
  history, no results: at its end (or Escape, or a skip after the seat
  ended) the play goes back to the audience's page (`return_to`, one
  history step), which shows the stage's result; the context is kept for
  the next stage (`crate::online::keep_stage_audio`).
- **The sound's gesture**: the Watch press on the invite page is the
  gesture: it makes and starts the context there
  (`OnlineCall::WatchPressed`, new, called inside the press;
  `crate::online::audience_gesture`). A stage handed over gets that
  context, or one made then where the browser lets a context start
  without a press (`web::autoplay::may_start`: Firefox's
  `getAutoplayPolicy`, else `userActivation.hasBeenActive`, Chrome's
  rule once the page had a press), and its prompt starts it at once
  with no card. **Chrome and Firefox**: after the Watch press (or any
  press on the page) every stage starts by itself. **Safari**: only the
  context made in the Watch press runs; it is kept from stage to stage,
  so the stages start by themselves too, unless the page was reloaded.
  **After a reload** (no press on the page yet, any browser that blocks
  autoplay): the prompt's card shows with Start (the existing "needs a
  tap" path): one press, and the stage plays from where it stands (live
  start part-way). The picture waits for that press: a "Tap to hear"
  over a running picture is not built (the play's clock is the audio
  clock; a picture on another clock until a press would be a second
  timing path).
- **Back and reload**: the browser's Back leaves the stage for the
  audience's page with nothing asked (no guard entry; the seat goes on);
  Forward comes back to the stage as it stands (the order is kept until
  the stage's play ends). A reload of `/stage` shows the audience's page
  in its place (the address replaced), which makes the seat again (a new
  ticket), takes its snapshot and hands the stage over again: the play
  starts part-way. The prompt says "Stage 1 of 1 · watching 3 players"
  (`PromptInfo::set_watched_stage`, new, both shells, for a member's
  Watch too, which said "against N other players").
- **Found and fixed**: a watched play started on its room client's clock
  as last ticked; nothing ticks an audience's seat between its page and
  the play, so the music started 0.4 s late on the web (seen as "the
  song clock and the room's disagree by 433 ms"). The session now ticks
  the room's client just before reading where song zero is
  (`PlaySession::start`); tested (the seat left unticked 0.4 s before the
  play: the music 0.8 s behind, as without; 1.22 s without the fix).

### A seat that ends during a watched stage

`LiveRoom::seat_end` (new, a member's never ends): the audience's seat
ended by its relay (4403, 4410) or a new ticket refused (the link
replaced: `audience_link_unknown`; the audience closed or the room ended
meanwhile; signed out) is `SeatEnd`. The session marks the watching
ended (`WatchView::ended`): the music fades, nothing ends it by itself
before the song's end, and why is said over the whole play
(`OwnOut` over the screen at a darker veil, `ENDED_VEIL`, with
"Enter: back", or "Back" to tap); Enter, Start, South or the tap go back
to the audience's page (a spectator could not skip before). Both shells
(desktop's play host goes back to the page on any end of a watched
stage). Tested: `ddi-session` `a_seat_closed_during_a_watched_stage_stops_its_view_and_says_why`
(the host closes the audience at 15 s: ended 30 ms later, no frame
after it, not over, Enter finishes it), `ddi-ui`
`a_seat_s_end_during_a_watched_stage_is_its_relay_s_or_a_refused_ticket`,
`watch.rs`'s keys.

### The music's delay follows the fields'

`audience-view-v1` (`AUDIENCE_VIEW_V1`, PROVISIONAL): the music's delay
is moved **at each stage's start only**, by what the last stage watched
needed: where the fields had to fall further behind the music than
100 ms for a tenth of its frames or more (their own delay's 90th
percentile, noted a frame through `LiveRoom::note_view_delay`), the next
stage's music is put that much further behind (rounded up to 50 ms, at
most 1.6 s, which leaves the fields 0.4 s of the 2 s ceiling); after a
stage that needed less it falls 0.2 s a stage back to 0.8 s. Why at a
stage's start: the music is one take, never moved, sped up or slowed
down, so nothing is ever heard; within a stage the fields keep adapting
on top as before. A resync at a silence was not chosen (songs have few,
and a jump is heard however short), nor a slow drift (a rate change of
the music is a pitch change, and the play's clock is the music's).
`__DDI_DEBUG.ui.theme.audience.next_delay_ms`, `view_rules`. Tested
(`ddi-ui`: the rule, a spike that moves nothing, the cap, the fall; the
seat noting the fields' delay).

### A 73-player stage through the session as audience

`ddi-session` `the_audience_watches_seventy_three_players_through_the_session`:
73 members' scripted streams (every sixth out), a seat 60 ms away (60 of
jitter): every field at the end as its member sent it (combo, out), four
audience frames a second, the music 0.8 s behind and the fields at most
50 ms more (within the 2 s ceiling), the grid's seven pages of twelve
(Page Down round), Tab following the first, 73 fields with the 12 out
counted out.

### The fake's links answered as the server's

`ddi_room::fake` `invites.rs` (new): `FakeServer::invite_preview` (the
room's own link `play`, the watch-only link `watch`, the audience link
`audience` with `audience_count`, `audience: true` on it and at the
member bound while the audience is open, `404 invite_unknown`, `410
room_closed`) and `audience_join_answer` (`{ticket, relay,
expires_at_ms}` or the refusal's status and `{error}`; any of the three
links joins the audience), from the server's `RoomCore::preview` and
join. The fake's links are `FakeLinks` (settable: a link replaced). The
audience's tests (`ddi-ui`, `ddi-session`, `ddi-desktop`
`audience_offscreen`) read these rather than answers of their own; the
online tests' member previews still answer their own (member joins are
not the audience's).

### Looked at in headless Chrome and Firefox

`crate::fake_seat` (TESTING ONLY, `#/?mp=1&mpfakeseat=1`, `mpfakeend=S`):
the next audience seat is one in a fake room in the page (three members
streaming Beatucada's single chart as scripted hits, each missing every
few notes, pumped twenty times a second on the wall clock); the seat,
its page, the handover, the route and the view are the real ones. Walked
(`target/scratch/mp/audweb/watch.mjs`, 1280×720): the page, the stage
handed over and started by itself, the grid at 8 s (music 0.8 s behind,
the fields 50 ms more), Tab following alice, the browser's Back to the
audience's page (the seat still watching); with `mpfakeend=14` the
notice over the play at 14.8 s ("The host closed the audience.",
"Enter: back"), Enter back to the page, which says why; in Firefox with
autoplay blocked the prompt's card with Start, Enter, and the stage
played from where it stood. Pictures looked at. Not walked: a reload
mid-stage (the fake room lives in the page and would start over) and a
real server's latency.

### Not done

- "Tap to hear" over a running picture (above): after a reload the
  picture waits for one press.
- The audience's page after Back says the stage is being played but has
  no "Watch" to go back to it (Forward does).
- A reload mid-stage was not tried against a server.

### Trying it in Firefox against the local server

1. `just dev` in `../ddi-server` and the game's `just dev`; in Firefox
   (profile A) open `http://localhost:8080/#/?mp=1`, sign in, make a
   private room, More → **Copy audience link**.
2. In a second Firefox profile (or a private window) signed in as
   someone else, open the audience link: the invite page says "You're
   invited to watch with the audience."; press **Watch** (the press
   readies the sound).
3. In profile A pick a song and Ready (a second member, or bots:
   `DDI_SHARE_TOKEN=test-token cargo run -- --mp-bots --join "<invite>"
   --api http://127.0.0.1:8787/ditoditoinfinito/api --site
   http://localhost:8080/`). When the room starts the stage, profile B
   goes from the audience's page to `#/mp/watch/<link>/stage` by itself
   and plays it about a second behind, with sound; Tab, Page Down/Up
   and a tap on a field as a member who watches; the browser's Back
   returns to the audience's page; at the stage's end it returns there
   with the result.
4. Mid-stage, Host options → **Close the audience** in profile A:
   profile B's view stops with "The host closed the audience." and
   "Enter: back".
5. Reload profile B mid-stage: the audience's page, then the stage again
   from where it stands after one press on the prompt's Start.

## Step down: an easier chart instead of going out (2026-11-13)

(Superseded the same day by "Adaptive charts", below: the rule is now
Charts: As picked / Adaptive, steps go up as well as down and a stage
ranks by the score. What follows is kept as built then; where it differs,
the next section says so.)

The maintainer (2026-11-12) remembered a planned "dynamic difficulty mode
where instead of knocking out players they'd be 'downgraded' to an easier
chart of the same song". The adaptive ladder in `docs/plans/multiplayer.md`
stays research; this round builds the simple version proposed to them: a
match rule, **At zero life: Out** (as before, the default) **or Step
down**. It reacts only to life reaching zero, so nothing guesses anyone's
skill, and a step down costs standing.

### The rule as built (`step-down-v0`, PROVISIONAL)

- **The ladder** (`ddi_engine::ladder::ladder`): the song's charts that are
  originals (no `derivation`), in the stage chart's style, with notes, on
  the same timing as the stage chart (the boundary and the breather are
  measured on one timing; a chart with split timing of its own is no
  rung), and easier. Below a slotted chart (Beginner to Challenge): the
  charts of lower slots, the nearest slot first, a slot's charts by meter
  from the highest, then by their place in the song; Edits are never
  rungs of a slotted chart. Below an Edit: the slotted charts of a lower
  meter, by slot from the highest, then meter. The ladder is cut at the
  first rung the CPUs cannot plan (`ddi_engine::cpu::planned_ladder`), so
  everyone has the same. A song with no easier chart has no ladder: the
  rule changes nothing there, and the Rules pop-up says so under its row
  when the song the picker rests on has none (as the song list has it).
- **At zero life with a rung below** (`Player::add_rung`, `STEP_DOWN_V0`):
  life is given back to **half** (DDI's 50 of 100); the old chart's notes
  not judged by then are taken away unjudged at once (an active hold too:
  neither kept nor let go; `NoteState::Removed`, never drawn); the
  **boundary** is the first bar line (every four beats; time signatures
  are not read) at least the player's **reading horizon plus one second**
  past the zero; until it, a **breather**: nothing of a chart is judged,
  a stray press counts for nothing, and life cannot drop below **one
  point** (a battle's shocks are still judged in it). From the boundary
  the next rung's notes are theirs. On Beatucada's Hard at the default
  speed the breather is about five to eight seconds.
- **On the lowest rung** zero life is out, as before.
- **Standing** (`ddi_room::battle::rank`, additive `Run::Live::steps`):
  those still in before those out; among those in, **fewer steps first**,
  then life, then the score; those out by when they went out. The early
  decision (one left in) and the attacks' settlement read "in" as before:
  a stepped-down competitor is in.
- **The score**: each stretch counts against what it was shown (the old
  chart's notes judged before the zero, the new chart's from the
  boundary: `ScoreRules::restep`); for comparing and the standings it is
  put on the stage chart's scale as a CPU's is (`CPU_SCORE_V0`'s way).
  The standings mark the chart a competitor finished on ("↓ Medium", the
  chart's name as the song list names it; `RunDetail::chart`, additive).
- **Attacks**: the battle plan's sections are placed over the stage's
  chart and its ladder at once (`ddi_engine::ladder::merged`: every
  lane's notes of every rung together), so one plan holds for everyone
  and a commitment needs nothing new. What it costs is measured below.
- **CPUs** step down by the same rule: each rung planned once for the
  stage, each CPU scripted on it at its own level relative to that rung
  (`Opponent::add_rungs`), its response to shocks made again on the new
  script; a CPU with a ladder judges each edge's time before feeding it,
  so a step is found where its life ran out whatever the frames.
- **The cue**: "Stepping down: Medium" in the status line
  (`StageCue::StepDown`; no announcer line exists). The field's old notes
  vanish, the new chart's scroll in from the boundary; reduced motion
  changes nothing here (nothing moves but the notes).
- **The hand** plays out the notes its planner was given before the step
  and then rests to the end of the stage (`RuntimePreview::step_down`,
  2026-11-13): given the new chart from the boundary (the first way) it
  failed now and then in the desktop's offscreen stage; see "Not done".
- **Not in solo play.** Stage plays are not kept in the history; nothing
  stored changes. The rule is locked for a match like the others.

### How a play changes chart, and why that way

One player, one judge: at the step the `Judge` is rebuilt
(`Judge::step_down`) with every note it had (those taken away marked
`Removed`, not judgeable) and the new chart's notes from the boundary
appended after them, the buttons, the judging clock and the locks kept.
Notes keep their indices (nothing that reads a play by note index sees
them move), and every reader of a play (frames, events, results, a
battle's feed, the hand, a replay) reads one `Player` as before. A
second player started at the boundary would have needed every reader to
join two. Exactness: the step is made while the event that emptied life
is applied; events of the old chart judged past it in the same update
are dropped and their notes taken away too, and a lane lock one of them
set is put back (`Judge::locks`), so what is judged does not depend on
how time is cut into frames. Tested at 60 and 144 frames a second and
from the log alone, the player's and the CPUs'
(`crates/engine/tests/step_down.rs`, the opponent's tests, the session's
`play/tests/stage/step_down.rs`). One residual: a battle shock judged in
the same update right after the step, in a lane a chart mine of the old
chart locked in that same update, sees the lock (shocks are placed clear
of the lane's notes, so this needs a mine and a shock within a frame of
each other).

A play without a ladder is the play it was: no rung, no step; the
engine's and the session's fixtures are unchanged, and a stage under
Step down on a song with no easier chart gives the Out rule's results
exactly (tested).

### Practice

The room's Rules pop-up has the row **At zero life: Out / Step down**
with its sentence under it (brightening while focused), marked during a
running match as the other rows; Reset gives Out; the rules' summary
names it only when it is Step down ("DDI · Step down"). The match rule
is `MatchRules::at_zero` (additive, left out of the JSON at Out, read as
Out where missing); `RoomPolicy::step_down` says the authority honors it
(true for every room this build makes, false where missing, so an older
server is never offered it). The player's and the CPUs' plays step down
the ladder; their miniatures and the spectator's grid draw the chart each
is on (`FieldFeed::step_down`, a `MomentKind::StepDown` moment with the
life given back); the ranking and the standings follow the rule above.

### Online (the live contract, additive)

- The battle plan names the ladder: `BattlePlan::ladder`, each rung's
  chart digest, in the plan's digest only where it is not empty (a plan
  without it keeps its digest). Every device computes its own plan, so a
  device whose charts differ disagrees on the digest and sits the stage
  out, as for the stage's chart.
- A stream says each step: `live::MomentKind::StepDown { rung }`, its
  time the zero, its tick the boundary, its life what was given back. On
  the wire it is a refusal's kind with its flag (unused before), the rung
  counted down from `u32::MAX` in the attack's place (`wire::STEP_DOWN_ATTACK`):
  a reader that predates it reads the refusal of an attack no stage has
  and takes nothing off. The others' fields follow it.
- The run reports its steps (`ReportedRun::Live::steps`); the room's live
  stage counts the steps each stream said (`LiveStage::steps`) and its
  own ranking reads them.
- Offered online only where the room plays live and its authority says
  `step_down` (`RoomClient::step_down_offered`); elsewhere the row reads
  "Out · not in online rooms yet".
- What a deploy changes: the server built against this crate makes rooms
  whose policy says `step_down`, ranks by reported and streamed steps,
  and takes plans with a ladder. Before a deploy the deployed server's
  policy says nothing, so no client offers the rule there. An older
  client in a newer room: it cannot choose the rule; if another member
  sets it, the older client plays its stages without a ladder (its play
  goes out at zero), reads others' step moments as nothing, and its plan
  digest differs where the stage has a ladder, so it sits such stages
  out. The server repository needed only the new field in three places
  (`ReportedRun::Live { steps }`).
- Tested over `FakeServer` with two people (`play/tests/stage/live.rs`,
  `a_live_stage_steps_down_and_the_others_fields_follow`): the same plan
  on both devices with the ladder named, Bob stepping down, Alice's view
  of his field following each step at its boundary, the room counting his
  steps and ranking Alice first.

### Measured

`cargo test --release -p ddi-session --lib step_down_study -- --ignored
--nocapture` (`crates/session/src/battle/step_study.rs`); the local
corpus is the thirteen arcade mixes of the maintainer's library read in
place (`DDI_STEP_CORPUS`, one simfile a song folder), aggregates only.
Stages: ten CPUs, one at each level, attacks on, on every Hard and
Challenge chart with a ladder, two seeds, each under Out and Step down.
MEASUREMENTS OF CPUS AGAINST CPUS.

Ladders (996 songs, 970 with a ladder; single charts by the stage
chart's slot):

| Stage chart | Charts | Ladder 0 / 1 / 2 / 3 / 4+ | Mean | Plan's sections kept when merged |
| --- | --- | --- | --- | --- |
| Beginner | 913 | 913 / 0 / 0 / 0 / 0 | 0.00 | — |
| Easy | 970 | 58 / 912 / 0 / 0 / 0 | 0.94 | 100.0% |
| Medium | 970 | 23 / 58 / 889 / 0 / 0 | 1.89 | 99.9% |
| Hard | 970 | 1 / 23 / 58 / 888 / 0 | 2.89 | 98.2% |
| Challenge | 324 | 28 / 0 / 12 / 3 / 281 | 3.57 | 82.5% |

The bundled songs: 4 of 4 with a ladder; merging keeps every section
(a few more slots where an easier chart ends later). Placing the plan
over every rung at once costs little below Challenge and about a sixth of
the sections on Challenge, so sections per rung were not built.

By level (2,530 stages a rule), DDI Normal:

| Level | Out: out % | place | Step down: out % | stepped % | mean steps | place |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 96.9 | 9.51 | 4.1 | 96.0 | 1.90 | 9.73 |
| 2 | 81.0 | 8.97 | 2.2 | 76.2 | 1.23 | 8.80 |
| 3 | 62.4 | 7.75 | 1.0 | 54.9 | 0.74 | 7.87 |
| 4 | 43.4 | 7.03 | 0.5 | 35.5 | 0.42 | 6.97 |
| 5 | 20.6 | 6.14 | 0.0 | 11.5 | 0.12 | 6.13 |
| 6 | 4.3 | 4.95 | 0.0 | 1.7 | 0.02 | 4.96 |
| 7 | 3.3 | 4.25 | 0.0 | 1.5 | 0.02 | 4.23 |
| 8 | 0.1 | 3.06 | 0.0 | 0.1 | 0.00 | 3.05 |
| 9 | 0.0 | 2.24 | 0.0 | 0.0 | 0.00 | 2.18 |
| 10 | 0.0 | 1.09 | 0.0 | 0.0 | 0.00 | 1.07 |

DDI Hard:

| Level | Out: out % | place | Step down: out % | stepped % | mean steps | place |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 100.0 | 9.68 | 79.5 | 100.0 | 3.09 | 9.82 |
| 2 | 99.8 | 8.60 | 38.3 | 99.8 | 2.81 | 8.54 |
| 3 | 99.8 | 8.40 | 30.3 | 99.8 | 2.72 | 8.18 |
| 4 | 97.0 | 6.81 | 8.4 | 95.4 | 2.12 | 6.97 |
| 5 | 86.4 | 5.68 | 2.6 | 77.4 | 1.36 | 6.16 |
| 6 | 57.2 | 4.75 | 0.5 | 39.3 | 0.45 | 4.68 |
| 7 | 55.3 | 4.49 | 0.5 | 35.9 | 0.41 | 4.28 |
| 8 | 13.5 | 3.01 | 0.0 | 6.9 | 0.07 | 3.05 |
| 9 | 10.9 | 2.34 | 0.0 | 5.1 | 0.05 | 2.25 |
| 10 | 0.9 | 1.15 | 0.0 | 0.4 | 0.00 | 1.07 |

What it says: under Normal, Step down keeps nearly everyone in to the
song's end (the weakest out 4% instead of 97%), mostly one or two rungs
down; under Hard the weakest still go out on the lowest rung four times
in five. The stage's order barely moves (mean places within half a place
of Out's): those who step down rank below everyone who stayed, as the
rule intends, so it changes who plays to the end, not who wins.

What 72 CPUs cost (`step_down_cost_of_72_cpus`, the release profile, one
stage of 72 CPUs on Beatucada's Hard with its three rungs): the ladder
planned once in under a millisecond; the stage loaded in 103 ms against
57 ms under Out (each CPU scripted on every rung when the stage loads);
the whole stage then played out in 53 ms against 39 ms, 14 of the 72
stepping down. Planning every rung at the load is cheap enough that
nothing is planned lazily.

### Tested

- Engine: `crates/engine/tests/step_down.rs` (the ladder's order; two
  steps the same at 60 and 144 fps and from the log; nothing judged in a
  breather; half the life back; the floor; without a ladder exactly as
  before, its results' JSON without `steps`), `ladder.rs`'s unit tests
  (the boundary, the merged chart), the opponent's (a CPU's steps the
  same at any frame rate, without a ladder as before).
- Room: the rule's JSON and the policy's, the ranking by steps and the
  wire of a reported run, the plan's ladder in its digest, the step
  moment on the wire and as an older reader reads it.
- Session: `play/tests/stage/step_down.rs` (the player and the CPUs step
  down alike at 60 and 144 fps, the cue once a step, the fields'
  steps, the details' chart; a field draws the chart its player is on;
  fewer steps first; a song without a ladder plays as Out exactly) and
  the live test above.
- UI: the Rules row (Out first, its sentences, Apply, the summary, Reset
  to Out, fixed where the authority cannot honor it), the standings'
  mark; the `qps-ploc` walk; `xtask l10n check` (the Japanese faces cut
  again for 脱).
- Desktop offscreen: `crates/desktop/tests/step_down_offscreen.rs`
  (practice on Beatucada's Hard, the player pressing nothing and three
  one-star CPUs: the player steps down, the cue shows, only the new
  chart's notes in the breather, a CPU's field steps, the standings mark
  the player and at least one CPU). Pictures in
  `target/scratch/mp/step/shots/` (`step-breather-cue`, `step-new-chart`,
  `step-standings`).
- Headless Chrome and Firefox (`target/scratch/mp/step/browsers.sh`,
  `step.mjs`), 1280×720: the Rules row set to Step down, a stage picked
  and played with nothing pressed: the ladder Medium, Easy, Beginner, the
  player stepping down at 8.9, 15.9 and 23.3 s (boundaries 13.8, 19.7,
  27.5 s) the same in both browsers, the cue drawn, the old notes gone
  and the easier chart's coming in, out on Beginner at 0:36, the
  standings marking "↓ Beginner"; no console errors. Screenshots in
  `target/scratch/mp/step/shots/` (`rules-*`, `breather-*`, `new-chart-*`,
  `out-*`, `standings-*`).

### Not done in this round

- An older client in a room that steps down plays without the ladder
  and sits laddered stages out (above); nothing tells it why.
- (Fixed 2026-11-13) The assist clap follows the new chart from a step:
  its rows past the step are the judge's after it
  (`EventSounds::step_down`); at most the 0.3 s it schedules ahead of
  the old chart's are still heard (the backend cannot take a scheduled
  sound back). The mine sound always followed what is judged. Tested on
  the fake clock (`the_claps_follow_the_new_chart_after_a_step`).
- The audience's seats draw a stepped field only where their device
  computed the same ladder (it does when it loads the stage like a
  member); not tested through a relay.
- Time signatures are not read for the boundary's bar lines.
- The dancing hand does not play the new chart after a step: it plays
  out what its planner was given and rests (2026-11-13). What was found:
  given the new chart's notes from the boundary, its motion preparation
  failed with `floor_penetration` on the way to the first new contact
  (at 12.47 s, the step at 8.94 s, the boundary at 13.77 s on
  Beatucada's Hard) in three of four runs of the desktop's offscreen
  stage, which draws eight frames a second on the real clock; never at
  60 or 120 frames a second, nor at 8 in the shell's own test on a fake
  or real clock (`the_hand_plays_on_through_a_step_down`), so it depends
  on how the preparation's work falls between frames rather than on the
  notes alone. Playing the new chart as a fresh preparation from a rest
  (as a quick restart warms a fresh buffer) is left for a later round;
  the offscreen test now asserts the hand never stops.

## Adaptive charts: down and up the song's ladder, ranked by the score (2026-11-13)

The maintainer, on step down ranking below everyone who stayed: "The
point of having the adaptive difficulty is to allow players of different
skill levels compete meaningfully and not punish lower skills too much
[…] harder charts naturally have a higher score potential, because of
the absolute component" — and "We should support stepping back up. And
not only 'back' up, but higher than the original chart." The match rule
"At zero life: Out / Step down" became **Charts: As picked / Adaptive**,
built as proposed (provisional, every value named).

### The rule as built (`adaptive-v0`, PROVISIONAL)

- **The match rule** (`ddi_room::Charts`, `MatchRules::charts`,
  `PlayRules::charts`: per stage as the others, changeable from a later
  stage): As picked (the default) or Adaptive. On the wire and in
  storage the field is `charts` (`"adaptive"`), and the rule it replaces
  is read as it: `at_zero` with `"step_down"` reads as Adaptive, `"out"`
  as As picked (serde aliases, tested). The authority says it honors it
  with `RoomPolicy::adaptive` (additive; `step_down`, still set, is read
  by nobody); a client offers the row only where it says so. The
  refusal is `RulesProblem::AdaptiveNotHonored` (read from
  `step_down_not_honored`).
- **The ladder** (`ddi_engine::ladder::Ladder`, `below`, `above`): every
  original chart of the song in the stage chart's style on the same
  timing, easier and harder. Below: as before (lower slots, nearest
  first, a slot's charts by meter from the highest). Above, turned over:
  higher slots, nearest first, a slot's charts by meter from the lowest,
  then by place. Edits as before: never a rung of a slotted chart; an
  Edit's rungs are the slotted charts of a lower meter below and of a
  higher meter above. Generated versions are never rungs. Each way cut at
  the first rung the CPUs cannot plan (`cpu::planned_ladder`). Rungs are
  numbered from 1, the easier ones then the harder ones
  (`Ladder::rungs`), 0 the stage's chart: the number a step, a stream's
  moment and the battle plan's `ladder` (every rung's digest in that
  order) share.
- **Step down** as built (`step-down-v0`'s values in
  `ADAPTIVE_V0`): at zero life with a rung below, half the life back,
  the old chart's remaining notes taken away, the easier chart from the
  first bar line at least the reading horizon plus a second ahead, a
  breather until it; on the lowest rung zero life is out.
- **Step up** (`Player::add_rung_up`, `Judge::step_up`): at a bar line
  (every four beats; time signatures are not read), a competitor whose
  life has been **full** for **four bars (16 beats)**, with **no miss, no
  hold let go and no shock or mine stepped on** in them, counted from the
  last step's boundary (`AdaptiveRule::up_bars`), moves to the next harder
  rung. The boundary is the first bar line at least the reading horizon
  plus a second past that bar line. Nothing is taken away before it: the
  old chart is played up to the boundary, its notes from the boundary on
  taken away (none can have been judged: they are past the reading
  horizon). **A hold of the old chart crossing the boundary is played to
  its end as the old chart's; the new chart's notes in its lane up to
  that end are left out** (the simplest sound thing: no note is judged
  twice and no lane carries two). Life is unchanged. On the top rung
  nothing happens.
- **Hysteresis**: stepping back up to a rung whose life ran out takes
  **eight bars** (`AdaptiveRule::return_bars`), every time in the stage.
- **A step down before a step up's boundary** (life emptied in the few
  seconds between them) is made from the rung the player still plays:
  the step up's notes are taken away with the rest, as anything not
  judged is at a step down. The step up stays in the play's steps (its
  moment was told), the step down after it says where they are.
- **Deterministic**: the decision reads only the competitor's own life
  and judgments, at bar lines: a play with a ladder is judged to each bar
  line before it is decided (`Player::judge_to_bars`, in `update_at` and
  before every edge in `input_at`), so the same inputs decide the same
  steps at any frame rate and from the log. A play without a ladder never
  runs this (As picked and a song with no other chart are today's play
  exactly: the engine's and the session's fixtures are unchanged).
- **The three research problems** (`docs/plans/multiplayer.md`, the
  mixed-skill study), answered simply for v0: transitions are at bar
  lines past the reading horizon; disruption by attacks is **not** told
  apart from difficulty (a shock stepped on in the four bars blocks a
  step up, and zero life by shocks steps down: accepted for v0);
  deliberate underperformance is made pointless by the ranking, not
  prevented (measured below).
- **Standing (Adaptive only; As picked keeps life, then the score)**:
  those still in before those out; among those still in **by the score
  as earned** (DDI's sum: each stretch's notes against what it showed for
  the proportional part, plus the absolute score, which grows with the
  notes played, so a denser chart earns more), **life only breaking a
  tie**, the steps never counted (`ddi_room::RankBy::Score`,
  `Charts::rank_by`, `battle::rank`'s new argument; the room's reducer,
  the live server and practice pass the stage's own). Those out by when
  they went out. A person's score is their play's own sum, never put on
  another scale (the step-down round's rescaling to the stage chart is
  gone, live and in practice). A CPU's is put on the charts it played,
  stretch by stretch (`ddi_session::battle::Scales`: each chart's whole
  most in the share of its tap-only copy's notes it was shown; a CPU that
  never stepped is on the stage chart's most as before). The per-rung
  score factor (`AdaptiveRule::score_factor`, `rung_score_factor`) is 1
  and applied nowhere: the named place for balancing once the rule is
  felt; there is no UI for it.
- **The Standings** mark the chart each competitor finished on when it is
  not the picked one: "↓ Easy" or "↑ Challenge" (`RunDetail::harder`,
  additive; `room-table-stepped-up`). **The cue**: "Stepping up: Hard" as
  "Stepping down: Medium" (`StageCue::Step { rung, up }`,
  `room-play-cue-step-up`). Miniatures, the player's own field shown
  large and watched fields draw the chart each competitor is on both ways
  (`FieldFeed::step`: a step down's chart from its moment, a step up's
  from its boundary, the old one to it; a moment of a note at or past a
  step up's boundary tick is the new chart's, one before it the old
  chart's judged late).
- **CPUs** step by the same rule at their level relative to the rung they
  are on (`Opponent::add_rungs` takes rungs either way; its script after a
  step up keeps the old chart's presses before the boundary and its
  response to shocks is made again on the new rung's presses alone). A
  CPU's strength is relative to the chart it plays, so **CPUs of every
  level climb when they play four bars cleanly**, the weakest falling
  back down more often (measured below).
- **The hand** plays out what it was given and rests after a step up as
  after a step down (it may rest a few seconds before the old chart
  ends: the hand is never the play). The fresh preparation from a rest
  was not tried this round.
- **The clap** follows the chart being played after a step up as after a
  step down (`EventSounds::step`).
- **Not in solo play; no manual stepping; no score balancing.**

### Online

- **The plan** names every rung (`BattlePlan::ladder`: the easier rungs,
  then the harder ones, each its chart's digest) and its sections are
  placed over all of them; a device whose charts differ disagrees on the
  digest and sits the stage out, as before.
- **The stream**: a step is `live::MomentKind::Step { rung }` (renamed
  from `StepDown`; the wire is unchanged: a refusal's kind with its flag,
  the rung counted down from `u32::MAX`, rung 0 included). Whether it
  went up is the ladder's to say, so nothing on the wire is new for a
  rung above. The others' fields follow both ways (`RemoteSeat`, the
  stage's chart again for rung 0). A person's streamed score is their
  play's own sum (no rescaling); the room's live stage keeps the rung
  each stream last said (`LiveStage::steps`).
- **The ranking**: the reducer, the live server's early decision and its
  song end, and practice all rank a stage by its own Charts rule
  (`RoomState::rank_by_for`, `Battle`'s `setup.rules.rank_by()`).
- **`ROOM_MODEL_REVISION` is 3**: peers must agree on the ladder, the
  steps up and the ranking. A client of revision 2 is told to reload; a
  newer client meeting an older authority (one whose policy lacks
  `adaptive`) never offers the rule.
- **What a deploy changes**: the server built against this crate makes
  rooms whose policy says `adaptive`, ranks Adaptive stages by the score
  and reads rules stored as "At zero life: Step down" as Adaptive. A
  stage in play across the deploy finishes: its stored rules read as
  Adaptive and rank by the score (where before they ranked by steps
  first), and its clients, which reconnect mid-stage without the model
  check, play it out on the ladder they loaded. The server repository
  needed no code change (one assertion added to a test).
- **CPUs online** follow (`cpus::Drive` takes the full ladder; the
  determinism proof `the_cpus_play_alike` passes natively and for
  wasm32-wasip1; its fixture was regenerated with the full ladder).
- **Known limit**: the percent shown for a remote person who stepped is
  worked out from their sum against the stage chart's most
  (`ddi_of_sum`), so it is off for them; the score shown is right. The
  bots' own plays (`--mp-bots`) have no ladder (their CPUs do).

### Measured

`cargo test --release -p ddi-session --lib adaptive_study -- --ignored
--nocapture` (`crates/session/src/battle/adaptive_study.rs`, which
replaces the step-down study; `DDI_STEP_CORPUS`, the thirteen arcade
mixes of the maintainer's library read in place, aggregates only; 336 s).
Stages: ten CPUs, one at each level, attacks on, from every Medium and
Hard pick with a ladder, two seeds, As picked and Adaptive. MEASUREMENTS
OF CPUS AGAINST CPUS.

Ladders (996 songs, 970 with one; single charts by the stage chart's
slot) and what placing the plan over them costs:

| Stage chart | Charts | Below 0/1/2/3/4+ | Above 0/1/2/3/4+ | Sections kept: rungs below | every rung | Charts losing over a third |
| --- | --- | --- | --- | --- | --- | --- |
| Beginner | 913 | 913/0/0/0/0 | 1/23/1/607/281 | 100.0% | 93.9% | 44 of 912 |
| Easy | 970 | 58/912/0/0/0 | 23/1/662/284/0 | 100.0% | 93.9% | 45 of 970 |
| Medium | 970 | 23/58/889/0/0 | 1/673/296/0/0 | 99.9% | 93.4% | 48 of 970 |
| Hard | 970 | 1/23/58/888/0 | 674/296/0/0/0 | 98.2% | 93.3% | 48 of 969 |
| Challenge | 324 | 28/0/12/3/281 | 324/0/0/0/0 | 82.5% | 82.5% | 46 of 296 |

The full ladder costs a typical stage about 7% of its sections (17% on
Challenge, as before), far from a third: **the merged plan is kept**,
sections per rung were not built. About one chart in twenty loses more
than a third. The bundled songs: every section kept or more.

Where the CPUs finish, from a Medium pick under DDI Normal (1,940 stages a
rule; "rung" from the pick, −2 to +2):

| Level | As picked: out % | place | score | Adaptive: out % | place | score | ups | downs | bounce % | −2 / −1 / 0 / +1 / +2 % |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 72.6 | 9.60 | 775,597 | 6.9 | 9.97 | 1,214,715 | 0.02 | 0.93 | 0.1 | 21.3 / 49.1 / 28.5 / 1.0 / 0 |
| 2 | 36.5 | 8.59 | 1,314,680 | 2.0 | 8.87 | 1,546,702 | 0.31 | 0.43 | 0.4 | 3.7 / 24.5 / 53.4 / 17.8 / 0.7 |
| 3 | 16.2 | 7.72 | 1,587,671 | 1.4 | 8.07 | 1,711,898 | 0.53 | 0.24 | 0.1 | 0.4 / 10.1 / 50.7 / 37.7 / 1.2 |
| 4 | 5.6 | 6.97 | 1,825,383 | 1.2 | 6.97 | 1,963,894 | 0.93 | 0.13 | 0.0 | 0.1 / 2.2 / 23.4 / 66.4 / 7.9 |
| 5 | 1.8 | 6.30 | 2,000,700 | 0.6 | 6.08 | 2,278,441 | 1.27 | 0.06 | 0.0 | 0 / 0.1 / 3.5 / 71.6 / 24.8 |
| 6 | 0.3 | 5.14 | 2,130,131 | 0.0 | 5.01 | 2,543,122 | 1.30 | 0.01 | 0.1 | 0 / 0 / 0.3 / 69.8 / 29.9 |
| 8 | 0.0 | 3.07 | 2,376,768 | 0.0 | 2.98 | 2,857,372 | 1.30 | 0.00 | 0.0 | 0 / 0 / 0.1 / 69.4 / 30.5 |
| 10 | 0.0 | 1.10 | 2,488,190 | 0.0 | 1.01 | 3,029,927 | 1.30 | 0.00 | 0.0 | 0 / 0 / 0.1 / 69.4 / 30.5 |

(+1 is Hard; +2 is Challenge, which 30% of the songs have.) The other
three settings, in short (levels 1 / 5 / 10):

| Pick, timing | Out % as picked | Out % adaptive | Ups a stage | Downs a stage | Bounce % (worst level) | Stronger above weaker: as picked | adaptive |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Medium, Normal | 72.6 / 1.8 / 0 | 6.9 / 0.6 / 0 | 0.02 / 1.27 / 1.30 | 0.93 / 0.06 / 0 | 0.4 | 93.3% | 98.9% |
| Medium, Hard | 100 / 59.1 / 0.2 | 96.2 / 3.5 / 0.1 | 0 / 1.18 / 1.31 | 1.89 / 1.18 / 0.01 | 2.0 | 93.3% | 97.0% |
| Hard, Normal | 94.8 / 11.9 / 0 | 4.9 / 0 / 0 | 0 / 0.31 / 0.31 | 1.71 / 0.10 / 0 | 0.5 | 94.6% | 99.1% |
| Hard, Hard | 100 / 82.8 / 0.7 | 83.7 / 2.7 / 0 | 0 / 0.34 / 0.31 | 2.87 / 1.37 / 0.01 | 3.1 | 93.2% | 97.4% |

What it says:

- **A stronger level still wins**: the share of level pairs where the
  stronger CPU places above the weaker rises from 93–95% as picked to
  97–99% under Adaptive (the strong ones climb to denser charts and earn
  more; ranking by the score sorts the field more cleanly than life).
  The score gap between level 10 and level 1 grows in points (1.71 M to
  1.82 M from a Medium pick under Normal) and shrinks as a ratio (3.2× to
  2.5×): the weakest keep playing and earn something.
- **Nearly everyone plays to the end under Normal** (the weakest out 5–7%
  instead of 73–95%); under Hard timing the weakest still go out on the
  lowest rung most of the time.
- **Bouncing is rare**: at most 3.1% of stages for any level (Hard
  timing, mid levels), under 0.5% under Normal; the eight bars back up
  do their job.
- **Every level from about 4 climbs**: a CPU's strength is relative to
  the chart it is on, so one that plays Medium cleanly plays Hard as
  cleanly, and the CPUs climb to the top rung whenever four clean bars
  come (69% finish on Hard, 30% on Challenge where the song has one,
  from level 5 up). This is a property of the CPU model, not of the
  rule: people will settle where they stop playing cleanly (open
  question below).

**Playing down on purpose** (each chart with an easier rung: a perfect
play of the next easier chart against 85% and 95% plays of it, DDI's sum
as earned, the W0s aside): over the local mixes (3,124 charts) the
perfect easier play beats an 85% play of the chart in 25.9% of charts and
a 95% play in 3.2%; on average it is worth 0.94 and 0.84 of them. The
bundled songs (12 charts): Beatucada beats 85% in 2 of 4 charts and 95%
in 1 (its Hard and Challenge have the same meter and close counts),
Sweeteners 1 and 0 of 2, Some Things Must 1 and 0 of 4, Into My Dream 0
of 2. To get there a player must also empty their life on the harder
chart (a breather of five to eight seconds with nothing scored) and then
play the easier one perfectly, and four clean bars put them back up. So
it never pays against a good play of the harder chart and only
sometimes against a poor one; where charts of neighbouring slots have
close note counts it can.

What 72 CPUs cost with a full ladder (`adaptive_cost_of_72_cpus`, release,
Beatucada's Medium: two rungs below, two above): the ladder planned in a
millisecond; the stage loaded in 114 ms against 48 ms as picked; played
out in 179 ms against 81 ms, 56 of the 72 stepping up and 4 down.

### Tested

- Engine (`crates/engine/tests/adaptive.rs`, replacing `step_down.rs`):
  the ladder above (its order, `Ladder::chart`, `level`); a perfect play
  stepping up once after four bars, on a bar line, the boundary past the
  horizon and a second, life untouched; up then down (from the harder
  rung, back to the picked chart) and back up taking eight bars; down
  then up past the picked chart (eight bars back to it, four on to Hard);
  each the same at 60 and 144 fps and from its log, no note judged twice
  at a boundary; a rule without steps up; the step-down tests as before.
  The opponent: a strong CPU with the full ladder climbs, the same at 60
  and 144 fps, its script valid. The fixtures unchanged.
- Room: the rule's JSON both ways (`charts: "adaptive"`, `at_zero:
  "step_down"` and `"out"` read as Adaptive and As picked, in the match's
  rules and a stage's), the policy, the refusal read from its old name;
  the ranking by the score with life the tie-break and steps never
  (`under_adaptive_charts_those_still_in_rank_by_the_score_and_never_by_steps`);
  the per-stage change refused where the authority does not honor it.
- Session: the player and strong CPUs stepping up alike at 60 and 144
  fps, the cue saying up, the player's own field drawing the old chart to
  the boundary, the details' chart marked harder and the score the play's
  own; one-star CPUs on Hard climbing to Challenge and back down, their
  fields agreeing with their plays both ways at any frame rate; those
  still in ranked by the score; a song with no other chart as picked
  exactly; the claps after a step; live over the fake server
  (`a_live_stage_steps_up_and_the_others_fields_follow`: the plan naming
  every rung on both devices, Alice climbing two rungs, Bob's view of her
  field following, the room's stream saying her rung, her score as
  earned).
- UI: the Rules row "Charts: As picked / Adaptive" and its sentences,
  Apply, the summary ("DDI · Adaptive charts"), Reset, fixed where the
  authority cannot honor it; the Standings' ↑; the `qps-ploc` walk;
  `xtask l10n check` (the Japanese faces cut again for 稼).
- Desktop offscreen (`crates/desktop/tests/step_down_offscreen.rs`): the
  step-down stage as before, and a stage where the autoplayed player steps
  up from Medium (the cue, the old chart to the boundary, the harder one
  from it), a CPU climbs two rungs and the Standings mark ↑. Pictures in
  `target/scratch/mp/adapt/shots/` (`climb-cue`, `climb-new-chart`,
  `climb-cpu-two-rungs`, `climb-standings`, `step-*`).
- Headless Chrome and Firefox (`target/scratch/mp/adapt/browsers.sh`,
  `adapt.mjs`), 1280×720, practice with `auto=1`: the Rules row set to
  Adaptive, a stage picked on Beatucada's Hard (ladder Medium, Easy,
  Beginner below, Challenge above), the player stepping up at 23.6 s
  (boundary 27.5 s) in both browsers, the cue drawn, every field
  stepping, the Standings marking "↑ Challenge" for all four; no console
  errors. Screenshots in `target/scratch/mp/adapt/shots/`.

### The flaky step-down test (cause found)

`a_practice_stage_steps_down_the_player_and_a_cpu` drew a different set
of CPUs on every run: a desktop practice room's seed is the time it was
made (`ddi_desktop::room::current`, `RoomClient::shared(now, now)`), and
the test used the Fixed CPU mode, whose CPUs' seeds come from it. That is
by design (a new practice room, new CPUs), so the fix is the test's: it
sets the CPUs' seed (`rules.cpu.opponent.seed = 7`) and discards any
earlier room. Nothing in the stage, the CPUs' judging or the ledger reads
wall time or frame boundaries: three runs gave the same CPUs' power and
life, frame for frame.

### Not done in this round

- The dancing hand rests after a step up as after a step down; the fresh
  preparation from a rest was not tried.
- No score balancing (the factor is 1 and applied nowhere) and no way for
  a player to hold their rung.
- The percent shown for a remote stepped person (above); the bots' own
  plays have no ladder.
- Time signatures are still not read for bar lines.

## CPUs in private online rooms (2026-11-13)

The maintainer wants private rooms at parity with CPU practice: the host
adds named CPUs to an online room's match as practice does, and everyone
plays against them. The contract is in
[Online play: the shared base](multiplayer-online-play.md), "CPUs"
(`online-cpus-v0`, PROVISIONAL).

### What is built

- **The room** (`ddi_room`): `RoomPolicy::cpus` (additive: true for every
  room this build makes; an authority that predates it never offers or
  takes a roster, `RulesProblem::CpusNotHonored` where one is sent);
  `RoomPolicy::seats_cpus`. The reducer seats the roster's CPUs as in
  practice; a live stage with CPUs takes its people's own reports and its
  CPUs' through the authority's `Command::ReportFor` (`StagePlay::reported`).
  `live::cpus`: the rules (`ONLINE_CPUS_V0`), the binary CPU news
  (`CpuNews`, `wire::CPUS_V1`), `ClientLive::CpuRuns`, who sends
  (`sends_for_cpus`, `cpu-reporters-v0`), report digests; `LiveStage`
  seats each CPU in the ledger (a 2 s horizon's lead), takes the first
  report of each CPU and checkpoint, counts and logs a different one
  (`cpu_mismatches`, `LiveOut::notes`), never makes a CPU a no-show, and
  ends the CPUs' streams as forfeits once nobody is left who could compute
  them (`anyone_computes`). The client's `LiveLink` keeps every CPU report
  it made and sends them while the device is one that sends (all again on
  a new socket or when it becomes one).
- **The game**: `ddi_session::battle::cpus::Drive`, a CPU's online play
  (50 ms steps, the room's shocks put in at their song time, its play
  held until their news came, `ImmediateContinue`), with its proof
  (`cpus/tests.rs`, `fixture.txt`; the same fixture checked built for
  wasm32 under Node). `Battle` in a live stage computes the CPUs beside
  the remote people (their fields on the tap-only copy, drawn the view's
  delay behind; `Battle::track_of`), routes the room's commitments against
  them, sends their reports, ends and runs (`LiveRoom::cpu_news`,
  `cpu_run`); every link, warning, watcher's moment and view counts them
  as seats. Bots compute and report them too (`LivePlay`). An audience's
  seat computes them from the public view (its CPU kind now carries the
  seed, personality and policy, additive) and sends nothing.
- **The screens**: the Rules pop-up offers the roster online where the
  room seats CPUs (`RoomClient::cpus_offered`), the seats kept at the
  room's for people still to come; such a room's picks are charts its
  CPUs can play (`has_cpus`); the players panel, the Players pop-up, the
  miniatures and the standings show CPUs as practice does (stars, never
  Bot). No new message.
- **The server** (`../ddi-server`): builds against the changed crate, its
  room seats CPUs (new rooms), logs the live side's notes; its load
  driver gains `--cpus N`; a native test through the object
  (`the_object_seats_cpus_and_takes_their_reports_from_the_devices`).
- **The bots**: `--cpus N[:LEVEL]` for the host bot's Rules.

### Tested

`crates/room` (`tests/live.rs`): the ledger with CPUs reported by two
devices equal to one device judging everyone; a device computing a CPU
differently counted (30 and more mismatches, the first kept, logged); a
device leaving mid-stage changing nothing, the watcher taking over when
both players left, and the CPUs ended as forfeits when everyone left; an
authority that predates it refusing a roster; 13 people and 60 CPUs
settling (and what they cost in messages); the CPU news' wire (read back
equal, every truncation and 2,000 corruptions refused without a panic).
`crates/session`: the determinism proof; a live stage of two players, a
watcher and three CPUs (Hard timing) over the fake with jitter: every
device's CPUs equal, attacks to and from them, a CPU knocked out, no
mismatch, their runs and details kept. `crates/ui`: the roster offered
online where the room seats CPUs, not where it does not (two policies),
the seats kept. Desktop offscreen (`room_cpus_offscreen`,
`live_offscreen`'s `a_live_stage_with_cpus_…`): the Rules pop-up with the
roster and the players with CPUs, wide and on a phone, English and
Japanese; a live stage with two CPUs and a person, the miniatures drawn
from this device's computation. The server's native test; against a local
`wrangler dev`, the load driver and three bots with three CPUs.
Screenshots: `target/scratch/mp/ocpu/shots/`.

### Not done in this round

- No browser walk of an online room with CPUs: the web reaches it only
  through a server (no hook into the fake room for members); the UI's
  harness and the desktop's offscreen pictures cover the screens, and
  the web build runs the same code (its smoke check passed).
- A winning CPU whose gauge empties after the decision: two devices may
  report its detail differently where the decision reached them on
  either side of that moment (the first report stands, the other counted).
- A device whose play ended leaves its CPUs where the room's news allowed
  it to judge them; the other devices finish them. A room whose last
  device leaves the play before its CPUs' ends waits for the report
  deadline for their runs (60 s after the song's end).


## Follow-ups: Points from a later stage, and four gaps (2026-11-13)

### Points from a later stage (decided by the maintainer)

The maintainer: "I don't think it's bad to allow that, as long as the
host is aware and either the match is not competitive enough for this to
be a problem or it's easy to apply and reset the match if it is."

- **The model** (additive to protocol 2): `PlayRules::points`, an
  `Option<PointsBias>`, `None` for the match's own and left out of the
  JSON then; kept `None` wherever it equals the match's
  (`Match::play_rules_of`, the one form a change is sent, compared and
  kept in; the reducer normalizes what it receives), so a match whose
  Points nobody changes sends, stores and mirrors exactly what it did. A
  change that names none keeps the match's. `Match::rules_for(stage)`
  carries the bias (`MatchRules::with_play`), and the room awards each
  stage by the bias of the rules it was played under
  (`RoomState::confirm`): a field of nine, Mild from stage 3 of four,
  awards stages 1 and 2 on Strong's table and 3 and 4 on Mild's, the
  totals their sum. Every other reader takes the stage's rules already:
  the Standings and the match's result read each stage's explicit
  `StageResult` (its rule and points), the audience's public view names a
  changed stage's rules (`PublicStage::rules`), the bots never read the
  bias.
- **Old and new peers**: a new flag, `RoomPolicy::stage_points` (true for
  every room this build makes, left out when false). `stage_rules` could
  not say it honestly: an authority built with `stage_rules` alone (the
  same day) would drop the bias and award by the match's. A client sends a
  bias only where `stage_points` (and `points_bias`) say so
  (`RoomClient::change_stage_rules`); the reducer refuses one elsewhere
  (`RulesProblem::PointsNotHonored`). An older authority's room shows
  today's From-stage form (Points fixed, its note). An older client in a
  newer room reads a change without its bias: it shows the stage under
  the match's Points, which only its words would get wrong (the room
  awards). Points alone never changes the battle plan's digest
  (`StageRules::play_changed`, the tag names only what a play is judged
  by), so an older device plays such a stage as everyone else.
- **The Rules pop-up**, From stage N: the Points row is editable (‹ › as
  the others), the line under the statement says "Timing, Boos, zero
  life, attacks and Points below apply from stage N on.", the note "Stages
  and CPUs can't change during a match: …". Where the row differs from
  the stage before the one it lands on, a quiet sentence follows the
  row's own: "Stages before stage 3 keep Strong points, so the match's
  totals will add stages awarded on different curves. For a close
  contest, use Apply and restart." (the last sentence where that button
  is offered). Apply and restart takes the Points shown, as the other
  per-stage rows. Next match: as before.
- **The marks**: the status line names it in the change's words ("From
  stage 2: Medium points. Pick stage 2's song."; `room::change_words`),
  and the Standings' "● Changed rules: DDI · Medium points" and the
  match's standings' "● From stage 2: DDI · Medium points" through the
  rules' existing summary words (which name Points where it is not
  Strong).
- **Tested**: `ddi-room` (`walkthroughs`: the field of nine with Mild
  from stage 3, each stage's rule and every total; the JSON both ways,
  the match's own bias kept as none, an older policy refusing a bias;
  `online`: both authorities and every mirror alike), `ddi-ui`
  (`room_tests`: the row, the sentence, Apply, the status line, the
  order, the Standings' marks, an older online authority never offered
  or sent a bias, a co-host's Mild reaching every mirror and status line
  over the fake; the `qps-ploc` walk; `xtask l10n check`). The server
  (`../ddi-server`) builds and passes its checks against the changed
  crate without a change; once deployed its rooms say `stage_points`.

### The Rules pop-up fits

At 1280×720 the pop-up had come to fill the whole height (it was zoomed
smaller where it did not fit, so a long roster or one more row made it
small). Now:

- **The rows scroll inside it**, its title and its buttons always on
  view (`room_dialogs::RulesDialog`: the rows in a clipped frame,
  `Rules.Rows`, as tall as the screen leaves; a thin bar at the right
  edge says where they are; zoomed as before only below two rows). Focus
  moved by keys or a controller scrolls its row into view; Up and Down
  go along the rows in their order, not by where they are drawn
  (explicit links, so a row scrolled away is still the next one, and
  Down from the last row reaches the buttons, the notes shown); the
  wheel and a drag on the rows scroll them, as the Players pop-up does;
  a member's read-only pop-up scrolls with Up and Down. The roster's own
  window of four rows and its "more" marks went: one scroll for the
  whole pop-up (a CPU added or removed keeps the focus on the row that
  takes its place, in view).
- **Its notes show only where they apply**: the stars' note only with
  CPUs in the roster; the Points note under the Points row (where the
  field, the people and the CPUs, is eight or fewer: the choice changes
  nothing there; the fixed row's note where the field is not eight);
  "Applying clears readiness" only in the lobby with someone readied or
  loading; the ruleset's and the battle's notes as before.
- **Tested**: `ddi-ui` (`room_tests`: at 1280×720 by keys and at 844×390
  by a controller, twelve CPUs, the title and buttons whole, each row
  scrolled into view by focus, the wheel, the notes; the roster's test
  through the scroll), desktop offscreen
  (`room_rules_offscreen::the_rules_pop_up_fits_with_few_and_many_cpus`:
  1280×720 and 844×390, English and Japanese, 3 and 12 CPUs; pictures
  `target/scratch/mp/fua/shots/rules-*cpus-*.png`), headless Chrome and
  Firefox (`target/scratch/mp/fua/shots/`).

### The audience's page offers a stage in progress

- **Watch** (`room-audience-watch-stage`, "Watch the stage"): the
  audience's page shows it, primary and focused first as it comes
  (`PageScreen::focus_button`), whenever the room plays a stage the seat
  can follow and is not watching (`AudienceClient::watchable`: its song
  begun, prepared here, the room playing live, the seat not ended). Its
  press hands the stage over as it stands (`AudienceClient::watch_now`:
  the order made again, the play joining part-way as a late watcher's
  does; the stage's music delay kept as it was the first time), the
  shell told first inside the press (`AudienceCall::WatchPressed`: the
  web makes and starts the audio context there, as the invite page's
  Watch does; desktop needs nothing).
- **Which stages start by themselves**: as before, a stage the seat saw
  gathering or counting down is handed over as it starts. A stage the
  seat first saw already playing (it arrived during it, or its page was
  reloaded) is no longer handed over by itself: the page offers Watch,
  whose press is the gesture that starts the sound. A stage handed over
  and left (the browser's Back on the web) is offered again the same
  way, never handed twice by itself.
- **Tested**: desktop offscreen (`audience_offscreen`: a second seat
  arriving mid-stage is not handed it, its page shows Watch focused,
  Enter tells the shell of the press then hands the stage over; the
  first seat, back from it, is offered it again), headless Chrome and
  Firefox over the fake seat (the stage, the browser's Back, Watch
  focused, Enter, the stage again from where it stands; Firefox with
  autoplay blocked too: `target/scratch/mp/fua/shots/watch-*`,
  `gesture-*`).

### Sound after a reload during a watched stage: nothing to build

With Watch on the audience's page (above), a reload during a watched
stage lands on the page with Watch focused, and its press is the gesture
that makes and starts the audio context before the stage is handed over.
The other way into a watched stage without a press, a stage that starts
by itself after a reload (no press on the page yet, a browser that
blocks autoplay), still shows the prompt's card asking for one press
(the existing "needs a tap" path), and the picture waits for it. So no
path runs a watched stage's picture without its sound, and no "Tap to
hear" plate was built.

### A watched stage's prompt has no key diagram

A watched stage's prompt (a member who chose Watch, a stage watched in
CPU practice, the audience's) showed the player's controls diagram, key
bindings and this play's options, which mean nothing to someone
watching. `PromptInfo::watched` (set by `set_watched_stage`, which both
shells call for every watched stage) leaves them out: no diagram, no
field preview asked for, no Modifiers; the song's card, the line
"Stage 1 of 4 · watching 3 players · Rules: …" and the buttons stay.
Tested: `ddi-ui` (`prompt::tests::a_watched_stages_prompt_has_no_key_diagram_nor_play_options`),
headless Chrome and Firefox with autoplay blocked over the fake seat
(the prompt asking for its press: `target/scratch/mp/fua/shots/gesture-asked-*.png`).

## Decisions round B1: the room's model after the maintainer's answers (2026-11-13)

The maintainer answered the open questions on awards, the host's
succession, who may do what, CPU seeds and older clients on 2026-11-13.

### One award rule for every field size

"Let's go with the 12, 6, ... points progression for fewer players too,
for consistency." Fields under eight read the eight-player table (12, 9,
7, 5, 4, 3, 2, 1) as large fields do, squeezed up the field:
`ddi_room::AwardRule::EightPlayerScaledV1` (`eight-player-scaled-v1`).
Place `p` of `N` is read at position `7 × (p − 1) ÷ (N − 1)`, linearly
between the table's entries, rounded to a whole number (halves up); a
field of one earns 12. Fixed tables in the source, checked by a test in
exact arithmetic (`scaled_tables_follow_the_formula`). The Points bias
changes nothing at eight or fewer.

| Field | Awards |
| --- | --- |
| 1 | 12 |
| 2 | 12 1 |
| 3 | 12 5 1 |
| 4 | 12 6 3 1 |
| 5 | 12 8 5 3 1 |
| 6 | 12 8 5 4 2 1 |
| 7 | 12 9 6 5 3 2 1 |
| 8 | 12 9 7 5 4 3 2 1 |

Five's second place is exactly 7.5 (position 1.75, between 9 and 7) and
rounds up to 8 by the rule; the brief's example had 7.

`ExperimentalLinearV0` is retired as `ExperimentalNineV0` is: never
awarded, still read (a stored or mirrored result keeps the points it was
given). Every "points experimental" mark went: the Rules' fixed row says
"By place", its notes only that the Points choice changes nothing with
eight players or fewer (where it is offered), and the Standings, the
results' footer and the status lines' stage marks no longer say
"points experimental" (`room-mark-experimental` and
`room-rules-points-note` removed from every language). Shared places
share as before (four: two tied first share 12 and 6, 9 each).

### The host's succession (`succession-v1`)

The maintainer: "A co-host role should never be inherited, a member can
inherit the host role if there isn't a co-host to inherit it, but if a
co-host disconnects and there's still a host connected there should be
no role changes", and an administrator who only observes is skipped.
`RoomState::succession` (`Succession::To`, `Wait`, `Nobody`;
`RoomState::heir` its `To` or `Wait`): among the people present who may
run the room (never a bot, never an administrator who only observes),
a connected co-host (the first joined); else a co-host inside the
reconnect grace (`Reconnecting`), waited for; else the connected member
who joined first; else, only where the host left the room, whoever is
coming back, then whoever is away. An administrator who only observes
is `RoomState::observes`: the admin flag, no role of the room's and
watching (how an administrator who is not a member enters, from a
ticket alone, `../ddi-server` `RoomCore::connect`); one who chose Play
competes and may inherit, one made a co-host is a co-host; one who
joined by the invite to watch looks the same and is skipped too.

While a co-host inside the grace is waited for, nothing changes hands:
the room keeps the host gone as its owner (left or away), nobody is
made anything, the pick is nobody's ("Nobody is choosing songs right
now.", operators told they may use Pick songs, which a connected
operator may) until the co-host is back (theirs, with the pick) or the
authority sets them `Disconnected` at the end of their grace (the next
co-host, else the member). `Joining` is never waited for: no grace times
it.

| Who goes | Who is there | What happens to each role |
| --- | --- | --- |
| The host leaves, or is away past the grace (Pass it on) | a connected co-host | that co-host (the first joined) becomes the host; the host gone becomes a member; everyone else as they were |
| The same | co-hosts only inside the grace, members connected | nothing yet; the room waits for the first such co-host: back, the host; past the grace, the next co-host, else the first-joined connected member; the host gone then a member |
| The same | no co-host present (or all past the grace), members connected | the first-joined connected member becomes the host; the host gone a member |
| The host leaves | nobody connected | whoever is coming back, then whoever is away (a co-host before a member) becomes the host; with nobody at all, the room closes |
| The host is away past the grace | nobody connected | nothing: the room keeps its absent host; the first person to connect (a co-host first) has it |
| Either | only bots or observing administrators | as nobody: the room closes (the host left) or waits (the host away) |
| Either, under Close the room | anyone | the room closes for everyone |
| A co-host leaves or is away (any time) while the host is connected | anyone | no role changes; a pick the co-host held goes back to the host |
| The host hands it over (`MakeHost`) | a connected person | they become the host; the host before becomes a co-host (a deliberate act, the one case the room makes a co-host) |

Before: the host gone was made a co-host whenever the room passed on,
and a connected member came before a co-host who was away inside the
grace. The Players pop-up's sentence ("If the host leaves, a co-host
becomes the host, or else the player who joined first") promises the
same order and stays.

### Who may do what

- Letting someone who came by the watch-only link play, and closing and
  opening the audience, are an operator's (the owner, a co-host, an
  administrator): `ddi_room::Capability::LetPlay` (`SetWatchOnly` off)
  and `Capability::CloseAudience` (`AudienceHub::set_open`, which the
  server's `RoomAudience` runs, so the authority's side follows by the
  path dependency). A co-host's Host options
  (`RoomClient::may_host_options`) have only those rows, shown where
  there is something to do (someone held to watching, a room with an
  audience): Let play beside each such person, the audience's Close or
  Open; never the "If the host leaves" setting, Make co-host or Make
  host. A tap on a row of the Players pop-up opens it on those people
  only. Host options opened as the owner's close when the room changes
  hands, as before.
- The invite is the host's and co-hosts' to copy and to replace (the
  maintainer: "they can share an invite and after some people join they
  can regenerate it"). Replace invite link was already More's, the
  owner's alone and done at once; it is now an operator's
  (`Capability::ReplaceInvite`: `SetAccess` with the admission as it
  is; changing the admission stays `ChangeAccess`, the owner's) and
  asked once ("Replace the invite link?": the old invite link stops
  working, and so do the room's other links made from it, watch-only and
  audience; everyone already in the room stays), on both shells (the
  room's song select's More). What it does to the other links: the
  server derives the watch-only link's secret from the invite's and the
  audience link's from the watch-only link's (`../ddi-server`
  `invite::watch_secret`, `audience_secret_of_watch`), so all three
  change together; a join by any old one is refused (`invite_rotated`,
  `audience_link_unknown`); seats already in the audience stay
  (`AudienceHub::set_key`); members stay.
- The watch-only link is kept but hidden ("Keep, but maybe we can keep
  watch-only hidden"): joining by one works exactly as before; More
  offers Copy watch-only link only with the hidden option
  `Settings::multiplayer_watch_link` (off by default; the web's
  `mpwatchlink=1`/`0` beside `mp=1`, desktop's `--mp-watch-link on|off`).
  More shows Copy invite link and Copy audience link otherwise.

### A CPU's seed per stage (`cpu-stage-seed-v1`)

A CPU kept one seed for the match, so the same chart picked twice was
played the same twice. Each stage's seed is now
`ddi_room::OpponentSpec::stage_seed(seed, number)`: the CPU's own seed
for the stage numbered 1 (plays and fixtures recorded before hold), else
that seed and the stage's number through SplitMix64's finalizer (plain
integer arithmetic, the same natively and in wasm). Every place that
builds a CPU's seat for a stage takes it: practice and an online room's
CPUs (`RoomClient`'s stage order, which the `Drive` of every device
computes from) and the audience's local computation
(`AudienceClient::take_stage_order`). The bots play people's seats with
seeds of their own per stage already. Tests: the stage order's seeds
(`reported_stages_run_a_match_to_its_results_and_the_next_one`), the
same chart in two stages played two ways
(`the_same_chart_in_two_stages_is_played_two_ways`), and the proof
`the_cpus_play_alike` with its fixture unchanged, natively and for
wasm32-wasip1.

### "Reload to update": the room model's revision

The maintainer: "Refusing with 'reload to update' seems useful, I'd
rather not focus on keeping compatibility right now." From now on rounds
need not build per-feature soft failures for older clients: raise
`ddi_room::ROOM_MODEL_REVISION` (1, holding this round's changes)
whenever peers must agree on something new, and the older client is
told to update.

- The wire: the client says `"model": N` in the bodies of the requests
  where the authority first hears from it: `POST /rooms/<id>/join`,
  `POST /rooms/<id>/ticket` (every socket, so every reconnect and reload)
  and `POST /rooms/<id>/audience` (`ddi_ui::state::online::api`). The
  authority (`ddi_room::check_model`) refuses a client whose revision is
  lower than its own, or that says none, with the problem body
  `{"error":"update_required"}` (`ddi_room::UPDATE_REQUIRED`): 409 at a
  join or an audience join, 403 at a ticket. A ticket is checked by the
  room (`RoomState::admits_model`, `../ddi-server`
  `RoomCore::admits_model`) and never refused while a stage is under way
  (its countdown, play or song's end): a deploy restarts the room's
  sockets, and a member who reconnects mid-stage plays it out; their next
  reconnect between stages is refused. A socket that stays open is not
  checked again (none survives a deploy of the server).
- What a new client shows: an invite's page whose join was refused says
  "The game was updated. Reload to play in this room." with a Reload
  button (the web: `UiRequest::Reload`, a real reload of the page) or
  "This room needs a newer version of the game." with Back (desktop); a
  member refused a ticket ends its link `LinkState::UpdateRequired`: the
  room's status line says the same, and the web's More has Reload. The
  bots stop with a line.
- What an older client shows (one that predates the revision): it sends
  no `model`, so a new server refuses it. Its join and audience join
  show their usual "Couldn't join (update_required)."; a ticket refused
  with 403 ends its link as Left ("You left the room" on its status
  line), the closest it has.
- The other direction stays soft: a client newer than its authority
  sends a `model` the older server ignores, and keeps hiding what that
  authority does not honor by the policy flags, as before.
- What a deploy changes: deploying the server after the game makes every
  client from before this build (no `model`) unable to join, watch or
  reconnect between stages until it reloads (web) or updates (desktop);
  stages under way finish. Deploying the game first changes nothing
  until the server follows (the new client's `model` is ignored).
- `FakeServer::set_model`, `admit_join`, `admit_ticket` stand for an
  authority ahead; tests: `an_older_room_model_is_refused_except_during_a_stage`
  (the model), `an_authority_ahead_refuses_an_older_client_between_stages`
  (the client, its invite page and its link), and the server's
  `a_ticket_for_an_older_room_model_is_refused_between_stages`.
## The maintainer's answers of 2026-11-13, client side (round B2)

Each part below is a question the maintainer answered on 2026-11-13,
built as answered: practice's lead, the practice roster (the provisional
table's "CPU practice … forgotten" row), the audience's count (its row
too), attack lines for everyone who watches, a watching member's music,
the hold words and the Boos row, three small fixes. Development builds
here skipped FFmpeg (`DDI_NO_VIDEO_HELPER=1`, `DDI_NO_VIDEO=1`).

### Practice's attacks land when online's do

The maintainer: "Practice should behave similar to how online works, so
that practice is more meaningful." Online, an attack's section may start
no sooner than `LEAD_V0.of(horizon)` plus the network's lead (1.25 s,
`LiveRules::network_lead_us`) after its checkpoint; practice used
`LEAD_V0` alone, so its shocks came about 1.25 s sooner. Practice now
reads the same lead (`ddi_session::battle::practice_lead_us`, from
`PRACTICE_LEAD`, the live rules a room is made with, `LIVE_V2`, whose
network lead is `live-v1`'s 1.25 s, unchanged), for the player and the
CPUs alike. The attack-rate study (`attack_rates`, its six charts, four
seeds; it now also counts the checkpoints a full bar waited, for no one
to attack or no safe section), before and after:

| DDI Normal, eight CPUs at one level | launched a seat a stage | life a received attack | out | checkpoints waited a seat |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 0.88 → 0.87 | 32.8 → 34.0 | 62.0% → 64.6% | 0.00 → 0.06 |
| 3 | 2.07 → 2.06 | 31.7 → 32.0 | 37.0% → 37.0% | 0.30 → 0.46 |
| 5 | 3.57 → 3.59 | 29.0 → 28.9 | 15.6% → 12.5% | 0.34 → 0.37 |
| 7 | 5.01 → 4.96 | 21.3 → 22.4 | 3.6% → 4.2% | 0.90 → 1.20 |
| 10 | 6.33 → 6.30 | 3.3 → 3.3 | 0% → 0% | 1.36 → 1.54 |

A mixed field (levels 1, 2, 3, 4, 5, 6, 8 and 10): 26.2 → 25.7 attacks
a stage under Normal, 36.5 → 36.0 under Easy, 15.0 → 15.2 under Hard;
the checkpoints a full bar waited, a stage, 4.5 → 6.6 (Normal), 10.2 →
13.1 (Easy), 2.0 → 4.2 (Hard). By chart (Normal, mixed): Beatucada
Medium 15.8 → 15.8, its Hard 19.2 → 19.2, Sweeteners Hard 24.5 → 25.8,
Into My Dream 24.2 → 24.2, Some Things Must Hard 36.5 → 33.8, its
Challenge 40.0 → 37.5. So about as many attacks, a little more waiting
(a section 1.25 s further on is more often taken or past the chart's
room), the same damage: the lead moves when a stage's shocks land, not
how many there are.

### The practice roster is remembered

A practice room's CPU roster (each CPU's name, level, personality and
attack policy, in order) is kept in the settings whenever its Rules are
applied (`Settings::practice_roster`, written by `RoomClient::save_rules`
and `restart_match` where the room is the shells' practice room,
`RoomClient::practice`), and the next practice room starts with it
instead of the default three. Kept as a versioned value of its own
(`cpu_roster::kept`, `{"v":1,"cpus":[…]}`), read only when a practice
room is made (`cpu_roster::from_kept`): a later version, a level or
personality this build lacks, a name twice or more CPUs than the cap is
the default roster, and the rest of the settings is never touched by it
(the field is plain JSON to the settings, so an unreadable roster never
fails their load). Each CPU's seed is the new room's own (the same CPUs,
not the same plays). Not kept: the stage count and the other rules, the
player's own attack policy (decided: "Don't remember your last choice"),
online rooms, the development switch's rooms. Reset in Rules still gives
the default (applied, that is what is kept then). Both shells make their
practice room through `RoomClient::shared`, so both remember. Tested:
`ddi-ui` `a_practice_rooms_roster_is_what_the_next_one_starts_with`
(through the Rules pop-up: two added, one removed, Apply; the next room's
CPUs the same in order with their own seeds and four stages; Reset then
OK kept as the default; three unreadable values falling back).

### The audience's count between stages only; attack lines for those who watch

- **The count** ("· 132 watching") shows to the room's people between
  stages only: the room's song select status line (not while a stage is
  counted down, played or completing: `screens::room::status` leaves it
  out in `Countdown`, `Stage` and `SongCompletion`), Host options, the
  invite page and the audience's page. Taken out of a stage's prompt
  (`screens::room::status_line`, the line alone). Nothing in a play or a
  watched stage ever showed it (checked: the shells' play view and the
  session read no count). Tested: `ddi-ui` (the status line with and
  without it), `ddi-session` (never on the status line while the
  audience's live stage is played).
- **Who attacks whom, for everyone who watches** (the maintainer: "all
  attack lines … for the overall view, and incoming/outgoing attacks for
  a selected player view"). The three ways of watching (a member who
  chose Watch, a player out of the stage watching on, an audience's
  seat) already drew through one path (`PlayView`: the grid's every link
  between fields, `attack_links::shown_between`; the focus view's
  followed competitor's links to and from the fields beside it and their
  warning by the large field's receptors, `SeenLink::of`,
  `Battle::incoming_for`), from one list (`Battle::seen_links`, the
  audience's from the ledger its frames carry). What differed: a player
  out of the stage had no field among the others for their own attacks
  still on their way. Now their field is one (`Battle::own_seat`, after
  the others', as the play view places their miniature while they follow
  someone): their attack is drawn from it; none against them (an attack
  on them ends with their going out). Around their own dimmed field
  (following nobody) the field is the band, which a line between
  miniatures never enters, so it is not drawn there.
- **A large field's grid**: a page shows only the lines between its own
  fields; an attack from or to a field on another page is left out (no
  stub at the grid's edge: a stub says nothing of whom, and twelve
  fields a page already carry many lines); that field's page shows it.
- **Tested**: `ddi-session`
  `everyone_watching_sees_every_attack_between_the_fields` (a spectator's
  practice stage: every link between two of the four fields, most of the
  stage's attacks seen; a player who leaves with an attack on its way:
  drawn from their field, nothing against them), the audience's live
  stage (the member who watches and the audience's seat see the same
  lines, by their ends' participants); `ddi-shell` (a link with an end
  off the page left out); `ddi-desktop` `room_watch_offscreen` (a watched
  practice stage: lines in the grid with a warning, then a followed
  competitor's own lines in the focus view; pictures `watch-grid-attack`
  and `watch-focus-attack`, looked at).

### A watching member's music matches the fields

In an online room that plays live, a member who chose Watch heard the song
on the room's time while the others' fields were drawn the view's delay
behind it (about half a second). Now their music is put behind too, as
the audience's is: `WATCH_MUSIC_V0` (`watch-music-v0`, PROVISIONAL,
`ddi_ui::state::room`), the audience's rule (`AudienceView`) with a
member's numbers: 0.4 s with nothing measured, fixed when the stage is
handed over (`RoomClient::watch_music_ms`), the next watched stage's up
by what the fields still needed beyond it (their 90th percentile past
100 ms, rounded up to 50 ms) to at most 0.8 s, which leaves the fields
0.2 s of the live rules' one-second ceiling, down 0.1 s a stage after
one that needed less. The room client's `LiveRoom` gives the stage's
song zero that much later (the play's clock is the music's), the fields'
own delay from nothing beyond it (`LiveRoom::view_delay_for`), and keeps
the fields' delay a frame (`note_view_delay`). The CPUs such a device
computes are still judged on the room's song time (`LiveRoom::cpu_lead_ms`,
`Live::cpu_lead_us`), so what it reports for them, where it is one that
sends, is not late.

- **CPU practice** has no delay (its CPUs are judged here: nothing is
  behind) and stays on time.
- **A player out of the stage who watches on keeps the music on time**
  until the stage ends: the music is one take, and moving it mid-song is
  a jump that is heard (or a slow pitch change); the stage's end comes
  within the song anyway, and their own field, which is on time, is
  still on screen beside the others'. The fields are drawn behind as
  before.
- Tested: `ddi-session` (the audience's live stage, its member who
  watches: the music 0.400 s behind the room's song time throughout, the
  players' on time, the fields equal to the audience's at the end).

### Hold judgments in the player's language, and a Boos row on the results

- **Held and Off** follow DDI's other judgment words where Options →
  Rules → Judgment words is "In your language" (the default): two
  messages, `judgments-held` and `judgments-dropped` (English "Held" and
  "Off"; "Off" is the one message to rename should "Dropped" be chosen),
  `ddi_l10n::judgments::Word::Held`/`Dropped`, taken by
  `JudgeNames::shown` (tiers 8 and 9), so the HUD's word by a hold's
  receptor and the results' "Held / Off" row follow, as the other words
  do (a kept play in the language of now). Japanese (an unverified
  draft, in `locales/ja/GLOSSARY.md`): キープ and オフ, plain game
  katakana, neither early nor late; Dancing☆Onigiri's キター / イクナイ
  were considered and read as net slang.
- **Boos as a row of their own** on the results where the play had Boos
  on: last among the judgments, its count, the Boo's color
  (`Judgement.Boo`), the word in the player's language ("Boos", ブー); off,
  no row (and no Boos line among the counts any more under DDI). A kept
  play's record keeps its ruleset's options (`RulesetKey`'s DDI options,
  Boos among them), so its results and its replay's show the row exactly
  where it was played with Boos on. The stage's Standings are unchanged.
  Other rulesets keep their "Stray presses" line as before.
- Tested: `ddi-ui` (the row, its color and count, none with Boos off,
  Japanese words for the row and the holds), `ddi-engine` (`shown` takes
  the holds' words), `ddi-l10n` (the words in English); `xtask l10n
  check`; headless Chrome and Firefox (a DDI play with Boos on, quit,
  its results in English and Japanese: `results-boos-*`, looked at).

### Three small fixes in the Rules pop-up and the status line

- **A row that grows stays in view.** A row's sentences under it (Points'
  and At zero life's) now count as the row's when focus or a press
  scrolls it into view (`RulesDialog::scroll_for`, the sentences' end
  kept as built, `below`), so changing Points with the mouse at the
  bottom of the scrolled rows brings its new sentence up with it; the
  row's top wins where both cannot show.
- **The status line reads in order.** Where a change is for a later
  stage than the one the line is about (being picked, gathered or
  counted down), the line comes first and the change after it, one
  message (`room-status-then-rules-change`), joined by a dot as the
  audience's count is: "Stage 1 starts once everyone is ready · 30 s ·
  From stage 2 on: Medium points" (ja: "… · ステージ2からは：…"); where it is
  the same stage, as before ("From stage 2: Medium points. Pick stage
  2's song.").
- **A change that names no Points keeps the stage's.** The reducer read
  a change without a bias as the match's own Points, so a change from a
  later stage by a peer that names none (an older client's, which cannot)
  put Points back to the match's after an earlier change had moved them.
  `ddi_room`'s reducer now completes a change from the rules of the stage
  it lands on as they stand (`Match::rules_for(from)`, one line in
  `reduce.rs`), and the client names the bias wherever it differs from
  the match's or from that stage's, so a change back to the match's own
  is still said (to an authority of either build). A match whose Points
  nobody changes sends and keeps what it did.
- Tested: `ddi-ui` (`a_rows_new_sentence_is_scrolled_into_view`: the
  Points sentence in view after a mouse press at the rows' foot, failing
  without the fix; the status line's order both ways, in English; a later
  change of the timing through the pop-up keeping Medium;
  `a_change_naming_no_points_keeps_the_stage_s`: a change sent naming no
  Points after one that moved them keeps them through the reducer, and a
  change back to the match's own is said and taken); `ddi-room`'s own
  suite unchanged (no test of its own added there: the other session
  edits that crate's tests); `xtask l10n check`; the `qps-ploc` walk.

## Attacks, version 2: over once placed, chained without limit; "Last standing" before "Stage won" (2026-11-13)

The maintainer, asked when an attack counts as over: "when the last
shock is added into the chart the attack counts as over"; on chaining:
"by default I think it should be possible to 'chain' multiple attacks,
although successive attacks would be placed after one another and not
overlap in the same chart region"; and on the stage's decision: "we can
differentiate between 'last standing' and 'stage won' … to rule that the
stage has been won has to wait for it to be played until the end, since
that player could still fail and cause a tie."

### The rules as built (`attacks-v2`, `settlement-v1`, PROVISIONAL)

- **`attacks-v2`** (`ddi_room::attacks::ATTACKS_V2`, what stages play:
  `ATTACKS`; `attacks-v1` and `attacks-v0` kept by name, every version
  found by `attacks::attack_rules(id)`). The bar, the cost, the
  checkpoints and the power classes are `attacks-v1`'s (cap and cost
  100, a checkpoint every four beats), no recovery.
- **An attack is over for the ledger the moment it is committed.**
  Nothing waits any more: anyone still in can be attacked at any time
  (`AttackRules::chain` is `CHAIN_UNLIMITED`), and the sender's and the
  recipient's bookkeeping no longer depends on an attack passing (its
  states still go Active, Cleared and Recovered at the checkpoints, for
  what is drawn and for the settlement; they hold no launch back).
- **Placement**: a launch commits at once into the first safe section
  that starts no sooner than the recipient's lead (as before) **and at
  or after the moment the last attack already placed against them
  clears** (its last shock and its window: `Section::clear_us`), so
  successive attacks against one recipient follow one another and never
  share a region. No gap is kept between them (`AttackRules::chain_gap`,
  0). The search reaches to the song's end (`search_beats` unbounded);
  where no such section is left the launch is `Blocked`
  (`NoSafeSection`) and the power kept, as before.
- **Targeting** is unchanged in kind. "Eligible" is now everyone else
  still in. **Spread** picks whoever has the fewest attacks still to come
  against them (committed, not cleared), then the fewest sent this stage,
  then the seeded tie (`AttackRules::version` 2); Retaliate and Leader
  as before.
- **No cooldown** (`AttackRules::cooldown_checkpoints`, 0): a named place
  for one, a sender's wait in checkpoints after a launch; no Rules row
  offers it.
- **`settlement-v1`** (`SETTLEMENT_V1`, what stages use: `SETTLEMENT`):
  `settlement-v0`'s decision and ranking. **Last standing**
  (`Settlement::last_standing`) is the one competitor still in while
  attacks against them are still to come; the stage is **won** once
  every shock already placed against them has passed with them still in;
  if they go out before that, first is shared as before ("Drawn: final
  attack"). Nobody out launches, so nothing new is placed once they are
  last standing; an attack committed at the checkpoint before the last
  other competitor went out (they were still in at it) is placed and
  waited for (traced in `settlement_v1_tells_last_standing_before_the_win`).
  Where nothing is pending the two coincide and only the win is told.
- **The room model's revision** is 2 (`ROOM_MODEL_REVISION`): the
  ledger's rules are something every device and the room must agree on.
  A client of revision 1 is refused at a join and a ticket between
  stages ("Reload to update").
- **A stage in play when a server is deployed**: a restart loses the
  stage's live part, and the stage is settled on its reports as before
  (`a_restart_mid_stage_settles_that_stage_on_its_reports_and_the_next_plays_live`,
  the server's). A stage whose Durable Object only hibernates keeps its
  stored plan, and the room runs its ledger under the rules the plan
  names: a stage begun under `attacks-v1` finishes under it
  (`a_stage_finishes_under_the_attack_rules_its_stored_plan_names`, the
  server's; `a_stage_runs_under_the_attack_rules_its_plan_names`,
  `crates/room/tests/live.rs`). The plan's digest covers the rules' id,
  so a device of the other build sits out of the attacks as for any
  mismatch.

### What a player sees

- **The warning** by the receptors (`ddi_shell::battle_cue`) is the
  nearest attack's, with a smaller bolt for each attack placed behind it
  in a column at its right, up to three (`BEHIND_GLYPHS`); past three, one
  bolt and the count ("×5"). They are never dimmed (still to come); it
  all stays inside the warning's place, which the miniatures keep clear
  of. `Incoming::behind`, `Cue::behind` (the web's debug state keeps
  `more` as `behind > 0`). Reduced motion as before.
- **Who attacks whom**: the lines and outlines last from the commitment
  until that attack's last shock has passed, as before; a chained one is
  still drawn quieter.
- **The status line** says "Last standing" (`room-play-last-standing`)
  in the place "Stage won" takes, while the player is last standing
  (`Battle::last_standing_me`), then "Stage won". Someone watching is
  told "X is last standing" (`room-watch-cue-last-standing`,
  `WatchMoment::LastStanding`, kept on the line until the decision as
  the decision is). No announcer line (a cue only). The Rules' sentence
  about attacks does not describe a limit, so it is unchanged.
- **CPUs**: their response to shocks (`MINE_RESPONSE_V1`) is per attack
  and unchanged; under several chained attacks each attack's shocks
  reach the CPU once (the study checks that no CPU steps on more shocks
  than eight an attack received, and the session's chained test that
  every field holds eight shocks an attack).

### Shown

`target/scratch/mp/av2/shots/` (desktop, offscreen, 1280×720,
`chained_attacks_offscreen`: three CPUs on the Leader policy against the
autoplayed player, Beatucada's chart 5, Hard timing):
`warning-2-behind.png` (the warning with two small bolts: three attacks
placed against the player at once, in sections from 41.56 s, 45.37 s and
49.43 s), `last-standing.png` (the three CPUs out at 77.6 s with an
attack still to come: "Last standing"), `stage-won.png` (its last shock
passed at 86.1 s: "Stage won").
Headless Chrome (WebGPU) and Firefox (WebGL2, through the verify
skill's helper), a practice stage of eight CPUs on Some Things Must's
Challenge chart, the autoplayed player on Spread
(`target/scratch/mp/av2/look.mjs`): the plain warning and the warning
with one small bolt (`av2-chrome-…-warning-1-behind-t28.6.png`,
`av2-firefox-…-warning-1-behind-t18.1.png`); under Spread the CPUs
never placed more than two against the player at once, as the study
says. No console problems.

`step_down_offscreen` failed once in the desktop suite and passed four
times alone, under `attacks-v2` and `attacks-v1` alike: the practice
room's CPUs play differently run to run there (their powers differ from
the first checkpoints, before any attack), so whether a weak CPU steps
down is chance. Not this round's; noted.

### Measured: what unlimited chaining does

`chain_pileups` (`crates/session/src/battle/study.rs`, opt-in:
`DDI_ATTACK_RULES=<v1|v2-gap1|v2-cap2|v2-gap1-cap2|v2-cool2> cargo test
--release -p ddi-session --lib chain_pileups -- --ignored --nocapture`,
none for `attacks-v2`): the attack-rate study's six charts, two seeds
each, fields of 4, 9 and 33 CPUs of mixed levels (1, 2, 3, 4, 5, 6, 8,
10 in turn), the player gone at the start, every CPU on one policy,
under DDI Normal and Hard. MEASUREMENTS OF CPUS AGAINST CPUS
(`MINE_RESPONSE_V1`). "Run": attacks against one recipient each in the
very first safe section after the one before it cleared; "at once": the
most placed against one recipient at a time; "filled": of the attacks
that would fit back to back over the most attacked one's attacked span,
how many it got; "blocked": checkpoints a full bar waited, of those with
one (mostly the song's tail, where no section is left).

`attacks-v1` → `attacks-v2`, DDI Normal (attacks a stage; longest run,
mean and max; most at once; the most attacked one's share of attacks and
its span filled; blocked):

| Field, policy | Attacks | Run mean / max | At once | Top share / filled | Blocked |
| --- | ---: | ---: | ---: | ---: | ---: |
| 4, Spread | 10.2 → 10.4 | 1.3/2 → 1.2/2 | 2 → 2 | 35% / 26% → 33% / 25% | 24% → 23% |
| 4, Retaliate | 9.5 → 9.6 | 1.2/2 → 1.3/2 | 2 → 2 | 37% / 20% → 38% / 19% | 28% → 27% |
| 4, Leader | 10.3 → 10.7 | 1.7/2 → 2.1/3 | 2 → 3 | 44% / 28% → 46% / 28% | 25% → 24% |
| 9, Spread | 26.8 → 26.8 | 2.0/2 → 1.1/2 | 2 → 2 | 17% / 28% → 17% / 23% | 21% → 23% |
| 9, Retaliate | 28.8 → 28.8 | 1.7/2 → 1.7/2 | 2 → 2 | 22% / 25% → 21% / 25% | 21% → 19% |
| 9, Leader | 30.7 → 30.8 | 2.0/2 → 11.6/18 | 2 → 6 | 48% / 56% → 74% / 85% | 23% → 31% |
| 33, Spread | 103.8 → 106.0 | 2.0/2 → 1.0/1 | 2 → 2 | 5% / 22% → 4% / 21% | 17% → 16% |
| 33, Retaliate | 106.1 → 106.8 | 2.0/2 → 2.2/3 | 2 → 3 | 7% / 31% → 6% / 26% | 17% → 17% |
| 33, Leader | 116.5 → 84.8 | 2.0/2 → 26.3/39 | 2 → 32 | 16% / 67% → 33% / 97% | 25% → 85% |

Under Hard the same shape, smaller (33, Leader: 78.2 → 69.0 attacks,
run 2.0/2 → 24.1/33, at once 2 → 23, 95% of the span filled, blocked
24% → 72%; Spread and Retaliate at most 2 at once and runs of 2).
Neither rule drew a stage by a final attack in these runs (0%); "Last
standing" came in 8% to 25% of the Hard stages of 4 and 9 (5 to 6 s
before the win), the same under both, and never in Normal or in 33.

Life lost to shocks and knockouts, by level (a seat; life points as the
study counts them, a mine's and a Miss's cost by the difficulty): the
pile-up falls on the strongest, who dodge best. 33 under Normal, Leader:
level 10 received 14.2 attacks a seat under `attacks-v1` and 20.9 under
`attacks-v2`, losing 36 and 61 life, knocked out in none; level 8 9.6 →
0.3; levels 6 and below next to nothing. Under Spread and Retaliate the
by-level lines move by noise. No CPU stepped on more shocks than eight an
attack received (asserted).

What it says:

- **Spread and Retaliate do not pile up.** With Spread preferring the
  fewest still to come, chains stay at two at most in every field; a
  launch is never blocked for want of someone to attack (only for want
  of a section, at the song's end).
- **Leader buries one competitor in a large field.** Nine or 33 CPUs all
  on Leader send every attack at the one with the most life: in 33, a
  run of 26 sections on average (39 at most) filled one after another,
  up to 32 attacks placed at once, 97% of what could be placed over its
  span; 85% of full bars then wait because the leader's chart is full to
  the song's end (and those bars are not spent elsewhere). The CPUs at
  level 10 survive it (they dodge), so it is not unplayable for them; for
  a person it would be shocks in every safe section for the rest of the
  song. Every CPU on Leader is the extreme: CPUs are Spread unless a
  roster entry names a policy, and a person chooses Leader for
  themselves.
- **The limits measured** (none built: the default is Spread, which
  never piles up), 33 under Normal, Leader:

  | Rules | Run mean / max | At once | Filled | Blocked |
  | --- | ---: | ---: | ---: | ---: |
  | `attacks-v2` | 26.3/39 | 32 | 97% | 85% |
  | a gap of one safe section (`chain_gap` 1, `v2-gap1`) | 1.0/1 | 23 | 69% | 93% |
  | at most three at once (`chain` 2, `v2-cap2`) | 12.0/18 | 3 | 90% | 30% |
  | both (`v2-gap1-cap2`) | 1.0/1 | 3 | 68% | 43% |
  | a sender's cooldown of two checkpoints (`v2-cool2`) | 26.3/39 | 32 | 97% | 85% |

  A cooldown changes nothing: a bar takes longer than two checkpoints to
  fill again. A gap alone keeps a free section between attacks but still
  stacks them far ahead. A cap alone keeps the leader's queue short, but
  the leader is chosen again the moment one clears, so runs stay long.
  **Proposed** as the smallest fair limit, should Leader's pile-up
  matter: both together (`attacks-v2` with `chain: 2, chain_gap: 1`): at
  most three attacks placed against one competitor at a time, and one
  free safe section after each, so nobody is ever hit in two sections in
  a row; full bars beyond it go to the next leader (Spread and Retaliate
  unchanged by it: their chains are shorter than the cap).

### Tested

- `ddi-room` `attacks/tests.rs`: `the_rules_are_named_and_v2_chains_without_limit`,
  `v2_places_a_chain_of_any_length_one_section_after_another` (four at
  once against the leader in four consecutive sections, a fifth behind
  them), `v2_spread_prefers_the_fewest_still_to_come`,
  `v2_blocks_when_no_section_is_left_and_searches_to_the_end`,
  `v2_gap_and_cooldown_are_named_rules_off_by_default`,
  `settlement_v1_tells_last_standing_before_the_win`; `attacks-v1`'s and
  `attacks-v0`'s traces kept. `tests/live.rs`:
  `a_stage_runs_under_the_attack_rules_its_plan_names`, and the room's
  ledger through `FakeServer` checked to never place two attacks against
  one recipient in one region. `tests/authority.rs`: the revision is 2.
- `ddi-session`: `chained_attacks_are_delivered_back_to_back_into_plays_and_cpus`
  regenerated for `attacks-v2` (three level-10 CPUs on Leader against
  the player: 6 of 12 attacks chained, up to 2 behind the nearest, each
  at or after the ones before it cleared, every one in its recipient's
  play and field, the record the same at 60 and 144 frames a second);
  `an_early_win_waits_for_the_last_attack_to_clear` (last standing before
  the win wherever the stage waited, never where it did not);
  `a_watched_stage_ends_when_the_winner_finishes_and_a_watcher_leaves_freely`
  (the watcher's "X is last standing" exactly where the stage waited).
  The online CPUs' proof `the_cpus_play_alike` passes natively and built
  for wasm32-wasip1; its fixture, regenerated, is unchanged (its stages
  place no chain longer than `attacks-v1` would).
- `ddi-shell`: `it_scales_in_dims_in_sight_and_marks_those_placed_behind`,
  `it_sits_beside_the_receptors_and_follows_reverse` (one to three small
  bolts inside the warning's place, then one and the count).
- `ddi-desktop`: `chained_attacks_offscreen` (above).
- The server (`../ddi-server`): its tests pass against the changed crate;
  `a_stage_finishes_under_the_attack_rules_its_stored_plan_names` added.

## Attacks, version 3: a placement window and Attacks at once; Shift changes the policy (2026-11-13)

The maintainer, on the pile-up study: "So attacks are queued
indefinitely, I think we should put a cap on how far in the future they
can be placed to limit the chaining. That is, if an attack cannot be fit
into the next, say, 15 seconds, then the player is not a valid target. In
addition to that cap we can have a match rule for how many attacks can be
added within that window." On the window: "I think this window is more
intrinsic to what is a balance of what is practical to use and what
actually counts as an attack, since if it takes too long it kind of
defeats the purpose of targeting a leader or retaliating (both of which
could have changed a lot)." And on the policy: "we should have an easy
way to switch the targeting mode during a match and make that way
visible on the ready screen, I'm leaning towards making 'shift' the
shortcut."

### The rules as built (`attacks-v3`, PROVISIONAL)

- **`attacks-v3`** (`ddi_room::attacks::ATTACKS_V3`, what stages play:
  `ATTACKS`; `attacks-v2`, `attacks-v1` and `attacks-v0` kept by name
  with their traces, `ALL_ATTACKS`, `attack_rules(id)`). `attacks-v2`'s
  bar, cost, checkpoints and chaining: each attack against one recipient
  in the first safe section at or after the moment the last one placed
  against them clears, never the same region, no gap, no cooldown.
- **The window** (`AttackRules::window_us`, `ATTACK_WINDOW_V0_US`,
  `attack-window-v0`: **15 s of song time**, one balance value, not a
  match rule). A launch may be placed against a recipient only in a safe
  section that **starts within 15 s of the launch's checkpoint**. The
  recipient's lead is inside it: from a checkpoint, the earliest start is
  the lead (`practice_lead_us`: the reading horizon, 1.5 s of warning,
  0.1 s of delay and the network's 1.25 s; 4.85 s for a 2 s horizon) or
  the next checkpoint, whichever is later, so about 10 s of the window is
  left at a 2 s horizon, 7 s at a 5 s horizon, and 5 s at the lead's cap
  (10 s, a slow speed raised on a stage to fit it). A safe section can
  start every four beats and lasts about as long with its clear, so how
  many fit back to back in what is left depends on the tempo: measured on
  the charts (below), a median of two under 120 BPM, three from 120 to
  160 BPM and four over 160 BPM.
- **Attacks at once** (the match rule, `MatchRules::at_once`,
  `PlayRules::at_once`, `ddi_room::AttacksAtOnce`: 1, 2, **3 by default**,
  No limit; `AttackRules::with_at_once`, `chain` = the limit less one):
  how many attacks may stand placed and not passed against one player at
  a time. Per stage like Attacks (changeable from a later stage), additive
  in the JSON (left out at three), read only where Attacks is On.
- **Who is a valid target**: anyone else still in, under the limit, with
  a safe section starting within the window. **A sender's policy now
  orders every valid recipient and the launch goes to the first with
  room** (before `attacks-v3` only the first choice was tried):
  - **Spread**: by the fewest attacks still to come against them, then
    the fewest sent this stage, then the seeded tie; the next in that
    order where the first has no room.
  - **Retaliate**: whoever attacked the sender last, where they are valid
    and have room; else Spread's order.
  - **Leader**: by life, then the timing points earned, then the seeded
    tie; a buried leader (at the limit, or with no room in the window)
    spills over to the second, the third, and so on.
  - Nobody valid: the launch waits, the power kept, as before. The
    reasons: no one to attack (everyone else out or at the limit,
    `NoEligible`), no safe section to the song's end (`NoSafeSection`),
    and, new, **no room within the window** where some recipient has a
    section only later (`BlockReason::NoRoomInWindow`, on the live wire
    as its third value; `SeatStats::waited_no_room_in_window`, the web's
    `waited` its third entry).
- **The warning** by the receptors needs no change: it counts the attacks
  placed behind the nearest, which is now at most the limit less one (two
  by default, never past three bolts: the "×N" form only shows under No
  limit).
- **The room model's revision is 4** (`ROOM_MODEL_REVISION`): every
  device and the room must run the same ledger. A client of revision 3 is
  refused at a join and a ticket between stages ("Reload to update"). The
  rule is honored by an authority that says so (`RoomPolicy::at_once`,
  additive, true for every room this build makes); a room on an older
  authority offers no row, and a stage's rules naming another limit are
  refused there (`RulesProblem::AtOnceNotHonored`).
- **The battle plan names the limit** (`BattlePlan::at_once`, additive,
  in the digest only where set; `BattlePlan::rules()`, the rules the plan
  names with it): every device and the room run the stage under the
  plan's limit; a device of another limit disagrees on the digest and
  sits out of the attacks, as for any mismatch.
- **A stage in play across a server deploy** finishes under the rules its
  plan names: one begun under `attacks-v2` stays `attacks-v2` (no window,
  no limit; `a_stage_runs_under_the_attack_rules_its_plan_names`,
  `crates/room/tests/live.rs`, and the server's
  `a_stage_begun_under_attacks_v2_finishes_under_it_and_v3_keeps_its_limit`,
  through the Durable Object's hibernation). A restart mid-stage settles
  that stage on its reports, as before.

### Shift changes the policy (Part 2)

- **Shift**, either one by physical code, cycles the player's attack
  policy in a stage with attacks (`ddi_ui::controller_hints::POLICY_KEYS`,
  `ShiftLeft`, `ShiftRight`, `Tab`; `ddi_session::play::POLICY_KEYS`). Each
  key does unless the player's bindings give it to a lane: it then plays
  the lane and the others still cycle (a Dancing☆Onigiri layout with a
  Shift lane). **Tab still does.** A pad's Back pressed briefly and a tap
  on the label, as before. Each press is kept in the play's log at its
  judged second, so replays and the ledger are as before; a Shift press
  is never judged (never a stray press, never a Boo), and a lane pressed
  with Shift held is judged as without it (the browser and the OS give
  the lane's own physical code).
- **The stage's prompt** says how under the Targeting row: the Shift
  cap and "during play" (`room-targeting-during-play`, en and ja) as the
  screens' other key hints are drawn (`KeyHint`); for an active
  controller its Back's art from the controller atlas; nothing on a
  touch screen (the label is tapped); Tab where both Shifts are a lane's
  (`prompt_room::targeting_keys`).
- **In play**, beside the policy's word under the life gauge, a small
  quiet cap naming the key ("shift", or the pad's Back by its cap's name
  once a pad was pressed last; none after a tap or on a touch screen),
  outlined and dimmer than the word, where it fits before the bars'
  middle (`battle_cue::draw_policy`, `PlaySession::policy_key`).
  **Chosen: always shown.** Looked at in the offscreen picture and in
  Chrome and Firefox, it is calm (a 9 px legend in a hairline outline at
  70% of the word's muted color, nothing moving), so it stays for the
  whole stage rather than fading after the first seconds; nothing in it
  moves, so reduced motion changes nothing.
- **Known hazard**: Windows offers Sticky Keys after five quick presses
  of Shift (a system dialog). The game does not try to suppress it; a
  player who cycles the policy quickly on Windows may see it (Tab does
  not raise it).


### Measured

`attack_window` (`crates/session/src/battle/study.rs`, opt-in:
`cargo test --release -p ddi-session --lib attack_window -- --ignored
--nocapture`; `DDI_WINDOW_CONFIGS`, `DDI_CHAIN_SIZES`,
`DDI_WINDOW_DIFFICULTIES`): `chain_pileups`' six bundled charts, two
seeds each, fields of 4, 9, 33 and 73 CPUs of mixed levels, every CPU on
one policy, the player gone at the start, DDI Normal and Hard, under
`attacks-v2` and under `attacks-v3` with Attacks at once at 1, 2, 3 and
No limit (15 s), and the window alone at 10 and 20 s with No limit (the
study sets the rules itself: `attacks::RULES_OVERRIDE`). MEASUREMENTS OF
CPUS AGAINST CPUS (`MINE_RESPONSE_V1`). "Staleness": from a launch (its
checkpoint) to its first shock and to its last, median / 90th percentile
/ longest. "Held": Leader's recipient still the leader (most life among
the others still in) at the first shock; Retaliate's answer (where the
sender answered its last attacker) not overtaken by another sender's
attack landing on the sender before it.

**Leader, DDI Normal** (the policy that piles up; attacks a stage, most
placed against one at once, the most attacked one's share of attacks /
of its span filled, the top sender's different recipients, staleness to
the first shock, held):

| Field, rules | Attacks | At once | Top share / filled | Top sender hit | To first shock | Held |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 9, `attacks-v2` | 30.8 | 6 | 74% / 85% | 1.2 | 6.9 / 12.1 / 19.8 s | 92% |
| 9, v3 at once 1 | 27.8 | 1 | 30% / 32% | 2.6 | 6.1 / 6.4 / 6.6 s | 82% |
| 9, v3 at once 2 | 30.8 | 2 | 47% / 56% | 1.9 | 6.2 / 7.7 / 10.1 s | 90% |
| 9, v3 at once 3 | 31.6 | 3 | 62% / 73% | 1.5 | 6.3 / 8.1 / 12.1 s | 89% |
| 9, v3 no limit | 31.5 | 6 | 72% / 85% | 1.2 | 6.6 / 11.8 / 14.4 s | 92% |
| 33, `attacks-v2` | 84.8 | 32 | 33% / 97% | 3.0 | 21.9 / 51.0 / 91.7 s | 84% |
| 33, v3 at once 1 | 113.5 | 1 | 9% / 36% | 5.3 | 6.1 / 6.3 / 6.6 s | 87% |
| 33, v3 at once 2 | 117.7 | 2 | 16% / 67% | 4.2 | 6.2 / 9.1 / 10.2 s | 86% |
| 33, v3 at once 3 | 119.8 | 3 | 21% / 90% | 4.0 | 6.6 / 10.3 / 14.1 s | 83% |
| 33, v3 no limit | 121.0 | 6 | 23% / 98% | 3.7 | 12.1 / 15.1 / 15.3 s | 84% |
| 73, `attacks-v2` | 120.7 | 39 | 26% / 100% | 3.8 | 31.8 / 70.9 / 114.2 s | 85% |
| 73, v3 at once 3 | 267.2 | 3 | 10% / 96% | 4.5 | 6.9 / 10.8 / 14.1 s | 83% |
| 73, v3 no limit | 268.8 | 6 | 11% / 100% | 5.2 | 13.5 / 15.2 / 15.3 s | 82% |

Blocked launches (checkpoints a full bar waited, of those with one):
`attacks-v2` 85% (33) and 95% (73) under Leader, the leader's chart full
to the song's end and the bars not spent elsewhere; `attacks-v3` 18% to
22% at every limit, the song's tail (the same as Spread's); **"no room
within the window" never blocked a launch in these fields** (someone
always had room), so the window costs no attacks: in 33 and 73 Leader
launches more under `attacks-v3` (120 and 267 a stage against 85 and
121), spilled over to the next in standing.

**Spread and Retaliate** are the same under every rule to the noise
(Spread at most 2 at once, Retaliate at most 3, staleness 6.1 / 6.4 /
6.6 s to the first shock and 8.8 / 9.6 / 9.7 s to the last under the
limits; up to 11 s under `attacks-v2`), Retaliate's answer held 96% to
99% of the time. The limit of one cuts Retaliate's 90th percentile from
10 s to 6.4 s.

**DDI Hard** has the same shape, smaller (33, Leader: `attacks-v2` 23 at
once, 95% filled, 76% held, first shock 17.5 / 42.3 / 68.9 s; at once 3:
3, 76%, 81%, 6.9 / 10.6 / 14.1 s). **Life lost and knockouts by level**
(a seat) under the default, 33, Normal, Leader: level 10 received 20.9
attacks a seat under `attacks-v2` (61 life lost) and 21.6 under
`attacks-v3` at three (58 life): the spill-over adds attacks rather than
taking them from the strongest, and passes the rest to level 8 (0.3 → 7.9
a seat, 2 → 82 life) and a little to level 6 (0 → 0.5, knocked out by one
in 4% of its seats); levels 5 and below next to nothing, as before.
Under Spread and Retaliate the by-level lines move by noise. Draws by a final attack: none in any run. "Last
standing" came in 8% to 17% of the Hard stages of 4 and 9 (5 to 7.5 s
before the win), the same under every rule.

**Does Leader still bury anyone at the defaults?** Less, not never: with
every CPU of a large field on Leader and three at once, the leader has
three attacks placed against them almost continuously while they lead
(90% of the room over their attacked span in 33, 96% in 73), but never
more than three at a time and never more than about 14 s ahead; the
rest go to the second, the third, so the most attacked one's share falls
from a third to a fifth (33) and a tenth (73). At one at once the leader
gets at most one, 36% of their span filled. A whole field on Leader is
the extreme: CPUs are Spread unless a roster entry names a policy.

**The window, alone** (No limit): at 10 s the most at once against the
leader is 4 and staleness at most 10.2 s; at 15 s 6 and 15.3 s; at 20 s 8
and 20.0 s (33 and 73, Leader). It caps staleness at its own length, as
it should; it never blocked a launch here.

**How many attacks fit in the window** (`window_fit`, opt-in:
`DDI_WINDOW_CORPUS=<dirs> cargo test --release -p ddi-session --lib
window_fit -- --ignored --nocapture`): from every checkpoint of a chart, a
recipient with nothing placed against them reading 2 s ahead (lead
4.85 s), the attacks that would fit back to back starting within the
window; over the bundled dance-single charts and 4 000 charts of the
local arcade mixes, read in place (aggregates only):

| Charts, tempo | Checkpoints | None fit | 10 s: mean, p10 / median / p90 | 15 s | 20 s |
| --- | ---: | ---: | ---: | ---: | ---: |
| bundled, 120 to 160 BPM | 1 481 | 7.8% | 1.8, 1 / 2 / 2 | 3.0, 1 / 3 / 4 | 4.3, 1 / 5 / 5 |
| local, under 120 BPM | 12 326 | 13.5% | 1.0, 0 / 1 / 1 | 2.0, 0 / 2 / 3 | 2.8, 0 / 3 / 4 |
| local, 120 to 160 BPM | 118 054 | 10.8% | 1.7, 0 / 2 / 2 | 2.8, 0 / 3 / 4 | 4.0, 0 / 5 / 5 |
| local, over 160 BPM | 142 233 | 8.9% | 2.0, 1 / 2 / 3 | 3.8, 1 / 4 / 5 | 5.4, 1 / 6 / 7 |

"None fit" is the same at every window: those checkpoints have no safe
section left at all (the songs' tails and their densest stretches), so
no window of 10 s or more blocks a fresh recipient on these charts.

**The window recommended: keep 15 s.** The shortest that does not block
most launches would be 10 s (it blocks none), but at 10 s a song under
120 BPM fits one attack at the median and none a tenth of the time, so
Attacks at once of two or three would mean one on slow songs, and the
match rule would say something the window takes back. At 15 s the
default of three fits at the median from 120 BPM up (two under 120),
and the stalest attack lands 14 s after its launch at three at once
(15.3 s at No limit), against 92 s under `attacks-v2`; Leader's reason
still held 83% of the time at the first shock (84% under `attacks-v2`,
whose average attack waited far longer but whose leader also rarely
changed, being buried), Retaliate's 97%. At 20 s the window adds a fifth
attack's room and five seconds of staleness for nothing the rule needs.

### Shown

`target/scratch/mp/awin/shots/`: the Rules pop-up with its new row
focused and set to 2 (`at-once-1280x720-en.png`, `-ja`, and at
844×390; `room_rules_offscreen`'s `the_attacks_at_once_row_is_drawn`),
the row under Attacks with its sentence; the stage's prompt with "shift
during play" under the Targeting row and the play with "Retaliate" and
its small Shift cap under the life gauge after Shift was pressed
(`prompt.png`, `play.png`; `policy_key_offscreen`). Headless Chrome
(WebGPU) and Firefox (WebGL2, through the verify skill's helper), a
practice stage of three CPUs (`target/scratch/mp/awin/shift.mjs`): the
prompt's hint, then Shift (left), Shift (right) and Tab cycling Spread →
Retaliate → Leader → Spread, three changes in the play's log, nothing
judged (`shift-chrome-…`, `shift-firefox-…`). No console problems.

### Tested

- `ddi-room` `attacks/tests.rs`:
  `the_rules_are_named_and_v3_has_a_window_and_a_limit`,
  `v3_a_recipient_at_the_limit_is_skipped_and_leader_goes_to_the_next`
  (three against the leader, the fourth to the next in standing; one and
  No limit), `v3_the_window_holds_six_sections_and_the_seventh_goes_elsewhere`,
  `v3_no_room_within_the_window_waits_and_keeps_the_power`,
  `v3_retaliate_goes_by_spread_where_its_attacker_has_no_room` (and
  `attacks-v2` answering its last attacker regardless); every older
  rules' trace kept. `rules.rs`:
  `attacks_at_once_is_an_additive_match_rule_three_by_default`.
  `tests/live.rs`: the room's ledger through `FakeServer` under
  `attacks-v3` with the plan's three (never more than three standing,
  every section within the window), a stage under `attacks-v2` finishing
  under it, the limit in the plan's digest. `tests/authority.rs`: the
  revision is 4.
- `ddi-session`: `chained_attacks_are_delivered_back_to_back_into_plays_and_cpus`
  under `attacks-v3` (at most three at once, within the window);
  `shift_cycles_the_policy_tab_still_does_and_a_lane_s_shift_plays_the_lane`
  (both Shifts and Tab, the log replayed to the same record, a lane
  pressed with Shift held judged as without it, the same tally, a lane's
  Shift playing the lane, the label's key: Shift, the right Shift, the
  pad's Back, none after a tap). The online CPUs' proof
  `the_cpus_play_alike` passes natively and built for wasm32-wasip1; its
  fixture is unchanged (no stage of it placed more than three at once or
  past the window).
- `ddi-ui`: `the_attacks_at_once_row_shows_under_attacks_and_three_is_the_default`,
  `the_prompt_s_policy_row_says_how_to_change_it_during_play` (keyboard,
  controller, touch); the `qps-ploc` walk.
- `ddi-shell`: `the_policy_label_is_under_the_gauge_inside_the_field`
  (the cap after the word, inside the tap area, left of the bars'
  middle, at every size the miniatures' table uses, or left out).
- `ddi-desktop`: `the_attacks_at_once_row_is_drawn`,
  `shift_is_shown_on_the_prompt_and_beside_the_policy_and_cycles_it`.
- The server (`../ddi-server`): its tests pass against the changed crate;
  `a_stage_begun_under_attacks_v2_finishes_under_it_and_v3_keeps_its_limit`
  added, and its live tests' plans name three at once as a client's do.

## CPUs with absolute levels (`cpu-levels-v1`, 2026-11-13)

The maintainer, after the adaptive-charts round measured that a CPU's
strength is relative to whatever chart it plays (a level-4 CPU plays a
Challenge chart about as well as a Beginner one, so nearly every CPU from
level 4 climbed to the hardest chart): "rework the CPU model so that its
performance is 'absolute' and not relative to the chart being played,
that should give us much more realistic simulations."

### What is built

- **The model** (`ddi_engine::cpu::absolute`, `docs/plans/cpu-player.md`,
  "The model"): a CPU's abilities in real units, the same on every chart
  (a timing spread in milliseconds on a comfortable note, the notes a
  second it keeps up with, a floor of notes left out where nothing is
  hard; a repeated lane's and a sustained stretch's limits in fixed shares
  of the speed; the seven pattern axes multiplying the limit their pattern
  meets, Evasion as before); a row's demand read from the chart in real
  units (the notes in the second before it and the gap since the last
  row, the notes in the eight seconds before it, how soon a lane repeats,
  how irregular the rhythm is), never normalized by the chart and nothing
  fitted per chart; past its limits its spread widens and it leaves notes
  out with a chance that rises with how far the demand is above them. The
  shape of that fall-off was checked against one player's local play
  history, which is not included anywhere in the repository.
- **Ten levels** (`CPU_LEVELS_V1`, PROVISIONAL), rungs on that scale,
  shown as the same five stars, anchored by measurement over the local
  arcade mixes: level 1 at home on Beginner and lost on Medium, 3 on
  Easy, 5 on Medium, 7 on Hard, 9 on Challenge, 10 playing Challenge
  nearly perfectly (the tables in `docs/plans/cpu-player.md`).
- **Where it applies**: every multiplayer CPU (`Opponent::with_skills`
  and `new` play `CPU_LEVELS`, which is `cpu-levels-v1`): practice, the
  online rooms' CPUs computed on every device (`cpus::Drive`), the bots'
  CPUs, the audience's local computation, the studies. `cpu-levels-v0` is
  kept by name (`LevelScale::RelativeV0`, `Opponent::on_scale`) for
  comparison and its fixtures.
- **The solo CPU** keeps its five chart-relative strengths and its
  fixture exactly; `--cpu-level N` (desktop) and `level=N` (web), 1 to 10,
  play it at a level of the absolute scale instead, to watch one
  (`ddi_engine::cpu::solo_script`; the prompt and results say "Level 3 of
  10").
- **What follows it**: `MINE_RESPONSE_V1` unchanged (its input is the
  level, as before; measured again, sane on every chart); `CPU_SCORE_V0`
  unchanged; the roster unchanged (levels and personalities; each axis
  bends the limit its pattern meets, tested); the Rules' note about the
  stars says "the charts it keeps up with, not a skill rating" (en, ja).
- **`ROOM_MODEL_REVISION` is 5**: every device of a room must compute the
  same CPUs, so a client of revision 4 is told to reload. The server needs
  no code change (it reads the constant from the crate); a server built
  against this revision refuses older clients between stages, and a stage
  in play across the deploy finishes on the CPUs its devices loaded.

### Measured

The levels by difficulty slot, meter and notes a second, under DDI
Easy, Normal and Hard, v0 beside v1, the stream packs and the bundled
songs, the spread across seeds, a level watched and the cost: in
`docs/plans/cpu-player.md`, "Measured (`cpu-levels-v1`)". In short
(arcade mixes, DDI Normal, percent and out %):

| Level | Beginner | Easy | Medium | Hard | Challenge |
| --- | --- | --- | --- | --- | --- |
| 1 | 86.9, 1.5 | 72.6, 21 | 40.6, 78 | 11.1, 98 | -3.4, 99 |
| 3 | 93.7, 0.1 | 90.3, 3.2 | 73.3, 30 | 43.8, 78 | 19.2, 94 |
| 5 | 96.3, 0 | 95.7, 0.4 | 89.4, 7.0 | 70.0, 40 | 45.3, 73 |
| 7 | 97.5, 0 | 97.6, 0 | 96.6, 0.4 | 89.4, 14 | 74.5, 45 |
| 9 | 98.5, 0 | 98.6, 0 | 98.5, 0 | 96.6, 1.8 | 90.4, 18 |
| 10 | 99.1, 0 | 99.1, 0 | 99.1, 0 | 98.7, 0.2 | 96.4, 2.2 |

(`cpu-levels-v0`: level 1 66.9% on Beginner and 51.7% on Challenge, level
5 85.6 and 74.3, never out.)

**Under Adaptive charts** (`adaptive_study`, the same mixes, 2,080 Medium
and 2,078 Hard picks, ten CPUs one a level, attacks on, two seeds; where
each finished, from the pick; MEASUREMENTS OF CPUS AGAINST CPUS):

| Picked Medium, DDI Normal | −2 / −1 / 0 / +1 / +2 % | ups | downs | bounce % | out % adaptive (as picked) |
| --- | --- | ---: | ---: | ---: | --- |
| Level 1 | 31.5 / 52.6 / 14.3 / 1.5 / 0.1 | 0.16 | 1.30 | 3.9 | 6.7 (89.3) |
| Level 3 | 3.2 / 27.6 / 46.6 / 21.8 / 0.8 | 0.75 | 0.86 | 7.5 | 2.3 (39.7) |
| Level 5 | 0.3 / 4.8 / 33.9 / 55.1 / 5.9 | 1.22 | 0.61 | 6.6 | 2.2 (11.4) |
| Level 7 | 0 / 0.2 / 8.7 / 74.2 / 17.0 | 1.35 | 0.27 | 3.7 | 1.5 (1.0) |
| Level 10 | 0 / 0 / 0.1 / 71.6 / 28.3 | 1.29 | 0 | 0 | 0 (0) |

(+1 is Hard, the top rung of 70% of the songs; +2 Challenge.) Under
`cpu-levels-v0` levels 4 to 10 all finished at +1 or +2 74% to 99.9% of
the time and level 5 at Medium 3.5%; now level 5 stays at Medium a third
of the time, level 4 45%, level 3 47% and level 1 drops to Easy or
Beginner 84%. From a Hard pick (Normal): level 1 finishes on Easy or
Beginner 85% of the time, level 3 on Medium 49% and Hard 20%, level 5 on
Hard 55%, levels 8 to 10 on Hard 72 to 76% and Challenge the rest. Under
Hard timing the spread is wider (from Medium: level 1 77.5% two rungs
down, level 5 41.5% at Medium). The stage's order by level holds better
than ever: a stronger level placed above a weaker one in 98.7% of pairs
under Adaptive (97.5% Hard timing; 99.1% and 97.8% from a Hard pick),
94.4% as picked. Bouncing (a rung left and come back to) rose with the
rungs a level now meets above its head: 4% to 11% for levels 1 to 6
under Normal timing (v0: under 0.5%), up to 21% under Hard timing, the
eight bars back still bounding it.
Playing down on purpose is unchanged (it reads charts, not CPUs).

**Attacks**, the studies again (`attack_rates`, `chain_pileups`,
`attack_window`; the same six bundled charts, mixed fields): weak levels
now earn less power on the harder of those charts and go out of more
stages, so a mixed field of eight launches fewer attacks (DDI Normal
24.0 a stage against 25.7 under v0, Easy 31.0 against 36.0, Hard 16.0
against 15.2). Eight CPUs of one level: level 1 launched 0.54 a seat
(0.87) and went out of 71% of its stages (65%), level 5 2.95 (3.59) and
37% (12.5%), level 10 7.16 (6.30) and never out; an attack received cost
34.2 / 25.3 / 22.1 / 18.8 / 2.6 life at levels 1 / 3 / 5 / 7 / 10 (34.0
/ 32.0 / 28.9 / 22.4 / 3.3). Under `attacks-v3` at three at once, Leader,
DDI Normal: fields of 9 launch 24.9 attacks a stage (31.6), the top
recipient taking 58% of them (62%), the first shock 6.3 / 8.9 / 13.8 s
after its launch (6.3 / 8.1 / 12.1), the reason held 90% (89%); of 33,
104 (120), 22% / 81% of its span (21% / 90%), 7.2 / 10.8 / 14.1 s, held
85% (83%). The window still never blocked a launch. Evasion's tables in
`docs/plans/cpu-player.md`, "Evasion", measured again: the shocks stepped
on as before, more weak levels knocked out by them on busy charts.

**Cost**: a CPU's presses at an absolute level cost about what a frozen
strength's did (57 µs a bundled chart natively against 50, 79 against 65
in wasm) and less than a weak v0 level's (90 against 233); 72 CPUs a
stage loaded in 115 ms with a full ladder (114 under v0) and played out
in 198 ms (179); a frame of 72 online CPUs 0.013 ms mean.

### Tested

- Engine: the demand in real units, the failure past the limits and not
  before, bounds on extreme rows, the law a probability; monotone in
  level on every bundled single chart and in the chart's slot for a
  level (stated tolerances); each pattern axis bending its limit; the
  solo CPU's `level=`; the five strengths' press hashes and Evasion 1's
  fixture unchanged (`cpu-levels-v0`); the opponents' tests (frame rates,
  shocks, steps down and up) on `cpu-levels-v1`.
- Session: `the_cpus_play_alike` passes natively and for wasm32-wasip1
  with its fixture recorded again; one test's CPUs moved from level 2 to
  3 (`a_watched_stage_ends_when_the_winner_finishes_and_a_watcher_leaves_freely`:
  at level 2 its three CPUs went out together, a draw, where the test
  needs a winner who plays on).
- Room: the revision's test (5). UI: the Rules' note; the solo prompt's
  line; the web route `level=`; desktop's `--cpu-level`.
- Desktop offscreen (`step_down_offscreen.rs`,
  `absolute_levels_end_a_stage_on_different_charts`): a practice stage on
  Some Things Must's Medium under Adaptive charts, the player autoplayed,
  CPUs of levels 2, 5 and 9: they end on Beginner, Easy and Challenge
  (the Standings say "↓ Beginner", "↓ Easy", "↑ Challenge"); pictures
  `levels-on-their-rungs` (the three fields on three charts) and
  `levels-standings` in `target/scratch/mp/cpuabs/shots/`.

### The Standings: names keep their room

In a stage's Standings the mark of the chart a competitor finished on
("↑ Challenge") goes on the line under the name, and where a CPU's name
and its stars do not fit side by side (a phone's) the stars follow the
mark there, so names are never cut for them (`results_stage.rs`;
`a_stepped_mark_never_cuts_the_name`; drawn offscreen at 1280×720 and
844×390 with nine starters, `crates/desktop/tests/standings_offscreen.rs`,
pictures in `target/scratch/mp/cpuabs/shots/`).

### Not done in this round

- CPUs still play a tap-only copy (no holds, mines or lifts of the
  chart's own); no new personalities; the stars unchanged.
- The levels' anchoring is provisional: level 3 scores 93.7% on Beginner
  where "a slot below" asks 95%, and a level at home goes out on the
  hardest charts of its slot more as the slots get denser (7% of Medium
  charts at level 5, 18% of Challenge at level 9).

## Provisional choices

| Choice | Where | User-visible effect | Decision needed |
| --- | --- | --- | --- |
| `online-cpus-v0` (2026-11-13): an online room's CPUs computed alike on every device (50 ms steps from −2 s, a shock put in 1.25 s after its checkpoint, a 2 s horizon's lead); every connected player's device reports for them, the room taking the first and counting a different one; nobody left to compute ends them as forfeits | `ddi_room::live::cpus`, `ddi_session::battle::cpus` | The host adds CPUs online as in practice; a wrong report that reached the room first stands | First or majority; the bytes (every device sends every CPU); the horizon |
| Rules from the next stage (2026-11-13): a change lands on the next stage nobody has readied, loaded or started (once picked, the one after); Points changeable from a stage on too (the maintainer, answered); Apply and restart takes the per-stage rows as shown | `RoomState::rules_land_on`, `room_dialogs::RulesDialog` | A host's change of timing, Boos, attacks, zero life or (since the follow-up) Points applies from the stage the pop-up names, said on everyone's status line | Whether a change should land on a picked stage nobody has readied yet |
| `adaptive-v0` (2026-11-13, replaces `step-down-v0`): down at zero life with a rung below, life back to half, the old chart taken away, the boundary the first bar line (four beats) at least the reading horizon plus 1 s ahead, a breather until it (nothing of a chart judged, stray presses free, life kept at one point or more); up after four bars (16 beats) of full life with no miss, no hold let go and no shock stepped on, counted from the last step's boundary, at the next such boundary, the old chart played to it, life unchanged; eight bars to go back up to a rung whose life ran out; every rung's score factor 1 | `ddi_engine::ladder::ADAPTIVE_V0` | A player out of life plays on an easier chart after a few seconds' rest; one who plays cleanly moves up, above the picked chart too | The four bars; the eight; the life given back; whether a player may hold their rung; a per-rung score factor once the rule is felt |
| The ladder: the style's original charts on the same timing, lower slots below (nearest first, by meter from the highest), higher slots above (nearest first, by meter from the lowest); never an Edit above or below a slotted chart (an Edit's rungs are the slotted charts of a lower or higher meter); each way cut at the first rung the CPUs cannot plan | `ddi_engine::ladder::{below, above, Ladder}`, `cpu::planned_ladder` | From Hard: Medium, Easy, Beginner below; Challenge above | How Edits and split-timing charts should take part |
| Standing under Adaptive: those still in by the score as earned (each stretch against what it showed, the absolute score growing with the notes played), life only breaking a tie, steps never counted; a CPU's score put on the charts it played stretch by stretch (`CPU_SCORE_V0`'s way: each chart's whole most in the share of its tap-only copy's notes it was shown), never on the stage chart's scale | `ddi_room::RankBy::Score`, `battle::rank`, `ddi_session::battle::Scales` | A player who climbs to denser charts earns more; one who stepped down is not ranked below for it | Life as the tie-break; whether a balancing factor is wanted after playing it |
| The battle plan placed over every rung at once, below and above | `ddi_engine::ladder::merged`, `ddi_ui::state::battle_plan::plan` | See "Adaptive charts", measured | Whether to place sections per rung instead (the room then needs each competitor's rung) |
| A step down on the live wire as a flagged refusal (`STEP_DOWN_ATTACK`) | `ddi_room::live::wire` | Older readers ignore it | Whether to version the wire instead when it next changes |
| `audience-view-v0` (2026-11-13): an audience device plays the music and draws the fields 0.8 s behind the room's song time (fixed for a stage), the fields a little more where frames come later, within the audience's 2 s ceiling | `ddi_ui::state::audience::MUSIC_DELAY_V0` | A seat sees the stage about a second behind the players | The delay; whether to raise it after a stage that lagged |
| `audience-view-v1` (2026-11-13, follows v0): the music's delay moved only at a stage's start, up by what the last stage's fields needed beyond it (their 90th percentile past 100 ms, rounded up to 50 ms, at most 1.6 s), down 0.2 s a stage to 0.8 s after good ones | `ddi_ui::state::audience::AUDIENCE_VIEW_V1` | After a laggy stage the next one is watched further behind, its fields in step with the sound; nothing heard changes within a stage | The numbers; whether a stage's own lag should be fixed within it |
| The count shown to members between stages only (song select's status line, Host options, the invite and audience pages; since 2026-11-13 not a stage's prompt nor while a stage is played) | `screens::room::status` | "· 132 watching" after the room's line | Answered (the maintainer, 2026-11-13): between stages only |
| `join-watch-v0`: a join asking to play where every player's place is taken is in to watch; nobody is moved to Play when a place frees up; the arrival is said on the status line for 12 s | `ddi_room::RoomState::arrival_intent`, `players_full`; `ddi_ui`'s `LinkNotice::JoinedToWatch`, `JOINED_TO_WATCH_MS` | A tenth person joins a room of nine players to watch, and plays only once they choose Play after a place frees | Whether a freed place should go to the longest-waiting watcher (asked, or at once) |
| The watch-only link: held to watching until an operator's Let play (co-hosts too since round B1, ANSWERED); replaced with the room's invite; its Copy hidden behind `Settings::multiplayer_watch_link` | `ddi_room::Member::watch_only`, `Command::SetWatchOnly`, `Capability::LetPlay`; `../ddi-server` `invite::watch_secret` | Someone who came to watch never takes a player's place unless the host or a co-host lets them | Whether the watch link should be replaceable on its own |
| `watch-v0`: a grid's field is at most a play's own field (`GRID_LANE_SHARE` 1.0); a layout with more rows wins within a tenth of the largest lane (`GRID_FILL` 0.9) | `ddi_shell::opponents::arrange_grid` | One to three watched fields are nearly the screen's height; eight are four by two at lanes of 29 px (1280×720) | The sizes; whether eight should give up the play's proportion (shorter fields, larger lanes) |
| `slew-v0` (2026-11-13): a live play's stream and views on the room's song clock, its correction averaged over 4 s and moved at most 2 ms a second; its judging on its own | `ddi_room::live::slew::SLEW_V0`, `ddi_session::battle::live::Live::sync` | None visible: remote fields keep their delay however the devices' clocks drift | The rate; whether a large disagreement should be told to the room |
| `watch-v1` (2026-11-13; `watch-v0` ended 2.5 s after the decision): a watched stage goes on after its decision while anyone still plays its song, its results 2.5 s after the last one ended (`WATCH_END_HOLD`) or at the song's end | `ddi_session::play::watch`, `Battle::still_playing` | A watcher (and a player out of the stage) sees the winner finish the song, "X wins the stage" on the status line from the decision on; Enter or Start still skips | Whether the hold after the winner finishes should be shorter |
| `cpu-levels-v1` (2026-11-13): a CPU's level is abilities in real units, the same on every chart (spread 17 to 5.5 ms, speed 2.2 to 9 notes a second, a floor of 0.4% to 0.02%); `cpu-model-v1`'s shape (`ABSOLUTE_V1`: onset at 0.85 of a limit, the spread three times wider per unit past it, the lapse 0.067x + 0.1x², a lane's limit 0.65 of the speed, sustained 0.75, a 12% tail 2.5 times as wide) | `ddi_engine::cpu::{CPU_LEVELS_V1, ABSOLUTE_V1}` | Level 1 at home on Beginner, 3 on Easy, 5 on Medium, 7 on Hard, 9 on Challenge; under Adaptive the levels settle on different rungs | What each level should be at home on; whether "at home" may cost a CPU the stage on the slot's hardest charts (7% to 18%) |
| `watch-v0`: where every person watches, a practice stage starts with the pick (the room's countdown), with no press of the watcher's | `ddi_room` `humans_needed`, `maybe_commit` | A watching host's pick is followed by the stage three seconds later | Whether a watching host should start it with a press |
| `watch-v0`: the status line's moments for a watcher: out, attacks, the win; an attack never takes the line from someone going out | `ddi_session::play::watch::Watch::tell` | With eight CPUs attacking the line changes every few seconds | Which moments a watcher is told, and whether as a line of the announcer's |
| A forfeit in play: the stage is watched on; the player's results are graded QUIT, their line last with no points | `Battle::forfeit_now`, `PlaySession::tick` | Escape twice in a stage shows "Left the stage" over the field and the stage goes on | Whether leaving should ask first, or be undoable for a moment |
| CPU practice with a match running is forgotten when left, silently | `screens::room::leave_now`, the web's `forget_left_practice` | Back from a running practice match, then CPU practice again, is a new room with the default rules | Answered (2026-11-13): its roster survives, kept in the settings; the other rules do not |
| A room from before the picker (no `picking` in its state) keeps every operator choosing and offers no take, for as long as it lives | `ddi_room::RoomState::chooses`, `may_take_pick` | A room open across the server's deploy never gets Pick songs; a new one does | Whether such a room should turn into a one-picker room at the deploy |
| The pick returns to the owner when its holder is Disconnected: the server's reconnect grace (30 s); a pick nobody holds is the owner's whenever the owner is there | `reduce.rs` `release_pick`, `settle_host`; `../ddi-server` `ROOM_RECONNECT_GRACE_MS` | A picker whose connection is down for more than 30 s finds the owner choosing on return; nobody chooses only while the owner is away with nobody connected | The grace for a picker |
| A host away is given the room's reconnect grace, 30 s (`Connection::Disconnected`), then the room passes on or closes by its setting | `reduce.rs` `settle_host`; `../ddi-server` `ROOM_RECONNECT_GRACE_MS` | A host whose connection is down for more than 30 s comes back a member (since round B1; one press by the new host hands it back), or to a closed room under Close | Whether a host should be waited for longer than a picker (a minute, say), above all under Close |
| A host away with nobody connected keeps the room (Pass it on); a host who leaves with nobody to pass it to ends it | `reduce.rs` `settle_host` | A host alone whose connection drops finds their room as it was; the first other person to connect meanwhile becomes its host | Whether an absent host's empty room should close after a while sooner than the server's idle end (7 days) |
| `succession-v1` (round B1, ANSWERED by the maintainer): a connected co-host, else a co-host inside the grace (waited for), else the first-joined connected member; never a bot or an observing administrator; the host gone a member | `ddi_room::RoomState::succession`, `observes` | A co-host reconnecting is waited for before a member is made host | None |
| A room from before the setting passes on from its first change after the deploy | `reduce.rs` `apply` | A room open across the server's deploy gets the succession without being made again | None, unless such rooms should stay as they were |
| A quitting game waits 0.6 s for the room's answer to its leaving | `screens::room::QUIT_WAIT` | Leave from the window's close takes up to 0.6 s to close the window | The time; whether to wait at all |
| Asked again while the question is open, desktop quits without it | `Shell::close_requested` | Closing the window twice always closes it; the room then sees a dropped connection, not a leaving | Whether the second request should still tell the room first |
| A resting song's share is kept across changes of hands, the newest eight | `ddi_room::Picking::KEPT_SHARES` | A song rested on again after taking the pick back is shown to the others without a second upload | None unless rooms change hands over many songs |
| The joining line shows after 0.4 s of joining (`JOINING_AFTER`) | `select_room.rs` | A room that answers quickly shows the bars alone, then its view | The delay |
| A report's detail (`RunDetail`) is self-reported and shown as sent, bounded only in size | `crates/room/src/battle.rs`, `reduce.rs` | An online player's grade, combo and judgments on the standings are what their device said | Whether the room should check a detail against the run, or wait for server-judged plays |
| The stage's standings seen on its results count as seen: no pop-up back on song select | `results_stage.rs` `stage_update` | One screen fewer after each stage for those who saw the standings | Keep, or always pop the standings up |
| The results' tab is remembered for the session, the last stage always opening on the standings | `results_stage.rs` `CHOSEN` | A player who prefers the graphs sees them after middle stages | Whether it should be a saved setting, or always the standings |
| Bots: 12 h sessions, never re-signed; 200 sessions an hour per admin token; 73 bots in a room (60 and 8 before 2026-11-13) | `../ddi-server` `bots.rs` | A long bot run stops after 12 h (signed out); a 74th bot is refused | The limits for bots on the deployed server |
| Bot tool pacing: Ready after 0.7–2.5 s, host rests 8 s on a song, 15 s after each stage's results, waits for 2 players | `crates/desktop/src/bots.rs` | How quickly a room of bots moves | None (a test tool), unless it should mimic people more closely |
| Under-eight awards: `EightPlayerScaledV1`, the eight-player table read up the field (practice's four: 12, 6, 3, 1); `ExperimentalLinearV0` retired, still read (ANSWERED by the maintainer, 2026-11-13) | `awards::AwardRule` | Small rooms earn on the eight-player scale, unmarked | None |
| Award field = the stage's committed starters; sit-outs are not counted | `awards::AwardScheme::rule_for` | A stage with seven starters out of eight seats uses the small-field rule | The population basis for points with sit-outs |
| A confirmed forfeit earns zero; its place's award goes unused | `awards::StageResult::award` | A disconnect forfeit adds nothing, earlier points stay | Forfeit scoring (the connection-loss study proposes zero) |
| Readiness policy `hosted-deadline-v0`: 15 s window, 3 s countdown, at least one ready human and two competitors | `rules::ReadinessPolicy` | Start opens a 15 s window when someone is unready | Window and countdown values; minimum population |
| The window waits only for those who could still become eligible (not ready, loading, load failed, disconnected); departed and watching competitors sit out without a wait | `SitOutReason::may_still_join` | Start is immediate when only leavers are missing | Whether a load failure or a disconnect should hold a window |
| A committed starter leaving or disconnecting before the start cancels the countdown | `reduce.rs` | Everyone returns to preparation; the host starts again | Roster-invalidating failures; repeated cancellations |
| More than 73 starters have no award rule (the top-biased tables stop at the cap) | `AwardScheme::rule_for`, `TopBiased::MAX_FIELD` | Cannot happen under the cap of 73 | A table before raising the cap again |
| `member-bound-v0`: 384 members besides administrators (a full field and about 300 watching) | `RoomPolicy::DEFAULT_MAX_MEMBERS`; the server's `ROOM_MAX_MEMBERS` (raised to the competitor cap where set below it) | The 385th person is refused as the room being full | The bound, and the audience beyond it (counted, not listed) |
| `rivals-v0`: past eight opponents, eight slots; a slot's opponent stays 8 s at least, keeps it within two places and a twelfth of the field of the nearest rivals; attackers in at once; a 0.3 s fade-in | `ddi_shell::rivals::RIVALS_V0` | With 72 CPUs the eight beside the field are the player's attackers and nearest rivals, changing every eight seconds or more | The hold and slack; whether the leader should always have a slot |
| `watch-pages-v0`: a spectator's grid shows twelve fields a page by standing; Page Up/Down, the D-pad's Up/Down and a tap on the ladder turn the pages | `ddi_session::play::GRID_PAGE`, `watch.rs` | A watcher of 73 sees the top twelve first, then pages | The page's size; whether pages should turn by themselves |
| A large room's lists (past nine rows) put the local player first, then those with a role, then people, then CPUs; Host options its co-hosts and administrators first | `screens::room::player_rows`, `panel_order`, `HostDialog::people` | Among 73 the player and the operators are found at once | A search field for the Players pop-up |
| A stage's Standings keep the player's line in view (the window's first or last line); the match's result names one line of leaders and the player's own place | `screens::room::window_keeping`, `results_stage.rs` | The player's place among 73 always shows | None |
| CPU seeds per seat derived from the match's profile seed, and per stage from the stage's number (`cpu-stage-seed-v1`, round B1, ANSWERED) | `member::OpponentSpec::for_seat`, `stage_seed` | Each CPU of a match plays its own seed, another each stage | Per-CPU profiles, personalities |
| Present people who want to play hold a seat at the match start whatever their connection | `reduce.rs`, `start_match` | A disconnected person sits out stages rather than losing the seat | Roster lock for absent people |
| Persistent elimination ranks by points like the default | `state::Match::standings` | Under that option the standings are not yet survival order | Its ranking (the design says survival placement) |
| Admins may exceed the member limit by `admin_reserve` (4) | `rules::RoomPolicy` | Admins can always enter to help | Operator-observer capacity |
| Protocol version 2, accepting 1 (`OLDEST_PROTOCOL_VERSION`); beside it the room model's revision (`ROOM_MODEL_REVISION` 1, round B1): an older client refused at a join and at a ticket between stages ("reload to update"), a newer one soft | `protocol.rs`, `RoomState::admits_model` | An old page in a room on a newer server is told to reload | Whether a socket open across a revision change should be told too |
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
| CPU shock response `mine-response-v1` (the DDI ruleset's round 4, replacing `mine-response-v0`, which stays as its levels' table): each level's response as `mine-response-v0` had it (avoidance from level: 1: 0, then a fifteenth more each to 5; 6–7: 1/3, 8–9: 2/3, 10: 1; at 0 a CPU steps on 30% of shocks, its presses within 0.3 s of one moved ±65 ms and 8% left out; at 1/3 14%, ±45 ms, 4%; at 2/3 6%, ±25 ms, 1.5%; at 1 2%, ±12 ms, 0.5%), bent by its `Skills`' eighth axis, Evasion (0.5 to 2, 1 the level's response exactly): the chances times `Evasion^-γ`, the moves times `Evasion^-γ/2`, `γ = 1 + (busy + beside) / 2`, busy from 2 to 8 notes within 0.5 s of the shock (all lanes), beside a note of its lane within 0.5 s. Presets: technical 1.25, chords 0.8, evasive 1.6, reckless 0.6, the rest 1; the roster's Brisk Otter and Tessel Moth reckless, Velvet Metronome and Saltglass Rook evasive | `ddi_engine::cpu::MINE_RESPONSE_V1`, `Skills::evasion` | How often a CPU's life drops to an attack, and how much its timing suffers near one; dodgers and stompers in practice (measured: `docs/plans/cpu-player.md`, "Evasion") | Calibration against people; the presets' and roster's values; whether γ's shares suit |
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
| Life, combo and moments drawn after a presentation delay, 0 for local CPUs; online each device's measured delay (`view-delay-v0`) | `feed::LOCAL_DELAY`, `FieldFeed::set_delay`, `ddi_room::live::delay` | None locally; online about half a second at typical latencies | The online delay's numbers |
| A miniature's combo never below 0.3 of the play's scale | `hud::MINI_COMBO_MIN_SCALE` | Below 22 px lanes the combo is larger than its field's scale | Legibility against faithfulness at small sizes |
| Nine players: `ExperimentalNineV0`, the eight-player table and 0 for ninth: RETIRED 2026-11-13 (the top-biased tables, decided by the maintainer) | `ddi_room::awards` | None now; still read from an authority one build behind | None |
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
| Attacks are a match rule, On by default where they are offered (CPU practice: `gauge-attacks-v0`; Off by default until M8 round 3; a private online room that plays live since 2026-11-13, `MatchRules::hosted_for`); an online room that does not play live shows them as not there yet and plays none whatever its rules say | `ddi_room::MatchRules::practice`, `hosted_for`, `attacks`, `ddi_ui`'s `RoomClient::attacks_played`, `screens::room::attacks_offered`; conveniences `mpattacks=0` (Off), `--mp-attacks` and `mpattacks=1` (redundant) | A practice match and a new live room's match have attacks unless the host turns them off in Rules | Whether a public room (later) should too |
| A shock is a mine under DDI: −5 life on Normal (−1 Easy, −10 Hard), −10 points, the combo, the floors, the lamp, and its lane locked a second; `injected-mine-v0` (the ruleset's miss to the gauge alone) only under a ruleset without the lock, which no room plays | `ddi_engine::Player::shock_is_mine`, `ddi_engine::battle::INJECTED_MINE_V0` | A shock stepped on costs what a chart's mine costs and a second of its lane | The lock's second and the mine's cost (DDI's *tune* values) |
| `placement-v1`: eight shocks over eight beats from a checkpoint, the eighths then the sixteenths, at most two a beat, one a row; margins about each step of the lane: none within 0.25 s after it nor within W3 before it, none from a hold's or roll's head to 0.25 s past its end, none from 1 s before a lift (`placement-v0`'s clearances by windows kept for comparison) | `ddi_engine::battle::PLACEMENT`, `Clearance::Margins` | Where the shocks fall: a shock may sit a sixteenth before a note of its lane from 150 BPM on, where a press more than half of W3 early steps on it | The 0.25 s; whether "before" should be W3 plus the mine window; whether sixteenths are acceptable |
| Development trigger: the first safe section from a beat in each practice stage, its seed the stage's id | `--mp-dev-mines`, `mpdevmines=`, `ddi_session::battle::dev_shocks` | Only through the switch | Replaced by the attack loop (S4) |
| A miniature's score: small (0.8 of its name line, 8 to 11 px; down to 6 px where the sum is wider than its field) under its life gauge's right end; the score (the sum) or the percent, by the player's Score display | `ddi_shell::opponents::ScoreShown`, `SCORE_SIZE`, `SCORE_FLOOR`, `SCORE_TOP` | At lanes under about 17 px the number's top touches the receptors' row | Its place and size |
| `attacks-v1`: no recovery period; one more attack may be reserved behind the one on its way or passing, in the next section after it clears | `ddi_room::attacks::ATTACKS_V1`, `AttackRules::chain` | Two attacks can arrive back to back; nobody is safe for a while after one | Answered (2026-11-13): an attack is over once placed and attacks chain without limit, `attacks-v2` below; kept by name for stages recorded under it |
| `attacks-v2`: an attack over for the ledger once placed; attacks against one recipient chain without limit, each in the first safe section at or after the last one's clear, searched to the song's end; Spread prefers the fewest still to come; no gap, no cooldown (`chain_gap`, `cooldown_checkpoints`, both 0) | `ddi_room::attacks::ATTACKS_V2`, `ATTACKS`, `CHAIN_UNLIMITED` | Anyone still in can be attacked at any time; several attacks can arrive one section after another; the warning counts those behind the nearest | Answered (2026-11-13): `attacks-v3`'s window and the match rule Attacks at once (three by default); kept by name for stages recorded under it |
| `attacks-v3`: `attacks-v2` with a window and a limit; a sender's policy orders every valid recipient and the launch goes to the first with room (Leader to the next in standing, Retaliate to Spread's order) | `ddi_room::attacks::ATTACKS_V3`, `ATTACKS`, `StageBattle::launch` | A buried leader's attacks go to the second; a full bar waits only where nobody has room | Whether Leader should rather wait for its leader than spill over |
| `attack-window-v0`: an attack's section starts at most 15 s of song time after its launch's checkpoint (the recipient's lead inside it); not a match rule | `ddi_room::attacks::ATTACK_WINDOW_V0_US`, `AttackRules::window_us` | No attack lands more than about 15 s after it was launched; about 10 s left after a 2 s horizon's lead | The length (measured: 10 s fits one attack on slow songs, 20 s adds staleness for nothing) |
| Attacks at once: 1, 2, 3 (default), No limit, per stage, shown only with Attacks On | `ddi_room::AttacksAtOnce`, `MatchRules::at_once`, `AttackRules::with_at_once`, `BattlePlan::at_once` | At most that many attacks stand placed against one player | The default of three |
| Shift (either, by physical code) and Tab cycle the policy in play, each unless a lane has it; a small quiet cap after the policy's word, always shown; Windows' Sticky Keys dialog after five quick Shifts not suppressed | `ddi_ui::controller_hints::POLICY_KEYS`, `ddi_session::play::PolicyKey`, `ddi_shell::battle_cue::draw_policy` | The key is on the ready screen and beside the label | Whether the cap should fade after the first seconds |
| `settlement-v1`: `settlement-v0`'s decision, the one left while attacks against them are still to come told as last standing | `ddi_room::attacks::SETTLEMENT_V1`, `Settlement::last_standing` | "Last standing", then "Stage won" in the status line; "X is last standing" to those watching | As decided (2026-11-13) |
| `placement-v2`: six of an attack's eight shocks next to a note of their lane (0.25 s to 0.5 s after one, W3 to twice W3 before a step), a second between a lane's shocks | `ddi_engine::battle::PLACEMENT_V2`, `Near` | An attack is harder to ignore: about a third more life lost by the CPUs an attack, mostly through locks | The window's sizes, the aim (six of eight), whether W3 before a step is too close for early players |
| A CPU's (and a bot's) score put on the chart's own scale: its points as the same share of the chart's most, its W0s as counted | `ddi_engine::rules::ddi::CPU_SCORE_V0`, `DdiScoreView::on_scale`, `ddi_session::battle::cpu_score` | A CPU's number beside the player's reads as if it had held the holds as well as it tapped; a CPU can tie a person | Whether imputing the holds is right, or CPUs should play (and be able to drop) holds |
| A miniature's locked lane: the lane darkened (0.42) to its end and its receptor's place tinted violet (0.5) for the lock's second, from the feed's moment | `ddi_shell::opponents::LOCK_LANE`, `LOCK_TINT`, `FieldFeed::locks` | A CPU that stepped on a shock shows a dead lane for a second | The look; a shock's own art |
| Personal targeting: a member's own policy (Spread until chosen), set outside a stage's play, clearing nobody's readiness; in play Tab, a tap on the label, or a short press of a pad's View (`policy-pad-v0`: held it quits, pressed twice it does not where the policy cycles) | `ddi_room::Member::targeting`, `Command::SetTargeting`, `ddi_session::play` (`POLICY_KEY`, `policy_pad`) | A controller player in a stage with attacks quits by holding View, not by pressing it twice | The controller's control for it (View is the quit); whether the policy should be remembered across rooms |
| Eight of the roster's CPUs name a policy (five Retaliate, three Leader), the rest Spread | `crates/ui/src/state/cpu_roster.json` | Some CPUs strike back or go for the leader | Which CPUs, if any; whether a CPU's policy should follow its level |
| `attack-links-v0`: a pulse of 0.6 s, the line gone after 1.5 s, the outline in over 0.2 s, a chained attack's parts at 0.45; straight legs kept 1.5 px from everything; an incoming line from the left ends at the field's left edge | `ddi_shell::attack_links::ATTACK_LINKS_V0`, `route` | How who attacks whom reads; the line is thin and brief | The look (a free curve, a shock motif for the pulse), its times, whether the warning should mirror to the attacker's side |
| Beyond the cap, the attackers and the latest target are kept in view while their attacks last and one turn (8 s) after, in the first slots | `ddi_session::battle::Battle::kept_seats`, `opponents::priority_seats` | Development-only practice | The large-party policy |
| An attack against the player is told at most once in six seconds; attack moments rank 30 (incoming) and 8 (launched) in `battle-voice-v0`; shown 2.5 s in the status line | `ddi_session::announcer::battle::BATTLE_VOICE_V0` (`incoming_again`, `attack_priority`) | "Attack launched" shows several times a minute for a strong player | Whether the launched moment should show at all once it has a line; the six seconds |
| The standings' attack columns take the combo's place; the summary flows under the lines | `screens::results_stage` (`columns`, `summary_lines`) | With attacks the stage's standings have no max combo | Which column should give way (the combo, the place in the match, the judgments' strip) |

## Open questions

- Watching (2026-11-13): a watching member's music is delayed to match
  the fields (`watch-music-v0`). The maintainer wants to explore later
  whether delayed judgments could be drawn over on-time music and notes
  instead (the others' hits shown late over a field that scrolls on
  time), which would need no music delay at all.

- Rooms of 1+72: fields under eight. ANSWERED (the maintainer,
  2026-11-13): "the 12, 6, ... points progression for fewer players too,
  for consistency" (decisions round B1).
- Rooms of 1+72: an older client cannot read a stage result awarded by
  the top-biased tables. ANSWERED (the maintainer, 2026-11-13: "Refusing
  with 'reload to update' seems useful"): the room model's revision
  (round B1).
- Rooms of 1+72: the member bound is 384 (a full field and about 300
  watching). Enough until the audience exists? The deployed
  `ROOM_MAX_MEMBERS = "32"` is raised to 73 by the server; set it to 384
  (or remove it) to have watchers beyond a full field.
- Rooms of 1+72: the eight slots beside the field show the attackers and
  the nearest rivals. Should the leader always have one (the one to
  catch), and should a phone show fewer?

- The host's succession: a host away is given 30 s (the room's
  reconnect grace) before the room passes on or, under Close, ends.
  Enough, or should a host be waited for longer than anyone else?
- The host's succession: a host alone whose connection is lost keeps
  the room (it waits, and the first other person to connect becomes its
  host); only a host who leaves ends an empty room. The brief had both
  end it. Right?
- The host's succession: a co-host inside the grace, and administrators
  who only watch. ANSWERED (the maintainer, 2026-11-13): "a member can
  inherit the host role if there isn't a co-host to inherit it"; the
  co-host inside the grace first; observers skipped (round B1).
- Leaving: Back on an online room's song select now leaves the room
  (it used to keep the player in it, silent, behind the home). Right
  for a member between matches too, who is not asked, or should that
  Back keep them in the room?
- Leaving, answered (the maintainer, 2026-11-12): CPU practice is
  never asked about and a match left running is dropped silently; a
  stage's play is left as a forfeit and watched on, the question asked
  from there ("Watching a stage"); the host's 30 s grace stays; a host
  away alone keeps the room.
- Watching: a watched stage ends 2.5 s after it is decided. Should a
  watcher (and a player out of it) see the winner finish the song
  instead, as the design study has it?
- Watching: eight fields in the play's proportion are four by two at
  lanes of 29 px on 1280×720, hardly larger than a player's eight
  miniatures. Shorter fields with larger lanes for seven and eight?
- Watching: a watching host's pick starts the practice stage by itself
  after the room's countdown. Wanted, or a press to start?
- Watching: the control is More's item and a button at the Players
  pop-up's foot, saying what a press does ("Watch only", "Play").
  Enough, or a line of its own on the players panel?
- Watching: a watcher who chooses Play during a match waits for the
  next match (they were not of it). Should practice let them into the
  next stage?
- Watching: a player out of a stage keeps their own dimmed field.
  Should they be able to follow one competitor large instead?
- Leaving a play: the quit forfeits at once and cannot be undone.
  Right, given how easy Escape twice is?
- Leaving: a member's browser Back off an online room's page, where
  nothing is asked, only changes the page: the room keeps them until
  the tab closes. Should leaving the room's page always tell the room?
- Host options: a landscape phone shows one or two other players at a
  time (the rest scroll). Enough, or should the pop-up take the whole
  screen there?
- One picker: after taking the pick, the song the new picker's list
  shows is told the room only once it has rested 3 s, as any song.
  Should the first one after a take be shown at once?
- One picker: the mark is words after the role ("Host · Picking") on
  every online room's rows, also where the owner has always picked. Too
  much where nobody else could pick?
- One picker: an operator who follows and backs out of a stage's prompt
  only withdraws now (it used to cancel the pick for everyone). Should
  an operator keep a way to cancel a start from the prompt?
- One picker: Pick songs is T on a keyboard and More on a controller.
  Should a controller have a button of its own for it?
- The room's song select: the swap on a change of hands takes 0.15 s to
  leave and the usual entrance to come in. Right, or should it be
  instant always?
- M8 round 3: the two announcer lines for attacks could not be added
  without touching the announcer studio's files (its screen names every
  line in a match with no catch-all). May the next round add the four
  arms and eight messages there, or should the studio's matches get a
  catch-all so a new line never needs them?
- M8 round 3: on a controller, View/Select is the quit (held or pressed
  twice). As built, in a stage with attacks a short press of it changes
  the attack policy and only holding it quits. Is that right, or should
  the policy have another control (a stick's click is free on a standard
  pad; nothing else is under the default bindings)?
- M8 round 3: changing the policy does not clear readiness (the plan
  said it would). Agreed?
- M8 round 3: the policy is the room's (Spread on entering every room).
  Should it be remembered as a setting across rooms?
- M8 round 3: who attacks whom is drawn as a thin line in straight legs
  with a bright dot for the pulse, for 1.5 s. Is that noticeable enough
  while playing, and should it be a free curve with the shock motif as
  the study had it (it would cross miniatures at some sizes)?
- M8 round 3: an incoming line from a miniature on the left ends at the
  field's left edge (the warning is at the right). Should the warning
  stand on the attacker's side instead?
- M8 round 3: "Attack launched" shows in the status line every time
  (several a minute for a strong player), with the line and the outline
  beside it. Too much text?
- M8 round 3: with attacks the standings give up the combo column for
  Sent, Taken and Shocks. The right one to give up?
- M8 round 3 (found, not changed): the singleplayer prompt's play
  options rows step forward on a tap of their ◀ (the arrow's box is
  drawn clear and takes no press). Fix it?
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
- Fields under eight: ANSWERED (round B1: the eight-player table read up
  the field). Should sit-outs count toward the field?
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
- M3: a CPU's seed per stage. ANSWERED and built (round B1,
  `cpu-stage-seed-v1`).
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
- M5c: who may copy the invite. ANSWERED (the maintainer, 2026-11-13):
  the host and co-hosts, who may also replace it (round B1).
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
  (Answered by `attacks-v1` and `attacks-v2`, 2026-11-13: there is no
  recovery and no shield; an attack is over for the ledger once placed.)
- Attacks, version 2 (2026-11-13): every CPU on Leader in a field of 9
  or 33 buries the competitor with the most life (up to 32 attacks
  placed at once against them, 97% of their chart's room filled to the
  song's end). Build the proposed limit (at most three placed at once
  and a free safe section after each, `chain: 2, chain_gap: 1`), leave
  it to Spread being the default, or change what Leader means (say, the
  leader among those with fewer than N still to come)?
  (Answered by `attacks-v3`, 2026-11-13: a window of 15 s from the
  launch, and Attacks at once as a match rule, three by default; a
  sender's policy goes on to its next choice where the first has no
  room.)
- Attacks, version 3 (2026-11-13): the window is kept at 15 s (measured:
  "Attacks, version 3"); is three the right default for Attacks at once,
  given that with every CPU of a large field on Leader the leader still
  has three placed against them almost continuously (90% of the room
  over their attacked span)? Should Leader wait for its leader instead of
  spilling over to the second?
- Shift changes the policy (2026-11-13): the cap beside the policy's word
  is always shown (it is calm); should it fade after the stage's first
  seconds? Windows' Sticky Keys dialog after five quick Shifts is left to
  the system.
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
- Step down (2026-11-13): the life given back (half), the breather's
  length (a bar past the reading horizon and a second: five to eight
  seconds on Beatucada), whether a player may ever step back up, and how
  a step down should rank (fewer steps first now) are the maintainer's
  to decide. Measured: under Normal it keeps the weakest CPUs in to the
  song's end without moving the stage's order much. (Superseded the
  same day by adaptive charts: steps up, ranked by the score, below.)
- Adaptive charts (`adaptive-v0`, 2026-11-13): the step-up trigger (four
  bars of full life without a miss, a hold let go or a shock stepped on),
  the hysteresis (eight bars back up to a rung whose life ran out),
  ranking by the score with life only as the tie-break, and whether a
  player should be able to hold their rung (an "I stay here" choice) are
  the maintainer's to try. A CPU's strength is relative to the chart it
  is on, so CPUs of every level climb when they play cleanly: whether
  CPUs should take a chart's absolute difficulty into account is open.
- Rules from the next stage (2026-11-13): whether Points may change
  from a stage on. ANSWERED by the maintainer the same day: yes, with the
  host aware of it ("Points from a later stage", below).
- Rules from the next stage: a change never lands on a stage once picked
  (its window open), even if nobody has readied or loaded yet. Should a
  picked stage whose readiness has not begun take it (sooner, but its
  song's prompt may already be on someone's screen)?
- Rules from the next stage: an older client in a newer room plays a
  changed stage under the match's own timing and Boos where the stage
  has no attacks (no plan to disagree on). Acceptable for the gated
  prototype, or should the content (not only the plan) name a stage's
  changed rules?
- Rules from the next stage: Apply and restart from "From stage N"
  takes the per-stage rows as shown into the next match's rules. Right,
  or should it take the next match's draft alone?
- CPUs online (2026-11-13): the room keeps the first report of a CPU's
  checkpoint (a dishonest device that is first stands, every other
  counted as a mismatch and logged). Keep that, or wait for a majority
  (later checkpoints for everyone)? Every connected player's device sends
  every CPU's reports (simple, no takeover: 43 KB/s into the room for 13
  people and 60 CPUs, 5% more messages): fine, or two devices a CPU?
- CPU levels (2026-11-13, `cpu-levels-v1`): what should each level be at
  home on (today 1 Beginner, 3 Easy, 5 Medium, 7 Hard, 9 Challenge, 10
  Challenge nearly perfectly, the coordinator's starting points)? Should
  the solo CPU move to the ten levels too (today its five chart-relative
  strengths, with `--cpu-level`/`level=` beside them)?
- CPUs online: a CPU's reading horizon is 2 s for every CPU (practice
  reckons it from the local player's own). Keep it fixed, or take the
  stage's slowest player's?
