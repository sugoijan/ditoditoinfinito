## The start prompt: a card over the song's picture, about 560 px wide
## (the width of about 45 Latin letters at the card's 15 px; on a phone
## upright 400), naming the song, its chart and how it will be played,
## with Start and Back. Its buttons are at least 104 px wide and grow
## with their label.

# The card's main button: starts the song (its background fills as the
# countdown to it runs). Also after the footer's key hint caps
# ("ENTER Start"). 18 px; about 6 letters before the button grows.
prompt-start = Start

# The footer's hint after a controller's buttons, which press the
# focused button (Start, unless another has the focus).
prompt-hint-activate = Activate

# The heading over this play's options (speed, reverse, turn, the
# noteskin…), 11 px in spaced capitals; changes there are for this play
# only.
prompt-modifiers = MODIFIERS

# A small button on the right of the options' heading, once an option
# differs from the saved settings: keeps them as the defaults. Sized to
# its text, beside the heading.
prompt-save-as-default = Save as default

# In that button's place once it was pressed: the options are now the
# defaults.
prompt-saved-as-default = Saved as default

## The controls diagram on the card: each lane's receptor with the key or
## the controller's button that plays it under it (a cell 48-72 px wide).

# Under a lane's receptor that no key or button plays (10 px, about 42 px
# wide).
prompt-unbound = unbound

# Under a controller's button in the diagram, where the field is split
# between two controllers (a Double chart): the half this controller
# plays (10 px, about 50 px wide).
# Variables:
#   $pad (Number) - the pad's number, 1 or 2
prompt-controller-pad = Pad { $pad }

## The quiet line on the card about the timing, from the plays kept on
## the devices in use: 13 px, beside the buttons when it fits there (then
## about 20 letters), else on a row of its own; a link "Calibrate"
## follows it.

# The kept plays' hits are consistently late.
# Variables:
#   $ms (Number) - how many milliseconds late, a whole number
prompt-timing-late = Hits run { $ms } ms late

# The kept plays' hits are consistently early.
# Variables:
#   $ms (Number) - how many milliseconds early, a whole number
prompt-timing-early = Hits run { $ms } ms early

# The devices in use were never calibrated and the kept plays say
# nothing yet.
prompt-timing-not-calibrated = Timing not calibrated

## The card's other steps: a sentence or two under the song (15 px,
## wrapped), then their buttons.

# Web only: the song plays from a folder the player linked, which the
# browser asks permission for again.
# Variables:
#   $folder (String) - the folder's name
prompt-access = This song plays from the folder “{ $folder }”, which the browser asks about again.

# Web only: the button asking the browser for that folder. 18 px; it
# takes a row of its own where it does not fit beside Back.
prompt-allow-folder = Allow reading the folder

# Web only: under it, in red, when the browser did not allow it.
prompt-access-refused = Reading the folder was not allowed.

# The heading when the play was cut off (18 px, bold).
prompt-interrupted = Interrupted

# Under that heading: why a play is cut off.
# Variables:
#   $platform (String) - web or desktop
prompt-interrupted-why = { $platform ->
    [web] The tab was hidden or audio was suspended.
   *[desktop] The window was hidden or the sound output changed.
}

# The button that starts the song again after it was cut off.
prompt-retry = Retry

# The loading screen, after Start while the play still loads: in the
# lower right corner (15 px), three dots stepping after it.
prompt-loading = Loading

## Why the song cannot play, in red on the card.

# Variables:
#   $error (String) - why, a clause (errors-load-…, or a failure's kind)
prompt-song-failed = The song failed to load: { $error }

# A kept play's replay that cannot be shown.
# Variables:
#   $why (String) - why, a sentence (the chart changed, a noteskin is missing…)
prompt-replay-refused = This replay cannot be watched. { $why }

# Desktop: a kept play's results that cannot be shown.
# Variables:
#   $why (String) - why, a sentence
prompt-results-refused = This play's results cannot be shown. { $why }

# The address names a chart in a style the game leaves out.
# Variables:
#   $style (String) - the style's id as the simfile names it ("kb7-single")
prompt-chart-style = This chart is for { $style }, a style this game does not play.

# The address names a chart the song does not have.
# Variables:
#   $chart (Number) - the chart's number in the song
prompt-no-chart = This song has no chart { $chart }.

## Quiet lines under the song while the play readies (13 px, wrapped).

# Web only: the browser lets sound start only from a press.
prompt-needs-gesture = This browser needs a tap, click or key press to start the audio.

# Web only: the background videos' decoder is coming down.
prompt-video-loading = loading the video decoder…

# The same, with how much of it has come.
# Variables:
#   $percent (Number) - how much has come, 0-100, a whole number
prompt-video-progress = loading the video decoder… { $percent } %

# Web only: the decoder could not be loaded.
# Variables:
#   $error (String) - why, a clause (errors-video-…)
prompt-video-failed = background videos cannot play: { $error }

# A song shared by link: its files coming down, their size not known yet.
prompt-download = downloading the song…

# The same, with what has come.
# Variables:
#   $done (String) - the size come so far, as the common-size messages write it ("4.2 MB")
prompt-download-done = downloading the song… { $done }

# The same, with what has come of how much.
# Variables:
#   $done (String) - the size come so far, as the common-size messages write it ("4.2 MB")
#   $total (String) - the size in all, the same way ("37 MB")
prompt-download-progress = downloading the song… { $done } of { $total }

# Desktop: a share's songs being read, before their files come down.
# Variables:
#   $done (Number) - songs read so far
#   $total (Number) - songs in the share
prompt-share-reading = { $total ->
    [one] reading the share… { $done } of { $total } song
   *[other] reading the share… { $done } of { $total } songs
}

## The play: a line at the bottom of the field (12 px or more, white)
## after the quit or the restart control was pressed once, or while it is
## held; a bar under it fills as it is held.

# Variables:
#   $control (String) - the key or the controller's button, as its cap prints it (Esc, B, Circle, ↑)
prompt-play-quit = hold or double-tap { $control } to quit

# A controller's control the game has no name for.
prompt-play-quit-this = hold or double-tap this control to quit

# A touch screen: the round ✕ button over the play.
prompt-play-quit-button = hold or double-tap the quit button

# Variables:
#   $key (String) - the key, as its cap prints it (R, \)
prompt-play-restart = hold or double-tap { $key } to restart
