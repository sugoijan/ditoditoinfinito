Latest inner-notch and continuous-border refinement: [../outline-joins/](../outline-joins/README.md).

# Cap color and diagonal head/body joins

This small revision follows `../stacking/` and preserves its full cap length,
repeat phases and directional stacking rules.

## Cap color

The original Down/Up/Left/Right caps keep a fixed yellow-green rather than
continuing the body's vertical gradient. A visible colored sample from each
original cap is RGB `(232,246,80)`, `#e8f650`. The base cap field and the upper
diagonals' terminal-note field now use that material. Their white/gray masks
and shape are preserved. All cap endpoints remain source row 482.

`*-cap.pcraft` masters keep the color fields separate from the repeated masks;
active/inactive PNG exports preserve the original white-to-gray change. The
DownReference control is also recolored to the original cap material.

## Diagonal connectors

The body starts at the head's vertical center. Some diagonal columns end
above that line, exposing the backdrop between the head and body. Depending
on the direction, 68 or 92 source columns need filling, with a maximum gap
of 69 source pixels (8.625 logical pixels).

For each column, the exposed interval between its last opaque head pixel
above center and the body start becomes a connector mask. Gray body-edge
material and the head's existing green/yellow field fill that mask on layers
**behind** the original head, with a half-logical-pixel overlap at the join.
The original head remains intact above it. This fills only the exposed gaps;
it does not move the body start or extend a full rectangle beyond the arrow.

`*-head.pcraft` masters retain the original head, gray connector and masked
color-field layers. The roll head uses the same repaired image as its hold
head. Read-only exported-image analysis records the masks in `geometry.json`
and confirms zero unfilled pixels in all four gap regions in `validation.json`.

## Preview and workflow

The saved runtime study is
`target/scratch/peters/joins-caps-ready/NoteSkins/dance/Peters-DDR`, shown in
SkinCraft on the nine-lane Vivid preview. The earlier body/stacking textures
are retained. All art edits, masks and exports used SkinCraft native commands;
Python only measured/read pixels and registered new resources in the separate
preview copy, as required by the MCP's current resource-creation limitation.

Sources: Peter Lawrence's Peters-DDR textures (Unlicense), plus this project's
square study and native editing work (MIT), annotated in REUSE.toml.
