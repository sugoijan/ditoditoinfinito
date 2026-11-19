# Song-select navigation (2026-10-20)

The maintainer calibrated this behavior in a separate, local timing tool.
The shared UI implements it for keyboard and controllers on desktop and
web. No new saved setting or noteskin parameter is involved.

| Parameter | Value |
| --- | --- |
| First-to-second press window | 260 ms |
| Quiet after the second release | 180 ms |
| Quiet after browsing to rearm a pair | 400 ms |
| Provisional distance at commit | 20% of one move |
| Completion after commit | 160 ms (420 ms total) |
| Hold's first repeat | 275 ms |
| Initial / maximum hold rate | 8 / 24 rows per second |
| Acceleration to maximum rate | 1.2 seconds, smoothstep |

## Recognition

`crates/ui/src/screens/select_navigation.rs` owns a deterministic recognizer.
All four arrows browse; Up/Left select easier charts and Down/Right harder.
The first tap after quiet is provisional. A matching second tap freezes
its visual preview; quiet after the second release confirms a difficulty
change. A same-direction third tap commits all three as navigation and
enters immediate browsing. Subsequent browsing taps each move one item.

An opposite third tap after a matching pair may begin another difficulty
pair, so the preview stays frozen. A matching fourth confirms the pair
chain after quiet; chart changes apply in order with normal ladder
clamping and no song-selection callbacks. `Up Up Down Down` therefore
never jumps to a temporary song. A missing fourth tap resolves all three
as navigation after the new tap window. Confirmed difficulty changes
immediately rearm another pair. Keys and pad slots stay separate; pad
reversals within the same slot can form opposite pairs.

Holds always mean navigation. They supersede provisional pairs, accelerate
smoothly to the cap, and stop on physical release. Reversing or resuming
an older held direction resets acceleration. Catch-up repeats are bounded
when frames stall. Browsing never adds extra steps to a physical tap.
Pack headers remain navigable and do not turn taps into chart changes.
A library of fewer than ten songs has none: its songs are one list.

`Screen::direction_edges` opts into physical `MenuRelease` and cancellation;
`Cx::menu_source` identifies the actual key or pad slot. A screen owning
its repeat removes that source from the ordinary UI repeater. Other
screens keep their existing repeat rules. Focus loss, overlays, chrome
focus, pointer input, search and other navigation cancel provisional/held
arrows. Enter or Tab commits a pending single (or resolves a complete
pair) before acting. Committed song notifications keep their input source
and existing gesture-bound media behavior.

## Motion and selection

The wheel's logical selection and its visual scroll are separate.
A single trajectory starts with the first press, passes through the
provisional point at 260 ms with continuous position/velocity/acceleration,
and ends at 420 ms. Commit changes selection, not the trajectory's clock.
A second tap freezes the current position. Confirmed pairs return it to
the same song; a navigation run retargets from the current motion.

`List::selection_scroll`, `select_without_motion` and `visual_scroll`
allow screen-owned motion without accidentally settling a provisional
selection. The virtual actor pool, stable keys, scrollbar, banners,
wheel/drag/flick physics and compact list's visibility rules remain in
the list widget. No song info, background or audio selection is changed
by a partial preview. Reduced motion keeps the same recognition while
removing glides.

The theme's `Look::wheel_focus` blends row height, banner/text geometry,
plate color, shadow and ring by visual distance. Selection does not restart
row tweens at commit. The plain look retains its Select/Deselect commands.

## Opening a pack (2026-10-22)

Start on a pack header (or a tap on the selected one) opens it and
closes the pack open before. The header stays where it shows at that
moment and the wheel glides on from there to the line, also when Start
comes while the wheel still moves to the header (Down, then Start at
once: the tap is committed first). Only the items around the header
change (`List::keep_item_at`). Before, opening cut the motion short and
the header reached the line in one frame, up to a row's jump, which read
as the list jumping when a pack opened. A song's preview movie
(`#PREVIEWVID`) has no part in it: it shows in the banner's place on the
song's side, and the wheel's rows keep their sizes. Checked offscreen on
desktop with a local library, a pack with preview movies and one without
opened at rest and mid-glide, and by a UI harness test.

## The search field from a controller (2026-10-22)

The arrows keep browsing the list as above; the header is reached only
past the list's top, so no tap, pair or hold changes meaning. A fresh
Up on the first row (the first pack header, or the first song where a
library has none) moves the focus to the search field, as a key's Up
did already; a held Up stops on the first row. From the field:

| Input | Not typing | Typing (a keyboard in hand) |
| --- | --- | --- |
| Start (Enter, A, a pad's Start) | starts typing | goes to the songs found, as Enter |
| Down | back to the list where it was | the same, the text and filter kept |
| Back (B, a pad's Back; Escape) | back to the list where it was | closes the suggestions, then stops typing, as Escape |
| Left / Right | along the header: Clear (while there is text), the mods, the volume, More | the same, typing stopped |
| Up | the app's notices, as from the rest of the header | nothing |

Clear (the × inside the field's right end) empties the search and
returns to the list; Back on it does too.

The quick volume sits between the search and More
(`ddi_ui::widgets::volume`): its icon shows the music's level (muted,
then one to three waves) and a vertical slider drops under it while the
mouse is over either, while it has the focus, or after a tap (touch has
no hover: the slider stays until a press elsewhere). A press on the icon
while the slider shows mutes, or brings back the level from before the
mute (the level the last mute kept anywhere, song select's, the start
prompt's or the play's, `kept_level`; the default level when muted since
the game started), so a mouse's click
always mutes and a touch's first tap only shows. With the focus, Up and
Down step it by 5%, Start mutes and Back returns to the list; a wheel
over it steps it too. It is the setting Options calls Volume (Music
volume until 2026-11-19: it was always every sound's, round AD): the
preview follows at once (`SelectCall::Volume`), the settings keep it
when a drag is released. The compact layout has none (a phone's player
reaches the volume on the start prompt and in the play instead,
`docs/plans/start-prompt.md`, "The music's volume", and
`docs/plans/play-hud.md`). A room's follower, who has no search, keeps the
quick volume in its place (round AB, 2026-11-19). Back on top of a screen
that changed the level (Options, the prompt, a play), it shows the new
level and tells the preview (`SelectCall::Volume`).
While typing, a keyboard's keys stay the field's (its arrows still move
through the suggestions or browse the songs found), so these rows only
change what a controller does. A browser starts typing only inside a key
or a tap (a phone's keyboard opens only then, `UiRequest::needs_gesture`):
there a controller's Start on the field says "Press / or tap the field to
type." in the footer and the focus stays, so Clear and the way back
still work; desktop types at once. There is no on-screen keyboard.
No new menu button was added: a standard pad's X is a lane, Y is
Select (the next style), and Start/Back are A/B's twins.

## Returning to Start

Unmodified Escape and Backspace map to the same menu Back action. Text
editing, dialogs and binding capture get the key first. Ordinary browsing
leaves for the silent Start menu in one press, retaining the selected
song/chart, expanded pack and filter. The Clear × inside Search
clears a filter explicitly; pack headers still toggle by activation.
Leaving cancels provisional arrows, held repeats and wheel motion before
the shell stops/cancels preview playback. The desktop header and compact
Back control lead to Start. Options and Credits return to their caller.
See [the start-menu contract](start-menu.md).

## In a room (2026-11-14)

A multiplayer room's song select (`select_room.rs`) comes back where
the picker left it, as singleplayer's does, with a place of its own:
`ddi_ui::state::place::ROOM_KEY`, kept for the session only (the web's
`sessionStorage`, so a stage's routes, a reload and the browser's Back
and Forward find it; desktop's run). The first room of a session starts
at singleplayer's place; every later one (a practice room made again,
an online room joined again) where the last room was. The room never
moves singleplayer's place or the settings' style (practice plays
Single alone; a room's style is its own).

- Only the picker's own list is a place (`SelectScreen::tell` reports
  none while following or joining): the song followed is nobody's.
- A pick keeps its song and chart as the place
  (`state::place::room_picked`, as singleplayer's play does), so the
  list comes back on the song picked after the stage, the next pick
  usually being near it.
- The picker's own list is kept through a change of hands
  (`RoomMode::library`, `Place`: the song, the chart, the style, the
  open pack, the search); the wheel's scroll follows the selected song,
  as everywhere in song select.
- A follower's screen tells nothing of its own list before the song
  followed replaced it (`SelectScreen::room_unfollowed`): no preview of
  their own song starts for a frame.
- The footer's Options and Calibrate open over the room and come back
  to it as it was, on both shells (desktop's did nothing in a room
  before); the announcer studio is reached through Options as ever. The
  header has the room's Rules and Players in the mods' place, so the
  mods' pills are not a way to Options in a room.

The table of what came back where before this, row by row:
`docs/plans/multiplayer-prototype-progress.md`, "Feedback round E2".

## Favorites (2026-11-18)

The maintainer: "let's add a 'favorite' feature, so that songs can be
individually favorited (maybe with a heart icon) and show up in a
'Favorites' virtual pack at the top of the packs list."

- **A song is a favorite or not**, whatever its charts
  (`ddi_ui::state::favorites`). A favorite is the song's library id,
  which both shells give alike: a bundled song's own id, an imported or
  read-in-place one `ddi_library::pack::song_id` of its pack's name and
  its folder's, the id the song's offset and the list's place keep too.
  It survives a restart, a library read again, the chart editor's saves
  and a pack removed and imported again under the same name; a pack or a
  song folder renamed is a new song. A share's songs (`shared:<id>:<n>`)
  are never favorites.
- **Kept** per device under `ddi.favorites.v1` in local storage (the
  web's `localStorage`, desktop's `storage.json`), apart from the
  settings: the ids in the order they were added, about 21 kB at 1,000
  imported songs. Read once and kept in memory
  (`ddi_platform::storage::generation` says when another store was
  installed), written at each change. A favorite whose song is not in
  the library now is kept, and shows again with its song.
- **The heart** (`ddi_ui::heart`, the look's `Heart` component, its
  colors `Heart` and `Heart.Empty`): a vector shape of our own, two
  stadiums turned an eighth, each cut at the heart's middle by its own
  clip, so the halves never overlap and a faded heart (a far row, a
  screen leaving) reads as whole; filled for a favorite, an outline
  otherwise. On the song's side it sits at the right of the title (the
  look's `FavoriteSlot` in `SongInfo`, 24 px tall, a phone's and a room's
  short side 18 px) and is a button (`Favorite`); turned, it pops a fifth
  of a second (at once under reduced motion). A favorite's row carries a
  small filled heart over its banner's upper left corner (the look's
  `Fav`, on a darker heart so it reads on any art); other rows nothing.
- **Inputs**: a click or a tap on the heart; F (by its physical place,
  `KeyF`, not while typing a search), its word in the footer on a song's
  row ("Favorite", "Unfavorite"; a phone's layout has no footer); More's
  "Add to favorites" or "Remove from favorites", a controller's and a
  phone's way. A change is at once, asks nothing and says nothing. They
  act on the song shown (on a pack's header, the song still shown beside
  it).
- **The Favorites pack**, a pack of the screen's own at the top of the
  list, above every pack, while one of its songs is in the library: each
  favorite once, from whatever pack it lives in (listed there too),
  sorted as the shells sort a pack's songs (by title, ignoring case). Its
  header has the heart in the cover's place and its count. It opens,
  closes, takes the arrows and gestures and keeps its place as a pack
  does: the place's open pack is `favorites`
  (`ddi_ui::state::place::FAVORITES_KEY`), so a song opened from it comes
  back in it; the closed packs the place keeps are real packs alone.
  Nothing a pack's header offers (Manage, Share…) is offered on its
  header; a song in it offers its own song's. A song taken out of it
  while it is open gives the focus to the next of its songs, its header
  after the last, and, the pack gone, to the first pack's header, nothing
  open. A library of fewer than ten songs has no packs, so no Favorites
  pack: its rows' and the song's hearts alone.
- **Searching** leaves the Favorites pack out: a song shows once, under
  its own pack, with its heart, so the songs found are the same rows as
  without favorites; the search cleared, the pack is back as it was.
- **Where**: singleplayer on both shells, and a room's song select for
  whoever chooses songs (their own library; More's item first among the
  room's); a follower's side has no heart and no pack, nor do the
  shared-songs mode (`#/s/<id>`, `--shared`) and a song that exists only
  as a room's share.

Checked by `crates/ui/src/screens/favorites_tests.rs` (each input, the
pack's order, place and emptying, search, a share, a room's picker and
follower, what is kept), `crates/ui/src/state/favorites.rs` and
`crates/desktop/tests/favorites_offscreen.rs` (pictures, wide and on a
phone, English and Japanese).

## The skill radar's side (2026-11-18, round Z1)

With Options → Display → Skill radar On, the wide layout's song side has
no chart ladder: a row of chart buttons in its place (the multiplayer
host's, `chart_buttons`: a place for each of the style's charts, five
at least and eight at most, a window round the chosen one past eight),
the chosen chart's lines under it on the left
and its skill radar on the right (`docs/plans/msd-ratings.md`, "Round
Z1"). The gestures are the ladder's: Up/Left twice an easier chart,
Down/Right twice a harder one, Tab the style, a tap on a button picks it
and a tap on the chosen one plays; the buttons and the style tabs are the
side's focusables; the footer's hints are unchanged. With it Off, or on a
phone, or in a room, the song side is as described above.

## The footer's hints (2026-11-18)

Since the favorites' F a footer could hold more hints than fit beside its
links ("Favorite" ran under "Manage…" on an imported song at 1280×720).
The footer (`ddi_ui::screens::footer_hints`) now never lets them meet:
where the hints and the note do not fit before the links, the hints with
a rank leave, the highest first (`FooterHint::drop`), then the note is
cut with an ellipsis, then hints without a rank leave from the end (the
first kept). Song select's order (`footer_drop`): the editor's E first,
then the plays' P, a room's players' L, Delete's managing, the heart's F,
the style's Tab, the search's /; the song's and the chart's arrows,
Enter, Escape and a room's T stay. Checked by the theme's
`song_selects_footer_hints_never_overlap` (the real look, English,
Japanese and `qps-ploc`, 1280×720, 1024×576 and a phone sideways, which
has no footer; a bundled song, an imported one, a pack's header, the
Favorites pack, with and without P and E, a practice room's picker).

## Verification

Pure recognizer/motion tests cover singles, isolated and opposite pairs,
missing fourth taps, repeated navigation, device separation, held second
presses, acceleration/reversal/release, cancellation, explicit Start and
continuous motion through commit. UI harness tests assert that quick
opposite pairs emit no temporary song callbacks on keyboard or pad, and
that blur/search stops holds; a pad reaches the search field from the
first row, types where the platform allows, clears and goes back to the
list where it was, in a browser and on desktop. Theme/widget tests cover wheel geometry,
compact navigation and existing pointer/flick behavior. Browser checks
exercise the built app through the same shared screen.
