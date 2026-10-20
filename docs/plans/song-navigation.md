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

## Verification

Pure recognizer/motion tests cover singles, isolated and opposite pairs,
missing fourth taps, repeated navigation, device separation, held second
presses, acceleration/reversal/release, cancellation, explicit Start and
continuous motion through commit. UI harness tests assert that quick
opposite pairs emit no temporary song callbacks on keyboard or pad, and
that blur/search stops holds. Theme/widget tests cover wheel geometry,
compact navigation and existing pointer/flick behavior. Browser checks
exercise the built app through the same shared screen.
