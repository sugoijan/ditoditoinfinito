# Touch controls (2026-11-20)

Status: built (round touch-1, 2026-11-20; round TC2, 2026-11-21: the
rain's outlines as the glyphs, the hand on an upright screen, no
selection during a play; round DE, 2026-11-22: the receptors above the
controls under Reverse upright, a size): the buttons and the pad drawn
over the play, presses and slides on both shells, the option in Options
and on the prompt, the prompt's diagram, touch as an input source. Open:
the look (three candidates, below), the default (Buttons: a change for
every existing touch player, below), what the smallest phone does with
seven lanes and more.

## What the maintainer asked

"I wanted to advance the playability in mobile. I think touching the
receptors is a bad design, I want to initially support 2 touch control
styles: 4 touch buttons split into two regions on the bottom left and
right, matching the order of the 4 receptors (and similarly when there
are more arrows); and a 3x3 virtual pad with the arrows used by the
current receptor drawn on the pad."

Before this round touch play split the play screen into full-height
columns at the receptors' midpoints (`crates/web/src/web/touch.rs`): a
finger anywhere in a lane's column pressed it, so fingers covered the
notes, and nothing was drawn. Desktop had no touch play at all.

## The maintainer's answers (2026-11-20), question by question

1. Where the field goes: "The field stays the same and the controls are
   drawn over it a bit translucent."
2. Odd lane counts: "One more button on the right side by default
   (better for right-handed people, optionally left side version)".
3. Many lanes (8 to 10): "I don't know what would be best in that case,
   we can defer it and only support a pad (or double pad?)". So Buttons
   are not offered for those layouts; they get the pad (two pads for the
   doubles).
4. Doubles on the pad: "Two pads side by side even when upright, scaled
   to fit the best way they can. This might not be good, but at least it
   should be technically functional... they aren't a priority."
5. Look: "I'm not sure about the look maybe they light up on touch down,
   maybe something lightly inspired by the behavior of the dancing hand's
   pad."
6. Slides: "Keep that, but with an asterisk, this needs careful testing,
   and I won't test it too deeply myself. I don't know when I will have
   good feedback on this UI."
7. System gestures: "I'm not sure, for now don't worry too much about it
   and let's adjust after feedback." The safe-area insets are kept as
   today; edge swipes were not studied further.
8. The option: "We can keep today's as a legacy option for now, but I do
   want to remove it."
9. The prompt's controls diagram: "It should show the receptors and each
   control they map to drawn below (something like a 'cut' view of the
   control)".
10. Desktop: "We should support it too, with a similar logic to how we
    support keyboard/controllers, when using touch to interact with the
    interface the touch controllers are assumed, but if keyboard is used
    we hide the controls and assume keyboard input."
11. The smallest phone: "I'm not sure what to do in this case. We don't
    have to support the smallest phone."

## What was built

- **The layout** (`ddi_ui::touch_controls`, pure, natively tested): from
  the screen's size in CSS pixels, its safe-area insets, the field's
  edges, the layout's lanes and the player's choice
  (`touch_controls::Choice`: `Style` and `Hand`), each lane's target
  (`Target`: its rectangle, a button of a group or a pad's tile) and the
  groups' bounds. Buttons: the lanes in the receptors' order (by column),
  the left half at the bottom left from the safe area's left edge, the
  right half at the bottom right, an odd layout's extra button on the
  right (`Hand::Right`, the default) or the left. Widths share each
  side's room (at most 29% of the screen's short side, 72 to 120 px),
  heights 26% of the short side (64 to 120 px). Buttons are offered up
  to seven lanes on one row (`BUTTONS_MOST`); a screen where they would
  be narrower than 44 CSS pixels (`BUTTON_LEAST`: about 7 mm on a phone,
  the platforms' least touch target) plays the pad instead (seven lanes
  on a phone upright). The pad: `ddi_dance_mat::MatLayout`'s tiles, keypad
  numbering (7 8 9 the top row), each lane on its tile (9A's onigiri on
  both centers), the other tiles plain; one pad beside the field on the
  chosen side where a wide screen leaves room for tiles of 44 px and
  more (`PAD_BESIDE_LEAST`; at the left, room above it for the quit
  button and the volume, `QUIT_ROOM`), else under the field centered,
  at most 42% of the screen's height (`PAD_TALL`); two pads (doubles, half-double, 9A) beside the
  field left and right on a wide screen, else side by side at the
  bottom. Every layout the game plays has a mat layout, so every one
  has a pad.
- **Which lane a finger presses** (`Controls::lane_at`): going down, the
  nearest target within half a gap of it, or within 14 px beyond the
  group's outer edges (`REACH`; never between the two groups); a plain
  tile presses nothing. A slide keeps its lane until the finger is 8 px
  inside another lane's target (`DEAD_BAND`): the gaps, the plain tiles,
  the space between the groups and the screen beyond the controls keep
  the lane, so a hold survives a thumb drifting and a boundary's jitter
  never flutters. Several fingers on one lane hold it until the last
  lifts (counted for both shells by `ddi_shell::play_pointers`, as the
  columns did; round PU, below).
- **Drawn** (`ddi_shell::touch_controls::TouchView`): by the play's 2D
  pass under everything else of it, over the unchanged field (but for
  round DE's receptors above them under Reverse upright), each
  target a translucent plate with the lane's glyph turned as its
  receptor (since round TC2 the start menu's rain outlines, one shape
  each, below), lit while held. The quit button and the volume move
  above the left group while the controls show, and a stage's
  opponents' fields keep clear of them. Nothing is drawn under the legacy
  columns.
- **Touch as an input source** (`ddi_ui::controller_hints::Source::Touch`):
  a finger or a pen on the menus or the play makes it the source, a key
  or a pad's press hands it back; the controls show only while touch is
  the source and are hit-tested always (a touch shows them again). The
  web starts from touch where a finger is the primary pointer
  (`pointer: coarse`). Between the keyboard and touch nothing is rebuilt
  (`Ui::set_source`); the prompt reads the source as it draws.
- **The option**: Options → Controls, under "Touch screens": Touch
  controls (Buttons, Pad, Columns (old)) and Extra button (Right side,
  Left side), `Settings::touch_controls`, `Settings::touch_hand`; the
  prompt's play options show Touch controls on a touch screen
  (`PlayOptions::set_touch`).
- **The prompt's diagram** on a touch screen: under each receptor, a cut
  of its control in place of the keycap (`prompt_controls::touch_cut`):
  its pad whole, or its group of buttons over a line for the screen's
  bottom edge, shifted to its side, the lane's own lit.
- **The web** (`crates/web/src/web/touch.rs`): the play's view places the
  controls each frame (`Drawn::touch`) and the pointer listener lands
  presses on them; the columns are kept as they were for the legacy
  style. `__DDI_DEBUG.touch` says the placed controls (CSS pixels),
  whether they show and the look. TESTING ONLY: `touchlook=glass|mat|glyphs`.
- **Desktop** (`Shell::touch`): winit's touch events reach the play's
  controls as on the web, the same counting and slides; touch becomes
  the UI's source, a key or a pad hides them. TESTING ONLY:
  `DDI_TOUCH_LOOK`. Checked offscreen (`tests/play_pointer.rs`); no touch
  screen was available.

## Round TC2 (2026-11-21): the glyphs, the hand upright, no selection

What the maintainer said after trying the controls on a phone: "None of
the 3 look great nor terrible, any should be fine. Some bad points that
stand out are: the arrow strokes overlap, maybe we can use the same
outline shape as the arrow rain background effect, on the control
preview it would be good to see the same shape as well. Also, the
dancing hand is too small when the screen is vertical, maybe we can move
it up a little (when the screen is vertical), so it doesn't get hidden
behind the touch controls." And, later the same day, on iPhone Safari:
"sometimes touching the controls it looks like it 'selected' the canvas.
Can we disable that behavior?" Nothing else of the mobile play was to
change; the look stays Mat.

- **The glyphs are the rain's outlines** (`ddi_ui::touch_controls::glyphs`):
  the arrow and onigiri contours of `assets/ui/menu-rain.svg`, copied
  as path data (a test holds them to the SVG), stroked at the rain's
  heaviest weight (4.07 units of a 64-unit cell; a heavier 7 for the
  prompt's small cut) into an atlas the game rasterizes itself (white,
  coverage as alpha, each pixel's from its distance to the contour), and
  drawn as one textured quad a target, turned as its receptor. A
  translucent glyph is one shape: no two quads of it overlap, so no
  darker joints at any opacity
  (`every_look_draws_each_target_with_its_glyph_as_one_quad`). The
  atlas is no file: the play's 2D pass puts it in its painter
  (`ddi_shell::touch_controls::glyph_texture`), and both shells' menu
  painters answer its source, `ddi:touch-glyphs`, from the same bytes
  (`ddi_shell::touch_controls::serve`).
  What each layout's lanes draw: every arrow lane the rain's arrow; the
  diagonals (Solo's up-left and up-right, Pump's four corners, Techno's
  four) the same arrow turned 45°; Pump's center a square with the
  arrow's round corners (radius 8.75 units), in the same outline;
  Dancing☆Onigiri's onigiri (5, 7, 9A, 9B keys) the rain's onigiri; any
  other glyph a song's layout names (giko, iyo, a custom one) a ring.
- **The prompt's diagram shows them too**: each target of the cut
  carries its lane's glyph (the heavier weight), dark on the lit target,
  light on the others.
- **The hand on an upright screen** (`ddi_shell::play_view::upright_band`,
  `dance::Preview::set_upright`): on a screen taller than wide, a solo
  play's hand keeps its frame's place across (the settings' Hand frame)
  and takes, down the screen, the lowest band at least three quarters as
  tall as the tallest of those clear of the receptors' row, of the
  judgment down to the combo's foot, and of the touch controls while
  they show: under the combo, its mat 8 px above the Buttons (at the
  screen's foot with no controls showing); above a pad, which leaves too
  little under the combo, the band between the receptors and the
  judgment. In it the hand is whole and as large as the band allows, up
  to twice its frame's width (`UPRIGHT_ZOOM`, as behind the field).
  Two candidates were drawn: "moved up only" (never larger than before)
  and this one, built. Its size against before: on a phone at 390×844,
  1.11 times above the Buttons or between the receptors and the
  judgment above the pad, 1.62 times with no controls showing; on a
  tablet at 820×1180, 0.79 times above the Buttons and 0.71 above the
  pad (it stood behind them before, larger; the band under the combo
  is lower there), 1.07 times with none. "Moved up only" is the same
  but never above 1. A room's stage keeps the hand behind the field,
  its band now ending above the controls (`play_view::behind_field`'s
  `floor`). Sideways, and windows wider than tall: unchanged. Placement
  only: nothing of the hand's planning, pose, contacts or timing
  changed.
- **No selection during a play** (`web::touch::NoSelection`, the web
  shell): while a play is on, the page's root is unselectable and
  without the long-press callout (set on the element by the shell; the
  stylesheet is the first round trip's), `selectstart` is refused, a
  selection that comes anyway is cleared (`selectionchange`), and a
  finger's `touchstart` on the play's host is default-prevented (no long
  press, double tap or magnifier; the pointer events the lanes read
  still come). Gone when the play ends: the prompt's Start, the results
  and the menus' text fields are as before. `__DDI_DEBUG.touchSelection`
  counts what it refused. Before it, the one selectable text of the page
  during a play was the search engines' hidden title and description
  (`.sr-only`) beside the canvas: a selection of the page landed there,
  which is where an iPhone's long press or double tap could start one.
  The real check is the maintainer's phone: a long press on a button, a
  double tap on a button, two fingers close on one button.

## Round DE (2026-11-22): the receptors above the controls, and a size

The maintainer's two decisions of 2026-11-22: under Reverse on a screen
held upright the receptors stand above the controls (this plan's open
question "Reverse upright"), and the controls have a size.

- **Reverse upright: the receptors above the controls**
  (`ddi_render::FieldGeometry::lifted`, `RenderOptions::receptor_floor`,
  `ddi_shell::play_view::receptor_floor`). Only where the touch controls
  show, the screen is taller than wide and the play is in Reverse, the
  receptors' row stands just above them: its foot 8 px (`GAP`) above the
  controls' top, never above two fifths of the screen from its top
  (`RECEPTOR_LIFT_MOST`, which no control reaches today). The judgment and
  the combo follow it: they are placed from the field's middle, which is
  now the middle of the field above where the row stands
  (`FieldGeometry::middle`); so do the hold explosions and the Held and
  Let go words, the attack warning, the lane covers, the lyrics' keep-out,
  the quit button's side and the dancing hand's band (round TC2's
  `upright_band` and a stage's `behind_field` read the lifted rows). The
  life gauge stays at the top; the opponents' miniatures keep their own
  rows (they have no controls). Nothing else moves: normal scroll,
  sideways, a keyboard or a pad as the input source (the controls hidden),
  the legacy columns, and a layout whose lanes scroll both ways (its
  receptors at both ends) keep their geometry. `__DDI_DEBUG.receptors` is
  `{y, lifted}` (CSS pixels).

  What it costs, Single, on devices at their pixel ratios (a phone's
  arrow is 160 of its pixels at most, 53 CSS pixels at 3×; `cargo test -p
  ddi-shell --lib receptor_lift_table -- --ignored --nocapture`, which the
  browsers' `__DDI_DEBUG.receptors` matched: 698 and 453 on the 390×844
  phone): the receptors' row down the screen before and after (CSS
  pixels), the arrows of the field lost, and the reading time from the
  screen's top to the row at ×2.5 at 150 BPM (6.25 arrows a second):

  | screen | controls | row before → after | arrows lost | arrows read | at ×2.5, 150 BPM |
  |---|---|---|---|---|---|
  | 390×844 at 3× | Buttons | 764 → 698 | 1.2 | 14.3 → 13.1 | 2.29 → 2.09 s |
  | 390×844 at 3× | Pad | 764 → 453 | 5.8 | 14.3 → 8.5 | 2.29 → 1.36 s |
  | 360×640 at 3× | Buttons | 560 → 502 | 1.1 | 10.5 → 9.4 | 1.68 → 1.51 s |
  | 360×640 at 3× | Pad | 560 → 335 | 4.2 | 10.5 → 6.3 | 1.68 → 1.00 s |
  | 820×1180 at 2× | Buttons | 1060 → 1002 | 0.7 | 13.2 → 12.5 | 2.12 → 2.00 s |
  | 820×1180 at 2× | Pad | 1060 → 746 | 3.9 | 13.2 → 9.3 | 2.12 → 1.49 s |

  Above the Buttons it is about an arrow; above the pad, which takes 42%
  of the height, four to six arrows, and the smallest phone reads a
  second at ×2.5 and 150 BPM (still slower than the reading times many
  players choose; a slower speed gives it back). The sizes (below) move
  it with the controls: on the 390×844 phone Smaller and Larger Buttons
  718 and 673 (2.15 and 2.02 s), Smaller and Larger pads 522 and 429
  (1.57 and 1.29 s); the Larger pad on the smallest phone 285 (0.86 s).
  The dancing hand (round TC2's band) follows the rows: above the Buttons
  it keeps the band above the judgment as before; above the pad the
  judgment and the combo move up with the row, so the band left above
  them is shorter and the hand drawn there smaller (on the 390×844 phone
  564 of the target's pixels tall where it was 1030): placement only, its
  planning and motion unchanged.

- **A size** (`ddi_ui::touch_controls::Size`, `Choice::size`,
  `Settings::touch_size`, saved on the device as every setting is):
  Options → Controls, under "Touch screens", "Touch controls size":
  Smaller (0.8), Standard, Larger (1.25); the same row in the start
  prompt's play options on a touch screen (with Touch controls). Three
  words rather than a percent scale: it is a comfort choice, and a phone
  held upright leaves the buttons no room to grow wider anyway. Each group
  grows or shrinks from its corner at the screen's foot (the left group
  from its left corner, the right one from its right, a pad under the
  field from its foot's middle, a pad beside the field from its outer
  corner), within the safe area and the room its place has: a button at
  most the side's share of the width and two fifths of the safe height
  (`BUTTON_TALL`), a pad under the field at most half the height
  (`PAD_TALL_MOST`, the standard 42%), beside the field its room beside
  it. The rule that Buttons narrower than 44 px play the pad instead is
  decided at the standard size, so a size never changes the style played;
  Smaller never draws a target under 44 px where Standard does not (it
  stops there). The prompt's diagram follows: its cut of each control is
  drawn at the size's share of the standard one's, within its cell.

  Single's targets (`cargo test -p ddi-ui --lib sizes_by_choice_table --
  --ignored --nocapture`), width × height in CSS pixels, and where the
  controls begin down the screen:

  | screen | style | Smaller | Standard | Larger |
  |---|---|---|---|---|
  | phone 390×844 | Buttons | 68×81, from 753 | 84×101, from 733 | 84×127, from 707 |
  | phone 390×844 | Pad | tiles 87, from 557 | 110, from 488 | 118, from 464 |
  | phone 844×390 | Buttons | 90×90, from 290 | 113×113, from 267 | 141×141, from 239 |
  | phone 844×390 | Pad (beside) | 81, from 121 | 103, from 56 | 103 (no room), from 56 |
  | 360×640 | Buttons | 62×75 | 77×94 | 77×117 |
  | 360×640 | Pad | 64 | 81 | 98 |
  | tablet 820×1180 | Buttons | 96×96 | 120×120 | 150×150 |
  | tablet 820×1180 | Pad | 95 | 120 | 151 |

  Upright, Larger buttons grow in height alone (two a side already fill
  the width), and the pad grows to the width; on its side a phone's pad
  beside the field has no room to grow.

## Round PU (2026-11-24): one pointer handling for both shells

Which pointer holds what during a play (a lane's touch control, the
quit button, a stage's attack policy label, the music's volume, a
watched stage's fields) was written twice, the web's in
`crates/web/src/web/touch.rs` and desktop's in its shell, and the two
had drifted (round RFb fixed a bug in desktop's copy alone). It is one
platform-free `ddi_shell::play_pointers::PlayPointers` now: a press goes
`down(id, press, places, lane)`, a move `moved(id, pos, lane)`, a lift
`up(id, pos, cancel)`, and `release_all` lets go of everything; the
edges go to a `Sink` (the play's inputs, the volume's edges, stamped
with the shell's own time). Each shell keeps where its event lands (CSS
pixels of the canvas, logical pixels of the window), the lane under a
finger (the controls' plan; the web's legacy columns), its time stamps
(the web's event time, desktop's clock as the event is handled, as
before; under a test's own play clock, `Host::test_clock`, that clock,
`Shell::pointer_time`, so an offscreen test never measures a press
across two clocks) and what it does around a press (the web's pointer capture and
`preventDefault`, desktop's screens under a framed play).

The maintainer's rule (2026-11-24): "The web input behavior makes most
sense and unifying them would be best." Where the copies differed, the
web's is taken:

- **Every pointer on a target is counted**, the quit button and the
  policy label as the lanes: pressed by the first to land, released by
  the last to lift. Desktop kept one holder for the quit button and the
  label: a second finger took it from the first and pressed it again,
  which the play read as a second tap (two fingers on the quit button
  within half a second quit); the first finger's lift was nobody's.
- **A second finger on the volume** hands it its edges too; desktop
  gave the volume to the last finger down and the first one's moves
  and lift to nothing.
- **A pointer id that goes down again before its lift** is taken and
  does nothing; desktop pressed the quit button, the label or the volume
  again.
- **Hidden**: the window hidden (occluded) lets go of everything as the
  web's page does on `visibilitychange`; desktop let go on the focus
  lost alone.
- **A finger landing on the play away from the buttons** makes touch
  the input source whether touch controls are placed or not (desktop:
  only with controls; it never had the legacy columns).
- **A new play** starts with nothing held; desktop cleared the lanes'
  fingers alone, and kept a quit button's, label's or volume's holder
  from the play before.
- **A focus lost**: each target released once, in the order the web
  keeps its pointers (desktop: the lanes by number, then the quit
  button, then the label; one time stamp either way).

And one of desktop's: a drag on the volume cut off by the focus lost
ends where it last was (the web sent `[0, 0]`, a placeholder; the level
was already set while dragging, so nothing heard changes).

Already the same in both and kept: the order a press is tried in (the
volume, the quit button, the policy label, the skip plate, the watched
fields, the lanes, so the quit button wins over the label where they
overlap and the skip plate over a field); a press missing the volume
while its slider is open closes it and goes on; only the primary press
reaches the volume, the buttons and the fields (the web's other mouse
and pen buttons, desktop's other mouse buttons, never reach it); the
mouse presses no lane; a slide keeps its lane until the controls' dead
band says another (`Controls::lane_at`), and moving off every target
keeps it; a pointer that went down on nothing holds nothing and presses
nothing when it slides onto a lane; a cancel is a lift (the volume's
`Cancel` phase); a press on a watched field is a press alone, its lift
nobody's.

## The look: three candidates (open)

The pictures (the study's sheets, `q5-looks`) show each at rest, with
fingers held on two lanes, and autoplayed:

- **Mat** (the default for now): after the dancing hand's pad, which
  lights a tile in a bright hue of its own each contact, keeps it lit a
  moment and spills it onto the tiles around: dark mat tiles at half
  opacity with a thin light edge; a press lights the tile in a hue drawn
  per press (`ddi_character::appearance::tile_color`), a soft spill
  round it, kept 0.08 s after the release then faded over 0.2 s.
- **Glass**: a quiet white plate and outline; a press brightens both.
- **Glyphs**: the glyphs alone with a faint outline; a press fills a
  jade plate behind the glyph.

Under reduced motion a target is lit while held and goes out at once.

## The default (a decision for the maintainer)

The default style is Buttons. Every player on a touch screen who played
with the columns before now gets buttons at the bottom corners: a
change of what a finger does, which the maintainer should confirm
before this ships. The columns stay one choice away (Options → Controls
or the prompt).

## Sizes

CSS pixels to millimeters: a phone about 0.166 mm (153 CSS px an inch,
a 460 ppi panel at 3×), a tablet 0.192 mm (132 CSS px an inch, 264 ppi
at 2×), a desktop touch screen 0.265 mm (96 a nominal inch). Rhythm
play wants larger targets than a UI's 9 mm: 11 mm and more for buttons
was the aim, 7 mm (44 CSS px) the least drawn.

Each target's smallest side, in CSS pixels and millimeters (buttons are
taller than wide: 101 px on a phone):

| layout | drawn | phone 390×844 | phone 844×390 | 360×640 | tablet 820×1180 | tablet 1180×820 |
|---|---|---|---|---|---|---|
| Single | Buttons | 84, 14.0 | 113, 18.8 | 77, 12.8 | 120, 23.1 | 120, 23.1 |
| Single | Pad | 110, 18.3 | 103, 17.0 | 81, 13.5 | 120, 23.1 | 120, 23.1 |
| Solo, Pump single, Pump half-double, 5 keys | Buttons | 54, 8.9 | 113, 18.8 | 49, 8.1 | 120, 23.1 | 120, 23.1 |
| 7 keys | Buttons (the pad upright on phones) | pad 110, 18.3 | 95, 15.8 | pad 81, 13.5 | 92, 17.7 | 120, 23.1 |
| Double, Pump double, 9A | two pads | 54, 8.9 | 77 to 64, 12.7 to 10.6 | 49, 8.1 | 120, 23.1 | 75 to 48, 14.5 to 9.2 |
| Techno 8, 9B | one pad | 110, 18.3 | 77 to 70, 12.7 to 11.6 | 81, 13.5 | 120, 23.1 | 75 to 62, 14.5 to 11.9 |

Upright on a phone the buttons cover the field's bottom 111 px (its
last 1.7 of 11.5 rows of notes at Single pass under them, translucent);
the pad covers the bottom 42% (6.0 of 11.5 rows clear above it), its top
below the screen's middle so the judgment and the combo stay clear. On
a screen on its side the controls sit beside the field and cover none
of it. `cargo test -p ddi-ui --lib touch_controls::tests::sizes_table --
--ignored --nocapture` prints every layout and screen.

What keeps clear of the controls while they show: the quit button and
the volume (above the left group, `QUIT_ROOM` above a pad at the left),
a stage's opponents' miniature fields (their slots avoid the controls'
groups, `opponents::arrange_up_to`'s keep-outs).

## What does not fit, and open questions

- **Reverse upright**: decided and built in round DE (above): the
  receptors stand above the controls while they show.
- Five and six lanes on Buttons upright on a phone: 54 px (8.9 mm) wide
  each, 49 (8.1 mm) on the smallest; usable, under the aim; the pad is
  larger there (18.3 mm) and covers more of the field.
- The doubles upright: two pads in the width, tiles of 54 px (8.9 mm),
  49 (8.1 mm) on the smallest phone: functional, as the maintainer
  expected, not comfortable.
- A tablet on its side with ten lanes (pump double): the field leaves
  180 px a side, so the pads beside it have tiles of 48 px (9.2 mm on a
  tablet).
- The smallest phone (360×640): everything places and presses; seven
  lanes play the pad; the pad upright covers the bottom 42%.
- On a page loaded straight on a play (not from song select), the first
  tap on the prompt seemed to start nothing and the second to start.
  Round LO (2026-11-22) found the walk's first tap landing on the page's
  loader (`#boot-canvas`): the prompt says `ready` in `__DDI_DEBUG` a
  moment before the loader turns click-through and fades, so a tap then
  meets the loader, which a person still sees. The first tap once the
  prompt shows starts the play and its sound, in Chrome under
  `--autoplay-policy=document-user-activation-required` and in Firefox;
  the prompt rebuilt for the first touch (`Msg::TouchSeen`) does not
  spend it (`the_first_touch_starts_though_it_rebuilds_the_prompt`).

## Later

- The skin's own receptor art on the targets (the rain's outlines are
  drawn now): the noteskin draws receptors through its Lua on the field
  only; drawing it at the targets would go through the miniatures' way of
  drawing the skin elsewhere (`ddi_render::scene::Miniature`).
- A size adjustment: built in round DE (above), three sizes.
- Edge swipes (the browsers' back swipe, iOS's bottom edge): kept as the
  safe area today; to be adjusted after feedback.
- Remove the legacy columns once the maintainer decides.
