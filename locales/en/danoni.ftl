## Desktop's Dancing☆Onigiri URL import (Add songs → the DanOni URL tab):
## what the importer is doing while it runs and how it ended. "DanOni" is
## Dancing☆Onigiri's short name, kept as it is. The importer is the
## program that comes with the game, downloads the work at the address
## and converts it into songs for the library; its catalog is its list of
## the songs it imported.

## The status line while an import runs: under the address field, wrapped
## at about 700 px. When the step's count is known, " · 2 of 5" follows it
## (select-import-stage-progress). Each ends in "…": it is under way.

# Begun, before the importer has said what it does.
danoni-stage-preparing = Preparing DanOni import…

# The importer reading its catalog of the songs it imported before.
danoni-stage-opening-catalog = Opening DanOni catalog…

# Opening the work at the address, before anything of it is known.
danoni-stage-opening-work = Opening the work…

# Downloading the work's page and files.
danoni-stage-downloading = Downloading the work…

# Reading the charts out of the work's files.
danoni-stage-reading-charts = Reading chart data…

# Checking the charts read out of the work.
danoni-stage-checking-charts = Checking extracted charts…

# Converting the songs' music for the game.
danoni-stage-converting-music = Converting music…

# Working out the charts' timing (their tempo changes) for the game.
danoni-stage-mapping-timing = Mapping chart timing…

# Making the songs' pictures (banners, backgrounds).
danoni-stage-making-artwork = Making song artwork…

# Making the songs for the game from what was read.
danoni-stage-preparing-songs = Preparing songs…

# The last of making the songs.
danoni-stage-finishing-preparation = Finishing preparation…

# Saving the songs made.
danoni-stage-saving-prepared = Saving prepared songs…

# Checking the records of what was downloaded and made.
danoni-stage-checking-import = Checking the import…

# Checking the songs' files.
danoni-stage-checking-files = Checking song files…

# Checking the converted charts against the work's own.
danoni-stage-verifying-charts = Verifying converted charts…

# Adding the songs' details (titles, levels) for the song list.
danoni-stage-adding-details = Adding song details…

# The last of the checks.
danoni-stage-finishing-checks = Finishing checks…

# Checking the songs made, before the importer says which check.
danoni-stage-checking-prepared = Checking prepared songs…

# Comparing with the songs imported from this work before.
danoni-stage-checking-existing = Checking existing imports…

# Beginning to change the library.
danoni-stage-preparing-update = Preparing library update…

# Saving the import's records.
danoni-stage-saving-details = Saving import details…

# Adding the songs to the player's library.
danoni-stage-adding-songs = Adding songs to your library…

# Completing the import: the songs are in the library after it.
danoni-stage-finishing-import = Finishing import…

# The import is over; the importer is closing.
danoni-stage-finishing = Finishing DanOni import…

# The player cancelled; waiting for the importer to stop.
danoni-stage-cancelling = Cancelling DanOni import…

## How an import ended: the status line, wrapped at about 700 px, in red
## when nothing was imported.

# The work's songs were imported.
# Variables:
#   $added (Number) - songs new to the library
#   $replaced (Number) - songs that replaced their earlier import
#   $skipped (Number) - songs left as they were
danoni-done = DanOni import complete: { $added } added, { $replaced } replaced, { $skipped } skipped.

# Every song of the work was imported before, and has not changed.
danoni-unchanged = These songs are already in your DanOni library.

# The library was left half changed and needs repairing (the app does it
# when it opens again); the report under it says why. In red.
danoni-needs-recovery = Import needs library recovery.

# The player cancelled while the importer worked: nothing was added to
# the library ("publication": the songs appearing in it).
danoni-cancelled = DanOni import cancelled before publication.

# The player cancelled before the import itself began: nothing was added.
danoni-cancelled-early = DanOni import cancelled.

# The import failed; the report under it says why. In red.
danoni-failed = DanOni import could not finish.

# The player allowed the files of another web address for this import,
# but the choice to allow it for later imports too could not be saved.
# Variables:
#   $error (String) - why, as errors-* say it or the system's own words
danoni-not-remembered = Access allowed for this import; could not remember the decision: { $error }

## The report under the status line: a heading over a list of what went
## wrong, a line each (wrapped at about 680 px). An $error is what the
## system or the importer said itself, as errors-* say it or in its own
## words (often English).

# The heading over what went wrong: a list's heading, about 40 letters.
danoni-details = Details

# The heading over what went wrong after the songs were imported (they
# stay imported): a list's heading, about 40 letters.
danoni-after-import = After import

# The address typed is not a whole web address, or holds a user name or
# a password.
danoni-url-invalid = Enter a complete http:// or https:// DanOni work URL without embedded credentials.

# The importer was already importing something else.
danoni-busy = Another import is running.

# The importer could not get ready when it started; reinstalling the game
# puts its files back.
danoni-unavailable = The importer could not start. Reinstall the complete application package.

# The importer stopped in the middle of its work. Opening the app again
# checks its catalog and repairs what it left half done.
danoni-importer-stopped = The importer stopped unexpectedly. Reopen the app to verify/recover its catalog.

# After the import, the importer could not list the library's songs again.
# Variables:
#   $error (String) - why
danoni-refresh-failed = Library refresh failed: { $error }

# The game could not take the importer's new list of the songs to play;
# opening the app again reads it again.
# Variables:
#   $error (String) - why
danoni-catalog-rejected = Could not refresh the playable catalog: { $error }. Reopen the app to verify it.

# The importer's new list of the songs to play ended early; the game keeps
# the last whole list.
danoni-catalog-incomplete = The catalog refresh was incomplete; retaining the last complete snapshot.

# The importer that comes with the game is not where it should be, or its
# files changed since it was installed.
# Variables:
#   $error (String) - what is wrong with it
danoni-bundle-damaged = The packaged DanOni importer is missing or damaged: { $error }. Reinstall the complete application package.

# The system does not say where the game's data folder is.
danoni-no-data-folder = Cannot locate the application's data folder.

# The system does not say where the game's cache folder is.
danoni-no-cache-folder = Cannot locate the application's cache folder.

# The folders the importer keeps its files in could not be made.
# Variables:
#   $error (String) - why
danoni-storage-failed = Cannot create importer storage: { $error }

# A folder the importer keeps its files in is a file, or a symbolic link
# (a shortcut) to another place, which it refuses.
danoni-storage-not-folder = Importer storage must be a folder without symbolic links.

# The importer's program could not be started.
# Variables:
#   $error (String) - why
danoni-start-failed = Cannot start the DanOni importer: { $error }

# The importer could not open its catalog when it started.
danoni-catalog-unopened = The importer could not open its catalog.

# The same, with what the importer said.
# Variables:
#   $error (String) - why
danoni-catalog-unopened-because = The importer could not open its catalog: { $error }

# The game lost its connection with the importer's program.
# Variables:
#   $error (String) - what happened
danoni-connection-lost = The connection to the importer was lost: { $error }

# The importer that comes with the game does not match this version of it.
danoni-incompatible = The packaged importer uses an incompatible protocol.

# The importer answered something the game does not accept.
# Variables:
#   $error (String) - what was wrong with it
danoni-protocol = The importer sent an invalid response: { $error }

# The importer's program ended with a failure.
# Variables:
#   $error (String) - how it ended, as the system says it
danoni-exited = The importer exited with an error: { $error }

# The importer's program kept running after it had finished.
danoni-no-exit = The importer did not exit after closing its protocol.

# The importer worked longer than it may (half an hour for an import).
# Opening the app again checks its catalog and repairs what it left half
# done.
danoni-timed-out = The importer exceeded its time limit. Reopen the app to verify/recover its catalog.

# After a cancel, or at its end, the importer did not stop when asked.
danoni-not-stopped = The importer did not stop in time. Its catalog will be verified/recovered when the app reopens.

# The part of the game that talks with the importer stopped by itself.
danoni-client-stopped = The importer client stopped unexpectedly. Reopen the app to verify its catalog.

# The import ended without saying how.
danoni-lost-result = The importer lost its completion result.
