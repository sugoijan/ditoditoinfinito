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
pages-calibration-about = Each test plays a generated chart (100 BPM, three clicks and a rest, a click on every note) exactly like a song. While you play, the test measures how early or late your hits are, applies the correction on the fly and keeps measuring until the remaining error is within noise (95% confidence), then stops. Saving adds the correction to the setting.

# The second paragraph: the order the guided calibration runs the tests
# in. "Arrows only", "sound only" and "both" are the three tests below.
pages-calibration-order = The two offsets are independent, so the guided order is: arrows only (display), then sound only (audio path), then a check with both that should read close to zero.

# Under the first paragraphs, in a lighter color.
# Variables:
#   $ms (Number) - the largest offset a test measures, in milliseconds (250)
pages-calibration-range = Offsets beyond ±{ $ms } ms cannot be measured this way (a press that late cannot be told apart from one on the next note); set those by hand in the options, then calibrate from there.

# The first test's heading: the arrows drawn, no sound.
pages-calibration-visual = 1. Arrows only

# Under it: what the first test does.
pages-calibration-visual-about = Muted. Hit each arrow at the receptor. Measures display lag; saves to the visual offset (only moves what is drawn).

# The large button under the first test: runs the three tests in turn,
# starting with the first. Sized to its text.
pages-calibration-start-guided = Start guided calibration

# The second test's heading: the clicks heard, no arrows.
pages-calibration-audio = 2. Sound only

# Under it: what the second test does.
pages-calibration-audio-about = Arrows hidden; press any arrow key on each click. Measures the audio path; saves to the audio offset (the judged timeline).

# A button under the second and the third test: runs that test alone.
# Sized to its text.
pages-calibration-start = Start

# The third test's heading: arrows and sound together.
pages-calibration-combined = 3. Check: both

# Under it: what the third test does.
pages-calibration-combined-about = Play normally. Should read near zero after 1 and 2; a significant remainder can be saved as a fine-tune to the audio offset.

# A heading over the offsets kept.
pages-calibration-offsets = Offsets

# The offsets used for a sound output or a display the game has not seen
# before.
# Variables:
#   $audio (String) - the audio offset in milliseconds, signed ("+12", "-3", "+0")
#   $visual (String) - the visual offset in milliseconds, signed
pages-calibration-defaults = Defaults, for devices without a profile: audio { $audio } ms, visual { $visual } ms.

# In place of the devices' list when there is none.
pages-calibration-no-device = No device calibrated yet.

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
pages-calibration-reset = Reset offsets to 0

# Near the page's end, in a lighter color. “Timing error of each hit” is
# the name of a row in the options (Display): write it as that row is
# translated.
pages-calibration-tip = Tip: turn on “Timing error of each hit” in the options to see each hit's offset during normal play; the debug overlay shows the running mean.

# The last button: opens the options. Sized to its text.
pages-calibration-options = Options

# The question before the offsets are reset (a dialog's title).
pages-calibration-reset-title = Reset the offsets?

# The question's text, under its title.
pages-calibration-reset-body = The default audio and visual offsets and every device's go back to 0 ms; the devices stay known, as not calibrated.

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
    [visual] The arrows-only test could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the arrows really show that late, set the visual offset by hand in the options first.
    [audio] The sound-only test could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the clicks really sound that late, set the audio offset by hand in the options first.
   *[combined] The check with both could not measure reliably, so nothing changed. Run it again, hitting every note with a steady rhythm. If the clicks really sound that late, set the audio offset by hand in the options first.
}

# The test measured no meaningful error: the offset stays. The arrows-only
# test measures the visual offset, the others the audio offset.
# Variables:
#   $test (String) - the test: "visual", "audio" or "combined"
pages-calibration-run-right = { $test ->
    [visual] The arrows-only test found the visual offset right as it is.
    [audio] The sound-only test found the audio offset right as it is.
   *[combined] The check with both found the audio offset right as it is.
}

# The test changed the offset.
# Variables:
#   $test (String) - the test: "visual", "audio" or "combined"
#   $ms (String) - the offset now, in milliseconds, signed ("+12", "-3")
pages-calibration-run-set = { $test ->
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
## (its label, then headings and paragraphs, wrapped in a column about
## 880 px wide), the same text as the site's privacy page. Plain words,
## no legal terms. Twitch is the sign-in's provider, named as people see
## it; "User-Agent" is a browser's technical header, kept as it is.

# The fold's label, and what the multiplayer home's footer and Options'
# row are called (a word or two).
pages-privacy = Privacy

# The note's first paragraph.
pages-privacy-intro = This note says what { -game-name } and its online server keep about you, and for how long. Last changed on November 21, 2026.

# A heading inside the note.
pages-privacy-alone-heading = Playing alone

# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-alone = { $platform ->
    [web] Playing alone, the game keeps everything on your device: your settings, your songs and your play history stay in your browser's storage, or in the app's folders on your computer. It sends nothing about you or your plays to our server. A song someone shared with you by link is read from our server.
   *[desktop] Playing alone, the game keeps everything on your device: your settings, your songs and your play history stay in the app's folders on your computer. It sends nothing about you or your plays to our server. A song someone shared with you by link is read from our server.
}

# A heading inside the note.
pages-privacy-sign-in-heading = Signing in

# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-sign-in = Online rooms need you to sign in with Twitch (watching a room with its audience does not). When you sign in, our server asks Twitch for your Twitch user id, your login name and your display name, and keeps none of Twitch's keys. { $platform ->
    [web] These three go into a signed session that your browser keeps as a cookie (the app keeps it in its settings folder).
   *[desktop] These three go into a signed session that the app keeps in its settings folder.
} A session lasts a week, is renewed while you use it, and never lasts more than 30 days after you signed in. Our server keeps no list of the people who signed in. Signing out removes the session from your device. We never ask Twitch for your email address.

# A heading inside the note.
pages-privacy-rooms-heading = Rooms

pages-privacy-rooms = A room keeps what its members see: its name, each member's display name and Twitch user id (so they can come back), their roles, the room's song list (each song's title, artist, pack and charts, and who added it), the players' choices of the next song until each draw and the last draw (who chose which song, and which one was drawn) where the players choose the songs, the songs picked and the results of its matches. A room is deleted when it ends: two minutes after its last member's connection closed, after a week without activity, when its host leaves and it is not passed on, or when the server's administrators close it. After that, only a note that it ended (its id, when and why) is kept for a week, so that its links can say the room is closed; its session log, described below, is kept apart and names nobody. While a room is open, the server's administrators can see it listed with its name, its members' display names and how many people are in it.

# A heading inside the note.
pages-privacy-songs-heading = Songs

pages-privacy-songs = When someone in a room adds a song to the room's song list (its host or a co-host, or anyone in the room where the host lets everyone add songs), the game uploads it from their device to our server so that everyone in the room can play it, and the room keeps who added it. Anyone with that copy's link could read it while the room is open. It is kept as long as the room lives: it is deleted when it is removed from the list (by whoever added it, by a host, or to make room in a full list) and when the room ends. Should a room ever stop without ending, its songs are deleted a day after it last showed it was still open, at the latest. A few people we give a key can also share songs by link: such a song is kept until the expiry chosen when it was shared (from an hour to no expiry) or until it is deleted. While you are in a room, the game also downloads the room's songs onto your device before they are picked, so that a stage can start at once: every song of the list by default, or only the likely next ones or none, as Options → Data → Room songs ahead says, never their movies. They are kept apart from your own songs and your play history (in your browser's storage, or the app's cache folder), and deleted when you leave the room, or the next time the game starts if it stopped while you were in one. Your device tells our server which of the room's songs it has, so that everyone sees how many have each one; the server keeps this only while your connection to the room is open and stores none of it.

# Round TW1b: the Privacy fold's part on a person's Twitch access for a
# room's features (the same as site/privacy.html's).
pages-privacy-twitch-heading = Twitch access for a room's features
pages-privacy-twitch = A room's host may keep out the people they banned or blocked on Twitch, and show their Twitch chat during play. These are off unless you turn them on, and turning one on takes you through Twitch to allow only what it needs: reading who is banned or timed out in your channel, who you blocked, or chat. If you allow it, our server keeps Twitch's keys for that access (sealed so that only the server can read them) and never gives them to the game. It is used only when a room's feature needs it, and it is deleted when you sign out, when you disconnect it in Options → Data, after 30 days unused, or when Twitch ends it. While a host's room keeps people out this way, the server asks Twitch about each person at their joins, when their connection is reopened and when a song is picked; the answers are kept in memory for at most five minutes (an answer about a timeout no longer than the timeout lasts) and never stored. Someone kept out is told only "You can't join this room."; the other members see them leave. The server's logs count these checks and never name anyone.
# Round TW7: the Privacy fold's paragraph after the Twitch access part, on
# a device showing a host's Twitch chat (the same as site/privacy.html's).
# "Emotes from" and "Twitch" are Options → Display's row and its value.
pages-privacy-twitch-chat = While your device shows a host's Twitch chat during play, the game connects from your device directly to Twitch, which sends the chat's messages to it: Twitch sees your device's address and that your Twitch account reads that channel's chat (our server only asks Twitch to start this and never sees a message). For emotes, your device asks 7TV, BetterTTV and FrankerFaceZ for the channel's and their global emote lists, and them and Twitch for the emote pictures, so they see your device's address; where a direct request fails, our server asks in its place and they see the server's address instead. Options → Display → Emotes from: Twitch keeps your device from asking the other three.
# The part on a host's Kick.
pages-privacy-kick-heading = Removing someone from a room
pages-privacy-kick = A room's host and co-hosts can remove a member (Kick). Everyone in the room is told who was removed and by whom; nothing else is kept, and the person can come back by the room's invite link.

# A heading inside the note.
pages-privacy-audience-heading = Watching with an audience

pages-privacy-audience = You can watch a room with its audience without signing in. So that one network cannot take every seat, our server turns your network's address into a mark: a code made with a secret key, different every day, that cannot be turned back into the address. The address itself is not kept. The mark exists only in the server's memory while your seat is open, and the server's administrators see only its first eight characters. Signed in, your seats count against your account instead, and no mark is made.

# A heading inside the note.
pages-privacy-stats-heading = Usage statistics

pages-privacy-stats = To see whether online play works well, our server counts what happens in rooms as daily totals, for example:

# A line of the list of what is counted (after a bullet).
# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-stats-devices = { $platform ->
    [web] what kind of device and browser opened a connection (phone, tablet or computer; Chrome, Edge, Firefox, Safari, Samsung Internet or another), read from your browser's User-Agent when the connection opens and then dropped;
   *[desktop] what kind of device opened a connection (a phone, a tablet, a computer or this app), read from the connection's User-Agent when it opens and then dropped;
}

# A line of the list of what is counted (after a bullet).
pages-privacy-stats-connections = how connections closed and how long they lasted, how long people were away and whether they came back, and the round trip from your network to our server, in ranges;

# A line of the list of what is counted (after a bullet).
pages-privacy-stats-loading = how long songs took to upload and to load, and why players sat a stage out;

# A line of the list of what is counted (after a bullet). Adaptive is
# the match rule's name (Charts: Adaptive), shocks a battle's arrows.
pages-privacy-stats-play = what was played: the charts' difficulty range and style, the rules, when runs ran out of life, the attacks sent and shocks stepped on, and the steps on the Adaptive ladder;

# A line of the list of what is counted (after a bullet).
pages-privacy-stats-people = how many different people were in rooms each day.

pages-privacy-stats-kept = These totals contain no names, accounts, addresses, rooms or song titles. To count the people of a day without knowing who they are, the server keeps a code made from each account with a secret key that changes every day, and deletes it once the day is counted. The totals are kept day by day for 13 months, then month by month. Bots and the game's computer players are counted apart from people. Playing alone is never counted.

# What the game itself sends for the totals (the stats proposal's step 2),
# and the Options row that turns it off (options-usage-stats, "Send usage
# statistics", in the Data section).
# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-stats-game = { $platform ->
    [web] While you are in an online room, the game also tells our server a few things for these totals, each time its connection to the room opens: that it is the game in a browser; your system (such as Windows, macOS, Android or iOS); whether your device is a phone, a tablet or a computer; whether the game draws with WebGPU or WebGL2; whether the screen is a touch screen; the game's version; how many times it has connected to the room and how many tries this connection took; and how its previous connection to the room ended (closed or dropped, and with which code), how long that connection lasted, how long the page was in the background while it lasted, whether its keep-alive messages were answered, and the round trip to our server. Each of these is one value from a fixed list or a range, never a name, an account, an address, a room or a song. The server adds them to the totals and keeps nothing else of them. To turn this off, set Options → Data → Send usage statistics to Off: the game then sends none of it, and the server's own counts above go on. Watching with an audience sends none of it.
   *[desktop] While you are in an online room, the app also tells our server a few things for these totals, each time its connection to the room opens: that it is the app; your system (Windows, macOS or Linux); whether it draws with Metal, Vulkan, DirectX 12 or OpenGL; the game's version; how many times it has connected to the room and how many tries this connection took; and how its previous connection to the room ended (closed or dropped, and with which code), how long that connection lasted, how long the window was hidden while it lasted, and the round trip to our server. Each of these is one value from a fixed list or a range, never a name, an account, an address, a room or a song. The server adds them to the totals and keeps nothing else of them. To turn this off, set Options → Data → Send usage statistics to Off: the app then sends none of it, and the server's own counts above go on. Watching with an audience sends none of it.
}

# A heading inside the note (the stats proposal's step 3: what a room's
# session log keeps, by seat numbers alone, for 14 days).
pages-privacy-sessions-heading = A room's session log

# Send usage statistics is the Options row (options-usage-stats), named
# as it reads there.
pages-privacy-sessions = So that problems in a room can be understood afterwards (for example, why people lost their connection), each room keeps a short log of its session: when each connection opened and closed and with which code, when someone was away and whether they came back, each seat's role and whether it played or watched, the kind of device and browser of each seat as in the totals above, how long songs took to upload and to load, when a song was added to the room's list (by which seat) or left it (and why, never which song), when a seat's own choice started the countdown to a draw of the next song, each draw (how many choices, never which song) and why a countdown ended without one, who was ready, who sat a stage out and why, how runs ended, and when someone was turned away (a full room, for example). It names nobody: each person is only a seat number that means something inside that room alone, and the log holds no names, accounts, addresses, room name or id, or song titles. Bots and the game's computer players are marked as such. If you turned Send usage statistics off, it has only what our server saw by itself. The room keeps the log while it is open; when the room ends, our server keeps it for 14 days for its administrators to read, then deletes it.

# A heading inside the note (the stats proposal's step 4: the web game's
# own error reports).
pages-privacy-errors-heading = Error reports

# Send error reports is the Options row (options-error-reports), named as
# it reads there; Global Privacy Control is a browser setting, named as
# browsers name it.
# Variables:
#   $platform (String) - web or desktop (the app sends none)
pages-privacy-errors = { $platform ->
    [web] When the game itself goes wrong in your browser (a crash, a failure in its code that nothing caught, a graphics device lost or missing, or a page that cannot start the game), it sends our server a short report holding at most the kind of error, the kind of screen you were on (such as song select or a play, never the page's address), the game's version, your system, the kind of device, the graphics it uses and the error's message. The game takes out of the message file paths (but where in the game's program the error happened), web and email addresses, network addresses, quoted text, long numbers, ids and any letters beyond English, and cuts it to 300 characters. A message can still hold an ordinary English word, such as a word of a song's title that the game's own message did not quote. Our server reads the kind of browser and device from your browser's User-Agent, as for the totals, and drops it; it takes the same things out of the message again and drops a report that still holds any of them. It counts reports in the daily totals by kind, version, screen, browser, device and graphics, and keeps one example message of each distinct error for 30 days after it was last seen. To stop floods, it counts the reports from one network for a minute by the daily mark described above, only in its memory. The game sends at most five reports a visit and never the same error twice in one visit. To turn this off, set Options → Data → Send error reports to Off. If your browser sends the Global Privacy Control signal, the game sends no error reports whatever that option says. The app on a computer sends no error reports.
   *[desktop] The app sends no error reports.
}

# A heading inside the note.
pages-privacy-logs-heading = Server logs

pages-privacy-logs = Our server runs on Cloudflare, which handles the connections to it and keeps logs of the requests it receives for up to 7 days. These logs can include the address asked for (an invite link, for example) and technical details of the connection. Our own lines in these logs name rooms by their ids and never a player (only the administrator who changes the server's settings).

# A heading inside the note.
pages-privacy-questions-heading = Questions

pages-privacy-questions = If something in this note is unclear, open an issue on the game's repository, or write to me@sugoijan.dev.

# A link to the same note on the game's site (opens the browser).
# Variables:
#   $platform (String) - web or desktop (desktop's says nothing of a browser or a site)
pages-privacy-page = { $platform ->
    [web] This note on the game's site
   *[desktop] This note on the web
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
pages-calibration-run-visual-hint = hit each arrow as it reaches the receptor; the test adjusts itself and stops once it converges

# The sound-only test: a click to press along with, nothing to see.
pages-calibration-run-audio-hint = press ANY arrow key on each click; the test adjusts itself and stops once it converges

# The check of both together, on a chart played normally.
pages-calibration-run-combined-hint = play normally; the test adjusts itself and stops once it converges
