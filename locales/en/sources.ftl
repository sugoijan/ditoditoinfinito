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

# The page's first line, muted, wrapped: what the page does, and what is
# kept of a song found and played (the plays, best scores and replays),
# which the page then lists under "Played before" (sources-played-heading).
sources-intro = Search a source for songs. What you find plays without being added to your library, unless you import it. Your plays of it are kept, with your best scores and replays, and listed here under Played before.

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
# from, and that the plays of them are kept (in the play history, with
# their bests; Find songs lists them under "Played before").
# Variables:
#   $source (String) - the source's name (This computer, or a plugin's)
sources-select-note = From { $source }. Your plays of these songs are kept.

# The filters This computer offers a search, by their labels (a source's
# filter, drawn by the real Find songs screen later): the charts' style,
# and their level (a range of meters).
sources-filter-style = Style
sources-filter-level = Level

# Find songs opened from an online room (stage 3): songs found added to
# the room's song list without being added to the library.

# The online room's More item that opens the page (desktop, while song
# sources are on, for whoever may add songs to the room). A menu item.
sources-more-find-room = Find songs to add…

# The page's first line from a room, muted, wrapped: what the page does
# there.
sources-intro-room = Search a source for songs to add to the room. A song you add is shared with the room from here, without being added to your library.

# A result's button from a room: add its songs to the room's song list.
sources-add-to-room = Add to room

# A tag of a result's line (joined by " · "): the room's list has the same
# song already.
sources-result-in-room = In the room

# A tag of a result's line (joined by " · "), where its buttons are fewer:
# its source (or the source for this song) lets it be played on this
# computer and nothing more, so it has no Import button and, from an
# online room, no Add to room button.
sources-result-play-only = Play only: its source does not allow importing it or adding it to rooms

# A tag of a result's line, where its buttons are fewer: its source (or
# the source for this song) lets it be played and added to online rooms
# but not imported into the library, so it has no Import button.
sources-result-not-for-keeping = Not for importing: its source allows playing it and adding it to rooms only

# After Add to room: its songs are in the room's list; each is uploaded
# from this computer, as any song added to a room.
# Variables:
#   $songs (Number) - how many songs were added
sources-added-to-room = { $songs ->
    [one] Added to the room. It is uploaded from this computer and not added to your library.
   *[other] Added { $songs } songs to the room. They are uploaded from this computer and not added to your library.
}

# An online room's My songs: the pack of songs found with Find songs in
# this run and not imported (a pack's header, and the song's pack line).
sources-pack-fetched = Fetched this session

# Songs played before (every play of a song found is kept): the page lists
# the songs played from the source chosen, each with Play, which fetches it
# again from its source; and those played from a source not on now.

# The subheading over the songs played before from the source chosen.
sources-played-heading = Played before

# The subheading over the songs played before from a source that is not
# on now (switched off, or no longer installed).
# Variables:
#   $source (String) - the source's name (This computer, or a plugin's own name)
sources-played-heading-from = Played before from { $source }

# A tag of a played song's line (joined by " · "): its chart's best.
# Variables:
#   $best (String) - the score and the grade as the lists write them (data)
sources-played-best = Best { $best }

# A tag of a played song's line: when it was last played.
# Variables:
#   $when (String) - a date and time, "2026-11-27 14:03" (data)
sources-played-last = Last played { $when }

# A tag of a played song's line where it has no Play button: its source
# is installed but not switched on (or This computer is off). Its plays
# stay kept.
sources-played-source-off = Its source is not on

# A tag of a played song's line where it has no Play button: its plugin
# is no longer installed. Its plays stay kept.
sources-played-not-installed = Its source is not installed

# A tag of a played song's line where it has no Play button: its source
# said it no longer has it (a folder moved or deleted, a song taken down).
# Its plays stay kept.
sources-played-gone = No longer at its source

# The fold under a source's first played songs that shows the rest.
# Variables:
#   $count (Number) - how many more are in the fold
sources-played-more = { $count ->
    [one] One more song played before
   *[other] { $count } more songs played before
}

# Muted, under a source's played songs: more were played from it than the
# page lists (the most recent are listed).
# Variables:
#   $count (Number) - how many are not listed
sources-played-unlisted = { $count ->
    [one] One more song played from this source is not listed here. Its plays are kept.
   *[other] { $count } more songs played from this source are not listed here. Their plays are kept.
}

# After Play on a song played before: it changed at its source since it
# was last played, and the source's current version was fetched. Plays of
# a chart whose notes changed stay with the old chart; charts that did not
# change keep their plays.
sources-changed = This song changed at its source since you last played it. Charts whose notes changed start with no plays; the plays of the old charts are kept.

# After Import of a song found on this computer: the folder it is in (its
# pack's folder, its own, or its zip) was added to the library's song
# folders (Options → Song list's folders), nothing copied.
# Variables:
#   $folder (String) - the folder's or the zip's name (data)
sources-added-folder = Added “{ $folder }” to your song folders. Its songs are in your library now.

# After Import of a song found on this computer that a song folder of the
# library holds already: nothing added.
# Variables:
#   $folder (String) - the song folder's name that holds it (data)
sources-in-folders = This song is in your song folder “{ $folder }” already, so it is in your library.

# Options → Plugins (desktop only, shown with Show advanced options On;
# docs/plans/source-plugins.md, stage 4): the plugins installed in the
# plugins folder, each switched on or off, and This computer's switch. A
# plugin's own name, version, publisher, page and hosts are data, never
# these messages.

# The section's name in Options' list of sections (a rail 208 px wide:
# about 18 letters).
sources-plugins-section = Plugins

# The section's lead, muted, wrapped over the rows' width above them: what
# a plugin is, what switching one on means, how to add one, and that the
# game never fetches one itself. Said once.
sources-plugins-lead = A plugin is a program that finds songs for Find songs. It runs on this computer with your access to your files and the network, like any program you run, and the game cannot check what it does: switch on only plugins from people you trust. To add one, put its folder in the plugins folder, come back here, choose Look again and switch it on. The game never downloads, installs or updates plugins.

# A row's label: the folder plugins are installed in.
sources-plugins-folder = Plugins folder

# The row's buttons: show the folder in the system's file browser; read
# the folder again (after adding, changing or removing a plugin).
sources-plugins-open-folder = Open folder
sources-plugins-look-again = Look again

# Beside the buttons, muted (about 14 letters): how many plugins the
# folder holds, none, or that it is being read.
# Variables:
#   $count (Number) - the plugins found (at least one)
sources-plugins-folder-count = { $count ->
    [one] One plugin
   *[other] { $count } plugins
}
sources-plugins-folder-none = None yet
sources-plugins-folder-looking = Looking…

# The row's sentence in Options' description panel.
# Variables:
#   $folder (String) - the folder's path (data)
sources-plugins-folder-description = Plugins are installed in { $folder }. Open folder shows it in your file browser; Look again reads it again after you add, change or remove a plugin.

# The heading over This computer's row (in capitals where the language has
# them).
sources-plugins-built-in-heading = BUILT INTO THE GAME

# This computer's row's sentence (its label is sources-this-computer),
# where no folder is chosen yet, and where some are.
sources-plugins-this-computer-description = Part of the game, not a plugin: Find songs searches folders on this computer that you choose there, without adding them to your library. No folder is chosen yet.
# Variables:
#   $folders (String) - the folders' paths, " · " between them (data)
sources-plugins-this-computer-folders = Part of the game, not a plugin: Find songs searches folders on this computer, without adding them to your library. It searches { $folders }. Choose the folders on Find songs.

# The heading over the plugins' rows (in capitals where the language has
# them).
sources-plugins-installed-heading = INSTALLED PLUGINS

# A row where the folder holds no plugin, muted, and its sentence.
sources-plugins-none = No plugin is installed.
sources-plugins-none-description = Put a plugin's folder in the plugins folder, then choose Look again.

# Beside a plugin's switch, muted (about 40 letters, cut where longer):
# what its folder says of it. The version and the publisher are the
# plugin's own words; the digest is the first characters of its program's
# SHA-256, for comparing with what its publisher lists.
# Variables:
#   $version (String) - its version (data)
#   $publisher (String) - its publisher as it names itself (data)
#   $sha (String) - the start of its program's SHA-256 (data)
sources-plugins-version-by = Version { $version } · { $publisher } · { $sha }

# Beside a plugin's switch in its version's place, where it cannot run now
# (about 40 letters, cut where longer): it failed three times this run;
# it did not start as its plugin.toml says; it speaks a newer protocol
# than this game; or an older one.
sources-plugins-state-stopped = Stopped working: off until the game starts again
sources-plugins-state-not-as-declared = Not as its plugin.toml says: off until the game starts again
sources-plugins-state-newer = Needs a newer version of the game
sources-plugins-state-older = Made for an older version of the game

# The first sentence of a plugin's description: what it is now. Off; on
# (a plugin's program runs only while Find songs asks it for something,
# and is stopped after a minute with nothing to do); failed three times
# this run; did not start as its plugin.toml says; too new; too old.
sources-plugins-now-off = Switched off: it does not run.
sources-plugins-now-on = Switched on: it runs only when Find songs asks it for songs, and stops after a minute with nothing to do.
sources-plugins-now-stopped = It stopped working three times, so it is off until you start the game again.
sources-plugins-now-not-as-declared = It did not start as its plugin.toml says, so it is off until you start the game again.
sources-plugins-now-newer = It needs a newer version of the game, so it cannot run.
sources-plugins-now-older = It was made for an older version of the game, so it cannot run.

# A plugin's description in Options' description panel (wrapped, six
# lines at most).
# Variables:
#   $state (String) - what it is now, a sentence (sources-plugins-now-*)
#   $version (String) - its version (data)
#   $publisher (String) - its publisher as it names itself (data)
#   $can (String) - what it does, sources-plugins-can-* with " · " between them
#   $reach (String) - how far its songs may go, a sentence (sources-plugins-reach-*)
#   $page (String) - its page, a sentence (sources-plugins-page or sources-plugins-no-page)
#   $sha (String) - its program's SHA-256, 64 characters (data)
sources-plugins-description = { $state } Version { $version }, from { $publisher }, as the plugin says. What it does: { $can }. { $reach } { $page } SHA-256 of its program: { $sha }

# What a plugin does, as a list joined by " · " in
# sources-plugins-description: search; filters for a search; browse its
# collection; describe a song before fetching it; fetch songs; take a
# folder the player chooses, a web address pasted, links to its own site,
# or text typed.
sources-plugins-can-search = searching
sources-plugins-can-filters = search filters
sources-plugins-can-browse = browsing
sources-plugins-can-describe = song details
sources-plugins-can-fetch = fetching songs
sources-plugins-can-folder = songs from a folder you choose
sources-plugins-can-url = songs from a web address you paste
sources-plugins-can-text = songs from text you type
# Variables:
#   $hosts (String) - the sites and paths it claims, ", " between them (data)
sources-plugins-can-link = songs from links to { $hosts }

# How far a plugin's songs may go, as its plugin.toml says (its songs may
# lower it for themselves): played on this computer only; played and
# added to online rooms; played, added to rooms and imported.
sources-plugins-reach-play = Its songs can be played here only: not added to rooms or imported.
sources-plugins-reach-rooms = Its songs can be played here and added to online rooms, but not imported.
sources-plugins-reach-keep = Its songs can be played here, added to online rooms and imported into your library.

# A plugin's page, a sentence of its description, or that it names none.
# Variables:
#   $page (String) - the page's address (data)
sources-plugins-page = Its page: { $page }.
sources-plugins-no-page = It names no page of its own.

# A row under a plugin with a page: its button opening the page in the
# system's browser (about 16 letters), and the row's sentence.
sources-plugins-open-page = Open its page
# Variables:
#   $page (String) - the page's address (data)
sources-plugins-page-description = Opens { $page } in your web browser: the page its plugin.toml names.

# A folder in the plugins folder that is not a plugin the game can take,
# beside its name (about 40 letters, cut where longer): no plugin.toml;
# a plugin.toml that does not read; a folder not named as its plugin.toml
# says; no program for this computer.
sources-plugins-refused-no-manifest = Not taken: no plugin.toml
sources-plugins-refused-manifest = Not taken: its plugin.toml does not read
sources-plugins-refused-folder = Not taken: its folder has another name
sources-plugins-refused-program = Not taken: no program for this computer

# The same, as the row's sentence in the description panel.
sources-plugins-refused-no-manifest-description = This folder holds no plugin.toml the game can read, so it is not a plugin.
# Variables:
#   $why (String) - what the reader found wrong, in its own words (data)
sources-plugins-refused-manifest-description = Its plugin.toml does not read, so the game does not take it: { $why }
sources-plugins-refused-folder-description = A plugin's folder must have the name its plugin.toml gives it, so the game does not take it.
# Variables:
#   $platform (String) - this computer's kind, as plugins name it (data: aarch64-macos)
#   $path (String) - where its program was looked for (data)
sources-plugins-refused-program-description = It has no program for this computer ({ $platform }) where its plugin.toml says: { $path }

# The question when a plugin is switched on, asked once each time: its
# title, its text, and its two buttons (Keep it off chosen first).
# Variables:
#   $name (String) - the plugin's own name (data)
sources-plugins-ask-title = Switch on { $name }?
# Variables:
#   $name (String) - the plugin's own name (data)
#   $publisher (String) - its publisher as it names itself (data)
sources-plugins-ask-body = { $name }, from { $publisher } as it says, is a program. Switched on, it runs on this computer whenever Find songs asks it for songs, with your access to your files and the network, like any program you run. The game cannot check what it does: switch it on only if you trust where it came from.
sources-plugins-ask-yes = Switch on
sources-plugins-ask-no = Keep it off

# Options' notice once a switch changed, or the folder was read again.
# Variables:
#   $name (String) - the plugin's own name (data)
sources-plugins-notice-on = { $name } is on. Its songs are on Find songs.
# Variables:
#   $name (String) - the plugin's own name (data)
sources-plugins-notice-off = { $name } is off and not running.
sources-plugins-notice-this-computer-on = This computer is on. Find songs searches the folders you choose there.
sources-plugins-notice-this-computer-off = This computer is off.
# Variables:
#   $count (Number) - the plugins found
sources-plugins-notice-looked = { $count ->
    [one] The plugins folder holds one plugin.
   *[other] The plugins folder holds { $count } plugins.
}
sources-plugins-notice-looked-none = The plugins folder holds no plugin.
# Variables:
#   $error (String) - why, as the system said it (data)
sources-plugins-notice-not-opened = The plugins folder could not be opened: { $error }
