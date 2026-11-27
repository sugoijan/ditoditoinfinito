## The pages to read: calibration's menu and the credits, a column of
## headings, paragraphs (wrapped), links and buttons under the screen's
## header; and desktop's first-start offer of other games' song folders.

## Every page's footer: key hints after the keys' caps ("↑↓ Move"), a
## word or two each.

# Moves between the page's links and buttons.
pages-move-hint = Move

# A controller's hint: opens the link or presses the button the focus is
# on.
pages-activate = Activate

# A link a controller pressed: the browser opens links only from a key or
# a tap, so its line says so after the link's text.
# Variables:
#   $link (String) - the link's text, as its line shows it
pages-press-to-open = { $link } · press Enter or tap to open

## Calibration's menu: what the three timing tests do, a button to start
## each, the offsets kept for each device, resetting them. Paragraphs wrap
## in a column about 880 px wide (about 110 letters a line).

# The page's title beside its back icon on a phone (about 20 letters).
pages-calibration-title = Calibration

# The page's name in its header, in small capitals: written in capitals
# where the language has them.
pages-calibration-heading = CALIBRATION

# A heading over a line for each test run since the menu was last opened
# from the start (what each came to).
pages-calibration-last-test = Last test

# The first paragraph: how a test works.
pages-calibration-about = Each test plays a short song of clicks. Play along: the test measures how early or late your presses are, corrects for it as you play, and stops by itself once it is sure. Saving keeps the correction.
    .advanced = Each test plays a generated chart (100 BPM, three clicks and a rest, a click on every note) exactly like a song. While you play, the test measures how early or late your hits are, applies the correction on the fly and keeps measuring until the remaining error is within noise (95% confidence), then stops. Saving adds the correction to the setting.

# The second paragraph: the order the guided calibration runs the tests
# in. "Arrows only", "sound only" and "both" are the three tests below.
pages-calibration-order = Start guided calibration runs all three in turn: the arrows alone (how late your screen shows them), the sound alone (how late you hear it), then both, which should come out close to right.
    .advanced = The two offsets are independent, so the guided order is: arrows only (display), then sound only (audio path), then a check with both that should read close to zero.

# Under the first paragraphs, in a lighter color.
# Variables:
#   $ms (Number) - the largest offset a test measures, in milliseconds (250)
pages-calibration-range = A delay beyond ±{ $ms } ms cannot be measured this way. Set one that large by hand first: turn on Show advanced options (Options → Interface), then Options → Timing.
    .advanced = Offsets beyond ±{ $ms } ms cannot be measured this way (a press that late cannot be told apart from one on the next note); set those by hand in the options, then calibrate from there.

# The first test's heading: the arrows drawn, no sound.
pages-calibration-visual = 1. Arrows only

# Under it: what the first test does.
pages-calibration-visual-about = No sound. Press each arrow as it reaches its target. Measures how late your screen shows the arrows.
    .advanced = Muted. Hit each arrow at the receptor. Measures display lag; saves to the visual offset (only moves what is drawn).

# The large button under the first test: runs the three tests in turn,
# starting with the first. Sized to its text.
pages-calibration-start-guided = Start guided calibration

# The second test's heading: the clicks heard, no arrows.
pages-calibration-audio = 2. Sound only

# Under it: what the second test does.
pages-calibration-audio-about = No arrows. Press any arrow key on each click. Measures how late you hear the sound.
    .advanced = Arrows hidden; press any arrow key on each click. Measures the audio path; saves to the audio offset (the judged timeline).

# A button under the second and the third test: runs that test alone.
# Sized to its text.
pages-calibration-start = Start

# The third test's heading: arrows and sound together.
pages-calibration-combined = 3. Check: both

# Under it: what the third test does.
pages-calibration-combined-about = Play normally. After 1 and 2 it should find little left to correct; what it finds is saved to the sound's delay.
    .advanced = Play normally. Should read near zero after 1 and 2; a significant remainder can be saved as a fine-tune to the audio offset.

# A heading over the offsets kept.
pages-calibration-offsets = Delays
    .advanced = Offsets

# The offsets used for a sound output or a display the game has not seen
# before.
# Variables:
#   $audio (String) - the audio offset in milliseconds, signed ("+12", "-3", "+0")
#   $visual (String) - the visual offset in milliseconds, signed
pages-calibration-defaults = Defaults, for devices without a profile: audio { $audio } ms, visual { $visual } ms.

# In place of the devices' list when there is none.
pages-calibration-no-device = No device calibrated yet.

# Under "Delays" while advanced options are hidden, in place of the
# defaults and every device's list: this device's sound and picture
# delays as the tests left them (the last sound output and display the
# game saw).
# Variables:
#   $audio (String) - the sound's delay in milliseconds, signed ("+12", "+0")
#   $visual (String) - the picture's delay in milliseconds, signed
pages-calibration-this-device = This device: sound { $audio } ms, picture { $visual } ms.

# Over the devices' list.
pages-calibration-per-device = Offsets are kept per audio path and per display; the game picks the matching one when a song starts.

# A sound output in the devices' list: its name and its offset, marked
# when a test measured it.
# Variables:
#   $device (String) - the output, as devices-audio* word it ("48 kHz · ~20 ms output latency")
#   $ms (String) - its offset in milliseconds, signed ("+12", "-20")
#   $calibrated (String) - "yes" when a test measured it, else "no"
pages-calibration-audio-device = { $calibrated ->
    [yes] Audio: { $device } — { $ms } ms (calibrated)
   *[no] Audio: { $device } — { $ms } ms
}

# A display in the devices' list, as above.
# Variables:
#   $device (String) - the display, as devices-display* word it ("1512×982 (2×), ~120 Hz")
#   $ms (String) - its offset in milliseconds, signed ("+12", "-20")
#   $calibrated (String) - "yes" when a test measured it, else "no"
pages-calibration-display-device = { $calibrated ->
    [yes] Display: { $device } — { $ms } ms (calibrated)
   *[no] Display: { $device } — { $ms } ms
}

# A button: sets every offset back to 0 (asked first). Sized to its text.
pages-calibration-reset = Reset the delays to 0
    .advanced = Reset offsets to 0

# Near the page's end, in a lighter color. “Timing error of each hit” is
# the name of a row in the options (Screen): write it as that row is
# translated.
pages-calibration-tip = Tip: with Show advanced options On (Options → Interface), Options lists each device's delays, and a play can show how early or late each press was.
    .advanced = Tip: turn on “Timing error of each hit” in the options to see each hit's offset during normal play; the debug overlay shows the running mean.

# The last button: opens the options. Sized to its text.
pages-calibration-options = Options

# The question before the offsets are reset (a dialog's title).
pages-calibration-reset-title = Reset the delays?
    .advanced = Reset the offsets?

# The question's text, under its title.
pages-calibration-reset-body = The sound and picture delays of this device and every other go back to 0 ms.
    .advanced = The default audio and visual offsets and every device's go back to 0 ms; the devices stay known, as not calibrated.

## What a test run came to: a line each under "Last test". The tests are
## named as their headings: "the arrows-only test" (1), "the sound-only
## test" (2), "the check with both" (3). Each variable $test is the test
## run: "visual" (1), "audio" (2) or "combined" (3).

# A device (the sound output or the display) changed during the test.
# Variables:
#   $test (String) - the test: "visual", "audio" or "combined"
pages-calibration-run-device-changed = { $test ->
    [visual] The arrows-only test: a device changed during it, so nothing was saved. Run it again.
    [audio] The sound-only test: a device changed during it, so nothing was saved. Run it again.
   *[combined] The check with both: a device changed during it, so nothing was saved. Run it again.
}

# The hits were too few or too scattered to measure anything.
# Variables:
#   $test (String) - the test: "visual", "audio" or "combined"
pages-calibration-run-unreliable = { $test ->
    [visual] The arrows-only test could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm.
    [audio] The sound-only test could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm.
   *[combined] The check with both could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm.
}

# As above, and the hits came later than a test can measure (the sound
# may be that late: Bluetooth headphones; for the arrows-only test, the
# picture: a mirrored screen).
# Variables:
#   $test (String) - the test: "visual", "audio" or "combined"
pages-calibration-run-out-of-range = { $test ->
    [visual] The arrows-only test could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the arrows really show that late, set the picture's delay by hand first: turn on Show advanced options (Options → Interface), then Options → Timing.
    [audio] The sound-only test could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the clicks really sound that late, set the sound's delay by hand first: turn on Show advanced options (Options → Interface), then Options → Timing.
   *[combined] The check with both could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the clicks really sound that late, set the sound's delay by hand first: turn on Show advanced options (Options → Interface), then Options → Timing.
}
    .advanced = { $test ->
        [visual] The arrows-only test could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the arrows really show that late, set the visual offset by hand in the options first.
        [audio] The sound-only test could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the clicks really sound that late, set the audio offset by hand in the options first.
       *[combined] The check with both could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the clicks really sound that late, set the audio offset by hand in the options first.
    }

# The test measured no meaningful error: the offset stays. The arrows-only
# test measures the visual offset, the others the audio offset.
# Variables:
#   $test (String) - the test: "visual", "audio" or "combined"
pages-calibration-run-right = { $test ->
    [visual] The arrows-only test found the picture's delay right as it is.
    [audio] The sound-only test found the sound's delay right as it is.
   *[combined] The check with both found the sound's delay right as it is.
}
    .advanced = { $test ->
        [visual] The arrows-only test found the visual offset right as it is.
        [audio] The sound-only test found the audio offset right as it is.
       *[combined] The check with both found the audio offset right as it is.
    }

# The test changed the offset.
# Variables:
#   $test (String) - the test: "visual", "audio" or "combined"
#   $ms (String) - the offset now, in milliseconds, signed ("+12", "-3")
pages-calibration-run-set = { $test ->
    [visual] The arrows-only test set the picture's delay to { $ms } ms.
    [audio] The sound-only test set the sound's delay to { $ms } ms.
   *[combined] The check with both set the sound's delay to { $ms } ms.
}
    .advanced = { $test ->
        [visual] The arrows-only test set the visual offset to { $ms } ms.
        [audio] The sound-only test set the audio offset to { $ms } ms.
       *[combined] The check with both set the audio offset to { $ms } ms.
    }

## The credits: who made the bundled songs, noteskins, announcers, fonts,
## the video decoder and the code from other projects, and their licenses.
## Titles, names, licenses' names and their texts are written as their
## works give them. Paragraphs wrap in a column about 880 px wide.

# The page's title beside its back icon on a phone (about 20 letters).
pages-credits-title = Credits

# The page's name in its header, in small capitals: written in capitals
# where the language has them.
pages-credits-heading = CREDITS

# While the credits' file loads.
pages-credits-loading = loading…

# When the credits' file could not be read.
# Variables:
#   $error (String) - why: its kind as errors-* say it, then the system's own words (untranslated)
pages-credits-unreadable = The credits could not be read: { $error }

# The page's first paragraph. CC0 and MIT are licenses' names.
pages-credits-about = { -game-name }'s code is MIT-licensed. The bundled demo songs, noteskins and fonts, the video decoder and the code from other projects are third-party works used under their own licenses, credited here as those licenses require. Our announcers are CC0, credited with what made them.

# A link to where a work's source is (the game's own, a song's, a
# noteskin's).
# Variables:
#   $url (String) - its address
pages-credits-source = Source: { $url }

## The sections' headings.

pages-credits-songs = Songs
pages-credits-noteskins = Noteskins
pages-credits-announcers = Announcers
pages-credits-fonts = Fonts
pages-credits-video = Video decoder
pages-credits-code = Code from other projects
pages-credits-rust = Rust libraries

## Under each work (a song, a noteskin), in a lighter color.

# Which version of its source the work is (a Git commit) and where in it.
# Variables:
#   $commit (String) - the commit, its first ten characters
#   $folder (String) - the work's folder in its source
pages-credits-commit-folder = commit { $commit }, folder “{ $folder }”

# As above, with the day it was taken.
# Variables:
#   $commit (String) - the commit, its first ten characters
#   $folder (String) - the work's folder in its source
#   $date (String) - the day it was taken ("2026-10-01")
pages-credits-commit-folder-retrieved = commit { $commit }, folder “{ $folder }”, retrieved { $date }

# Who made a song's charts.
# Variables:
#   $authors (String) - each author with the charts' difficulties in parentheses ("Daniel Rotwind (Beginner, Easy)")
pages-credits-chart-authors = chart authors: { $authors }

# Files of the work's source the game leaves out.
# Variables:
#   $files (String) - the files, separated by ";"
pages-credits-not-included = not included: { $files }

# The work is as its source has it.
pages-credits-no-modifications = modifications: none

# What was changed in the work (its license asks to say so).
# Variables:
#   $changes (String) - each change with the command that made it, separated by ";"
pages-credits-modifications = modifications: { $changes }

## Our announcers: what made each voice.

# A link to the speech model that made the announcer.
# Variables:
#   $url (String) - its address
pages-credits-model = Model: { $url }

# The model's version.
# Variables:
#   $revision (String) - the model's revision, its first ten characters
pages-credits-revision = revision { $revision }

# As above, with the day the announcer was made.
# Variables:
#   $revision (String) - the model's revision, its first ten characters
#   $date (String) - the day it was made ("2026-11-04")
pages-credits-revision-made = revision { $revision }, made { $date }

# A link to recordings the model's voice learned from, credited as their
# license asks.
# Variables:
#   $voice (String) - the voice's name in the model ("jm_kumo")
#   $what (String) - the recordings, as their source names them
#   $holders (String) - who holds their rights, separated by ","
#   $license (String) - their license's name ("CC-BY-3.0")
pages-credits-learned-from = its voice ({ $voice }) learned from { $what }: { $holders } — { $license }

## Fonts, the code from other projects.

# A link to where a font or a project's code comes from (a word).
pages-credits-source-link = source

# How a font was changed (it was cut to the letters the game uses).
# Variables:
#   $how (String) - as the font's record says it ("subset")
pages-credits-modified = modified: { $how }

# Who holds a project's code's rights, and the version of it taken.
# Variables:
#   $holders (String) - the holders, separated by ";"
#   $commit (String) - the Git commit, its first ten characters
pages-credits-holders-commit = { $holders } · commit { $commit }

# A folded section with the license's notice a project's code asks to be
# shown (its text is the project's).
pages-credits-notice = Notice

## The video decoder.

# What the video decoder is and how it is built, after its heading. The
# desktop game runs it in a sandbox; the web page fetches it.
# Variables:
#   $platform (String) - "web" or "desktop"
#   $name (String) - the decoder's project ("FFmpeg")
#   $version (String) - its version ("8.0")
#   $license (String) - its license's name ("LGPL-2.1-or-later")
#   $sha256 (String) - its source's SHA-256 checksum (64 hex digits)
pages-credits-video-about = { $platform ->
    [desktop] Background videos play through a separate module, run in a sandbox of its own when a song needs it. It uses code of { $name } { $version } licensed under the { $license }; its source, unmodified, comes with the module (SHA-256 { $sha256 }). It is built by video/build.sh in this game's repository with the flags below and linked with the game's own code (MIT).
   *[web] Background videos the browser cannot decode play through a separate module, fetched only when a song needs it. It uses code of { $name } { $version } licensed under the { $license }; its source, unmodified, is served next to the module (SHA-256 { $sha256 }). It is built by video/build.sh in this game's repository with the flags below and linked with the game's own code (MIT).
}

# A link to the decoder's source as the game has it.
# Variables:
#   $file (String) - its file's name ("ffmpeg-8.0.tar.xz")
pages-credits-video-source = Its source: { $file }

# A link to the same source on FFmpeg's site.
pages-credits-video-release = The release at FFmpeg

# A link to the source's version tag in FFmpeg's repository.
pages-credits-video-tag = The tag

# A link to the decoder's licenses' full texts.
pages-credits-video-licenses = License texts

# Under that link, in a lighter color: where the texts are.
# Variables:
#   $platform (String) - "web" or "desktop"
pages-credits-video-licenses-where = { $platform ->
    [desktop] (next to the module, in builds that include it)
   *[web] (served with the module, in builds that include it)
}

# A folded section: the options the decoder was built with (FFmpeg's
# configure script's flags, shown as they are).
pages-credits-configure = Configure flags

## The Rust libraries the game is built on, their licenses' notices.

# Under the heading.
# Variables:
#   $platform (String) - "web" or "desktop"
#   $count (Number) - how many libraries
pages-credits-rust-about = { $platform ->
    [desktop] The game is built on { $count } open-source Rust libraries in the desktop app. Their licenses ask for these notices; where a library offers a choice of licenses, the one it is used under is named.
   *[web] The game is built on { $count } open-source Rust libraries in the web app (with its Opus decoder). Their licenses ask for these notices; where a library offers a choice of licenses, the one it is used under is named.
}

# A folded section: the libraries, a line each.
# Variables:
#   $count (Number) - how many
pages-credits-rust-crates = The libraries ({ $count })

# A folded section: the licenses' notices, each folded too.
# Variables:
#   $count (Number) - how many
pages-credits-rust-notices = Their license notices ({ $count })

# A library's line: its name, version and license, and the numbers of
# its notices below.
# Variables:
#   $name (String) - the library's name
#   $version (String) - its version
#   $license (String) - its license, as it declares it ("MIT OR Apache-2.0")
#   $notices (String) - its notices' numbers ("2", "1, 3")
pages-credits-rust-crate = { $name } { $version } — { $license } (notice { $notices })

# As above, for a library offering a choice of licenses: the one taken.
# Variables:
#   $name (String) - the library's name
#   $version (String) - its version
#   $license (String) - its licenses, as it declares them ("MIT OR Apache-2.0")
#   $chosen (String) - the one it is used under ("MIT")
#   $notices (String) - its notices' numbers ("2", "1, 3")
pages-credits-rust-crate-chosen = { $name } { $version } — { $license }, used under { $chosen } (notice { $notices })

# A notice's folded section: its number, its license and the libraries it
# is for, one line.
# Variables:
#   $number (String) - its number ("1")
#   $license (String) - the license's name
#   $crates (String) - one or two libraries with their versions, separated by ","
pages-credits-rust-notice = { $number }. { $license } — { $crates }

# As above, for a notice of more than two libraries: the first two, then
# how many more.
# Variables:
#   $number (String) - its number ("1")
#   $license (String) - the license's name
#   $first (String) - the first library with its version
#   $second (String) - the second library with its version
#   $more (Number) - how many more
pages-credits-rust-notice-more = { $number }. { $license } — { $first }, { $second } and { $more } more

# Inside a notice's section, over its text: every library it is for.
# Variables:
#   $crates (String) - the libraries with their versions, separated by ","
pages-credits-rust-notice-for = for { $crates }

## The privacy note: what the game and its online server keep, and the
## usage totals the server counts. A fold near the top of the credits
## (its label, then headings and text, wrapped in a column about 880 px
## wide), the same text as the site's privacy page (site/privacy.html).
##
## Written in plain language (since 2026-11-22): short sentences with one
## idea each, active voice, "I" for the one person who makes the game and
## runs its server (since 2026-11-27: a solo developer, not a company)
## and "you" for the reader, the same word for the same thing every time
## (keep, delete, my server where a part first names it or the server
## is an object, the server where it acts, your device, the game, the
## app), no facts
## in parentheses, and a list wherever there are three or more items. A
## body's paragraphs are separated by an empty line; a list's lines
## start with "• " (the fold has no list of its own: each line is drawn
## as text). Keep the lines and the empty lines as they are. Japanese:
## やさしい日本語 (locales/ja/GLOSSARY.md, "The privacy note").
## No legal terms. Twitch is the sign-in's provider, named as people see
## it; "User-Agent" is a browser's technical header, kept as it is.
## Options' rows are named as they read there.

# The fold's label, and what the multiplayer home's footer and Options'
# row are called (a word or two).
pages-privacy = Privacy

# The note's first paragraph.
pages-privacy-intro = This note tells you what { -game-name } and its online server keep about you. It also tells you for how long. I last changed this note on November 27, 2026.

# A heading inside the note.
pages-privacy-alone-heading = Playing alone

# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-alone = { $platform ->
    [web]
        When you play alone, the game keeps everything on your device: your settings, your songs and your play history. In a browser, they stay in the browser's storage. In the app, they stay in the app's folders on your computer.

        The game sends nothing about you or your plays to my server. If someone shares a song with you by a link, the game reads that song from my server.
   *[desktop]
        When you play alone, the game keeps everything on your device: your settings, your songs and your play history. They stay in the app's folders on your computer.

        The game sends nothing about you or your plays to my server. If someone shares a song with you by a link, the game reads that song from my server.
}

# A heading inside the note.
pages-privacy-sign-in-heading = Signing in

# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-sign-in = { $platform ->
    [web]
        To use online rooms, you must sign in with Twitch. You do not have to sign in to watch a room as the audience.

        When you sign in, my server asks Twitch for three things:
        • your Twitch user ID
        • your login name
        • your display name

        The server keeps none of Twitch's keys. The three things go into a signed session. Your browser keeps the session as a cookie. The app keeps the session in its settings folder.

        A session lasts one week. It is renewed while you use it. It never lasts more than 30 days after you signed in.

        When you sign in from the app, the server keeps the sign-in attempt for 11 minutes at most. The attempt holds your Twitch user ID, your login name and your display name. It also holds the session until the app takes it.

        The server keeps no list of the people who signed in. Signing out removes the session from your device. I never ask Twitch for your email address.
   *[desktop]
        To use online rooms, you must sign in with Twitch. You do not have to sign in to watch a room as the audience.

        When you sign in, my server asks Twitch for three things:
        • your Twitch user ID
        • your login name
        • your display name

        The server keeps none of Twitch's keys. The three things go into a signed session. The app keeps the session in its settings folder.

        A session lasts one week. It is renewed while you use it. It never lasts more than 30 days after you signed in.

        When you sign in from the app, the server keeps the sign-in attempt for 11 minutes at most. The attempt holds your Twitch user ID, your login name and your display name. It also holds the session until the app takes it.

        The server keeps no list of the people who signed in. Signing out removes the session from your device. I never ask Twitch for your email address.
}

# A heading inside the note.
pages-privacy-rooms-heading = Rooms

# "A room's session log" and "Watching as the audience" name the note's
# headings of those parts (pages-privacy-sessions-heading,
# pages-privacy-audience-heading).
pages-privacy-rooms =
    A room keeps what its members see:
    • the room's name
    • each member's display name and Twitch user ID, so that members can come back
    • each member's role
    • the room's song list: each song's title, artist, pack and charts, and who added the song
    • where the players choose the songs: each player's choice of the next song until each draw
    • where the players choose the songs: the last draw, that is, who chose which song and which song was drawn
    • the songs picked
    • the results of the room's matches

    A room is deleted when it ends. A room ends:
    • two minutes after the connection of its last member closed
    • two minutes after it was made, if no member connected to it
    • when its host leaves and the room does not pass to another member
    • when its host loses the connection for too long, in a room set to close when its host leaves
    • when the server's administrators close it

    After a room ends, the server keeps only a note that the room ended. This note holds the room's ID, when the room ended and why. The server keeps this note for one week, so that the room's links can say that the room is closed.

    These times are the usual ones. The server's administrators can change them. They can also turn off the two-minute limit. Then a room with no member connected ends after one week without activity.

    The room's session log is kept apart and names nobody. The part "A room's session log" below tells you about it.

    While a room is open, the server's administrators can see it in a list. The list shows the room's name and the display name of its host. It also shows what the room is doing, for example which stage it plays. It also shows counts, for example how many people are in the room.

    An administrator can also open the details of a room. The details show each member's display name, role and connection, and whether the member plays or watches. They also show the seats of the audience by short codes. The part "Watching as the audience" below tells you about these codes.

# A heading inside the note.
pages-privacy-songs-heading = Songs

pages-privacy-songs =
    The host and the co-hosts of a room can add songs to the room's song list. If the host lets everyone add songs, anyone in the room can add songs.

    When someone adds a song, the game uploads the song from their device to my server. This lets everyone in the room play it. The room keeps who added the song. With the copy, the server keeps the room's ID. It also keeps the Twitch user ID of the person who added the song.

    While the room is open, anyone with the link to this copy can read it. The server keeps the copy as long as the room exists. It deletes the copy:
    • when the song is removed from the list: by the person who added it, by a host, or to make space in a full list
    • when the room ends

    A room may stop without ending. Then nobody can read its songs one day after the room last showed that it was open. The server deletes them within the next hour.

    I give a key to a few people. These people can also share songs by link. Such a song is kept until the expiry that was chosen when it was shared, or until it is deleted. The expiry can be from one hour to one month. With such a song, the server keeps a short code made from the key that shared it.

    If the upload of a song never finishes, the server deletes the song after one day.

    The terms of use say which songs you may add, and how to report a song.

# A heading inside the note: the room's songs the game downloads ahead.
pages-privacy-songs-ahead-heading = Room songs on your device

# "Room songs ahead" is Options → Multiplayer's row (options-room-ahead), named as
# it reads there; its values are Everything, Likely next and Off.
# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-songs-ahead = { $platform ->
    [web]
        While you are in a room, the game also downloads the room's songs onto your device before they are picked. This lets a stage start at once. The game never downloads the songs' movies.

        Options → Multiplayer → Room songs ahead decides which songs the game downloads:
        • every song of the list
        • only the songs that will likely come next
        • none

        The songs that will likely come next are:
        • the song picked for the next stage
        • the song that the person who picks the songs is looking at
        • up to 8 of the newest other songs, 128 MB at most in all

        Unless you choose none, the game first downloads a banner and a list of files. It does this for each song that your device does not have.

        Every song of the list is the default. A browser on a small screen, such as a phone, has another default: only the songs that will likely come next. The default is the same in a browser that is set to save data. If the browser's storage becomes full, the game downloads only the songs that will likely come next.

        The game keeps these songs apart from your own songs and your play history. In a browser, it keeps them in the browser's storage. In the app, it keeps them in the app's cache folder.

        The game deletes these songs when you leave the room. If the game stopped while you were in a room, it deletes them the next time it starts. In a browser, it does this only when no page of the game has used them for 30 minutes.

        Your device tells my server which of the room's songs it has. The server passes this on to the devices of the room's members. Then the game can show how many players have each song. The server keeps this only while your connection to the room is open. It stores none of it.
   *[desktop]
        While you are in a room, the game also downloads the room's songs onto your device before they are picked. This lets a stage start at once. The game never downloads the songs' movies.

        Options → Multiplayer → Room songs ahead decides which songs the game downloads:
        • every song of the list
        • only the songs that will likely come next
        • none

        The songs that will likely come next are:
        • the song picked for the next stage
        • the song that the person who picks the songs is looking at
        • up to 8 of the newest other songs, 128 MB at most in all

        Unless you choose none, the game first downloads a banner and a list of files. It does this for each song that your device does not have.

        Every song of the list is the default.

        The game keeps these songs apart from your own songs and your play history. It keeps them in the app's cache folder.

        The game deletes these songs when you leave the room. If the game stopped while you were in a room, it deletes them the next time it starts.

        Your device tells my server which of the room's songs it has. The server passes this on to the devices of the room's members. Then the game can show how many players have each song. The server keeps this only while your connection to the room is open. It stores none of it.
}

# A heading inside the note: a person's Twitch access for a room's
# features (round TW1b).
pages-privacy-twitch-heading = Twitch access for a room's features

# "Options → Data and privacy" is where the Twitch access is disconnected.
pages-privacy-twitch =
    A room's host can turn on two features that use Twitch:
    • keep out of the room the people that the host banned or blocked on Twitch
    • show the host's Twitch chat during play

    A new room has both features on. If you turned a feature off for your last room, it stays off. If you did not allow keeping people out when Twitch asked, it also stays off. This device remembers that choice. You can turn each feature on or off in the room's Host options.

    When you make a room, the game asks Twitch for what keeping people out needs. It asks when you sign in to make a room. If you are already signed in, it asks when you make the room. It does not ask if you turned this feature off. If you do not allow it, the room is made with this feature off. When you only join a room, the game asks Twitch for nothing new.

    Each feature needs only this:
    • keeping out banned people: reading who is banned or timed out in your channel
    • keeping out blocked people too: reading who you blocked
    • showing your chat: nothing from you

    To see a host's chat on your device, turn on Options → Multiplayer → Twitch chat in rooms. The game then asks Twitch for reading chat.

    If you allow it, my server keeps Twitch's keys for this access. It seals the keys so that only the server can read them. It never gives the keys to the game.

    The server uses this access only when a room's feature needs it. It deletes the access:
    • when you sign out
    • when you disconnect it in Options → Data and privacy
    • after 30 days without use
    • when Twitch ends it

    If the access ends while you host a room, the room stops keeping people out. It still shows your chat.

    The game then offers to ask Twitch again for the permissions that you had. It does not offer this when you disconnect on this device. If you allow them, the room keeps people out again, as before. If you do not, your next rooms start with this feature off.

    Until then, this device remembers that the room kept people out. It forgets this when you close the game.

    This device also remembers which of these permissions you used, and when. It keeps this in the game's settings on this device, never on my server. You use a permission when you:
    • host a room that keeps out people banned or blocked on Twitch
    • show a host's chat on this device

    When you sign in on this device again, it asks Twitch for the permissions that you used in the last 30 days. It does this in the same step as the sign-in. So you do not go through Twitch again for each permission when you sign out and in. The device does not ask for a permission that you have not used for 30 days.

    When you disconnect in Options → Data and privacy, this device forgets these permissions.

    When a host's room keeps people out in this way, the server asks Twitch about each person:
    • when the person joins
    • when the person's connection opens again
    • when a song is picked
    • when the host turns this on, or changes it to keep out blocked people too
    • when a new host of the room sets it
    • when Twitch tells a device in the room that the person was banned or timed out in the host's channel

    The server keeps the answers only in memory, for five minutes at most. It keeps an answer about a timeout no longer than the timeout lasts. It never stores the answers.

    Someone who is kept out is told only: "You can't join this room." The other members see that person leave. The server's logs count these checks and never name anyone.

# A heading inside the note: a device showing a host's Twitch chat
# (round TW7).
pages-privacy-twitch-chat-heading = A host's Twitch chat on your device

# "Emotes from" and "Twitch" are Options → Multiplayer's row and its value.
pages-privacy-twitch-chat =
    When your device shows a host's Twitch chat during play, the game on your device connects directly to Twitch. Twitch sends the chat's messages to your device. So Twitch sees:
    • your device's address
    • that your Twitch account reads that channel's chat

    My server only asks Twitch to start this. It never sees a message.

    Twitch also tells your device when someone is banned or timed out in that chat. In a room that keeps out people banned on Twitch, your device then tells my server who it is. It does this only for someone who is in the room. The server then asks Twitch about that person itself.

    The server keeps in memory, for one minute at most, who asked and about whom. It never stores this.

    To show emotes, your device asks 7TV, BetterTTV and FrankerFaceZ for emote lists: the channel's lists and their global lists. It also asks these three services and Twitch for the emote pictures. So these services see your device's address.

    If a direct request fails, my server asks in place of your device. Then the services see the server's address instead. The server keeps each emote picture or list that it gets in this way for one day at most. It does not store who asked for it.

    If you set Options → Multiplayer → Emotes from to Twitch, your device does not ask the other three services.

# The part on a host's Kick.
pages-privacy-kick-heading = Removing someone from a room
pages-privacy-kick = The host and the co-hosts of a room can remove a member. The game calls this Kick. Everyone in the room is told who was removed and who removed them. Nothing else is kept. The removed person can come back by the room's invite link.

# A heading inside the note.
pages-privacy-audience-heading = Watching as the audience

pages-privacy-audience =
    You can watch a room as the audience without signing in.

    My server then turns your network's address into a mark, so that one network cannot take every seat. The mark is a code made with a secret key. The code is different every day. Without the key, nobody can turn the mark back into the address. The key never leaves the server. The server does not keep the address itself.

    The mark exists only in the server's memory, and only while your seat is open. The server's administrators see only a short code of eight characters made from the mark.

    If you are signed in, your seats count against your account instead. Then no mark is made. The server's administrators see a short code of eight characters made from your account.

# A heading inside the note.
pages-privacy-stats-heading = Usage statistics

# What the server counts by itself, as a list. Adaptive is the match
# rule's name (Charts: Adaptive), shocks a battle's arrows.
# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-stats = { $platform ->
    [web]
        My server counts what happens in rooms, as daily totals. This helps me see whether online play works well. For example, the server counts:
        • what kind of device and browser opened a connection: a phone, a tablet or a computer, and Chrome, Edge, Firefox, Safari, Samsung Internet or another browser. The server reads this from your browser's User-Agent when the connection opens. Then it drops the User-Agent. The User-Agent is a line of text in which your browser describes itself.
        • how connections closed
        • how long connections lasted, in ranges
        • how long people were away, in ranges, and whether they came back
        • the round trip from your network to my server, in ranges. The round trip is the time that a message takes to go to the server and back.
        • how long songs took to upload and to load
        • why players sat a stage out
        • what was played: the charts' difficulty range and style, the rules, when runs ran out of life, the attacks sent, the shocks stepped on and the steps on the Adaptive ladder
        • how many different people were in rooms each day
   *[desktop]
        My server counts what happens in rooms, as daily totals. This helps me see whether online play works well. For example, the server counts:
        • what kind of device opened a connection: a phone, a tablet, a computer or this app. The server reads this from the connection's User-Agent when the connection opens. Then it drops the User-Agent. The User-Agent is a line of text in which a program describes itself.
        • how connections closed
        • how long connections lasted, in ranges
        • how long people were away, in ranges, and whether they came back
        • the round trip from your network to my server, in ranges. The round trip is the time that a message takes to go to the server and back.
        • how long songs took to upload and to load
        • why players sat a stage out
        • what was played: the charts' difficulty range and style, the rules, when runs ran out of life, the attacks sent, the shocks stepped on and the steps on the Adaptive ladder
        • how many different people were in rooms each day
}

# After the list of what is counted.
pages-privacy-stats-kept =
    These totals contain no names, accounts, addresses, rooms or song titles.

    The server counts the people of each day without knowing who they are. For this, it keeps a code made from each account and the day, with a secret key. The code changes every day. The server deletes the code once it has counted the day.

    The server keeps the totals day by day for 13 months. After that, it keeps them month by month.

    Bots and the game's computer players are counted apart from people. Playing alone is never counted.

# A heading inside the note: what the game itself sends for the totals
# (the stats proposal's step 2).
# Variables:
#   $platform (String) - web or desktop (desktop's says the app)
pages-privacy-stats-game-heading = { $platform ->
    [web] What the game sends for the totals
   *[desktop] What the app sends for the totals
}

# What the game itself sends for the totals, and the Options row that
# turns it off (options-usage-stats, "Send usage statistics", in the Data
# section). The site's page adds a list of how the desktop app's report
# differs.
# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-stats-game = { $platform ->
    [web]
        While you are in an online room, the game also tells my server a few things for these totals. It does this each time its connection to the room opens. It tells:
        • that it is the game in a browser
        • your system, such as Windows, macOS, Android or iOS
        • whether your device is a phone, a tablet or a computer
        • whether the game draws with WebGPU or WebGL2
        • whether the screen is a touch screen
        • the game's version
        • how many times it has connected to the room
        • how many tries this connection took
        • how its previous connection to the room ended, for example closed or dropped, and with which code
        • how long that connection lasted
        • how long the page was in the background during that connection
        • whether the page was in the background when that connection ended, and for how long
        • whether that connection's keep-alive messages were answered. Keep-alive messages check that a connection still works.
        • the round trip to my server

        Each of these things is one value from a fixed list, or a range. It is never a name, an account, an address, a room or a song. The server adds these values to the totals and keeps nothing else of them.

        To turn this off, set Options → Data and privacy → Send usage statistics to Off. Then the game sends none of these things. The server's own counts above go on.

        When you watch as the audience, the game sends none of these things.
   *[desktop]
        While you are in an online room, the app also tells my server a few things for these totals. It does this each time its connection to the room opens. It tells:
        • that it is the app
        • your system: Windows, macOS or Linux
        • that your device is a computer
        • whether it draws with Metal, Vulkan, DirectX 12 or OpenGL
        • the game's version
        • how many times it has connected to the room
        • how many tries this connection took
        • how its previous connection to the room ended, for example closed or dropped, and with which code
        • how long that connection lasted
        • how long the window was hidden during that connection
        • whether the window was hidden when that connection ended, and for how long
        • that the app keeps its connection alive by itself
        • the round trip to my server

        Each of these things is one value from a fixed list, or a range. It is never a name, an account, an address, a room or a song. The server adds these values to the totals and keeps nothing else of them.

        To turn this off, set Options → Data and privacy → Send usage statistics to Off. Then the app sends none of these things. The server's own counts above go on.

        When you watch as the audience, the app sends none of these things.
}

# A heading inside the note (the stats proposal's step 3: what a room's
# session log keeps, by seat numbers alone, for 14 days).
pages-privacy-sessions-heading = A room's session log

# Send usage statistics is the Options row (options-usage-stats), named
# as it reads there.
pages-privacy-sessions =
    Each room keeps a short log of its session. It is there so that problems in a room can be understood afterwards. For example, the log can show why people lost their connection. The log holds:
    • when each seat joined, left or came back
    • each seat's role, and whether the seat played or watched
    • when each connection was asked for, opened and closed, with which code, and how long it lasted
    • when someone was away, and whether they came back
    • the kind of device and browser of each seat, as in the totals above
    • what the game of each seat told my server for the totals above
    • how long songs took to upload and to load
    • how big each upload was, in ranges
    • when a seat could not load a song, and why
    • when a song was added to the room's list, and by which seat, but never which song
    • when a song left the room's list, and why, but never which song
    • when a seat's own choice started the countdown to a draw of the next song
    • each draw: how many choices there were, but never which song
    • why a countdown ended without a draw
    • who was ready
    • who sat a stage out, and why
    • who entered a stage late
    • how runs ended
    • the room's own steps, for example when a match or a stage started or ended, and how the room closed
    • when someone was turned away, for example because the room was full

    The log names nobody. Each person is only a seat number, and a seat number means something only inside that room. The log holds no names, accounts, addresses, room name, room ID or song titles.

    The log marks bots and the game's computer players as such. If you turned Send usage statistics off, the log has only what the server saw by itself.

    The room keeps the log while it is open. If the log becomes too long, the room drops the oldest events. When the room ends, the server keeps the log for 14 days for its administrators to read. Then the server deletes the log. The server deletes the oldest logs sooner if it holds more than 20,000 logs or 512 MB of logs.

# A heading inside the note (the stats proposal's step 4: the web game's
# own error reports).
pages-privacy-errors-heading = Error reports

# Send error reports is the Options row (options-error-reports), named as
# it reads there; Global Privacy Control is a browser setting, named as
# browsers name it. "Watching as the audience" names the note's heading
# of that part (pages-privacy-audience-heading).
# Variables:
#   $platform (String) - web or desktop (the app sends none)
pages-privacy-errors = { $platform ->
    [web]
        When the game itself goes wrong in your browser, it sends my server a short report. The game goes wrong when there is:
        • a crash
        • a failure in the game's code that nothing caught
        • a graphics device that is lost or missing
        • a page that cannot start the game

        The report holds at most:
        • the kind of error
        • the kind of screen you were on, such as song select or a play, but never the page's address
        • the game's version
        • your system
        • the kind of device
        • the graphics that the game uses
        • the error's message

        The game takes these things out of the message:
        • file paths, but it keeps where in the game's program the error happened
        • web addresses and email addresses
        • network addresses
        • quoted text
        • long numbers
        • IDs
        • any letters beyond English

        The game also cuts the message to 300 characters. The message can still hold an ordinary English word. An example is a word of a song's title that the game's own message did not quote.

        The server reads the kind of browser and device from your browser's User-Agent, as it does for the totals. Then it drops the User-Agent. The server takes the same things out of the message again. It drops a report that still holds any of them.

        The server counts reports in the daily totals by kind, version, screen, browser, device and graphics. For each distinct error, it keeps one example message. It keeps that example for 30 days after the error was last seen. It keeps 500 examples at most. When it has more, it first deletes the examples whose errors were last seen longest ago.

        To stop floods of reports, the server counts the reports from one network for one minute. It counts them by the daily mark from "Watching as the audience" above, and only in its memory. It takes at most 20 reports in one minute from one network. It drops the rest.

        The game sends at most five reports in one visit. It never sends the same error twice in one visit.

        To turn this off, set Options → Data and privacy → Send error reports to Off. If your browser sends the Global Privacy Control signal, the game sends no error reports, whatever that option says.

        The app on a computer sends no error reports.
   *[desktop] The app sends no error reports.
}

# A heading inside the note.
pages-privacy-logs-heading = Server logs

pages-privacy-logs =
    My server runs on Cloudflare. Cloudflare handles the connections to the server. It keeps logs of the requests that it receives for up to 7 days.

    These logs can include the address that was asked for, for example an invite link. They can also include technical details of the connection.

    The server's own lines in these logs name rooms by their IDs and never name a player. The only person that they name is an administrator, by display name. They name an administrator who changes the server's settings, closes a room, or opens or closes a room's audience.

    The server also keeps a list of the last 1,000 changes to its settings and rooms closed by administrators. The list holds the administrator's name. It is there for the server's administrators to read.

# A heading inside the note.
pages-privacy-questions-heading = Questions

pages-privacy-questions = If something in this note is not clear, open an issue on the game's repository. Or write to me@sugoijan.dev.

# A link to the same note on the game's site (opens the browser).
# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-page = { $platform ->
    [web] This note on the game's site
   *[desktop] This note on the web
}

## The terms of use (round TU, 2026-11-25; round TU2, 2026-11-27, with
## the maintainer's decisions: awaiting their reading before it is
## published): the rules for the online service, what may be uploaded,
## who can read an upload and for how long, and how to report one. A
## fold in the credits after the privacy note (its label, then headings
## and text, wrapped in a column about 880 px wide), the same text as the
## site's terms page (site/terms.html).
##
## Written in the same plain language as the privacy note (see there):
## short sentences with one idea each, active voice, "I" for the one
## person who makes the game and runs its server (as the note, since
## 2026-11-27) and "you" for the reader, the same word for the same thing, no facts in parentheses, a list wherever there are three
## or more items; a body's paragraphs separated by an empty line, a
## list's lines starting with "• ". Japanese: やさしい日本語
## (locales/ja/GLOSSARY.md, "The privacy note"). No legal terms: these
## are plain rules, not a lawyer's text. Every lifetime here is the
## privacy note's, and must stay so.

# The fold's label, and what the multiplayer home's footer, Options' row
# and the button beside the upload notice on a room's My songs are called
# (a word or two).
pages-terms = Terms of use

# The terms' first paragraph.
pages-terms-intro = These terms are the rules for using the online service of { -game-name }. Please read them before you use a room or share a song. I last changed these terms on November 27, 2026.

# A heading inside the terms.
pages-terms-about-heading = What these terms are about

pages-terms-about =
    The online service is:
    • the online rooms
    • watching a room as the audience
    • songs shared by a link
    • my server, which runs these things

    When you play alone, you need no account. These terms do not apply to playing alone.

    The game's code has its own license, the MIT license.

# A heading inside the terms.
pages-terms-who-heading = Who runs the service

pages-terms-who =
    I make the game and run its online server by myself, as a hobby. There is no company behind the game. The online service is free.

    My server runs on Cloudflare.

# A heading inside the terms.
pages-terms-sign-in-heading = Signing in

pages-terms-sign-in =
    To use online rooms, you sign in with Twitch. Twitch's own terms also apply to your Twitch account.

    You do not need to sign in to watch a room as the audience.

# A heading inside the terms.
pages-terms-upload-heading = Songs that you upload

pages-terms-upload =
    The host and the co-hosts of a room can add songs to the room's song list. If the host lets everyone add songs, anyone in the room can add songs. When you add a song, the game uploads it from your device to my server. This lets the people in the room play it.

    Add only songs that you have the right to share with the people in your room. For example:
    • songs that you made yourself, both the music and the charts
    • songs whose license lets you share them
    • songs whose owners told you that you may share them

    Many songs hold music and charts that belong to other people. A song on your device is not always a song that you may share.

    The game does not check where a song came from. You are responsible for the songs that you upload.

# A heading inside the terms.
pages-terms-readers-heading = Who can read a song, and for how long

pages-terms-readers =
    Rooms are private. People join a room by its invite link. People can also watch a room as the audience, by the room's audience link.

    My server does not show the uploaded songs in any public list. The game gives a song's link only to the people in its room and to the room's audience. Anyone who has the link can read the song. The server's administrators can also see and delete it.

    The server deletes a song that you added:
    • when someone removes the song from the room's list
    • when the room ends

    Usually, a room ends two minutes after the connection of its last member closed. The privacy note tells you when a room ends.

    A room may stop without ending. Then nobody can read its songs one day after the room last showed that it was open. The server deletes them within the next hour.

    I give a key to a few people. With it, they can also share songs by a link, outside a room. Such a link works for the time that they choose: from one hour to one month. These terms apply to their songs too.

# A heading inside the terms.
pages-terms-behavior-heading = In rooms

pages-terms-behavior =
    Rooms are for playing together. Do not use them to harm other people, the service or the server.

    The host and the co-hosts of a room can:
    • remove a person from the room. The game calls this Kick.
    • replace the room's invite link, so that the old link stops working

    The host can also keep out of the room the people that the host banned or blocked on Twitch.

# A heading inside the terms.
pages-terms-ours-heading = What I may do

pages-terms-ours =
    I may delete any upload or close any room. I do not have to say why.

    When I close a room, the game tells its members that the room was closed.

# A heading inside the terms.
pages-terms-promise-heading = What I do not promise

pages-terms-promise =
    I give the game and the online service as they are, without any promise. I do not promise that the service works, that it is always there, or that it keeps your songs. Keep your own copy of every song that you upload.

    I can change the service, or stop it, at any time.

# A heading inside the terms.
pages-terms-report-heading = Reporting a song

# me@sugoijan.dev is the same address as the privacy note's (kept as it is).
pages-terms-report =
    If you think that a song on my server should not be there, write to me@sugoijan.dev. For example, you made its music or its chart, and you did not allow this.

    Please send:
    • the link to the shared song, or the room's invite link or audience link
    • the song's title and artist
    • what your work in the song is, for example the music or a chart
    • who you are: the person who made it, or a person who acts for them
    • how I can reach you

    Please do not post these links in a public issue. That would show the song to more people.

    I read these reports myself, in my free time. If a song should not be on my server, I delete it. I can also close its room. The server deletes a room's songs when the room ends anyway.

# A heading inside the terms.
pages-terms-changes-heading = Changes to these terms

pages-terms-changes = I can change these terms. When I do, I change the date at the top of these terms.

# A heading inside the terms.
pages-terms-questions-heading = Questions

pages-terms-questions = If something in these terms is not clear, open an issue on the game's repository. Or write to me@sugoijan.dev. The privacy note tells you what the game and my server keep about you.

# A link to the same terms on the game's site (opens the browser).
# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-terms-page = { $platform ->
    [web] These terms on the game's site
   *[desktop] These terms on the web
}

## Desktop's first start: a dialog offering the song folders of other
## dance games (StepMania, ITGmania, OutFox) found on the computer. About
## 600 px wide; its text wraps.

# The dialog's title.
pages-offer-title = Add the songs you have?

# The dialog's first paragraph, over the folders found.
pages-offer-found = Song folders of other dance games are on this computer. The game can read them where they are: nothing is copied or changed.

# After the first five folders: how many more were found.
# Variables:
#   $count (Number) - how many more
pages-offer-more = and { $count } more

# The dialog's last paragraph.
pages-offer-where = Options lists the song folders: add or remove them there at any time.

# A button (about 180 px wide): declines the offer, which is not made
# again.
pages-offer-decline = No thanks

# A button (about 180 px wide): adds the folders found.
# Variables:
#   $count (Number) - how many folders were found
pages-offer-add = { $count ->
    [one] Add it
   *[other] Add them
}

## A calibration test's start prompt: its title (a line about 40 letters)
## and what to do, under it (a line, wrapped).

pages-calibration-run-visual-title = Calibration 1/3: arrows only (muted)
pages-calibration-run-audio-title = Calibration 2/3: sound only
pages-calibration-run-combined-title = Calibration check: sound and arrows

# The arrows-only test: the sound is muted; the player hits what they see.
pages-calibration-run-visual-hint = hit each arrow as it reaches its target; the test adjusts itself and stops once it is sure
    .advanced = hit each arrow as it reaches the receptor; the test adjusts itself and stops once it converges

# The sound-only test: a click to press along with, nothing to see.
pages-calibration-run-audio-hint = press ANY arrow key on each click; the test adjusts itself and stops once it is sure
    .advanced = press ANY arrow key on each click; the test adjusts itself and stops once it converges

# The check of both together, on a chart played normally.
pages-calibration-run-combined-hint = play normally; the test adjusts itself and stops once it is sure
    .advanced = play normally; the test adjusts itself and stops once it converges
