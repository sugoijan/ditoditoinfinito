## What went wrong outside the game (the system, the browser, the
## network): its kind in a few words, then what the system said itself,
## in parentheses and untranslated (it is the system's, often English).
## They follow a message's colon ("Could not export the plays: not found
## (…)"), so they begin in lower case where the language has case.
## Variables, in each:
##   $detail - what the system said

# A file, a folder, a record or a page is not there.
# Variables:
#   $detail (String) - what the system said
errors-not-found = not found ({ $detail })

# The system or the browser refused access.
# Variables:
#   $detail (String) - what the system said
errors-denied = access was refused ({ $detail })

# Out of storage: the disk is full, or the browser's storage for the site.
# Variables:
#   $detail (String) - what the system said
errors-full = there is no storage left ({ $detail })

# The network failed or the server could not be reached.
# Variables:
#   $detail (String) - what the system said
errors-network = the network failed ({ $detail })

# It took too long and was given up.
# Variables:
#   $detail (String) - what the system said
errors-timeout = it took too long ({ $detail })

# The data is damaged or not what it should be.
# Variables:
#   $detail (String) - what the system said
errors-unreadable = the data could not be read ({ $detail })

# This browser or system cannot do it.
# Variables:
#   $detail (String) - what the system said
errors-unsupported = this is not supported here ({ $detail })

## Why a song cannot be played, after "The song failed to load:"
## (prompt-song-failed): a clause, so beginning in lower case where the
## language has case.

# The library has no song of this id (it was removed, or the address
# names none).
errors-load-no-song = the library has no such song

# One of the song's files could not be read.
# Variables:
#   $file (String) - the file, as the song names it ("music.ogg")
#   $error (String) - why: its kind and what the system said (errors-not-found…)
errors-load-file = { $file } could not be read: { $error }

# Web: a file of a song imported into the browser is no longer stored.
# Variables:
#   $file (String) - the file, as the song names it
errors-load-missing = { $file } is missing from the stored song; import it again

# Web: the song plays from a folder of the player's computer, and the
# browser does not know which.
errors-load-folder-unknown = the folder this song plays from is unknown; import it again

# Web: the song plays from a folder of the player's computer that the
# browser no longer keeps.
errors-load-folder-gone = the folder this song plays from is no longer kept; import it again

# Web: a file the song plays is no longer in its folder.
# Variables:
#   $file (String) - the file, its path in the folder
#   $folder (String) - the folder's name
errors-load-not-in-folder = { $file } is no longer in the folder “{ $folder }”; re-read the folder or import the song again

# The song's chart file could not be read.
# Variables:
#   $file (String) - the chart's file ("song.ssc")
#   $error (String) - what the chart's reader said (technical, in English)
errors-load-chart = its chart { $file } could not be read: { $error }

# The song's music could not be decoded.
# Variables:
#   $error (String) - what the decoder said (technical, often in English)
errors-load-music = its music could not be decoded: { $error }

# Web: a Dancing☆Onigiri work's song whose work is no longer stored.
errors-load-work-gone = the work is no longer stored; import it again

# A Dancing☆Onigiri work's song whose work could not be read.
# Variables:
#   $error (String) - what was wrong (technical, in English)
errors-load-work = its Dancing☆Onigiri work could not be read: { $error }

# A song shared by link whose share could not be read.
# Variables:
#   $error (String) - why (errors-share-…)
errors-load-share = its share could not be read: { $error }

# The sound could not start (the browser's or the system's audio).
# Variables:
#   $error (String) - why: its kind and what the system said
errors-load-audio = the sound could not start: { $error }

## Why a background video cannot play, in the results' note
## (results-note-video-trouble) and the prompt's line on the web
## (prompt-video-failed): a clause, beginning in lower case.

# This build of the game has no video decoder.
errors-video-not-included = the video decoder is not included in this build

# Variables:
#   $error (String) - why: its kind and what the system said
errors-video-not-loaded = the video decoder could not be loaded: { $error }

# The decoder (a program of its own) stopped.
# Variables:
#   $detail (String) - what was known of it (technical, in English)
errors-video-crashed = the video decoder crashed ({ $detail })

errors-video-unknown-format = it is not a movie the decoder knows

# Variables:
#   $count (Number) - broken frames in a row
errors-video-broken = { $count ->
    [one] the movie does not decode ({ $count } broken frame in a row)
   *[other] the movie does not decode ({ $count } broken frames in a row)
}

errors-video-unseekable = the movie could not be read again after seeking

# The movie's frames are larger than the graphics card's textures.
# Variables:
#   $width (String) - the frames' width in pixels ("3840")
#   $height (String) - their height in pixels ("2160")
#   $limit (String) - the largest side the card takes, in pixels ("2048")
errors-video-too-large = its { $width }×{ $height } frames exceed the GPU's { $limit } px limit

# A frame came that the game cannot draw.
errors-video-bad-frame = a malformed frame

## What went wrong with songs shared by link, after a colon
## (errors-share-not-loaded, errors-share-not-shared, the shares dialog's
## "The shares could not be read:"): a clause, beginning in lower case.

# No share has this link, or it expired.
errors-share-gone = the link doesn't exist or has expired

# Uploading or listing shares needs the token whoever runs the sharing
# server gave; Options has a Sharing section for it.
errors-share-no-token = set the upload token under Options → Sharing first

errors-share-nothing = there is nothing to share

errors-share-token-refused = the upload token was refused

# The server's limits.
errors-share-too-large = too large to share (2 GB a share, 512 MB a file, 200 songs)

# Variables:
#   $status (String) - the HTTP status code ("500")
errors-share-status = the server answered HTTP { $status }

# Variables:
#   $status (String) - the HTTP status code ("500")
#   $body (String) - what the server said (technical, in English)
errors-share-status-body = the server answered HTTP { $status }: { $body }

# Variables:
#   $detail (String) - what the reader said (technical, in English)
errors-share-answer = the server's answer could not be read ({ $detail })

# Variables:
#   $song (String) - the song's title
errors-share-no-chart = the chart of “{ $song }” could not be read

# A song, or a file of one, failed.
# Variables:
#   $name (String) - the song's title or the file's path
#   $error (String) - what went wrong (one of these)
errors-share-in = { $name }: { $error }

## The same as sentences on their own.

# The page of a shared link that is gone (web); a notice over the shared
# songs (desktop).
errors-share-link-gone = This link doesn't exist or has expired.

# The same page or notice when the share could not be read otherwise.
# Variables:
#   $error (String) - why (errors-share-…, or a failure's kind)
errors-share-not-loaded = The shared songs could not be loaded: { $error }.

# Under the share dialog's title ("The share failed").
# Variables:
#   $error (String) - why (errors-share-…, or a failure's kind)
errors-share-not-shared = The songs could not be shared: { $error }.

## The page of a shared link that is gone (web, #/s/…).

# The page's title beside its back icon on a phone (about 20 letters).
errors-shared-page-title = Shared songs

# Under the page's line, muted.
errors-shared-page-hint = A shared link plays its songs from where they were shared, for as long as whoever shared them chose. Ask them for a new one.

# Its button back to the start menu (about 20 letters).
errors-shared-page-home = Start menu

## Why a line of a play history file was left out of an import, after
## "line 3:" (options-import-problem): a clause, beginning in lower case.

# Variables:
#   $mb (Number) - the longest line read, in megabytes
errors-history-too-long = longer than { $mb } MB

errors-history-not-utf8 = not UTF-8 text

# Variables:
#   $detail (String) - what the reader said (technical, in English)
errors-history-not-a-play = not a play: { $detail }

errors-history-not-a-play-id = its id is not a play's

errors-history-record-not-json = its record is not JSON

errors-history-record-no-version = its record has no version

errors-history-record-version-zero = its record's version is 0

# Variables:
#   $detail (String) - what the reader said (technical, in English)
errors-history-record-unreadable = its record does not read: { $detail }

errors-history-record-out-of-range = its record's time or offset is out of range

errors-history-record-no-song = its record names no song

errors-history-log-out-of-range = its input log's times are out of range

# Variables:
#   $version (String) - the record's format, a whole number ("4")
errors-history-newer-record = kept by a newer version of the game (record format { $version })

# Variables:
#   $version (String) - the judge's version, a whole number ("3")
errors-history-newer-judge = judged by a newer version of the game (judge { $version })

# Variables:
#   $version (String) - the input log's format, a whole number ("2")
errors-history-newer-log = its input log is of a newer format ({ $version })

# Variables:
#   $version (String) - the timing's format, a whole number ("2")
errors-history-newer-timing = its timing is of a newer format ({ $version })

# Variables:
#   $detail (String) - what the reader said (technical, in English)
errors-history-log-unreadable = its input log does not read: { $detail }

# Variables:
#   $detail (String) - what the reader said (technical, in English)
errors-history-timing-unreadable = its timing does not read: { $detail }
