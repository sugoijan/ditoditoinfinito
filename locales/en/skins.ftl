## The bundled noteskins' own options, under Options → Arrows for the
## skin chosen (docs/plans/skin-parameters.md, "The bundled skins'
## words"): each a row's label on the left (about 24 Latin letters
## wide), its choices on the right (a switch's are Off and On, the
## common words) and its description below when it is focused (a
## sentence or two, wrapped, about 90 letters a line). The game finds
## each message by its id, made from the skin's key and the option's
## name in its metrics.ini: `skins-<game>-<skin>-<option>` the label,
## `…-description` the description, `…-choice-<choice>` a choice's
## label. A skin the game does not ship shows its own words. At the
## Advanced level each English message says exactly what the skin's
## metrics.ini says.

### Infinito, the game's own arrows (assets/noteskins/dance/Infinito).

# How Infinito's notes are colored. Shown at the Basic level too.
skins-dance-infinito-colors = Note colors
# Vivid, Rainbow and Flat are the choices' names, as below.
skins-dance-infinito-colors-description = Vivid: colors flow through every arrow, each shifted by where it falls in the beat. Rainbow: arrows take one of four colors by where they fall in the beat. Flat: every arrow shows the same flowing colors.
    .advanced = Vivid shifts the spiral by note timing. Rainbow uses four timing-color families. Flat gives every note the same cycling spiral.
# A choice of Note colors: the game's own palette, which flows through
# each note shifted by its timing (the default).
skins-dance-infinito-colors-choice-vivid = Vivid
# A choice of Note colors: four color families by where a note falls in
# the beat.
skins-dance-infinito-colors-choice-rainbow = Rainbow
# A choice of Note colors: every note the same.
skins-dance-infinito-colors-choice-flat = Flat

# A switch: each note fills up (charges) over the beat before it is hit.
skins-dance-infinito-note-timed-charge = Charge by note timing
skins-dance-infinito-note-timed-charge-description = Give each note its own one-beat charge cycle, reaching full charge at its hit time. Lifts keep their reversed sequence and empty at release.

# A switch: shocks (a battle's attack notes, and mines) drawn in the
# shape of their lane's arrow.
skins-dance-infinito-shock-arrows = Shock arrows
skins-dance-infinito-shock-arrows-description = Use lane-shaped shock notes instead of the shared spiked mine. This changes their appearance only. In a multiplayer stage with attacks, a chart's own mines stay spiked: the lane-shaped notes are the attack's shocks.

# A switch: the onigiri lane's note drawn as a rice ball (a rounded
# triangle) or a square.
skins-dance-infinito-onigiri = Onigiri note
skins-dance-infinito-onigiri-description = Use the rounded rice triangle on Onigiri lanes. Disable to use the center square.

# A switch: the flashes drawn when a note is hit.
skins-dance-infinito-explosions = Explosions
skins-dance-infinito-explosions-description = Original SVG flashes for hits, holds, rolls and mines.

# A switch: the receptors (the targets the notes reach) filled faintly
# or hollow.
skins-dance-infinito-receptor-fill = Receptor interior
skins-dance-infinito-receptor-fill-description = Faint neutral fill follows the note's inset charge regions. Disable for a hollow receptor.

# A switch: each hit's burst turned at random.
skins-dance-infinito-hit-variation = Vary bursts
skins-dance-infinito-hit-variation-description = Randomizes broad-burst rotation on each hit. Arrow echoes and linked mine contours stay aligned; colors and timing stay fixed.

### Peters-DDR (assets/noteskins/dance/Peters-DDR).

# How the notes are colored.
skins-dance-peters-ddr-colors = Note colors
# Vivid and By subdivision are the choices' names, as below.
skins-dance-peters-ddr-colors-description = Vivid: colors cycle with the beat, shifted by subdivision. By subdivision: each rhythmic division has its own color.
# A choice of Note colors: colors cycling with the beat (the default).
skins-dance-peters-ddr-colors-choice-vivid = Vivid
# A choice of Note colors: a color for each rhythmic division (quarter
# notes, eighths, sixteenths…).
skins-dance-peters-ddr-colors-choice-quantized = By subdivision

# A switch: the flashes drawn when a note is hit.
skins-dance-peters-ddr-explosions = Explosions
skins-dance-peters-ddr-explosions-description = Flashes for hits, holds, rolls and mines. Receptor presses stay visible.
