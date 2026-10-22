# Revised raster square drafts

[Comparison board](comparison.png), made in SkinCraft. The top row uses
`√2 : 1 : 2`; the bottom uses `√2 : 1 : 3`. These are the inner square's
side, middle charge region's width and outer charge region's width,
respectively. Dividing bands have a fixed reference-derived thickness.

The square keeps the arrow-derived perimeter. Its former black inner outline
is removed: the main colored band directly touches the large outer charge
region. The center is substantially smaller, and the outer region is the
main flash. Charge labels are now **3 = inner, 2 = middle, 1 = outer**;
states accumulate **3 → 3+2 → 3+2+1**, lighting the main outer region last.

The separating bands are `3⅓ / √2 = 2.357…` logical pixels wide, measured
perpendicular to the original arrow's diagonal internal stripe. This is
about 21% thinner than the first draft's 3-pixel walls. The original model's
parallel internal boundaries at `y = x + 5⅔` and `y = x + 2⅓` give that
distance. The preserved main perimeter band is still about 4.2 pixels wide.

Each receptor is produced from the **same region coordinates** as its note:
the note's colored perimeter and dividers become original receptor gray,
its charge regions become transparent, and its outer black border is
replaced by the original white receptor border. No separate inner receptor
outline is introduced. `geometry.json` records the shared boundaries.

`outer-2-note.pcraft` / `outer-3-note.pcraft` contain the editable region
layers. Toggle `Charge 3 - inner`, then `Charge 2 - middle`, then
`Charge 1 - outer main`. Their receptor counterparts retain the same masks.
The `charge-0` through `charge-3` PNGs show the cumulative states; the native
comparison board remains editable too.

All artwork changes used SkinCraft's native copy/paste, transforms, selections,
layer visibility and clear operations. The temporary orchestration script
calculated the shared coordinates and dispatched native commands; it did not
render or edit pixels outside SkinCraft. This remains a visual design study.

## Selected draft in the live preview

The outer-width-2 version is selected. Its note and receptor atlases were
assembled in SkinCraft from the editable draft. The temporary live project is
`target/scratch/peters/outer-2-live/NoteSkins/dance/Peters-DDR`; the saved
runtime export, including the script drafts, is
`target/scratch/peters/outer-2-live-ready/NoteSkins/dance/Peters-DDR`.
SkinCraft's project home for this preview is
`target/scratch/peters/studio-preview`.

The initial preview used **By subdivision** colors. The square repeats the original
arrow charge cycle once per beat: full charge at the beat, reset a quarter
beat later, inner at half a beat, inner+middle at three quarters, then the
large outer region completes the charge at the next beat. The receptor keeps
the original 0.2/0.8 beat pulse and press timing. The Vivid follow-up adds an atlas sampled directly from the original
`textures/colours.png`, with a four-beat color cycle and a once-per-beat
charge cycle. Empty Vivid regions use the original frame texture’s opaque
`#525252`; charged regions use its white. Quantized’s original uncharged
texture samples instead have zero alpha, so that variant needs transparent
charge holes. Black in the first raster draft was incorrect for both.

The visible macOS session now uses `target/scratch/peters/studio-visible`
and the saved `outer-2-live-ready` skin on the five-lane layout. A temporary
`target/scratch/peters/SkinCraft.app` wrapper makes the development build
accessible through normal macOS app/window controls.

The saved Vivid preview is now
`target/scratch/peters/outer-2-vivid-ready/NoteSkins/dance/Peters-DDR`.
The visible session loads that export with Note colors set to Vivid.
`outer-2-vivid-note.pcraft` is the editable gray/white Vivid master, and
`outer-2-vivid-charge-0.png` through `-3.png` show its cumulative states
(samples have different hues because they are taken along the color cycle).
The runtime atlas has 32 snapshots over four beats, eight per beat, sampled
from the original Vivid color texture; its charge phases remain full, empty,
inner, inner+middle in successive quarter beats.

## Center hold draft

`center-hold-body.pcraft` retains the original hold's green/yellow column and
side edges, scaled to the square's 48px width. The white material is copied
from the arrow's texture and shaped into repeating chamfered rectangular
links. The square head keeps the note's shared regions, with white markings
and the original hold's gray perimeter. Inactive markings become `#787878`,
matching the arrow's inactive white-to-gray substitution. The flat bottom cap
continues the body pattern and terminates with a gray edge.

`Center Hold *.png` exports are raster resources for this draft. The saved
preview is `target/scratch/peters/outer-2-holds-ready/NoteSkins/dance/Peters-DDR`.
The body and cap have native editable masters. Script wiring is in that
export, leaving the game's earlier center study untouched during review.

The shared demo now places holds and rolls in every lane of layouts other
than four lanes. This makes center/onigiri long notes visible in SkinCraft
and the game's field previews. The running development build uses local DDI
patches for this change; production Git dependency pins are unchanged.

The wider-layout preview omits the center mine so its shock effect does not
obscure the hold/cap inspection. The original four-lane demo keeps its mine.

Center roll heads now reuse the hold-head artwork for both active and
inactive states, matching the original arrow roll redirects. This replaces
the early bright tap-atlas frame. The saved review skin is
`target/scratch/peters/outer-2-holds-rolls-ready/NoteSkins/dance/Peters-DDR`.

## Open-top U hold variant

`center-hold-u-body.pcraft` changes the repeated rectangles to open-top U
shapes. Their strokes are 4 logical pixels wide (previously 3), with the
same 16px repeat, width and original green/yellow fill. Active markings keep
the original off-white and inactive markings keep `#787878`. The matching
cap continues the U pattern. The previous rectangular masters remain for
comparison. `Center Hold U *.png` are the native raster exports.

The live preview now loads
`target/scratch/peters/outer-2-u-holds-ready/NoteSkins/dance/Peters-DDR`.

The next U variant increases the stroke to **5 logical pixels** (40 source
pixels), keeping the same outer bounds and 16px repeat. `center-hold-u5-body.pcraft`
and `center-hold-u5-cap.pcraft` are its masters; `Center Hold U5 *.png` are
the exports. The current preview is `target/scratch/peters/outer-2-u5-holds-ready/NoteSkins/dance/Peters-DDR`.

Wider-layout demo phrases now last 12 beats: holds start at beat 4 and end
at 5.5, rolls start at beat 8 and end at 9. The 2.5-beat gap leaves the hold
caps unobscured by roll heads. The four-lane phrase retains its timing.

## Note-shaped hold cap

The latest cap is copied directly from the lower half of the center hold
head in SkinCraft, for both active and inactive states. It therefore keeps
its exact colored band, gray perimeter, chamfered bottom corners and inner
regions. It uses a 64×32 logical image (512×256 raster), rather than stretching
the half-note into the previous 64×64 cap. The 5px U body is retained.
`center-hold-note-cap-active.pcraft` and `-inactive.pcraft` are the masters;
`Center Hold Note BottomCap *.png` are the exports.

The saved preview is
`target/scratch/peters/outer-2-note-cap-ready/NoteSkins/dance/Peters-DDR`.

## Deriving the hold motif from the note

The latest study reconstructs the original **down arrow** before applying the
rule to center/diagonal lanes. It uses a 7-logical-pixel vertical strip along
the lowest white interior edge of `Down Hold Head Active`, repeated every
16 logical pixels. The source's side edges are retained and its original
continuous green/yellow column fills the colored interior beneath the white
mask. SkinCraft performs the source extraction, masking, repeated pasting
and composition; read-only pixel analysis finds the contour and compares
results. No external raster rendering is used.

`down-hold-repetition-study.pcraft` / `.png` contain this reconstruction;
the reference is the original `Down Hold Body Active (res 64x128).png`.
Its exact-white mask has 97.56% intersection-over-union with the source body.
This is a close reconstruction, not a claim of the original artist's method
or a pixel-exact match; corner/join rasterization still differs. The first
rule uses the note's lower interior contour, rather than repeating the entire
finished note (which would also repeat its gray perimeter and shaft).

The green/yellow material should flow throughout every colored region of the
head/body/cap. The early flat-green center head therefore remains superseded
in intent; transferring the accepted repetition rule and shared color field
to the center and diagonal lanes is the next design step. This study leaves
the existing live noteskin artwork unchanged while the rule is reviewed.
