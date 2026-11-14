## The results: a play's score and counts in a column at the left, its
## panels (graphs) at the right, a bar of buttons under them. The ruleset's
## judgment names, grades, lamps and marks (REPLAY, AUTOPLAY, ASSIST,
## FAILED) are never translated; the numbers are the game's readouts.

# The screen's name in its header, in small capitals: written in capitals
# where the language has them.
results-title = RESULTS

## The bar's buttons (about 120 px wide, sized to their text) and the
## footer's key hints (after the keys' caps, "ENTER Song list").

# Goes back to the song list.
results-song-list = Song list

# Plays the same chart again.
results-retry = Retry

# On a replay's results: watches the replay again.
results-watch-again = Watch again

# Watches this play's replay.
results-watch-replay = Watch replay

# A controller's hint: presses the button the focus is on.
results-activate = Activate

## The More menu's items (a list, about 24 letters each): what the bar
## and the offset editor do, for a controller or a phone.

results-more-offset-down = Offset −1 ms
results-more-offset-up = Offset +1 ms
results-more-reset-offset = Reset offset

# Sets the song's offset to what this play's hits say (their mean error).
results-more-use-mean = Use mean offset

## The song's offset editor, in the bar beside the buttons: a label, −1
## and +1 buttons around the value, then buttons to take this play's mean.

# Its label on a wide screen.
results-song-offset = Song offset

# Its label on a phone: about 8 letters.
results-offset = Offset

# The offset as set now, between the −1 and +1 buttons.
# Variables:
#   $ms (String) - milliseconds, signed ("+12", "−3", "0")
results-offset-value = { $ms } ms

# Sets the song's offset to what this play's hits say (their mean error).
# Variables:
#   $ms (String) - that offset in milliseconds, signed ("+12")
results-use-mean = Use { $ms } ms

## The score column's stats: two a row, a label at the left (about 20
## letters) and its value at the right.

# A hold's or a roll's checkpoints (Pump's ticks): how many were hit and
# how many missed ("38 / 2").
results-ticks = Ticks hit / missed

results-mines-hit = Mines hit

# Presses where there was no note to hit.
results-stray-presses = Stray presses

# The longest run of notes without a break.
results-max-combo = Max combo

# How many hits were early (fast) and late (slow): "12 / 30".
results-fast-slow = Fast / slow

# DDI's rules: how many hits were early and late (never "fast" and
# "slow" under them): "12 / 30".
results-early-late = Early / late

# DDI's rules: the score (a longer chart gives more: several million)
# and the percent, where the big number shows the other of them.
results-ddi-score = Score
results-ddi-percent = Percent

# The average of the hits' errors, then their spread: "−2.1 ms (σ 9.8)".
results-mean-offset = Mean offset

# DDR A's FLARE SKILL: the rank kept and its points ("FLARE VII · 944
# points").
results-flare-skill = Flare skill

# DDR A's FLARE SKILL points, after the rank.
# Variables:
#   $points (Number) - the points
results-flare-points = { $points ->
    [one] { $points } point
   *[other] { $points } points
}

# FFR's (FlashFlashRevolution's) total score, beside its raw score.
results-ffr-total-score = Total score

# FFR's raw goods: its count of hits short of the best, as FFR names it.
results-ffr-raw-goods = Raw goods

## Notes under the results (sentences, wrapped), and on the footer's line.

results-note-autoplay = Autoplay: this play does not count toward your bests.

results-note-device-changed = An audio device or the display changed during play.

# The life bar emptied; the rules let the play go on to its end (the fail
# option "fail delayed", as the mods line says it).
# Variables:
#   $at (String) - when, as a clock reads it ("1:35")
results-note-fail-delayed = Failed at { $at }: the life bar emptied, and the play went on (fail delayed).

# DDI's Practice option: the life emptied and the play went on, failed.
# Variables:
#   $at (String) - when, as a clock reads it ("1:35")
results-note-practice = Failed at { $at }: the life emptied, and the play went on (Practice).

# A kept play judged again under rules other than those it was played by
# (Options → Rules → Kept plays), under its results and its replay.
# Variables:
#   $played (String) - the rules it was played by, as the games name them ("ITG")
#   $now (String) - the rules it is judged by here ("DDI")
results-note-rejudged = Played by { $played } rules and judged again here by { $now } rules, which the play did not react to.

# The rules check the life bar at the end only (the fail option "fail at
# end"): it was still empty.
# Variables:
#   $at (String) - when it emptied ("1:35")
results-note-fail-at-end = Failed at the end: the life bar emptied at { $at } and was still empty.

# The same rules: it emptied and came back, so the play did not fail.
# Variables:
#   $at (String) - when it emptied ("1:35")
results-note-not-failed-at-end = The life bar emptied at { $at } and came back: not failed (fail at end).

# The rules never fail a play (the fail option "fail off").
# Variables:
#   $at (String) - when it emptied ("1:35")
results-note-not-failed-off = The life bar emptied at { $at }: not failed (fail off).

# The game turned the background videos off because the play was dropping
# frames; Options has the video setting.
results-note-video-dropped-frames = Video was turned off for this song: frames were being dropped (Options).

# The same, because the video decoder could not keep up.
results-note-video-decoder-behind = Video was turned off for this song: the decoder could not keep up (Options).

# The game turned the dancing hand (the 3D hand that plays the chart beside
# the field) off for the rest of the song because the play was dropping
# frames with it on; Options has the hand's setting.
results-note-hand-dropped-frames = The dancing hand was turned off for this song: frames were being dropped (Options).

# A background video could not play; the song's picture showed instead.
# Variables:
#   $name (String) - the video's file name
#   $problem (String) - why (errors-video-…, or the decoder's own words)
results-note-video-trouble = The video { $name } could not play: { $problem }; the song background showed.

# Several background videos could not play, all for the same reason.
# Variables:
#   $count (Number) - how many, 2 or more
#   $problem (String) - why (errors-video-…, or the decoder's own words)
results-note-videos-trouble = { $count ->
    [one] { $count } video could not play: { $problem }; the song background showed.
   *[other] { $count } videos could not play: { $problem }; the song background showed.
}

# Several background videos could not play, for different reasons: the
# first one's.
# Variables:
#   $count (Number) - how many, 2 or more
#   $name (String) - the first one's file name
#   $problem (String) - why it could not (errors-video-…, or the decoder's own words)
results-note-videos-trouble-first = { $count ->
    [one] { $count } video could not play; { $name }: { $problem }; the song background showed.
   *[other] { $count } videos could not play; { $name }: { $problem }; the song background showed.
}

# A song's scripted backgrounds (its own Lua) failed during the play; its
# picture showed in their place.
# Variables:
#   $error (String) - the scripts' own error, as their Lua said it (technical, in English)
results-note-scripts-failed = The song's scripted backgrounds stopped ({ $error }); the song background showed.

# The same: they took longer than a frame allows.
results-note-scripts-slow = The song's scripted backgrounds stopped (they took too long); the song background showed.

# The same: they did more work in a frame than allowed.
results-note-scripts-busy = The song's scripted backgrounds stopped (they did too much work); the song background showed.

# The same: they used more memory than allowed.
results-note-scripts-memory = The song's scripted backgrounds stopped (they used too much memory); the song background showed.

# The same: they made a text longer than allowed.
results-note-scripts-string = The song's scripted backgrounds stopped (a string grew too long); the song background showed.

# Desktop: the sound output fell behind during the play, so its timing
# may be off.
# Variables:
#   $count (Number) - how many times
results-note-sound-dropped = { $count ->
    [one] The sound dropped out { $count } time during this play: its timing may be off.
   *[other] The sound dropped out { $count } times during this play: its timing may be off.
}

# Variables:
#   $error (String) - why: its kind and what the storage said (errors-full…)
results-note-not-saved = This play could not be saved: { $error }.

# A kept play's or a replay's results: when it was played.
# Variables:
#   $when (String) - the date and time ("2026-10-16 14:03")
results-played-at = Played { $when }.

## A replay judged otherwise than the play it replays (a fault of the
## game): a note under its results for each way it differed.

# Its log, judged again at once, gave other results than the replay.
results-replay-differs-inputs = This replay judged differently from the play: the replay and its inputs judged at once differ.

results-replay-differs-timing = This replay judged differently from the play: its judgments or life are not the play's.

results-replay-differs-score = This replay judged differently from the play: its score or counts are not the play's.

# The play could not be judged again to compare.
# Variables:
#   $why (String) - why, a sentence (results-replay-chart-changed…)
results-replay-not-compared = This replay could not be compared with the play. { $why }

## Why a replay cannot be watched or a kept play's results shown, a
## sentence after "This replay cannot be watched." (prompt-replay-refused)
## or "This play's results cannot be shown." (prompt-results-refused).

# Variables:
#   $error (String) - why: its kind and what the storage said (errors-not-found…)
results-replay-history-unreadable = The play history could not be read: { $error }.

# Desktop: the play history did not answer (its work stopped).
results-replay-history-silent = The play history did not answer.

# The play was deleted from the history.
results-replay-gone = This play is no longer kept.

results-replay-record-unreadable = This play's record does not read.

# Variables:
#   $detail (String) - what the reader said (technical, in English)
results-replay-record-broken = This play's record does not read ({ $detail }).

results-replay-newer-record = This play was kept by a newer version of the game: this one cannot replay it.

results-replay-newer-judge = This play was judged by a newer version of the game: this one would judge it differently.

# Its log of presses is damaged.
# Variables:
#   $detail (String) - what the reader said (technical, in English)
results-replay-log-unreadable = This play's presses do not read ({ $detail }).

results-replay-chart-changed = This chart changed since the play (its notes or timing are not the ones played): its replay would be wrong.

# Its song is in the library but could not be read.
# Variables:
#   $error (String) - why (errors-load-…)
results-replay-song-unreadable = Its song could not be read: { $error }.

# A kept play judged by an older version of the judge: today's judges it
# here, and may differ from what the play showed then.
results-replay-older-judge = This play was judged by an older version of the game; it is judged here by today's, which may differ from what it showed.

# Why a replay cannot be watched (on the start prompt): its noteskin went.
# Variables:
#   $skin (String) - the noteskin's name as the settings know it
results-replay-missing-skin = This play was drawn with the noteskin { $skin }, which is no longer installed: add it again to watch the replay.

# Why a replay cannot be watched: its song went.
results-replay-missing-song = The song of this play is no longer in the library (no song has a chart with its notes).

## The panels: a title at the top left, a short note at the top right
## (numbers and a few words), a graph under them.

# The hits' errors as a histogram, early at the left.
results-panel-timing = Timing

# The histogram's note: the hits' mean error and its spread.
# Variables:
#   $mean (String) - the mean in milliseconds, signed ("−2.1")
#   $sd (String) - the standard deviation in milliseconds ("9.8")
results-timing-note = mean { $mean } ms · σ { $sd }

# Under the histogram's left end.
# Variables:
#   $ms (String) - the graph's range in milliseconds ("45")
results-timing-early = −{ $ms } ms early

# Under the histogram's right end.
# Variables:
#   $ms (String) - the graph's range in milliseconds ("45")
results-timing-late = late +{ $ms } ms

# The life bar over the song.
results-panel-life = Life

# The life graph's note: the largest fall in a few seconds, and when.
# Variables:
#   $percent (String) - how much of the bar fell ("29")
#   $at (String) - when it began, as a clock reads it ("1:35")
results-life-drop = largest drop { $percent }% at { $at }

# The life graph's note when the life never fell.
results-life-no-drop = no drop

# Each hit's error over the song, early hits above the middle line.
results-panel-offsets = Offsets over the song

# Its note when there was no miss.
# Variables:
#   $ms (String) - the graph's range in milliseconds ("45")
results-offsets-range = early above · ±{ $ms } ms

# Its note with misses (drawn as lines).
# Variables:
#   $misses (Number) - how many notes were missed
results-offsets-misses = early above · { $misses ->
    [one] { $misses } miss
   *[other] { $misses } misses
}

# The hits short of the best judgment, counted lane by lane.
# Variables:
#   $judgment (String) - the best judgment as the ruleset names it ("Fantastic")
results-panel-lanes = By lane · not { $judgment }

# The same where the ruleset names no judgment.
results-panel-lanes-any = By lane · not the best

# The hits by their notes' places in the beat (quarter notes, eighths…):
# a row each, its count, its mean error and spread.
results-panel-beats = By beat

# Its note: what each row's numbers are.
results-beats-note = mean · σ ms

results-beats-none = No notes were judged.

# A play kept by a version of the game that did not record where in the
# beat each note fell.
results-beats-not-kept = This play was kept before notes' places in the beat were.

# A row's name: the notes at that place in the beat, named as notes are
# (quarter notes are "4th", sixteenth notes "16th", a 32nd note "32nd").
# Variables:
#   $n (Number) - the beat's division: 4, 8, 12, 16, 24, 32, 48, 64 or 192
results-beat = { NUMBER($n, type: "ordinal") ->
    [one] { $n }st
    [two] { $n }nd
    [few] { $n }rd
   *[other] { $n }th
}

# The row of the rarer places in the beat, together.
results-beat-other = Other

# The developer screens' sample results (`--screen results --sample`): a
# made-up play.
results-sample-note = Sample play: the numbers are illustrative.

# The title of the results' More menu (a dialog listing the actions).
results-more-title = Results
