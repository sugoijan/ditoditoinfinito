# Multiplayer design snapshots

Archived 2026-11-10 for the [CPU/private-room prototype handoff](../../docs/plans/multiplayer-prototype-handoff.md).
These six HTML fragments are unchanged snapshots of the conversation studies.
They contain local scripted interactions, fictional songs/players and synthetic
results. They perform no real sign-in, matchmaking, room management, uploads,
network synchronization or CPU simulation. They are not the game's runtime UI.

## Start here

| Study | Useful views |
| --- | --- |
| [Entry](battle-entry.html) | Multiplayer, public rooms, automatic search/gathering, CPU practice, offline entry |
| [Room settings](battle-room-settings.html) | Creation, public/invite-only access, CPUs, locked rules, next-match drafts, boundary application |
| [Unified room and roles](battle-room-roles.html) | Signed-out invite, host/player/waiter, automatic intermission, co-host, admin observer, evaluation |
| [Play and watch](battle-play-and-watch.html) | Normal field with faithful miniatures, quiet HUD, power, electrical attacks, elimination, watching, winner finish, large-party study |
| [Endings](battle-endings.html) | Pending hazards, winner completion, stage evaluation, per-stage points, shared ranks, return to the room |
| [Reconnecting](battle-reconnecting.html) | Connection loss, conditional same-stage recovery, out/watching, missed start, reauthentication, unavailable room |

Use Entry, Room settings and Unified room first for the implementation scope.
The remaining studies document the play boundary these flows will eventually
connect to. Some earlier snapshots use different proposed CPU defaults or show
more custom controls than the first prototype needs. The handoff and the latest
screen-plan sections explain which values are proposals.

## Viewing

These are conversation fragments, not standalone HTML documents. The study host
supplies the variant carousel, Lucide icons and optional `Tweak` controls. Ask a
Codex conversation with visualization support to display a fragment from this
checkout, or use the installed visualization skill's `scripts/render.py` to make
a standalone preview. For example, with the skill directory set explicitly:

```sh
VISUALIZE_SKILL=/path/to/installed/visualize/skill
python3 "$VISUALIZE_SKILL/scripts/render.py" \
  "$PWD/tools/multiplayer-design/battle-entry.html" \
  /tmp/ddi-multiplayer-entry-preview.html
```

The renderer/host runtime is a tooling dependency and is not vendored in this
repository. Its export may require access to its permitted external resources;
we do not claim browser previews are fully offline. Do not add generated exports
or the visualization runtime to the production app. Simply opening a fragment
without the host may show only its initial view and omit study controls.

The study controls simulate authentication, server responses, roles and match
boundaries. They belong outside the depicted game UI. Typical walkthroughs:

- Entry: local practice → CPU settings/song choice → Start match; or sign-in →
  Find match → simulate Room found → gathering/readiness → CPU-filled roster.
- Room settings: Prepare next match → edit → Save for next match → simulate Match
  ends; also try incompatible rules, insufficient seats or another reserved human.
- Reconnecting: simulate an authoritative room update; successful identity return
  is separate from permission to resume participation.

No live test server or real user account is needed to inspect these examples.

## Logic checks

The archived checks use Node's standard library and a small fake DOM/clock. They
exercise the original state transitions and inspect generated markup. Run from
any working directory, replacing the path below with this checkout as needed:

```sh
for check in tools/multiplayer-design/checks/*.cjs; do
  node "$check" || exit 1
done
```

They cover the six studies, with an additional quiet-HUD regression check. There
is no Rust, OAuth, network, bot, real DOM layout, font rasterization or browser
animation validation in these checks. Browser rendering was not inspected during
the original studies. These tests document mockup behavior; they must not be
used as proof that the game implements multiplayer correctly.

## Maintaining the archive

Keep this handoff snapshot reproducible. Future design changes should have dated
notes and explicit snapshot updates, not be confused with implementation progress.
Use the repository-relative files in `checks/`; none requires the original chat's
absolute visualization directory. Usernames, song names, invite URLs and score
values in the sources are examples rather than external records.

The interface reuses embedded assets already present in this repository:

- Iosevka Term Regular/SemiBold and Iosevka Custom ExtraBold subsets, under
  OFL-1.1. See [font provenance](../../assets/fonts/PROVENANCE.toml) and the
  repository's corresponding license/credit records; embedding does not change
  their licenses.
- Infinito note contours and shock-arrow mine art from
  `assets/noteskins/dance/Infinito/`. The mockups approximate the renderer's
  materials; they are not new noteskin implementations.
- The play study's hand picture was rendered from the existing `assets/hand/hand.glb`.
  See [hand notices](../../assets/hand/NOTICE.txt) for its source attribution and
  licenses. It is a static pose, not a validated moving-hand overlay.
- Generic icons are requested by Lucide name from the viewing host. No external
  icon-library implementation is copied into these fragments.

No third-party songs or recordings are bundled by these studies.
