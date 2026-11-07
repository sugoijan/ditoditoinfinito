## Devices: the sound outputs and displays the game keeps an offset for,
## as Options' device rows name them (Timing, after “Audio: ” or
## “Display: ”) and calibration's page; and controllers' controls where
## the game knows no legend printed on them, as Options' binding rows and
## dialogs and the play's quit hint name them. A device's line sits in a
## row's label, room for about 60 letters; a control's name in a short
## list or inside a sentence (“button 3 added to Left.”).

# A sound output, as the browser tells it apart (it is told no device's
# name): its sample rate and its output latency, with what the latency
# suggests of how it is connected.
# Variables:
#   $rate (Number) - its sample rate in kHz (48, 44.1)
#   $latency (Number) - its output latency in milliseconds, rounded (not written when $link is "unknown")
#   $link (String) - "wired" for a short latency (built-in speakers, wired headphones), "wireless" for a long one (Bluetooth, most likely), "unknown" when the browser reports no latency, "none" when nothing is guessed
devices-audio = { $link ->
    [wired] { $rate } kHz · ~{ $latency } ms output latency (wired/built-in?)
    [wireless] { $rate } kHz · ~{ $latency } ms output latency (wireless?)
    [unknown] { $rate } kHz · output latency unknown
   *[none] { $rate } kHz · ~{ $latency } ms output latency
}

# A sound output by its name (desktop), as above.
# Variables:
#   $name (String) - its name as the system gives it ("MacBook Pro Speakers")
#   $rate (Number) - its sample rate in kHz (48, 44.1)
#   $latency (Number) - its output latency in milliseconds, rounded
devices-audio-named = { $name } · { $rate } kHz · ~{ $latency } ms output latency

# A display, as the browser tells it apart: its size in pixels and its
# scale (how many of its pixels draw one: 2 on a high-density screen).
# Variables:
#   $width (String) - its width in pixels ("1512")
#   $height (String) - its height in pixels ("982")
#   $scale (Number) - its scale (1, 1.25, 2)
devices-display = { $width }×{ $height } ({ $scale }×)

# As above, with the refresh rate measured when it was seen.
# Variables:
#   $width (String) - its width in pixels ("1512")
#   $height (String) - its height in pixels ("982")
#   $scale (Number) - its scale (1, 1.25, 2)
#   $hz (Number) - its refresh rate in Hz (60, 120)
devices-display-refresh = { $width }×{ $height } ({ $scale }×), ~{ $hz } Hz

# A display by its name (desktop), as above.
# Variables:
#   $name (String) - its name as the system gives it ("DELL U2720Q")
#   $width (String) - its width in pixels ("3840")
#   $height (String) - its height in pixels ("2160")
#   $scale (Number) - its scale (1, 1.5, 2)
devices-display-named = { $name } · { $width }×{ $height } · ×{ $scale }

# A controller's button by its number, where the game knows no legend for
# it (a controller without the standard layout: a dance pad, an adapter).
# Shown in a list of controls, inside a sentence, and beside a round cap
# printing its number.
# Variables:
#   $number (String) - the button's number as the system numbers it ("3")
devices-button = button { $number }

# One end of a controller's axis (a stick, a trigger, or arrows the
# controller reports as an axis), by the axis's number, as above.
# Variables:
#   $number (String) - the axis's number ("1")
#   $direction (String) - "plus" or "minus": the end pressed, written + or −
devices-axis = { $direction ->
    [plus] axis { $number } +
   *[minus] axis { $number } −
}

# One direction of a controller's hat switch (a directional pad the
# controller reports as one control), by the hat's number, as above.
# Variables:
#   $number (String) - the hat's number ("0")
#   $direction (String) - "up", "right", "down" or "left"
devices-hat = { $direction ->
    [up] hat { $number } up
    [right] hat { $number } right
    [down] hat { $number } down
   *[left] hat { $number } left
}

# The four face buttons of a controller whose brand the game does not
# know, by their places in the diamond (the game draws them without
# letters). In lists of controls and in the play's quit hint (“hold or
# double-tap South to quit”). A word or two.
devices-face-south = South
devices-face-east = East
devices-face-west = West
devices-face-north = North

# Joins a lane's controls into a list (Options' binding rows), a pair at
# a time: a longer list is its first control and the rest, joined so in
# turn.
# Variables:
#   $first (String) - a control ("LB", "button 3")
#   $rest (String) - the next control, or the rest of the list joined so
devices-list = { $first }, { $rest }
