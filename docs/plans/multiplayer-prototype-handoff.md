# Prototype handoff: CPU practice and invite-only rooms

Status: authorized parallel implementation scope, 2026-11-10. The maintainer
asked to begin prototyping **CPU practice and private rooms** while the original
design conversation continues the remaining multiplayer work. This document
packages the current direction; it does not declare the entire battle design
final or require a full multiplayer implementation before trying those flows.
No game or server implementation is included in the documentation handoff.

Start with the [handover prompt](multiplayer-prototype-prompt.md). The broader
[battle design](multiplayer.md) and [screen plan](multiplayer-ui.md) remain the
semantic and visual references. The [mockup archive](../../tools/multiplayer-design/README.md)
contains the actual reviewed interactive source snapshots and their logic checks,
so the handoff does not depend on this chat's temporary paths or message history.

## Scope and authority

Use this order when sources differ:

1. New maintainer instructions and applicable repository instructions.
2. This handoff's implementation scope and the selected product constraints below.
3. The latest dated sections in the screen and battle plans.
4. Mockups as interaction/layout evidence, not a server protocol or balance model.
5. Research and older examples as alternatives and historical reasoning.

In particular, the October worked match includes mixed attacks and defense paid
from attack power. That is an earlier comparison, **not** the current working
mines-only, non-spending weakening proposal. Placeholder values, role switches,
simulated server events, example invite URLs and synthetic results in the HTML
must not be copied into a production authority.

### Selected constraints to preserve

- **Room → Match → Stage.** A persistent room can run successive finite matches;
  a stage is one song. The last confirmed survivor can finish the song. Everyone
  normally returns next stage; match resources and results have distinct lifetimes.
- **Two entry routes first.** CPU practice works locally without Twitch or a lobby
  service. Private online rooms use invite-only admission and Twitch OAuth identity;
  other providers can be added later without game-owned passwords/registration.
- **Stable identity and membership.** Reauthenticating/reconnecting restores the
  original participant record, never another scoring row, a CPU's score or a respawn.
  Host ownership, operating permissions, watching and playing are separate facts.
- **Eight competitors initially, humans plus CPUs.** Model collections and IDs so
  this is an enforced initial cap, not a permanent eight-player architecture.
- **Focus on playing notes.** No manual attack or target-selection action during
  play. Personal targeting policy is chosen before the stage.
- **Familiar screens.** Use the shared Infinito UI and song-select composition:
  participants/standings, selected song/chart, and ordered song wheel together;
  Rules and More are pop-ups. Text is sparse and functional.
- **Local library and sharing.** Hosts may choose from their entire local library;
  committed songs are shared through the existing shared-song mechanism. Loading
  and readiness are separate. Do not reduce hosted play to a bundled catalog.
- **Progress without an AFK veto.** An unready participant cannot indefinitely
  stall a host-triggered start. Missing a later stage retains earlier results;
  newcomers normally wait until the next match. Viewing results is not a Ready action.
- **Rule scopes.** Access changes affect future admission, not current membership.
  Competitive and CPU settings lock for a match; explicit next-match drafts do
  not rewrite its current rules or readiness. Unstarted-song replacement follows
  its separate readiness/content invalidation rule.
- **Gameplay presentation.** Preserve the normal local field. Power sits above
  life in both local and miniature fields. Opponents' miniatures approximate their
  actual gameplay/settings. Incoming feedback is subtle. Charge electricity and
  a committed-target arc convey an automatic attack; they do not themselves deal
  damage or choose the authoritative target.
- **Announcer semantics, not recordings.** Preserve event/context boundaries and
  equivalent visual information. No voice recording, script-writing or new pack
  naming is requested as part of this prototype handoff.

### Working defaults, not validated balance

| Item | Current study value | Implementation treatment |
| --- | --- | --- |
| Stages | Four | Configurable working default; not a four-song product limit |
| Practice opponents | Three CPUs; Gentle profile | Convenient starting configuration; real skill/MSD calibration remains open |
| Hosted CPUs | Off initially; optional Fixed or Fill | Reuse one participant model; never replace humans/eliminees during a song |
| Battle | Scattered mines + recent-accuracy weakening | Provisional standard; preserve mines-only play-through as a comparison seam |
| Readiness window | 15 seconds | Test value; make the policy explicit rather than embedding a UI timer as authority |
| Automatic intermission | 30 seconds in some studies | Optional future/host feature, not required for the first private-room slice |
| Eight-player stage awards | 12, 9, 7, 5, 4, 3, 2, 1 | Working table; tied places average occupied awards; equal match totals share rank |
| Reconnection | Same-stage return only with verified continuity | No agreed grace interval; room/watch recovery can precede competitive resumption |
| CPU profiles and mine sensitivity | Gentle/Regular/Challenging; Low/Typical/High | Proposed controls, not implemented bot capabilities or numerical contracts |

The examples use eight-player awards, while practice often has fewer players.
**The award rule for other populations is unresolved.** Do not quietly treat a
truncated or normalized eight-player table as approved. Flow prototyping can use
explicit scripted awards or stage placements first. If a playable prototype
needs a temporary rule, isolate/version it and record the experimental choice
for design review. Likewise, final-contender settlement, timing constants,
attack placement and online validation need explicit contracts before their
results can be called authoritative.

## Suggested implementation slices

These are useful independently reviewable increments, not permission gates.
Inspect the implementation checkout first and reuse any work already present.

### Shared flow model and local CPU practice

1. Represent room membership/roles, match identity and immutable rules, stage
   identity/content, participation, readiness and connection as separate facts.
   Use stable participant IDs, not names or list positions. Bind shared screens
   to this state rather than copying HTML logic into platform shells.
2. Add the local entry path and unified room with real local library selection,
   ordered stages, chart choice, CPU configuration and rule validation. It must
   remain usable offline and signed out. Do not add network controls to practice.
3. Connect stage preparation and the existing play/session boundary. Keep input,
   judgment and rendering paths shared with ordinary play. Restore the same room
   for intermission, stage evaluation and another match with fresh readiness.
4. Integrate the bot implementation if available. If it is not ready, use a
   clearly identified development-only scripted participant driver to exercise
   transitions. It must not masquerade as a calibrated Gentle CPU or provide
   balance evidence. A real opponent should produce judged play and respond to
   mines under the same battle rules, rather than merely decrementing a fake life
   bar or using perfect autoplay as the skill model.
5. Connect battle mechanics incrementally behind explicit rules/configuration.
   Keep the screen/model work useful even while bot quality and attack balance
   continue independently. Preserve the event order and timing safeguards from
   the battle design whenever attacks become playable.

### Invite-only hosted rooms

1. Reuse those screens and state concepts. Add create room, explicit invite
   preview, signed-out join intent, Twitch handoff/cancellation, revalidation and
   return to the intended room. No implicit Ready or new match on entry.
2. Establish the online room authority/transport and provider-backed identity
   contract. These have not been selected by the mockups. The host controls the
   room, but their client is not assumed to be the server. Private membership,
   host/co-host/admin permissions and invite admission must be enforced by that
   authority, not just by hidden buttons.
3. Connect real sharing/loading. An existing upload-and-read service is available,
   but room membership, Twitch identity and song-upload authorization are not
   already the same system. Trace the existing adapters and extend the contract
   deliberately; do not ship an administrative upload bearer token in a client
   or assume Twitch sign-in grants every share operation.
4. Support an explicit preparation/start contract: content identity, readiness
   revision, eligible roster, start commitment/cancellation and the transition
   into a common stage. Server events do not move note timestamps, rewind input
   history or pause local audio. UI-only fakes can validate flows before a real
   network stage, but must be reported as such.
5. Exercise initial membership, waiting until the next match, missed readiness,
   host absence and return to the room. Start with conservative room/watch
   reconnect behavior if safe same-stage recovery is not implemented. An
   unreachable room is not a confirmed forfeit, interruption or win.

A first slice may stop at a real local flow or a private room with preparation
and a clearly stated development play boundary. Report what is real, what is
scripted, and what remains; do not label it completed online battle support.
There is no need to implement public discovery, queues or 100-player rendering
in order to start these two paths.

## Contracts worth keeping explicit

These are conceptual seams; the implementation can choose appropriate types and
module locations after inspecting current code:

- Room access/membership and host/operator capability checks, independent of
  participation and competitive results.
- Current match rules versus editable next-match proposal, with revisions and
  atomic application. Revalidate capacity and content at commitment/boundaries;
  concurrent changes must not silently overwrite live rules.
- Ordered stage selection/content identity versus each player's loading and
  readiness acknowledgment. Replacing a committed selection invalidates the
  relevant preparation, not already judged play.
- Participant state and competitive events versus presentation snapshots. A stale
  miniature or local prediction is not the source of a result or attack decision.
- Local authority/driver and online service adapter behind shared behavior, with
  deterministic inputs/event ordering where the battle engine requires it.
- Bot judgments and mine sensitivity behind a replaceable opponent interface;
  no dependency of screen navigation on the future simulation research harness.
- Confirmed stage result, optional winner song completion, and match aggregation.
  Never turn the winner's remaining solo notes into additional match points.

When implementing attacks, preserve one reserved/active incoming slot per player,
no backlog, safe future placement, committed weakening, full-bar retention when
no attack can be admitted, and recovery measured by the selected rule. Exact
thresholds remain tuning inputs. Already committed hazards can outlive their
sender; do not award the sole remaining player victory before settlement.
If continuation cannot preserve readable warning lead and complete history,
recover the room/observer state rather than backdating unseen hazards.

## Repository integration map

Read the current checkout's `AGENTS.md` and relevant plans before changing these
areas. Locations below are verified against the documentation handoff checkout;
inspect current `main` rather than assuming the implementation has not moved.

| Area | Existing entry points |
| --- | --- |
| UI state, screens, input and harness | `crates/ui/src/state/`, `crates/ui/src/screens/start.rs`, `select.rs`, `prompt.rs`, `results.rs`; [UI core](ui-core.md) |
| Library-backed song selection | `crates/ui/src/state/select_view.rs`; [song navigation](song-navigation.md) |
| Theme and gameplay presentation | `crates/theme/src/infinito.toml`, `crates/render/src/hud.rs`, noteskin rendering; [theme](theme.md), [HUD](play-hud.md) |
| Shared play and deterministic rules | `crates/session/src/play.rs`, `crates/engine/`; [desktop parity](desktop.md) |
| Song-sharing data and shell adapters | `crates/library/src/share.rs`, `crates/web/src/share.rs`, `crates/desktop/src/share.rs`; [sharing contract](shared-songs.md) |
| Announcer catalog and shared event logic | `crates/library/src/announcer.rs`, `crates/session/src/announcer.rs`; [announcer plan](announcers.md) and [line catalog](../announcer-lines.md) |
| Localization and input hints | `locales/README.md`, [localization](localization.md), [controller input](controller-input-core.md), [controller hints](controller-hints.md) |

The shared-song backend is in a separate private repository. Its location and
current capabilities should be discovered in the implementation environment;
this handoff does not imply that a room service or Twitch backend already exists.
No secrets, credentials or deployed endpoint changes are included here.

Reuse shared UI/session/theme code across web and desktop; put browser OAuth,
native browser return, storage and transport I/O in their proper adapters.
Follow the repository's localization and controller/keyboard conventions when
turning mockup text and controls into real screens. Do not embed the HTML studies
as the game's UI or port shell-specific copies of the room state machine.

## Acceptance walkthroughs for each applicable slice

- Signed-out, offline CPU practice: select real songs/charts, configure opponents,
  start a stage, return to the same room, and start another match. Verify that
  the bot/driver boundary is honestly identified and singleplayer still works.
- Invite-only room: create, select/share a host-local song, open its invite signed
  out, cancel sign-in, retry, and return as the same identity. Recheck an invite
  that expired/rotated or whose admission changed during sign-in.
- A new arrival during a match watches/waits. An existing participant returning
  after a missed stage retains prior points. Neither joins a running song.
- Required content loading/failure, one forgotten Ready, no ready humans,
  canceled start and a song replacement during preparation have distinct results.
  Results inspection never blocks readiness/progression.
- Room access changes leave present members and readiness intact. Current rules
  cannot change mid-match. Saved next-match edits survive and are revalidated
  at the boundary, including newly reserved human seats and edit/start races.
- Host, co-host, admin observer and ordinary participant permissions stay distinct.
  Co-host/admin policy is part of the intended model; full management UI can be
  staged, but tests must not assume every connected client is authorized.
- Known attack state and confirmed results survive recovery where implemented.
  No CPU takeover, immunity, respawn, invented zero-point loss, or packet-order win.
- Winner completion and overall results remain separate. Keep earlier results
  inspectable after the room prepares the next match.
- Use the UI headless harness and relevant engine/session tests for real behavior;
  exercise service adapters with failures and stale events. Check keyboard,
  controller and reduced-motion presentation, content sharing on both shells as
  applicable, and runtime layout. Mockup logic checks are not substitutes.

## Parallel-work boundary

The implementation lane owns its code, migrations/service contracts, tests and a
prototype progress record. Begin on a suitable current checkout with these docs
available; do not commandeer the design chat's worktree or restart its research.
If importing this documentation commit onto a newer `main`, resolve documentation
conflicts in favor of the newer product work while preserving this scope.

This design lane continues public discovery/automatic-session policy, mixed-skill
and Adaptive mechanics, battle balance/simulation, scaling beyond eight, detailed
attack/network settlement, remaining edge cases and announcer event refinement.
Those subjects are not silently finalized because a prototype needs a temporary
choice. Small routine implementation choices can proceed; record consequential
assumptions, their user-visible behavior and the decision needed back here.

Use a separate progress note or a new dated implementation section to record
what was actually built and tested. Do not rewrite these research mockups into
claims of implemented or balanced behavior. When feeding back, include the
concrete flow, chosen provisional behavior, validation and open design issue.
