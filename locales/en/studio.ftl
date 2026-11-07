## The announcer studio (the web's, from Options → Sound): a page where
## someone records an announcer of their own. Every line the game speaks
## has a zone of its own, by a plain name, with when it speaks, examples
## to say, the recordings on it and buttons. The examples are an
## announcer's words in English or Japanese (data, never translated); the
## recordings' file names are data too.

# The labels of two buttons, as the sentences below name them: write them
# as the buttons are written (studio-add-recording without its "…",
# studio-take-which-line).
-studio-button-add-recording = Add recording
-studio-button-which-line = Which line?

# The page's name: the compact header shows it as written, the wide one in
# capitals.
studio-title = Announcer studio

# The page's first paragraph (wrapped, the page's width).
studio-intro = Drop each recording of your voice on its line below, or add it with the line's { -studio-button-add-recording } button. Then use your announcer in the game, or export it to share.

## The fields: a label at the left (about 12 letters), what was typed at
## the right, or a hint while it is empty.

studio-name = Name
studio-name-hint = Your announcer's name

# Who made the announcer, as its README credits them.
studio-author = By
studio-author-hint = Who made it

studio-license = License
studio-license-hint = How others may use it

## The page's buttons, side by side, each as wide as its text.

# Shows the next set of examples (each line's examples change language).
# Variables:
#   $set (String) - the set's name, written in its own language ("English", "日本語")
studio-examples = Examples: { $set }

# Stores the announcer as one the game can choose (Options → Sound).
studio-use-in-game = Use in the game

# Makes the announcer's zip, to save with the next press.
studio-export = Export…

# Saves the zip made (a download).
# Variables:
#   $file (String) - the zip's file name ("My Voice.zip")
studio-save = Save { $file }

# A warning under the buttons: the pack has more sounds, or more bytes,
# than the game keeps of an announcer.
# Variables:
#   $files (Number) - the most sounds an announcer keeps (2000)
#   $megabytes (Number) - the most megabytes it keeps (128)
studio-too-big = The pack is past what the game keeps of an announcer ({ $files } sounds, { $megabytes } MB): the last ones would be left out.

## Advanced: a fold (its label about 16 letters) holding the credits, the
## imports and three switches, each a button and a sentence under it.

studio-advanced = Advanced

studio-import-zip = Import a pack from a zip…
studio-import-folder = Import a pack folder…

# Under the two import buttons.
studio-import-description = An announcer pack made before, here or for StepMania: its recordings go on their lines.

# A switch's button: its name and its state, "on" or "off".
# Variables:
#   $state (String) - "on" or "off"
studio-tidy = { $state ->
    [on] Tidy recordings: on
   *[off] Tidy recordings: off
}

studio-tidy-description = Cuts the silence before and after each recording and evens its loudness with our announcers'.

# Variables:
#   $state (String) - "on" or "off"
studio-stepmania = { $state ->
    [on] Also for StepMania: on
   *[off] Also for StepMania: off
}

studio-stepmania-description = Writes only the sound formats StepMania reads (MP3, WAV and Ogg Vorbis), others as WAV.

# Shows more about each line and recording (their folders, the tidying).
# Variables:
#   $state (String) - "on" or "off"
studio-details = { $state ->
    [on] Details: on
   *[off] Details: off
}

studio-details-description = Shows each line's folder and what tidying did to each recording.

## Recordings no line was found for yet, in a zone at the top.

# The zone's heading.
# Variables:
#   $count (Number) - how many recordings wait
studio-unsorted = Not sorted yet ({ $count })

studio-unsorted-help = Press { -studio-button-which-line } to hear a recording and choose its line, or drop it on its line below.

## The lines' sections: a heading each, then the zones of the lines the
## game says there.

studio-section-select = On the song select screen
studio-section-play = During gameplay
studio-section-results = On the results screen

## The lines' names: a zone's heading, a choice in the line picker, the
## scene's heading in quotes ("“Ready”"). Short: about 28 letters. Most
## name what the announcer says or the moment it says it.

studio-line-ready = Ready
studio-line-here-we-go = Here we go
studio-line-restart = Restart

# Comments said every 10 seconds of a play, by how it goes.
studio-line-comment-hot = Doing great
studio-line-comment-good = Doing fine
studio-line-comment-danger = In danger

# The comment on a life gauge of lives (a battery).
studio-line-comment-oni = On lives

# A combo reached (every hundred notes in a row).
# Variables:
#   $combo (String) - the combo ("100", "1500")
studio-line-combo = { $combo } combo

# Past 1000 notes in a row, each hundred.
studio-line-combo-overflow = More combo

studio-line-combo-stopped = Combo broken
studio-line-full-combo = Full combo

# A full combo with every note at the best judgment.
studio-line-full-combo-w1 = Full combo, all perfect

# A full combo with every note at the second judgment or better.
studio-line-full-combo-w2 = Full combo, nearly perfect

# A full combo with every note at the third judgment or better.
studio-line-full-combo-w3 = Full combo, all good

studio-line-last-life = Last life
studio-line-life-lost = Life lost
studio-line-cleared = Cleared
studio-line-failed = Failed
studio-line-quit = Quit
studio-line-first-clear = First clear
studio-line-new-record = New record

# The results' line for a grade.
# Variables:
#   $grade (String) - the grade's letters as StepMania names them ("AAA", "B")
studio-line-grade = Grade { $grade }

# The results' line for a failed play's grade.
# Variables:
#   $grade (String) - the grade's letter as StepMania names it ("E")
studio-line-grade-failed = Failed play ({ $grade })

# Said after the results' line of a play graded well.
studio-line-cheer = Cheer

# Said as song select shows.
studio-line-select-intro = Song select

# Said as a chart is chosen on song select, by the chart.
studio-line-select-new = A new chart
studio-line-select-repeat = Played again
studio-line-select-hard = A hard chart
studio-line-select-general = Any other chart

## When each line is said: a sentence under the line's name, wrapped.

studio-when-ready = As a song starts.
studio-when-here-we-go = Just before the first note: it ends half a second before it.
studio-when-restart = As a song starts again at once (R held, or pressed twice).
studio-when-comment-hot = Every 10 seconds while the life is full.
studio-when-comment-good = Every 10 seconds otherwise.
studio-when-comment-danger = Every 10 seconds while the life is about to run out.
studio-when-comment-oni = Every 10 seconds on a life gauge of lives (a battery).

# A combo line.
# Variables:
#   $count (Number) - the combo (100, 1500)
studio-when-combo = { $count } notes in a row.

# A combo line past 1000: the game says it where it has it, else More
# combo (studio-line-combo-overflow).
# Variables:
#   $count (Number) - the combo (1100, 1500)
studio-when-combo-past-1000 = { $count } notes in a row (in place of More combo).

studio-when-combo-overflow = Each hundred notes in a row past 1000.
studio-when-combo-stopped = When more than 50 notes in a row end with a miss.
studio-when-full-combo = As the last note is hit with no note missed.
studio-when-full-combo-w1 = As the last note is hit, every note at the best judgment.
studio-when-full-combo-w2 = As the last note is hit, every note at the second judgment or better.
studio-when-full-combo-w3 = As the last note is hit, every note at the third judgment or better.
studio-when-last-life = When one life is left on a gauge of lives.
studio-when-life-lost = When a life is lost on a gauge of lives.
studio-when-cleared = As the last note is judged.
studio-when-failed = When the life runs out.
studio-when-quit = On the results of a song the player quit.
studio-when-first-clear = On the results, the first time a chart is cleared.
studio-when-new-record = On the results, for a new best on a chart played before.
studio-when-evaluation-full-combo-w1 = On the results of a full combo with every note at the best judgment.
studio-when-evaluation-full-combo-w2 = On the results of a full combo with every note at the second judgment or better.
studio-when-evaluation-full-combo-w3 = On the results of a full combo with every note at the third judgment or better.
studio-when-evaluation-full-combo = On the results of any other full combo.

# The best grade there is (AAAAA, Etterna's), which no rules give.
studio-when-grade-aaaaa = Never: no rules give this grade.

# The results' lines for the grades, best first (AAAA, AAA, AA, A, B, C,
# D: StepMania's names for them, whatever the ruleset calls its grades).
studio-when-grade-aaaa = On the results, for the best grade.
studio-when-grade-aaa = On the results, for the second best grade.
studio-when-grade-aa = On the results, for the third best grade.
studio-when-grade-a = On the results, for the fourth grade.
studio-when-grade-b = On the results, for the fifth grade.
studio-when-grade-c = On the results, for the sixth grade.
studio-when-grade-d = On the results, for the lowest passing grade.
studio-when-grade-failed = On the results of a failed song.

studio-when-cheer = 2.5 seconds after the results' line, for the third best grade or better.
studio-when-select-intro = Each time the song list shows, after a song too.
studio-when-select-new = When a chart is chosen that was never played under these rules.
studio-when-select-repeat = When a chart is chosen that was played earlier this session.
studio-when-select-hard = When a chart of level 10 or more is chosen.
studio-when-select-general = When any other chart is chosen.

## A line's zone, under its name and when it is said.

# With the details on: the folder a pack keeps the line's sounds in.
# Variables:
#   $folder (String) - the folder, as StepMania names it ("gameplay ready")
studio-line-folder = Folder: { $folder }

# A fold of the line's examples (its label, about 16 letters).
# Variables:
#   $count (Number) - how many examples
studio-examples-fold = Examples ({ $count })

# An example in the fold: what an announcer might say, in quotes (a
# play button's label where our announcer's sound of it can be heard).
# Variables:
#   $words (String) - the example's words, in its set's language
studio-example = “{ $words }”

# Where the line has no recording: what the game says instead.
# Variables:
#   $line (String) - the line said in its place, by its name (studio-line-…)
studio-missing-instead = Nothing yet: “{ $line }” is said instead.

studio-missing-next-line = Nothing yet: the results' next line is said instead.
studio-missing-silent = Nothing yet: the game says nothing then.

# Under Here we go: Ready's recordings (or Restart's) and Here we go's,
# the longest of each, take this long together, so a chart whose first
# note comes sooner gets only Here we go (the lines' names, as above).
# Variables:
#   $seconds (String) - seconds, one decimal ("4.3")
studio-opening-long = Ready and here we go take { $seconds } s together: a song whose first note comes sooner says only here we go.

# The line's buttons, as wide as their texts.
studio-add-recording = { -studio-button-add-recording }…

# Shows the line's scene: the screen the game says it on, the recording
# heard in it.
studio-hear-recorded = Hear it in the game

# The same where the line has no recording: our announcer's example is
# heard.
studio-hear-example = Hear an example in the game

## A recording on a line: a button that plays it (its file's name and
## length, after ▶), then buttons to move or remove it.

# Variables:
#   $file (String) - its file's name ("ready 2.m4a")
#   $seconds (String) - its length in seconds, one decimal ("1.4")
studio-take = { $file } · { $seconds } s

# Variables:
#   $file (String) - its file's name
studio-take-unplayable = { $file } · cannot be played

# Its length is not known yet: it is being decoded.
# Variables:
#   $file (String) - its file's name
studio-take-reading = { $file } · reading…

# Puts it on another line (opens the line picker).
studio-take-move = Move…

# On a recording not sorted yet: hears it and chooses its line.
studio-take-which-line = { -studio-button-which-line }

studio-take-remove = Remove

## A recording's notes (warnings, in the theme's gold, wrapped).

# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
studio-note-unreadable = This file cannot be played here ({ $error }).

studio-note-clipped = It was recorded too loud: it crackles where it peaks.

# Variables:
#   $seconds (String) - its length in seconds, one decimal ("3.1")
#   $limit (String) - what its moment allows, seconds, one decimal ("2.0")
studio-note-too-long = At { $seconds } s it runs past what comes next in the game ({ $limit } s).

## With the details on: what tidying did to a recording (one line, muted).

studio-tidied-as-recorded = As recorded
studio-tidied-nothing = Tidy: nothing to change

# What tidying changed: the silence cut at the ends, the loudness evened.
# Variables:
#   $change (String) - "cut", "louder", "quieter", "cut-louder" or "cut-quieter"
#   $seconds (String) - the silence cut, seconds, one decimal ("0.6")
#   $db (Number) - by how many decibels it was made louder or quieter
studio-tidied = { $change ->
    [cut] Tidied: { $seconds } s of silence cut
    [louder] Tidied: louder by { $db } dB
    [quieter] Tidied: quieter by { $db } dB
    [cut-louder] Tidied: { $seconds } s of silence cut, louder by { $db } dB
   *[cut-quieter] Tidied: { $seconds } s of silence cut, quieter by { $db } dB
}

## The line picker: a dialog listing every line, from Which line? and
## Move…; the recording plays meanwhile.

studio-picker-title = Which line is it?

# Its first choice, on a recording on a line: back to the unsorted.
studio-picker-unsorted = Not sorted yet

# A choice: a line's name, then the screen it is said on.
# Variables:
#   $line (String) - the line's name (studio-line-…)
studio-picker-line-select = { $line } · Song select screen

# Variables:
#   $line (String) - the line's name (studio-line-…)
studio-picker-line-play = { $line } · Gameplay

# Variables:
#   $line (String) - the line's name (studio-line-…)
studio-picker-line-results = { $line } · Results screen

# Under the title.
# Variables:
#   $file (String) - the recording's file name
studio-picker-note = { $file } plays as you choose.

## The status line under the page's buttons: what the studio is doing, or
## did (a sentence or two, wrapped). Errors are as the browser says them.

# Variables:
#   $count (Number) - how many files were picked or dropped
studio-status-adding = { $count ->
    [one] Adding { $count } file…
   *[other] Adding { $count } files…
}

# Variables:
#   $added (Number) - recordings added
#   $passed (Number) - files passed over, which were no sounds (0: none)
studio-status-added = { $added ->
    [one] Added { $added } recording.
   *[other] Added { $added } recordings.
}{ $passed ->
    [0] {""}
    [one] {" "}{ $passed } file was passed over: not sounds.
   *[other] {" "}{ $passed } files were passed over: not sounds.
}

# The same, where the browser would not keep them (they are gone after a
# reload).
# Variables:
#   $added (Number) - recordings added
#   $passed (Number) - files passed over, which were no sounds (0: none)
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
studio-status-added-unkept = { $added ->
    [one] Added { $added } recording.
   *[other] Added { $added } recordings.
}{ $passed ->
    [0] {""}
    [one] {" "}{ $passed } file was passed over: not sounds.
   *[other] {" "}{ $passed } files were passed over: not sounds.
} They could not be kept in the browser: { $error }

# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
studio-status-files-unreadable = The files could not be read: { $error }

# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
studio-status-draft-unreadable = The draft could not be read: { $error }

studio-status-no-decoder = This browser cannot decode sounds here.

# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
studio-status-no-sound = No sound: { $error }

# A recording could not be played.
# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
studio-status-unplayable = It could not be played: { $error }

# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
studio-status-example-unplayable = The example could not be played: { $error }

# Our announcer's example could not be fetched or decoded for a scene.
# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
studio-status-example-unreadable = The example could not be read: { $error }

studio-status-exporting = Making the pack…

# The zip is made: the Save button saves it (studio-save, as written there).
# Variables:
#   $file (String) - the zip's file name ("My Voice.zip")
#   $count (Number) - the sounds in it
studio-status-exported = { $count ->
    [one] { $file } is ready, with { $count } sound: press Save { $file }.
   *[other] { $file } is ready, with { $count } sounds: press Save { $file }.
}

# Variables:
#   $error (String) - why
studio-status-not-exported = The pack could not be made: { $error }

# Variables:
#   $file (String) - the zip's file name
studio-status-saved = { $file } saved to your downloads.

studio-status-not-saved = The pack could not be saved.

studio-status-using = Adding it to the game…

# Use in the game is done: where to choose it (Options' section and row,
# as Options names them).
# Variables:
#   $name (String) - the announcer's name
studio-status-used = { $name } is in the game: choose it under Options, Sound, Announcer.

# Variables:
#   $error (String) - why
studio-status-not-used = It could not be added: { $error }

# A scene could not show its screen.
studio-status-no-select = Song select could not be shown.
studio-status-no-results = The results could not be shown.

# After a scene: nothing was said (the line and what speaks in its place
# have no recording).
studio-status-heard-nothing = Nothing was said in the game.

# After the cheer's scene with nothing said: why.
studio-status-heard-nothing-cheer = Nothing was said in the game: the cheer follows the results' line of a play graded AA or better, and none of those lines is recorded.

# After a scene: the line heard, by its name, said by the recordings.
# Variables:
#   $line (String) - the line's name (studio-line-…)
studio-status-heard = Heard in the game: “{ $line }”.

# Two lines heard, one after the other (song select's welcome then a
# comment; a results line then the cheer).
# Variables:
#   $first (String) - the first line's name (studio-line-…)
#   $then (String) - the second's
studio-status-heard-two = Heard in the game: “{ $first }”, then “{ $then }”.

# The same, said by our announcer's examples.
# Variables:
#   $pack (String) - our announcer's name ("Mr. Clank")
#   $line (String) - the line's name (studio-line-…)
studio-status-heard-example = Heard in the game, an example by { $pack }: “{ $line }”.

# Variables:
#   $pack (String) - our announcer's name ("Mr. Clank")
#   $first (String) - the first line's name (studio-line-…)
#   $then (String) - the second's
studio-status-heard-example-two = Heard in the game, an example by { $pack }: “{ $first }”, then “{ $then }”.

## A line's scene: the game's own screens shown smaller in a frame over
## the studio. Above the picture: a kicker, the line heard and whose voice
## says it; below: what happens (the caption) and the way back.

# The kicker above the line's name: small, in capitals where the
# language has them, tracked (about 16 letters).
studio-stage-kicker = IN THE GAME

# The line heard, in quotes (a heading, cut with "…" where it runs long).
# Variables:
#   $line (String) - the line's name (studio-line-…)
studio-stage-line = “{ $line }”

# Whose voice says it, at the heading's right.
studio-stage-voice-recording = Your recording

# Variables:
#   $pack (String) - our announcer's name ("Mr. Clank")
studio-stage-voice-example = An example by { $pack }

# A button 190 px wide (150 on a phone): about 18 letters.
studio-stage-back = Back to the studio

# The words shown over the picture as a recording is said, whose words
# are not known: the line's name in brackets, as subtitles mark a sound.
# Variables:
#   $line (String) - the line's name (studio-line-…)
studio-stage-said = [{ $line }]

## The scenes' captions: what happens on the screen shown, a sentence under
## the picture (one line, cut with "…" where it runs long).

studio-caption-start = The start menu: Singleplayer is chosen.
studio-caption-select-intro = Song select comes in.
studio-caption-select-new = A chart never played (no best on it) is chosen.
studio-caption-select-hard = A chart of level 10 or more, played before, is chosen.
studio-caption-select-repeat = A chart played earlier this session is chosen.
studio-caption-select-general = A chart played before, below level 10, is chosen.
studio-caption-quit = The player quits partway through the song.
studio-caption-first-clear = After the play: the chart is cleared for the first time.

# Variables:
#   $score (String) - the play's score as the results show it ("92.40%")
studio-caption-new-record = After the play: { $score }, better than ever on this chart.

# A full combo with every note at the best judgment.
# Variables:
#   $judgment (String) - the judgment as the ruleset names it ("Fantastic")
studio-caption-full-combo-best = A full combo: every note { $judgment }.

# Variables:
#   $judgment (String) - the judgment as the ruleset names it ("Excellent")
studio-caption-full-combo = A full combo: every note { $judgment } or better.

studio-caption-failed = The life runs out: the song is failed.

# Variables:
#   $grade (String) - the grade as the ruleset names it ("AA", "S+")
studio-caption-graded = A play graded { $grade }.

# Variables:
#   $grade (String) - the grade as the ruleset names it ("AA", "S+")
studio-caption-cheer = A play graded { $grade }: the cheer follows its line.

# The cheer after a new record's line.
# Variables:
#   $grade (String) - the grade as the ruleset names it ("AA", "S+")
studio-caption-cheer-record = A play graded { $grade }, a better score than ever: the cheer follows its line.

# The cheer after a first clear's line.
# Variables:
#   $grade (String) - the grade as the ruleset names it ("AA", "S+")
studio-caption-cheer-first-clear = A play graded { $grade }, cleared for the first time: the cheer follows its line.

# Song select before the play of a first clear's scene.
studio-caption-before-failed = Before the play: this chart was only ever failed.

# Song select before the play of a new record's scene.
# Variables:
#   $score (String) - the chart's best score as song select shows it ("90.15%")
studio-caption-before-best = Before the play: this chart's best is { $score }.
