## What the shells show outside the screens: the web's plain pages and
## the note over song select, desktop's app menu (macOS) and its file
## dialogs, and the entry the system's application menu lists the game
## by (Linux).

## The web's plain pages, in the browser's own fonts.

# The heading of the page shown when the browser gives the game no GPU.
shell-no-gpu-title = No graphics

# Under that heading; the browser's own reason (untranslated) follows on
# a line of its own.
shell-no-gpu = This browser gave the game no usable GPU (neither WebGPU nor WebGL2), so nothing can be drawn.

# A link back to the start menu, under a plain page.
shell-home = ← home

# The page for an address the game has no page for.
# Variables:
#   $address (String) - the address after the #, as it was typed
shell-not-found = There is no page at “{ $address }”.

# Over song select while files are dragged over the page.
shell-drop = Drop to add the songs

# Under song select when its list of songs could not be read.
# Variables:
#   $error (String) - why: its kind as errors-* say it, then the browser's own words (untranslated)
shell-song-list-failed = Could not load the song list: { $error }

## Desktop's app menu on macOS, named as macOS names these items in its
## own apps.

# Opens the panel with the game's name, version and copyright.
shell-menu-about = About { -game-name }

# The submenu of the system's services.
shell-menu-services = Services

# Hides the game's window (⌘H).
shell-menu-hide = Hide { -game-name }

# Hides every other app's windows (⌥⌘H).
shell-menu-hide-others = Hide Others

# Shows every app's windows again.
shell-menu-show-all = Show All

# Quits the game (⌘Q).
shell-menu-quit = Quit { -game-name }

## The kinds of file desktop's file dialogs offer, in the dialog's list
## of file types (Windows, Linux).

# Packs as zips (.zip, .smzip).
shell-file-zips = Zip archives

# Music files, for a new song.
shell-file-music = Music

# A chart saved as a StepMania file (.ssc, .sm).
shell-file-simfile = Simfile

## The entry Linux's application menus list the game by (its desktop
## entry); written into the package for each shipped language.

# What the game is, shown under or beside its name.
shell-entry-kind = Dance rhythm game

# The entry's description (a tooltip).
shell-entry-comment = A dance rhythm game that plays StepMania simfiles

# Words the menu's search finds the game by, each followed by a
# semicolon; English's work in every language as well.
shell-entry-keywords = dance;rhythm;music;stepmania;
