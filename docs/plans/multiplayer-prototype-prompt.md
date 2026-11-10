# Handover prompt: prototype CPU practice and private rooms

Copy the following into the implementation conversation, with the documentation
handoff commit available in its checkout. The original conversation continues
the rest of the multiplayer design; this prompt does not transfer that work.

```text
Start prototyping Dito Dito Infinito's multiplayer CPU practice and invite-only
hosted-room flows from the design work already done. This is authorization to
implement those flows incrementally, not another research-only task.

Work in this implementation checkout. Preserve unrelated work and do not switch
or edit the original design chat's worktree. Inspect current main/code first; the
documentation handoff was prepared separately and may need to be applied to this
branch. Do not revert newer work to match the handoff's base revision.

Read AGENTS.md, then:
- docs/plans/multiplayer-prototype-handoff.md (scope, constraints and open choices)
- docs/plans/multiplayer-ui.md (flows and latest visual/interaction decisions)
- docs/plans/multiplayer.md (battle semantics and announcer event contexts)
- tools/multiplayer-design/README.md (archived interactive mockups and checks)
- the relevant UI, session, theme, localization and shared-song plans it links.

Implement useful slices, starting with shared room/match/stage state and the local
practice path, then the private online-room/auth/invite/sharing path as appropriate.
Reuse the shared Rust UI/session/theme architecture for web and desktop. Keep room
membership, role authority, participation, loading, readiness and connection
separate. Reuse local song selection and the existing shared-song mechanism.
Online identity uses Twitch OAuth initially; local practice needs neither sign-in
nor a service. Inspect the backend contract before deciding how room authorization
and song-upload authorization fit together.

Preserve the reviewed design: unified room + song wheel; Rules/More pop-ups;
no manual attacks or in-play targeting; eight competitors initially; everyone
normally returns each stage; finite matches in persistent rooms; no automatic
Ready on entry; no AFK veto; winner finishes the song; immutable current-match
rules with explicit next-match drafts. Use the handoff's acceptance walkthroughs.

The CPU simulator may still be unavailable. Keep a replaceable bot/participant
interface and use an explicitly development-only scripted driver if necessary
for the first flow slice. Do not present perfect autoplay or fake life bars as
skill-tuned CPUs. Mockups and scripted traces are not balance validation.

Numerical battle rules, scoring outside eight-player fields, final-contender
settlement, reconnect windows and the online transport/authority contract remain
provisional. Make routine implementation decisions and continue useful work;
isolate and document experimental choices rather than silently treating them as
approved. Do not let unresolved balance or a missing backend stop independent
screen/state work. Report any actual external dependency needed for the next slice.

Do not expand this task into public matchmaking, 100-player scaling, Adaptive
balance, recording announcer lines or implementing every custom-room option.
Keep their extension points where necessary. The original conversation continues
those studies in parallel. Announcer work here concerns event semantics and
visual fallbacks, not recordings or prematurely invented catalog keys.

Add meaningful tests for real state transitions, failures and adapter contracts;
run checks appropriate to the affected code and inspect the UI at runtime. Keep
singleplayer working. Track implemented behavior separately from the design
studies, and report what is real, what is mocked, validation performed, and
consequential assumptions/questions to bring back to the design conversation.
Proceed with implementation; do not stop at a plan or ask for another blanket
approval of the already authorized prototype scope.
```
