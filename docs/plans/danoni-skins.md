# Implementation plan: noteskins on Dancing☆Onigiri layouts (2026-10-13)

Until now a noteskin draws the dance layouts only: on a Dancing☆Onigiri
key mode the built-in arrows draw every lane, whatever skin is chosen.
Most of those lanes are arrows (some at diagonals); one, the onigiri, is
not. The maintainer's decisions (2026-10-13):

- **7i is no longer played.** Its giko and iyo lanes have nothing in
  StepMania's skins to stand for them; with it gone, the onigiri is the
  only lane that is not an arrow (5, 7, 9A and 9B keys). Works in 7i are
  left out on import with a note, as other key modes are.
- **A lane draws only from the chosen skin and the skins it names**
  (its `FallbackNoteSkin` chain, which ends in StepMania's own `default`
  and `common`, our stand-ins). Never from another skin: not Dito, not
  another bundled skin, not the built-in arrows standing in for a lane.
- **The onigiri lane**: the skin's own `Onigiri` elements (our name, for
  skins that draw one), else StepMania's `Center` (the middle panel of
  pump and techno, which some dance skins carry), else the skin's Down
  elements as they are (unturned).

## Status (2026-10-13)

Done: 7i out; `Skin::lane_button` (`noteskin/src/load.rs`) and
`NoteskinPlay` on every built-in layout, natively and through the skin's
Lua (`Button::Center` and `Button::Onigiri` are buttons the Lua is given
as `Var "Button"`); Dito's onigiri star (`xtask gen-dito`, every colour
scheme, and `Onigiri Tap Note.lua`, `Onigiri Receptor.lua`); CF_CHROME's
centre files back (CF_VIBRANT and CF_ALLOY have their own models and
glow). Checked: native tests (the mapping, a 7-key play of a synthetic
skin, every bundled skin's onigiri through its Lua); danoniplus's sample
works (MIT) in headless Chrome on WebGPU and WebGL2 and in Firefox:
Dito's stars, CF_CHROME's squares and Delta's unturned Down on 5 and 7
keys, and 7-key autoplays with Dito all Fantastic through their first
332 notes (70 s) on each. After an independent review: a skin has
`Onigiri` or `Center` elements when its chain holds files (or folders)
a lookup finds, `Center Tap Note…`, not art its Lua reaches by another
name (Dake's `_center …`, whose Lua draws its placeholder X for a button
it lacks: Dake gets Down); when the skin's Lua cannot build the chosen
button's tap note or receptor (a redirect table without `Center`), the
lane is built again with the next choice, down to Down; 7i's charts stay
in their works' songs as unplayable placeholders (no notes, a layout no
one resolves), since works stored before refer to the charts after them
by position (`ChartInfo::index`), and a song of them alone is dropped;
Dito's explosion rings the onigiri with its star's outline. A skin's own `DownLeft` and
`DownRight` files (pump's diagonals, a few skins) are not used: the Down
arrow turned stands for them, as for every skin.

## Steps

1. **7i out**: `PLAYED_DANONI_MODES` without it, its tests and docs.
2. **Lanes onto a skin's buttons** (`noteskin`): an arrow lane takes the
   dance button its angle names (Left, Down, Up, Right, UpLeft,
   UpRight), else Down's elements turned to its angle (the diagonals
   down-left and down-right of 7 keys); the onigiri lane `Onigiri`,
   `Center` or Down by what the chain holds. Natively tested.
3. **The play** (`render/src/noteskin.rs`, the app): a skin draws every
   built-in layout, natively and through its Lua; the preview's layout
   picker shows it.
4. **Dito's onigiri**: its generator draws the onigiri star the built-in
   arrows draw (tap note sheets per colour scheme, receptor), and its Lua
   loads them for the `Onigiri` button.
5. **CF_CHROME's centre**: the `Center` files left out when it was
   bundled come back (byte for byte), so its family draws its own
   onigiri.
6. **Checks**: native tests, the preview on 5, 7, 9A and 9B keys with
   Dito, CF_CHROME, Delta and a skin without either element, and a full
   autoplay of a Dancing☆Onigiri work (synthetic) on WebGPU, WebGL2 and
   in Firefox.
