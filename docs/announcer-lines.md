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

To make one from recordings of your own, the game's announcer studio
(on the web, Options → Sound → Your own announcer) puts each recording
on its line, lets you hear it where the game says it and exports the
pack.

To use one on the web, add it with Add songs (a folder, a `.zip` or a
`.smzip`) and choose it under Options, Sound. On desktop, put it in the
`Announcers/` folder of a StepMania folder that Options lists (zips
there are read too), or list its folder itself.

Limits: 2,000 sounds and 128 MB an announcer, 50 announcers an import.
The lines spoken keep their sounds first.

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
| `gameplay combo overflow` | Each hundred past 1000 | nothing | SM |
| `gameplay combo stopped` | A combo above 50 broken | nothing | SM |
| `gameplay life lost` | A life lost on a gauge of lives | nothing | ours |
| `gameplay last life` | One life left | nothing | ours |
| `gameplay full combo w1`, `w2`, `w3` | As the last note is judged, the full combo of that kind kept (W1: nothing worse than the first judgment, and so on) | `gameplay full combo` | ours |
| `gameplay full combo` | As the last note is judged, a full combo kept | nothing | ours |
| `gameplay cleared` | As the last note is judged (after the full combo's line, if one spoke) | nothing | SM |
| `gameplay failed` | At the fail (at the end, for a play that fails at its end) | nothing | SM |

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

### Read, not spoken

These are recognized (and not reported as unknown), but nothing speaks
them: StepMania 3.9's `stage 1` … `stage final`, `stage event`,
`game over`, `caution`; screen lines such as `ScreenSelectStyle intro`
and the `select difficulty comment` family; `gameplay intro`,
`hurry up`, `title menu game name`, the idle comment, the course and
battle lines. Calibration never speaks: nothing may cue the player.
