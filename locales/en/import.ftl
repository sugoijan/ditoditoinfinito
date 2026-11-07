## What an import of songs reports, in the import dialog (Add songs):
## the line saying what runs, the summary line after, and the report's
## sections under it, each line "what: why" (`import-line`). File and
## folder names, layouts and formats in them are as the files have them,
## never translated. Lines are wrapped in a list about 700 px wide; most
## are read by pack authors fixing their files.

## While an import runs: the status line, one line, lower case as a
## running step reads. A "…" says it goes on.

# Listing what was picked.
import-step-reading-files = reading files…

# Reading the list of files in a zip.
# Variables:
#   $path (String) - the zip's path in what was picked
import-step-reading-zip = reading { $path }…

# Storing images a pack shares between its songs (StepMania's
# RandomMovies and SongMovies folders).
# Variables:
#   $count (Number) - images
import-step-shared-images = { $count ->
    [one] storing { $count } shared background image…
   *[other] storing { $count } shared background images…
}

# The same for videos.
# Variables:
#   $count (Number) - videos
import-step-shared-videos = { $count ->
    [one] storing { $count } shared background video…
   *[other] storing { $count } shared background videos…
}

# One song after another.
# Variables:
#   $index (Number) - this song's place, from 1
#   $total (Number) - songs in all
#   $name (String) - the song's folder name
import-step-song = importing { $index }/{ $total }: { $name }

# A Dancing☆Onigiri work (a web page or a captured dump).
# Variables:
#   $label (String) - the work's page or manifest, its path
import-step-work = importing { $label }…

# An FFR level, one after another.
# Variables:
#   $index (Number) - this level's place, from 1
#   $total (Number) - levels in all
#   $title (String) - the level's title
import-step-level = reading level { $index }/{ $total }: { $title }

# Storing a noteskin found among the files.
# Variables:
#   $name (String) - the skin's name
import-step-skin = storing the noteskin { $name }…

# Running a noteskin's Lua scripts to see what of it draws.
# Variables:
#   $name (String) - the skin's name
import-step-skin-lua = running the noteskin { $name }'s Lua…

# Storing an announcer (a voice pack) found among the files.
# Variables:
#   $name (String) - the announcer's name
import-step-announcer = storing the announcer { $name }…

# After an import, the songs' pictures are taken from the first frame of
# their background videos, one after another.
# Variables:
#   $done (Number) - songs done
#   $total (Number) - songs in all
import-step-stills = taking pictures from the songs' videos… { $done } of { $total }

## The summary line after an import: what it did, as a line of tags
## joined by " · " ("3 songs imported · 1 warning · 2 skipped"). Each tag
## stands alone, starting with its number.

# Variables:
#   $count (Number) - songs imported
import-summary-songs = { $count ->
    [one] { $count } song imported
   *[other] { $count } songs imported
}

# Noteskins imported; Options → Arrows is where a skin is chosen.
# Variables:
#   $count (Number) - noteskins imported
import-summary-noteskins = { $count ->
    [one] { $count } noteskin stored (choose it under Options, Arrows)
   *[other] { $count } noteskins stored (choose one under Options, Arrows)
}

# Announcers (voice packs) imported; Options → Sound is where one is
# chosen.
# Variables:
#   $count (Number) - announcers imported
import-summary-announcers = { $count ->
    [one] { $count } announcer stored (choose it under Options, Sound)
   *[other] { $count } announcers stored (choose one under Options, Sound)
}

# Images from a pack's shared background folders, stored.
# Variables:
#   $count (Number) - images
import-summary-shared-images = { $count ->
    [one] { $count } shared background image stored
   *[other] { $count } shared background images stored
}

# Videos from a pack's shared background folders, kept.
# Variables:
#   $count (Number) - videos
import-summary-shared-videos = { $count ->
    [one] { $count } shared background video kept
   *[other] { $count } shared background videos kept
}

# Other files in those shared folders (scripted animations and the like),
# left out.
# Variables:
#   $count (Number) - files
import-summary-shared-skipped = { $count ->
    [one] { $count } other file from shared folders left out
   *[other] { $count } other files from shared folders left out
}

# The imported songs' background videos that will play.
# Variables:
#   $count (Number) - videos
import-summary-videos = { $count ->
    [one] { $count } background video will play
   *[other] { $count } background videos will play
}

# Background videos not stored because the import option “Also store
# background videos” (select-import-option-videos) is off: name it as
# that option reads.
# Variables:
#   $count (Number) - videos
import-summary-videos-left-out = { $count ->
    [one] { $count } background video left out (“Also store background videos” is off)
   *[other] { $count } background videos left out (“Also store background videos” is off)
}

# Songs of a folder read again whose own folder is gone from it,
# removed.
# Variables:
#   $count (Number) - songs
import-summary-removed = { $count ->
    [one] { $count } song removed, no longer in the folder
   *[other] { $count } songs removed, no longer in the folder
}

# Songs imported with something missing (the section "Imported with
# problems").
# Variables:
#   $count (Number) - problems
import-summary-warnings = { $count ->
    [one] { $count } warning
   *[other] { $count } warnings
}

# Things not imported (the section "Not imported").
# Variables:
#   $count (Number) - things left out
import-summary-skipped = { $count } skipped

# Details for pack authors, shown when asked for (the section "Details
# for pack authors").
# Variables:
#   $count (Number) - details
import-summary-details = { $count ->
    [one] { $count } detail
   *[other] { $count } details
}

# The summary when nothing at all among the picked files could be
# imported. A sentence.
import-nothing-found = No .sm, .ssc or .dwi files, Dancing☆Onigiri works, FFR levels, noteskins or announcers were found in what was picked.

# The summary when nothing was imported and nothing said why. A
# sentence.
import-no-simfiles = No simfiles found in what was picked.

## The report's sections: headings over their lines, one line each.

# The noteskins imported, each with what of it draws.
import-section-noteskins = Noteskins

# The announcers imported, each with what it speaks.
import-section-announcers = Announcers

# What was left out, each with why.
import-section-skipped = Not imported

# Songs imported with something missing, each with what.
import-section-problems = Imported with problems

# Details only pack authors need (shown when the option is on).
import-section-details = Details for pack authors (no difference in play)

## A report's line: what it is about, then why. The reasons below follow
## the colon, so they begin in lower case. A reason marked "whole line"
## says what it is about itself and stands alone.

# Variables:
#   $what (String) - the song folder, file or name the line is about (as found)
#   $why (String) - the reason, one of the messages below
import-line = { $what }: { $why }

## Files and archives.

# A file of the import could not be read.
# Variables:
#   $file (String) - the file
#   $error (String) - what failed (the system's words, perhaps with their kind)
import-file-failed = { $file }: { $error }

# A file listed for the import is not there any more.
# Variables:
#   $file (String) - the file
import-file-missing = { $file } is missing

# A zip's entry ends past the end of the zip (a cut-off download).
# Variables:
#   $file (String) - the entry
import-truncated = { $file } is cut off (truncated zip)

# An encrypted entry of a zip: the game reads no encrypted entries.
import-encrypted = encrypted; not read

# The zip reader's reasons. A zip without its directory at its end.
import-zip-not-a-zip = not a zip archive (no end of central directory record)

# A damaged zip.
# Variables:
#   $detail (String) - what is wrong with it (the zip reader's words, English)
import-zip-corrupt = corrupt zip archive: { $detail }

# A zip split over several files.
import-zip-multi-disk = multi-disk zip archives are not supported

# Variables:
#   $name (String) - the entry
import-zip-encrypted = “{ $name }” is encrypted; encrypted zip entries are not supported

# An entry compressed in a way the game does not read. The methods
# named are names, kept as they are.
# Variables:
#   $name (String) - the entry
#   $method (String) - the compression method's number
import-zip-method = “{ $name }” uses compression method { $method }; only stored, deflate, Deflate64, bzip2, LZMA and Zstandard are supported

# An entry whose size is not what the zip says.
# Variables:
#   $name (String) - the entry
#   $expected (Number) - bytes the zip says
#   $actual (Number) - bytes there are
import-zip-size = “{ $name }”: expected { $expected } bytes, got { $actual }

# An entry whose checksum is wrong (damaged).
# Variables:
#   $name (String) - the entry
#   $expected (String) - the checksum the zip says, in hexadecimal
#   $actual (String) - the checksum of its bytes, in hexadecimal
import-zip-crc = “{ $name }”: CRC-32 mismatch (expected { $expected }, got { $actual })

# Compressed data that does not decompress.
# Variables:
#   $detail (String) - the decompressor's words (English)
import-zip-inflate = deflate stream is invalid: { $detail }

# Variables:
#   $name (String) - the entry
#   $method (String) - the compression's name (LZMA, bzip2…)
#   $detail (String) - the decompressor's words (English)
import-zip-decode = “{ $name }”: { $method } data is invalid: { $detail }

## StepMania simfiles (.sm, .ssc, .dwi) and their songs' files. A "tag"
## is a line of the simfile naming a file (#MUSIC, #BANNER…), kept as it
## is written.

# Not imported: the simfile has no charts at all.
import-no-charts = the simfile has no charts

# Not imported: none of its charts is in a layout the game plays.
# Variables:
#   $layouts (String) - the layouts found, as the simfile names them (dance-couple, kb7-single…)
import-no-playable-charts = no charts in a layout this game plays; found { $layouts }

# Not imported: no music file beside the simfile.
import-no-music = no audio file found next to the simfile

# Not imported: another song folder of the same pack and name came
# first (two folders with the same names).
# Variables:
#   $first (String) - the folder imported
import-same-folder = same pack and folder name as { $first }

# Not imported: a folder holding music without a simfile.
# Variables:
#   $music (String) - the music file
import-music-without-simfile = has music ({ $music }) but no .sm, .ssc or .dwi simfile

# Detail: charts in layouts the game does not play, left out of a song
# imported.
# Variables:
#   $layouts (String) - the layouts, as the simfile names them
import-layouts-left-out = charts in layouts this game does not play were left out: { $layouts }

# Detail: a file the simfile names is missing.
# Variables:
#   $tag (String) - the simfile's tag (#JACKET, #CDTITLE…)
#   $file (String) - the file it names
import-missing = { $tag } names { $file }, which is missing

# Detail: the preview sound it names is missing; a part of the music
# plays as the preview instead.
# Variables:
#   $tag (String) - the simfile's tag (#PREVIEW)
#   $file (String) - the file it names
import-missing-preview = { $tag } names { $file }, which is missing; the music's sample plays instead

# Detail: a chart's own music is missing; the song's music plays for it.
# Variables:
#   $tag (String) - the simfile's tag (import-tag-chart-music)
#   $file (String) - the file it names
import-missing-chart-music = { $tag } names { $file }, which is missing; the song's music plays instead

# Problem: the banner it names is missing and no other image stands in.
# Variables:
#   $tag (String) - the simfile's tag (#BANNER)
#   $file (String) - the file it names
import-missing-banner = { $tag } names { $file }, which is missing; the song has no banner

# Problem: the background it names is missing and no other image stands
# in.
# Variables:
#   $tag (String) - the simfile's tag (#BACKGROUND)
#   $file (String) - the file it names
import-missing-background = { $tag } names { $file }, which is missing; the song has no background

# Detail: a file it names is missing and another of the folder's stands
# in (as StepMania does).
# Variables:
#   $tag (String) - the simfile's tag (#MUSIC, #BANNER, #BACKGROUND)
#   $file (String) - the file it names
#   $used (String) - the file used instead
import-missing-replaced = { $tag } names { $file }, which is missing; { $used } is used instead

# A chart's own #MUSIC tag, as the lines above name it.
# Variables:
#   $chart (Number) - the chart's place in the simfile, from 1
import-tag-chart-music = #MUSIC (chart { $chart })

# Problem: more background-change images than a song keeps.
# Variables:
#   $count (Number) - images named
#   $kept (Number) - images kept
import-too-many-bg-images = { $count } background-change images; only the first { $kept } are kept

# Detail: the song's preview video was not stored (the import option is
# off; name it as select-import-option-videos reads).
# Variables:
#   $file (String) - the video
import-preview-video-not-stored = the preview video { $file } was not stored (“Also store background videos” is off)

# Detail: the preview video is in a format the game cannot play.
# Variables:
#   $file (String) - the video
import-preview-video-unplayable = #PREVIEWVID names { $file }, in a format the game cannot play

# Problem: background videos the game cannot play.
# Variables:
#   $videos (String) - the videos, each with its format (import-video-item), joined by commas
import-videos-unplayable = background videos in a format the game cannot play (the song background shows instead): { $videos }

# A video in that list, with its format.
# Variables:
#   $file (String) - the video
#   $format (String) - its format (import-video-format and those after it)
import-video-item = { $file } ({ $format })

# A video's format: its codec in its container (H.264 in MP4).
# Variables:
#   $codec (String) - the codec's name (H.264, VP9…)
#   $container (String) - the container's name (MP4, AVI…, or one of the messages below)
import-video-format = { $codec } in { $container }

# A video whose format is not known.
import-video-format-unknown = an unrecognized format

# Containers named in words rather than by a name.
import-video-mpeg-ps = an MPEG program stream
import-video-mpeg-ts = an MPEG transport stream
import-video-mpeg-video = a raw MPEG stream

# Detail: background videos not stored (the import option is off).
# Variables:
#   $files (String) - the videos, joined by commas
import-videos-not-stored = background videos not stored (“Also store background videos” is off): { $files }

# Problem: more files in scripted backgrounds than a song keeps.
# Variables:
#   $count (Number) - files
#   $kept (Number) - files kept
import-too-many-script-files = { $count } files in scripted backgrounds; only the first { $kept } are kept

# Detail: background changes use animations of a pack's shared folders,
# which the game does not play.
# Variables:
#   $names (String) - the animations, joined by commas
import-shared-animations = background changes use animations from shared folders, which are not played (the song background shows instead): { $names }

# Detail: background changes name scripted animations that are missing.
# Variables:
#   $names (String) - the animations, joined by commas
import-missing-animations = background changes name scripted animations that are missing (they would not be played anyway): { $names }

# Problem: background changes name images or videos that are missing.
# RandomMovies is a folder's name, kept as is.
# Variables:
#   $names (String) - the files, joined by commas
import-missing-backgrounds = background changes name images or videos that were not found (the song background shows instead, or a random movie from a RandomMovies folder): { $names }

## Dancing☆Onigiri works. "dos" is the work's chart data (danoniplus's
## own word, kept); "back and mask images" are its pictures drawn behind
## and over the notes.

# Not imported: the work splits its charts over several files.
import-divided-dos = splits its charts over several dos files, which is not supported yet

# Not imported: the dos file the page names is missing.
# Variables:
#   $file (String) - the file
import-dos-missing = its dos file { $file } is missing

# Not imported: the dos file was not read.
# Variables:
#   $file (String) - the file
import-dos-not-read = { $file } was not read

# Not imported: the dos file makes its data by running a script.
# Variables:
#   $file (String) - the file
import-dos-script = { $file } builds its dos with a script, which an import never runs

# Not imported: the music the work names is not among the files.
# Variables:
#   $url (String) - the music, as the work names it
import-work-music-missing = music { $url } was not found

# Not imported: the work names no music.
import-work-without-music = the work names no music

# Not imported: the script holding the music holds no audio.
import-music-script-empty = its music script holds no audio

# Problem: back and mask images missing (or in a format not drawn).
# Variables:
#   $names (String) - the first images, joined by commas
#   $more (Number) - how many more (0: none)
import-layer-images-missing = back and mask images not found (or not drawn, like SVG): { $names }{ $more ->
    [0] {""}
   *[other] {" "}and { $more } more
}

# Problem: more back and mask images than a song keeps.
# Variables:
#   $count (Number) - images
#   $kept (Number) - images kept
import-too-many-layer-images = { $count } back and mask images; only the first { $kept } are kept

# Problem: a captured work (a dump) holds none of its back and mask
# images.
import-capture-without-layers = a capture holds no back and mask images

# Not imported: another work's song of the same pack and file name came
# first.
# Variables:
#   $first (String) - the work imported
import-same-file = same pack and file name as { $first }

# Not imported: songs of a captured work whose music was not captured.
# Variables:
#   $count (Number) - songs
#   $titles (String) - their titles, joined by commas
import-music-not-captured = { $count ->
    [one] { $count } song of the work has no captured music: { $titles }
   *[other] { $count } songs of the work have no captured music: { $titles }
}

## FFR levels. "beatBox" is the level's chart data (FFR's own word,
## kept).

# Not imported: the level's SWF holds no MP3 music.
import-level-without-mp3 = the level has no MP3 music

# Not imported: a text level without an audio file beside it.
import-level-without-music = no audio file beside the beatBox

# Not imported: the file is not a SWF.
import-swf-not-swf = not a SWF file

# Not imported: the SWF's compressed data does not decompress.
# Variables:
#   $detail (String) - the decompressor's status (English)
import-swf-inflate = the SWF does not inflate: { $detail }

# Not imported: a SWF compressed with LZMA (the ZWS kind).
import-swf-lzma = an LZMA-compressed SWF (ZWS) is not read

# Not imported: the SWF ends too soon (damaged or cut off).
import-swf-cut-short = the SWF is cut short

# Not imported: a text in the SWF has no end (damaged).
import-swf-string = a string in the SWF is not ended

# Not imported: the SWF's music is longer than the game reads.
import-swf-music-too-long = the music is too long

## Noteskins found among the files (the web's import).

# Whole line: skins for games other than dance and pump, left out.
# Variables:
#   $count (Number) - skins
#   $games (String) - the games, as StepMania names them (kb7, techno…), joined by commas
import-skins-other-games = { $count ->
    [one] { $count } noteskin: for other games ({ $games }), not dance or pump
   *[other] { $count } noteskins: for other games ({ $games }), not dance or pump
}

# Whole line: more skins than one import keeps.
# Variables:
#   $count (Number) - skins found
#   $kept (Number) - skins imported
import-too-many-skins = { $count } noteskins: only the first { $kept } were imported

# Not imported: another skin of this import has the same name.
import-same-skin-name = another noteskin of this import has the same name

# Problem: more images and scripts than a skin keeps.
# Variables:
#   $count (Number) - files
#   $kept (Number) - files kept
import-too-many-skin-files = { $count } images and scripts; only the first { $kept } were kept

# Problem: a text file (a script, metrics) too large, left out.
# Variables:
#   $file (String) - the file
import-skin-file-too-large = { $file } is too large; left out

# Problem: the skin's texts together pass the limit.
# Variables:
#   $megabytes (Number) - the limit, in megabytes
import-skin-texts-too-large = its texts pass { $megabytes } MB; the rest were left out

## What of a noteskin draws: the line beside its name. "Elements" are the
## parts the game draws a note with (its arrow, its hold's body…); what
## the skin does not draw itself is "drawn built in" (by the game's own
## skin).

# StepMania's shared skin files (a folder named "common").
import-skin-common = StepMania's shared files (common), which the other skins fall back on

# A pump skin is tried on the Pump single layout.
# Variables:
#   $summary (String) - what of it draws (one of the messages below)
import-skin-pump = a pump skin, on Pump single: { $summary }

# Its Lua scripts ran and drew every element.
# Variables:
#   $own (Number) - elements it draws
#   $total (Number) - elements in all
import-skin-lua = its Lua ran: { $own } of { $total } elements are its own

# Its Lua scripts ran; some elements are drawn built in.
# Variables:
#   $own (Number) - elements it draws
#   $total (Number) - elements in all
#   $builtin (Number) - elements drawn built in
import-skin-lua-partly = its Lua ran: { $own } of { $total } elements are its own; { $builtin } drawn built in

# The same, with why the first few are drawn built in.
# Variables:
#   $own (Number) - elements it draws
#   $total (Number) - elements in all
#   $builtin (Number) - elements drawn built in
#   $why (String) - why (the skin loader's words, English), joined by semicolons
import-skin-lua-partly-why = its Lua ran: { $own } of { $total } elements are its own; { $builtin } drawn built in ({ $why })

# Its Lua cannot draw the layout's lanes.
import-skin-no-lanes = a layout's lanes it cannot draw

# Its Lua stopped with an error; the skin is drawn without its Lua.
# Variables:
#   $error (String) - the error (the script's words, English)
#   $files (String) - what of it draws without Lua (import-skin-files)
import-skin-lua-stopped = its Lua stopped ({ $error }); it is drawn without it: { $files }

# A skin in the older format of StepMania 3.9 and OpenITG.
# Variables:
#   $own (Number) - elements it draws
#   $total (Number) - elements in all
import-skin-legacy = made for StepMania 3.9 or OpenITG: { $own } of { $total } elements are its own

# The same, some elements drawn built in.
# Variables:
#   $own (Number) - elements it draws
#   $total (Number) - elements in all
#   $builtin (Number) - elements drawn built in
import-skin-legacy-partly = made for StepMania 3.9 or OpenITG: { $own } of { $total } elements are its own; { $builtin } drawn built in

# The same, with why the first few are drawn built in.
# Variables:
#   $own (Number) - elements it draws
#   $total (Number) - elements in all
#   $builtin (Number) - elements drawn built in
#   $why (String) - why (the skin loader's words, English), joined by semicolons
import-skin-legacy-partly-why = made for StepMania 3.9 or OpenITG: { $own } of { $total } elements are its own; { $builtin } drawn built in ({ $why })

# A skin of images and models without Lua: its own elements, then those
# it makes with Lua (not run without), those StepMania's own skins
# supply, and those that cannot be drawn (each part only when there are
# some).
# Variables:
#   $own (Number) - its own elements
#   $total (Number) - elements in all
#   $lua (Number) - elements made with Lua
#   $builtin (Number) - elements from StepMania's own skins
#   $other (Number) - elements that cannot be drawn
import-skin-files = { $own } of { $total } elements are its own images and models{ $lua ->
    [0] {""}
   *[other] ; { $lua } are Lua (drawn built in)
}{ $builtin ->
    [0] {""}
   *[other] ; { $builtin } come from StepMania's own skins (drawn built in)
}{ $other ->
    [0] {""}
   *[other] ; { $other } cannot be drawn
}

## Announcers (voice packs) found among the files. A "line" is a thing
## an announcer says at a moment (the play's start, a grade…), each with
## its folder of sounds.

# What an announcer speaks: its lines and where, its sounds, their size.
# Variables:
#   $lines (Number) - lines it has
#   $play (Number) - of them, spoken in play
#   $results (Number) - spoken on the results
#   $select (Number) - spoken on song select
#   $sounds (Number) - sound files
#   $megabytes (Number) - their size, in megabytes
import-announcer-summary = { $lines ->
    [one] { $lines } line
   *[other] { $lines } lines
} ({ $play } in play, { $results } on the results, { $select } on song select), { $sounds ->
    [one] { $sounds } sound
   *[other] { $sounds } sounds
}, { NUMBER($megabytes, minimumFractionDigits: 1) } MB

# Whole line: more announcers than one import keeps.
# Variables:
#   $count (Number) - announcers found
#   $kept (Number) - announcers imported
import-too-many-announcers = { $count } announcers: only the first { $kept } were imported

# Not imported: a folder holding none of the lines the game speaks.
import-announcer-without-lines = an announcer with none of the lines the game speaks

# Not imported: another announcer of this import has the same name.
import-same-announcer-name = another announcer of this import has the same name

# Problem: sounds past the limits, left out.
# Variables:
#   $count (Number) - sounds left out
#   $files (Number) - the most sounds kept
#   $megabytes (Number) - the most megabytes kept
import-announcer-sounds-left-out = { $count ->
    [one] { $count } sound left out past { $files } sounds or { $megabytes } MB
   *[other] { $count } sounds left out past { $files } sounds or { $megabytes } MB
}

# Detail: folders of lines an older StepMania spoke, no longer spoken.
# Variables:
#   $count (Number) - folders
#   $names (String) - the first few, joined by commas
#   $more (Number) - how many more (0: none)
import-announcer-old-folders = { $count ->
    [one] { $count } folder
   *[other] { $count } folders
} of lines no longer spoken: { $names }{ $more ->
    [0] {""}
   *[other] {" "}and { $more } more
}

# Detail: folders no line uses.
# Variables:
#   $count (Number) - folders
#   $names (String) - the first few, joined by commas
#   $more (Number) - how many more (0: none)
import-announcer-unknown-folders = { $count ->
    [one] { $count } folder
   *[other] { $count } folders
} no line uses: { $names }{ $more ->
    [0] {""}
   *[other] {" "}and { $more } more
}

# Detail: line folders without a sound in them.
# Variables:
#   $count (Number) - folders
#   $names (String) - the first few, joined by commas
#   $more (Number) - how many more (0: none)
import-announcer-empty-folders = { $count ->
    [one] { $count } folder
   *[other] { $count } folders
} without a sound: { $names }{ $more ->
    [0] {""}
   *[other] {" "}and { $more } more
}

# Detail: a copy of the announcer inside its own line folders, passed
# over.
# Variables:
#   $count (Number) - copies
#   $names (String) - the first few, joined by commas
#   $more (Number) - how many more (0: none)
import-announcer-nested-copies = { $count ->
    [one] { $count } copy
   *[other] { $count } copies
} inside its line folders passed over: { $names }{ $more ->
    [0] {""}
   *[other] {" "}and { $more } more
}

# Detail: other files passed over.
# Variables:
#   $count (Number) - files
import-announcer-other-files = { $count ->
    [one] { $count } other file passed over (not sounds, or not in a line's folder)
   *[other] { $count } other files passed over (not sounds, or not in a line's folder)
}

## Storing what was imported (the web's import, in the browser's
## storage).

# Whole line: the shared background videos could not be kept.
# Variables:
#   $error (String) - what failed (the system's words, perhaps with their kind)
import-shared-videos-failed = shared background videos: { $error }

# Problem: the pack's banner could not be stored.
# Variables:
#   $error (String) - what failed
import-pack-banner-failed = pack banner: { $error }

# Whole line: the packs' banners could not be stored.
# Variables:
#   $error (String) - what failed
import-pack-banners-failed = pack banners: { $error }

# Whole line: songs whose folder is gone from a folder read again could
# not be removed.
# Variables:
#   $error (String) - what failed
import-removing-gone-failed = removing songs gone from the folder: { $error }

# Whole line: the stored works and folders could not be tidied up.
# Variables:
#   $error (String) - what failed
import-tidying-failed = stored works and folders: { $error }

# Detail: the song's picture could not be taken from its background
# video; its background image shows instead.
# Variables:
#   $movie (String) - the video
#   $reason (String) - why (the decoder's words, English)
import-still-failed = the background video { $movie } could not be decoded for the song's picture ({ $reason }); its background image shows instead

# Detail: the song's picture could not be taken now; it is taken when
# the song is first played.
# Variables:
#   $reason (String) - why (English)
import-still-later = the song's picture could not be taken from its background video now ({ $reason }); it is taken when the song is first played
