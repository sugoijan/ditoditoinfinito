# Find songs in a first form (desktop only; docs/plans/source-plugins.md,
# stage 2): a page that searches song sources (folders on this computer,
# and plugins the player installed) without adding what it finds to the
# library, plays a song found, or imports it. A source's own name and
# words, songs' titles and folders' paths are data, never these messages.

# Song select's More item that opens the page (desktop, while song
# sources are on). A menu item.
sources-more-find = Find songs…

# The page's title beside its back icon on a compact screen.
sources-title = Find songs

# The page's header, in capitals where the language has them.
sources-heading = FIND SONGS

# The page's first line, muted, wrapped: what the page does.
sources-intro = Search a source for songs. What you find plays without being added to your library, unless you import it.

# Where no source is on (the page is reached only when one is; a source
# switched off since).
sources-no-sources = No song source is on.

# The subheading over the source buttons, where there are several.
sources-sources-heading = Source

# The built-in source that searches folders on the player's computer: a
# button, and in lines.
sources-this-computer = This computer

# Where only one source is on: which it is.
# Variables:
#   $source (String) - the source's name (This computer, or a plugin's own name)
sources-source-line = Searching: { $source }

# The subheading over the folders This computer searches.
sources-folders-heading = Folders to search

# Where no folder is named yet: muted, wrapped.
sources-folders-none = Add a folder to search it. Nothing in it is added to your library.

# A folder's button: stop searching that folder (nothing is deleted).
sources-folder-remove = Remove

# The button that opens the system's folder picker.
sources-folder-add = Add folder…

# The search field's label.
sources-search-label = Search

# The search field's hint, while it is empty.
sources-search-hint = Title, artist or pack

# The search button (an empty search lists everything).
sources-search = Search

# While a search runs.
sources-searching = Searching…

# While a result is fetched for Play or Import.
# Variables:
#   $title (String) - the result's title
#   $step (String) - what the source is doing (one of the step messages)
sources-fetching = “{ $title }”: { $step }

# What a source is doing, in the line above.
sources-step-resolving = finding it…
sources-step-searching = searching…
sources-step-downloading = downloading…
sources-step-reading = reading…
sources-step-converting = converting…
sources-step-checking = checking…

# How far a fetch is, under its line.
# Variables:
#   $percent (Number) - 0 to 100
sources-progress = { $percent } %

# While a result is written into the library.
# Variables:
#   $title (String) - the result's title
sources-importing = Importing “{ $title }”…

# The fold that shows the source's own words on a problem (data).
sources-details = Details

# After an import: its songs are in the library now.
# Variables:
#   $songs (Number) - how many songs were imported
sources-imported = { $songs ->
    [one] Imported one song. It is in your library now.
   *[other] Imported { $songs } songs. They are in your library now.
}

# Where a folder was too large to read whole: muted.
sources-partial = Some folders were too large to search whole: only part of them was searched.

# The subheading over the results.
sources-results-heading = Results

# Where a search found nothing.
sources-results-none = Nothing found.

# How many results are shown.
# Variables:
#   $count (Number) - results shown
sources-results-count = { $count ->
    [one] One result.
   *[other] { $count } results.
}

# How many results are shown, of how many the source has.
# Variables:
#   $count (Number) - results shown
#   $total (Number) - results in all
sources-results-of = { $count } of { $total } results.

# A tag of a result's line under its title (its artist, its pack, its
# charts, shown joined by " · "): the library has the same song.
sources-result-in-library = In your library

# A tag of a result's line: one of its charts.
# Variables:
#   $style (String) - the chart's style, by its name (Single, Double…)
#   $meter (Number) - its level
sources-chart = { $style } { $meter }

# A result's buttons: play it without importing it; import it into the
# library.
sources-play = Play
sources-import = Import

# The button under the results that asks the source for its next page.
sources-more = More results

# Problems, each a sentence: a source's failure as the game words it.
sources-problem-busy = The source is busy. Try again in a moment.
sources-problem-invalid-input = The source could not use what it was given.
sources-problem-unsupported = The source does not offer this.
sources-problem-not-found = The source does not have this song any more.
sources-problem-refused = The source refused.
sources-problem-rate-limited = The source asks you to wait before trying again.
sources-problem-unreachable = The source could not be reached.
sources-problem-too-large = This is too large to fetch.
sources-problem-unreadable = No song could be read from what the source sent.
sources-problem-changed = This changed at the source. Search again.
sources-problem-failed = The source failed.
sources-problem-crashed = The source stopped working.
sources-problem-timed-out = The source took too long and was stopped.
sources-problem-misbehaved = The source sent something the game does not accept, and was stopped.
sources-problem-not-started = The source could not be started.
sources-problem-needs-newer-game = This source needs a newer version of the game.
sources-problem-too-old = This source is too old for this version of the game.
sources-problem-off-for-the-run = The source failed too often. It is off until the game starts again.
sources-problem-switched-off = The source is not switched on.
sources-problem-stopped = The source was stopped.
sources-problem-import-failed = The song could not be written into your library.

# Song select over a result's songs, played without importing them: its
# header, in capitals where the language has them.
sources-select-title = FOUND SONGS

# Its footer's line, where nothing else is said: where the songs are
# from, and that a play is kept in the play history only where the
# library has the same song.
# Variables:
#   $source (String) - the source's name (This computer, or a plugin's)
sources-select-note = From { $source }. Plays of songs not in your library are not kept.

# The filters This computer offers a search, by their labels (a source's
# filter, drawn by the real Find songs screen later): the charts' style,
# and their level (a range of meters).
sources-filter-style = Style
sources-filter-level = Level
