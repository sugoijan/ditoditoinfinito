# Announcer lines

For people who make announcer packs: what Dito Dito Infinito reads, and
which lines it speaks when. It reads the packs made for StepMania and
its forks as they are, and speaks a few lines of its own. Those use
folder names in StepMania's style, so a pack that has them still works
in StepMania, which passes over folders it does not know.

The plan behind this is `docs/plans/announcers.md`. The research, with
sources, is `docs/research/announcers.md`.

## The layout

An announcer is a folder of line folders. Each line folder holds that
line's sounds directly inside it:

```
Announcers/
  My Announcer/
    gameplay ready/
      ready 1.ogg
      ready 2.ogg
    gameplay cleared/
      cleared.mp3
    evaluation aa/
      aa.wav
```

- **Sounds:** `.mp3`, `.wav`, `.ogg` and `.oga` (Ogg Vorbis or Opus),
  `.opus` and `.flac`. StepMania reads the first four with Vorbis inside,
  and passes over the rest. Other files are passed over.
- **Several sounds in one folder** are variants of the line. The game
  takes them in a shuffle, as StepMania 5.1 does: none comes again
  until each has played, over a whole run of the game.
- **Names** are matched without regard to case. The announcer's name is
  its folder's.
- **A folder with no sound in it is no line.** StepMania treats a folder
  holding only a readme as the line, which then says nothing; the game
  treats it as absent, so its fallback speaks instead.
- **Old names still work:** `gameplay combo 100` is read as
  `gameplay 100 combo`, and StepMania's other aliases as theirs.
- **Folders inside a line folder** are passed over (a copy of the pack
  zipped into itself is common), and so are names no line uses. The
  import's report lists both.
- **Loudness:** a pack is played as it is, at the announcer's volume (a
  share of the music's). Our own announcers sit at about −10.5 LUFS,
  about as loud as the bundled songs' music, with true peaks under
  −1 dBFS; the announcer studio tidies recordings to the same level.

To make one from recordings of your own, the game's announcer studio
(Options → Sound → Your own announcer) puts each recording on its line,
lets you hear it where the game says it and exports the pack, in several
languages, with tags and recordings said for several lines where you
want them ("Made in the studio", below).

To use one on the web, add it with Add songs (a folder, a `.zip` or a
`.smzip`) and choose it under Options, Sound. On desktop, put it in the
`Announcers/` folder of a StepMania folder that Options lists (zips
there are read too), or list its folder itself.

Limits: 2,000 sounds and 128 MB an announcer, 50 announcers an import.
The lines spoken keep their sounds first.

## Several languages in one pack

A pack can speak more than one language: put an `announcer.ini` beside
its folders and a folder of line folders for each language, named by
the language's tag (`en`, `ja`, `pt-BR`):

```
My Announcer/
  announcer.ini
  en/
    gameplay ready/
      ready.ogg
  ja/
    gameplay ready/
      junbi.ogg
  evaluation cheer/
    crowd.ogg
```

```ini
[Announcer]
Name=My Announcer
Name.ja=マイ・アナウンサー

[Parameters]
List=Language

[Parameter Language]
Type=Language
Languages=en,ja
Mixed=1
```

- **A folder with `announcer.ini` is one announcer**, whatever is in it:
  its language folders are its voices, never announcers of their own.
- **The player chooses the language** in Options → Sound → Language
  under the announcer: "Same as game language" by default, each language
  the pack lists in `Languages=` (in that order), and "Mixed" where the
  pack says `Mixed=1` (its lines taken from every language, each sound
  as likely as any other). Where the game's language has no voice, it
  speaks English, else the pack's first language.
- **Line folders beside the language folders** (`evaluation cheer/`
  above) have no language: they are said in every one.
- **`Name.<tag>`** names the pack in that language of the game's menus,
  `Name` in any other; without them its name is its folder's.
- **`Labels=`** names languages the game does not know, in the order of
  `Languages=` (`Labels=,,Deutsch`); a language the game knows is named
  in itself.
- A line one language lacks falls back as in any pack (the lines below);
  it is not taken from another language.

StepMania reads none of this: a pack meant for it too keeps its line
folders at its top.

## A script: one sound for several lines, tags, weights

A pack may also carry `announcer.lua`, run once when the pack is chosen
(never during a play), which says which sounds there are and what each
may be said for. The game still decides when a line is said.

```lua
local pack = Announcer.pack()
-- Every sound of the language folders, for its folder's line.
for _, file in ipairs(Announcer.files()) do
  if file:sub(1, 7) ~= "shared/" then pack:take(file) end
end
-- One sound for two lines.
pack:take("shared/awesome.ogg", {
  events = { "gameplay 100 combo", "evaluation cheer" },
  lang = "en",
})
-- A sound the player may switch off.
pack:take("en/gameplay 200 combo/damn.ogg", { tags = { "profanity" } })
pack:tag("profanity", { label = "Profanity", default = true })
pack:languages({ "en", "ja" }, { mixed = true })
return pack
```

- `pack:take(file, options)`: `file` is the sound's path in the pack.
  `events` lists the lines it may be said for (folder names); without
  them, its folder's line. `lang` is its language; without it, its first
  folder's when that is a language's tag. `tags` are words of your own.
  `weight` is its share when languages are mixed (1 by default).
- `pack:tag(name, { label, default })` shows a switch under the announcer
  in Options (`label` its name there); takes with a tag switched off are
  not said. A line left with none falls back as if the pack lacked it.
- `pack:languages(list, { mixed = true })` says the languages offered, in
  order, and whether Mixed is.
- `Announcer.files()` lists the pack's sounds (`Announcer.files("en/")`
  those under a folder), `Announcer.param(name)` reads a parameter
  declared in `announcer.ini` (as a noteskin's are: `Type=Choice`,
  `Toggle`, `Number`, `Color`, shown as rows under the announcer; the
  Language's value is the language chosen, or `mixed`), and
  `Announcer.language()` is the game's language.
- Sounds outside line folders (`shared/` above) are kept for the script.
- Limits: the sandboxed Lua 5.1 of noteskins without anything that
  reaches outside the pack, 1,000,000 fuel, 4 MiB, a 64 KB script, 8,000
  takes, 16 tags. A script that stops, or names a file the pack lacks or
  a line the game does not know, is refused whole: the pack's folders
  speak instead, and Options says "Its script stopped".

## Made in the studio

The announcer studio (Options → Sound → Your own announcer) writes the
plain layout above while it can, and an `announcer.ini` and a script only
where that cannot say what the pack does. What it writes for each thing,
for an announcer named "My Voice":

- **One language, no tags, no recording on two lines:** the line folders
  at the pack's top, as StepMania reads them; no `announcer.ini`, no
  script.

  ```
  Announcers/My Voice/gameplay ready/ready.opus
  Announcers/My Voice/evaluation cheer/cheer.opus
  README.txt
  ```

- **Two languages or more:** a folder a language, and an `announcer.ini`
  declaring them, each named in itself, Mixed where it is turned on (the
  languages with recordings only):

  ```
  Announcers/My Voice/announcer.ini
  Announcers/My Voice/en/gameplay ready/ready.opus
  Announcers/My Voice/ja/gameplay ready/ready.opus
  ```

  ```ini
  ; announcer.ini, written by Dito Dito Infinito's announcer studio.

  [Announcer]
  Name=My Voice

  [Parameters]
  List=Language

  [Parameter Language]
  Type=Language
  Languages=en,ja
  Labels=English,日本語
  Mixed=1
  ```

- **Tags, and a recording said for several lines:** an `announcer.lua`
  besides, listing the recordings that say more of themselves by their
  paths in the pack (every other one is its folder's line, in its
  language folder's language), then the tags players may switch off and
  the languages. A recording on several lines is written once, in
  `shared/`:

  ```lua
  local pack = Announcer.pack()

  local listed = {
    ["en/evaluation cheer/damn.opus"] = { tags = { "rude" } },
    ["shared/awesome.opus"] = { events = { "gameplay 100 combo", "evaluation cheer" }, lang = "en" },
  }

  for _, file in ipairs(Announcer.files()) do
    pack:take(file, listed[file])
  end

  pack:tag("rude", { label = "Rude words", default = true })
  pack:languages({ "en", "ja" }, { mixed = true })
  return pack
  ```

  You may edit it by hand afterwards: it uses only what "A script" above
  describes. The studio writes the same bytes for the same pack, and reads
  a pack like this back into a draft (Advanced → Import a pack), saying
  what it cannot keep (a script of your own is not kept as written; its
  results are).

## The lines

**Who** says where a line comes from: **SM** for StepMania 5.1,
**Etterna** for Etterna, **ours** for this game. **Falls back to** is
the line spoken instead when a pack lacks this one; "nothing" means
silence.

### In play

| Folder | When | Falls back to | Who |
|---|---|---|---|
| `gameplay ready` | As the play begins, if the lead-in has room for it and `here we go` | nothing | SM |
| `gameplay restart` | In place of `ready`, when the play was restarted at once (R or `\` held or pressed twice) | `gameplay ready` | ours |
| `gameplay here we go normal` | Ending half a second before the first note. If the lead-in has room for only one line, this one; if none, neither | nothing | SM |
| `gameplay comment hot` | Every 10 s (counted from the play's start, as StepMania does), from the first note on, at full life | nothing | SM |
| `gameplay comment good` | The same, otherwise | nothing | SM |
| `gameplay comment danger` | The same, while the gauge is in danger (never with failing off) | nothing | SM |
| `gameplay comment oni` | The same, on a gauge of lives (StepMania's battery, DDR's LIFE4 and RISKY) | the life's comment above | SM |
| `gameplay 100 combo` … `gameplay 1000 combo` | The combo reaching each hundred | nothing | SM |
| `gameplay 1100 combo`, `gameplay 1200 combo` … | Each hundred past 1000 | `gameplay combo overflow` | ours |
| `gameplay 64 combo`, `gameplay 222 combo` … (any count) | The combo reaching that count, for a pack that counts its own way (64, 128, 256…) | nothing | ours |
| `gameplay combo overflow` | Each hundred past 1000 | nothing | SM |
| `gameplay combo stopped` | A combo above 50 broken | nothing | SM |
| `gameplay life lost` | A life lost on a gauge of lives | nothing | ours |
| `gameplay last life` | One life left | nothing | ours |
| `gameplay full combo w1`, `w2`, `w3` | As the last note is judged, the full combo of that kind kept (W1: nothing worse than the first judgment, and so on) | `gameplay full combo` | ours |
| `gameplay full combo` | As the last note is judged, a full combo kept | nothing | ours |
| `gameplay cleared` | As the last note is judged (after the full combo's line, if one spoke) | nothing | SM |
| `gameplay failed` | At the fail (at the end, for a play that fails at its end) | nothing | SM |

A combo line is any count a pack has a folder for: when the combo
reaches one, the highest reached that frame is said. Where a pack has
none for a hundred, nothing is said for it, but past 1000 `combo
overflow` is, as in StepMania (which says only the hundreds to 1000 and
the overflow, and passes over the other counts' folders).

The comments, the combos and `life lost` share one gap: each waits 2 s
after the last of them (10 s for a comment), and one that comes sooner
is dropped, not delayed, as in StepMania. `last life` speaks whatever
the gap. Lines may overlap. A quit or a restart stops what is speaking;
a fail does not, so `gameplay failed` carries into the results.

### On the results

Exactly one of these, the first in this order that applies and that the
pack has; then `evaluation cheer`.

| Folder | When | Who |
|---|---|---|
| `evaluation quit` | The player quit (nothing else then) | ours |
| `evaluation first clear` | The chart's first cleared play under these rules | ours |
| `evaluation new record` | A new best for the chart under these rules, after an earlier play (a first play's best speaks this only where nothing below applies) | SM |
| `evaluation full combo w1` … `w4` | A full combo of that kind (W4 only where the rules keep the combo on the fourth judgment) | SM |
| `evaluation aaaa`, `aaa`, `aa`, `a`, `b`, `c`, `d` | The grade, as one of StepMania's seven | SM |
| `evaluation aaaaa` | Never yet: Etterna's top Wife grade, which no rules here give | Etterna |
| `evaluation e` | A failed play (falls back to `evaluation f`) | SM |
| `evaluation f` | A failed play, Etterna's name (falls back to `evaluation e`) | Etterna |
| `evaluation cheer` | 2.5 s after the line above, for AA or better | SM |

Where a pack lacks the full combo's line, the grade speaks (StepMania
says nothing then). Grades become StepMania's seven by letter for
rules whose grades are those letters (SM5's; DDR's, their `+` and `-`
dropped), and by place for the rest, rounded as StepMania 5.1 does (ITG's
★★★★ and ★★★ are `aaaa`, ★★ and ★ `aaa`, and so on). FFR's rules have no
grades, so no grade line. Autoplay, replays and playtests say neither
`first clear` nor `new record`.

### On song select

| Folder | When | Who |
|---|---|---|
| `select music intro` | Each time song select shows | SM |
| `select music comment repeat` | A chart chosen that was played in this run | SM |
| `select music comment new` | A chart chosen that has never been played under these rules | SM |
| `select music comment hard` | A chart chosen of meter 10 or more | SM |
| `select music comment general` | Any other chart chosen | SM |

### In battle

Ours, heard only in multiplayer battles (for now the CPU practice of
the multiplayer prototype, `docs/plans/multiplayer-prototype-progress.md`,
M4), never in a play of your own. Each names one moment of a battle as
the local player lives it (`docs/plans/multiplayer.md`, "Announcer events
implied by the mechanism"). During a stage the routine lines of a play
are left out (the comments, `life lost`, `last life` and the results'
grades, records and cheer): the battle's lines speak instead, and a pack
without them is quiet but for its fallbacks (our own announcers have
them all). The combo lines stay (`gameplay <N> combo`,
`gameplay combo overflow`, `gameplay combo stopped`; since 2026-11-15),
below every battle line: one is said only where nothing else is
speaking or about to (a step's line due), at most a second after the
combo reached its count, else not at all. So do the lines of your own
play's end (since 2026-11-15 too): `gameplay full combo` (and `w1` to
`w3`) as your last note is judged and `gameplay cleared` after it, or
`gameplay failed`, as in a play of your own but below every battle line:
said only where nothing else is speaking or about to, waiting at most a
second for a line being said, else not at all; a battle line due while
one speaks starts at its end (the results' outcome too). `cleared` and
`failed` are not said once a line has told you the stage's outcome (you
are out, with `battle eliminated` or `failed` in its place; you won
before the song's end): that line stands, so a chart's clear is never
heard after a win. Nor are they said where your pack has the lines the
stage's results will say of its outcome, whichever way it goes: a pack
with all three of `battle stage won`, `battle stage drawn` and `battle
stage not won` does not say `cleared` in a stage whose results tell how
it went (the full combo's line is still said), and one with all three
of `battle match won`, `battle match drawn` and `battle match not won`
says neither `cleared` nor `failed` on a match's last stage; the
outcome's line is what you hear. A pack without them says `cleared` and
`failed` as above. A quit stage still says `evaluation quit`.

| Folder | When | Falls back to |
|---|---|---|
| `battle match begin` | As the first stage of a match starts (a one-stage match's too), in place of `ready` | `battle stage begin`, then `gameplay ready` |
| `battle stage begin` | As a later stage starts, in place of `ready` | `gameplay ready` |
| `battle final stage` | As the last stage of a match of two or more starts, in place of `ready`: a counted match's last stage, or the song marked as the last of an open match (on the Ready card, More, or by the room's limit on stages; since 2026-11-19) | `battle stage begin`, then `gameplay ready` |
| `battle life critical` | Your life is about to run out while you are still in (the gauge entering danger), once a stage | `gameplay comment danger` |
| `battle attack incoming` | An attack against you is on its way (a battle with attacks): as it is committed, before its shocks can be seen, or not at all; not again for an attack within six seconds of the one before | nothing |
| `battle attack launched` | Your own attack was sent (your attack power was full) | nothing |
| `battle step up` | Under "Charts: Adaptive", you step up to a harder chart: as its notes begin (not when the step is decided); never over a line being said, at most a second late, else shown | nothing |
| `battle step down` | Under "Charts: Adaptive", your life ran out and you step down to an easier chart: as its notes begin; as `battle step up` otherwise | nothing |
| `battle survivors 4`, `battle survivors 2` | The players still in become four, or two, having been more; only the count reached (one passed over in the same moment is not said) | nothing |
| `battle eliminated` | Your life runs out: you are out of the stage | `gameplay failed` |
| `battle stage won` | You alone win the stage, as soon as it is decided (when you are the last one in, the song goes on for you), else on the results | nothing |
| `battle stage not won` | On the results of a stage you finished without winning | nothing |
| `battle stage drawn` | You share the stage's first place: as it is decided (the last ones out together), else on the results | nothing |
| `battle song finished` | Having won the stage early, you reach the song's end (on its results) | `gameplay cleared` |
| `battle match won` | On the results of a match's last stage, the match won alone | nothing |
| `battle match not won` | The same, the match not won | nothing |
| `battle match drawn` | The same, first place shared | nothing |

A stage opens with one line, never two: the match's beginning on its
first stage (a one-stage match's too), the final stage's on its last
(an open match's song marked as its last included), else the stage's.

`here we go` still ends half a second before the first note. One battle
line speaks at a time: one due while another is still speaking speaks
only if it matters more (an outcome over an attack against you, that
over your life, your life over a step of yours, a step over who is
left, who is left over an attack of your own, and the openings last),
else it is passed over, never delayed; a step's line alone waits for
the line being said to end, up to a second; a battle line due while a
combo line or a line of your play's end speaks starts at its end; a line about a moment that has
passed is never said late (an attack against you is said before its
first shock comes into view, or shown). On a match's last stage the match's
line takes the place of the stage's (where the pack has it). The game
never needs these lines: where a pack has no line for a moment in play
(or the announcer is off), the moment shows quietly beside the field
for a few seconds; the stage's and the match's outcomes are on the
results and in the room. The moments the design names that the
prototype cannot have yet (the readiness deadline, a watched stage, a
connection lost) have no lines yet.

### When a line is over (since 2026-11-17)

Wherever these rules say a line "is speaking", "is being said" or
"ends" (a line that waits for another to end, one passed over while
another speaks, `cleared` after the full combo's line, `here we go`
ending half a second before the first note), they mean the line's
voice, not the whole sound: the game finds where each sound's voice
ends and only a fading tail is left (a reverb, an echo), and counts the
line as said until then. The sound still plays to its end, and the next
line may start over its tail, the two heard together. The voice ends
at the end of the last 10 ms that is no more than 20 dB under the
sound's voiced level (the level of its parts within 30 dB of its
loudest); what follows is its tail only if it lasts a quarter of a
second or more (`voice-tail-v0`). A sound with no reverb, cut close to
its last word as most packs' are, has no tail: its line ends with its
sound, as before. A long quiet ending (breath, a held note fading away)
may count as a tail.

### Read, not spoken

These are recognized (and not reported as unknown), but nothing speaks
them: StepMania 3.9's `stage 1` … `stage final`, `stage event`,
`game over`, `caution`; screen lines such as `ScreenSelectStyle intro`
and the `select difficulty comment` family; `gameplay intro`,
`hurry up`, `title menu game name`, the idle comment, the course lines
and StepMania's `gameplay battle …` lines (which are not ours above). Calibration never speaks: nothing may cue the player.
