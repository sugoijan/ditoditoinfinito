## Options: rows of settings, each a label on the left (about 24 Latin
## letters wide), its value on the right and a description below when it
## is focused. A row's values are drawn side by side when they fit the
## row (about 30 letters together), else one at a time with arrows. A
## description is a sentence or two, wrapped in the panel under the rows
## (about 90 letters a line, six lines at most).

# The row choosing the UI's language. Keep the English word in
# parentheses after a translation, for someone who cannot read the
# language it is set to.
options-language = Language

# The Language row's first choice: the system's language, named in
# parentheses.
# Variables:
#   $language (String) - the language that gives, written in itself ("English", "日本語")
options-language-system = As the system ({ $language })

options-language-description = The language of the menus. Judgments and scores read the same in every language.

## The screen: its header, the notice beside it, its footer's key hints
## (a word after the keys' caps, about 10 letters) and the field
## preview's parts.

# The screen's name in the header, in small capitals.
options-header = OPTIONS

# The screen's name on a phone's screen, at the top left.
options-title = Options

# Beside the header while nothing else is said: settings are saved as
# they change (there is no Save button).
options-saved = Saved as you change them

# Key hint: Up and Down move between the rows (the options).
options-hint-option = Option

# Key hint: Left and Right change the focused row's value.
options-hint-change = Change

# Key hint: P changes the layout the field preview shows.
options-hint-preview = Preview

# Key hint: Tab moves to the next section of rows.
options-hint-section = Section

# The More menu's title (the screen's further actions).
options-more-title = Options

# In the More menu: changes the layout the field preview shows.
options-more-preview-layout = Change preview layout

# The button above the field preview that changes its layout (about 30
# letters wide, arrows each side).
# Variables:
#   $style (String) - the layout shown, as the style tabs name it ("Single")
options-preview-layout = Preview: { $style }

# Under the field preview, in its line of tags ("Infinito · Single · ×2.5
# · Reverse"): the arrows scroll down.
options-preview-reverse = Reverse

# In the field preview's line of tags: the dancing hand is shown in its
# standing pose.
options-preview-standing-hand = Standing hand

# Under the field preview while it shows the whole play with the dancing
# hand: what it is (a small caption).
options-preview-caption = Standing hand · layout preview

# A scroll speed: the chart's tempo times this.
# Variables:
#   $speed (Number) - the multiplier (2.5)
options-speed-value = ×{ $speed }

# A value in percent (volume, heights, sizes, brightness).
# Variables:
#   $percent (Number) - the percentage (69)
options-percent = { $percent }%

# A value in milliseconds, with its sign (an offset).
# Variables:
#   $ms (String) - the number with its sign ("+12", "-5", "+0")
options-ms = { $ms } ms

# A size in pixels.
# Variables:
#   $px (Number) - the size
options-px = { $px } px

# A time in seconds.
# Variables:
#   $seconds (Number) - the seconds
options-seconds = { $seconds } s

# A loudness in decibels, beside the volume.
# Variables:
#   $db (Number) - the decibels, 0 or below (-10)
options-db = { $db } dB

# Beside a volume at zero: no sound at all.
options-muted = muted

## Sections: the rail on the left (about 18 letters) or the tabs on a
## phone.

options-section-scroll = Scroll
options-section-notes = Notes
options-section-rules = Rules
options-section-timing = Timing
options-section-sound = Sound
options-section-display = Display
options-section-arrows = Arrows
options-section-controls = Controls
options-section-data = Data

# Desktop: the song folders the game reads.
options-section-songs = Songs

# Sharing songs by link (a hidden feature).
options-section-sharing = Sharing

## Headings over groups of rows, in small capitals.

options-heading-simplify = SIMPLIFY
options-heading-convert = CONVERT
options-heading-danoni = DANCING☆ONIGIRI RULES
options-heading-known-devices = KNOWN DEVICES
options-heading-backgrounds = BACKGROUNDS
options-heading-menus = MENUS
options-heading-diagnostics = DIAGNOSTICS
options-heading-keyboard = KEYBOARD
options-heading-controllers = CONTROLLERS

## Scroll.

options-speed = Speed
options-speed-description = How fast the arrows scroll: the chart's tempo times this.

options-speed-source = Charts with their own speed

# The scroll speed set above.
options-speed-source-mine = Mine

# The chart's own scroll speed.
options-speed-source-chart = The chart's

options-speed-source-description = Some charts set their own scroll speed (Dancing☆Onigiri works): play them at the speed above, or at theirs.

options-reverse = Reverse
options-reverse-description = Arrows scroll down to receptors at the bottom.

options-scroll-action = Scroll action
options-scroll-action-normal = Normal
options-scroll-action-boost = Boost
options-scroll-action-brake = Brake
options-scroll-action-wave = Wave
options-scroll-action-description = Boost speeds the arrows up near the receptors, Brake slows them down, Wave sways them.

## Notes: what the chart's notes become.

# Moves the arrows to other lanes.
options-turn = Turn
options-turn-mirror = Mirror
options-turn-left = Left
options-turn-right = Right
options-turn-shuffle = Shuffle
options-turn-description = Moves every arrow to another lane, the same way for the whole chart; Shuffle makes a new lane order each play.

# How the notes show on their way.
options-appearance = Appearance
options-appearance-normal = Normal
options-appearance-hidden = Hidden
options-appearance-sudden = Sudden
options-appearance-both = Both
options-appearance-stealth = Stealth
options-appearance-description = Hidden: notes vanish before the receptors. Sudden: they appear late. Stealth: no notes at all.

# A panel covering part of the lanes.
options-lane-cover = Lane cover
options-lane-cover-sudden = Sudden+
options-lane-cover-hidden = Hidden+
options-lane-cover-both = Both

# The lane cover's description, played on a keyboard.
options-lane-cover-description-keys = A panel over the lanes. Sudden+ covers the end the notes come from, Hidden+ the end they go to. In play, Page Up and Page Down move it and Enter hides or shows it.

# The lane cover's description, played on a controller with a Start
# bound.
# Variables:
#   $start (String) - the controller's Start control, as its button is named ("Menu", "Options")
options-lane-cover-description-pad = A panel over the lanes. Sudden+ covers the end the notes come from, Hidden+ the end they go to. In play, hold { $start } and press an up- or down-arrow lane binding to move it; tap { $start } to hide or show it.

# The lane cover's description, played on a controller with no Start
# bound.
options-lane-cover-description-unbound = A panel over the lanes. Sudden+ covers the end the notes come from, Hidden+ the end they go to. Bind a controller Start control to move or toggle the cover during play.

options-cover-sudden-height = Sudden+ height
options-cover-sudden-description = How much of the lane Sudden+ covers. A play keeps the height you leave it at.
options-cover-hidden-height = Hidden+ height
options-cover-hidden-description = How much of the lane Hidden+ covers. A play keeps the height you leave it at.

# Under SIMPLIFY: which notes are kept.
options-cut = Notes
options-cut-all = All
options-cut-quarters = Quarters
options-cut-eighths = Quarters and eighths
options-cut-description = Leaves out the faster notes. Simplified plays are marked as assisted on the results.

# Dancing☆Onigiri charts offered without their onigiri (the middle lane's
# notes).
options-simplify-onigiri = Without onigiri
options-simplify-onigiri-description = Adds separate DanOni charts to Single, Solo, Double and Techno 8-panel, without the onigiri notes. Only these versions are marked as assisted; the original charts stay available.

# Pump charts offered without their center notes.
options-simplify-center = Without center

# Only where the song has no chart of that difficulty.
options-simplify-center-missing = Where missing
options-simplify-center-all = All
options-simplify-center-description = Adds Pump charts to Single and Double without the center notes, each pad turned an eighth clockwise, marked as assisted. Where missing: only at difficulties the song has no chart of there.

# The most panels pressed at once.
options-at-once = At once
options-at-once-any = Any
options-at-once-two = Up to two
options-at-once-one = One
options-at-once-description = The most panels pressed at once, holds included: Up to two keeps jumps but leaves out arrows meant for a hand or a keyboard, One leaves out jumps too. The leftmost arrows go first.

# Leaves out combinations too wide to reach.
options-no-stretch = No stretch
options-no-stretch-description = Leaves out wide combinations on Double, Pump half-double/double and DanOni 9A. Existing holds stay; new leftmost notes go first. Marks the play assisted.

options-no-holds = No holds
options-no-holds-description = Holds and rolls become single taps.

# Pump single charts converted to Dancing☆Onigiri's 5 keys.
options-convert-pump-single = Pump on 5 keys
options-convert-pump-single-description = Adds Pump single charts to 5 keys: the center on the onigiri, the corners turned an eighth clockwise. Marked as converted, played by the original's rules.

# Dancing☆Onigiri's 5-key charts converted to Pump single.
options-convert-five-keys = 5 keys on Pump
options-convert-five-keys-description = Adds DanOni 5-key charts to Pump single: the onigiri on the center, the arrows turned onto the corners. Marked as converted, played by the original's rules.

## Rules: how plays are judged.

options-ruleset-mode = Rules for each chart
options-ruleset-mode-ruleset = The ruleset below
options-ruleset-mode-game = The chart's game

# Dancing☆Onigiri's rules for every chart.
options-ruleset-mode-danoni = Dancing☆Onigiri
options-ruleset-mode-description = The ruleset below for every chart, the rules of the game each chart was made for (Dancing☆Onigiri works and FFR levels by theirs, the rest by the ruleset below), or Dancing☆Onigiri rules for every chart.

options-ruleset = Ruleset

# A ruleset this game follows only approximately.
# Variables:
#   $ruleset (String) - its name, as the games name it ("DDR A")
options-ruleset-approximate = { $ruleset } (approximate)
options-ruleset-description = How timing is judged and scored: DDI, this game's own rules, or the rules of the game named.

# DDI, this game's own ruleset: its options show under it when it is the
# ruleset chosen. Its timing difficulty: how wide the timing windows are
# and how much life a mistake costs (not a chart's difficulty). A row's
# label: about 20 letters.
options-ddi-difficulty = Timing difficulty
options-ddi-easy = Easy
options-ddi-normal = Normal
options-ddi-hard = Hard
options-ddi-difficulty-description = Easy widens every timing window and takes a fifth of the life for a mistake; Hard narrows them and takes twice as much. Bests are kept apart for each.

# Boo: DDI's name for a press that hits no note (a judgment's name, the
# same in every language).
options-ddi-boos = Boos
options-ddi-boos-description = On, a press that hits no note is a Boo: it costs life and points and keeps the play from a full combo's lamp. Never while getting off a mine, readying a lift, in a locked lane or in a long break. Bests are kept apart with Boos on.

options-ddi-practice = Practice
options-ddi-practice-description = On, an empty life does not end the play: it goes on to the end with the life at zero, and still counts as failed.

# What the play and the song list show of a DDI score: the score (points
# that grow with the chart's length, several million on a long chart),
# or the percent. The results show both.
options-ddi-score = Score display
options-ddi-score-score = Score
options-ddi-score-percent = Percent
options-ddi-score-description = What a play and the song list show of the score: the points, which a longer chart gives more of, or the percent. The results show both.

# A row under DDI's rules (its label, then its two values, then the note
# under it): whether the judgment words a play shows ("Glorious", "Too
# late") are written in the language chosen for the menus or in English.
# The values are short buttons, about 16 letters.
options-ddi-words = Judgment words
options-ddi-words-localized = In your language
options-ddi-words-english = English
options-ddi-words-description = The words a play shows for each hit, in the language of the menus or in English. Only the words change: never how a hit is judged.

# A row with one button: every kept play judged again under the rules
# chosen above. Its note says how many plays are kept.
options-rejudge = Kept plays
options-rejudge-button = Judge again
options-rejudge-description = Judges every kept play again from its presses under the rules chosen now, so that its score counts for the bests here; what it came to as played is kept too. A play judged again did not react to these rules: to a lane locked by a mine, or to Boos.

options-holds = Holds

# Holds as each chart's game has them.
options-holds-by-game = By game

# Holds as dance games have them: one result at their end.
options-holds-dance = Dance

# Holds counted at steady ticks while held (as Pump has them).
options-holds-ticked = Ticked
options-holds-description = Ticked holds count at steady ticks while held, each adding to the combo and the score; dance-style holds give one result at their end. By game: ticked on pump charts. Dancing☆Onigiri charts keep their own.

# When an empty life bar fails the play.
options-fail = Fail
options-fail-immediate = Immediate
options-fail-delayed = Delayed
options-fail-at-end = Fail at end
options-fail-description = When an empty life bar fails the play: at once, ending the song; at once but playing on to the end; only if it is still empty at the end; or never. The results say when the bar emptied. FFR's rules always fail at once.

options-life-difficulty = Life difficulty
options-life-difficulty-description = Higher drains the life bar faster and refills it slower (StepMania's levels; 4 is the ruleset's own). DDR A's gauge and FFR's life keep their own.

options-judge-difficulty = Judge difficulty
options-judge-difficulty-description = Higher narrows every timing window (StepMania's levels; 4 is the ruleset's own, Justice the narrowest). Bests are kept apart for each level. FFR's rules keep their own windows.

options-life-type = Life
options-life-type-bar = Bar
options-life-type-battery = Battery
options-life-type-description = The ruleset's life bar, or a battery of lives: one lost for each step worse than a Great, each mine stepped on and each hold let go. Not under FFR's rules.

options-lives = Battery lives
options-lives-description = How many lives the battery starts with.

# DDR A's life gauge (its other choices keep DDR's names: LIFE4, RISKY, FLARE).
options-ddr-gauge = DDR gauge

# DDR A's normal gauge.
options-ddr-gauge-normal = Normal
options-ddr-gauge-description = DDR A's gauge: LIFE4 has four lives, RISKY one. FLARE never refills and keeps its level as a rank; FLOATING FLARE drops a level when one empties.

# Dancing☆Onigiri's life gauge (its choices keep Dancing☆Onigiri's names:
# Original, Light, NoRecovery…).
options-danoni-gauge = Gauge

# The first gauge the chart offers.
options-danoni-gauge-first = The chart's first

# A Dancing☆Onigiri gauge that fails below a border.
# Variables:
#   $gauge (String) - its name, as Dancing☆Onigiri names it ("Normal", "Easy", "Hard")
options-danoni-gauge-border = { $gauge } (border)
options-danoni-gauge-description = Used for the charts played by Dancing☆Onigiri rules. A chart may offer its own gauges; one it does not offer plays its first.

# How wide Dancing☆Onigiri's timing windows are (its choices keep
# Dancing☆Onigiri's names).
options-danoni-range = Judge range
options-danoni-range-description = How wide the judgment windows are under Dancing☆Onigiri rules.

# Dancing☆Onigiri's Excessive judgment: early stray presses.
options-danoni-excessive = Excessive
options-danoni-excessive-chart = As the chart says
options-danoni-excessive-description = Early stray presses cost life.

## Timing: offsets.

options-calibration = Calibration
options-calibration-description = Measures your audio and display offsets by playing along.

options-audio-offset = Default audio offset
options-audio-offset-description = For audio without a profile. Positive: you hear the music later than the clock says (typical for Bluetooth).

options-visual-offset = Default visual offset
options-visual-offset-description = For displays without a profile. Positive: arrows are drawn as if the song were slightly ahead.

# A sound output the game has met, with its offset.
# Variables:
#   $device (String) - the output, as devices-audio* word it ("48 kHz · ~10 ms output latency")
options-profile-audio = Audio: { $device }

# A display the game has met, with its offset.
# Variables:
#   $device (String) - the display, as devices-display* word it ("1512×982 (2×)")
options-profile-display = Display: { $device }

# Beside a device's offset: measured by calibration.
options-profile-calibrated = calibrated

# Beside a device's offset: the default, not measured.
options-profile-default = default
options-profile-description = Offsets are kept per audio path and per display; the game picks the matching one when a song starts.

# Button: forgets a device and its offset.
options-profile-forget = Forget
options-profile-forget-description = Forgets this device and its offset: next time, it plays on the default, and the game suggests calibrating.

## Sound.

options-volume = Music volume
options-volume-description = How loud the music plays (equal steps are about equal in loudness).

# Plays a song's preview to hear the volume.
options-sample = Sample
options-sample-play = Play a sample
options-sample-stop = Stop
options-sample-description = Plays a song's preview at this volume.

options-assist-clap = Assist clap
options-assist-clap-description = A clap on every row of notes, right on the beat: to hear where the steps fall.

options-mine-sound = Mine sound
options-mine-sound-description = A sound when you step on a mine or a shock arrow.

# A voice that comments on the plays.
options-announcer = Announcer

# Variables:
#   $platform (String) - web or desktop
options-announcer-description = { $platform ->
    [web] A voice that comments on your plays and calls your grade: one that comes with the game, or an announcer pack (StepMania's, or its forks') imported with Add songs.
   *[desktop] A voice that comments on your plays and calls your grade: one that comes with the game, or an announcer pack (StepMania's, or its forks') in a song folder's Announcers folder.
}

options-announcer-volume = Announcer volume
options-announcer-volume-description = How loud the announcer speaks, as a share of the music volume (muting the music mutes it too).

# The imported announcer chosen (desktop: one made in the announcer
# studio), to remove.
options-announcer-remove = This announcer
options-announcer-remove-button = Remove
# Variables:
#   $platform (String) - web or desktop
options-announcer-remove-description = { $platform ->
    [web] Removes the imported announcer from this browser.
   *[desktop] Deletes the announcer you made in the announcer studio.
}

# Opens the announcer studio, to make an announcer of one's own voice.
options-announcer-studio = Your own announcer
options-announcer-studio-button = Make one…
options-announcer-studio-description = Opens the announcer studio: bring recordings of your voice, hear each line where the game says it, then use it here or share it.

## Display.

# Where the lanes are on the screen.
options-field-place = Field position
options-field-place-center = Center
options-field-place-left = Left
options-field-place-right = Right
options-field-place-near-left = Near left
options-field-place-near-right = Near right
options-field-place-description = Where the lanes are on the screen: in the middle, or in the middle of its left or right half.

options-show-deltas = Timing error of each hit
options-show-deltas-description = Shows how many milliseconds early or late each hit was.

# A hand that dances the chart beside the field. Its description names
# the styles it plays as common-style-* name them.
options-dancing-hand = Dancing hand
options-dancing-hand-description = A hand dances along to charts in Single, Solo, Pump single, Techno 8-panel, 5 keys, 7 keys and 9B keys (Oni on the center tile). Uses at most two simultaneous presses, including holds.

# Where the field and the dancing hand's view go together.
options-hand-layout = Field + hand layout
options-hand-layout-follow = Follow field
options-hand-layout-right-left = Field right / hand left
options-hand-layout-left-right = Field left / hand right
options-hand-layout-center-right = Field center / hand right
options-hand-layout-center-left = Field center / hand left
options-hand-layout-near-right-left = Field near right / hand near left
options-hand-layout-near-left-right = Field near left / hand near right
options-hand-layout-description = Places both views together. Changing Field position returns to Follow field. The preview shows the hand's standing reference pose.

options-hand-frame-width = Hand frame width
options-hand-frame-width-description = Width of the hand's camera frame as a share of the screen. It may extend behind the note band.

options-hand-frame-height = Hand frame height
options-hand-frame-height-description = Height of the hand's camera frame, aligned to the bottom with a small margin.

options-ghost = Ghost of your best
options-ghost-description = Shows, under the score, how far ahead of your best play of the chart you are, or behind it.

options-receptor-snap = Receptor snap
options-receptor-snap-description = Draws a note exactly on the receptor in its closest frame.

options-lyrics = Lyrics
options-lyrics-description = Shows the lyrics of songs that have them (Dancing☆Onigiri works, songs with a lyrics file).

options-bg-brightness = Background brightness
options-bg-brightness-description = How bright songs' backgrounds are behind the field; off shows none.

options-video = Background videos
options-video-auto = Automatic
options-video-description = Videos play behind the field in time with the music; automatic turns one off for the rest of a song when it costs frames. Off never loads the decoder.

# StepMania's random movies (a RandomMovies folder).
options-random-movies = Random movies
options-random-movies-description = Videos from a RandomMovies folder play behind songs without background changes, a new one every four measures, as StepMania plays them.

# A background drawn from the music.
options-generated-background = Generated background
options-generated-background-bars = Spectrum bars
options-generated-background-kaleidoscope = Kaleidoscope
options-generated-background-field = Field and lines
options-generated-background-rain = Outline rain
options-generated-background-description = Drawn from the music in place of a missing song background (not for Dancing☆Onigiri works, whose layers are their background). Off shows the plain ground.

options-generated-over-pictures = Over song backgrounds
options-generated-over-pictures-description = Draws the generated background over a song's own background picture too (the kaleidoscope folds the picture).

# The generated background's palette, by names of colors.
options-generated-palette = Generated colors
options-generated-palette-ink = Ink
options-generated-palette-dusk = Dusk
options-generated-palette-moss = Moss
options-generated-palette-ember = Ember
options-generated-palette-mono = Mono
options-generated-palette-description = The generated background's colors.

# The shape backgrounds fill (4:3, 16:9, …).
options-background-frame = Background frame
options-background-frame-auto = Automatic
options-background-frame-screen = Whole screen
options-background-frame-description = Backgrounds and videos fill this shape, as on the screen they were made for (automatic: the song's video, else its background); the rest shows a blur of them.

# Darkens the background behind the lanes.
options-field-filter = Field filter
options-field-filter-description = Dims the song's background behind the lanes.

options-reduced-motion = Reduced motion

# As the system's accessibility setting says.
options-reduced-motion-system = As the system
options-reduced-motion-description = No slides in the menus, and their art held still.

# The mouse pointer in the menus.
options-cursor = Cursor

# The game's animated arrow (Infinito is the game's own art's name).
options-cursor-infinito = Infinito arrow
options-cursor-pointer = Pointer tip
options-cursor-system = System
options-cursor-description = Animated Infinito artwork, or your system cursor. Text and drag cursors keep their system shapes.

options-cursor-size = Cursor size
options-cursor-size-description = Artwork size in logical pixels; the system cursor uses your system's size.

options-start-countdown = Start countdown
options-start-countdown-description = A song chosen in song select starts by itself after this long. Enter (or Start) starts it at once, any other key or a tap waits.

options-graphics = Graphics
options-graphics-auto = Automatic

# The fallback renderer, by its name.
options-graphics-webgl2 = WebGL2
options-graphics-description = Automatic: WebGPU where it works, else WebGL2. Takes effect when the page is loaded again.

options-debug = Debug overlay
options-debug-description = Shows the backend, frames a second, clock drift and timing error in play.

# Plays a bundled song by itself, to check the game runs well.
options-demo = Autoplay demo
options-demo-play = Play

# Plays it with the fallback renderer (WebGL2 is its name).
options-demo-webgl2 = On WebGL2
options-demo-description = Diagnostics: a bundled song played by itself (WebGL2 forces the fallback renderer).

## Arrows: the noteskin (the arrows' art) and its options.

options-noteskin = Arrows

# A noteskin whose script is switched off for this session.
# Variables:
#   $skin (String) - the noteskin's name ("Infinito")
options-noteskin-lua-off = { $skin } (its Lua is off)

# Variables:
#   $platform (String) - web or desktop
options-noteskin-description = { $platform ->
    [web] Infinito: the game's own arrows, its options below. Or a bundled or an imported noteskin.
   *[desktop] Infinito: the game's own arrows, its options below. Or a bundled noteskin, or one from the song folders.
}

# The web: the imported noteskin chosen, to remove.
options-noteskin-remove = This noteskin
options-noteskin-remove-button = Remove
options-noteskin-remove-description = Removes the imported noteskin from this browser.

# The noteskin for the Pump styles (five panels).
options-pump-noteskin = Pump arrows

# The same noteskin as the Arrows row above.
options-pump-noteskin-same = As above
options-pump-noteskin-description = The noteskin the pump styles are drawn with: as above, or a pump noteskin (its own art for each panel).

options-pump-noteskin-remove = This pump noteskin
options-pump-noteskin-remove-button = Remove
options-pump-noteskin-remove-description = Removes the imported pump noteskin from this browser.

# The layout the field preview shows the arrows on.
options-preview-on = Preview on
options-preview-on-description = The style the preview shows the arrows on (the style chosen until you pick another; not kept).

## Controls: keys and controllers.

# The style whose bindings the rows below show.
options-bind-layout = Bindings for
options-bind-layout-description = Each device keeps its own bindings for each style.

# The same, for a two-pad style (Double) that two controllers can share.
# Variables:
#   $one (String) - the one-pad style each controller plays its half with ("Single")
options-bind-layout-description-pads = Each device keeps its own bindings for each style. With two controllers, each plays one pad of this style with its { $one } bindings (the one connected first on the left), unless it has bindings of its own for this style.

# Binds whichever device presses first.
options-bind-any = Any device
options-bind-any-button = Bind…
options-bind-any-description = Press a key or a controller's button, then each lane in turn. Controllers show up once one of their buttons has been pressed.

options-keyboard = Keyboard
options-keyboard-bind = Bind
options-keyboard-reset = Reset keys

# Beside a device's bindings: the default ones.
options-bindings-defaults = defaults

# Beside a device's bindings: ones the player set.
options-bindings-saved = saved
options-keyboard-description = Bind asks for each lane in turn; Reset keys puts this style's defaults back.

# A lane's row: takes one more key for it.
options-key-add = Add key

# A lane's row: unbinds its keys.
options-key-clear = Clear
options-key-description = Add key takes the next key you press for this lane (Escape cancels); a key plays one lane only.

options-fast-pad-poll = Poll every millisecond
options-fast-pad-poll-description = Polls controllers every millisecond during play: smoother timing, more CPU.

# Where controllers cannot be read.
options-pads-none = Controllers
options-pads-none-note = not supported here

# Variables:
#   $platform (String) - web or desktop
options-pads-none-description = { $platform ->
    [web] This browser does not read controllers.
   *[desktop] Controllers are not read here.
}

# A controller's state, before whether it is connected ("saved ·
# connected"): it plays one pad of a two-pad style with another
# controller.
# Variables:
#   $style (String) - the one-pad style whose bindings it plays with ("Single")
options-pad-one-pad-each = one pad each, { $style } bindings

# The same, a controller alone playing one pad.
# Variables:
#   $style (String) - the one-pad style whose bindings it plays with ("Single")
options-pad-one-pad = one pad, { $style } bindings

options-pad-nothing-bound = nothing bound
options-pad-not-bound = not bound yet
options-pad-not-connected = not connected

# Variables:
#   $count (Number) - how many identical controllers are connected
options-pad-connected = { $count ->
    [one] connected
   *[other] { $count } connected
}

# A controller's buttons: bind it, unbind this style, its default
# bindings back, forget it.
options-pad-bind = Bind
options-pad-clear = Clear
options-pad-defaults = Use defaults
options-pad-forget = Forget
options-pad-description = Bind asks for each lane, then Start and Back; Clear unbinds this style; Use defaults puts the standard ones back; Forget drops everything saved for it.

# Which controller's button pictures the hints show.
options-pad-style = Button hints

# The pictures of the controller detected.
options-pad-style-auto = Auto

# The same, with the kind detected.
# Variables:
#   $family (String) - the kind of controller ("Xbox", "PlayStation (DualSense)", or the word for generic)
options-pad-style-auto-detected = Auto ({ $family })

# A controller of no known brand.
options-pad-style-generic = Generic
options-pad-style-description = Changes artwork only. Unmapped controls retain numbered button/axis labels. Identical controller models share this preference.

# A lane's row of a controller: takes one more control for it.
options-pad-lane-add = Add button
options-pad-lane-clear = Clear
options-pad-lane-description = Add button takes the next button, stick or hat direction you press on this controller for this lane (Escape, its Back or holding a button a second cancels); a control plays one lane only. Clear empties the lane.
options-pad-lane-shared-description = What plays this lane; its bindings for the one-pad style change it.

# A controller's Start (starts a song, confirms in the menus).
options-pad-start = Start

# A controller's Back (leaves a song, goes back in the menus).
options-pad-back = Back
options-pad-start-back-description = Start and Back are shared by every style.

## Songs (desktop): the song folders the game reads in place.

options-folder-remove = Remove
options-folder-description = Songs read in place from this folder (StepMania's Songs folder, its folder, a pack, or a zip). Removing it only takes it off the list: no file is touched.

options-folders = Song folders
options-folders-add = Add…
options-folders-rescan = Read again
options-folders-description = Add a folder of songs (StepMania's, ITGmania's or OutFox's, a Songs folder, a pack) or a pack's zip; nothing is copied. Read again reads every folder whole.

# Beside Song folders, in a line of tags: the songs found.
# Variables:
#   $count (Number) - songs
options-folders-songs = { $count ->
    [one] { $count } song
   *[other] { $count } songs
}

# The same while the folders are still being read.
# Variables:
#   $count (Number) - songs so far
options-folders-songs-reading = { $count ->
    [one] { $count } song, reading…
   *[other] { $count } songs, reading…
}

# Variables:
#   $count (Number) - songs that could not be read
options-folders-failed = { $count } not read

# Variables:
#   $count (Number) - packs found again in a later folder (the earlier one is played)
options-folders-duplicates = { $count ->
    [one] { $count } pack also in an earlier folder
   *[other] { $count } packs also in an earlier folder
}

# Variables:
#   $count (Number) - folders listed that are not there
options-folders-missing = { $count ->
    [one] { $count } folder missing
   *[other] { $count } folders missing
}

## Sharing songs by link (hidden unless switched on).

options-share-token = Upload token
options-share-token-set = Set…
options-share-token-change = Change…
options-share-token-clear = Clear

# Beside Upload token: one is kept.
options-share-token-is-set = set

# Beside Upload token: none is kept.
options-share-token-not-set = not set
options-share-token-description = The token the uploads carry, as whoever runs the server gave it; kept with the settings, so it is only as private as they are on this device.

options-share-shares = Shares
options-share-list = List…

# Beside Shares: the list shows the player's shares, or everyone's for
# an admin.
options-share-shares-note = yours, or every one

# Beside Shares: a token must be set first.
options-share-shares-no-token = a token first
options-share-shares-description = The shares made with your token, with their size and time left, each to copy, open or delete; an admin's token lists everyone's.

# The server shares go to.
options-share-server = Server
options-share-server-description = Where shared songs are uploaded to and read from, built into the game.

options-share-about = What a share is
options-share-about-note = a link to a copy
options-share-about-description = Share… in a song's or a pack's actions uploads a copy of its files; anyone with the link plays them from there until it expires. Share only what you may.

## Data: the play history and the resets.

options-history = Play history
options-history-export = Export
options-history-import = Import…
options-history-reset = Reset
options-history-importing = importing plays…
options-history-none = no plays kept yet

# Variables:
#   $count (Number) - plays kept
options-history-plays = { $count ->
    [one] { $count } play kept
   *[other] { $count } plays kept
}

# Variables:
#   $platform (String) - web or desktop
options-history-description = { $platform ->
    [web] Every song played is kept (quits and autoplays too), with every press, so its score can be worked out again. Export downloads them as JSON lines; Import (or a drop here) adds a file's plays; Reset deletes them.
   *[desktop] Every song played is kept (quits and autoplays too), with every press, so its score can be worked out again. Export saves them as JSON lines to a file you choose; Import adds a file's plays; Reset deletes them.
}

options-reset-settings = Settings
options-reset-settings-button = Reset

# Variables:
#   $platform (String) - web or desktop
options-reset-settings-description = { $platform ->
    [web] Every setting back to its default; imported songs and per-song offsets are kept.
   *[desktop] Every setting back to its default; the song folders and per-song offsets are kept.
}

# Desktop: forgets every song folder.
options-reset-folders = Song folders
options-reset-folders-button = Forget

# Beside Song folders: none is read.
options-reset-folders-none = none

# Variables:
#   $count (Number) - song folders
options-reset-folders-count = { $count ->
    [one] { $count } folder
   *[other] { $count } folders
}
options-reset-folders-description = Stops reading every song folder and forgets the songs taken off the list and their offsets; nothing in the folders is deleted, settings are kept.

# The web: removes every imported song.
options-reset-songs = Imported songs
options-reset-songs-button = Remove

# Beside Imported songs: none.
options-reset-songs-none = none

# Beside Imported songs, or Everything: nothing can be removed meanwhile.
options-reset-importing = an import is running

# Beside Imported songs: the songs (none, while imported noteskins or
# images are there to remove).
# Variables:
#   $count (Number) - imported songs
options-reset-songs-count = { $count ->
    [0] no songs
    [one] { $count } song
   *[other] { $count } songs
}
options-reset-songs-description = Deletes the imported songs, their offsets, the imported noteskins and announcers and the shared background images; settings are kept.

options-reset-everything = Everything
options-reset-everything-button = Reset

# Variables:
#   $platform (String) - web or desktop
options-reset-everything-description = { $platform ->
    [web] The settings, the songs and the play history above, and the offsets of the bundled songs: like a first visit.
   *[desktop] The settings, the song folders and the play history above, and the offsets of the bundled songs: like a first start (nothing in the folders is deleted).
}

## The confirmations: a title (a question), the consequence, and the
## button that does it (about 18 letters); Cancel beside it.

options-confirm-settings = Reset settings?
options-confirm-settings-body = Speed, scroll and note options, rules, volume, keys, controller bindings, display options and device calibrations go back to their defaults. Imported songs and per-song offsets are kept.
options-confirm-settings-button = Reset

# Desktop.
options-confirm-folders = Forget the song folders?

# Variables:
#   $count (Number) - song folders read now
options-confirm-folders-body = { $count ->
    [one] The game stops reading { $count } song folder and forgets the songs and packs taken off the list and the offsets of the folders' songs. Nothing in the folders is deleted; settings are kept.
   *[other] The game stops reading { $count } song folders and forgets the songs and packs taken off the list and the offsets of the folders' songs. Nothing in the folders is deleted; settings are kept.
}
options-confirm-folders-button = Forget

# The web.
options-confirm-songs = Remove imported songs?

# Variables:
#   $songs (Number) - songs imported into this browser
#   $images (Number) - background images shared between songs
options-confirm-songs-body = Deletes the songs imported into this browser ({ $songs ->
        [0] no imported songs
        [one] { $songs } imported song
       *[other] { $songs } imported songs
    }) and their offsets, the imported noteskins and announcers and the shared background images ({ $images ->
        [0] no images
        [one] { $images } image
       *[other] { $images } images
    }). Settings are kept.
options-confirm-songs-button = Remove

options-confirm-everything = Reset everything?

# Variables:
#   $platform (String) - web or desktop
#   $plays (Number) - plays kept in the history
options-confirm-everything-body = { $platform ->
    [web] Settings back to their defaults, imported songs, noteskins and announcers removed, { $plays ->
            [0] the play history (empty) cleared
            [one] the { $plays } play kept deleted
           *[other] the { $plays } plays kept deleted
        }, and the offsets of the bundled songs too: like a first visit.
   *[desktop] Settings back to their defaults, the song folders forgotten (nothing in them is deleted), { $plays ->
            [0] the play history (empty) cleared
            [one] the { $plays } play kept deleted
           *[other] the { $plays } plays kept deleted
        }, and the offsets of the bundled songs too: like a first start.
}
options-confirm-everything-button = Reset everything

options-confirm-history = Reset the play history?

# Variables:
#   $plays (Number) - plays kept
options-confirm-history-body = { $plays ->
    [one] Deletes the { $plays } play kept here, and the bests and lamps it gives; songs and settings are kept. Export it first to keep a copy.
   *[other] Deletes the { $plays } plays kept here, and the bests and lamps they give; songs and settings are kept. Export them first to keep a copy.
}
options-confirm-history-button = Delete

# Between Cancel and Delete: saves the history first, the question stays.
options-confirm-history-export = Export first

options-confirm-noteskin = Remove this noteskin?

# Variables:
#   $skin (String) - the noteskin's name
options-confirm-noteskin-body = { $skin } is removed from this browser; songs play with Infinito again.

# The same for a pump noteskin.
# Variables:
#   $skin (String) - the noteskin's name
options-confirm-pump-noteskin-body = { $skin } is removed from this browser; the pump styles are drawn with the arrows above again.
options-confirm-noteskin-button = Remove

options-confirm-announcer = Remove this announcer?

# Variables:
#   $announcer (String) - the announcer's name
#   $platform (String) - web or desktop
options-confirm-announcer-body = { $platform ->
    [web] { $announcer } is removed from this browser; plays are silent again.
   *[desktop] { $announcer } is deleted; plays are silent again.
}
options-confirm-announcer-button = Remove

## Notices: what the last action did, beside the header (one line,
## about 60 letters).

# Variables:
#   $style (String) - the style ("Single")
options-notice-keys-default = The keyboard's { $style } keys are the defaults again.

# Variables:
#   $style (String) - the style ("Single")
options-notice-keyboard-bound = The keyboard is bound for { $style }.

# Variables:
#   $device (String) - the controller's name as it reports it
#   $style (String) - the style ("Single")
options-notice-pad-bound = { $device } is bound for { $style }.

# Variables:
#   $device (String) - the controller's name as it reports it
options-notice-pad-forgotten = { $device } forgotten.

# A key or a control moved to another lane.
# Variables:
#   $control (String) - the key or control, as named ("KeyF", "button 3")
#   $from (String) - the lane it played ("Down")
#   $to (String) - the lane it plays now ("Left")
options-notice-moved = { $control } moved from { $from } to { $to }.

# Variables:
#   $control (String) - the key or control, as named ("KeyF", "button 3")
#   $lane (String) - the lane ("Left")
options-notice-added = { $control } added to { $lane }.

options-notice-token-set = The upload token is set.
options-notice-token-cleared = The upload token is cleared.
options-notice-settings-reset = Settings reset.
options-notice-announcer-removed = Announcer removed.

# Variables:
#   $error (String) - what went wrong
options-notice-announcer-not-removed = Could not remove the announcer: { $error }
options-notice-noteskin-removed = Noteskin removed.

# Variables:
#   $error (String) - what went wrong
options-notice-noteskin-not-removed = Could not remove the noteskin: { $error }
options-notice-songs-removed = Imported songs removed.
options-notice-everything-removed = Settings reset and imported songs removed.

# Variables:
#   $error (String) - what went wrong
options-notice-songs-not-removed = Could not remove the imported songs: { $error }

# Desktop: everything was reset.
options-notice-everything-reset = Everything reset.
options-notice-history-deleted = Play history deleted.

# Variables:
#   $error (String) - what went wrong
options-notice-history-not-deleted = Could not delete the play history: { $error }

# After the Judge again button: the plays are judged in the background.
# Variables:
#   $count (Number) - plays kept
options-notice-rejudge = { $count ->
    [one] { $count } kept play is being judged again.
   *[other] { $count } kept plays are being judged again.
}

# Variables:
#   $count (Number) - plays exported
options-notice-exported = { $count ->
    [one] { $count } play exported.
   *[other] { $count } plays exported.
}

# Desktop: the export written to a file.
# Variables:
#   $count (Number) - plays exported
#   $path (String) - the file
options-notice-exported-to = { $count ->
    [one] { $count } play exported to { $path }.
   *[other] { $count } plays exported to { $path }.
}

# Variables:
#   $error (String) - what went wrong
options-notice-export-failed = Could not export the plays: { $error }
options-notice-importing = Importing plays…

# Variables:
#   $count (Number) - plays kept so far
options-notice-importing-kept = Importing plays… { $count } kept so far.

# The web: a controller's press cannot open the browser's file picker.
options-notice-import-gesture = Choose Import with a key, a click or a tap (the browser opens its file picker only then), or drop the file on this page.

# Desktop.
options-notice-import-busy = The plays cannot be imported now (an import runs, or the history is not read).

# Desktop: the file picker's filter for play history files.
options-history-file-kind = Play history

## A play history import's report, as a line of tags joined by " · "
## (each a short phrase, no full stop).

# Variables:
#   $count (Number) - plays imported
options-import-imported = { $count ->
    [one] { $count } play imported
   *[other] { $count } plays imported
}

# Variables:
#   $count (Number) - plays that were kept already
options-import-duplicates = { $count ->
    [one] { $count } play already kept
   *[other] { $count } plays already kept
}

# Variables:
#   $count (Number) - plays kept by a newer version of the game
options-import-newer = { $count ->
    [one] { $count } play from a newer version of the game left out
   *[other] { $count } plays from a newer version of the game left out
}

# Variables:
#   $count (Number) - lines of the file that are not plays
options-import-invalid = { $count ->
    [one] { $count } line that is not a play left out
   *[other] { $count } lines that are not plays left out
}

# Variables:
#   $max (Number) - the most plays read from one file
options-import-cut = only the first { $max } plays read: import the rest from another file

# The first problem found, with its line in the file.
# Variables:
#   $line (Number) - the line
#   $problem (String) - what was wrong with it
options-import-problem = line { $line }: { $problem }

# Variables:
#   $error (String) - what went wrong
options-import-not-kept = the plays could not be kept: { $error }

## Dialogs: the upload token, the shares, binding, a key for a lane.

options-token-title = Upload token
options-token-body = The token whoever runs the sharing server gave you. Uploads carry it; the game never shows it again.

# The empty field's hint.
options-token-placeholder = paste the token

# The shares dialog's title, in small capitals: the player's own shares.
options-shares-title-mine = SHARES · YOURS

# The same for an admin: everyone's.
options-shares-title-all = SHARES · ALL
options-shares-loading = Reading the shares…

# Variables:
#   $error (String) - what went wrong
options-shares-failed = The shares could not be read: { $error }
options-shares-none = No share yet.

# Copies the chosen share's link.
options-shares-copy = Copy link
options-shares-delete = Delete

# Delete pressed once: pressed again, it deletes.
options-shares-delete-sure = Sure? Delete

# A share's line of tags ("3 songs · 12 MB · expires in 2 days"): its
# songs.
# Variables:
#   $count (Number) - songs
options-shares-songs = { $count ->
    [one] { $count } song
   *[other] { $count } songs
}

# A share whose upload never ended.
options-shares-unfinished = never finished
options-shares-never-expires = never expires
options-shares-expired = expired

# Variables:
#   $minutes (Number) - minutes left
options-shares-expires-minutes = expires in { $minutes } min

# Variables:
#   $hours (Number) - hours left
options-shares-expires-hours = expires in { $hours } h

# Variables:
#   $days (Number) - days left
options-shares-expires-days = { $days ->
    [one] expires in { $days } day
   *[other] expires in { $days } days
}

# An admin's list: a share someone else made.
options-shares-others = another's

# Variables:
#   $style (String) - the style ("Single")
options-bind-title-keyboard = Bind the keyboard for { $style }

# Variables:
#   $device (String) - the controller's name as it reports it
#   $style (String) - the style ("Single")
options-bind-title-pad = Bind “{ $device }” for { $style }
options-bind-title = Bind a device

# Records the next control pressed (a controller's button may otherwise
# move between the dialog's buttons).
options-bind-capture = Capture

# Starts the binding again from the first lane.
options-bind-redo = Redo

# Keeps Start's or Back's control as it is.
options-bind-skip = Skip
options-bind-review-pad = Bindings ready. Choose Save, Redo or Cancel.
options-bind-capturing = Recording a control; any button may be bound. Hold to cancel.
options-bind-choose-capture = Choose Capture to record this step. Directions navigate actions.
options-bind-pads-appear = Controllers appear once one of their buttons is pressed.
options-bind-skip-help = Skip keeps the current one; Start and Back are shared by every style.

# Variables:
#   $control (String) - the key or control held
options-bind-waiting = Waiting for { $control } to be released.
options-bind-help-review-pad = Directions choose an action; confirm activates it. Hold a button to cancel.
options-bind-help-review-keys = Enter (or Space) saves; Escape cancels.
options-bind-help-capturing = Escape or holding a controller button cancels.
options-bind-help = Escape, a controller's Back or holding its button cancels.

# A step that may be skipped (Start, Back), in the list of steps.
# Variables:
#   $step (String) - the step ("Start")
options-bind-step-optional = { $step } (optional)

# The steps' names after the lanes: a controller's Start and Back.
options-bind-step-start = Start
options-bind-step-back = Back

# What to press now.
options-bind-press-any = Press any key or controller button on the device you want to bind
options-bind-press-start = Press the button for Start (starts a song)
options-bind-press-back = Press the button for Back (leaves a song)

# Variables:
#   $lane (String) - the lane ("Left")
options-bind-press-lane = Press { $lane }
options-bind-press-review = Press each one to test it: it lights while held.

# A step recorded with nothing.
options-bind-skipped = skipped

# Variables:
#   $control (String) - the key or control still held
options-bind-release-first = Release { $control } first.

# Backspace leaves a song in play, so it cannot play a lane.
options-bind-backspace = Backspace leaves a song; press a different key.

# Variables:
#   $control (String) - the key or control pressed
#   $step (String) - what it is bound to already ("Left", "Start")
options-bind-already = { $control } is already { $step }. Press a different one.

# Variables:
#   $control (String) - the control pressed
options-bind-lane-elsewhere = { $control } plays a lane in another style's bindings. Press a different one.

# Two lanes bound to the two ends of one stick or hat.
# Variables:
#   $first (String) - a lane ("Left")
#   $second (String) - another lane ("Right")
options-bind-opposite = { $first } and { $second } are opposite ends of one axis: they cannot be pressed together.

# The dialog taking one more control for a lane of a controller.
# Variables:
#   $lane (String) - the lane ("Left")
options-key-title-pad = A button for { $lane }

# The dialog taking one more key for a lane.
# Variables:
#   $lane (String) - the lane ("Left")
options-key-title = A key for { $lane }
options-key-prompt-pad = Press a button, a stick or a hat direction on the controller (Escape, its Back or holding a button cancels).
options-key-prompt = Press a key (Escape, a controller's Back or holding its button cancels).

# A key pressed while a controller's control is awaited.
options-key-on-pad = Press it on the controller.

# Variables:
#   $control (String) - the control pressed
options-key-is-start = { $control } is this controller's Start. Press a different one.

