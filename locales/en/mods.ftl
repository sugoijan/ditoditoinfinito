## The mods line: the options a play was played with, as tags with " · "
## between them ("×2 · reverse · mirror"), under a chart on song select,
## on the start prompt and on the results. Lower case in English, as the
## line reads; short (one or two words each). The speed ("×2", "C400") is
## a number, never translated.

# Receptors at the bottom, notes scrolling down.
mods-reverse = reverse

# Turns: the lanes mirrored, turned left or right, shuffled.
mods-mirror = mirror
mods-left = left
mods-right = right
mods-shuffle = shuffle

# A shuffle with the order it gave, the lanes' arrows in their new order.
# Variables:
#   $order (String) - the arrows in order ("↓←→↑")
mods-shuffle-order = shuffle { $order }

# Appearance: notes vanish before the receptors (hidden), appear late
# (sudden), both, or are never shown (stealth).
mods-hidden = hidden
mods-sudden = sudden
mods-hidden-sudden = hidden + sudden
mods-stealth = stealth

# Scroll actions: notes speed up (boost), slow down (brake) or sway in
# speed (wave) as they come.
mods-boost = boost
mods-brake = brake
mods-wave = wave

# Notes cut to the quarter notes only, or to quarters and eighths.
mods-quarters = quarter notes
mods-eighths = quarters and eighths

# No more than two notes at once; no jumps (two at once) at all.
mods-up-to-two = up to two at once
mods-no-jumps = no jumps

mods-no-holds = no holds

# No two notes far apart at once (wide "stretch" jumps left out).
mods-no-stretch = no stretch

# The lane cover (a panel over the lanes, DDR's SUDDEN+ and HIDDEN+), how
# much of the lanes it covers.
# Variables:
#   $percent (Number) - the share covered, 0 to 100
mods-cover-sudden = sudden+ { $percent }%
# Variables:
#   $percent (Number) - the share covered, 0 to 100
mods-cover-hidden = hidden+ { $percent }%
# Both covers at once.
# Variables:
#   $hidden (Number) - hidden+'s share, 0 to 100
#   $sudden (Number) - sudden+'s share, 0 to 100
mods-cover-both = hidden+ { $hidden }% sudden+ { $sudden }%

# StepMania's fail options other than failing at once: the play goes on
# after the life bar empties (fail delayed), fails only if it is empty at
# the end (fail at end), or never fails (fail off).
mods-fail-delayed = fail delayed
mods-fail-at-end = fail at end
mods-fail-off = fail off

# StepMania's battery life: a play fails after this many misses.
# Variables:
#   $lives (Number) - the lives
mods-battery = { $lives ->
    [one] battery { $lives } life
   *[other] battery { $lives } lives
}

# DDI's options that differ from their defaults: its timing difficulty
# (Easy or Hard; said with "timing" so that it is not read as a chart's
# difficulty), stray presses counted as Boos (a judgment's name, never
# translated), and Practice (an empty life does not end the play).
mods-ddi-easy = easy timing
mods-ddi-hard = hard timing
mods-ddi-boos = Boos
mods-ddi-practice = practice

# StepMania's life difficulty level (1 to 7; 4 by default).
# Variables:
#   $level (Number) - the level
mods-life = life { $level }

# StepMania's timing difficulty level, its windows' scale (1 to 8, then
# "Justice", as StepMania names the strictest).
# Variables:
#   $level (String) - the level ("5", "Justice")
mods-judge = judge { $level }
