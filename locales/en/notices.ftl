## Song select's notices: a line over the song list (cut to fit, the full
## text in a dialog), each perhaps with a link after it (a short label,
## about 20 letters, written in lowercase as a phrase).

## The web's: what the browser keeps.

# The songs imported into this browser could not be opened.
# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
notices-imported-unavailable = Imported songs are unavailable: { $error }

# Songs stored in this browser that could not be read (a broken import,
# an older version's records).
# Variables:
#   $count (Number) - how many
notices-unreadable = { $count ->
    [one] { $count } stored song could not be read.
   *[other] { $count } stored songs could not be read.
}

# The link after it: removes those songs (asked first).
notices-remove-unreadable = remove them

# A controller connected that plays none of the lanes of the style shown.
# Variables:
#   $controller (String) - the controller's name, as the browser gives it
#   $style (String) - the style's name, as song select shows it ("Single")
notices-controller-unbound = Controller “{ $controller }” has no bindings yet for this style ({ $style }).

# The link after it: opens the options to bind its buttons.
notices-bind-controller = bind it in Options

# How much the game keeps in the browser's storage, and whether the browser
# promised to keep it ("yes"), may clear it when space runs low ("no", with
# a link asking it to keep it), or did not say ("unknown").
# Variables:
#   $size (String) - how much, as the common-size messages write it ("37 MB")
#   $kept (String) - "yes", "no" or "unknown"
notices-storage = { $kept ->
    [yes] This site stores about { $size } in this browser. The browser keeps your songs until you remove them.
    [no] This site stores about { $size } in this browser. The browser may clear imported songs when space runs low.
   *[unknown] This site stores about { $size } in this browser.
}

# The link after the storage notice: asks the browser to keep the songs
# (it may ask the player).
notices-keep-songs = keep them

## Desktop's.

# Files chosen to add as songs that are not zips: a folder of songs is
# added with the import dialog's “Choose folder” button (write its name
# as that button is translated).
# Variables:
#   $files (String) - the files' names, separated by ","
notices-only-zips = Only zips are read as packs; left out: { $files }. A folder of songs is added with “choose folder”.

# The chart editor could not make a new song from the music file chosen.
# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
notices-new-song-failed = The new song could not be made: { $error }
