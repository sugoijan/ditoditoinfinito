## Shared by the screens.

# The game's name. Never translated.
-game-name = Dito Dito Infinito

# The game's name, where a screen shows it as text (the start menu's title
# when its art is missing).
common-game-name = { -game-name }

# A key hint in a screen's footer: the keys (or a controller's buttons)
# that move between choices, then what they do.
# Variables:
#   $keys (String) - the keys' names as their caps show them ("↑↓", a controller's D-pad)
common-hint-move = { $keys } Move

# A key hint: the key (or button) that chooses, then what it does.
# Variables:
#   $keys (String) - the key's name as its cap shows it ("ENTER", a controller's button)
common-hint-choose = { $keys } Choose

## Words every screen uses, alone on a button, a link or after a key hint's
## caps ("ESC Back"). Short: a button 80-120 px wide, about 10 letters.

# Leaves the screen or the dialog for the one before.
common-back = Back

# Closes a dialog or a question without doing what it offered.
common-cancel = Cancel

# Closes a dialog or a panel that only shows something.
common-close = Close

# Opens a menu of a screen's further actions.
common-more = More

# A switch's two values, as Options' rows show them.
common-on = On
common-off = Off

common-save = Save

# Puts something back as it was at first (settings, keys, an offset).
common-reset = Reset

# Opens a file, a folder, a link or a share.
common-open = Open

# Puts a link or a text on the clipboard.
common-copy = Copy

# Goes to the timing tests (calibration), from a screen's footer.
common-calibrate = Calibrate

## Styles: the chart layouts as the screens name them (song select's style
## tabs, about 14 letters; a chart's line, "Hard 13 · Single"; the field
## preview's layout). StepMania's games keep their names (Pump, Techno).

common-style-dance-single = Single
common-style-dance-solo = Solo
common-style-dance-double = Double
common-style-techno-single8 = Techno 8-panel
common-style-pump-single = Pump single
common-style-pump-halfdouble = Pump half-double
common-style-pump-double = Pump double

# A Dancing☆Onigiri key mode, named by its keys as Dancing☆Onigiri names it.
# Variables:
#   $mode (String) - the mode ("5", "7", "9A", "9B")
common-style-danoni = { $mode } keys

# A chart's layout this game does not know: a plain field of lanes.
# Variables:
#   $count (Number) - its lanes
common-style-lanes = { $count ->
    [one] { $count } lane
   *[other] { $count } lanes
}

## Lanes: an arrow's direction or a panel, as Options' key rows and the
## results' lanes panel name them (a short label, about 10 letters; a
## Dancing☆Onigiri lane has its place before it, "2 · Down-left").

common-lane-left = Left
common-lane-up-left = Up-left
common-lane-up = Up
common-lane-up-right = Up-right
common-lane-right = Right
common-lane-down-right = Down-right
common-lane-down = Down
common-lane-down-left = Down-left

# A five-panel pad's middle panel (Pump), or a Dancing☆Onigiri center lane.
common-lane-center = Center

# Dancing☆Onigiri's rice-ball lane, as its players call it.
common-lane-onigiri = Onigiri

# Dancing☆Onigiri's two characters' lanes (Giko the cat, Iyo), as its
# works name them.
common-lane-giko = Giko
common-lane-iyo = Iyo

# A lane of a two-pad style (Double): a label, not a sentence.
# Variables:
#   $lane (String) - the lane's name, as above ("Left")
#   $pad (Number) - its pad, 1 or 2
common-lane-on-pad = { $lane } (pad { $pad })

## Sizes: an amount of data (a file, a download, what the browser
## stores), in steps of 1000: B, kB, MB, GB. Below 10 of a unit the
## amount shows a tenth ("4.2 MB"), from 10 a whole number ("37 MB").

# A size in bytes, below 1000.
# Variables:
#   $amount (Number) - bytes, a whole number
common-size-b = { $amount } B

# A size in kilobytes (1000 bytes).
# Variables:
#   $amount (Number) - kilobytes: to a tenth below 10, else whole
#   $tenths (String) - "yes" when the amount shows a tenth, else "no"
common-size-kb = { $tenths ->
    [yes] { NUMBER($amount, minimumFractionDigits: 1) } kB
   *[no] { $amount } kB
}

# A size in megabytes (1000 kilobytes).
# Variables:
#   $amount (Number) - megabytes: to a tenth below 10, else whole
#   $tenths (String) - "yes" when the amount shows a tenth, else "no"
common-size-mb = { $tenths ->
    [yes] { NUMBER($amount, minimumFractionDigits: 1) } MB
   *[no] { $amount } MB
}

# A size in gigabytes (1000 megabytes).
# Variables:
#   $amount (Number) - gigabytes: to a tenth below 10, else whole
#   $tenths (String) - "yes" when the amount shows a tenth, else "no"
common-size-gb = { $tenths ->
    [yes] { NUMBER($amount, minimumFractionDigits: 1) } GB
   *[no] { $amount } GB
}
