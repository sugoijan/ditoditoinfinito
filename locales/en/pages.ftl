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
