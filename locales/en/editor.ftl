## The chart editor: a song's chart on a field that scrolls under a fixed
## cursor line, a panel at its right (under it on a phone held upright)
## saying where the cursor is and what is there, and a footer of key
## hints. Never translated: BPM, the charts' difficulty names (Beginner…
## Edit), file formats (.ssc, .sm), key names as caps print them (R, K,
## Shift, G, Y), and what the player types (“1.5 2 beats”).

# The screen's name in its header, when the song has no title.
editor-title = Edit

# The screen's name in its header: the song being edited.
# Variables:
#   $title (String) - the song's title, as its file has it
editor-title-song = Edit · { $title }

## The footer's key hints, each after its keys' caps ("↑ ↓ Move",
## "SPACE Listen"): a word or two.

# The arrow keys move the cursor along the chart.
editor-hint-move = Move

# The left and right keys change the snap (the rows the cursor moves by).
editor-hint-snap = Snap

# The number keys place notes in their lanes.
editor-hint-notes = Notes

# Plays the music from the cursor.
editor-hint-listen = Listen

# Records notes played along to the music.
editor-hint-record = Record

# Plays the chart (a test play).
editor-hint-play = Play

# Opens the editor's menu.
editor-hint-menu = Menu

# A controller's shoulder buttons: the lane its notes go in.
editor-hint-lane = Lane

# A controller's button: places (or takes away) a note in that lane.
editor-hint-place = Place

## The field: badges on notes the skin draws as others, and the timing's
## markers at the right of the lanes (short, a small text).

# A small badge (one letter, 9-14 px square) on a fake note: one that is
# drawn but never judged.
editor-badge-fake = F

# A small badge (one letter) on a lift note: hit by letting go.
editor-badge-lift = L

# A tempo change's marker.
# Variables:
#   $bpm (String) - the beats a minute ("120", "145.5")
editor-marker-bpm = { $bpm } BPM

# A stop's marker: the chart pauses after the notes on its row.
# Variables:
#   $seconds (String) - how long ("0.5")
editor-marker-stop = stop { $seconds } s

# A delay's marker: the chart pauses before the notes on its row.
# Variables:
#   $seconds (String) - how long ("0.5")
editor-marker-delay = delay { $seconds } s

# A warp's marker: the chart jumps ahead; no note in it is judged.
# Variables:
#   $beats (Number) - its length in beats (1, 0.5)
editor-marker-warp = warp { $beats ->
    [one] { $beats } beat
   *[other] { $beats } beats
}

# A scroll speed change's marker: the speed it goes to, and over how many
# beats it gets there.
# Variables:
#   $ratio (String) - the speed, a ratio to normal ("1.5")
#   $duration (Number) - the beats it takes (2, 0.5)
editor-marker-speed-beats = speed { $ratio }× over { $duration ->
    [one] { $duration } beat
   *[other] { $duration } beats
}

# The same, over seconds.
# Variables:
#   $ratio (String) - the speed, a ratio to normal ("1.5")
#   $duration (String) - the seconds it takes ("0.25")
editor-marker-speed-seconds = speed { $ratio }× over { $duration } s

# A change of how fast the notes scroll from here.
# Variables:
#   $ratio (String) - a ratio to normal ("2", "0.5")
editor-marker-scroll = scroll { $ratio }×

# A fake section's marker: notes in it are drawn, never judged.
# Variables:
#   $beats (Number) - its length in beats (1, 0.5)
editor-marker-fake = fake { $beats ->
    [one] { $beats } beat
   *[other] { $beats } beats
}

# How many times a beat a hold counts while held.
# Variables:
#   $ticks (Number) - the count (4)
editor-marker-ticks = { $ticks ->
    [one] { $ticks } tick
   *[other] { $ticks } ticks
}

# What a hit adds to the combo, and a miss.
# Variables:
#   $hit (String) - a hit's ("1")
#   $miss (String) - a miss's ("1")
editor-marker-combo = combo { $hit }/{ $miss }

# A label's marker: a name the step artist gave this place, in quotes.
# Variables:
#   $label (String) - the name, as typed
editor-marker-label = “{ $label }”

## The panel: lines saying where the cursor is (about 40 letters wide on a
## wide screen, 28 on a phone; longer lines wrap), then buttons.

# Where the cursor is: its beat, its measure, and the song's time there.
# Variables:
#   $beat (String) - the beat ("12.5")
#   $measure (String) - the measure ("3"; negative before the first)
#   $time (String) - the song's time, minutes:seconds ("0:06.250")
editor-panel-beat = Beat { $beat } · measure { $measure } · { $time }

# The snap: the rows the cursor moves by, named as notes are (sixteenth
# notes are "16th"); the note tool follows after " · ".
# Variables:
#   $snap (Number) - the beat's division: 4, 8, 12, 16, 24, 32, 48, 64 or 192
editor-panel-snap = Snap { NUMBER($snap, type: "ordinal") ->
    [one] { $snap }st
    [two] { $snap }nd
    [few] { $snap }rd
   *[other] { $snap }th
}

# The edit view: rows evenly spaced by beat.
editor-view-edit = Edit view

# The play view: rows where a play draws them, at the player's speed.
# Variables:
#   $speed (String) - the player's speed ("2x", "C400")
editor-view-play = Play view · { $speed }

# The same, with the receptors at the bottom and the notes coming down
# from the top (Reverse).
# Variables:
#   $speed (String) - the player's speed ("2x", "C400")
editor-view-play-reverse = Play view · { $speed }, reverse

# After the view, " · " before it: the notes drawn small on plates rather
# than in the player's noteskin.
editor-view-compact = compact

# The audio view (the music's waveform and spectrogram beside the lanes):
# hidden.
editor-panel-audio-hidden = Audio: hidden

# The audio view: the music is still being read.
editor-panel-audio-reading = Audio: reading the music…

# The audio view: what it shows.
# Variables:
#   $mode (String) - both, waveform or spectrogram
editor-panel-audio = { $mode ->
    [waveform] Audio: waveform
    [spectrogram] Audio: spectrogram
   *[both] Audio: waveform and spectrogram
}

# The tempo at the cursor.
# Variables:
#   $bpm (String) - the beats a minute ("120")
editor-panel-bpm = { $bpm } BPM

# The same, where the chart has a timing of its own (not the song's).
# Variables:
#   $bpm (String) - the beats a minute ("120")
editor-panel-bpm-own = { $bpm } BPM (chart's own timing)

# The metronome (a click each beat while listening), as a list's first
# item: "Metronome on · clap off".
editor-panel-metronome-on = Metronome on
editor-panel-metronome-off = Metronome off

# The assist clap (a clap on each note while listening), the list's second
# item.
editor-panel-clap-on = clap on
editor-panel-clap-off = clap off

# Tap tempo: the tempo the taps say.
# Variables:
#   $bpm (String) - the beats a minute, two decimals ("128.05")
#   $taps (Number) - the taps it is from
editor-panel-tapped = Tapped { $bpm } BPM ({ $taps ->
    [one] { $taps } tap
   *[other] { $taps } taps
})

# The same, with the offset the taps heard (the music's start against the
# first beat).
# Variables:
#   $bpm (String) - the beats a minute, two decimals ("128.05")
#   $taps (Number) - the taps it is from
#   $offset (String) - seconds ("-0.2")
editor-panel-tapped-offset = Tapped { $bpm } BPM ({ $taps ->
    [one] { $taps } tap
   *[other] { $taps } taps
}) · offset { $offset } s

# Tap tempo: too few taps yet to say a tempo.
# Variables:
#   $taps (Number) - the taps so far
editor-panel-tapping = Tapping… ({ $taps })

# Before the timing's changes at the cursor (listed after it: "120 BPM ·
# stop 0.5 s"): a short label.
editor-panel-here = Here:

# How many notes the chart has.
# Variables:
#   $count (Number) - the notes
editor-panel-notes = { $count ->
    [one] { $count } note
   *[other] { $count } notes
}

# The beats selected.
# Variables:
#   $from (String) - the first beat ("4")
#   $to (String) - the last ("8.5")
editor-panel-selection = Selected beats { $from }–{ $to }

editor-panel-unsaved = Unsaved changes

editor-panel-saved = Saved

# A button at the panel's bottom (three share its width: about 8 letters
# on a phone): opens the editor's menu.
editor-panel-menu = Menu

# A button at the panel's bottom (about 8 letters on a phone): plays the
# chart from the cursor (a test play).
editor-panel-play = Play

## The status line at the panel's bottom: what just happened (a sentence
## or two, wrapped).

editor-status-listening = Listening

editor-status-cannot-save = This song cannot be saved here: export a copy instead

# A controller's range: its first row set at the cursor.
editor-status-range-start = Range start set

editor-status-view-edit = Edit view: rows evenly by beat

editor-status-view-play = Play view: as play draws it

# The notes drawn small on plates.
editor-status-field-compact = Compact field

# The notes drawn in the player's noteskin.
editor-status-field-skin = Field in the skin

# A controller's lane chosen.
# Variables:
#   $lane (Number) - the lane, from 1 at the left
editor-status-lane = Lane { $lane }

# A hold or a roll placed with a controller (its head, then its tail).
editor-status-long-placed = Long note placed

# A controller placing a hold or a roll: its head is set at the cursor.
editor-status-head-set = Head set; move to the tail and confirm. Back cancels.

editor-status-long-canceled = Long note canceled

editor-status-range-canceled = Range canceled

# The note tool chosen (what a press places).
# Variables:
#   $tool (String) - tap, hold, roll, mine, lift or fake
editor-status-tool = { $tool ->
    [hold] Tool: Hold
    [roll] Tool: Roll
    [mine] Tool: Mine
    [lift] Tool: Lift
    [fake] Tool: Fake
   *[tap] Tool: Tap / hold
}

# The snap changed: the rows the cursor moves by.
# Variables:
#   $snap (Number) - the beat's division: 4, 8, 12, 16, 24, 32, 48, 64 or 192
editor-status-snap = Snap { NUMBER($snap, type: "ordinal") ->
    [one] { $snap }st
    [two] { $snap }nd
    [few] { $snap }rd
   *[other] { $snap }th
}

editor-status-chart-added = Chart added

# The selected notes copied, for pasting.
editor-status-copied = Copied

editor-status-saved = Saved

# A save failed.
# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
editor-status-not-saved = Not saved: { $error }

editor-status-metronome-on = Metronome on
editor-status-metronome-off = Metronome off
editor-status-clap-on = Assist clap on
editor-status-clap-off = Assist clap off

# Applying a tapped tempo before tapping one (Y is the key tapped).
editor-status-tap-first = Tap Y along to the beat first

editor-status-tempo-not-kept = That tempo cannot be kept in a file

# A tapped tempo applied at the cursor.
# Variables:
#   $bpm (String) - the beats a minute ("128.05")
#   $beat (String) - the cursor's beat ("4")
editor-status-tapped = { $bpm } BPM at beat { $beat }

# The same, with the offset the taps heard.
# Variables:
#   $bpm (String) - the beats a minute ("128.05")
#   $beat (String) - the cursor's beat ("4")
#   $offset (String) - the offset set, seconds ("-0.2")
editor-status-tapped-offset = { $bpm } BPM at beat { $beat }, offset { $offset } s

# The audio view asked for where the song has no music.
editor-status-no-music = No music to show

editor-status-audio-hidden = Audio view hidden

# The audio view shown, and what it shows.
# Variables:
#   $mode (String) - both, waveform or spectrogram
editor-status-audio = { $mode ->
    [waveform] Audio view: waveform
    [spectrogram] Audio view: spectrogram
   *[both] Audio view: waveform and spectrogram
}

# Recording notes played along to the music has started.
# Variables:
#   $beat (String) - the beat it records from ("4")
editor-status-recording = Recording from beat { $beat }: R or Escape stops

# The same, adding to the notes there rather than replacing them.
# Variables:
#   $beat (String) - the beat it records from ("4")
editor-status-recording-add = Recording, adding from beat { $beat }: R or Escape stops

# A recording ended: how many notes it put in the chart (how far the
# presses were from the rows may follow after " · ").
# Variables:
#   $count (Number) - the notes
editor-recorded = { $count ->
    [0] Nothing recorded
    [one] { $count } note recorded
   *[other] { $count } notes recorded
}

# After the count: the presses were late, on average.
# Variables:
#   $ms (String) - how late, milliseconds ("12")
#   $spread (String) - their spread (a standard deviation), milliseconds ("9")
editor-recorded-late = { $ms } ms late on average, ±{ $spread } ms

# After the count: the presses were early, on average.
# Variables:
#   $ms (String) - how early, milliseconds ("12")
#   $spread (String) - their spread (a standard deviation), milliseconds ("9")
editor-recorded-early = { $ms } ms early on average, ±{ $spread } ms

# After the count: the presses were on the rows, on average.
# Variables:
#   $spread (String) - their spread (a standard deviation), milliseconds ("9")
editor-recorded-on-grid = on the grid on average, ±{ $spread } ms

# The editor opened on changes kept from before (the game closed without
# saving them).
editor-status-recovered = Unsaved changes recovered from the last time; leave without saving to drop them

# The editor opened on a song just made from a music file.
editor-status-new-song = A new song: saving keeps it

## The editor's menu (Tab, a controller's Select or More): a list of
## buttons in one or two columns, about 32 letters each.

# The menu's title.
editor-menu-title = Editor

# Plays the music from the cursor.
editor-menu-listen = Listen

# Stops the music.
editor-menu-stop = Stop

# A test play of the chart from the cursor.
editor-menu-play-cursor = Play from cursor

editor-menu-play-start = Play from start

editor-menu-autoplay-cursor = Autoplay from cursor

# Records notes played along to the music, from the cursor (R is its key).
editor-menu-record-cursor = Record from cursor (R)

# The same, over the beats selected.
editor-menu-record-selection = Record the selection (R)

# Records notes, adding to those there rather than replacing them.
editor-menu-record-add-cursor = Record from cursor, adding (Shift R)

editor-menu-record-add-selection = Record the selection, adding (Shift R)

# The metronome (a click each beat while listening), and its key.
editor-menu-metronome-on = Metronome: on (K)
editor-menu-metronome-off = Metronome: off (K)

# The assist clap (a clap on each note while listening), and its keys.
editor-menu-clap-on = Assist clap: on (Shift K)
editor-menu-clap-off = Assist clap: off (Shift K)

# Sets the tempo tapped as the BPM at the cursor.
# Variables:
#   $bpm (String) - the beats a minute ("128.05")
editor-menu-apply-tapped = Apply tapped BPM: { $bpm } (Shift Y)

# The same, and the offset the taps heard.
# Variables:
#   $bpm (String) - the beats a minute ("128.05")
editor-menu-apply-tapped-offset = Apply tapped BPM ({ $bpm }) and offset

editor-menu-undo = Undo

editor-menu-redo = Redo

# Opens the list of note tools (what a press places), saying the one in
# use.
# Variables:
#   $tool (String) - tap, hold, roll, mine, lift or fake
editor-menu-tool = { $tool ->
    [hold] Tool: Hold
    [roll] Tool: Roll
    [mine] Tool: Mine
    [lift] Tool: Lift
    [fake] Tool: Fake
   *[tap] Tool: Tap / hold
}

# Opens the timing at the cursor (tempo, stops…).
editor-menu-timing = Timing here…

# Opens the song's fields (title, artist, files…).
editor-menu-song = Song…

# Opens the chart's fields and the song's other charts.
editor-menu-chart = Chart…

# Takes the selected notes away, for pasting.
editor-menu-cut = Cut

editor-menu-delete-selection = Delete selection

# Turns the selected notes around (left for right).
editor-menu-mirror-selection = Mirror selection

editor-menu-select-all = Select all

editor-menu-paste = Paste

# Keeps a copy of the song elsewhere, in a file format: the web downloads
# it, desktop asks where to save it.
# Variables:
#   $platform (String) - the shell: web or desktop
#   $format (String) - the file format (".ssc", ".sm")
editor-menu-export = { $platform ->
    [web] Download ({ $format })
   *[desktop] Save a copy ({ $format })
}

# Shows the rows where a play draws them.
editor-menu-view-play = Play view

# Shows the rows evenly spaced by beat.
editor-menu-view-edit = Edit view

# The audio view: the music's waveform and spectrogram beside the lanes.
editor-menu-audio-hide = Hide the audio view (G)
editor-menu-audio-show = Show the audio view (G)

# The audio view: what it shows next.
# Variables:
#   $mode (String) - both, waveform or spectrogram
editor-menu-audio-mode = { $mode ->
    [waveform] Audio view: waveform (Shift G)
    [spectrogram] Audio view: spectrogram (Shift G)
   *[both] Audio view: waveform and spectrogram (Shift G)
}

# Draws the notes in the player's noteskin.
editor-menu-field-skin = Field in the skin

# Draws the notes small on plates.
editor-menu-field-compact = Compact field

## The menu's items for a controller with few buttons: the same as keys do.

editor-menu-place = Place / remove note
editor-menu-lane-previous = Previous lane
editor-menu-lane-next = Next lane
editor-menu-range-start = Start range here
editor-menu-range-end = End range here
editor-menu-range-clear = Clear range

# Moves the selected notes a row (of the snap) earlier or later.
editor-menu-nudge-earlier = Nudge earlier
editor-menu-nudge-later = Nudge later

editor-menu-jump-start = Jump to start
editor-menu-jump-end = Jump to end
editor-menu-measure-previous = Previous measure
editor-menu-measure-next = Next measure
editor-menu-zoom-in = Zoom in
editor-menu-zoom-out = Zoom out

# Opens tap tempo: tapping along to the music says its tempo.
editor-menu-tap-tempo = Tap tempo…

editor-menu-leave = Leave the editor

## The note tools, in the list the menu opens (its title, then a button
## each) and after the snap in the panel.

editor-tool-title = Note tool

# Taps; held while the cursor moves, a hold.
editor-tool-tap = Tap / hold
editor-tool-hold = Hold
editor-tool-roll = Roll

# A note not to step on.
editor-tool-mine = Mine

# A note hit by letting go.
editor-tool-lift = Lift

# A note drawn but never judged.
editor-tool-fake = Fake

## Saving and leaving: questions (a title, a sentence or two, two buttons).

editor-save-over-title = Save over the song?

# What saving replaces, and what it keeps.
# Variables:
#   $platform (String) - the shell: web or desktop
#   $file (String) - desktop: the song's file replaced, its path
#   $name (String) - desktop: that file's name; the old one is kept as "<name>.old"
editor-save-over-body = { $platform ->
    [web] The song stored in this browser is replaced (the files it was imported from are not changed). Charts whose notes changed are new charts for the play history.
   *[desktop] { $file } is replaced; the file as it was is kept beside it as { $name }.old (at the first save each time the editor is opened). Charts whose notes changed are new charts for the play history.
}

editor-leave-title = Leave without saving?

editor-leave-body = The changes since the last save are lost.

# Leaving a song just made, never saved.
# Variables:
#   $platform (String) - the shell: web or desktop
editor-leave-new-song-body = { $platform ->
    [web] The new song is not kept.
   *[desktop] The new song is not kept: its folder is removed.
}

# The button that leaves without saving.
editor-leave = Leave

## The timing at the cursor (T): a list of what is there, a button each
## (about 32 letters), under a line saying where.

editor-timing-title = Timing

# Under the title.
# Variables:
#   $beat (String) - the cursor's beat ("4.5")
editor-timing-note = At beat { $beat }.

# A kind of timing change and its value at the cursor ("—" where there is
# none); picked, it is set.
# Variables:
#   $kind (String) - bpm, stop, delay, warp, speed, scroll, fake, timesig, tickcount, combo or label
#   $value (String) - its value, as typed ("120", "1.5 2 beats", "3/4", "—")
editor-timing-item = { $kind ->
    [bpm] BPM: { $value }
    [stop] Stop: { $value }
    [delay] Delay: { $value }
    [warp] Warp: { $value }
    [speed] Speed: { $value }
    [scroll] Scroll: { $value }
    [fake] Fake: { $value }
    [timesig] Time signature: { $value }
    [tickcount] Tick count: { $value }
    [combo] Combo: { $value }
   *[label] Label: { $value }
}

# The tempo at the cursor where no change is there: the one before.
# Variables:
#   $bpm (String) - the beats a minute ("120")
editor-timing-bpm-before = BPM: { $bpm } from before

# The song's offset: where its first beat is against the music.
# Variables:
#   $seconds (String) - seconds ("0.123")
editor-timing-offset = Offset: { $seconds } s

# How timing changes treat the notes (picked, it switches): they stay on
# their beats…
editor-timing-keep-beats = Changes keep notes on their beats

# …or where they are heard.
editor-timing-keep-times = Changes keep notes' times

# Whether this chart has its own timing rather than the song's (picked, it
# switches).
editor-timing-own-on = Chart's own timing: on
editor-timing-own-off = Chart's own timing: off

# A timing change's dialog title.
# Variables:
#   $kind (String) - bpm, stop, delay, warp, speed, scroll, fake, timesig, tickcount, combo or label
#   $beat (String) - the cursor's beat ("4.5")
editor-segment-title = { $kind ->
    [bpm] BPM at beat { $beat }
    [stop] Stop at beat { $beat }
    [delay] Delay at beat { $beat }
    [warp] Warp at beat { $beat }
    [speed] Speed at beat { $beat }
    [scroll] Scroll at beat { $beat }
    [fake] Fake at beat { $beat }
    [timesig] Time signature at beat { $beat }
    [tickcount] Tick count at beat { $beat }
    [combo] Combo at beat { $beat }
   *[label] Label at beat { $beat }
}

## Under a timing change's title when its value is typed: what to type.
## Typed examples in quotes are what the editor reads (“beats”, “s”):
## keep them as they are.

editor-segment-note-bpm = Beats a minute from here. Empty takes the change away.

editor-segment-note-stop = Seconds (a stop pauses after the notes on its row, a delay before). Empty or 0 takes it away.

# A warp's or a fake section's.
editor-segment-note-length = Its length in beats. Empty takes it away.

editor-segment-note-speed = The scroll speed's ratio, then how long it takes to get there and in what: “1.5 2 beats”, “0.5 0.25 s”.

editor-segment-note-scroll = How fast the notes scroll from here, 1 normal.

editor-segment-note-time-signature = As “3/4”.

editor-segment-note-tick-count = Hold ticks a beat, 0 to 48.

editor-segment-note-combo = Combo a hit adds, and a miss: “2/1”, or one number for both.

editor-segment-note-label = A name for this place.

# The song's offset's dialog.
editor-offset-title = Offset

editor-offset-note = Seconds; the music's start is at minus the offset (StepMania's sign).

## Why a value typed is not taken (under the field, a sentence).

editor-error-not-kept = That value cannot be kept in a file.

# Variables:
#   $text (String) - what was typed
editor-error-not-number = “{ $text }” is not a number.

# Variables:
#   $text (String) - what was typed
editor-error-not-whole = “{ $text }” is not a whole number.

editor-error-time-signature = As “3/4”.

# A chart's level (its meter).
editor-error-meter = A whole number, 0 or more.

## Numbers set with a controller: a button each, its value between ‹ and ›
## (left and right change it), then buttons.

# Variables:
#   $value (String) - the value ("1.5")
editor-number-value = Value: { $value }

# A scroll speed change's speed, a ratio to normal.
# Variables:
#   $value (String) - the value ("1.5")
editor-number-ratio = Ratio: { $value }

# How long a scroll speed change takes.
# Variables:
#   $value (String) - the value ("2")
editor-number-duration = Duration: { $value }

# A time signature's beats a measure ("3" of 3/4).
# Variables:
#   $value (String) - the value ("3")
editor-number-numerator = Numerator: { $value }

# A time signature's note value ("4" of 3/4).
# Variables:
#   $value (String) - the value ("4")
editor-number-denominator = Denominator: { $value }

# What a hit adds to the combo.
# Variables:
#   $value (String) - the value ("1")
editor-number-hit = Hit: { $value }

# What a miss adds to the combo.
# Variables:
#   $value (String) - the value ("1")
editor-number-miss = Miss: { $value }

# The tempo the song select shows (or its lower end).
# Variables:
#   $value (String) - the beats a minute ("120")
editor-number-bpm = BPM: { $value }

# The upper end of the tempo the song select shows.
# Variables:
#   $value (String) - the beats a minute ("180")
editor-number-upper-bpm = Upper BPM: { $value }

# What a scroll speed change's duration is in (a button: left and right
# switch).
editor-number-beats = Beats
editor-number-seconds = Seconds

# The tempo the song select shows: the chart's own, …
editor-number-automatic = Automatic

# …changing at random, …
editor-number-random = Random

# …one tempo, …
editor-number-single-bpm = Single BPM

# …or a range.
editor-number-bpm-range = BPM range

# Whether left and right change the values by ten steps or one.
editor-number-step-coarse = Step: coarse ×10
editor-number-step-fine = Step: fine

# Types the value with a keyboard instead.
editor-number-type-value = Type value…

# Puts the values back as they were when the dialog opened.
editor-number-reset = Reset changes

# Takes the timing change away.
editor-number-remove = Remove

# Takes the values set.
editor-number-apply = Apply

# A typed value's dialog: takes it.
editor-ok = OK

## A list of actions (the menu, the timing, the song's and the chart's
## fields): when they do not fit, pages.

editor-previous-page = Previous page
editor-next-page = Next page

## The song's fields (I): a list, a button each with the field's value
## (about 32 letters, cut), and a dialog for each.

# A field's line in the list, its value after it ("—" where there is
# none); seconds have their unit.
# Variables:
#   $field (String) - title, subtitle, artist, title_translit, artist_translit, genre, credit, music, banner, background, preview_start, preview_length or display_bpm
#   $value (String) - its value, as the song's file has it ("—" for none)
editor-song-item = { $field ->
    [subtitle] Subtitle: { $value }
    [artist] Artist: { $value }
    [title_translit] Title (romanized): { $value }
    [artist_translit] Artist (romanized): { $value }
    [genre] Genre: { $value }
    [credit] Credit: { $value }
    [music] Music: { $value }
    [banner] Banner: { $value }
    [background] Background: { $value }
    [preview_start] Preview from: { $value } s
    [preview_length] Preview length: { $value } s
    [display_bpm] Display BPM: { $value }
   *[title] Title: { $value }
}

# A field's dialog title.
# Variables:
#   $field (String) - title, subtitle, artist, title_translit, artist_translit, genre, credit, music, banner, background, preview_start, preview_length or display_bpm
editor-song-field = { $field ->
    [subtitle] Subtitle
    [artist] Artist
    [title_translit] Title (romanized)
    [artist_translit] Artist (romanized)
    [genre] Genre
    [credit] Credit
    [music] Music file
    [banner] Banner file
    [background] Background file
    [preview_start] Preview from (seconds)
    [preview_length] Preview length (seconds)
    [display_bpm] Display BPM
   *[title] Title
}

editor-song-title = Song

# Under a file's dialog title.
editor-song-file-note = A file in the song's folder, by its name.

# Under the display BPM's title: what to type (“*” and the range are typed
# as they are).
editor-song-display-bpm-note = One number, a range “100-200”, “*” for random, or empty for the chart's own.

## The chart's fields (C): a list, a button each, and the song's charts.

# The list's title.
# Variables:
#   $index (Number) - this chart's place, from 1
#   $count (Number) - the song's charts
editor-chart-title = Chart { $index } of { $count }

# The chart's difficulty (picked, it changes to the next).
# Variables:
#   $difficulty (String) - its name, as simfiles name them (Beginner, Easy, Medium, Hard, Challenge, Edit)
editor-chart-difficulty = Difficulty: { $difficulty }

# The chart's level (StepMania's meter).
# Variables:
#   $meter (String) - the level ("9")
editor-chart-meter = Meter: { $meter }

# The chart's name (an Edit chart's).
# Variables:
#   $name (String) - the name, as typed
editor-chart-name = Name: { $name }

# Variables:
#   $description (String) - the chart's description, as typed
editor-chart-description = Description: { $description }

# Who made the chart.
# Variables:
#   $credit (String) - the step artist's name, as typed
editor-chart-credit = Step artist: { $credit }

# A field's dialog title.
# Variables:
#   $field (String) - meter, name, description or credit
editor-chart-field = { $field ->
    [name] Name
    [description] Description
    [credit] Step artist
   *[meter] Meter
}

editor-chart-previous = Previous chart
editor-chart-next = Next chart

# Opens the list of layouts for a new chart.
editor-chart-add = Add a chart…

editor-chart-remove = Remove this chart…

# The list of layouts for a new chart: its title.
editor-chart-add-title = Add a chart

# Under its title. "Edit" is a difficulty's name (never translated).
editor-chart-add-note = An empty Edit chart on the song's timing.

# After a layout's name (" · " before it): its lanes.
# Variables:
#   $count (Number) - the lanes
editor-chart-lanes = { $count ->
    [one] { $count } lane
   *[other] { $count } lanes
}

editor-chart-remove-title = Remove this chart?

editor-chart-remove-body = Its notes go with it (Undo brings it back until you leave).

# The button that removes the chart.
editor-chart-remove-yes = Remove

## Tap tempo (Y, or the menu's Tap tempo…): taps along to the music say
## its tempo. A sentence, then buttons two a row (about 20 letters each).

editor-tap-title = Tap tempo

editor-tap-start = Tap along to the beat, then apply.

# Too few taps yet to say a tempo.
# Variables:
#   $taps (Number) - the taps so far
editor-tap-count = { $taps ->
    [one] { $taps } tap — tap along to the beat
   *[other] { $taps } taps — tap along to the beat
}

# The tempo the taps say.
# Variables:
#   $bpm (String) - the beats a minute, two decimals ("128.05")
#   $taps (Number) - the taps it is from
editor-tap-estimate = { $bpm } BPM ({ $taps ->
    [one] { $taps } tap
   *[other] { $taps } taps
})

# The button tapped along to the beat.
editor-tap-tap = Tap

# Sets the tempo as the BPM at the cursor.
editor-tap-apply = Apply BPM

# The same, and the offset the taps heard.
editor-tap-apply-offset = Apply BPM + offset

editor-tap-reset = Reset taps
