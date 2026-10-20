# Noteskin authoring with SkinCraft

[SkinCraft](https://github.com/sugoijan/skincraft) is a separate native application.
It embeds PhotoCraft and VectorCraft and draws the real DDI field preview.
Its source, architecture notes, launchers, MCP bridge and tests live in that
repository. See its README for Git pins and optional local Cargo patches.

DDI retains the shared loader, SVG support, Lua host, field renderer, demo
and bundled noteskins. SkinCraft consumes these crates as pinned Git dependencies;
DDI has no dependency on the editors. `ddi_render::HUD_FONT` supplies the same
font used by the game. `ddi-desktop` stages the dependency checkout's assets
beside the executable, so a Git build needs no sibling game checkout.

When changing a shared host API, test SkinCraft with local patches before
pushing DDI, then update SkinCraft's DDI revision and test without patches.
