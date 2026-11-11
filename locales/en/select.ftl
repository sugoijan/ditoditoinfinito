## Song select: the song list (the wheel) at the right, the song shown at
## the left with its styles and charts, the search and the footer's key
## hints, and its dialogs (adding songs, managing them, sharing them, a
## chart's kept plays, the notices in full).

## The wheel: a row a song, under a header a pack.

# The pack the songs that come with the game are listed under: its
# header in the list and the song's pack line. A header row's title,
# about 40 letters.
select-pack-bundled = Bundled songs

# A pack's header, at its right: how many songs it holds. Short, about
# 12 letters.
# Variables:
#   $count (Number) - the pack's songs
select-pack-songs = { $count ->
    [one] { $count } song
   *[other] { $count } songs
}

# A pack's header while searching: how many of its songs match. Short,
# about 16 letters.
# Variables:
#   $found (Number) - its songs the search matches
#   $count (Number) - the pack's songs
select-pack-songs-found = { $count ->
    [one] { $found } of { $count } song
   *[other] { $found } of { $count } songs
}

# A song's row, in place of its levels, when it has no chart in the style
# chosen (the row is greyed). Short, about 24 letters.
# Variables:
#   $style (String) - the style's name, as its tab shows it ("Single")
select-row-no-charts = no { $style } charts

## The header: the screen's name, the search, what the play is set to.

# The screen's name in the header, small capitals after the game's
# wordmark (written in capitals here; a language without them writes it
# as it is written).
select-title = SELECT MUSIC

# The screen's name when it lists the songs of a link someone shared.
select-title-shared = SHARED SONGS

# The search field's placeholder, with the key that starts typing (/).
# About 20 letters.
select-search-placeholder = Search  /

# Before the options in effect (pills: "×2.5", "Reverse"), in the header
# left of the search; pressing them opens the options. Short: the search
# gets narrower the longer this is.
select-playing-with = Playing with

## The song shown, at the left.

# The song's own offset (Options' per-song offset), when it has one, at
# the right of its line. Short.
# Variables:
#   $offset (String) - the offset in milliseconds with its sign ("+12", "-8")
select-info-offset = offset { $offset } ms

# The compact layout's line under the song's title: its artist, tempo and
# length. One line about 300 px wide, cut with … where longer.
# Variables:
#   $artist (String) - the song's artist, as its file names it
#   $bpm (String) - its tempo ("120", "90–180"); BPM is never translated
#   $length (String) - its length ("2:05")
select-info-line = { $artist } · { $bpm } BPM · { $length }

# A small label in capitals before the song's length ("2:05"), with
# about 50 px to itself (BPM before it is never translated).
select-info-length = LENGTH

# A small label in capitals before the song's pack's name, with about
# 38 px to itself.
select-info-pack = PACK

## The styles (Single, Double…) and the charts of the song shown.

# The compact layout's style button (a phone's): the style shown, how
# many charts the song has in it, and in how many other styles it has
# charts; ▾ says pressing it changes the style. One line about 300 px
# wide (about 38 letters).
# Variables:
#   $style (String) - the style's name, as its tab shows it ("Single")
#   $charts (Number) - the song's charts in this style
#   $others (Number) - how many other styles the song has charts in
select-style-button = { $charts ->
    [0] { $others ->
        [0] { $style }  no charts  ▾
        [one] { $style }  no charts · in { $others } other style  ▾
       *[other] { $style }  no charts · in { $others } other styles  ▾
    }
    [one] { $others ->
        [0] { $style }  { $charts } chart  ▾
        [one] { $style }  { $charts } chart · also in { $others } other style  ▾
       *[other] { $style }  { $charts } chart · also in { $others } other styles  ▾
    }
   *[other] { $others ->
        [0] { $style }  { $charts } charts  ▾
        [one] { $style }  { $charts } charts · also in { $others } other style  ▾
       *[other] { $style }  { $charts } charts · also in { $others } other styles  ▾
    }
}

# The compact layout's large Play button, before a chart is shown.
select-play-button = PLAY

# The compact layout's large Play button: what it plays. In capitals as
# a button's word; the chart is its difficulty or name and level, as the
# chart says them (never translated). A button about 300 px wide.
# Variables:
#   $chart (String) - the chart ("HARD 13", "MY EDIT")
select-play-chart = PLAY { $chart }

# The same while autoplay is on (the play is autoplayed).
# Variables:
#   $chart (String) - the chart ("HARD 13", "MY EDIT")
select-autoplay-chart = AUTOPLAY { $chart }

# The compact layout's Play button when the song has no chart in the
# style chosen.
select-play-no-charts = NO CHARTS

# After the Play button's words (" · ASSIST") when the chart is a version
# made with notes left out: the mark the results give such a play. As
# the results write it (never translated, as REPLAY and AUTOPLAY).
select-assist-mark = ASSIST

# Where the chart ladder would be, when the song has no chart in the
# style chosen. A heading over the hint below, about 60 letters.
# Variables:
#   $style (String) - the style's name, as its tab shows it ("Single")
select-no-charts = No { $style } charts for this song.

# Under that heading: the style tabs above are lit where the song has
# charts; the Tab key goes to the next one. About 70 letters.
select-no-charts-hint = The lit styles above have charts; Tab moves to the next one.

# A chart's row in the ladder, after its difficulty: how many notes it
# has. Short, about 30 letters.
# Variables:
#   $notes (Number) - its notes
select-chart-notes = { $notes ->
    [one] { $notes } note
   *[other] { $notes } notes
}

# The same with the chart's step artist (who made it), as the chart
# names them. About 40 letters, cut with … where longer.
# Variables:
#   $notes (Number) - its notes
#   $credit (String) - its step artist, as the chart names them
select-chart-notes-by = { $notes ->
    [one] { $notes } note · steps by { $credit }
   *[other] { $notes } notes · steps by { $credit }
}

# A chart version the game makes from another (Options → Notes →
# Simplify and Convert), marked before its notes in the ladder and after
# its chart in the plays' title. A short tag, about 20 letters each.
# Without the Dancing☆Onigiri rice-ball lane's notes.
select-version-without-onigiri = Without onigiri

# Without a Pump chart's middle panel's notes.
select-version-without-center = Without center

# Made from a Pump single chart (onto Dancing☆Onigiri's five keys).
select-version-from-pump-single = From Pump single

# Made from a Dancing☆Onigiri five-key chart (onto Pump single).
select-version-from-five-keys = From 5 keys

# A named chart that is a version the game made, on its chart button and
# the large Play button: its name with the version's mark, as it would
# otherwise read as the chart it was made from. The name keeps the case
# the button writes it in.
# Variables:
#   $chart (String) - the chart's name and level, as the chart says them ("MY EDIT 12")
#   $version (String) - the version, as the ladder marks it ("Without onigiri")
select-chart-version = { $chart } · { $version }

## The footer: key hints at the left (the keys' caps, then these words),
## links at the right. Each hint's words are short (about 10 letters).

# ↑ ↓: move through the songs.
select-hint-song = Song

# ↑ ↓ pressed twice (or a controller's shoulders): change the chart.
select-hint-chart = Chart

# Tab: the next style the song has charts in.
select-hint-style = Style

# Enter: play the chart.
select-hint-play = Play

# /: search.
select-hint-search = Search

# Escape: back to the start menu (the game's title screen).
select-hint-start-menu = Start menu

# Enter on a pack's header: opens or closes the pack.
select-hint-open-close = Open or close

# ↑ ↓ while typing a search: choose among the suggestions.
select-hint-choose = Choose

# Enter while typing a search: go to the songs found.
select-hint-go-to = Go to

# Delete: manage (remove, read again) the imported song or pack.
select-hint-manage = Manage

# P: the chart's kept plays (and their replays).
select-hint-plays = Plays

# E: open the chart in the chart editor.
select-hint-edit = Edit

# A controller's directions while on the header's row: move along it.
select-hint-navigate = Navigate

# A controller's confirm button: press what is chosen.
select-hint-activate = Activate

# The footer's note while typing a search: what the search looks in.
# One line after the hints, about 90 letters.
select-typing-note = Typing searches titles, artists, packs and step artists; kana and romaji match each other.

# The footer's note while a search is in effect: how many songs it finds.
# Variables:
#   $count (Number) - the songs matching
select-matches = { $count ->
    [0] No song matches.
    [one] { $count } song matches.
   *[other] { $count } songs match.
}

# The footer's note on the songs of a shared link: whose pack.
# Variables:
#   $pack (String) - the shared pack's name
select-shared-note = Shared: { $pack }

# The same with the time the link has left.
# Variables:
#   $pack (String) - the shared pack's name
#   $expires (String) - when the link expires, as the server's answer says it ("expires in 3 days")
select-shared-note-expires = Shared: { $pack } · { $expires }

# The footer's note: how many songs were added to the next share (More →
# Add to share).
# Variables:
#   $count (Number) - the songs added
select-share-list-note = { $count ->
    [one] { $count } song to share
   *[other] { $count } songs to share
}

# A footer link: manage (remove, read again) the imported song or pack
# shown. Short, about 10 letters.
select-link-manage = Manage…

# A footer link: opens the dialog that adds songs. Short, about 12
# letters.
select-link-add = + Add songs

# A footer link: opens the options. Short, about 10 letters.
select-link-options = Options

# Shown in the footer for a few seconds after a controller pressed a
# link: a browser opens links only from a key or a tap.
select-gesture-link = Press Enter or tap to open the link.

# Shown in the footer for a few seconds after a controller pressed the
# search field: a browser starts typing only from a key or a tap.
select-gesture-search = Press / or tap the field to type.

## The More menu (song actions): a list of buttons about 300 px wide.

# The menu's title.
select-more-title = Song actions

# Starts typing a search.
select-more-search = Search

# Opens the suggestions for what was typed in the search.
select-more-suggestions = Search suggestions…

# Empties the search: every song shows again.
select-more-clear-search = Clear search

# The chart's kept plays (and their replays).
select-more-history = Chart history

# Watches the replay of the best play of the chart.
select-more-watch-best = Watch best

# Opens the chart in the chart editor.
select-more-edit = Edit chart

# Manages (removes, reads again) the imported song or pack.
select-more-manage = Manage song / pack

# Takes the song shown off the list of songs to share together.
select-more-share-remove = Remove from share

# Adds the song shown to a list of songs to share together.
select-more-share-add = Add to share

# Shares the songs added to the list, by a link.
# Variables:
#   $count (Number) - the songs on the list
select-more-share-list = { $count ->
    [one] Share { $count } song…
   *[other] Share { $count } songs…
}

# Empties the list of songs to share.
select-more-share-clear = Clear the share list

# Shares the song shown, by a link.
select-more-share-song = Share this song…

# Shares the pack whose header is chosen, by a link.
select-more-share-pack = Share the pack…

# Shows the chart before the one shown (an easier one).
select-more-previous-chart = Previous chart

# Shows the chart after the one shown (a harder one).
select-more-next-chart = Next chart

# Shows the song's charts in a style.
# Variables:
#   $style (String) - the style's name, as its tab shows it ("Single")
select-more-style = Style: { $style }

# Opens the dialog that adds songs.
select-more-add-songs = Add songs

# Opens the options.
select-more-options = Options

# The title of the menu of suggestions for what was typed in the search.
select-suggestions-title = Search suggestions

# A suggestion's kind, a small tag after it in capitals: a song's title,
# an artist, a pack. A word, about 8 letters.
select-suggest-title = TITLE
select-suggest-artist = ARTIST
select-suggest-pack = PACK

## Sharing songs by a link (shown only where sharing is switched on).

# The name a share of songs from several packs goes by (its viewers see
# it as the pack's name).
select-share-default-pack = Shared songs

# How long the link lives: a button each, about 160 px wide.
select-share-expiry-hour = 1 hour
select-share-expiry-day = 1 day
select-share-expiry-week = 1 week
select-share-expiry-month = 1 month

# The link never expires.
select-share-expiry-never = Forever

# Puts the share's link on the clipboard: a button about 160 px wide.
select-share-copy-link = Copy link

# The dialog's title: what is shared, a song or a pack, by its name.
# Variables:
#   $title (String) - the song's title or the pack's name
select-share-title = Share “{ $title }”

# The dialog's title when the songs added to the share list are shared.
# Variables:
#   $count (Number) - the songs on the list
select-share-title-list = { $count ->
    [one] Share { $count } song
   *[other] Share { $count } songs
}

# Under the title, before choosing how long the link lives. Wrapped.
# Variables:
#   $count (Number) - the songs shared
#   $pack (String) - the name they are shared under (a pack's)
select-share-intro = { $count ->
    [one] Uploads a copy of { $count } song of “{ $pack }” for anyone with the link to play, until the link expires.
   *[other] Uploads a copy of { $count } songs of “{ $pack }” for anyone with the link to play, until the link expires.
}

# The dialog's title while the songs go up.
select-share-uploading = Uploading…

# How the upload goes, under that title.
# Variables:
#   $done (Number) - the songs uploaded
#   $total (Number) - the songs to upload
#   $sent (String) - the size uploaded, as the common-size messages write it ("4.2 MB")
#   $size (String) - the size to upload, the same way ("37 MB")
select-share-progress = { $done } of { $total } songs, { $sent } of { $size }.

# The dialog's title once the songs are up.
select-share-done = Shared

# Under it, before the link (on the next line).
select-share-done-note = Anyone with this link can play the songs:

# The dialog's title when the upload failed (the reason under it).
select-share-failed = The share failed

## Adding songs (the import dialog).

# The dialog's title, in capitals as the screens' names are.
select-import-title = ADD SONGS

# Desktop's two tabs: songs from folders and ZIP files, or a
# Dancing☆Onigiri work from its web address. Buttons about 340 px wide.
select-import-tab-files = Folders / ZIPs
select-import-tab-danoni = DanOni URL

# What can be added and how, at the top of the dialog. Wrapped, about
# three lines. On the web the songs are copied into the browser's
# storage; on desktop they are read where they are.
# Variables:
#   $platform (String) - where the game runs: web or desktop
select-import-intro = { $platform ->
    [web] StepMania packs (.sm, .ssc, .dwi), Dancing☆Onigiri works and FFR levels (.swf): a folder, a zip or files, or drop them on this page. They stay in this browser only.
   *[desktop] Choose a song folder, a Songs folder or a pack's ZIP. StepMania packs (.sm, .ssc, .dwi), Dancing☆Onigiri works and FFR levels (.swf) are supported. Folders and ZIPs are read where they are; Options lists them.
}

# The same on the web where a browser keeps a chosen folder to read it
# again (its music and backgrounds stay in the folder). Wrapped.
select-import-intro-linked = StepMania packs (.sm, .ssc, .dwi), Dancing☆Onigiri works and FFR levels (.swf): a folder, a zip or files, or drop them on this page. A chosen folder's music and backgrounds are read from the folder, the rest is stored in this browser only.

# The same, shorter, on a small screen (a phone sideways). Wrapped, two
# lines.
# Variables:
#   $platform (String) - where the game runs: web or desktop
select-import-intro-compact = { $platform ->
    [web] Choose a folder, ZIP or files, or drop them here. Files stay in this browser.
   *[desktop] Choose a song folder or a pack's ZIP. Folders and ZIPs are read where they are.
}

# The short one where a browser keeps a chosen folder (web only).
select-import-intro-compact-linked = Choose a folder or ZIP. Linked folders are read in place; other files stay in this browser.

# Opens the file picker for a folder: a button about 260 px wide.
select-import-choose-folder = Choose folder

# Opens the file picker for ZIP files (and on the web, other files): a
# button about 260 px wide.
# Variables:
#   $platform (String) - where the game runs: web or desktop
select-import-choose-files = { $platform ->
    [web] Choose zip or files
   *[desktop] Choose zip
}

# Makes a new song from a music file and opens the chart editor on it: a
# button about 530 px wide.
select-import-new-song = New song from a music file

# The web's import options, a switch each (a row about 700 px wide).
# Keeps a chosen folder's songs in the browser's storage rather than
# reading them from the folder.
select-import-option-copy = Store a chosen folder's songs in this browser

# Keeps the songs' background videos too (they are large).
select-import-option-videos = Also store background videos

# The import's report lists details only pack authors need.
select-import-option-details = Details for pack authors

# Desktop's URL tab: what to do. One line.
select-import-danoni-intro = Paste a Dancing☆Onigiri work's URL.

# Starts the import from the address typed: a button 104 px wide (about
# 10 letters).
select-import-danoni-import = Import

# The same after an import failed.
select-import-danoni-retry = Retry

# Under the address field, small. One line.
select-import-danoni-note = Files are downloaded and converted for your library.

# The address field's placeholder: a web address's start. Kept as is.
select-import-url-placeholder = https://…

# When an import asks to read files from another web address: the
# heading over the address and what it is for.
select-import-origin-heading = Allow files from this address?

# Under it, what the three buttons below do. Wrapped.
select-import-origin-explain = Allow once permits this import. Remember also permits future imports from this address. Decline stops this request.

# The three buttons, about 230 px wide each.
select-import-origin-once = Allow once
select-import-origin-remember = Remember
select-import-origin-decline = Decline

# The status line while the import waits for those buttons.
select-import-decision-needed = Your decision is needed to continue.

# The status line during a URL import: the step the importer reports,
# then how far it is.
# Variables:
#   $stage (String) - the importer's step, as danoni-stage-* say it
#   $done (Number) - the files done
#   $total (Number) - the files to do
select-import-stage-progress = { $stage } · { $done } of { $total }

# The status line when the address typed is not a web address.
select-import-url-invalid = Enter a URL beginning with https:// or http://.

# The status line (web only) when a controller pressed a file button: a
# browser opens its file picker only from a key, a click or a tap.
# Wrapped, two lines.
select-import-needs-gesture = Press a keyboard key, click or tap to open the browser's file picker; or drop files here. Back returns to the song list.

# Stops a running URL import: a button 140 px wide (about 14 letters).
select-import-cancel = Cancel import

## Managing an imported song or pack (Delete, or More → Manage). On the
## web the songs are removed from the browser; on desktop they are hidden
## from the list (their folders are only read, never changed). Buttons
## about 160 px wide; titles one line; texts wrapped.

# Variables:
#   $platform (String) - where the game runs: web or desktop
select-manage-remove-song = { $platform ->
    [web] Remove song
   *[desktop] Hide song
}

# Reads the pack's folder again (new and changed songs come in).
select-manage-reread = Read folder again

# Variables:
#   $platform (String) - where the game runs: web or desktop
select-manage-remove-pack = { $platform ->
    [web] Remove pack
   *[desktop] Hide pack
}

# The confirmation's button that removes (or hides).
# Variables:
#   $platform (String) - where the game runs: web or desktop
select-manage-confirm = { $platform ->
    [web] Remove
   *[desktop] Hide
}

# The confirmation's button that keeps what was going to be removed.
select-manage-keep = Keep

# The title over the songs the game could not read (a notice's link).
select-manage-unreadable-title = Songs that could not be read

# While an import runs, nothing can be removed or read again.
select-manage-busy = An import is running: removing and reading folders again wait until it ends.

# Under a pack's name: what it is.
# Variables:
#   $platform (String) - where the game runs: web or desktop
#   $count (Number) - the pack's songs
select-manage-pack = { $platform ->
    [web] { $count ->
        [one] { $count } song imported into this browser.
       *[other] { $count } songs imported into this browser.
    }
   *[desktop] { $count ->
        [one] { $count } song, read from a song folder in place.
       *[other] { $count } songs, read from a song folder in place.
    }
}

# The same where the browser keeps the pack's folder, to read it again.
# Variables:
#   $platform (String) - where the game runs: web or desktop
#   $count (Number) - the pack's songs
select-manage-pack-rereadable = { $platform ->
    [web] { $count ->
        [one] { $count } song imported into this browser, from a folder it can read again.
       *[other] { $count } songs imported into this browser, from a folder it can read again.
    }
   *[desktop] { $count ->
        [one] { $count } song, read from a song folder in place.
       *[other] { $count } songs, read from a song folder in place.
    }
}

# Under a song's title: its pack.
# Variables:
#   $pack (String) - the pack's name
select-manage-song-pack = In the pack “{ $pack }”.

# The confirmation's title for a pack.
# Variables:
#   $platform (String) - where the game runs: web or desktop
#   $pack (String) - the pack's name
select-manage-confirm-pack-title = { $platform ->
    [web] Remove “{ $pack }”?
   *[desktop] Hide “{ $pack }”?
}

# Under it: what removing (hiding) the pack does.
# Variables:
#   $platform (String) - where the game runs: web or desktop
#   $count (Number) - the pack's songs
select-manage-confirm-pack = { $platform ->
    [web] { $count ->
        [one] Its { $count } song leaves this browser; its plays stay in the history.
       *[other] Its { $count } songs leave this browser; their plays stay in the history.
    }
   *[desktop] { $count ->
        [one] Its { $count } song leaves the list; nothing in the folder is deleted, and reading it again brings it back. Its plays stay in the history.
       *[other] Its { $count } songs leave the list; nothing in the folder is deleted, and reading it again brings them back. Their plays stay in the history.
    }
}

# The confirmation's title for a song.
# Variables:
#   $platform (String) - where the game runs: web or desktop
#   $title (String) - the song's title
select-manage-confirm-song-title = { $platform ->
    [web] Remove “{ $title }”?
   *[desktop] Hide “{ $title }”?
}

# Under it: what removing (hiding) the song does.
# Variables:
#   $platform (String) - where the game runs: web or desktop
select-manage-confirm-song = { $platform ->
    [web] It leaves this browser; its plays stay in the history.
   *[desktop] It leaves the list; nothing in the folder is deleted, and reading the folder again brings it back. Its plays stay in the history.
}

# The confirmation's title for the songs that could not be read.
# Variables:
#   $platform (String) - where the game runs: web or desktop
#   $count (Number) - the songs
select-manage-confirm-unreadable-title = { $platform ->
    [web] { $count ->
        [one] Remove { $count } stored song that could not be read?
       *[other] Remove { $count } stored songs that could not be read?
    }
   *[desktop] { $count ->
        [one] Forget { $count } song that could not be read?
       *[other] Forget { $count } songs that could not be read?
    }
}

# Under it: what removing (forgetting) them does.
# Variables:
#   $platform (String) - where the game runs: web or desktop
select-manage-confirm-unreadable = { $platform ->
    [web] Their records no longer read; their files go too. Their plays stay in the history.
   *[desktop] They leave the list; nothing on disk is deleted.
}

## A chart's kept plays (P, or More → Chart history): a page of rows,
## newest first; buttons sized to their words.

# The dialog's title: the chart whose plays these are.
# Variables:
#   $chart (String) - the chart, its difficulty (never translated) or name and level ("Hard 9")
select-plays-title = PLAYS · { $chart }

# The same for a chart version the game made (Options → Notes →
# Simplify and Convert).
# Variables:
#   $chart (String) - the chart, its difficulty (never translated) or name and level ("Hard 9")
#   $version (String) - the version, as the ladder marks it ("Without center")
select-plays-title-version = PLAYS · { $chart } · { $version }

# A row's mark for a play the game autoplayed: as the results mark it
# (never translated, as REPLAY and ASSIST).
select-plays-autoplay = AUTOPLAY

# A row's mark for a play judged again under rules other than those it
# was played by (the rules named before it are those it was judged by).
select-plays-rejudged = judged again

# A row's mark for the best play of the chart.
select-plays-best = ★ best

# Watches the chosen play's replay.
select-plays-watch = Watch

# Shows the chosen play's results screen.
select-plays-results = Results

# Forgets the chosen play (pressed twice: the second time it reads as
# below).
select-plays-remove = Remove

# The same button once pressed: pressing it again removes the play.
select-plays-remove-confirm = Sure? Remove

# In place of the rows when the chart has no kept play.
select-plays-empty = No play of this chart is kept yet.

# A page of newer rows (the rows are newest first).
select-page-newer = ‹ Newer

# A page of older rows.
select-page-older = Older ›

# Which rows the page shows, at the dialog's top right.
# Variables:
#   $first (Number) - the page's first row
#   $last (Number) - the page's last row
#   $count (Number) - all the rows
select-page-range = { $first }–{ $last } of { $count }

## The notices in full (a press on the notices' line).

# The dialog's title, in capitals as the screens' names are.
select-notices-title = NOTICES
